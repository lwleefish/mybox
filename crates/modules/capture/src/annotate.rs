//! Retained annotation model + drawing (CAP-06) + undo stack (CAP-07).
//!
//! Annotations are retained in a [`Vec`]-backed [`AnnotationList`] and redrawn
//! every frame from the list (never baked into pixels — RESEARCH Anti-Pattern,
//! T-2-10). Undo pops the last annotation; drawing uses annotation orange
//! `0xFF6000` with round caps (T-2-08: empty/zero-size inputs return safely).

use mybox_core::tiny_skia::{
    Color, FillRule, LineCap, Paint, PathBuilder, PixmapMut, Point, Rect, Stroke, Transform,
};

use crate::session::Annotation;
use crate::text;

/// The annotation color: opaque orange `0xFF6000`.
pub fn annotation_color() -> Color {
    Color::from_rgba8(0xFF, 0x60, 0x00, 0xFF)
}

/// A fresh orange fill/stroke paint for annotation shapes.
fn annotation_paint() -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(0xFF, 0x60, 0x00, 0xFF);
    paint
}

/// The annotation stroke: 3px, round caps (soft freehand feel).
fn annotation_stroke() -> Stroke {
    Stroke {
        width: 3.0,
        line_cap: LineCap::Round,
        ..Stroke::default()
    }
}

/// Max length (in chars) of a text annotation, to prevent malformed layout from
/// oversized input (T-2-09).
const MAX_TEXT_CHARS: usize = 64;

impl Annotation {
    /// Draw this annotation into `pm` (immediate-mode: called every redraw from
    /// the retained list). Empty/zero-size inputs return without panicking
    /// (T-2-08 guard).
    pub fn draw(&self, pm: &mut PixmapMut) {
        match self {
            Annotation::Rect { a, b } => draw_rect(pm, *a, *b),
            Annotation::Arrow { a, b } => draw_arrow(pm, *a, *b),
            Annotation::Pen { pts } => draw_pen(pm, pts),
            Annotation::Text { at, s, size } => draw_text_annotation(pm, *at, s, *size),
        }
    }
}

/// A stroked rectangle from corner `a` to corner `b` (normalized).
fn draw_rect(pm: &mut PixmapMut, a: Point, b: Point) {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = a.x.max(b.x);
    let bottom = a.y.max(b.y);
    // `from_ltrb` returns `None` for a zero-size rect (T-2-08 guard).
    if let Some(rect) = Rect::from_ltrb(left, top, right, bottom) {
        let path = PathBuilder::from_rect(rect);
        let paint = annotation_paint();
        let stroke = annotation_stroke();
        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// A stroked line from `a` to `b` with a filled triangular arrowhead at `b`.
fn draw_arrow(pm: &mut PixmapMut, a: Point, b: Point) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    let paint = annotation_paint();
    let stroke = annotation_stroke();

    let mut pb = PathBuilder::new();
    pb.move_to(a.x, a.y);
    pb.line_to(b.x, b.y);
    if let Some(path) = pb.finish() {
        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    // Filled triangular arrowhead with its tip at `b`. Skip for a degenerate
    // zero-length arrow (T-2-08 guard).
    if len < 1.0 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let head_len = 10.0;
    let head_half = 4.0;
    let bx = b.x - ux * head_len;
    let by = b.y - uy * head_len;
    // Perpendicular to the direction (base of the triangle).
    let (px, py) = (-uy, ux);
    let lx = bx + px * head_half;
    let ly = by + py * head_half;
    let rx = bx - px * head_half;
    let ry = by - py * head_half;
    let mut pb = PathBuilder::new();
    pb.move_to(b.x, b.y);
    pb.line_to(lx, ly);
    pb.line_to(rx, ry);
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

/// A freehand path stroked through the accumulated points.
fn draw_pen(pm: &mut PixmapMut, pts: &[Point]) {
    if pts.is_empty() {
        return; // T-2-08 guard: nothing to draw.
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].x, pts[0].y);
    for p in &pts[1..] {
        pb.line_to(p.x, p.y);
    }
    if let Some(path) = pb.finish() {
        let paint = annotation_paint();
        let stroke = annotation_stroke();
        pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// A text annotation, rasterized at `at` (baseline) in annotation orange.
fn draw_text_annotation(pm: &mut PixmapMut, at: Point, s: &str, size: f32) {
    // T-2-09: cap the string length so oversized input can't produce a
    // malformed layout.
    let truncated: String = s.chars().take(MAX_TEXT_CHARS).collect();
    let font = text::load_font();
    text::draw_text(pm, &font, &truncated, (at.x, at.y), size, annotation_color());
}

/// A retained, undoable list of annotations (CAP-07): undo pops the last entry,
/// and the overlay redraws the whole list every frame (immediate-mode).
#[derive(Default)]
pub struct AnnotationList {
    pub items: Vec<Annotation>,
}

impl AnnotationList {
    /// Append an annotation.
    pub fn push(&mut self, ann: Annotation) {
        self.items.push(ann);
    }

    /// Remove and return the most recent annotation, if any.
    pub fn undo(&mut self) -> Option<Annotation> {
        self.items.pop()
    }

    /// Whether the list is empty (== the original image, CAP-07).
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Iterate the retained annotations in draw order.
    pub fn iter(&self) -> std::slice::Iter<'_, Annotation> {
        self.items.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mybox_core::tiny_skia::Pixmap;

    fn p(x: f32, y: f32) -> Point {
        Point::from_xy(x, y)
    }

    fn pixel(data: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    }

    /// Run `draw` on a fresh transparent pixmap and return its bytes.
    fn render(w: u32, h: u32, ann: &Annotation) -> Vec<u8> {
        let mut pixmap = Pixmap::new(w, h).expect("pixmap");
        pixmap.fill(Color::TRANSPARENT);
        {
            let mut pm = pixmap.as_mut();
            ann.draw(&mut pm);
        }
        pixmap.data().to_vec()
    }

    #[test]
    fn rect_draw_marks_the_border_in_annotation_orange() {
        // A 20x20 rect from (10,10) to (30,30): the top border passes through
        // (20, 10), which the 3px stroke covers at full coverage.
        let ann = Annotation::Rect {
            a: p(10.0, 10.0),
            b: p(30.0, 30.0),
        };
        let data = render(40, 40, &ann);
        // Opaque orange (premultiplied == straight for alpha 255).
        assert_eq!(
            pixel(&data, 40, 20, 10),
            [0xFF, 0x60, 0x00, 0xFF],
            "rect border pixel must be annotation orange"
        );
    }

    #[test]
    fn arrow_draw_marks_the_line_between_a_and_b() {
        let ann = Annotation::Arrow {
            a: p(10.0, 10.0),
            b: p(30.0, 30.0),
        };
        let data = render(40, 40, &ann);
        // The midpoint of the diagonal is on the stroked line.
        assert!(
            pixel(&data, 40, 20, 20)[3] > 0,
            "arrow line must produce a non-transparent pixel at its midpoint"
        );
    }

    #[test]
    fn pen_draw_marks_the_midpoint_of_the_path() {
        let ann = Annotation::Pen {
            pts: vec![p(10.0, 10.0), p(20.0, 20.0), p(30.0, 10.0)],
        };
        let data = render(40, 40, &ann);
        assert!(
            pixel(&data, 40, 20, 20)[3] > 0,
            "pen path must produce a non-transparent pixel at its midpoint"
        );
    }

    #[test]
    fn empty_pen_does_not_panic_or_draw() {
        let ann = Annotation::Pen { pts: vec![] };
        let data = render(40, 40, &ann);
        assert!(
            data.iter().all(|&b| b == 0),
            "an empty pen must not draw any pixel"
        );
    }

    #[test]
    fn zero_size_rect_does_not_panic() {
        let ann = Annotation::Rect {
            a: p(10.0, 10.0),
            b: p(10.0, 10.0),
        };
        // from_ltrb returns None for a zero-size rect — no panic (T-2-08).
        let _ = render(40, 40, &ann);
    }

    #[test]
    fn text_draw_writes_pixels_in_the_text_region() {
        let ann = Annotation::Text {
            at: p(5.0, 20.0),
            s: "A".to_string(),
            size: 18.0,
        };
        let data = render(40, 40, &ann);
        let mut any_alpha = false;
        for y in 0..40u32 {
            for x in 0..40u32 {
                if pixel(&data, 40, x, y)[3] > 0 {
                    any_alpha = true;
                }
            }
        }
        assert!(any_alpha, "text annotation must produce at least one covered pixel");
    }

    #[test]
    fn undo_pops_last_and_reaches_empty() {
        let mut list = AnnotationList::default();
        assert!(list.is_empty());

        list.push(Annotation::Rect {
            a: p(0.0, 0.0),
            b: p(1.0, 1.0),
        });
        list.push(Annotation::Rect {
            a: p(2.0, 2.0),
            b: p(3.0, 3.0),
        });
        list.push(Annotation::Rect {
            a: p(4.0, 4.0),
            b: p(5.0, 5.0),
        });
        assert_eq!(list.items.len(), 3);

        // Undo pops in LIFO order.
        assert_eq!(
            list.undo(),
            Some(Annotation::Rect {
                a: p(4.0, 4.0),
                b: p(5.0, 5.0),
            })
        );
        assert_eq!(list.items.len(), 2);
        assert!(!list.is_empty());

        list.undo();
        list.undo();
        assert!(list.is_empty(), "undo to empty == original image (CAP-07)");
        assert_eq!(list.undo(), None, "undo on empty returns None");
    }
}
