//! Application assembly and the winit event-loop integration (plan 01-04).
//!
//! [`AppBuilder`] registers modules (FRMW-01) and assembles the runtime
//! [`App`], which implements winit's `ApplicationHandler<AppEvent>` and runs the
//! walking skeleton: macOS Accessory activation (FRMW-06), global hotkeys
//! registered from config (FRMW-04, W1), the tray (INFRA-02), event forwarding
//! (hotkey/tray/menu → `AppEvent`), and main-thread window creation from
//! enqueued [`WindowRequest`]s (W2).
//!
//! # Threading model
//!
//! - The event bus dispatches handlers on its own worker thread (FRMW-05).
//! - `winit::Window` and `ActiveEventLoop` are main-thread-bound (not `Send`).
//! - Modules therefore enqueue [`WindowRequest`]s through
//!   [`WindowManagerHandle`] (any thread, non-blocking); the App drains the
//!   queue in `about_to_wait()` where `&ActiveEventLoop` is available and
//!   creates windows there (W2).
//! - A worker thread's `send()` does **not** wake a `ControlFlow::Wait` loop, so
//!   `WindowManagerHandle::create/destroy` fire a wake hook (W3) that the App
//!   installs in [`App::run`] to pulse `AppEvent::WindowRequested`.
//!
//! # Quit handling (N2)
//!
//! `PredefinedMenuItem::quit` is handled natively by macOS (it sends `terminate:`
//! to `NSApp`, ending the process) — the App does **not** wire the quit menu to
//! `event_loop.exit()`. The 01-04-05 manual checklist ("退出菜单项退出应用")
//! relies on this native behavior. If a future platform (Windows) does not
//! terminate natively, the quit menu id should map to bus
//! `FrameworkEvent::AppExit` and the App should call `event_loop.exit()` on it —
//! recorded as a v2 item, not implemented in Phase 1.

use std::sync::Arc;

use crate::config::ConfigCenter;
use crate::context::{ModuleContext, UiThreadProxy};
use crate::error::{MyboxError, Result};
use crate::event::{Event, EventBus, EventPayload, FrameworkEvent};
use crate::hotkey::HotkeyManager;
use crate::module::{Module, ModuleRegistry};
use crate::renderer::Renderer;
use crate::tray::TrayManager;
use crate::window::{
    window_attributes, WindowId, WindowManager, WindowManagerHandle, WindowRequest, WindowSpec,
};

/// Events mybox injects into the winit loop (RESEARCH §4).
///
/// `AppEvent` is mybox's own event namespace — it is NOT merged into winit's
/// native event sources. The `EventLoopProxy` is only the wake-up bridge from
/// the global-hotkey / tray listener threads into a `ControlFlow::Wait` loop
/// (D-08 reconciliation).
pub enum AppEvent {
    /// A global hotkey fired (forwarded from global-hotkey's listener thread).
    Hotkey(global_hotkey::GlobalHotKeyEvent),
    /// A tray icon event (clicks etc.). Logged and ignored in Phase 1.
    Tray(tray_icon::TrayIconEvent),
    /// A tray menu item was clicked.
    Menu(tray_icon::menu::MenuEvent),
    /// A closure to run on the winit main thread (`UiThreadProxy`).
    Ui(Box<dyn FnOnce() + Send>),
    /// Wake pulse: a [`WindowRequest`] was enqueued by a worker thread. The
    /// pulse wakes the `ControlFlow::Wait` loop; the request itself is drained
    /// in `about_to_wait()` (W3).
    WindowRequested,
}

/// Builds a per-window [`Renderer`]. Takes the `Arc`-shared winit window because
/// the softbuffer backend stores the window handle inside the surface.
type RendererFactory = Box<dyn Fn(Arc<winit::window::Window>) -> Result<Box<dyn Renderer>> + Send + Sync>;

/// The running application: owns the module registry, the event bus, the
/// main-thread-bound window manager, and the shared services.
pub struct App {
    registry: ModuleRegistry,
    bus: Arc<EventBus>,
    windows: WindowManager,
    /// Receiver half of the module→main-thread window-request channel; drained
    /// in `about_to_wait()` (W2).
    window_rx: crossbeam_channel::Receiver<WindowRequest>,
    /// Sender half (+ wake hook) shared with modules via `ModuleContext`.
    window_handle: Arc<WindowManagerHandle>,
    config: Arc<ConfigCenter>,
    hotkeys: Arc<HotkeyManager>,
    tray: Option<TrayManager>,
    ui_proxy: UiThreadProxy,
    renderer_factory: RendererFactory,
}

/// Assembles an [`App`]: registers modules (FRMW-01), then builds the runtime.
pub struct AppBuilder {
    registry: ModuleRegistry,
}

impl AppBuilder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self {
            registry: ModuleRegistry::new(),
        }
    }

    /// Register a module. Returns `Err(MyboxError::Module)` on a duplicate
    /// module id (FRMW-01; the error bubbles up through `build()` — N1).
    pub fn module(&mut self, module: Box<dyn Module>) -> Result<&mut Self> {
        self.registry.register(module)?;
        Ok(self)
    }

    /// Assemble the runtime [`App`].
    ///
    /// Creates the event bus and the window-request channel (sender → handle,
    /// receiver → `App::window_rx`), loads (or first-run generates) the config
    /// from every module's `default_config()` (D-12/D-13), constructs one
    /// [`ModuleContext`], and calls each module's `init` exactly once.
    ///
    /// Note: config creation targets the real platform user dir (INFRA-04).
    pub fn build(self) -> anyhow::Result<App> {
        let bus = Arc::new(EventBus::new());
        let (tx, window_rx) = crossbeam_channel::unbounded::<WindowRequest>();
        let window_handle = Arc::new(WindowManagerHandle::from_sender(tx));

        let module_refs: Vec<&dyn Module> = self.registry.iter().collect();
        let config = Arc::new(ConfigCenter::load_or_create(&module_refs)?);
        let hotkeys = Arc::new(HotkeyManager::new());
        let ui_proxy = UiThreadProxy::new();

        let context = ModuleContext::new(
            Arc::clone(&bus),
            Arc::clone(&window_handle),
            Arc::clone(&config),
            Arc::clone(&hotkeys),
            ui_proxy.clone(),
        );
        // FRMW-01: each module's init runs exactly once, after config is loaded.
        Self::init_modules(&self.registry, &context)?;

        let renderer_factory: RendererFactory = Box::new(|window: Arc<winit::window::Window>| {
            Ok(Box::new(crate::renderer::TinySkiaSoftbufferRenderer::new(window)?))
        });
        // The WindowManager's own factory is reserved for Phase-2 `batch_create`
        // (dead code until then); reuse the same renderer construction.
        let windows = WindowManager::new(Box::new(|window: Arc<winit::window::Window>| {
            Box::new(
                crate::renderer::TinySkiaSoftbufferRenderer::new(window)
                    .expect("tiny-skia softbuffer renderer"),
            )
        }));

        Ok(App {
            registry: self.registry,
            bus,
            windows,
            window_rx,
            window_handle,
            config,
            hotkeys,
            tray: Some(TrayManager::default()),
            ui_proxy,
            renderer_factory,
        })
    }

    /// Call every registered module's `init` exactly once (FRMW-01). Shared so
    /// the App assembly and the init-once unit test drive the same loop.
    fn init_modules(registry: &ModuleRegistry, ctx: &ModuleContext) -> anyhow::Result<()> {
        for module in registry.iter() {
            module.init(ctx)?;
        }
        Ok(())
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Create a builder for assembling an [`App`].
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }

    /// Run the event loop (the "skeleton" lifecycle, RESEARCH §2.1).
    ///
    /// 1. Build a winit loop with `AppEvent` as the user-event type; on macOS
    ///    run as an Accessory app (no Dock icon — FRMW-06).
    /// 2. Initialize the global hotkey manager and build the tray from every
    ///    module's `menu_items()` (main thread — macOS requirement).
    /// 3. Register the `[hotkeys]` config section (W1): each string is parsed
    ///    and registered; a failing entry is logged and skipped.
    /// 4. Install the hotkey/tray/menu event forwarders (01-04-02).
    /// 5. Attach the `EventLoopProxy` to [`UiThreadProxy`] and install the
    ///    window-request wake hook (W3).
    /// 6. `run_app(self)`.
    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut builder = winit::event_loop::EventLoop::<AppEvent>::with_user_event();
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            // FRMW-06: Accessory = no Dock icon; windows can still take focus.
            builder.with_activation_policy(ActivationPolicy::Accessory);
        }
        let event_loop = builder.build()?;
        let proxy = event_loop.create_proxy();

        // Main-thread initialization: global hotkey manager + tray (macOS
        // requires both on the main thread — RESEARCH §2.4/§2.5).
        if let Err(e) = self.hotkeys.init() {
            log::warn!("global hotkey manager init failed: {e}");
        }
        let mut module_items = Vec::new();
        for module in self.registry.iter() {
            module_items.extend(module.menu_items());
        }
        if let Some(tray) = self.tray.as_mut() {
            if let Err(e) = tray.build(module_items, 32) {
                log::warn!("tray build failed: {e}");
            }
        }

        // W1: register every [hotkeys] config entry; warn-and-continue on bad
        // ones so a single broken hotkey never blocks startup.
        self.register_config_hotkeys();

        // 01-04-02: install the three event forwarders (hotkey/tray/menu).
        self.install_event_forwarders(proxy.clone());

        // FRMW-05: UiThreadProxy forwards closures through the loop.
        self.ui_proxy.set_proxy(proxy.clone());

        // W3: waking the Wait loop when a worker thread enqueues a WindowRequest.
        self.window_handle.set_wakeup(Arc::new(move || {
            let _ = proxy.send_event(AppEvent::WindowRequested);
        }));

        event_loop.run_app(self)?;
        Ok(())
    }

    /// Register every `[hotkeys]` entry from config (W1). A non-string value, a
    /// parse failure, or an OS registration failure logs a warning and skips
    /// that entry — a single broken hotkey must not prevent startup (01-03
    /// T-1-07 alignment; RESEARCH §2.4 lifecycle step 3).
    fn register_config_hotkeys(&self) {
        let Some(section) = self.config.get_section("hotkeys") else {
            return;
        };
        for (action, value) in section {
            let Some(hotkey_str) = value.as_str() else {
                log::warn!("[hotkeys].{action} is not a string; skipping");
                continue;
            };
            // register_str takes `&self` (interior mutability): `self.hotkeys`
            // is the shared Arc, refcount >= 2 at run time, so no `Arc::get_mut`
            // and no `&mut self` are involved (W1-L1).
            match self.hotkeys.register_str(&action, hotkey_str) {
                Ok(id) => log::info!("registered hotkey '{action}' ({hotkey_str}) -> id {id}"),
                Err(e) => log::warn!("skipping hotkey '{action}' ({hotkey_str}): {e}"),
            }
        }
    }

    /// Install the three event forwarders that wake the `ControlFlow::Wait` loop
    /// from the independent hotkey/tray listener threads (D-08, RESEARCH §4).
    fn install_event_forwarders(&self, proxy: winit::event_loop::EventLoopProxy<AppEvent>) {
        let hotkey_proxy = proxy.clone();
        global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(move |e| {
            let _ = hotkey_proxy.send_event(AppEvent::Hotkey(e));
        }));
        let tray_proxy = proxy.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |e| {
            let _ = tray_proxy.send_event(AppEvent::Tray(e));
        }));
        let menu_proxy = proxy.clone();
        tray_icon::menu::MenuEvent::set_event_handler(Some(move |e| {
            let _ = menu_proxy.send_event(AppEvent::Menu(e));
        }));
    }

    /// Translate a hotkey trigger: `id` → configured action → bus event
    /// (RESEARCH §11 #3: dispatch by action name, decoupled from the OS id).
    /// Unknown ids are only logged (T-1-02: never execute arbitrary code).
    fn on_hotkey(&self, e: global_hotkey::GlobalHotKeyEvent) {
        match self.hotkeys.action_for_id(e.id) {
            Some(action) => {
                self.bus.emit(Event {
                    from: "core",
                    kind: "hotkey.triggered",
                    payload: EventPayload::Framework(FrameworkEvent::HotkeyTriggered {
                        id: e.id,
                        action,
                    }),
                });
            }
            None => log::warn!("hotkey id {} has no registered action", e.id),
        }
    }

    /// Translate a tray menu click into a bus event; the payload carries the
    /// menu id string so modules can round-trip it (RESEARCH §2.5). Only
    /// forwards the id — never executes code for an arbitrary menu id (T-1-02).
    fn on_menu(&self, e: tray_icon::menu::MenuEvent) {
        self.bus.emit(Event {
            from: "core",
            kind: "menu.triggered",
            payload: EventPayload::Module(serde_json::json!({ "menu_id": e.id.0 })),
        });
    }

    /// Create a window on the main thread — the only place winit windows can be
    /// created (`ActiveEventLoop` is not `Send` and cannot be stored in a field;
    /// it only appears as a callback parameter, W2).
    ///
    /// Builds attributes from the spec, creates the winit window, attaches a
    /// renderer, registers the state, announces `FrameworkEvent::WindowCreated`,
    /// and requests the first redraw.
    pub fn create_window(
        &mut self,
        el: &winit::event_loop::ActiveEventLoop,
        spec: WindowSpec,
    ) -> Result<WindowId> {
        let attrs = window_attributes(&spec);
        let window = el
            .create_window(attrs)
            .map_err(|e| MyboxError::Window(format!("create window '{:?}': {e}", spec.kind)))?;
        let winit_id = window.id();
        let id = self.windows.next_id();
        let window = Arc::new(window);
        let renderer = (self.renderer_factory)(Arc::clone(&window))?;
        self.windows.register(
            id,
            spec.kind,
            winit_id,
            Some(Arc::clone(&window)),
            renderer,
            spec,
        );
        self.bus.emit(Event {
            from: "core",
            kind: "window-created",
            payload: EventPayload::Framework(FrameworkEvent::WindowCreated(id)),
        });
        window.request_redraw();
        Ok(id)
    }
}

impl winit::application::ApplicationHandler<AppEvent> for App {
    /// The app is live (macOS Accessory). Phase 1 opens no startup window; the
    /// skeleton's test window is created on demand by the hotkey path.
    fn resumed(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
        log::info!("mybox ready");
    }

    /// Route a window event to its [`crate::window::WindowState`] by winit id
    /// (D-07): notify the per-window callback, then let the renderer react to
    /// redraws and resizes; close requests destroy the state.
    fn window_event(
        &mut self,
        _el: &winit::event_loop::ActiveEventLoop,
        id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if let Some(state) = self.windows.get_mut_by_winit(id) {
            if let Some(cb) = &state.spec.on_event {
                cb(&event);
            }
            match event {
                winit::event::WindowEvent::RedrawRequested => {
                    if let Err(e) = state.renderer.present() {
                        log::warn!("renderer present failed: {e}");
                    }
                }
                winit::event::WindowEvent::Resized(size) => {
                    state.renderer.resize(size.width, size.height);
                }
                winit::event::WindowEvent::CloseRequested => {
                    let wid = state.id;
                    self.windows.destroy(wid);
                }
                _ => {}
            }
        }
    }

    /// Handle mybox-injected events (RESEARCH §4): hotkeys and menu clicks are
    /// translated into bus events; `Ui` closures run on the main thread; `Tray`
    /// is logged; `WindowRequested` is a no-op wake pulse — the actual window
    /// work happens in `about_to_wait`, which is invoked right after this.
    fn user_event(&mut self, _el: &winit::event_loop::ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Hotkey(e) => self.on_hotkey(e),
            AppEvent::Menu(e) => self.on_menu(e),
            AppEvent::Ui(f) => f(),
            AppEvent::Tray(_) => log::debug!("tray icon event (ignored in Phase 1)"),
            AppEvent::WindowRequested => {}
        }
    }

    /// Drain enqueued [`WindowRequest`]s on the main thread (W2): modules run on
    /// the bus worker thread and can only enqueue; creation must happen here
    /// where `&ActiveEventLoop` is available. Then sleep until the next event
    /// (`ControlFlow::Wait` — no polling).
    fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        while let Ok(req) = self.window_rx.try_recv() {
            match req {
                WindowRequest::Create(spec) => {
                    if let Err(e) = self.create_window(el, spec) {
                        log::warn!("create window failed: {e}");
                    }
                }
                WindowRequest::Destroy(id) => {
                    self.windows.destroy(id);
                }
            }
        }
        el.set_control_flow(winit::event_loop::ControlFlow::Wait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventFilter;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// Inert renderer for headless tests (window-creation logic never draws).
    struct MockRenderer;
    impl Renderer for MockRenderer {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn draw(&mut self, _f: &mut dyn FnMut(&mut tiny_skia::PixmapMut, u32, u32)) {}
        fn present(&mut self) -> Result<()> {
            Ok(())
        }
    }

    fn mock_factory() -> RendererFactory {
        Box::new(|_w: Arc<winit::window::Window>| Ok(Box::new(MockRenderer) as Box<dyn Renderer>))
    }

    /// Build an [`App`] directly with no real config I/O (the config lives in
    /// memory only), so the event-translation logic is testable headlessly.
    fn sample_app() -> App {
        let bus = Arc::new(EventBus::new());
        let (tx, window_rx) = crossbeam_channel::unbounded::<WindowRequest>();
        let window_handle = Arc::new(WindowManagerHandle::from_sender(tx));
        App {
            registry: ModuleRegistry::new(),
            bus,
            windows: WindowManager::new(Box::new(|_w: Arc<winit::window::Window>| {
                Box::new(MockRenderer) as Box<dyn Renderer>
            })),
            window_rx,
            window_handle,
            config: Arc::new(ConfigCenter::default()),
            hotkeys: Arc::new(HotkeyManager::new()),
            tray: None,
            ui_proxy: UiThreadProxy::new(),
            renderer_factory: mock_factory(),
        }
    }

    fn wait_until(cond: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// A fake module that counts `init` calls (FRMW-01 init-once assertion).
    struct CounterModule {
        id: &'static str,
        init_calls: Arc<AtomicUsize>,
    }

    impl CounterModule {
        fn new(id: &'static str, init_calls: Arc<AtomicUsize>) -> Self {
            Self { id, init_calls }
        }
    }

    impl Module for CounterModule {
        fn id(&self) -> &'static str {
            self.id
        }
        fn name(&self) -> &str {
            self.id
        }
        fn init(&self, _ctx: &ModuleContext) -> anyhow::Result<()> {
            self.init_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn builder_rejects_duplicate_module_id() {
        let mut builder = AppBuilder::new();
        builder
            .module(Box::new(CounterModule::new("a", Arc::new(AtomicUsize::new(0)))))
            .expect("first register ok");
        let err = builder
            .module(Box::new(CounterModule::new("a", Arc::new(AtomicUsize::new(0)))))
            .map(|_| ())
            .unwrap_err();
        assert!(matches!(err, MyboxError::Module(_)), "got {err:?}");
    }

    #[test]
    fn build_inits_each_module_exactly_once() {
        // Drives the exact init loop `build()` uses (AppBuilder::init_modules)
        // against a headless-safe context — the real `build()` additionally
        // writes the user config dir, which unit tests must never touch
        // (01-03 isolation convention).
        let mut registry = ModuleRegistry::new();
        let calls = Arc::new(AtomicUsize::new(0));
        registry
            .register(Box::new(CounterModule::new("a", Arc::clone(&calls))))
            .unwrap();
        registry
            .register(Box::new(CounterModule::new("b", Arc::clone(&calls))))
            .unwrap();

        let ctx = ModuleContext::new(
            Arc::new(EventBus::new()),
            Arc::new(WindowManagerHandle::new()),
            Arc::new(ConfigCenter::default()),
            Arc::new(HotkeyManager::new()),
            UiThreadProxy::new(),
        );
        AppBuilder::init_modules(&registry, &ctx).expect("init ok");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "each module init exactly once");
    }

    #[test]
    fn on_hotkey_known_id_emits_hotkey_triggered() {
        let app = sample_app();
        // Inject a fake id→action mapping (simulated GlobalHotKeyManager state;
        // real registration is 01-04 integration scope).
        app.hotkeys.insert_mapping_for_test(7, "open_test_window");

        let seen: Arc<parking_lot::Mutex<Vec<Event>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let s = seen.clone();
        app.bus.on(EventFilter::all(), Box::new(move |e| s.lock().push(e.clone())));

        app.on_hotkey(global_hotkey::GlobalHotKeyEvent {
            id: 7,
            state: global_hotkey::HotKeyState::Pressed,
        });

        assert!(
            wait_until(|| seen.lock().len() == 1),
            "hotkey.triggered event never dispatched"
        );
        let e = &seen.lock()[0];
        assert_eq!(e.from, "core");
        assert_eq!(e.kind, "hotkey.triggered");
        match &e.payload {
            EventPayload::Framework(FrameworkEvent::HotkeyTriggered { id, action }) => {
                assert_eq!(*id, 7);
                assert_eq!(action, "open_test_window");
            }
            other => panic!("expected Framework(HotkeyTriggered), got {other:?}"),
        }
    }

    #[test]
    fn on_hotkey_unknown_id_emits_nothing() {
        let app = sample_app();
        let seen: Arc<parking_lot::Mutex<Vec<Event>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let s = seen.clone();
        app.bus.on(EventFilter::all(), Box::new(move |e| s.lock().push(e.clone())));

        app.on_hotkey(global_hotkey::GlobalHotKeyEvent {
            id: 999,
            state: global_hotkey::HotKeyState::Pressed,
        });
        // No action is mapped to 999 → no emit; a short wait confirms nothing
        // was published (dispatch is async on the bus worker thread).
        std::thread::sleep(Duration::from_millis(50));
        assert!(seen.lock().is_empty(), "unknown hotkey id must not emit");
    }

    #[test]
    fn on_menu_emits_menu_triggered_with_menu_id() {
        let app = sample_app();
        let seen: Arc<parking_lot::Mutex<Vec<Event>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let s = seen.clone();
        app.bus.on(EventFilter::all(), Box::new(move |e| s.lock().push(e.clone())));

        app.on_menu(tray_icon::menu::MenuEvent {
            id: tray_icon::menu::MenuId("test.open_window".to_string()),
        });

        assert!(
            wait_until(|| seen.lock().len() == 1),
            "menu.triggered event never dispatched"
        );
        let e = &seen.lock()[0];
        assert_eq!(e.from, "core");
        assert_eq!(e.kind, "menu.triggered");
        match &e.payload {
            EventPayload::Module(v) => assert_eq!(v["menu_id"], "test.open_window"),
            other => panic!("expected Module payload, got {other:?}"),
        }
    }

    // NOTE: `user_event` itself is not unit-tested — it takes `&ActiveEventLoop`,
    // which only exists inside winit callbacks and cannot be constructed
    // headlessly (W2). Its dispatch logic is what the on_hotkey/on_menu tests
    // above assert; the five-variant match is verified by source assertion.
}
