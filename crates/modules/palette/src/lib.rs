//! Command palette module (id `"palette"`) — Phase 3.
//!
//! Global hotkey (`toggle_palette`, default `Cmd+Shift+Space`) → a Floating
//! window centered on the active monitor → egui renders the command list
//! (render closures land in Task 4 of plan 03-01). Lifecycle is build-on-
//! summon / destroy-on-close (D-06) with `window-created` pairing so no
//! orphan windows survive a fast toggle (the Phase 2 re-entrancy lesson
//! generalized).

pub mod fonts;
pub mod position;
pub mod session;

use std::sync::{Arc, OnceLock};

use mybox_core::anyhow;
use mybox_core::command::CommandRegistry;
use mybox_core::event::{EventFilter, EventPayload, FrameworkEvent};
use mybox_core::log;
use mybox_core::module::Module;
use mybox_core::toml;
use mybox_core::window::{WindowKind, WindowManagerHandle, WindowSpec};
use mybox_core::{ConfigCenter, ModuleContext, UiThreadProxy};

use position::PanelGeometry;
use session::PaletteSession;

/// Command palette module: hotkey toggle + build-destroy window lifecycle.
pub struct PaletteModule {
    session: Arc<PaletteSession>,
    /// Injected at `init` — Task 4's render closures and 03-02's execute path
    /// consume it via `ui.get()`; the injection wiring is completed here so
    /// 03-02 never touches the PaletteModule fields again.
    ui: Arc<OnceLock<UiThreadProxy>>,
}

impl PaletteModule {
    pub fn new() -> Self {
        let session = Arc::new(PaletteSession::new());
        // CJK fonts must be installed before the first frame (Pitfall 5).
        // Failure is warn-and-continue: ASCII fallback.
        if let Err(e) = fonts::install_cjk_fonts(&session.egui_ctx()) {
            log::warn!("palette: CJK font install failed ({e:#}) — ASCII fallback");
        }
        Self {
            session,
            ui: Arc::new(OnceLock::new()),
        }
    }
}

impl Module for PaletteModule {
    fn id(&self) -> &'static str {
        "palette"
    }

    fn name(&self) -> &str {
        "命令面板"
    }

    fn default_config(&self) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert(
            "hotkey".to_string(),
            toml::Value::String(default_hotkey()),
        );
        table
    }

    fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()> {
        // Inject the main-thread proxy for the Task 4 / 03-02 closures.
        let _ = self.ui.set(ctx.ui().clone());

        // Hotkey toggle: summon or close (D-06 build-destroy + re-entrancy).
        // The handler runs on the bus worker thread; window operations are
        // enqueued through the shared handle (W2).
        let session = Arc::clone(&self.session);
        let windows = ctx.windows().clone();
        let commands = ctx.commands().clone();
        ctx.on(
            EventFilter::kind("core", "hotkey.triggered"),
            Box::new(move |e| {
                if let EventPayload::Framework(FrameworkEvent::HotkeyTriggered { action, .. }) =
                    &e.payload
                {
                    if action == "toggle_palette" {
                        log::info!("palette: hotkey 'toggle_palette' triggered");
                        toggle_palette(&session, &windows, &commands);
                    }
                }
            }),
        );

        // window-created pairing: record the id, or destroy right away if a
        // close arrived first (prevents orphaned palette windows — capture's
        // `torn_down_pending` shape, generalized).
        let wc_session = Arc::clone(&self.session);
        let wc_windows = ctx.windows().clone();
        ctx.on(
            EventFilter::kind("core", "window-created"),
            Box::new(move |e| {
                if let EventPayload::Framework(FrameworkEvent::WindowCreated(id)) = &e.payload {
                    if wc_session.consume_pending_close() {
                        log::debug!("palette: late window {id} destroyed (close arrived first)");
                        wc_windows.destroy(*id);
                    } else {
                        wc_session.set_window_id(*id);
                    }
                }
            }),
        );

        // Deferred hotkey registration (capture template, CAP-01 shape): module
        // init runs inside AppBuilder::build BEFORE hotkeys.init(), so a
        // direct register_str would fail — ctx.ui().run stashes the closure
        // until the loop is live.
        let hotkeys = ctx.hotkeys().clone();
        let hotkey_str = hotkey_from_config(ctx.config());
        ctx.ui().run(Box::new(move || {
            if let Err(e) = hotkeys.register_str("toggle_palette", &hotkey_str) {
                log::warn!("failed to register toggle_palette hotkey: {e}");
            }
        }));

        Ok(())
    }
}

/// The platform default palette hotkey (config `[palette].hotkey` overrides).
fn default_hotkey() -> String {
    #[cfg(target_os = "windows")]
    {
        "Ctrl+Shift+Space".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "Cmd+Shift+Space".to_string()
    }
}

/// Read `[palette].hotkey`, defaulting when absent or not a string
/// (capture's `hotkey_from_config` shape).
fn hotkey_from_config(config: &ConfigCenter) -> String {
    config
        .get("palette", "hotkey")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(default_hotkey)
}

/// Toggle: visible → close; hidden → summon (D-06 + re-entrancy-safe).
fn toggle_palette(
    session: &PaletteSession,
    windows: &Arc<WindowManagerHandle>,
    commands: &Arc<CommandRegistry>,
) {
    if session.has_live_window() {
        close_palette(session, windows);
    } else if let Err(e) = summon_palette(session, windows, commands) {
        log::warn!("palette: summon failed: {e:#}");
    }
}

/// Summon: compute active-monitor geometry → snapshot the command list →
/// allocate the framebuffer → enqueue Create.
fn summon_palette(
    session: &PaletteSession,
    windows: &Arc<WindowManagerHandle>,
    commands: &Arc<CommandRegistry>,
) -> anyhow::Result<()> {
    let all = commands.all();
    // Placeholder height 560.0 — Task 4 replaces it with
    // `ui::window_height(PaletteState::Idle, n)` (UI-SPEC geometry table).
    let geometry = position::summon_geometry((600.0, 560.0))?;
    session.summon(all);
    session.install_framebuffer(geometry.inner_size.0, geometry.inner_size.1);
    windows.create(build_window_spec(geometry));
    Ok(())
}

/// Build the Floating window spec for a palette window. `pub` so 03-02's
/// `palette_checks` harness can reuse it; the render closures (on_event_win /
/// on_draw) are wired in by Task 4.
pub fn build_window_spec(geometry: PanelGeometry) -> WindowSpec {
    WindowSpec {
        kind: WindowKind::Floating,
        title: "mybox-palette".to_string(),
        inner_size: Some(geometry.inner_size),
        position: Some(geometry.position),
        ..Default::default()
    }
}

/// Close: enqueue Destroy for the recorded window id (a late window-created
/// is destroyed by the pairing handler in `init`).
fn close_palette(session: &PaletteSession, windows: &Arc<WindowManagerHandle>) {
    if let Some(id) = session.close() {
        windows.destroy(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mybox_core::{Event, EventBus, HotkeyManager, WindowRequest};
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
    fn sample_context() -> (Arc<EventBus>, Arc<WindowManagerHandle>, ModuleContext) {
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
        (bus, handle, ctx)
    }

    fn emit_toggle(bus: &Arc<EventBus>) {
        bus.emit(Event {
            from: "core",
            kind: "hotkey.triggered",
            payload: EventPayload::Framework(FrameworkEvent::HotkeyTriggered {
                id: 1,
                action: "toggle_palette".to_string(),
            }),
        });
    }

    #[test]
    fn id_name_and_default_config() {
        let module = PaletteModule::new();
        assert_eq!(module.id(), "palette");
        assert_eq!(module.name(), "命令面板");
        let table = module.default_config();
        assert_eq!(
            table.get("hotkey"),
            Some(&toml::Value::String(default_hotkey()))
        );
    }

    #[test]
    fn hotkey_toggle_summon_creates_floating_window() {
        let (bus, handle, ctx) = sample_context();
        PaletteModule::new()
            .init(&ctx)
            .expect("init registers handlers");

        emit_toggle(&bus);

        // The handler runs on the bus worker thread; poll for the Create
        // request (try_recv consumes — capture it in the poll).
        let received: Arc<std::sync::Mutex<Option<WindowRequest>>> =
            Arc::new(std::sync::Mutex::new(None));
        let got = received.clone();
        assert!(
            wait_until(|| {
                if let Some(req) = handle.try_recv() {
                    *got.lock().unwrap() = Some(req);
                    true
                } else {
                    false
                }
            }),
            "toggle hotkey never enqueued a WindowRequest"
        );
        let guard = received.lock().unwrap();
        match guard.as_ref() {
            Some(WindowRequest::Create(spec)) => {
                assert_eq!(spec.kind, WindowKind::Floating);
                assert_eq!(spec.title, "mybox-palette");
                assert!(spec.inner_size.is_some(), "palette must have a fixed size");
                assert!(spec.position.is_some(), "palette must be positioned at the monitor center");
                // Task 3: render closures not yet wired (Task 4 fills them).
                assert!(spec.on_event_win.is_none());
                assert!(spec.on_draw.is_none());
            }
            other => panic!(
                "expected Create, got {}",
                request_name(other.expect("request captured"))
            ),
        }
    }

    /// WindowRequest has no Debug derive — name the variant for panic messages.
    fn request_name(req: &WindowRequest) -> &'static str {
        match req {
            WindowRequest::Create(_) => "Create",
            WindowRequest::Destroy(_) => "Destroy",
            WindowRequest::Redraw(_) => "Redraw",
            WindowRequest::SetCursor(_, _) => "SetCursor",
        }
    }

    #[test]
    fn hotkey_toggle_closes_after_window_created() {
        let (bus, handle, ctx) = sample_context();
        let module = PaletteModule::new();
        module.init(&ctx).expect("init registers handlers");

        // Summon.
        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Create(_)))),
            "first toggle must summon"
        );
        // Simulate the core's window-created pairing.
        module.session.set_window_id(7);

        // Toggle again → close.
        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Destroy(7)))),
            "second toggle must enqueue Destroy(7)"
        );
        assert_eq!(module.session.state(), session::PaletteState::Hidden);
    }

    #[test]
    fn late_window_created_after_close_is_destroyed() {
        // Build-destroy pairing: close before window-created → the late window
        // is destroyed immediately (no orphan, the 02-04 re-entrancy lesson).
        let (bus, handle, ctx) = sample_context();
        let module = PaletteModule::new();
        module.init(&ctx).expect("init registers handlers");

        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Create(_)))),
            "first toggle must summon"
        );

        // Close before any window-created event arrives → pending_close set.
        emit_toggle(&bus);
        assert!(
            wait_until(|| module.session.has_live_window()),
            "close-before-create must leave the pairing pending"
        );

        // Now simulate the late window-created from the core.
        bus.emit(Event {
            from: "core",
            kind: "window-created",
            payload: EventPayload::Framework(FrameworkEvent::WindowCreated(9)),
        });
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Destroy(9)))),
            "late window must be destroyed immediately"
        );
        assert!(!module.session.has_live_window(), "pairing consumed — nothing live");
    }

    #[test]
    fn init_ok_headless() {
        let (_bus, _handle, ctx) = sample_context();
        let module = PaletteModule::new();
        // HotkeyManager is not initialized headlessly — registration is
        // deferred via the ui proxy stash and must not panic (capture shape).
        module.init(&ctx).expect("init must not panic headlessly");
        assert!(module.ui.get().is_some(), "ui proxy injected during init");
    }

    #[test]
    fn hotkey_from_config_reads_palette_section_or_defaults() {
        let config = ConfigCenter::default();
        assert_eq!(hotkey_from_config(&config), default_hotkey());
        config.set(
            "palette",
            "hotkey",
            toml::Value::String("Ctrl+Alt+Space".to_string()),
        );
        assert_eq!(hotkey_from_config(&config), "Ctrl+Alt+Space");
    }
}
