//! Display / OS integration checks for the capture module (plan 02-04-03).
//!
//! This is a **binary**, not a `#[test]`: winit on macOS requires the
//! `EventLoop` to be created on the *real main thread* and allows only one
//! `EventLoop` per process, so the `#[ignore]` integration tests spawn this
//! binary (one subprocess per check) — the same harness as `mybox-core`'s
//! `display_checks` (W2 / RESEARCH §2.4/§2.5).
//!
//! Usage: `capture_checks <overlay_capture|drag_selection|enter_clipboard|esc_destroy>`
//! Exit code 0 on success, 1 on failure, 2 on bad usage.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mybox_core::renderer::Renderer;
use mybox_core::tiny_skia::{Color, Paint, Point, Rect, Transform};
use mybox_core::window::{window_attributes, WindowKind, WindowManager, WindowSpec};
use mybox_core::winit::application::ApplicationHandler;
use mybox_core::winit::event::WindowEvent;
use mybox_core::winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use mybox_core::winit::window::WindowId;
use mybox_core::TinySkiaSoftbufferRenderer;

use mybox_capture::capture::MonitorGeom;
use mybox_capture::clipboard;
use mybox_capture::session::{CaptureSession, Phase};

/// Creates one Overlay window from a spec in `resumed()`, composites a fake
/// capture + mask, presents it, then exits. A deadline watchdog guards against
/// a hang if the first redraw never arrives.
struct OverlayHarness {
    spec: WindowSpec,
    wm: WindowManager,
    created_id: Option<mybox_core::window::WindowId>,
    presented: bool,
    deadline: Option<Instant>,
}

impl OverlayHarness {
    fn new(spec: WindowSpec) -> Self {
        Self {
            spec,
            wm: WindowManager::new(),
            created_id: None,
            presented: false,
            deadline: None,
        }
    }
}

impl ApplicationHandler for OverlayHarness {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.created_id.is_some() {
            return;
        }
        let attrs = window_attributes(&self.spec);
        let window = Arc::new(el.create_window(attrs).expect("create overlay window"));
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
        self.deadline = Some(Instant::now() + Duration::from_secs(10));
        el.set_control_flow(ControlFlow::WaitUntil(self.deadline.unwrap()));
        window.request_redraw();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::RedrawRequested = event {
            let mut ok = false;
            if let Some(state) = self.wm.get_mut_by_winit(id) {
                state.renderer.draw(&mut |pixmap, w, h| {
                    // Fake capture: an opaque fill, then a semi-transparent black
                    // mask over the whole frame (no selection) — mirrors
                    // `overlay::composite_frame` without a real capture.
                    pixmap.fill(Color::from_rgba8(0x20, 0x20, 0x20, 0xFF));
                    let mut mask = Paint::default();
                    mask.set_color_rgba8(0, 0, 0, 0x80);
                    if let Some(r) = Rect::from_xywh(0.0, 0.0, w as f32, h as f32) {
                        pixmap.fill_rect(r, &mask, Transform::identity(), None);
                    }
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

/// Check 1 — an Overlay window is created and its first `RedrawRequested`
/// composites content (capture fill + mask) and presents without panicking.
fn check_overlay_capture() -> Result<(), String> {
    let spec = WindowSpec {
        kind: WindowKind::Overlay,
        title: "capture-check".to_string(),
        inner_size: Some((2, 2)),
        transparent: true,
        always_on_top: true,
        ..Default::default()
    };
    let mut harness = OverlayHarness::new(spec);
    let event_loop = EventLoop::new().map_err(|e| format!("event loop: {e}"))?;
    event_loop.run_app(&mut harness).map_err(|e| format!("run app: {e}"))?;

    let id = harness.created_id.ok_or("overlay window must be created")?;
    if harness.wm.get_mut(id).is_none() {
        return Err("created overlay window must be registered with the WindowManager".into());
    }
    if !harness.presented {
        return Err("RedrawRequested must composite and present without panic".into());
    }
    Ok(())
}

/// Check 2 — the drag-select state machine reaches `Selected` with a non-None
/// selection after a synthetic drag (CAP-03).
fn check_drag_selection() -> Result<(), String> {
    let session = CaptureSession::new();
    session.on_mouse_down(0, Point::from_xy(10.0, 10.0));
    session.on_mouse_move(0, Point::from_xy(50.0, 60.0));
    session.on_mouse_up();

    if session.phase() != Phase::Selected {
        return Err(format!("expected Selected phase, got {:?}", session.phase()));
    }
    let (mi, sel) = session.selection().ok_or("selection must be present")?;
    if mi != 0 {
        return Err(format!("expected monitor 0, got {mi}"));
    }
    if sel.x0 != 10.0 || sel.y0 != 10.0 || sel.x1 != 50.0 || sel.y1 != 60.0 {
        return Err(format!("unexpected selection rect: {sel:?}"));
    }
    Ok(())
}

/// Check 3 — the full confirm flow: crop → bake → copy, then read the image
/// back from the clipboard and assert its dimensions match the selection
/// (CAP-04). Requires a real display/clipboard session, so it is `#[ignore]`.
fn check_enter_clipboard() -> Result<(), String> {
    // 能力探测（D-03）：Windows CI 会话可能无可用剪贴板 — 打开失败即
    // SKIPPED（非 fail、非静默）。能力探测必须先于任何断言：断言失败 = FAIL，
    // 能力失败 = SKIP，绝不 fail-to-skip。
    if std::env::consts::OS == "windows" && arboard::Clipboard::new().is_err() {
        println!("capture_checks 'enter_clipboard': SKIPPED (no clipboard in CI session)");
        return Ok(());
    }
    let session = CaptureSession::new();
    let shot = (
        MonitorGeom {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
        xcap::image::RgbaImage::new(2, 2),
    );
    session.store_shots(vec![shot]);
    session.on_mouse_down(0, Point::from_xy(0.0, 0.0));
    session.on_mouse_move(0, Point::from_xy(2.0, 2.0));
    session.on_mouse_up();

    let snapshot = session.confirm().ok_or("confirm must return a snapshot")?;
    let img = &snapshot.shot;
    let cropped = clipboard::crop_image(img, 0, 0, 2, 2);
    let baked = clipboard::bake_annotations(
        &cropped,
        2,
        2,
        &snapshot.annotations,
        Point::from_xy(0.0, 0.0),
    );
    clipboard::copy_to_clipboard(&baked, 2, 2).map_err(|e| format!("copy_to_clipboard: {e}"))?;

    let mut cb = arboard::Clipboard::new().map_err(|e| format!("open clipboard: {e}"))?;
    let read = cb.get_image().map_err(|e| format!("get_image: {e}"))?;
    if read.width != 2 || read.height != 2 {
        return Err(format!(
            "expected a 2x2 clipboard image, got {}x{}",
            read.width, read.height
        ));
    }
    Ok(())
}

/// Check 4 — ESC/cancel drains the overlay ids exactly once (CAP-05, T-2-06).
fn check_esc_destroy() -> Result<(), String> {
    let session = CaptureSession::new();
    {
        let state = session.state();
        let mut st = state.lock().unwrap();
        st.overlay_ids = vec![1, 2];
    }

    let ids = session.cancel();
    if ids != vec![1, 2] {
        return Err(format!("expected overlay ids [1, 2], got {ids:?}"));
    }
    if !session.cancel().is_empty() {
        return Err("a second cancel must return nothing (idempotent)".into());
    }
    Ok(())
}

fn main() {
    let check = std::env::args().nth(1).unwrap_or_default();
    let result = match check.as_str() {
        "overlay_capture" => check_overlay_capture(),
        "drag_selection" => check_drag_selection(),
        "enter_clipboard" => check_enter_clipboard(),
        "esc_destroy" => check_esc_destroy(),
        _ => {
            eprintln!("usage: capture_checks <overlay_capture|drag_selection|enter_clipboard|esc_destroy>");
            std::process::exit(2);
        }
    };
    match result {
        Ok(()) => println!("capture_checks '{check}': OK"),
        Err(e) => {
            eprintln!("capture_checks '{check}': FAILED: {e}");
            std::process::exit(1);
        }
    }
}
