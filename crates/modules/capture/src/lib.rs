//! Screenshot capture module (id `"capture"`).
//!
//! The Phase 2 capture backend (CAP-01 + CAP-08): a hotkey (`start_screenshot`)
//! or the tray "开始截图" menu item runs a permission preflight, then captures
//! every monitor on a named worker thread via xcap; results are forwarded to
//! the main thread through `UiThreadProxy` and stored in the shared
//! [`CaptureSession`] (no overlay window yet — that lands in 02-02).

pub mod capture;
pub mod overlay;
pub mod permission;
pub mod selection;
pub mod session;
pub mod text;

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

/// Screenshot capture module: hotkey/menu → preflight → worker-thread capture
/// → `SessionState`.
pub struct CaptureModule {
    capture: CaptureFn,
    access: AccessChecker,
}

impl CaptureModule {
    /// Production defaults: real xcap capture + real macOS access check.
    pub fn new() -> Self {
        Self {
            capture: Arc::new(capture_all_monitors),
            access: permission::real_access_checker,
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
        let session = Arc::new(CaptureSession::new());

        // Hotkey entry: core/hotkey.triggered with action "start_screenshot".
        let hotkey_session = Arc::clone(&session);
        let hotkey_ui = ctx.ui().clone();
        let hotkey_windows = ctx.windows().clone();
        let hotkey_capture = self.capture.clone();
        let hotkey_access = self.access;
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
                        );
                    }
                }
            }),
        );

        // Pair framework window ids with overlays: the core emits
        // `core/window-created` after each overlay window is created on the main
        // thread; `window_created` records ids so ESC/confirm can destroy them
        // (CAP-05).
        let wc_session = Arc::clone(&session);
        ctx.on(
            EventFilter::kind("core", "window-created"),
            Box::new(move |e| {
                if let EventPayload::Framework(FrameworkEvent::WindowCreated(id)) = &e.payload {
                    wc_session.window_created(*id);
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
/// (CAP-02). `capture`/`access` are injected so headless tests can substitute
/// fakes — this is the only injection point for tests.
fn start_capture(
    session: &Arc<CaptureSession>,
    ui: &UiThreadProxy,
    windows: Arc<WindowManagerHandle>,
    capture: CaptureFn,
    access: AccessChecker,
) {
    if !check_access(access) {
        log::warn!(
            "capture aborted: macOS Screen Recording permission not granted (CAP-08 preflight)"
        );
        return;
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
                Err(e) => log::error!("capture failed: {e:#}"),
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

        start_capture(&session, &ui, windows, fake, || true);

        assert!(
            wait_until(|| count.load(Ordering::SeqCst) > 0),
            "capture fn was never invoked on the worker thread"
        );
    }

    #[test]
    fn start_capture_aborts_when_access_denied() {
        let session = Arc::new(CaptureSession::new());
        let ui = UiThreadProxy::new();
        let windows = Arc::new(WindowManagerHandle::new());

        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let fake: CaptureFn = Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(vec![sample_shot()])
        });

        start_capture(&session, &ui, windows, fake, || false);

        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "denied access must not spawn the capture thread"
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
            capture: fake,
            access: || true,
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
