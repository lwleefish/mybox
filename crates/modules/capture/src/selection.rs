//! Pure selection geometry (CAP-03, D-02): drag-select and the 8 resize handles
//! (four corners + four edge midpoints).
//!
//! Every function is headless and unit-testable — no winit, no I/O. Coordinates
//! are physical pixels in the owning monitor's local space. The rectangle is
//! stored normalized (`x0 <= x1`, `y0 <= y1`) for display; the drag anchor is
//! tracked separately by the session so multi-step drags keep the original
//! corner fixed.

use mybox_core::tiny_skia::{Point, Rect};

use crate::session::SelectionRect;

/// The 8 resize handles: four corners + four edge midpoints (D-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

/// All 8 handles in a stable drawing/hit-test order.
pub const HANDLES: [Handle; 8] = [
    Handle::N,
    Handle::NE,
    Handle::E,
    Handle::SE,
    Handle::S,
    Handle::SW,
    Handle::W,
    Handle::NW,
];

/// Minimum selection dimension in physical pixels (T-2-05 guard).
pub const MIN_SELECTION: f32 = 4.0;

/// Normalize a selection so `x0 <= x1` and `y0 <= y1`.
pub fn normalize(r: SelectionRect) -> SelectionRect {
    SelectionRect {
        x0: r.x0.min(r.x1),
        y0: r.y0.min(r.y1),
        x1: r.x0.max(r.x1),
        y1: r.y0.max(r.y1),
    }
}

/// The handle's anchor point on the selection (corner / edge midpoint).
fn handle_center(sel: &SelectionRect, h: Handle) -> (f32, f32) {
    let cx = (sel.x0 + sel.x1) / 2.0;
    let cy = (sel.y0 + sel.y1) / 2.0;
    match h {
        Handle::N => (cx, sel.y0),
        Handle::NE => (sel.x1, sel.y0),
        Handle::E => (sel.x1, cy),
        Handle::SE => (sel.x1, sel.y1),
        Handle::S => (cx, sel.y1),
        Handle::SW => (sel.x0, sel.y1),
        Handle::W => (sel.x0, cy),
        Handle::NW => (sel.x0, sel.y0),
    }
}

/// The handle's on-screen rectangle: a square of `size` px centered on its anchor.
pub fn handle_rect(sel: &SelectionRect, h: Handle, size: f32) -> Rect {
    let (cx, cy) = handle_center(sel, h);
    let half = size / 2.0;
    Rect::from_xywh(cx - half, cy - half, size, size).expect("non-zero handle size")
}

/// The handle under `pos`, if any.
pub fn hit_test_handle(sel: &SelectionRect, pos: Point, size: f32) -> Option<Handle> {
    for h in HANDLES {
        let r = handle_rect(sel, h, size);
        if pos.x >= r.left() && pos.x <= r.right() && pos.y >= r.top() && pos.y <= r.bottom() {
            return Some(h);
        }
    }
    None
}

/// Move the edge/corner for `h` to `pos`, clamping to a minimum size.
pub fn apply_handle_drag(sel: &SelectionRect, h: Handle, pos: Point) -> SelectionRect {
    let mut r = *sel;
    let min = MIN_SELECTION;
    match h {
        Handle::N => r.y0 = pos.y.min(r.y1 - min),
        Handle::S => r.y1 = pos.y.max(r.y0 + min),
        Handle::W => r.x0 = pos.x.min(r.x1 - min),
        Handle::E => r.x1 = pos.x.max(r.x0 + min),
        Handle::NW => {
            r.x0 = pos.x.min(r.x1 - min);
            r.y0 = pos.y.min(r.y1 - min);
        }
        Handle::NE => {
            r.x1 = pos.x.max(r.x0 + min);
            r.y0 = pos.y.min(r.y1 - min);
        }
        Handle::SW => {
            r.x0 = pos.x.min(r.x1 - min);
            r.y1 = pos.y.max(r.y0 + min);
        }
        Handle::SE => {
            r.x1 = pos.x.max(r.x0 + min);
            r.y1 = pos.y.max(r.y0 + min);
        }
    }
    r
}

/// Start a new selection anchored at `pos` (a zero-size rect).
pub fn drag_start(pos: Point) -> SelectionRect {
    SelectionRect {
        x0: pos.x,
        y0: pos.y,
        x1: pos.x,
        y1: pos.y,
    }
}

/// Extend a drag from its anchor `sel` to `pos`, normalized for display.
///
/// `sel` is the drag-start anchor rect (`x0 == x1 && y0 == y1`), so `(x0, y0)`
/// is the fixed corner; the result is normalized so `x0 <= x1 && y0 <= y1`.
pub fn drag_update(sel: &SelectionRect, pos: Point) -> SelectionRect {
    normalize(SelectionRect {
        x0: sel.x0,
        y0: sel.y0,
        x1: pos.x,
        y1: pos.y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> SelectionRect {
        SelectionRect { x0, y0, x1, y1 }
    }

    fn p(x: f32, y: f32) -> Point {
        Point::from_xy(x, y)
    }

    #[test]
    fn normalize_reorders_inverted_coords() {
        assert_eq!(normalize(rect(50.0, 60.0, 10.0, 20.0)), rect(10.0, 20.0, 50.0, 60.0));
    }

    #[test]
    fn normalize_keeps_ordered_coords() {
        assert_eq!(normalize(rect(10.0, 20.0, 50.0, 60.0)), rect(10.0, 20.0, 50.0, 60.0));
    }

    #[test]
    fn drag_start_then_drag_update_produces_normalized_rect() {
        let start = drag_start(p(100.0, 100.0));
        assert_eq!(start, rect(100.0, 100.0, 100.0, 100.0));

        // Drag up-and-left: the result is normalized around the fixed anchor.
        let updated = drag_update(&start, p(50.0, 60.0));
        assert_eq!(updated, rect(50.0, 60.0, 100.0, 100.0));
    }

    #[test]
    fn hit_test_returns_each_of_eight_handles() {
        let sel = rect(10.0, 10.0, 110.0, 110.0);
        for h in HANDLES {
            let r = handle_rect(&sel, h, 8.0);
            let center = p((r.left() + r.right()) / 2.0, (r.top() + r.bottom()) / 2.0);
            assert_eq!(
                hit_test_handle(&sel, center, 8.0),
                Some(h),
                "handle {:?} must hit at its center",
                h
            );
        }
    }

    #[test]
    fn hit_test_misses_off_handle() {
        let sel = rect(10.0, 10.0, 110.0, 110.0);
        // Interior of the selection, far from any handle.
        assert_eq!(hit_test_handle(&sel, p(60.0, 60.0), 8.0), None);
    }

    #[test]
    fn apply_handle_drag_moves_edge() {
        let sel = rect(10.0, 10.0, 100.0, 100.0);
        // N handle: move the top edge up.
        assert_eq!(
            apply_handle_drag(&sel, Handle::N, p(50.0, 0.0)),
            rect(10.0, 0.0, 100.0, 100.0)
        );
        // E handle: move the right edge right.
        assert_eq!(
            apply_handle_drag(&sel, Handle::E, p(150.0, 50.0)),
            rect(10.0, 10.0, 150.0, 100.0)
        );
    }

    #[test]
    fn apply_handle_drag_moves_corner() {
        let sel = rect(10.0, 10.0, 100.0, 100.0);
        assert_eq!(
            apply_handle_drag(&sel, Handle::SE, p(150.0, 150.0)),
            rect(10.0, 10.0, 150.0, 150.0)
        );
        assert_eq!(
            apply_handle_drag(&sel, Handle::NW, p(0.0, 0.0)),
            rect(0.0, 0.0, 100.0, 100.0)
        );
    }

    #[test]
    fn apply_handle_drag_clamps_to_minimum_size() {
        let sel = rect(10.0, 10.0, 100.0, 100.0);
        // Dragging the N handle far below y1 must clamp to y1 - 4.
        assert_eq!(
            apply_handle_drag(&sel, Handle::N, p(50.0, 200.0)),
            rect(10.0, 96.0, 100.0, 100.0)
        );
        // Dragging the SE handle inside the rect clamps to x0 + 4 / y0 + 4.
        assert_eq!(
            apply_handle_drag(&sel, Handle::SE, p(0.0, 0.0)),
            rect(10.0, 10.0, 14.0, 14.0)
        );
    }
}
