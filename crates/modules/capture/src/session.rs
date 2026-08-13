//! Capture session state — the shared, lock-guarded state the capture module
//! reads and mutates across threads (CAP-01). Capture results land here via
//! `UiThreadProxy` after the worker-thread capture completes.

use std::sync::Arc;

use mybox_core::log;
use mybox_core::tiny_skia;
use mybox_core::WindowId;

use crate::annotate::AnnotationList;
use crate::capture::MonitorGeom;
use crate::selection::{self, Handle};
use crate::toolbar::ToolAction;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Rect,
    Arrow,
    Pen,
    Text,
}

/// A retained annotation (undo = pop + full redraw from the list — never bake
/// into pixels, RESEARCH Anti-Pattern). Declared here (02-01 skeleton) so
/// 02-02/02-03/02-04 compile against it; drawing lives in `crate::annotate`.
#[derive(Clone, Debug, PartialEq)]
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

/// Default text placed by the text tool on click (A6: no text editing in MVP —
/// a fixed label is placed once; editing lands in a later plan).
pub const DEFAULT_TEXT_ANNOTATION: &str = "Text";
/// Default size (physical px) of a placed text annotation.
pub const DEFAULT_TEXT_SIZE: f32 = 18.0;

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
    /// Whether Ctrl (or Cmd on macOS) is currently held, tracked via
    /// `ModifiersChanged` so Ctrl+Z can be detected on `KeyboardInput`.
    pub ctrl_down: bool,
    pub current_tool: Tool,
    /// Retained annotations (undo = pop + full redraw — CAP-07).
    pub annotations: AnnotationList,
    /// The in-progress annotation being drawn (rendered but not yet committed
    /// to `annotations` until the drag finishes).
    pub pending_annotation: Option<Annotation>,
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
            ctrl_down: false,
            current_tool: Tool::Select,
            annotations: AnnotationList::default(),
            pending_annotation: None,
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
        // Track the in-progress annotation drag (rect/arrow/pen) in parallel
        // with the selection/handle drag (D-03: both coexist).
        update_pending_annotation(&mut state, pos);
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

    /// The active annotation tool (D-03).
    pub fn current_tool(&self) -> Tool {
        self.state.lock().unwrap().current_tool
    }

    /// Handle a toolbar action: switch tool, pop an annotation, or (02-04)
    /// confirm/cancel — the latter two are logged for now (CAP-04 lands next).
    pub fn tool_action(&self, action: ToolAction) {
        match action {
            ToolAction::Tool(t) => {
                self.state.lock().unwrap().current_tool = t;
            }
            ToolAction::Undo => {
                self.undo();
            }
            ToolAction::Confirm => {
                log::info!("capture: confirm requested (clipboard copy wired in 02-04)");
            }
            ToolAction::Cancel => {
                log::info!("capture: cancel requested (wired in 02-04)");
            }
        }
    }

    /// Pop the most recent annotation (Ctrl+Z, CAP-07). Returns whether an
    /// annotation was actually undone.
    pub fn undo(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        state.annotations.undo().is_some()
    }

    /// Append a completed annotation to the retained list.
    pub fn push_annotation(&self, ann: Annotation) {
        self.state.lock().unwrap().annotations.push(ann);
    }

    /// Track the Ctrl/Cmd modifier state (via `ModifiersChanged`).
    pub fn set_ctrl_down(&self, down: bool) {
        self.state.lock().unwrap().ctrl_down = down;
    }

    /// Whether Ctrl (or Cmd) is currently held.
    pub fn ctrl_down(&self) -> bool {
        self.state.lock().unwrap().ctrl_down
    }

    /// Begin an annotation at `pos`, routed by the current tool. Rect/Arrow/Pen
    /// set a `pending_annotation`; Text is placed immediately (A6: no editing).
    pub fn on_annotation_start(&self, pos: tiny_skia::Point) {
        let mut state = self.state.lock().unwrap();
        match state.current_tool {
            Tool::Rect => {
                state.pending_annotation = Some(Annotation::Rect { a: pos, b: pos });
            }
            Tool::Arrow => {
                state.pending_annotation = Some(Annotation::Arrow { a: pos, b: pos });
            }
            Tool::Pen => {
                state.pending_annotation = Some(Annotation::Pen { pts: vec![pos] });
            }
            Tool::Text => {
                state.annotations.push(Annotation::Text {
                    at: pos,
                    s: DEFAULT_TEXT_ANNOTATION.to_string(),
                    size: DEFAULT_TEXT_SIZE,
                });
            }
            Tool::Select => {}
        }
    }

    /// Update the in-progress annotation's endpoint/path as the cursor moves.
    pub fn on_annotation_update(&self, pos: tiny_skia::Point) {
        let mut state = self.state.lock().unwrap();
        update_pending_annotation(&mut state, pos);
    }

    /// Commit the in-progress annotation into the retained list.
    pub fn on_annotation_finish(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(ann) = state.pending_annotation.take() {
            state.annotations.push(ann);
        }
    }

    /// Cancel the whole capture (ESC, D-04): reset to `Idle`, clear the
    /// selection, and hand back the overlay window ids to destroy. Idempotent —
    /// `overlay_ids` is drained, so a repeated ESC destroys nothing (T-2-06).
    pub fn cancel(&self) -> Vec<WindowId> {
        let mut state = self.state.lock().unwrap();
        state.phase = Phase::Idle;
        state.selection = None;
        state.drag_anchor = None;
        state.active_handle = None;
        state.last_cursor = None;
        std::mem::take(&mut state.overlay_ids)
    }
}

/// Update the endpoint of the in-progress rect/arrow, or append a point to the
/// in-progress pen path. No-op when nothing is pending.
fn update_pending_annotation(state: &mut SessionState, pos: tiny_skia::Point) {
    match &mut state.pending_annotation {
        Some(Annotation::Rect { b, .. }) | Some(Annotation::Arrow { b, .. }) => *b = pos,
        Some(Annotation::Pen { pts }) => pts.push(pos),
        _ => {}
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

    #[test]
    fn cancel_resets_and_returns_overlay_ids_once() {
        let session = CaptureSession::new();
        {
            let state_arc = session.state();
            let mut state = state_arc.lock().unwrap();
            state.pending_overlays = 2;
        }
        session.window_created(7);
        session.window_created(8);

        // Build a Selected selection first (CAP-05: cancel clears a live selection).
        session.on_mouse_down(0, tiny_skia::Point::from_xy(10.0, 10.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(50.0, 50.0));
        session.on_mouse_up();
        assert_eq!(session.phase(), Phase::Selected);

        let ids = session.cancel();
        assert_eq!(ids, vec![7, 8], "cancel returns the overlay ids to destroy");
        assert_eq!(session.phase(), Phase::Idle);
        assert_eq!(session.selection(), None);

        // Idempotent: overlay_ids is drained, so a second cancel returns nothing.
        assert_eq!(session.cancel(), Vec::<WindowId>::new());
    }

    #[test]
    fn tool_action_switches_current_tool() {
        let session = CaptureSession::new();
        assert_eq!(session.current_tool(), Tool::Select);

        session.tool_action(ToolAction::Tool(Tool::Rect));
        assert_eq!(session.current_tool(), Tool::Rect);

        session.tool_action(ToolAction::Tool(Tool::Pen));
        assert_eq!(session.current_tool(), Tool::Pen);
    }

    #[test]
    fn rect_annotation_drag_produces_one_rect() {
        let session = CaptureSession::new();
        session.tool_action(ToolAction::Tool(Tool::Rect));

        session.on_annotation_start(tiny_skia::Point::from_xy(10.0, 10.0));
        session.on_annotation_update(tiny_skia::Point::from_xy(50.0, 60.0));
        session.on_annotation_finish();

        let state_arc = session.state();
        let state = state_arc.lock().unwrap();
        assert_eq!(state.annotations.items.len(), 1);
        assert_eq!(
            state.annotations.items[0],
            Annotation::Rect {
                a: tiny_skia::Point::from_xy(10.0, 10.0),
                b: tiny_skia::Point::from_xy(50.0, 60.0),
            }
        );
        assert!(state.pending_annotation.is_none(), "finished annotation must not stay pending");
    }

    #[test]
    fn undo_to_empty_equals_original_image() {
        let session = CaptureSession::new();
        session.tool_action(ToolAction::Tool(Tool::Rect));

        // Draw three rects (CAP-07: undo back to the original image).
        for i in 0..3 {
            let x = i as f32 * 10.0;
            session.on_annotation_start(tiny_skia::Point::from_xy(x, x));
            session.on_annotation_update(tiny_skia::Point::from_xy(x + 5.0, x + 5.0));
            session.on_annotation_finish();
        }
        {
            let state_arc = session.state();
            let state = state_arc.lock().unwrap();
            assert_eq!(state.annotations.items.len(), 3);
        }

        assert!(session.undo(), "first undo must pop an annotation");
        assert!(session.undo());
        assert!(session.undo());
        assert!(!session.undo(), "undo on an empty list must report false");

        let state_arc = session.state();
        let state = state_arc.lock().unwrap();
        assert!(state.annotations.is_empty(), "undo to empty == original image (CAP-07)");
    }

    #[test]
    fn text_tool_places_immediately() {
        let session = CaptureSession::new();
        session.tool_action(ToolAction::Tool(Tool::Text));
        session.on_annotation_start(tiny_skia::Point::from_xy(30.0, 40.0));

        let state_arc = session.state();
        let state = state_arc.lock().unwrap();
        assert_eq!(state.annotations.items.len(), 1, "text is placed immediately (A6)");
        assert_eq!(
            state.annotations.items[0],
            Annotation::Text {
                at: tiny_skia::Point::from_xy(30.0, 40.0),
                s: DEFAULT_TEXT_ANNOTATION.to_string(),
                size: DEFAULT_TEXT_SIZE,
            }
        );
        assert!(state.pending_annotation.is_none(), "text never enters the pending slot");
    }
}
