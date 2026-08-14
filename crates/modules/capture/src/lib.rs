//! Screenshot capture module (id `"capture"`).
//!
//! The Phase 2 capture backend (CAP-01 + CAP-08): a hotkey (`start_screenshot`)
//! or the tray "开始截图" menu item runs a permission preflight, then captures
//! every monitor on a named worker thread via xcap; results are forwarded to
//! the main thread through `UiThreadProxy` and stored in the shared
//! [`CaptureSession`] (no overlay window yet — that lands in 02-02).

pub mod annotate;
pub mod capture;
pub mod clipboard;
pub mod overlay;
pub mod permission;
pub mod selection;
pub mod session;
pub mod text;
pub mod toolbar;

use std::sync::Arc;

use mybox_core::anyhow;
use mybox_core::event::{EventFilter, EventPayload, FrameworkEvent};
use mybox_core::log;
use mybox_core::module::Module;
use mybox_core::toml;
use mybox_core::tray_icon;
use mybox_core::window::WindowManagerHandle;
use mybox_core::{ConfigCenter, ModuleContext, UiThreadProxy};

use capture::{capture_all_monitors, CaptureFn};
use permission::{check_access, AccessChecker};
use session::CaptureSession;

/// Injectable access requester (triggers the macOS system prompt).
type AccessRequester = Arc<dyn Fn() -> bool + Send + Sync>;
/// Injectable System Settings opener (deep-link guidance). `Arc` so a counting
/// closure can be swapped in for tests.
type SettingsOpener = Arc<dyn Fn() + Send + Sync>;

/// Screenshot capture module: hotkey/menu → preflight → worker-thread capture
/// → `SessionState`.
pub struct CaptureModule {
    /// The shared session state (one per module instance). Held by the module
    /// so tests can reach it (e.g. release the re-entrancy guard) and so the
    /// production init path and any future shutdown path share one instance.
    session: Arc<CaptureSession>,
    capture: CaptureFn,
    access: AccessChecker,
    request: AccessRequester,
    open: SettingsOpener,
}

impl CaptureModule {
    /// Production defaults: real xcap capture + real macOS access check +
    /// real system prompt / Settings deep link.
    pub fn new() -> Self {
        Self {
            session: Arc::new(CaptureSession::new()),
            capture: Arc::new(capture_all_monitors),
            access: permission::real_access_checker,
            request: Arc::new(permission::request_access),
            open: Arc::new(permission::open_system_settings),
        }
    }
}

impl Module for CaptureModule {
    fn id(&self) -> &'static str {
        "capture"
    }

    fn name(&self) -> &str {
        "截图"
    }

    fn default_config(&self) -> toml::Table {
        // CAP-01: out-of-the-box hotkey — registered on startup from
        // [capture].hotkey, no manual [hotkeys] edit needed.
        let mut table = toml::Table::new();
        table.insert(
            "hotkey".to_string(),
            toml::Value::String("Cmd+Shift+S".to_string()),
        );
        table
    }

    fn menu_items(&self) -> Vec<tray_icon::menu::MenuItem> {
        vec![tray_icon::menu::MenuItem::with_id(
            "capture.start",
            "开始截图",
            true,
            None,
        )]
    }

    fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()> {
        let session = Arc::clone(&self.session);
        // The confirm flow emits `capture/screenshot-taken` from the overlay
        // `on_event` closure; give the session the shared bus to publish onto.
        session.set_bus(Arc::clone(ctx.bus()));

        // Hotkey entry: core/hotkey.triggered with action "start_screenshot".
        let hotkey_session = Arc::clone(&session);
        let hotkey_ui = ctx.ui().clone();
        let hotkey_windows = ctx.windows().clone();
        let hotkey_capture = self.capture.clone();
        let hotkey_access = self.access;
        let hotkey_request = self.request.clone();
        let hotkey_open = self.open.clone();
        ctx.on(
            EventFilter::kind("core", "hotkey.triggered"),
            Box::new(move |e| {
                if let EventPayload::Framework(FrameworkEvent::HotkeyTriggered { action, .. }) =
                    &e.payload
                {
                    if action == "start_screenshot" {
                        log::info!("capture: hotkey 'start_screenshot' triggered");
                        start_capture(
                            &hotkey_session,
                            &hotkey_ui,
                            hotkey_windows.clone(),
                            hotkey_capture.clone(),
                            hotkey_access,
                            hotkey_request.clone(),
                            hotkey_open.clone(),
                        );
                    }
                }
            }),
        );

        // Tray menu entry: core/menu.triggered with menu_id "capture.start".
        let menu_session = Arc::clone(&session);
        let menu_ui = ctx.ui().clone();
        let menu_windows = ctx.windows().clone();
        let menu_capture = self.capture.clone();
        let menu_access = self.access;
        let menu_request = self.request.clone();
        let menu_open = self.open.clone();
        ctx.on(
            EventFilter::kind("core", "menu.triggered"),
            Box::new(move |e| {
                if let EventPayload::Module(v) = &e.payload {
                    if v.get("menu_id").and_then(|m| m.as_str()) == Some("capture.start") {
                        log::info!("capture: menu 'capture.start' triggered");
                        start_capture(
                            &menu_session,
                            &menu_ui,
                            menu_windows.clone(),
                            menu_capture.clone(),
                            menu_access,
                            menu_request.clone(),
                            menu_open.clone(),
                        );
                    }
                }
            }),
        );

        // Pair framework window ids with overlays: the core emits
        // `core/window-created` after each overlay window is created on the main
        // thread; `window_created` records ids so ESC/confirm can destroy them
        // (CAP-05). If the session was torn down before a creation event
        // arrived, `window_created` reports the window must be destroyed right
        // away (prevents orphaned gray-mask overlays after a quick cancel).
        let wc_session = Arc::clone(&session);
        let wc_windows = ctx.windows().clone();
        ctx.on(
            EventFilter::kind("core", "window-created"),
            Box::new(move |e| {
                if let EventPayload::Framework(FrameworkEvent::WindowCreated(id)) = &e.payload {
                    if wc_session.window_created(*id) {
                        wc_windows.destroy(*id);
                    }
                }
            }),
        );

        // Register the screenshot hotkey (CAP-01). Must be deferred to the main
        // thread: module `init` runs inside `AppBuilder::build()` (before
        // `hotkeys.init()` in `App::run`), so a direct `register_str` call would
        // fail with "hotkey manager not initialized". `ctx.ui().run` stashes the
        // closure until `set_proxy` flushes it (after `hotkeys.init()`), landing
        // on the main thread where registration succeeds.
        let hotkeys = ctx.hotkeys().clone();
        let hotkey_str = hotkey_from_config(ctx.config());
        ctx.ui().run(Box::new(move || {
            if let Err(e) = hotkeys.register_str("start_screenshot", &hotkey_str) {
                log::warn!("failed to register start_screenshot hotkey: {e}");
            }
        }));

        Ok(())
    }
}

/// Preflight permission, then capture on a named worker thread. Results are
/// forwarded to the main thread via `UiThreadProxy` and stored in the session
/// (RESEARCH Pattern 4); on success the per-monitor overlay windows are created
/// (CAP-02). `capture`/`access`/`request`/`open` are injected so headless tests
/// can substitute fakes — this is the only injection point for tests.
///
/// Permission flow (macOS, CAP-08): denied → request the system prompt → recheck
/// → still denied → open System Settings + log guidance + abort (never silently
/// capture a black image — T-2-02).
fn start_capture(
    session: &Arc<CaptureSession>,
    ui: &UiThreadProxy,
    windows: Arc<WindowManagerHandle>,
    capture: CaptureFn,
    access: AccessChecker,
    request: AccessRequester,
    open: SettingsOpener,
) {
    // Re-entrancy guard: a rapid second trigger (hotkey repeat, or hotkey +
    // tray) must not stack a second set of overlays over the live session.
    if !session.begin_capture() {
        log::warn!("capture: already in progress — ignoring duplicate trigger");
        return;
    }
    if !check_access(access) {
        log::warn!("capture: macOS Screen Recording permission denied — requesting (CAP-08)");
        request();
        if !check_access(access) {
            open();
            log::info!(
                "capture: 请到 系统设置 → 隐私与安全性 → 屏幕录制 授权 mybox；\
                 授权后可能需要重启 mybox（A1）"
            );
            session.deactivate();
            return;
        }
    }
    let session = Arc::clone(session);
    let ui = ui.clone();
    std::thread::Builder::new()
        .name("mybox-capture".to_string())
        .spawn(move || {
            let result = capture();
            ui.run(Box::new(move || match result {
                Ok(shots) => {
                    session.store_shots(shots);
                    overlay::create_overlays(&session, &windows);
                }
                Err(e) => {
                    log::error!("capture failed: {e:#}");
                    session.deactivate();
                }
            }));
        })
        .expect("spawn capture worker thread");
}

/// Read `[capture].hotkey`, defaulting to "Cmd+Shift+S" when absent or not a
/// string (CAP-01: out-of-the-box registration).
fn hotkey_from_config(config: &ConfigCenter) -> String {
    config
        .get("capture", "hotkey")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "Cmd+Shift+S".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mybox_core::{Event, EventBus, HotkeyManager};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn wait_until(cond: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// A headless context over a fresh bus (no real config dir / OS hotkey).
    fn sample_context() -> (Arc<EventBus>, ModuleContext) {
        let bus = Arc::new(EventBus::new());
        let handle = Arc::new(WindowManagerHandle::new());
        let ctx = ModuleContext::new(
            Arc::clone(&bus),
            Arc::clone(&handle),
            Arc::new(ConfigCenter::default()),
            Arc::new(HotkeyManager::new()),
            Arc::new(mybox_core::CommandRegistry::default()),
            UiThreadProxy::new(),
        );
        (bus, ctx)
    }

    fn sample_shot() -> (capture::MonitorGeom, xcap::image::RgbaImage) {
        (
            capture::MonitorGeom {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            xcap::image::RgbaImage::new(2, 2),
        )
    }

    #[test]
    fn id_and_name() {
        let module = CaptureModule::new();
        assert_eq!(module.id(), "capture");
        assert_eq!(module.name(), "截图");
    }

    #[test]
    fn default_config_has_hotkey() {
        let table = CaptureModule::new().default_config();
        assert_eq!(
            table.get("hotkey"),
            Some(&toml::Value::String("Cmd+Shift+S".to_string()))
        );
    }

    #[test]
    fn menu_items_contains_capture_start() {
        let items = CaptureModule::new().menu_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id(), "capture.start");
        assert_eq!(items[0].text(), "开始截图");
    }

    #[test]
    fn start_capture_spawns_worker_thread_when_access_granted() {
        // Verifies the fake runs on the spawned worker thread; write-back to the
        // session is stashed (no proxy) and covered by session::store_shots.
        let session = Arc::new(CaptureSession::new());
        let ui = UiThreadProxy::new();
        let windows = Arc::new(WindowManagerHandle::new());

        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let fake: CaptureFn = Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(vec![sample_shot()])
        });

        start_capture(&session, &ui, windows, fake, || true, Arc::new(|| true), Arc::new(|| {}));

        assert!(
            wait_until(|| count.load(Ordering::SeqCst) > 0),
            "capture fn was never invoked on the worker thread"
        );
    }

    #[test]
    fn start_capture_ignores_duplicate_trigger_while_active() {
        // The re-entrancy guard: a second trigger while a capture session is
        // live must be a no-op — it must never stack a second capture/overlay
        // generation (the mask-accumulation bug).
        let session = Arc::new(CaptureSession::new());
        let ui = UiThreadProxy::new();
        let windows = Arc::new(WindowManagerHandle::new());

        let count = Arc::new(AtomicUsize::new(0));
        let c1 = count.clone();
        let c2 = count.clone();
        let fake: CaptureFn = Arc::new(move || {
            c1.fetch_add(1, Ordering::SeqCst);
            Ok(vec![sample_shot()])
        });
        let fake2: CaptureFn = Arc::new(move || {
            c2.fetch_add(1, Ordering::SeqCst);
            Ok(vec![sample_shot()])
        });

        start_capture(
            &session,
            &ui,
            Arc::clone(&windows),
            fake,
            || true,
            Arc::new(|| true),
            Arc::new(|| {}),
        );
        // Second trigger while the first session is active: rejected.
        start_capture(
            &session,
            &ui,
            Arc::clone(&windows),
            fake2,
            || true,
            Arc::new(|| true),
            Arc::new(|| {}),
        );

        assert!(
            wait_until(|| count.load(Ordering::SeqCst) == 1),
            "exactly one capture must run"
        );
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "duplicate trigger must never spawn a second capture"
        );
    }

    #[test]
    fn start_capture_denied_requests_then_opens_settings_and_aborts() {
        let session = Arc::new(CaptureSession::new());
        let ui = UiThreadProxy::new();
        let windows = Arc::new(WindowManagerHandle::new());

        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let fake: CaptureFn = Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(vec![sample_shot()])
        });

        let requested = Arc::new(AtomicUsize::new(0));
        let r = requested.clone();
        let opened = Arc::new(AtomicUsize::new(0));
        let o = opened.clone();

        start_capture(
            &session,
            &ui,
            windows,
            fake,
            || false, // access: always denied
            Arc::new(move || {
                r.fetch_add(1, Ordering::SeqCst);
                false
            }),
            Arc::new(move || {
                o.fetch_add(1, Ordering::SeqCst);
            }),
        );

        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "denied access must not spawn the capture thread"
        );
        assert_eq!(
            requested.load(Ordering::SeqCst),
            1,
            "denied path must trigger the system prompt once"
        );
        assert_eq!(
            opened.load(Ordering::SeqCst),
            1,
            "still-denied path must open System Settings once"
        );
    }

    #[test]
    fn hotkey_and_menu_both_route_to_start_capture() {
        let (bus, ctx) = sample_context();

        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let fake: CaptureFn = Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(vec![sample_shot()])
        });

        let module = CaptureModule {
            session: Arc::new(CaptureSession::new()),
            capture: fake,
            access: || true,
            request: Arc::new(|| true),
            open: Arc::new(|| {}),
        };
        module.init(&ctx).expect("init registers handlers");

        bus.emit(Event {
            from: "core",
            kind: "hotkey.triggered",
            payload: EventPayload::Framework(FrameworkEvent::HotkeyTriggered {
                id: 1,
                action: "start_screenshot".to_string(),
            }),
        });
        assert!(
            wait_until(|| count.load(Ordering::SeqCst) == 1),
            "hotkey path never fired capture"
        );

        // Release the re-entrancy guard (in a real app the user confirms or
        // cancels between screenshots; headlessly the session never finishes).
        module.session.finish();

        bus.emit(Event {
            from: "core",
            kind: "menu.triggered",
            payload: EventPayload::Module(serde_json::json!({ "menu_id": "capture.start" })),
        });
        assert!(
            wait_until(|| count.load(Ordering::SeqCst) == 2),
            "menu path never fired capture"
        );
    }

    #[test]
    fn hotkey_from_config_reads_capture_section_or_defaults() {
        let config = ConfigCenter::default();
        assert_eq!(hotkey_from_config(&config), "Cmd+Shift+S");
        config.set(
            "capture",
            "hotkey",
            toml::Value::String("Ctrl+Alt+S".to_string()),
        );
        assert_eq!(hotkey_from_config(&config), "Ctrl+Alt+S");
    }

    #[test]
    fn init_does_not_panic_when_hotkeys_uninitialized() {
        let (_bus, ctx) = sample_context();
        CaptureModule::new()
            .init(&ctx)
            .expect("init must not panic headlessly (hotkey registration is deferred via ui proxy)");
    }
}
