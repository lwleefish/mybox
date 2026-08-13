//! Capture session state — the shared, lock-guarded state the capture module
//! reads and mutates across threads (CAP-01). Capture results land here via
//! `UiThreadProxy` after the worker-thread capture completes.

use std::sync::Arc;

use mybox_core::log;
use mybox_core::tiny_skia;
use mybox_core::{Event, EventBus, WindowId};

use crate::annotate::AnnotationList;
use crate::capture::MonitorGeom;
use crate::selection::{self, Handle};
use crate::toolbar::ToolAction;

/// Selection interaction phase (CAP-03): drag-select, then adjustable selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Selecting,
    /// Dragging inside the selection to move it (the Move-cursor branch).
    Moving,
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

/// Snapshot of everything needed to copy a confirmed selection to the
/// clipboard: the owning monitor index, the selection rect, that monitor's
/// captured image, and the retained annotations. Cloned out of the shared state
/// so the clipboard path runs without holding the session lock.
pub struct ConfirmSnapshot {
    pub monitor_index: usize,
    pub rect: SelectionRect,
    pub shot: xcap::image::RgbaImage,
    pub annotations: Vec<Annotation>,
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
    /// The press position when a selection move (`Moving`) began.
    pub move_anchor: Option<tiny_skia::Point>,
    /// The selection rect at the moment the move began — the base every move
    /// update translates from (no per-frame drift accumulation).
    pub move_rect: Option<SelectionRect>,
    /// The resize handle currently being dragged (D-02).
    pub active_handle: Option<Handle>,
    /// Last known cursor position, used to hit-test handles on mouse-down.
    pub last_cursor: Option<tiny_skia::Point>,
    /// The monitor whose overlay last reported a cursor position (`last_cursor`
    /// is monitor-local, so the owning monitor must be tracked separately).
    /// Drives the Enter-without-selection full-screen fallback in `confirm`.
    pub last_cursor_monitor: Option<usize>,
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
    /// Re-entrancy guard: true from the moment a capture is triggered until the
    /// session is finished/cancelled/aborted. Prevents a second trigger (rapid
    /// hotkey repeat, hotkey + tray) from stacking a second set of overlays.
    pub active: bool,
    /// Count of overlay windows that were enqueued but not yet paired with
    /// their `core/window-created` event when the session was torn down. The
    /// next `window_created` events (that many, in order) belong to this
    /// session's late-created overlays and must be destroyed immediately —
    /// otherwise they would live forever as orphaned gray-mask windows.
    pub torn_down_pending: usize,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            shots: Vec::new(),
            phase: Phase::Idle,
            selection: None,
            drag_anchor: None,
            move_anchor: None,
            move_rect: None,
            active_handle: None,
            last_cursor: None,
            last_cursor_monitor: None,
            ctrl_down: false,
            current_tool: Tool::Select,
            annotations: AnnotationList::default(),
            pending_annotation: None,
            overlay_ids: Vec::new(),
            pending_overlays: 0,
            active: false,
            torn_down_pending: 0,
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
    /// The event bus, injected once at module init so the confirm flow can emit
    /// `capture/screenshot-taken` from the main-thread overlay closure.
    bus: Arc<std::sync::OnceLock<Arc<EventBus>>>,
}

impl CaptureSession {
    /// Create an empty session.
    pub fn new() -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(SessionState::default())),
            bus: Arc::new(std::sync::OnceLock::new()),
        }
    }

    /// Inject the shared event bus (called once during module `init`).
    pub fn set_bus(&self, bus: Arc<EventBus>) {
        let _ = self.bus.set(bus);
    }

    /// Publish an event onto the shared bus (no-op before `set_bus` — e.g. in
    /// headless tests).
    pub fn emit(&self, event: Event) {
        if let Some(bus) = self.bus.get() {
            bus.emit(event);
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

    /// Claim the session for a new capture. Returns `false` (and leaves state
    /// untouched) when a capture is already in flight or overlays are live —
    /// the re-entrancy guard that prevents stacking overlays.
    pub fn begin_capture(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.active {
            return false;
        }
        state.active = true;
        // A fresh generation starts with clean pairing bookkeeping.
        state.torn_down_pending = 0;
        true
    }

    /// Release the session without teardown (capture error / permission abort).
    pub fn deactivate(&self) {
        self.state.lock().unwrap().active = false;
    }

    /// Record a created overlay window id (paired with the framework's
    /// `core/window-created` event).
    ///
    /// Returns `true` when the window must be destroyed immediately: the
    /// session was torn down (ESC/confirm) while this window's creation was
    /// still in flight, so its `window-created` event arrived after the
    /// session's overlay list was already drained — destroying it now prevents
    /// an orphaned gray-mask overlay. Returns `false` for windows the session
    /// tracks normally (and for unrelated windows, which are never tracked).
    pub fn window_created(&self, id: WindowId) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.torn_down_pending > 0 {
            state.torn_down_pending -= 1;
            return true;
        }
        if state.pending_overlays > 0 {
            state.overlay_ids.push(id);
            state.pending_overlays -= 1;
        }
        false
    }

    /// The framework window id for a given monitor's overlay, if already paired
    /// (`overlay_ids` is populated in monitor order via `window_created`). Used
    /// to target per-window cursor changes.
    pub fn overlay_id(&self, monitor_index: usize) -> Option<WindowId> {
        self.state
            .lock()
            .unwrap()
            .overlay_ids
            .get(monitor_index)
            .copied()
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

    /// Whether `pos` (in monitor-local pixels) lies inside this monitor's
    /// current selection interior. Mirrors the hover hit-test in
    /// `cursor_for` so the cursor shown and the press routing always agree.
    pub fn selection_contains(&self, monitor: usize, pos: tiny_skia::Point) -> bool {
        self.state
            .lock()
            .unwrap()
            .selection
            .map(|(mi, sel)| {
                mi == monitor
                    && pos.x >= sel.x0
                    && pos.x <= sel.x1
                    && pos.y >= sel.y0
                    && pos.y <= sel.y1
            })
            .unwrap_or(false)
    }

    /// Begin moving the existing selection (`Selected` → `Moving`) when a press
    /// lands inside it. Returns `false` (no state change) when there is no
    /// `Selected` selection on this monitor.
    pub fn on_move_start(&self, monitor: usize, pos: tiny_skia::Point) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some((mi, sel)) = state.selection else {
            return false;
        };
        if mi != monitor || state.phase != Phase::Selected {
            return false;
        }
        state.phase = Phase::Moving;
        state.move_anchor = Some(pos);
        state.move_rect = Some(sel);
        true
    }

    /// Update the selection as the cursor moves: drag-select while `Selecting`,
    /// move the whole selection while `Moving`, or resize the active handle
    /// while `Selected`.
    pub fn on_mouse_move(&self, monitor: usize, pos: tiny_skia::Point) {
        let mut state = self.state.lock().unwrap();
        state.last_cursor = Some(pos);
        state.last_cursor_monitor = Some(monitor);
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
            Phase::Moving => {
                // Translate the move-start rect by the total cursor delta —
                // absolute (no per-frame drift), clamped to the owning
                // monitor's bounds so the selection never leaves the screen.
                if let (Some(anchor), Some(orig), Some((mi, _))) =
                    (state.move_anchor, state.move_rect, state.selection)
                {
                    let dx = pos.x - anchor.x;
                    let dy = pos.y - anchor.y;
                    let bounds = state
                        .shots
                        .get(mi)
                        .map(|(g, _)| (g.width as f32, g.height as f32));
                    let moved = match bounds {
                        Some((w, h)) => selection::translate_clamped(&orig, dx, dy, w, h),
                        None => selection::translate(&orig, dx, dy),
                    };
                    state.selection = Some((mi, moved));
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

    /// End a drag: `Selecting`/`Moving` → `Selected`.
    pub fn on_mouse_up(&self) {
        let mut state = self.state.lock().unwrap();
        if state.phase == Phase::Selecting || state.phase == Phase::Moving {
            state.phase = Phase::Selected;
        }
        state.drag_anchor = None;
        state.move_anchor = None;
        state.move_rect = None;
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

    /// Snapshot the current selection (monitor index + rect), that monitor's
    /// capture, and the retained annotations for clipboard copy — without
    /// mutating the shared state. Pure and idempotent: a failed copy can be
    /// retried by calling `confirm()` again.
    ///
    /// With no selection yet, Enter falls back to the **full screen** of the
    /// monitor under the cursor (or the first captured monitor when the cursor
    /// has not been seen) instead of doing nothing — debug session
    /// `overlay-not-fullscreen-enter`.
    ///
    /// Returns `None` when there are no captured shots at all (T-2-15 guard —
    /// never enter the clipboard path on empty state).
    pub fn confirm(&self) -> Option<ConfirmSnapshot> {
        let state = self.state.lock().unwrap();
        let (monitor_index, rect) = match state.selection {
            Some(sel) => sel,
            None => {
                let mi = state
                    .last_cursor_monitor
                    .filter(|mi| *mi < state.shots.len())
                    .unwrap_or(0);
                let (geom, _) = state.shots.get(mi)?;
                (
                    mi,
                    SelectionRect {
                        x0: 0.0,
                        y0: 0.0,
                        x1: geom.width as f32,
                        y1: geom.height as f32,
                    },
                )
            }
        };
        let shot = state.shots.get(monitor_index)?.1.clone();
        let annotations = state.annotations.items.clone();
        Some(ConfirmSnapshot {
            monitor_index,
            rect,
            shot,
            annotations,
        })
    }

    /// Tear down the whole session and hand back the overlay window ids to
    /// destroy (drop-before-close, T-2-01: the sensitive captured pixels must
    /// not outlive the session). Clears `shots`, the selection, annotations,
    /// pending annotation, and overlay bookkeeping; idempotent (a second call
    /// returns an empty id list).
    pub fn finish(&self) -> Vec<WindowId> {
        let mut state = self.state.lock().unwrap();
        state.shots.clear();
        state.phase = Phase::Idle;
        state.selection = None;
        state.drag_anchor = None;
        state.move_anchor = None;
        state.move_rect = None;
        state.active_handle = None;
        state.last_cursor = None;
        state.last_cursor_monitor = None;
        state.pending_annotation = None;
        state.annotations = AnnotationList::default();
        // Overlays whose `core/window-created` events are still in flight (the
        // event bus is async) have no paired ids yet — record the count so
        // `window_created` destroys them when the late events arrive instead of
        // letting them live on as orphaned gray-mask windows.
        state.torn_down_pending = state.pending_overlays;
        state.pending_overlays = 0;
        // Reset interaction state so the next capture starts fresh (WR-04/05:
        // a stuck tool/ctrl would otherwise trap the user out of Select mode).
        state.current_tool = Tool::Select;
        state.ctrl_down = false;
        state.active = false;
        std::mem::take(&mut state.overlay_ids)
    }

    /// Cancel the whole capture (ESC, D-04): full session teardown and hand back
    /// the overlay window ids to destroy. Idempotent — `overlay_ids` is drained,
    /// so a repeated ESC destroys nothing (T-2-06).
    pub fn cancel(&self) -> Vec<WindowId> {
        self.finish()
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
        assert!(!session.window_created(10), "tracked windows are kept");
        assert!(!session.window_created(20), "tracked windows are kept");
        assert!(
            !session.window_created(30),
            "extra — pending already drained, ignored"
        );

        let state = session.state();
        let state = state.lock().unwrap();
        assert_eq!(state.overlay_ids, vec![10, 20]);
        assert_eq!(state.pending_overlays, 0);
    }

    #[test]
    fn begin_capture_guards_against_reentrancy() {
        let session = CaptureSession::new();
        assert!(session.begin_capture(), "first claim succeeds");
        assert!(
            !session.begin_capture(),
            "second claim while active is rejected"
        );

        session.deactivate();
        assert!(session.begin_capture(), "claim succeeds after deactivate");

        // finish() also releases the guard.
        session.finish();
        assert!(session.begin_capture(), "claim succeeds after finish");
    }

    #[test]
    fn finish_while_pending_destroys_late_created_windows() {
        // A session torn down while its overlay creations are still in flight:
        // the late `window-created` events must report "destroy me" instead of
        // being tracked, so no orphaned gray-mask overlay survives.
        let session = CaptureSession::new();
        {
            let state_arc = session.state();
            let mut state = state_arc.lock().unwrap();
            state.pending_overlays = 2;
        }

        let ids = session.finish();
        assert!(ids.is_empty(), "no ids were paired yet");

        assert!(session.window_created(11), "late window must be destroyed");
        assert!(session.window_created(12), "late window must be destroyed");
        assert!(
            !session.window_created(13),
            "unrelated window is not destroyed"
        );

        let state = session.state();
        let state = state.lock().unwrap();
        assert_eq!(state.torn_down_pending, 0, "late windows all consumed");
        assert!(state.overlay_ids.is_empty());
        assert!(!state.active, "finish releases the re-entrancy guard");
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
    fn selection_contains_hit_tests_interior_only() {
        let session = CaptureSession::new();
        assert!(
            !session.selection_contains(0, tiny_skia::Point::from_xy(50.0, 50.0)),
            "no selection yet"
        );

        session.on_mouse_down(0, tiny_skia::Point::from_xy(10.0, 10.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(100.0, 100.0));
        session.on_mouse_up();

        assert!(
            session.selection_contains(0, tiny_skia::Point::from_xy(50.0, 50.0)),
            "interior hit"
        );
        assert!(
            session.selection_contains(0, tiny_skia::Point::from_xy(10.0, 10.0)),
            "edge is inside"
        );
        assert!(
            !session.selection_contains(0, tiny_skia::Point::from_xy(5.0, 50.0)),
            "outside misses"
        );
        assert!(
            !session.selection_contains(1, tiny_skia::Point::from_xy(50.0, 50.0)),
            "wrong monitor misses"
        );
    }

    #[test]
    fn move_start_requires_selected_selection_on_monitor() {
        let session = CaptureSession::new();
        // No selection yet: move start rejected.
        assert!(!session.on_move_start(0, tiny_skia::Point::from_xy(50.0, 50.0)));
        assert_eq!(session.phase(), Phase::Idle);

        session.on_mouse_down(0, tiny_skia::Point::from_xy(10.0, 10.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(100.0, 100.0));
        session.on_mouse_up();
        assert_eq!(session.phase(), Phase::Selected);

        // Inside the selection: move accepted.
        assert!(session.on_move_start(0, tiny_skia::Point::from_xy(50.0, 50.0)));
        assert_eq!(session.phase(), Phase::Moving);

        // A second move start while Moving must not re-anchor.
        assert!(
            !session.on_move_start(0, tiny_skia::Point::from_xy(5.0, 5.0)),
            "already moving"
        );
    }

    #[test]
    fn move_drag_translates_selection_and_clamps_to_monitor() {
        let session = CaptureSession::new();
        // Owning monitor is 200×200 physical px (used for clamping).
        session.store_shots(vec![(
            MonitorGeom {
                x: 0,
                y: 0,
                width: 200,
                height: 200,
            },
            xcap::image::RgbaImage::new(200, 200),
        )]);

        session.on_mouse_down(0, tiny_skia::Point::from_xy(10.0, 10.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(100.0, 80.0));
        session.on_mouse_up();
        assert!(session.on_move_start(0, tiny_skia::Point::from_xy(50.0, 40.0)));

        // Drag +30, +20: the whole selection translates, size preserved.
        session.on_mouse_move(0, tiny_skia::Point::from_xy(80.0, 60.0));
        let (_, sel) = session.selection().unwrap();
        assert_eq!(sel, SelectionRect { x0: 40.0, y0: 30.0, x1: 130.0, y1: 100.0 });

        // Drag far past the edges: clamped so the selection stays on-screen.
        session.on_mouse_move(0, tiny_skia::Point::from_xy(500.0, 500.0));
        let (_, sel) = session.selection().unwrap();
        assert_eq!(sel, SelectionRect { x0: 110.0, y0: 130.0, x1: 200.0, y1: 200.0 });

        // Release returns to Selected and keeps the moved rect.
        session.on_mouse_up();
        assert_eq!(session.phase(), Phase::Selected);
        let (_, sel) = session.selection().unwrap();
        assert_eq!(sel, SelectionRect { x0: 110.0, y0: 130.0, x1: 200.0, y1: 200.0 });
    }

    #[test]
    fn finish_resets_move_state() {
        let session = CaptureSession::new();
        session.on_mouse_down(0, tiny_skia::Point::from_xy(10.0, 10.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(100.0, 100.0));
        session.on_mouse_up();
        session.on_move_start(0, tiny_skia::Point::from_xy(50.0, 50.0));

        session.finish();
        let state = session.state();
        let state = state.lock().unwrap();
        assert_eq!(state.phase, Phase::Idle);
        assert!(state.move_anchor.is_none());
        assert!(state.move_rect.is_none());
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

    #[test]
    fn confirm_returns_snapshot_and_is_idempotent() {
        let session = CaptureSession::new();
        let shot = (
            MonitorGeom {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            },
            xcap::image::RgbaImage::new(4, 4),
        );
        session.store_shots(vec![shot]);
        session.on_mouse_down(0, tiny_skia::Point::from_xy(1.0, 1.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(3.0, 3.0));
        session.on_mouse_up();

        let snap = session.confirm().expect("selection + shot must confirm");
        assert_eq!(snap.monitor_index, 0);
        assert_eq!(
            snap.rect,
            SelectionRect { x0: 1.0, y0: 1.0, x1: 3.0, y1: 3.0 }
        );
        assert_eq!((snap.shot.width(), snap.shot.height()), (4, 4));

        // Pure: a second confirm returns the same snapshot and leaves the state
        // intact (so a failed clipboard copy can be retried).
        let snap2 = session.confirm().expect("confirm is idempotent");
        assert_eq!(snap2.rect, snap.rect);
        assert_eq!(session.phase(), Phase::Selected);
        assert!(session.selection().is_some());
    }

    #[test]
    fn confirm_returns_none_without_any_shots() {
        let session = CaptureSession::new();
        assert!(
            session.confirm().is_none(),
            "no shots => None (T-2-15: never copy from empty state)"
        );

        // A selection exists but no monitor has a capture (T-2-15 guard).
        session.on_mouse_down(0, tiny_skia::Point::from_xy(1.0, 1.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(3.0, 3.0));
        session.on_mouse_up();
        assert!(session.confirm().is_none(), "selection with no shot => None");
    }

    #[test]
    fn confirm_without_selection_falls_back_to_full_monitor() {
        // Two captured monitors, no selection: Enter confirms the full screen
        // of the monitor under the cursor (falling back to the first monitor
        // before any cursor movement) — debug session
        // `overlay-not-fullscreen-enter`.
        let session = CaptureSession::new();
        session.store_shots(vec![
            (
                MonitorGeom {
                    x: 0,
                    y: 0,
                    width: 200,
                    height: 100,
                },
                xcap::image::RgbaImage::new(200, 100),
            ),
            (
                MonitorGeom {
                    x: 200,
                    y: 0,
                    width: 300,
                    height: 200,
                },
                xcap::image::RgbaImage::new(300, 200),
            ),
        ]);

        // No cursor seen yet: fall back to the first captured monitor.
        let snap = session
            .confirm()
            .expect("no selection + shots => full-screen snapshot");
        assert_eq!(snap.monitor_index, 0);
        assert_eq!(
            snap.rect,
            SelectionRect {
                x0: 0.0,
                y0: 0.0,
                x1: 200.0,
                y1: 100.0
            }
        );
        assert_eq!((snap.shot.width(), snap.shot.height()), (200, 100));

        // Cursor moved over monitor 1: full screen of monitor 1.
        session.on_mouse_move(1, tiny_skia::Point::from_xy(50.0, 50.0));
        let snap = session
            .confirm()
            .expect("cursor monitor must drive the fallback");
        assert_eq!(snap.monitor_index, 1);
        assert_eq!(
            snap.rect,
            SelectionRect {
                x0: 0.0,
                y0: 0.0,
                x1: 300.0,
                y1: 200.0
            }
        );
        assert_eq!((snap.shot.width(), snap.shot.height()), (300, 200));

        // A real selection still wins over the fallback.
        session.on_mouse_down(0, tiny_skia::Point::from_xy(10.0, 10.0));
        session.on_mouse_move(0, tiny_skia::Point::from_xy(60.0, 40.0));
        session.on_mouse_up();
        let snap = session.confirm().expect("selection must still confirm");
        assert_eq!(snap.monitor_index, 0);
        assert_eq!(
            snap.rect,
            SelectionRect {
                x0: 10.0,
                y0: 10.0,
                x1: 60.0,
                y1: 40.0
            }
        );
    }

    #[test]
    fn finish_clears_shots_annotations_and_returns_overlay_ids() {
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
        session.tool_action(ToolAction::Tool(Tool::Rect));
        session.on_annotation_start(tiny_skia::Point::from_xy(0.0, 0.0));
        session.on_annotation_update(tiny_skia::Point::from_xy(1.0, 1.0));
        session.on_annotation_finish();
        {
            let state_arc = session.state();
            let mut state = state_arc.lock().unwrap();
            state.overlay_ids = vec![1, 2];
        }

        let ids = session.finish();
        assert_eq!(ids, vec![1, 2], "finish returns the overlay ids to destroy");

        let state_arc = session.state();
        let state = state_arc.lock().unwrap();
        assert!(state.shots.is_empty(), "shots cleared (T-2-01 drop-before-close)");
        assert!(state.annotations.is_empty(), "annotations cleared");
        assert_eq!(state.phase, Phase::Idle);
        assert!(state.overlay_ids.is_empty());
        assert_eq!(state.pending_overlays, 0);
    }
}
