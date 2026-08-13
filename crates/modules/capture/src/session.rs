//! Capture session state — the shared, lock-guarded state the capture module
//! reads and mutates across threads (CAP-01). Capture results land here via
//! `UiThreadProxy` after the worker-thread capture completes.

use std::sync::Arc;

use mybox_core::log;
use mybox_core::tiny_skia;
use mybox_core::WindowId;

use crate::capture::MonitorGeom;

/// Selection rectangle in the owning monitor's local pixel coordinates.
pub struct SelectionRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// Active annotation tool (unified mode, D-03 — no explicit mode switch).
pub enum Tool {
    Select,
    Rect,
    Arrow,
    Pen,
    Text,
}

/// A retained annotation (undo = pop + full redraw from the list — never bake
/// into pixels, RESEARCH Anti-Pattern). Declared now so 02-02/02-03/02-04
/// compile against it; drawing lands in a later plan.
pub enum Annotation {
    Rect {
        a: tiny_skia::Point,
        b: tiny_skia::Point,
    },
    Arrow {
        a: tiny_skia::Point,
        b: tiny_skia::Point,
    },
    Pen {
        pts: Vec<tiny_skia::Point>,
    },
    Text {
        at: tiny_skia::Point,
        s: String,
        size: f32,
    },
}

/// Shared per-session state. Held per session (not per app) and dropped on
/// confirm/cancel (T-2-01: sensitive pixels don't outlive the session).
pub struct SessionState {
    pub shots: Vec<(MonitorGeom, xcap::image::RgbaImage)>,
    pub selection: Option<(usize, SelectionRect)>,
    pub current_tool: Tool,
    pub annotations: Vec<Annotation>,
    pub overlay_ids: Vec<WindowId>,
    pub pending_overlays: usize,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            shots: Vec::new(),
            selection: None,
            current_tool: Tool::Select,
            annotations: Vec::new(),
            overlay_ids: Vec::new(),
            pending_overlays: 0,
        }
    }
}

/// Owns the shared session state behind `Arc<std::sync::Mutex<_>>` so every
/// closure (bus handler, draw closure, `on_event`) can share one state.
///
/// `std::sync::Mutex` (not `parking_lot`) — `parking_lot` is not re-exported by
/// `mybox-core` and is not a module-crate dependency (FRMW-02); the module-crate
/// precedent (`crates/modules/test`) shares state with `std::sync::Mutex`.
pub struct CaptureSession {
    state: Arc<std::sync::Mutex<SessionState>>,
}

impl CaptureSession {
    /// Create an empty session.
    pub fn new() -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(SessionState::default())),
        }
    }

    /// Clone the shared `Arc` handle to the session state.
    pub fn state(&self) -> Arc<std::sync::Mutex<SessionState>> {
        Arc::clone(&self.state)
    }

    /// Store a fresh capture (called on the main thread via `UiThreadProxy`).
    pub fn store_shots(&self, shots: Vec<(MonitorGeom, xcap::image::RgbaImage)>) {
        let mut state = self.state.lock().unwrap();
        state.shots = shots;
        log::info!("captured {} monitors", state.shots.len());
    }
}

impl Default for CaptureSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_shots_records_geometry_and_count() {
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

        let state = session.state();
        let state = state.lock().unwrap();
        assert_eq!(state.shots.len(), 1);
        assert_eq!(state.shots[0].0.x, 0);
        assert_eq!(state.shots[0].0.y, 0);
        assert_eq!(state.shots[0].0.width, 2);
        assert_eq!(state.shots[0].0.height, 2);
        assert_eq!(state.shots[0].1.width(), 2);
        assert_eq!(state.shots[0].1.height(), 2);
    }
}
