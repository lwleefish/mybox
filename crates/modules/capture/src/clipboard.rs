//! Clipboard copy (CAP-04): crop the selected region, bake retained annotations
//! into it, and hand the final RGBA8-straight bytes to `arboard` on the main
//! thread in a confined scope (Pitfall 6: Windows clipboard thread-affinity +
//! drop-before-loop).

use mybox_core::anyhow;
use mybox_core::tiny_skia::{IntSize, Pixmap, Point};

use crate::overlay::premultiply_rgba8;
use crate::session::Annotation;

#[cfg(target_os = "macos")]
use arboard::SetExtApple;

/// Crop an axis-aligned sub-rectangle from an `RgbaImage` (RGBA8 straight),
/// returning the cropped bytes via a manual row copy (no format conversion —
/// straight RGBA8 matches arboard `ImageData` exactly).
///
/// `x0`/`y0`/`w`/`h` are clamped to the image bounds (T-2-15) so a selection
/// that overhangs the captured monitor can never read out of bounds.
pub fn crop_image(img: &xcap::image::RgbaImage, x0: u32, y0: u32, w: u32, h: u32) -> Vec<u8> {
    let (iw, ih) = (img.width(), img.height());
    let x0 = x0.min(iw);
    let y0 = y0.min(ih);
    let w = w.min(iw.saturating_sub(x0));
    let h = h.min(ih.saturating_sub(y0));

    let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
    let src = img.as_raw();
    for row in 0..h {
        let start = ((y0 + row) * iw + x0) as usize * 4;
        let end = start + (w as usize) * 4;
        out.extend_from_slice(&src[start..end]);
    }
    out
}

/// Bake the retained annotations into the cropped selection bytes, returning
/// straight-alpha RGBA8 for the clipboard.
///
/// `cropped` is straight-alpha RGBA8 (from [`crop_image`]); annotations are in
/// the *monitor's* pixel coordinates, so each is translated by `-origin` (the
/// crop's top-left in monitor pixels) before drawing. The crop is premultiplied
/// into a tiny-skia `Pixmap`, every annotation is drawn on top (immediate-mode,
/// same discipline as the overlay), and the result is unpremultiplied back to
/// straight RGBA8.
///
/// When `annotations` is empty the original crop bytes are returned untouched
/// (D-01: confirming with no annotations copies the raw selection).
pub fn bake_annotations(
    cropped: &[u8],
    w: u32,
    h: u32,
    annotations: &[Annotation],
    origin: Point,
) -> Vec<u8> {
    if annotations.is_empty() {
        return cropped.to_vec(); // D-01: no annotations == original image
    }

    let premul = premultiply_rgba8(cropped);
    let Some(size) = IntSize::from_wh(w, h) else {
        return cropped.to_vec();
    };
    let Some(mut pixmap) = Pixmap::from_vec(premul, size) else {
        return cropped.to_vec();
    };

    let dx = -origin.x;
    let dy = -origin.y;
    let mut pm = pixmap.as_mut();
    for ann in annotations {
        translate_annotation(ann, dx, dy).draw(&mut pm);
    }

    unpremultiply_rgba8(pixmap.data())
}

/// Write RGBA8-straight bytes to the system clipboard (CAP-04).
///
/// The `Clipboard` is created, used, and dropped inside this single confined
/// scope (Pitfall 6: a long-lived clipboard must be dropped before app exit and
/// is thread-affine on Windows). On macOS the image is excluded from clipboard
/// history (`org.nspasteboard.ConcealedType`, T-2-13) via arboard's
/// `exclude_from_history` extension.
pub fn copy_to_clipboard(bytes: &[u8], w: usize, h: usize) -> anyhow::Result<()> {
    let data = arboard::ImageData {
        width: w,
        height: h,
        bytes: std::borrow::Cow::Owned(bytes.to_vec()),
    };
    {
        let mut cb = arboard::Clipboard::new()
            .map_err(|e| anyhow::anyhow!("failed to open clipboard: {e}"))?;
        #[cfg(target_os = "macos")]
        {
            cb.set()
                .exclude_from_history()
                .image(data)
                .map_err(|e| anyhow::anyhow!("failed to write clipboard image: {e}"))?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            cb.set_image(data)
                .map_err(|e| anyhow::anyhow!("failed to write clipboard image: {e}"))?;
        }
    } // `cb` dropped here — confined scope (Pitfall 6)
    Ok(())
}

/// Translate an annotation by `(dx, dy)` so it can be drawn into a cropped
/// pixmap whose origin differs from the monitor origin.
fn translate_annotation(ann: &Annotation, dx: f32, dy: f32) -> Annotation {
    fn shift(p: Point, dx: f32, dy: f32) -> Point {
        Point::from_xy(p.x + dx, p.y + dy)
    }
    match ann {
        Annotation::Rect { a, b } => Annotation::Rect {
            a: shift(*a, dx, dy),
            b: shift(*b, dx, dy),
        },
        Annotation::Arrow { a, b } => Annotation::Arrow {
            a: shift(*a, dx, dy),
            b: shift(*b, dx, dy),
        },
        Annotation::Pen { pts } => Annotation::Pen {
            pts: pts.iter().map(|p| shift(*p, dx, dy)).collect(),
        },
        Annotation::Text { at, s, size } => Annotation::Text {
            at: shift(*at, dx, dy),
            s: s.clone(),
            size: *size,
        },
    }
}

/// Convert premultiplied RGBA8 back to straight RGBA8 (Pitfall 2 inverse):
/// `r = min(255, r * 255 / a)` when `a > 0`; fully-transparent pixels collapse
/// to zero.
fn unpremultiply_rgba8(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            out.push(((u32::from(r) * 255 / u32::from(a)).min(255)) as u8);
            out.push(((u32::from(g) * 255 / u32::from(a)).min(255)) as u8);
            out.push(((u32::from(b) * 255 / u32::from(a)).min(255)) as u8);
            out.push(a);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill_2x2() -> xcap::image::RgbaImage {
        let mut img = xcap::image::RgbaImage::new(2, 2);
        // Distinct straight-alpha colors per pixel: (r,g,b,a)
        let px = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 255, 255],
        ];
        for (i, p) in img.pixels_mut().enumerate() {
            *p = xcap::image::Rgba(px[i]);
        }
        img
    }

    #[test]
    fn crop_image_returns_exact_subrect_bytes() {
        let img = fill_2x2();
        // Crop the bottom-right 1x1 pixel: white (255,255,255,255).
        let bytes = crop_image(&img, 1, 1, 1, 1);
        assert_eq!(bytes.len(), 4);
        assert_eq!(bytes, vec![255, 255, 255, 255]);
    }

    #[test]
    fn crop_image_clamps_out_of_bounds_region() {
        let img = fill_2x2();
        // A selection that overhangs the right/bottom edges is clamped to the
        // image bounds (T-2-15) and never reads out of range.
        let bytes = crop_image(&img, 1, 1, 100, 100);
        assert_eq!(bytes.len(), 4, "clamped to the 1x1 bottom-right pixel");
        assert_eq!(bytes, vec![255, 255, 255, 255]);
    }

    #[test]
    fn bake_annotations_empty_returns_original_crop() {
        let img = fill_2x2();
        let cropped = crop_image(&img, 0, 0, 2, 2);
        let baked = bake_annotations(&cropped, 2, 2, &[], Point::from_xy(0.0, 0.0));
        assert_eq!(baked, cropped, "no annotations == original image (D-01)");
    }

    #[test]
    fn bake_annotations_draws_rect_in_annotation_orange() {
        // A 4x4 opaque-white crop; bake a rect whose border covers the center.
        let cropped = vec![255u8; 4 * 4 * 4];
        let rect = Annotation::Rect {
            a: Point::from_xy(1.0, 1.0),
            b: Point::from_xy(3.0, 3.0),
        };
        let baked = bake_annotations(&cropped, 4, 4, &[rect], Point::from_xy(0.0, 0.0));

        // Scan for a pixel in annotation orange (straight == premultiplied at a=255).
        let mut found_orange = false;
        for px in baked.chunks_exact(4) {
            if px == [0xFF, 0x60, 0x00, 0xFF] {
                found_orange = true;
            }
        }
        assert!(found_orange, "a baked rect must produce annotation-orange pixels");
    }

    #[test]
    fn bake_annotations_translates_to_crop_origin() {
        // Annotation is in MONITOR coords (origin at 0); the crop starts at (2,2)
        // of the monitor, so an absolute rect (2,2)-(3,3) must land at crop-local
        // (0,0)-(1,1) — i.e. it must NOT be drawn as if at (2,2) in the crop.
        let cropped = vec![255u8; 4 * 4 * 4];
        let rect = Annotation::Rect {
            a: Point::from_xy(2.0, 2.0),
            b: Point::from_xy(3.0, 3.0),
        };
        let baked = bake_annotations(&cropped, 4, 4, &[rect], Point::from_xy(2.0, 2.0));

        // The translated rect border passes through crop-local (0,0) and (1,1),
        // so the top-left crop pixel should carry the stroke, and the far
        // corner (3,3) should remain the untouched white crop.
        let top_left_orange = &baked[0..4] == [0xFF, 0x60, 0x00, 0xFF];
        let bottom_right_white = &baked[((3 * 4 + 3) * 4)..((3 * 4 + 3) * 4) + 4] == [255, 255, 255, 255];
        assert!(top_left_orange, "translated rect must cover crop-local origin");
        assert!(bottom_right_white, "region outside the translated rect stays original");
    }

    #[test]
    fn unpremultiply_round_trips_opaque_and_transparent() {
        assert_eq!(unpremultiply_rgba8(&[255, 255, 255, 0]), vec![0, 0, 0, 0]);
        assert_eq!(unpremultiply_rgba8(&[255, 0, 0, 255]), vec![255, 0, 0, 255]);
    }
}
