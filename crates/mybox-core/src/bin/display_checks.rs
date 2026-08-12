//! Display / OS integration checks for the mybox framework (plan 01-04-05).
//!
//! This is a **binary**, not a `#[test]`: winit on macOS requires the
//! `EventLoop` to be created on the *real main thread* (`MainThreadMarker`
//! panics otherwise) and allows only one `EventLoop` per process. Rust's
//! `cargo test` harness runs each `#[test]` on a spawned worker thread, so the
//! checks cannot run inline in `tests/integration.rs`. Instead, the `#[ignore]`
//! integration tests spawn this binary (one subprocess per check) via
//! `std::process::Command`; each check runs on its own process's main thread
//! (W2 / RESEARCH §2.4/§2.5).
//!
//! Usage: `display_checks <panel|overlay|hotkey|tray>`
//! Exit code 0 on success, 1 on failure, 2 on bad usage.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mybox_core::renderer::Renderer;
use mybox_core::window::{window_attributes, WindowKind, WindowManager, WindowSpec};
use mybox_core::TinySkiaSoftbufferRenderer;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

/// Creates one window from a spec in `resumed()`, routes a real redraw through
/// `WindowManager` (D-07) to `TinySkiaSoftbufferRenderer::present`, then exits.
/// A `ControlFlow::WaitUntil` deadline guards against hanging if the first
/// redraw never arrives on a broken setup.
struct WindowHarness {
    spec: WindowSpec,
    wm: WindowManager,
    created_id: Option<mybox_core::window::WindowId>,
    presented: bool,
    deadline: Option<Instant>,
}

impl WindowHarness {
    fn new(spec: WindowSpec) -> Self {
        Self {
            spec,
            wm: WindowManager::new(Box::new(|window: Arc<winit::window::Window>| {
                Box::new(TinySkiaSoftbufferRenderer::new(window).expect("renderer"))
                    as Box<dyn Renderer>
            })),
            created_id: None,
            presented: false,
            deadline: None,
        }
    }
}

impl ApplicationHandler for WindowHarness {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.created_id.is_some() {
            return;
        }
        let attrs = window_attributes(&self.spec);
        let window = Arc::new(el.create_window(attrs).expect("create window"));
        let winit_id = window.id();
        let id = self.wm.next_id();
        let renderer = TinySkiaSoftbufferRenderer::new(Arc::clone(&window)).expect("renderer");
        let kind = self.spec.kind;
        self.wm.register(
            id,
            kind,
            winit_id,
            Some(Arc::clone(&window)),
            Box::new(renderer) as Box<dyn Renderer>,
            WindowSpec {
                kind,
                title: self.spec.title.clone(),
                transparent: self.spec.transparent,
                always_on_top: self.spec.always_on_top,
                ..Default::default()
            },
        );
        self.created_id = Some(id);
        // Watchdog: if the first redraw does not arrive, exit (fail) after 10s
        // rather than hang.
        self.deadline = Some(Instant::now() + Duration::from_secs(10));
        el.set_control_flow(ControlFlow::WaitUntil(self.deadline.unwrap()));
        window.request_redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::RedrawRequested = event {
            let mut ok = false;
            if let Some(state) = self.wm.get_mut_by_winit(id) {
                state.renderer.draw(&mut |pixmap, _w, _h| {
                    // Opaque content: softbuffer drops alpha on macOS anyway
                    // (RESEARCH §0.5), so an opaque fill is the Phase-1 contract.
                    pixmap.fill(tiny_skia::Color::from_rgba8(0x30, 0x90, 0xC0, 0xFF));
                });
                state.renderer.present().expect("present on RedrawRequested");
                ok = true;
            }
            self.presented = ok;
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                el.exit();
            }
        }
    }
}

/// Check 1 — a Panel window is created and its first `RedrawRequested` presents
/// without panicking (FRMW-03, D-02 present pipeline).
fn check_panel() -> Result<(), String> {
    let spec = WindowSpec {
        kind: WindowKind::Panel,
        title: "panel integration test".to_string(),
        inner_size: Some((400, 300)),
        ..Default::default()
    };
    let mut harness = WindowHarness::new(spec);
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop.run_app(&mut harness).map_err(|e| format!("run app: {e}"))?;

    let id = harness.created_id.ok_or("window must be created")?;
    if harness.wm.get_mut(id).is_none() {
        return Err("created window must be registered with the WindowManager".into());
    }
    if !harness.presented {
        return Err("RedrawRequested must present without panic".into());
    }
    Ok(())
}

/// Check 2 — an Overlay window (transparent + undecorated + always-on-top) is
/// created and registered (FRMW-03 profile; real alpha is Phase 2, RESEARCH §0.5).
fn check_overlay() -> Result<(), String> {
    let spec = WindowSpec {
        kind: WindowKind::Overlay,
        title: "overlay integration test".to_string(),
        inner_size: Some((800, 600)),
        transparent: true,
        always_on_top: true,
        ..Default::default()
    };
    let mut harness = WindowHarness::new(spec);
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop.run_app(&mut harness).map_err(|e| format!("run app: {e}"))?;

    let id = harness.created_id.ok_or("overlay window must be created")?;
    let state = harness
        .wm
        .get_mut(id)
        .ok_or("created overlay window must be registered")?;
    if !state.spec.transparent {
        return Err("overlay spec should request transparency".into());
    }
    Ok(())
}

/// Check 3 — the global hotkey manager initializes and a config string
/// registers successfully, returning a positive id (FRMW-04, D-11).
fn check_hotkey() -> Result<(), String> {
    let hm = mybox_core::HotkeyManager::new();
    hm.init().map_err(|e| format!("hotkey init: {e}"))?;
    let id = hm
        .register_str("test", "Cmd+Shift+T")
        .map_err(|e| format!("register_str: {e}"))?;
    if id == 0 {
        return Err("registered hotkey must have a positive id".into());
    }
    Ok(())
}

/// Check 4 — the tray builds with the runtime-generated icon and the menu with
/// module items (INFRA-02; needs a live macOS menu-bar session).
fn check_tray() -> Result<(), String> {
    let mut tray = mybox_core::TrayManager::default();
    let items = vec![mybox_core::tray_icon::menu::MenuItem::with_id(
        "test.open_window",
        "打开测试窗口",
        true,
        None,
    )];
    tray.build(items, 32).map_err(|e| format!("tray build: {e}"))
}

fn main() {
    let check = std::env::args().nth(1).unwrap_or_default();
    let result = match check.as_str() {
        "panel" => check_panel(),
        "overlay" => check_overlay(),
        "hotkey" => check_hotkey(),
        "tray" => check_tray(),
        _ => {
            eprintln!("usage: display_checks <panel|overlay|hotkey|tray>");
            std::process::exit(2);
        }
    };
    match result {
        Ok(()) => {
            println!("display_checks '{check}': OK");
        }
        Err(e) => {
            eprintln!("display_checks '{check}': FAILED: {e}");
            std::process::exit(1);
        }
    }
}
