//! Text rasterization for the selection size label (WxH) and (later) the text
//! annotation tool — `ab_glyph` `OutlinedGlyph::draw` coverage compositing, since
//! tiny-skia has no text module (RESEARCH).
//!
//! The font is the macOS system font (A4: `/System/Library/Fonts/Supplemental/
//! Arial.ttf` — verified present on the dev Mac). Windows font discovery is
//! deferred to Phase 4.

use std::sync::OnceLock;

use ab_glyph::{point, Font, FontArc, FontVec, ScaleFont};
use mybox_core::tiny_skia::PixmapMut;

/// Load the shared text font, caching it behind a `OnceLock` (A4).
///
/// macOS-first: reads the verified-present system Arial.ttf. A missing or
/// unparseable font is a hard error on macOS where it is a documented
/// precondition.
pub fn load_font() -> FontArc {
    static FONT: OnceLock<FontArc> = OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = std::fs::read("/System/Library/Fonts/Supplemental/Arial.ttf")
            .expect("system font Arial.ttf must be present on macOS (A4)");
        let font_vec =
            FontVec::try_from_vec(bytes).expect("Arial.ttf must parse as a TrueType font");
        FontArc::from(font_vec)
    })
    .clone()
}

/// Draw `text` (white, solid) with `at` as the baseline position and `size` in
/// pixels. Coverage from `OutlinedGlyph::draw` is blended as alpha into the
/// pixmap (premultiplied src-over).
pub fn draw_text(pm: &mut PixmapMut, font: &FontArc, text: &str, at: (f32, f32), size: f32) {
    let scaled = font.as_scaled(size);
    let mut pen_x = at.0;
    for ch in text.chars() {
        let mut glyph = scaled.scaled_glyph(ch);
        glyph.position = point(pen_x, 0.0);
        let advance = scaled.h_advance(glyph.id);
        if let Some(og) = font.outline_glyph(glyph) {
            let bounds = og.px_bounds();
            og.draw(|gx, gy, cov| {
                if cov <= 0.0 {
                    return;
                }
                let px = bounds.min.x as i32 + gx as i32;
                let py = (at.1 + bounds.min.y) as i32 + gy as i32;
                blend_white(&mut *pm, px, py, cov);
            });
        }
        pen_x += advance;
    }
}

/// Blend a white pixel of the given coverage (0.0..=1.0) over the pixmap at
/// `(x, y)` using premultiplied src-over. Out-of-bounds pixels are ignored.
fn blend_white(pm: &mut PixmapMut, x: i32, y: i32, cov: f32) {
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let alpha = (cov.clamp(0.0, 1.0) * 255.0).round() as u8;
    if alpha == 0 {
        return;
    }
    let idx = (y as usize * pm.width() as usize + x as usize) * 4;
    let data = pm.data_mut();
    let inv = 255 - u32::from(alpha);
    // src-over, premultiplied white source = (alpha, alpha, alpha, alpha).
    data[idx + 3] = (u32::from(alpha) + u32::from(data[idx + 3]) * inv / 255) as u8;
    data[idx + 0] = (u32::from(alpha) + u32::from(data[idx + 0]) * inv / 255) as u8;
    data[idx + 1] = (u32::from(alpha) + u32::from(data[idx + 1]) * inv / 255) as u8;
    data[idx + 2] = (u32::from(alpha) + u32::from(data[idx + 2]) * inv / 255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use mybox_core::tiny_skia::Pixmap;

    #[test]
    fn draw_text_writes_non_empty_pixels() {
        let font = load_font();
        let mut pixmap = Pixmap::new(40, 20).expect("40x20 pixmap");
        {
            let mut pm = pixmap.as_mut();
            draw_text(&mut pm, &font, "12 × 34", (2.0, 15.0), 16.0);
        }
        let data = pixmap.data();
        let mut any_alpha = false;
        for y in 0..20 {
            for x in 0..40 {
                if data[((y * 40 + x) * 4) + 3] > 0 {
                    any_alpha = true;
                }
            }
        }
        assert!(any_alpha, "draw_text must produce at least one covered pixel");
    }

    #[test]
    fn draw_text_out_of_bounds_does_not_panic() {
        // The label is drawn above the selection top-left; when the selection is
        // at the very top edge, the label may clip outside the pixmap. The blend
        // must silently ignore out-of-bounds pixels (T-2-05 guard).
        let font = load_font();
        let mut pixmap = Pixmap::new(40, 20).expect("40x20 pixmap");
        let mut pm = pixmap.as_mut();
        draw_text(&mut pm, &font, "12 × 34", (-20.0, -20.0), 16.0);
        // No panic = pass.
    }
}
