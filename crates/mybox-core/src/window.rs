//! Window types (FRMW-03) and the main-thread-bound [`WindowManager`] (D-07).
//!
//! `window_attributes` is a pure `WindowSpec` → winit `WindowAttributes`
//! builder (unit-testable, no platform calls). `WindowManager` owns the
//! id → state routing table; modules never touch it directly — they enqueue
//! [`WindowRequest`]s through [`WindowManagerHandle`], which the 01-04 App
//! drains and executes on the main thread.

use std::collections::HashMap;
use std::sync::Arc;

use crate::renderer::Renderer;

/// Three window kinds the framework supports (FRMW-03).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowKind {
    /// Fullscreen transparent overlay (screenshot selection, Phase 2).
    Overlay,
    /// Independent always-on-top floating window (pin-type, Phase 2).
    Floating,
    /// Regular panel window (command palette, Phase 3).
    Panel,
}

/// Abstract window creation spec (D-07 centralized + ID dispatch).
///
/// All fields are `pub` so modules in separate crates can construct specs with
/// struct-literal syntax (e.g. `WindowSpec { kind, title, inner_size, .. }`).
pub struct WindowSpec {
    pub kind: WindowKind,
    pub title: String,
    pub transparent: bool,
    pub always_on_top: bool,
    pub decorations: bool,
    pub visible: bool,
    /// Physical pixels.
    pub inner_size: Option<(u32, u32)>,
    /// Physical pixels.
    pub position: Option<(i32, i32)>,
    /// Mouse cursor shown while over this window (e.g. `Crosshair` for the
    /// screenshot overlay). `None` keeps the platform default.
    pub cursor_icon: Option<winit::window::CursorIcon>,
    /// Per-window event callback (D-07 routing target).
    pub on_event: Option<Box<dyn Fn(&winit::event::WindowEvent) + Send + Sync>>,
    /// Per-window draw callback (Phase 2): receives the tiny-skia pixmap and its
    /// size so the module can composite content (capture blit, mask, annotations,
    /// toolbar). Invoked by `handle_redraw` on the main thread before `present()`.
    pub on_draw: Option<Box<dyn Fn(&mut tiny_skia::PixmapMut, u32, u32) + Send + Sync>>,
}

impl Default for WindowSpec {
    fn default() -> Self {
        Self {
            kind: WindowKind::Panel,
            title: "mybox".to_string(),
            transparent: false,
            always_on_top: false,
            decorations: true,
            visible: true,
            inner_size: None,
            position: None,
            cursor_icon: None,
            on_event: None,
            on_draw: None,
        }
    }
}

/// Framework window id — a u64 incrementing counter (RESEARCH §11 #6).
pub type WindowId = u64;

/// Build winit `WindowAttributes` for a spec (pure function — no platform
/// calls, unit-testable).
///
/// The window `kind` decides the profile: Overlay is transparent + undecorated
/// + always-on-top; Floating is undecorated + always-on-top; Panel is decorated.
/// The spec's `visible`/`always_on_top`/`inner_size`/`position` are then mapped
/// onto the attributes. (`spec.transparent`/`spec.decorations` are profile
/// inputs owned by `kind`, so they are not re-applied here — applying them
/// would clobber the Overlay/Floating profiles.)
pub fn window_attributes(spec: &WindowSpec) -> winit::window::WindowAttributes {
    let mut attrs = winit::window::Window::default_attributes()
        .with_title(&spec.title)
        .with_visible(spec.visible);
    match spec.kind {
        WindowKind::Overlay => {
            attrs = attrs
                .with_transparent(true)
                .with_decorations(false)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        }
        WindowKind::Floating => {
            attrs = attrs
                .with_decorations(false)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        }
        WindowKind::Panel => {
            attrs = attrs.with_decorations(true);
        }
    }
    if spec.always_on_top {
        attrs = attrs.with_window_level(winit::window::WindowLevel::AlwaysOnTop);
    }
    if let Some((w, h)) = spec.inner_size {
        attrs = attrs.with_inner_size(winit::dpi::PhysicalSize::new(w, h));
    }
    if let Some((x, y)) = spec.position {
        attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(x, y));
    }
    if let Some(icon) = spec.cursor_icon {
        attrs = attrs.with_cursor(icon);
    }
    attrs
}

/// Runtime state for a live window (D-07 routing target).
///
/// Fields are `pub` so the App's routing hot path (§10.3) can read
/// `spec`/`renderer`/`id` directly.
pub struct WindowState {
    pub id: WindowId,
    pub kind: WindowKind,
    pub winit_id: winit::window::WindowId,
    /// The OS window, shared via `Arc` with the renderer (softbuffer's Surface
    /// holds the window, so it must be shared rather than owned twice). `None`
    /// when no window is attached yet (the D-07 enqueue → main-thread-execute
    /// flow) — winit windows cannot be constructed headlessly. The 01-04 App
    /// passes `Some(Arc::new(window))` after `ActiveEventLoop::create_window`.
    pub window: Option<Arc<winit::window::Window>>,
    pub renderer: Box<dyn Renderer>,
    pub spec: WindowSpec,
}

impl WindowState {
    /// Construct a state. The `window` is provided by the caller (App on the
    /// main thread).
    pub fn new(
        id: WindowId,
        kind: WindowKind,
        winit_id: winit::window::WindowId,
        window: Option<Arc<winit::window::Window>>,
        renderer: Box<dyn Renderer>,
        spec: WindowSpec,
    ) -> Self {
        Self {
            id,
            kind,
            winit_id,
            window,
            renderer,
            spec,
        }
    }
}

/// A queued window operation, produced by [`WindowManagerHandle`] and executed
/// by the App on the main thread (RESEARCH §2.3).
///
/// No `Debug` derive: `WindowSpec` holds a closure (`on_event`) and is not
/// `Debug`-able.
pub enum WindowRequest {
    Create(WindowSpec),
    Destroy(WindowId),
    /// Request a redraw for a window (Phase 2). The App drains this in
    /// `about_to_wait` and calls `window.request_redraw()` on the main thread.
    Redraw(WindowId),
}

/// Module-side handle to the window manager. Modules call `create`/`destroy`,
/// which enqueue a [`WindowRequest`] and fire the wake hook so the main loop
/// (`ControlFlow::Wait`) wakes deterministically (W3 — crossbeam send alone
/// would not wake a waiting event loop).
pub struct WindowManagerHandle {
    tx: crossbeam_channel::Sender<WindowRequest>,
    /// `None` when the caller owns the receiver (the App drains via its own
    /// `window_rx`, W2); `Some` on the standalone `new()` path.
    rx: Option<crossbeam_channel::Receiver<WindowRequest>>,
    wake: parking_lot::Mutex<Option<Arc<dyn Fn() + Send + Sync + 'static>>>,
}

impl WindowManagerHandle {
    /// Create a handle backed by a fresh request channel.
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            tx,
            rx: Some(rx),
            wake: parking_lot::Mutex::new(None),
        }
    }

    /// Build a handle whose sender was created externally so the caller (the
    /// App in `AppBuilder::build`) can own the receiver half for main-thread
    /// draining (`App::window_rx`, W2). The standalone `try_recv` is unavailable
    /// on this path — the caller drains instead.
    pub fn from_sender(tx: crossbeam_channel::Sender<WindowRequest>) -> Self {
        Self {
            tx,
            rx: None,
            wake: parking_lot::Mutex::new(None),
        }
    }

    /// Enqueue a window creation and wake the loop.
    pub fn create(&self, spec: WindowSpec) {
        let _ = self.tx.send(WindowRequest::Create(spec));
        self.trigger_wake();
    }

    /// Enqueue a window destruction and wake the loop.
    pub fn destroy(&self, id: WindowId) {
        let _ = self.tx.send(WindowRequest::Destroy(id));
        self.trigger_wake();
    }

    /// Enqueue a redraw request and wake the loop (Phase 2). Modules call this
    /// from the bus worker thread; the App drains it in `about_to_wait` and
    /// calls `window.request_redraw()` on the main thread.
    pub fn redraw(&self, id: WindowId) {
        let _ = self.tx.send(WindowRequest::Redraw(id));
        self.trigger_wake();
    }

    /// Inject the wake-up hook. Default is `None` (no-op). The App installs a
    /// hook that pulses `AppEvent::WindowRequested` into the winit loop.
    pub fn set_wakeup(&self, wake: Arc<dyn Fn() + Send + Sync + 'static>) {
        *self.wake.lock() = Some(wake);
    }

    /// Drain one queued request, if any. Available on the standalone `new()`
    /// path; the App (which owns the receiver) drains through `App::window_rx`.
    pub fn try_recv(&self) -> Option<WindowRequest> {
        self.rx.as_ref().and_then(|rx| rx.try_recv().ok())
    }

    fn trigger_wake(&self) {
        if let Some(w) = &*self.wake.lock() {
            w();
        }
    }
}

impl Default for WindowManagerHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Main-thread-bound window manager (holds non-`Send` winit windows). Stored
/// inside the App; never touches winit from other threads.
pub struct WindowManager {
    states: HashMap<WindowId, WindowState>,
    next_id: WindowId,
}

impl WindowManager {
    /// Create an empty manager. Renderers are constructed per-window in
    /// `App::create_window` via the App's own `renderer_factory`; the manager
    /// only owns the id -> state routing table.
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            next_id: 1,
        }
    }

    /// Allocate the next framework window id (monotonically increasing).
    pub fn next_id(&mut self) -> WindowId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Insert a window state into the routing table.
    pub fn register(
        &mut self,
        id: WindowId,
        kind: WindowKind,
        winit_id: winit::window::WindowId,
        window: Option<Arc<winit::window::Window>>,
        renderer: Box<dyn Renderer>,
        spec: WindowSpec,
    ) {
        self.states
            .insert(id, WindowState::new(id, kind, winit_id, window, renderer, spec));
    }

    /// Remove and return a state, if present.
    pub fn destroy(&mut self, id: WindowId) -> Option<WindowState> {
        self.states.remove(&id)
    }

    /// Look up a state by framework id.
    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut WindowState> {
        self.states.get_mut(&id)
    }

    /// Look up a state by winit window id — the routing hot path (D-07).
    pub fn get_mut_by_winit(&mut self, winit_id: winit::window::WindowId) -> Option<&mut WindowState> {
        self.states.values_mut().find(|s| s.winit_id == winit_id)
    }

    /// Close every window.
    pub fn close_all(&mut self) {
        self.states.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_kind_has_three_distinct_variants() {
        let kinds = [WindowKind::Overlay, WindowKind::Floating, WindowKind::Panel];
        assert_eq!(kinds.len(), 3);
        assert_ne!(WindowKind::Overlay, WindowKind::Floating);
        assert_ne!(WindowKind::Floating, WindowKind::Panel);
        assert_ne!(WindowKind::Overlay, WindowKind::Panel);
    }

    #[test]
    fn window_spec_default_is_panel_decorated_visible() {
        let spec = WindowSpec::default();
        assert_eq!(spec.kind, WindowKind::Panel);
        assert_eq!(spec.title, "mybox");
        assert!(spec.decorations);
        assert!(spec.visible);
        assert!(!spec.transparent);
        assert!(!spec.always_on_top);
        assert!(spec.inner_size.is_none());
        assert!(spec.position.is_none());
        assert!(spec.on_event.is_none());
        assert!(spec.on_draw.is_none());
    }

    #[test]
    fn window_spec_fields_are_public_for_struct_literal() {
        // Exercises the public-field contract: modules in separate crates build
        // WindowSpec with `..Default::default()` (mybox-test does this in 01-04).
        let spec = WindowSpec {
            kind: WindowKind::Overlay,
            title: "overlay".to_string(),
            inner_size: Some((1920, 1080)),
            ..Default::default()
        };
        assert_eq!(spec.kind, WindowKind::Overlay);
        assert_eq!(spec.inner_size, Some((1920, 1080)));
        assert!(spec.decorations); // inherited from Default
    }

    #[test]
    fn window_id_is_u64() {
        let id: WindowId = 42u64;
        assert_eq!(id, 42);
    }
}

/// WindowManager / window_attributes / WindowManagerHandle tests. Separate
/// module so the `window_manager::` nextest filter selects exactly these.
#[cfg(test)]
mod window_manager {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Inert renderer: window map logic never draws.
    struct MockRenderer;
    impl Renderer for MockRenderer {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn draw(&mut self, _f: &mut dyn FnMut(&mut tiny_skia::PixmapMut, u32, u32)) {}
        fn present(&mut self) -> crate::error::Result<()> {
            Ok(())
        }
    }

    fn register_dummy(wm: &mut WindowManager, id: WindowId, winit_id: winit::window::WindowId) {
        wm.register(
            id,
            WindowKind::Panel,
            winit_id,
            None,
            Box::new(MockRenderer),
            WindowSpec::default(),
        );
    }

    #[test]
    fn window_attributes_panel_is_decorated() {
        let attrs = window_attributes(&WindowSpec {
            kind: WindowKind::Panel,
            ..Default::default()
        });
        assert!(attrs.decorations, "Panel must keep decorations");
    }

    #[test]
    fn window_attributes_overlay_is_transparent_always_on_top() {
        let attrs = window_attributes(&WindowSpec {
            kind: WindowKind::Overlay,
            ..Default::default()
        });
        assert!(attrs.transparent, "Overlay must request transparency");
        assert!(!attrs.decorations, "Overlay must be undecorated");
        assert_eq!(
            attrs.window_level,
            winit::window::WindowLevel::AlwaysOnTop,
            "Overlay must be always-on-top"
        );
    }

    #[test]
    fn window_attributes_floating_is_always_on_top() {
        let attrs = window_attributes(&WindowSpec {
            kind: WindowKind::Floating,
            ..Default::default()
        });
        assert!(!attrs.decorations, "Floating must be undecorated");
        assert_eq!(
            attrs.window_level,
            winit::window::WindowLevel::AlwaysOnTop,
            "Floating must be always-on-top"
        );
    }

    #[test]
    fn window_attributes_maps_inner_size_and_position() {
        let spec = WindowSpec {
            inner_size: Some((800, 600)),
            position: Some((10, 20)),
            ..Default::default()
        };
        let attrs = window_attributes(&spec);
        assert_eq!(
            attrs.inner_size,
            Some(winit::dpi::Size::Physical(winit::dpi::PhysicalSize::new(800, 600)))
        );
        assert_eq!(
            attrs.position,
            Some(winit::dpi::Position::Physical(winit::dpi::PhysicalPosition::new(10, 20)))
        );
    }

    #[test]
    fn window_attributes_maps_visible_and_always_on_top() {
        let spec = WindowSpec {
            visible: false,
            always_on_top: true,
            ..Default::default()
        };
        let attrs = window_attributes(&spec);
        assert!(!attrs.visible, "spec.visible=false must map to hidden");
        assert_eq!(
            attrs.window_level,
            winit::window::WindowLevel::AlwaysOnTop,
            "spec.always_on_top must map to AlwaysOnTop"
        );
    }

    #[test]
    fn next_id_increments_monotonically() {
        let mut wm = WindowManager::new();
        let a = wm.next_id();
        let b = wm.next_id();
        assert!(a < b, "next_id must increase");
    }

    #[test]
    fn register_then_get_mut_and_destroy() {
        let mut wm = WindowManager::new();
        let id = wm.next_id();
        register_dummy(&mut wm, id, winit::window::WindowId::from(1u64));
        assert!(wm.get_mut(id).is_some(), "registered state must be found");
        assert!(wm.destroy(id).is_some(), "destroy must return the state");
        assert!(wm.get_mut(id).is_none(), "destroyed state must be gone");
    }

    #[test]
    fn consecutive_registers_get_distinct_ids() {
        let mut wm = WindowManager::new();
        let id1 = wm.next_id();
        let id2 = wm.next_id();
        register_dummy(&mut wm, id1, winit::window::WindowId::from(1u64));
        register_dummy(&mut wm, id2, winit::window::WindowId::from(2u64));
        assert_ne!(id1, id2);
        assert!(wm.get_mut(id1).is_some());
        assert!(wm.get_mut(id2).is_some());
    }

    #[test]
    fn get_mut_by_winit_hits_correct_state() {
        let mut wm = WindowManager::new();
        let id1 = wm.next_id();
        let id2 = wm.next_id();
        register_dummy(&mut wm, id1, winit::window::WindowId::from(100u64));
        register_dummy(&mut wm, id2, winit::window::WindowId::from(200u64));

        let found = wm.get_mut_by_winit(winit::window::WindowId::from(100u64));
        assert!(found.is_some(), "winit id 100 must resolve");
        assert_eq!(found.unwrap().id, id1, "winit id 100 must map to id1");

        let found2 = wm.get_mut_by_winit(winit::window::WindowId::from(200u64));
        assert!(found2.is_some(), "winit id 200 must resolve");
        assert_eq!(found2.unwrap().id, id2);

        assert!(
            wm.get_mut_by_winit(winit::window::WindowId::from(999u64)).is_none(),
            "unknown winit id must not resolve"
        );
    }

    #[test]
    fn close_all_empties_state_table() {
        let mut wm = WindowManager::new();
        let id1 = wm.next_id();
        let id2 = wm.next_id();
        register_dummy(&mut wm, id1, winit::window::WindowId::from(1u64));
        register_dummy(&mut wm, id2, winit::window::WindowId::from(2u64));
        wm.close_all();
        assert!(wm.get_mut(id1).is_none());
        assert!(wm.get_mut(id2).is_none());
    }

    #[test]
    fn handle_create_and_destroy_trigger_wake_hook() {
        // W3: the wake hook must fire after every enqueue so a `ControlFlow::Wait`
        // loop is woken deterministically.
        let handle = WindowManagerHandle::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        handle.set_wakeup(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        handle.create(WindowSpec::default());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "create must wake once");
        handle.destroy(42);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "destroy must wake once");
        handle.redraw(42);
        assert_eq!(calls.load(Ordering::SeqCst), 3, "redraw must wake once");
    }

    #[test]
    fn handle_queues_requests_in_order() {
        let handle = WindowManagerHandle::new();
        handle.create(WindowSpec::default());
        assert!(matches!(handle.try_recv(), Some(WindowRequest::Create(_))));
        handle.destroy(7);
        assert!(matches!(handle.try_recv(), Some(WindowRequest::Destroy(7))));
        handle.redraw(7);
        assert!(matches!(handle.try_recv(), Some(WindowRequest::Redraw(7))));
        assert!(handle.try_recv().is_none(), "queue must be empty after draining");
    }
}
