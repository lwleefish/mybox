//! Per-monitor overlay windows (CAP-02): one full-screen `WindowKind::Overlay`
//! per captured monitor, compositing the captured image with a semi-transparent
//! black mask outside the current selection.
//!
//! Rendering is immediate-mode: every redraw re-blits the capture from
//! `SessionState.shots` and re-derives the mask from the current selection —
//! never accumulating pixels (RESEARCH Anti-Pattern).

use std::sync::Arc;

use mybox_core::log;
use mybox_core::tiny_skia::{IntSize, Paint, Pixmap, PixmapMut, PixmapPaint, Rect, Transform};
use mybox_core::window::{WindowKind, WindowManagerHandle, WindowSpec};
use mybox_core::winit;

use crate::session::{CaptureSession, SelectionRect};

/// Dimming-mask opacity (CAP-02: semi-transparent black over the un-selected
/// area).
pub const MASK_ALPHA: u8 = 0x80;

/// Convert straight-alpha RGBA8 bytes to premultiplied RGBA8 (Pitfall 2).
///
/// xcap returns straight-alpha RGBA8; `tiny_skia::Pixmap` expects premultiplied
/// bytes. Alpha is unchanged; each color channel is scaled by `a / 255`.
pub fn premultiply_rgba8(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        out.push(((u32::from(r) * u32::from(a)) / 255) as u8);
        out.push(((u32::from(g) * u32::from(a)) / 255) as u8);
        out.push(((u32::from(b) * u32::from(a)) / 255) as u8);
        out.push(a);
    }
    out
}

/// Create one overlay window per captured monitor (RESEARCH Pattern 3, D-09).
///
/// Each `WindowSpec` is an `Overlay` placed at the monitor's physical-pixel
/// geometry; `on_draw` composites that monitor's shot and `on_event` routes
/// input (drag select, handles, ESC). Capture-before-create ordering is
/// guaranteed by the caller (Pitfall 1) — `shots` is already populated.
///
/// After enqueuing, `pending_overlays` is set to the shot count so the
/// `core/window-created` event can pair framework window ids with overlays
/// (CAP-05 teardown).
pub fn create_overlays(session: &CaptureSession, windows: &Arc<WindowManagerHandle>) {
    // Collect geometry under the lock, then create without holding it.
    let geoms: Vec<(i32, i32, u32, u32)> = {
        let state = session.state();
        let state = state.lock().unwrap();
        state
            .shots
            .iter()
            .map(|(g, _)| (g.x, g.y, g.width, g.height))
            .collect()
    };

    for (monitor_index, (x, y, width, height)) in geoms.iter().enumerate() {
        let (x, y, width, height) = (*x, *y, *width, *height);
        let windows_handle = Arc::clone(windows);
        let event_windows = Arc::clone(&windows_handle);
        let session_event = session.clone();
        let session_draw = session.clone();

        let spec = WindowSpec {
            kind: WindowKind::Overlay,
            title: "capture-overlay".to_string(),
            inner_size: Some((width, height)),
            position: Some((x, y)),
            on_event: Some(Box::new(move |event| {
                handle_overlay_event(&session_event, &event_windows, monitor_index, event);
            })),
            on_draw: Some(Box::new(move |pm, w, h| {
                draw_overlay(pm, w, h, &session_draw, monitor_index);
            })),
            ..Default::default()
        };
        windows_handle.create(spec);
    }

    {
        let state = session.state();
        let mut state = state.lock().unwrap();
        state.pending_overlays = geoms.len();
    }
}

/// The `on_draw` entry: re-composite one monitor's full frame from shared state.
fn draw_overlay(
    pm: &mut PixmapMut,
    w: u32,
    h: u32,
    session: &CaptureSession,
    monitor_index: usize,
) {
    let state = session.state();
    let state = state.lock().unwrap();
    // T-2-05 guard: a stale index or empty shot set must not panic the draw
    // loop (draw closures are already catch_unwind-wrapped by the core).
    if monitor_index >= state.shots.len() {
        return;
    }
    let shot = &state.shots[monitor_index].1;
    let selection = state
        .selection
        .as_ref()
        .filter(|(mi, _)| *mi == monitor_index)
        .map(|(_, rect)| *rect);
    composite_frame(pm, w, h, shot, selection.as_ref());
}

/// Composite one frame: blit the capture, then dim everything outside the
/// selection (or the whole screen when there is none). The selection interior
/// keeps the original image (CAP-02).
fn composite_frame(
    pm: &mut PixmapMut,
    w: u32,
    h: u32,
    shot: &xcap::image::RgbaImage,
    selection: Option<&SelectionRect>,
) {
    blit_shot(pm, shot);

    let mut mask = Paint::default();
    mask.set_color_rgba8(0, 0, 0, MASK_ALPHA);

    match selection {
        None => {
            let full = Rect::from_xywh(0.0, 0.0, w as f32, h as f32).expect("non-zero overlay");
            pm.fill_rect(full, &mask, Transform::identity(), None);
        }
        Some(sel) => draw_mask_outside(pm, w as f32, h as f32, sel, &mask),
    }
}

/// Blit the (straight-alpha) capture into the pixmap, premultiplied.
fn blit_shot(pm: &mut PixmapMut, shot: &xcap::image::RgbaImage) {
    let (w, h) = (shot.width(), shot.height());
    let premul = premultiply_rgba8(shot.as_raw());
    let size = IntSize::from_wh(w, h).expect("capture has non-zero dims");
    if let Some(pixmap) = Pixmap::from_vec(premul, size) {
        pm.draw_pixmap(
            0,
            0,
            pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
}

/// Fill a rectangle, skipping zero/negative sizes (T-2-05 guard).
fn fill_rect_safe(pm: &mut PixmapMut, x: f32, y: f32, w: f32, h: f32, paint: &Paint) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(r, paint, Transform::identity(), None);
    }
}

/// Dim the four regions outside the selection (above/below/left/right).
fn draw_mask_outside(pm: &mut PixmapMut, w: f32, h: f32, sel: &SelectionRect, mask: &Paint) {
    fill_rect_safe(pm, 0.0, 0.0, w, sel.y0, mask); // above
    fill_rect_safe(pm, 0.0, sel.y1, w, h - sel.y1, mask); // below
    fill_rect_safe(pm, 0.0, sel.y0, sel.x0, sel.y1 - sel.y0, mask); // left
    fill_rect_safe(pm, sel.x1, sel.y0, w - sel.x1, sel.y1 - sel.y0, mask); // right
}

/// Per-window input routing. Drag select / handles / ESC land in Task 3.
fn handle_overlay_event(
    _session: &CaptureSession,
    _windows: &WindowManagerHandle,
    _monitor_index: usize,
    _event: &winit::event::WindowEvent,
) {
    log::trace!("overlay event (interaction lands in Task 3)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::MonitorGeom;

    fn sample_shot(r: u8, g: u8, b: u8) -> xcap::image::RgbaImage {
        let mut img = xcap::image::RgbaImage::new(4, 4);
        for p in img.pixels_mut() {
            *p = xcap::image::Rgba([r, g, b, 255]);
        }
        img
    }

    #[test]
    fn premultiply_rgba8_semi_transparent() {
        // (255,128,64,128) → (128,64,32,128): each channel scaled by 128/255.
        assert_eq!(premultiply_rgba8(&[255, 128, 64, 128]), vec![128, 64, 32, 128]);
    }

    #[test]
    fn premultiply_rgba8_zero_alpha_collapses() {
        assert_eq!(premultiply_rgba8(&[255, 255, 255, 0]), vec![0, 0, 0, 0]);
    }

    #[test]
    fn premultiply_rgba8_opaque_unchanged() {
        assert_eq!(premultiply_rgba8(&[10, 20, 30, 255]), vec![10, 20, 30, 255]);
    }

    #[test]
    fn no_selection_masks_entire_frame() {
        let mut pixmap = Pixmap::new(4, 4).expect("4x4 pixmap");
        let shot = sample_shot(255, 255, 255);
        {
            let mut pm = pixmap.as_mut();
            composite_frame(&mut pm, 4, 4, &shot, None);
        }
        let d = pixmap.data();
        // White dimmed by ~50% black → roughly 128, and the frame stays opaque.
        assert!(d[0] < 200, "mask should dim the white pixel, got {}", d[0]);
        assert!(d[3] > 0, "masked pixel keeps an opaque alpha channel");
    }

    #[test]
    fn selection_interior_keeps_original_pixel() {
        let mut pixmap = Pixmap::new(4, 4).expect("4x4 pixmap");
        let shot = sample_shot(255, 255, 255);
        let sel = SelectionRect {
            x0: 1.0,
            y0: 1.0,
            x1: 3.0,
            y1: 3.0,
        };
        {
            let mut pm = pixmap.as_mut();
            composite_frame(&mut pm, 4, 4, &shot, Some(&sel));
        }
        let d = pixmap.data();
        let center = ((2 * 4 + 2) * 4) as usize; // (2,2) inside selection
        assert_eq!(d[center], 255, "selection interior keeps the original image");
        assert!(d[0] < 200, "pixel outside the selection must be masked");
    }

    #[test]
    fn create_overlays_builds_one_overlay_spec_per_shot() {
        let session = CaptureSession::new();
        let shot = (
            MonitorGeom {
                x: 100,
                y: 200,
                width: 30,
                height: 40,
            },
            xcap::image::RgbaImage::new(30, 40),
        );
        session.store_shots(vec![shot]);

        let windows = Arc::new(WindowManagerHandle::new());
        create_overlays(&session, &windows);

        let mut creates = 0;
        while let Some(req) = windows.try_recv() {
            if let mybox_core::WindowRequest::Create(spec) = req {
                creates += 1;
                assert_eq!(spec.kind, WindowKind::Overlay);
                assert_eq!(spec.title, "capture-overlay");
                assert_eq!(spec.inner_size, Some((30, 40)));
                assert_eq!(spec.position, Some((100, 200)));
                assert!(spec.on_event.is_some(), "overlay must route events");
                assert!(spec.on_draw.is_some(), "overlay must draw");
            }
        }
        assert_eq!(creates, 1, "one overlay per captured monitor");

        let state = session.state();
        let state = state.lock().unwrap();
        assert_eq!(state.pending_overlays, 1);
    }
}
