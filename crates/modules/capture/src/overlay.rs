//! Per-monitor overlay windows (CAP-02): one full-screen `WindowKind::Overlay`
//! per captured monitor, compositing the captured image with a semi-transparent
//! black mask outside the current selection.
//!
//! Rendering is immediate-mode: every redraw re-blits the capture from
//! `SessionState.shots` and re-derives the mask from the current selection —
//! never accumulating pixels (RESEARCH Anti-Pattern).

use std::sync::Arc;

use mybox_core::tiny_skia::{
    Color, IntSize, Paint, PathBuilder, Pixmap, PixmapMut, Point, Rect, Stroke, Transform,
};
use mybox_core::window::{WindowKind, WindowManagerHandle, WindowSpec};
use mybox_core::winit::event::{ElementState, MouseButton, WindowEvent};
use mybox_core::winit::keyboard::{Key, NamedKey};
use mybox_core::{log, Event, EventPayload};

use crate::clipboard;
use crate::selection;
use crate::session::{CaptureSession, SelectionRect, Tool};
use crate::text;
use crate::toolbar::{self, ToolAction};

/// Dimming-mask opacity (CAP-02: semi-transparent black over the un-selected
/// area).
pub const MASK_ALPHA: u8 = 0x80;

/// On-screen size of the 8 resize handles (D-02: 6px white squares).
const HANDLE_SIZE: f32 = 6.0;

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

/// Convert a straight-alpha capture into a premultiplied, draw-ready [`Pixmap`].
///
/// Perf-critical: this runs ONCE per capture (in [`create_overlays`]) and the
/// result is moved into the overlay's `on_draw` closure. The per-frame draw path
/// must only blit the cached pixmap — never re-premultiply a full-resolution
/// image every frame (the previous `blit_shot` did exactly that and made
/// selection drags and annotation input unusably laggy).
pub fn premultiply_pixmap(shot: &xcap::image::RgbaImage) -> Pixmap {
    let premul = premultiply_rgba8(shot.as_raw());
    let size = IntSize::from_wh(shot.width(), shot.height()).expect("capture has non-zero dims");
    Pixmap::from_vec(premul, size).expect("premultiplied bytes must form a valid pixmap")
}

/// Premultiply a capture AND bake the full-screen dimming mask in a single pass,
/// producing the "everything dimmed" base layer. Built once per capture (in
/// [`create_overlays`]) so the per-frame draw path never blends a full-screen
/// semi-transparent mask (the previous dominant per-frame cost).
///
/// Semantically equivalent to `premultiply_pixmap` followed by a full-screen
/// `fill_rect` of `(0,0,0,MASK_ALPHA)`: each premultiplied channel is scaled by
/// `(255 - MASK_ALPHA) / 255`.
pub fn premultiply_dimmed_pixmap(shot: &xcap::image::RgbaImage) -> Pixmap {
    let raw = shot.as_raw();
    let dim = u32::from(255 - MASK_ALPHA);
    let mut out = Vec::with_capacity(raw.len());
    for px in raw.chunks_exact(4) {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        let a = u32::from(a);
        out.push(((u32::from(r) * a * dim) / (255 * 255)) as u8);
        out.push(((u32::from(g) * a * dim) / (255 * 255)) as u8);
        out.push(((u32::from(b) * a * dim) / (255 * 255)) as u8);
        out.push(a as u8);
    }
    let size = IntSize::from_wh(shot.width(), shot.height()).expect("capture has non-zero dims");
    Pixmap::from_vec(out, size).expect("premultiplied bytes must form a valid pixmap")
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
    // Precompute each monitor's premultiplied draw-ready layers ONCE under the
    // lock (perf: the `on_draw` closure must only blit cached bytes every frame,
    // never re-premultiply or re-blend the full-resolution capture per frame).
    // Window enqueuing still happens outside the lock.
    let frames: Vec<((i32, i32, u32, u32), Pixmap, Pixmap)> = {
        let state = session.state();
        let state = state.lock().unwrap();
        state
            .shots
            .iter()
            .map(|(g, img)| {
                (
                    (g.x, g.y, g.width, g.height),
                    premultiply_pixmap(img),
                    premultiply_dimmed_pixmap(img),
                )
            })
            .collect()
    };

    let pending = frames.len();

    for (monitor_index, ((x, y, width, height), frame, dimmed)) in frames.into_iter().enumerate() {
        let windows_handle = Arc::clone(windows);
        let event_windows = Arc::clone(&windows_handle);
        let session_event = session.clone();
        let session_draw = session.clone();

        let spec = WindowSpec {
            kind: WindowKind::Overlay,
            title: "capture-overlay".to_string(),
            inner_size: Some((width, height)),
            position: Some((x, y)),
            cursor_icon: Some(mybox_core::winit::window::CursorIcon::Crosshair),
            on_event: Some(Box::new(move |event| {
                handle_overlay_event(
                    &session_event,
                    &event_windows,
                    monitor_index,
                    width as f32,
                    event,
                );
            })),
            on_draw: Some(Box::new(move |pm, w, h| {
                draw_overlay(pm, w, h, &session_draw, monitor_index, &frame, &dimmed);
            })),
            ..Default::default()
        };
        windows_handle.create(spec);
    }

    {
        let state = session.state();
        let mut state = state.lock().unwrap();
        state.pending_overlays = pending;
    }
}

/// The `on_draw` entry: re-composite one monitor's full frame from shared state,
/// blitting the cached premultiplied layers (built once in `create_overlays`).
fn draw_overlay(
    pm: &mut PixmapMut,
    w: u32,
    h: u32,
    session: &CaptureSession,
    monitor_index: usize,
    frame: &Pixmap,
    dimmed: &Pixmap,
) {
    let state = session.state();
    let state = state.lock().unwrap();
    let selection = state
        .selection
        .as_ref()
        .filter(|(mi, _)| *mi == monitor_index)
        .map(|(_, rect)| *rect);
    composite_frame(pm, w, h, frame, dimmed, selection.as_ref());

    // Selection chrome + annotations + toolbar only on the monitor that owns it.
    if let Some(sel) = selection {
        let font = text::load_font();
        draw_selection_overlay(pm, &font, &sel);

        // Retained + in-progress annotations (immediate-mode: full redraw from
        // the list every frame — never baked into pixels, T-2-10).
        if let Some(pending) = &state.pending_annotation {
            pending.draw(pm);
        }
        for ann in state.annotations.iter() {
            ann.draw(pm);
        }

        // Unified no-modes toolbar, anchored below the selection's bottom-left
        // (D-03).
        let buttons = toolbar::layout_buttons((sel.x0, sel.y1), w as f32);
        toolbar::draw_toolbar(pm, &buttons, state.current_tool);
    }
}

/// Composite one frame: blit the pre-dimmed capture, then restore the selection
/// interior from the undimmed capture. Both are raw premultiplied RGBA8 buffers
/// of identical layout, so this uses `copy_from_slice` (memcpy) instead of
/// tiny-skia's per-pixel pattern fill — the per-frame full-screen blend that
/// made drag interactions laggy.
fn composite_frame(
    pm: &mut PixmapMut,
    _w: u32,
    _h: u32,
    frame: &Pixmap,
    dimmed: &Pixmap,
    selection: Option<&SelectionRect>,
) {
    let dst = pm.data_mut();
    let src = dimmed.data();
    let n = src.len().min(dst.len());
    dst[..n].copy_from_slice(&src[..n]);

    if let Some(sel) = selection {
        let fw = frame.width();
        let fh = frame.height();
        let x0 = sel.x0.round().clamp(0.0, fw as f32) as u32;
        let y0 = sel.y0.round().clamp(0.0, fh as f32) as u32;
        let x1 = sel.x1.round().clamp(0.0, fw as f32) as u32;
        let y1 = sel.y1.round().clamp(0.0, fh as f32) as u32;
        if x1 > x0 && y1 > y0 {
            let orig = frame.data();
            let stride = fw as usize * 4;
            let row_len = (x1 - x0) as usize * 4;
            for row in y0..y1 {
                let off = row as usize * stride + x0 as usize * 4;
                dst[off..off + row_len].copy_from_slice(&orig[off..off + row_len]);
            }
        }
    }
}

/// Per-window input routing (CAP-03/05/06/07, D-02/D-03/D-04): drag-select,
/// handle resize, toolbar actions, tool-driven annotation input, Ctrl+Z undo,
/// and ESC cancel. Runs on the main thread from the core's `window_event` route.
fn handle_overlay_event(
    session: &CaptureSession,
    windows: &WindowManagerHandle,
    monitor_index: usize,
    screen_w: f32,
    event: &WindowEvent,
) {
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            let pos = Point::from_xy(position.x as f32, position.y as f32);
            // Handles the selection drag, active-handle resize, and the
            // in-progress annotation endpoint/path (D-03: all coexist).
            session.on_mouse_move(monitor_index, pos);
            redraw_all_overlays(session, windows);
        }
        WindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Left,
            ..
        } => {
            if let Some(pos) = session.last_cursor() {
                // Toolbar takes priority (only present once a selection exists).
                let toolbar_action = session
                    .selection()
                    .filter(|(mi, _)| *mi == monitor_index)
                    .map(|(_, sel)| toolbar::layout_buttons((sel.x0, sel.y1), screen_w))
                    .and_then(|buttons| toolbar::hit_test(&buttons, pos));
                if let Some(action) = toolbar_action {
                    match action {
                        // Confirm/Cancel are wired to the full teardown flow
                        // (clipboard copy + destroy overlays, or full cancel).
                        ToolAction::Confirm => confirm_and_copy(session, windows),
                        ToolAction::Cancel => cancel_overlays(session, windows),
                        _ => {
                            session.tool_action(action);
                            redraw_all_overlays(session, windows);
                        }
                    }
                    return;
                }

                // If the cursor is over a handle of this monitor's selection,
                // resize it; otherwise start a fresh drag selection (D-02), or
                // begin an annotation for the active tool (D-03).
                let over_handle = session
                    .selection()
                    .filter(|(mi, _)| *mi == monitor_index)
                    .and_then(|(_, sel)| selection::hit_test_handle(&sel, pos, HANDLE_SIZE));
                if let Some(h) = over_handle {
                    session.set_active_handle(Some(h));
                } else {
                    match session.current_tool() {
                        Tool::Select => session.on_mouse_down(monitor_index, pos),
                        _ => session.on_annotation_start(pos),
                    }
                }
            }
            redraw_all_overlays(session, windows);
        }
        WindowEvent::MouseInput {
            state: ElementState::Released,
            button: MouseButton::Left,
            ..
        } => {
            session.on_mouse_up();
            session.on_annotation_finish();
            session.set_active_handle(None);
            redraw_all_overlays(session, windows);
        }
        WindowEvent::ModifiersChanged(mods) => {
            let state = mods.state();
            // CAP-07 text says "Ctrl+Z"; on macOS also accept Cmd.
            session.set_ctrl_down(state.control_key() || state.super_key());
        }
        WindowEvent::KeyboardInput { event, .. } => {
            if event.state != ElementState::Pressed {
                return;
            }
            if event.logical_key == Key::Named(NamedKey::Escape) {
                // ESC cancels everything (CAP-05, D-04): destroy all overlays,
                // copy nothing (idempotent, T-2-06).
                cancel_overlays(session, windows);
            } else if event.logical_key == Key::Named(NamedKey::Enter) {
                // Enter confirms: crop + bake + copy to clipboard, then close
                // every overlay (CAP-04, D-01, D-04).
                confirm_and_copy(session, windows);
            } else if event.logical_key == "z" && session.ctrl_down() {
                // Ctrl+Z (Cmd+Z on macOS) undoes the last annotation (CAP-07).
                session.undo();
                redraw_all_overlays(session, windows);
            }
        }
        _ => {}
    }
}

/// Request a repaint for every live overlay (immediate-mode: any input change
/// redraws the whole frame — RESEARCH Pitfall 3, keep `ControlFlow::Wait`).
fn redraw_all_overlays(session: &CaptureSession, windows: &WindowManagerHandle) {
    let state = session.state();
    let state = state.lock().unwrap();
    for id in state.overlay_ids.iter() {
        windows.redraw(*id);
    }
}

/// ESC / toolbar-cancel teardown (CAP-05, D-04): full session reset (which also
/// drops the captured pixels — T-2-01) and destroy every overlay window. Copies
/// nothing.
fn cancel_overlays(session: &CaptureSession, windows: &WindowManagerHandle) {
    let ids = session.cancel();
    for id in ids {
        windows.destroy(id);
    }
}

/// Enter / toolbar-confirm flow (CAP-04, D-01, D-04): crop the selection, bake
/// the retained annotations, write to the clipboard on the main thread (this
/// runs from `on_event`, which satisfies the clipboard thread-affinity), then
/// destroy all overlays and clear the session. On any clipboard error the
/// overlays stay open for a retry (T-2-12).
fn confirm_and_copy(session: &CaptureSession, windows: &WindowManagerHandle) {
    let Some(snapshot) = session.confirm() else {
        return; // no selection or no capture — nothing to copy (T-2-15)
    };
    let img = &snapshot.shot;
    let rect = snapshot.rect;
    let (iw, ih) = (img.width(), img.height());

    let x0 = rect.x0.round().clamp(0.0, iw as f32) as u32;
    let y0 = rect.y0.round().clamp(0.0, ih as f32) as u32;
    let x1 = rect.x1.round().clamp(0.0, iw as f32) as u32;
    let y1 = rect.y1.round().clamp(0.0, ih as f32) as u32;
    if x1 <= x0 || y1 <= y0 {
        log::error!("capture: empty selection — not copying (T-2-15)");
        return;
    }
    let w = x1 - x0;
    let h = y1 - y0;

    let cropped = clipboard::crop_image(img, x0, y0, w, h);
    let baked = clipboard::bake_annotations(
        &cropped,
        w,
        h,
        &snapshot.annotations,
        Point::from_xy(x0 as f32, y0 as f32),
    );

    if let Err(e) = clipboard::copy_to_clipboard(&baked, w as usize, h as usize) {
        log::error!("capture: clipboard copy failed — overlays stay open: {e:#}");
        return;
    }

    // Success: destroy every overlay, clear the session (drop-before-close,
    // T-2-01), and notify the bus.
    let ids = session.finish();
    for id in ids {
        windows.destroy(id);
    }
    session.emit(Event {
        from: "capture",
        kind: "screenshot-taken",
        payload: EventPayload::Module(serde_json::json!({})),
    });
    log::info!("capture: selection copied to clipboard; overlays closed");
}

/// Draw the selection chrome: white border, 8 handles, and the WxH size label
/// (CAP-03, D-02).
fn draw_selection_overlay(pm: &mut PixmapMut, font: &ab_glyph::FontArc, sel: &SelectionRect) {
    draw_selection_border(pm, sel);
    draw_handles(pm, sel);
    draw_size_label(pm, font, sel);
}

/// White selection border (1.5px), drawn as a stroked rect path.
fn draw_selection_border(pm: &mut PixmapMut, sel: &SelectionRect) {
    let Some(rect) = Rect::from_xywh(
        sel.x0,
        sel.y0,
        (sel.x1 - sel.x0).max(0.0),
        (sel.y1 - sel.y0).max(0.0),
    ) else {
        return; // degenerate (zero-size) selection has no border yet
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    let stroke = Stroke {
        width: 1.5,
        ..Default::default()
    };
    let path = PathBuilder::from_rect(rect);
    pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

/// The 8 resize handles: 6px white squares with a 1px black outline.
fn draw_handles(pm: &mut PixmapMut, sel: &SelectionRect) {
    let mut fill = Paint::default();
    fill.set_color_rgba8(255, 255, 255, 255);
    let mut outline = Paint::default();
    outline.set_color_rgba8(0, 0, 0, 255);
    let stroke = Stroke {
        width: 1.0,
        ..Default::default()
    };
    for h in selection::HANDLES {
        let r = selection::handle_rect(sel, h, HANDLE_SIZE);
        pm.fill_rect(r, &fill, Transform::identity(), None);
        let path = PathBuilder::from_rect(r);
        pm.stroke_path(&path, &outline, &stroke, Transform::identity(), None);
    }
}

/// The `"{w} × {h}"` label, drawn just above the selection's top-left corner.
fn draw_size_label(pm: &mut PixmapMut, font: &ab_glyph::FontArc, sel: &SelectionRect) {
    let w = (sel.x1 - sel.x0).round() as u32;
    let h = (sel.y1 - sel.y0).round() as u32;
    let label = format!("{w} × {h}");
    text::draw_text(pm, font, &label, (sel.x0, sel.y0 - 6.0), 18.0, Color::WHITE);
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
        let dimmed = premultiply_dimmed_pixmap(&shot);
        let frame = premultiply_pixmap(&shot);
        {
            let mut pm = pixmap.as_mut();
            composite_frame(&mut pm, 4, 4, &frame, &dimmed, None);
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
        let frame = premultiply_pixmap(&shot);
        let dimmed = premultiply_dimmed_pixmap(&shot);
        let sel = SelectionRect {
            x0: 1.0,
            y0: 1.0,
            x1: 3.0,
            y1: 3.0,
        };
        {
            let mut pm = pixmap.as_mut();
            composite_frame(&mut pm, 4, 4, &frame, &dimmed, Some(&sel));
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

    #[test]
    fn selection_overlay_draws_white_handles() {
        let font = text::load_font();
        let mut pixmap = Pixmap::new(100, 100).expect("100x100 pixmap");
        let sel = SelectionRect {
            x0: 20.0,
            y0: 20.0,
            x1: 80.0,
            y1: 80.0,
        };
        {
            let mut pm = pixmap.as_mut();
            draw_selection_overlay(&mut pm, &font, &sel);
        }
        // The NW handle is a 6px white square centered on (20, 20); its centre
        // must be white (border + handle fill are both white).
        let d = pixmap.data();
        let idx = ((20 * 100 + 20) * 4) as usize;
        assert_eq!(&d[idx..idx + 3], &[255, 255, 255], "NW handle must be white");
    }
}
