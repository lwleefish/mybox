//! Capture session state — the shared, lock-guarded state the capture module
//! reads and mutates across threads (CAP-01). Capture results land here via
//! `UiThreadProxy` after the worker-thread capture completes.

use std::sync::Arc;

use mybox_core::log;
use mybox_core::tiny_skia;
use mybox_core::WindowId;

use crate::capture::MonitorGeom;
use crate::selection::{self, Handle};

/// Selection interaction phase (CAP-03): drag-select, then adjustable selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Selecting,
    Selected,
}

/// Selection rectangle in the owning monitor's local pixel coordinates.
///
/// `Copy`/`Clone`/`Debug`/`PartialEq` so the pure selection logic
/// (`crate::selection`) and the overlay draw closure can pass it by value and
/// unit-test it headlessly.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    pub phase: Phase,
    pub selection: Option<(usize, SelectionRect)>,
    /// Fixed drag corner, kept while `Selecting` so the selection doesn't flip
    /// as the cursor crosses the anchor (the stored rect is normalized).
    pub drag_anchor: Option<tiny_skia::Point>,
    /// The resize handle currently being dragged (D-02).
    pub active_handle: Option<Handle>,
    /// Last known cursor position, used to hit-test handles on mouse-down.
    pub last_cursor: Option<tiny_skia::Point>,
    pub current_tool: Tool,
    pub annotations: Vec<Annotation>,
    pub overlay_ids: Vec<WindowId>,
    pub pending_overlays: usize,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            shots: Vec::new(),
            phase: Phase::Idle,
            selection: None,
            drag_anchor: None,
            active_handle: None,
            last_cursor: None,
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
///
/// `Clone` shares the same state `Arc`, so `create_overlays` can clone the
/// session into every overlay's `on_draw`/`on_event` closure.
#[derive(Clone)]
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

    /// Record a created overlay window id (paired with the framework's
    /// `core/window-created` event). Only tracks windows while overlays are
    /// pending — used to destroy all overlays on ESC/confirm (CAP-05).
    pub fn window_created(&self, id: WindowId) {
        let mut state = self.state.lock().unwrap();
        if state.pending_overlays > 0 {
            state.overlay_ids.push(id);
            state.pending_overlays -= 1;
        }
    }

    /// Current interaction phase.
    pub fn phase(&self) -> Phase {
        self.state.lock().unwrap().phase
    }

    /// Snapshot of the current selection (monitor index + rect), if any.
    pub fn selection(&self) -> Option<(usize, SelectionRect)> {
        self.state.lock().unwrap().selection
    }

    /// Last known cursor position in the overlay (used for handle hit-testing).
    pub fn last_cursor(&self) -> Option<tiny_skia::Point> {
        self.state.lock().unwrap().last_cursor
    }

    /// Begin a drag selection on `monitor` at `pos` (Idle/Selected → Selecting).
    pub fn on_mouse_down(&self, monitor: usize, pos: tiny_skia::Point) {
        let mut state = self.state.lock().unwrap();
        state.phase = Phase::Selecting;
        state.drag_anchor = Some(pos);
        state.active_handle = None;
        state.selection = Some((monitor, selection::drag_start(pos)));
    }

    /// Update the selection as the cursor moves: drag-select while `Selecting`,
    /// or resize the active handle while `Selected`.
    pub fn on_mouse_move(&self, monitor: usize, pos: tiny_skia::Point) {
        let mut state = self.state.lock().unwrap();
        state.last_cursor = Some(pos);
        match state.phase {
            Phase::Selecting => {
                if let Some(anchor) = state.drag_anchor {
                    let anchor_rect = SelectionRect {
                        x0: anchor.x,
                        y0: anchor.y,
                        x1: anchor.x,
                        y1: anchor.y,
                    };
                    let mi = state
                        .selection
                        .as_ref()
                        .map(|(mi, _)| *mi)
                        .unwrap_or(monitor);
                    state.selection = Some((mi, selection::drag_update(&anchor_rect, pos)));
                }
            }
            Phase::Selected => {
                if let Some(h) = state.active_handle {
                    if let Some((mi, sel)) = state.selection {
                        if mi == monitor {
                            state.selection =
                                Some((mi, selection::apply_handle_drag(&sel, h, pos)));
                        }
                    }
                }
            }
            Phase::Idle => {}
        }
    }

    /// End a drag: `Selecting` → `Selected`.
    pub fn on_mouse_up(&self) {
        let mut state = self.state.lock().unwrap();
        if state.phase == Phase::Selecting {
            state.phase = Phase::Selected;
        }
        state.drag_anchor = None;
    }

    /// Set (or clear) the handle being dragged.
    pub fn set_active_handle(&self, h: Option<Handle>) {
        self.state.lock().unwrap().active_handle = h;
    }

    /// The currently-dragged handle, if any.
    pub fn active_handle(&self) -> Option<Handle> {
        self.state.lock().unwrap().active_handle
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

    #[test]
    fn window_created_tracks_overlay_ids_until_pending_drained() {
        let session = CaptureSession::new();
        {
            let state_arc = session.state();
            let mut state = state_arc.lock().unwrap();
            state.pending_overlays = 2;
        }
        session.window_created(10);
        session.window_created(20);
        session.window_created(30); // extra — pending already drained, ignored

        let state = session.state();
        let state = state.lock().unwrap();
        assert_eq!(state.overlay_ids, vec![10, 20]);
        assert_eq!(state.pending_overlays, 0);
    }

    #[test]
    fn drag_select_transitions_idle_to_selected() {
        let session = CaptureSession::new();
        assert_eq!(session.phase(), Phase::Idle);

        session.on_mouse_down(0, tiny_skia::Point::from_xy(10.0, 10.0));
        assert_eq!(session.phase(), Phase::Selecting);

        session.on_mouse_move(0, tiny_skia::Point::from_xy(50.0, 60.0));
        let (mi, sel) = session.selection().expect("selection exists while selecting");
        assert_eq!(mi, 0);
        assert_eq!(sel, SelectionRect { x0: 10.0, y0: 10.0, x1: 50.0, y1: 60.0 });

        session.on_mouse_up();
        assert_eq!(session.phase(), Phase::Selected);
        assert_eq!(session.selection().map(|(_, s)| s), Some(sel));
    }

    #[test]
    fn handle_drag_resizes_selection() {
        let session = CaptureSession::new();
        session.on_mouse_down(0, tiny_skia::Point::from_xy(10.0, 10.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(100.0, 100.0));
        session.on_mouse_up();

        session.set_active_handle(Some(Handle::SE));
        assert_eq!(session.active_handle(), Some(Handle::SE));

        session.on_mouse_move(0, tiny_skia::Point::from_xy(150.0, 150.0));
        let (_, sel) = session.selection().unwrap();
        assert_eq!(sel, SelectionRect { x0: 10.0, y0: 10.0, x1: 150.0, y1: 150.0 });
    }
}
