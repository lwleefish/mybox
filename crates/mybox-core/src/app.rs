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
use crate::event::{Event, EventBus, EventFilter, EventPayload, FrameworkEvent};
use crate::hotkey::HotkeyManager;
use crate::module::{Module, ModuleRegistry};
use crate::renderer::Renderer;
use crate::tray::TrayManager;
use crate::window::{
    window_attributes, WindowId, WindowKind, WindowManager, WindowManagerHandle, WindowRequest,
    WindowSpec,
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
    /// Exit the event loop (C5). Emitted by the bus `core/app-exit` forwarder —
    /// the quit/restart builtin runners run on worker threads and can only
    /// request the exit through the bus; `el.exit()` must run on the main
    /// thread. `FrameworkEvent::AppExit` existed since Phase 1 but nothing
    /// handled it until now.
    Exit,
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

        // PAL-02 / C5: assemble the command registry BEFORE `ModuleContext::new`
        // so modules can read commands during init. Module commands first (in
        // registration order), then the four framework builtins (UI-SPEC order
        // contract). A duplicate id bubbles up as `MyboxError::Command` (N1
        // class — T-3-02).
        let mut command_registry = crate::command::CommandRegistry::new();
        for module in &module_refs {
            for cmd in module.commands() {
                command_registry.register(cmd)?;
            }
        }
        // IN-04: config-dir failure is explicit — the builtins get `None` and
        // their runners return a descriptive error instead of silently opening
        // an empty path or a CWD-relative `logs/mybox.log`.
        let config_dir = crate::config::config_dir();
        if let Err(e) = &config_dir {
            log::warn!("config dir unavailable: {e}");
        }
        let config_dir_opt = config_dir.ok();
        let log_path_opt = config_dir_opt.as_ref().map(|d| d.join("logs").join("mybox.log"));
        for cmd in crate::command::BuiltinCommands::build(Arc::clone(&bus), config_dir_opt, log_path_opt)
        {
            command_registry.register(cmd)?;
        }
        let commands = Arc::new(command_registry);

        let context = ModuleContext::new(
            Arc::clone(&bus),
            Arc::clone(&window_handle),
            Arc::clone(&config),
            Arc::clone(&hotkeys),
            Arc::clone(&commands),
            ui_proxy.clone(),
        );
        // FRMW-01: each module's init runs exactly once, after config is loaded.
        Self::init_modules(&self.registry, &context)?;

        let renderer_factory: RendererFactory = Box::new(|window: Arc<winit::window::Window>| {
            Ok(Box::new(crate::renderer::TinySkiaSoftbufferRenderer::new(window)?))
        });
        let windows = WindowManager::new();

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
        // C5: the quit/restart builtin runners (worker threads) emit
        // `core/app-exit`; forward it into the loop as `AppEvent::Exit` so the
        // main thread can call `el.exit()`.
        let exit_proxy = proxy.clone();
        self.bus.on(
            EventFilter::kind("core", "app-exit"),
            Box::new(move |_| {
                let _ = exit_proxy.send_event(AppEvent::Exit);
            }),
        );
    }

    /// Translate a hotkey trigger: `id` → configured action → bus event
    /// (RESEARCH §11 #3: dispatch by action name, decoupled from the OS id).
    /// Unknown ids are only logged (T-1-02: never execute arbitrary code).
    fn on_hotkey(&self, e: global_hotkey::GlobalHotKeyEvent) {
        // GAP-1 root cause: global-hotkey 0.8.0's macOS backend reports BOTH
        // `kEventHotKeyPressed` and `kEventHotKeyReleased` (every one is
        // forwarded through `GlobalHotKeyEvent::set_event_handler`) — without
        // this filter one physical keypress produces two `hotkey.triggered`
        // bus events, and the palette's toggle logic (summon/close flip)
        // closes the just-summoned panel on the "release" event. The Windows
        // backend double-reports as well (platform_impl/windows), so the
        // filter is correct on Windows too. It also removes the Released
        // double-report from capture's hotkey (whose re-entrancy guard was
        // silently absorbing the second trigger).
        if e.state != global_hotkey::HotKeyState::Pressed {
            return;
        }
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
    /// renderer, registers the state, runs the per-window `on_created` callback
    /// (GAP-1 pairing fix), announces `FrameworkEvent::WindowCreated`, and
    /// requests the first redraw.
    pub fn create_window(
        &mut self,
        el: &winit::event_loop::ActiveEventLoop,
        mut spec: WindowSpec,
    ) -> Result<WindowId> {
        let attrs = window_attributes(&spec);
        let window = match el.create_window(attrs) {
            Ok(w) => w,
            Err(e) => {
                notify_create_failed(&mut spec);
                return Err(MyboxError::Window(format!("create window '{:?}': {e}", spec.kind)));
            }
        };
        match spec.kind {
            WindowKind::Overlay => {
                // macOS: raise the overlay above the menu bar + Dock so the mask
                // covers the full display (winit's AlwaysOnTop = level 3 sits
                // below both — debug session `overlay-not-fullscreen-enter`).
                #[cfg(target_os = "macos")]
                crate::window::elevate_overlay_window(&window);
                // Activate the app + make the overlay key so keyboard input
                // (Enter/ESC) works immediately after the hotkey fires. With
                // `ActivationPolicy::Accessory` a global hotkey does not activate
                // the app, and `set_visible` alone does not make a window key
                // while the app is inactive — without this the user must click
                // (start a drag) before Enter/ESC respond.
                window.focus_window();
            }
            WindowKind::Floating => {
                // C6: round the palette card corners via the NSWindow layer
                // (per-pixel alpha is dropped by softbuffer on macOS).
                #[cfg(target_os = "macos")]
                crate::window::round_floating_corners(&window);
                // C4 / Pitfall 1: same focus rationale as Overlay — under
                // `ActivationPolicy::Accessory` the global hotkey does not
                // activate the app, so without an explicit focus the borderless
                // palette never receives keyboard input.
                window.focus_window();
            }
            WindowKind::Panel => {}
        }
        let winit_id = window.id();
        let id = self.windows.next_id();
        let window = Arc::new(window);
        let renderer = match (self.renderer_factory)(Arc::clone(&window)) {
            Ok(r) => r,
            Err(e) => {
                notify_create_failed(&mut spec);
                return Err(e);
            }
        };
        // Take the per-window creation callback out before the spec moves into
        // `register`. It runs after the state is registered and before the
        // broadcast bus event below.
        let on_created = spec.on_created.take();
        self.windows.register(
            id,
            spec.kind,
            winit_id,
            Some(Arc::clone(&window)),
            renderer,
            spec,
        );
        // GAP-1 pairing fix: the callback may enqueue a Destroy (palette
        // pending_close pairing) — the same `about_to_wait` drain pass will
        // execute it. The bus `window-created` event below stays: the capture
        // module still relies on the broadcast.
        if let Some(cb) = on_created {
            cb(id);
        }
        self.bus.emit(Event {
            from: "core",
            kind: "window-created",
            payload: EventPayload::Framework(FrameworkEvent::WindowCreated(id)),
        });
        window.request_redraw();
        Ok(id)
    }
}

/// Draw content then present for a window state (RESEARCH Pattern 1).
///
/// Calls `renderer.draw` with the spec's `on_draw` closure (wrapped in
/// `catch_unwind` so a panicking module draw closure cannot kill the event
/// loop - T-2-03), then `renderer.present()`. This is the wired content
/// pipeline that was missing in Phase 1 (WR-05).
fn handle_redraw(state: &mut crate::window::WindowState) {
    if let Some(draw) = &state.spec.on_draw {
        state.renderer.draw(&mut |pixmap, w, h| {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draw(pixmap, w, h)));
        });
    }
    if let Err(e) = state.renderer.present() {
        log::warn!("renderer present failed: {e}");
    }
}

/// WR-01: invoke the spec's creation-failure callback exactly once.
/// `take()` so a later retry of the same spec cannot double-fire.
fn notify_create_failed(spec: &mut WindowSpec) {
    if let Some(cb) = spec.on_create_failed.take() {
        cb();
    }
}

/// WR-02: dispatch the per-window event callbacks in panic isolation —
/// a panicking module closure must not kill the event loop (mirrors
/// `handle_redraw`'s catch_unwind; CR-01 proved a CJK keyword-tier
/// highlight panic inside `on_event_win` kills the whole loop).
fn dispatch_window_event(state: &mut crate::window::WindowState, event: &winit::event::WindowEvent) {
    if let Some(cb) = &state.spec.on_event {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(event)));
    }
    if let Some(cb) = &state.spec.on_event_win {
        if let Some(w) = &state.window {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(w, event)));
        }
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
            // WR-02: both module callbacks run in panic isolation (the renderer
            // match below stays outside — the framework's own window handling
            // must not be masked by a module panic). C3 ordering preserved:
            // `on_event_win` runs right after `on_event` (and before the
            // renderer match) so the module's egui frame loop runs on
            // RedrawRequested while the framebuffer is fresh — `handle_redraw`
            // presents right after.
            dispatch_window_event(state, &event);
            match event {
                winit::event::WindowEvent::RedrawRequested => {
                    handle_redraw(state);
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
    fn user_event(&mut self, el: &winit::event_loop::ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Hotkey(e) => self.on_hotkey(e),
            AppEvent::Menu(e) => self.on_menu(e),
            AppEvent::Ui(f) => {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            }
            AppEvent::Tray(_) => log::debug!("tray icon event (ignored in Phase 1)"),
            AppEvent::WindowRequested => {}
            AppEvent::Exit => el.exit(),
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
                WindowRequest::Redraw(id) => {
                    if let Some(s) = self.windows.get_mut(id) {
                        if let Some(w) = &s.window {
                            w.request_redraw();
                        }
                    }
                }
                WindowRequest::SetCursor(id, icon) => {
                    if let Some(s) = self.windows.get_mut(id) {
                        if let Some(w) = &s.window {
                            w.set_cursor(icon);
                        }
                    }
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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

    /// Recording renderer: logs the order of `draw` / `present` calls so tests
    /// can assert the draw-then-present sequence (RESEARCH Pattern 1).
    struct RecordingRenderer {
        calls: Arc<parking_lot::Mutex<Vec<&'static str>>>,
    }
    impl Renderer for RecordingRenderer {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn draw(&mut self, f: &mut dyn FnMut(&mut tiny_skia::PixmapMut, u32, u32)) {
            self.calls.lock().push("draw");
            // Invoke the closure so the on_draw path is exercised.
            let mut pixmap = tiny_skia::Pixmap::new(1, 1).expect("1x1 pixmap");
            f(&mut pixmap.as_mut(), 1, 1);
        }
        fn present(&mut self) -> Result<()> {
            self.calls.lock().push("present");
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
            windows: WindowManager::new(),
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
            Arc::new(crate::command::CommandRegistry::default()),
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
    fn on_hotkey_released_event_is_ignored() {
        // GAP-1 regression: global-hotkey 0.8.0's macOS backend reports both
        // Pressed and Released for one physical keypress. A Released event for
        // a KNOWN id must not produce a `hotkey.triggered` — otherwise the
        // palette toggle flips twice per press (summon then instant close).
        let app = sample_app();
        app.hotkeys.insert_mapping_for_test(7, "open_test_window");

        let seen: Arc<parking_lot::Mutex<Vec<Event>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let s = seen.clone();
        app.bus.on(EventFilter::all(), Box::new(move |e| s.lock().push(e.clone())));

        app.on_hotkey(global_hotkey::GlobalHotKeyEvent {
            id: 7,
            state: global_hotkey::HotKeyState::Released,
        });
        // Dispatch is async on the bus worker thread; a short wait confirms
        // nothing was published (same shape as on_hotkey_unknown_id_*).
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            seen.lock().is_empty(),
            "Released hotkey events must not emit hotkey.triggered"
        );
    }

    #[test]
    fn handle_create_lands_in_app_window_rx() {
        // W2 plumbing: a module enqueues a WindowRequest through the shared
        // handle (any thread); the App drains it from its own window_rx — the
        // same channel, created once in build() (from_sender path).
        let app = sample_app();
        let spec = WindowSpec {
            title: "plumbing".to_string(),
            ..Default::default()
        };
        app.window_handle.create(spec);
        match app.window_rx.try_recv() {
            Ok(WindowRequest::Create(s)) => assert_eq!(s.title, "plumbing"),
            Ok(WindowRequest::Destroy(_)) => panic!("expected Create, got Destroy"),
            Ok(WindowRequest::Redraw(_)) => panic!("expected Create, got Redraw"),
            Ok(WindowRequest::SetCursor(_, _)) => panic!("expected Create, got SetCursor"),
            Err(_) => panic!("enqueued Create request never reached the App's window_rx"),
        }
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

    #[test]
    fn create_failed_notifies_callback_once() {
        // WR-01: the spec's on_create_failed callback must fire exactly once
        // (take semantics) — a retry of the same spec cannot double-fire.
        let flag = Arc::new(AtomicBool::new(false));
        let f = Arc::clone(&flag);
        let mut spec = WindowSpec {
            on_create_failed: Some(Box::new(move || {
                f.store(true, Ordering::SeqCst);
            })),
            ..Default::default()
        };
        notify_create_failed(&mut spec);
        assert!(flag.load(Ordering::SeqCst), "callback must fire");
        assert!(
            spec.on_create_failed.is_none(),
            "take() semantics: the callback is consumed by the first invocation"
        );
        // Second invocation must be a no-op.
        notify_create_failed(&mut spec);
    }

    #[test]
    fn panic_isolated_event_callbacks() {
        // WR-02: a panicking module event closure must not kill the event
        // loop.
        //
        // NOTE (deviation, Rule 3): the plan's primary approach (real headless
        // winit window in this unit test) is IMPOSSIBLE on macOS — winit's
        // EventLoop requires the actual process main thread
        // (MainThreadMarker), and nextest/libtest run every test on a
        // harness-spawned thread (the palette probes sidestep this by running
        // as `cargo run` binaries). Windows has no such restriction, so the
        // full real-window variant below runs on Windows CI (the CR-01 path —
        // the `on_event_win` arm — is fully covered there); the macOS variant
        // exercises the `on_event` arm headlessly and documents the guard.
        #[cfg(not(target_os = "macos"))]
        {
            // A real headless winit window drives the `on_event_win` arm —
            // with `window: None` that arm is skipped by the `if let Some(w)`
            // guard, which would leave the CR-01 CJK-panic path uncovered.
            #[allow(deprecated)] // winit 0.30 deprecates EventLoop::create_window (the
            // blessed path is run_app's ActiveEventLoop); this test only needs a real
            // OS window to pass the on_event_win guard — the full run_app harness
            // (palette_checks.rs:370-379) is too heavy for a unit test. The window is
            // never shown or drawn.
            //
            // winit requires the EventLoop on the actual process main thread on
            // EVERY platform (nextest runs tests on harness threads); the
            // Windows builder's `with_any_thread` lifts that restriction.
            #[cfg(not(target_os = "macos"))]
            use winit::platform::windows::EventLoopBuilderExtWindows;

            let event_loop = winit::event_loop::EventLoopBuilder::new()
                .with_any_thread(true)
                .build()
                .expect("event loop");
            let window = Arc::new(
                event_loop
                    .create_window(winit::window::Window::default_attributes())
                    .expect("window"),
            );
            let on_event = Arc::new(AtomicUsize::new(0));
            let on_event_win = Arc::new(AtomicUsize::new(0));
            let (a, b) = (Arc::clone(&on_event), Arc::clone(&on_event_win));
            let spec = WindowSpec {
                on_event: Some(Box::new(move |_e| {
                    a.fetch_add(1, Ordering::SeqCst);
                    panic!("on_event boom");
                })),
                on_event_win: Some(Box::new(move |_w, _e| {
                    b.fetch_add(1, Ordering::SeqCst);
                    panic!("on_event_win boom");
                })),
                ..Default::default()
            };
            let mut state = crate::window::WindowState::new(
                1,
                WindowKind::Panel,
                window.id(),
                Some(window),
                Box::new(MockRenderer),
                spec,
            );
            let evt = winit::event::WindowEvent::RedrawRequested;
            // Both arms panic once, each swallowed by catch_unwind.
            dispatch_window_event(&mut state, &evt);
            assert_eq!(on_event.load(Ordering::SeqCst), 1, "on_event arm ran and was isolated");
            assert_eq!(
                on_event_win.load(Ordering::SeqCst),
                1,
                "on_event_win arm ran and was isolated (CR-01 path)"
            );
            // Returning normally proves the panics did not propagate out of
            // dispatch_window_event — the event-loop-survives semantics.
        }
        #[cfg(target_os = "macos")]
        {
            // macOS: no real winit window is constructible on a test thread
            // (main-thread requirement). The `on_event` arm runs regardless of
            // the window guard, so panic isolation is verified headlessly; the
            // `on_event_win` arm's isolation is verified by the non-macOS
            // variant (Windows CI — the CR-01 path). The guard itself is
            // asserted here: with `window: None` the `on_event_win` callback
            // must not be invoked at all.
            let on_event = Arc::new(AtomicUsize::new(0));
            let on_event_win = Arc::new(AtomicUsize::new(0));
            let (a, b) = (Arc::clone(&on_event), Arc::clone(&on_event_win));
            let spec = WindowSpec {
                on_event: Some(Box::new(move |_e| {
                    a.fetch_add(1, Ordering::SeqCst);
                    panic!("on_event boom");
                })),
                on_event_win: Some(Box::new(move |_w, _e| {
                    b.fetch_add(1, Ordering::SeqCst);
                    panic!("on_event_win boom");
                })),
                ..Default::default()
            };
            let mut state = crate::window::WindowState::new(
                1,
                WindowKind::Panel,
                winit::window::WindowId::from(1u64),
                None,
                Box::new(MockRenderer),
                spec,
            );
            let evt = winit::event::WindowEvent::RedrawRequested;
            dispatch_window_event(&mut state, &evt);
            assert_eq!(on_event.load(Ordering::SeqCst), 1, "on_event arm ran and was isolated");
            assert_eq!(
                on_event_win.load(Ordering::SeqCst),
                0,
                "on_event_win arm must be guard-skipped when no window is attached"
            );
        }
    }

    #[test]
    fn redraw_draws_then_presents() {
        // RESEARCH Pattern 1: handle_redraw must call draw() before present(),
        // and the on_draw closure must be invoked inside draw.
        let calls: Arc<parking_lot::Mutex<Vec<&'static str>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let renderer = RecordingRenderer { calls: calls.clone() };

        let drew: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let drew_clone = drew.clone();
        let on_draw: Option<Box<dyn Fn(&mut tiny_skia::PixmapMut, u32, u32) + Send + Sync>> =
            Some(Box::new(move |_pm, _w, _h| {
                drew_clone.fetch_add(1, Ordering::SeqCst);
            }));

        let spec = WindowSpec {
            on_draw,
            ..Default::default()
        };
        let mut state = crate::window::WindowState::new(
            1,
            crate::window::WindowKind::Panel,
            winit::window::WindowId::from(1u64),
            None,
            Box::new(renderer),
            spec,
        );

        handle_redraw(&mut state);

        let recorded = calls.lock();
        assert_eq!(
            *recorded,
            vec!["draw", "present"],
            "handle_redraw must call draw before present"
        );
        assert_eq!(
            drew.load(Ordering::SeqCst),
            1,
            "on_draw closure must be invoked exactly once"
        );
    }

    // NOTE: `user_event` itself is not unit-tested — it takes `&ActiveEventLoop`,
    // which only exists inside winit callbacks and cannot be constructed
    // headlessly (W2). Its dispatch logic is what the on_hotkey/on_menu tests
    // above assert; the five-variant match is verified by source assertion.
}
