//! Unified no-modes annotation toolbar (D-03): confirm/cancel/undo + the four
//! annotation tools, drawn with tiny-skia (NO egui — RESEARCH) and resolved by
//! hit-testing stored button rects inside `on_event`.

use mybox_core::tiny_skia::{
    Color, LineCap, Paint, PathBuilder, PixmapMut, Point, Rect, Stroke, Transform,
};

use crate::session::Tool;
use crate::text;

/// Button side length in physical pixels.
pub const BUTTON_SIZE: f32 = 32.0;
/// Gap between adjacent buttons.
pub const BUTTON_GAP: f32 = 4.0;

/// A toolbar action (D-03 unified toolbar). `Tool(t)` switches the active
/// annotation tool; `Confirm`/`Cancel` are wired in 02-04; `Undo` pops the last
/// annotation (CAP-07).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAction {
    Confirm,
    Cancel,
    Undo,
    Tool(Tool),
}

/// A laid-out button: its action and on-screen rectangle.
#[derive(Clone, Copy, Debug)]
pub struct ToolbarButton {
    pub action: ToolAction,
    pub rect: Rect,
}

/// Lay out the seven toolbar buttons in a horizontal row, anchored just below
/// the selection's bottom-left corner. If the row would overflow the right
/// edge, it is shifted left (clamped to the screen).
pub fn layout_buttons(selection_bottom_left: (f32, f32), screen_w: f32) -> Vec<ToolbarButton> {
    const ACTIONS: [ToolAction; 7] = [
        ToolAction::Confirm,
        ToolAction::Cancel,
        ToolAction::Undo,
        ToolAction::Tool(Tool::Rect),
        ToolAction::Tool(Tool::Arrow),
        ToolAction::Tool(Tool::Pen),
        ToolAction::Tool(Tool::Text),
    ];

    let total_w = ACTIONS.len() as f32 * BUTTON_SIZE + (ACTIONS.len() - 1) as f32 * BUTTON_GAP;
    let mut x = selection_bottom_left.0;
    if x + total_w > screen_w {
        x = (screen_w - total_w).max(0.0);
    }
    let y = selection_bottom_left.1 + 6.0; // a small gap below the selection

    let mut buttons = Vec::with_capacity(ACTIONS.len());
    for action in ACTIONS {
        let rect = Rect::from_xywh(x, y, BUTTON_SIZE, BUTTON_SIZE).expect("non-zero button size");
        buttons.push(ToolbarButton { action, rect });
        x += BUTTON_SIZE + BUTTON_GAP;
    }
    buttons
}

/// The button whose rect contains `pos`, if any.
pub fn hit_test(buttons: &[ToolbarButton], pos: Point) -> Option<ToolAction> {
    buttons
        .iter()
        .find(|b| {
            pos.x >= b.rect.left()
                && pos.x <= b.rect.right()
                && pos.y >= b.rect.top()
                && pos.y <= b.rect.bottom()
        })
        .map(|b| b.action)
}

/// Draw the toolbar: dark-gray buttons with a white outline; the current tool's
/// button is highlighted orange.
pub fn draw_toolbar(pm: &mut PixmapMut, buttons: &[ToolbarButton], current: Tool) {
    let font = text::load_font();
    for b in buttons {
        let is_current = b.action == ToolAction::Tool(current);

        let mut bg = Paint::default();
        if is_current {
            bg.set_color_rgba8(0xFF, 0x60, 0x00, 0xFF);
        } else {
            bg.set_color_rgba8(0x40, 0x40, 0x40, 0xFF);
        }
        pm.fill_rect(b.rect, &bg, Transform::identity(), None);

        let mut border = Paint::default();
        border.set_color_rgba8(255, 255, 255, 255);
        let border_path = PathBuilder::from_rect(b.rect);
        let border_stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pm.stroke_path(&border_path, &border, &border_stroke, Transform::identity(), None);

        draw_button_icon(pm, &font, b);
    }
}

/// Draw the icon for one button: tiny-skia paths for the shapes, a single text
/// glyph for the text tool.
fn draw_button_icon(pm: &mut PixmapMut, font: &ab_glyph::FontArc, b: &ToolbarButton) {
    let cx = b.rect.x() + b.rect.width() / 2.0;
    let cy = b.rect.y() + b.rect.height() / 2.0;

    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    let stroke = Stroke {
        width: 2.0,
        line_cap: LineCap::Round,
        ..Stroke::default()
    };

    match b.action {
        ToolAction::Confirm => {
            // Checkmark: two segments.
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 6.0, cy + 1.0);
            pb.line_to(cx - 2.0, cy + 5.0);
            pb.line_to(cx + 7.0, cy - 5.0);
            stroke_built_path(pm, pb, &paint, &stroke);
        }
        ToolAction::Cancel => {
            // An X: two crossing segments.
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 5.0, cy - 5.0);
            pb.line_to(cx + 5.0, cy + 5.0);
            pb.move_to(cx + 5.0, cy - 5.0);
            pb.line_to(cx - 5.0, cy + 5.0);
            stroke_built_path(pm, pb, &paint, &stroke);
        }
        ToolAction::Undo => {
            // A counterclockwise arc with an arrowhead.
            let mut pb = PathBuilder::new();
            pb.move_to(cx + 6.0, cy + 4.0);
            pb.cubic_to(cx + 2.0, cy - 5.0, cx - 7.0, cy - 4.0, cx - 7.0, cy + 1.0);
            pb.move_to(cx - 7.0, cy + 1.0);
            pb.line_to(cx - 11.0, cy - 1.0);
            pb.move_to(cx - 7.0, cy + 1.0);
            pb.line_to(cx - 5.0, cy - 3.0);
            stroke_built_path(pm, pb, &paint, &stroke);
        }
        ToolAction::Tool(Tool::Rect) => {
            // A hollow square.
            let r = Rect::from_xywh(cx - 7.0, cy - 7.0, 14.0, 14.0).expect("icon rect");
            let path = PathBuilder::from_rect(r);
            pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
        ToolAction::Tool(Tool::Arrow) => {
            // A diagonal line with a small head.
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 8.0, cy + 7.0);
            pb.line_to(cx + 6.0, cy - 7.0);
            pb.move_to(cx + 6.0, cy - 7.0);
            pb.line_to(cx + 0.0, cy - 6.0);
            pb.move_to(cx + 6.0, cy - 7.0);
            pb.line_to(cx + 5.0, cy - 1.0);
            stroke_built_path(pm, pb, &paint, &stroke);
        }
        ToolAction::Tool(Tool::Pen) => {
            // A freehand curve.
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 8.0, cy + 7.0);
            pb.quad_to(cx, cy - 8.0, cx + 8.0, cy + 3.0);
            stroke_built_path(pm, pb, &paint, &stroke);
        }
        ToolAction::Tool(Tool::Text) => {
            text::draw_text(pm, font, "A", (cx - 6.0, cy + 6.0), 18.0, Color::WHITE);
        }
        ToolAction::Tool(Tool::Select) => {
            // Select is not a toolbar button (D-03: no explicit mode switch).
        }
    }
}

/// Finish a path builder and stroke it (skips an empty/invalid path).
fn stroke_built_path(pm: &mut PixmapMut, pb: PathBuilder, paint: &Paint, stroke: &Stroke) {
    if let Some(path) = pb.finish() {
        pm.stroke_path(&path, paint, stroke, Transform::identity(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mybox_core::tiny_skia::Pixmap;

    fn p(x: f32, y: f32) -> Point {
        Point::from_xy(x, y)
    }

    #[test]
    fn layout_buttons_produces_seven_in_order() {
        let buttons = layout_buttons((100.0, 200.0), 1920.0);
        assert_eq!(buttons.len(), 7);
        let actions: Vec<ToolAction> = buttons.iter().map(|b| b.action).collect();
        assert_eq!(actions[0], ToolAction::Confirm);
        assert_eq!(actions[1], ToolAction::Cancel);
        assert_eq!(actions[2], ToolAction::Undo);
        assert_eq!(actions[3], ToolAction::Tool(Tool::Rect));
        assert_eq!(actions[4], ToolAction::Tool(Tool::Arrow));
        assert_eq!(actions[5], ToolAction::Tool(Tool::Pen));
        assert_eq!(actions[6], ToolAction::Tool(Tool::Text));
    }

    #[test]
    fn layout_buttons_anchors_below_selection_bottom_left() {
        let buttons = layout_buttons((100.0, 200.0), 1920.0);
        // First button sits 6px below the selection bottom (y = 200).
        assert_eq!(buttons[0].rect.x(), 100.0);
        assert_eq!(buttons[0].rect.y(), 206.0);
        assert_eq!(buttons[1].rect.x(), 100.0 + BUTTON_SIZE + BUTTON_GAP);
    }

    #[test]
    fn layout_buttons_clamps_when_overflowing_right_edge() {
        let total = 7.0 * BUTTON_SIZE + 6.0 * BUTTON_GAP;
        let buttons = layout_buttons((900.0, 100.0), 800.0);
        assert!(buttons[6].rect.right() <= 800.0, "last button must stay on screen");
        assert_eq!(buttons[0].rect.x(), (800.0 - total).max(0.0));
    }

    #[test]
    fn hit_test_hits_and_misses() {
        let buttons = layout_buttons((100.0, 200.0), 1920.0);
        let center = p(
            buttons[3].rect.x() + BUTTON_SIZE / 2.0,
            buttons[3].rect.y() + BUTTON_SIZE / 2.0,
        );
        assert_eq!(hit_test(&buttons, center), Some(ToolAction::Tool(Tool::Rect)));

        // A point in the gap between buttons misses.
        let gap = p(buttons[0].rect.right() + 1.0, buttons[0].rect.y() + 1.0);
        assert_eq!(hit_test(&buttons, gap), None);
    }

    #[test]
    fn draw_toolbar_highlights_current_tool() {
        let buttons = layout_buttons((0.0, 0.0), 1920.0);
        let mut pixmap = Pixmap::new(260, 40).expect("pixmap");
        {
            let mut pm = pixmap.as_mut();
            draw_toolbar(&mut pm, &buttons, Tool::Rect);
        }
        // The Rect button (index 3) background corner must be orange.
        let rect_btn = buttons[3].rect;
        let cx = (rect_btn.x() + 2.0) as u32;
        let cy = (rect_btn.y() + 2.0) as u32;
        let data = pixmap.data();
        let i = ((cy * 260 + cx) * 4) as usize;
        assert_eq!(
            &data[i..i + 3],
            &[0xFF, 0x60, 0x00],
            "current tool button background must be orange"
        );
    }
}
