//! egui tessellate → tiny-skia rasterizer (RESEARCH Pattern 1) — the only
//! hand-rolled piece of the palette, forced: no maintained crate exists for
//! egui 0.30 + tiny-skia 0.12.
//!
//! Contracts honored:
//! - egui `Vertex.color` is sRGBA with **premultiplied** alpha (epaint source);
//!   tiny-skia `Color` is constructed from straight RGBA and premultiplies
//!   internally — the solid path un-premultiplies the vertex color first
//!   (an exact round-trip within 8-bit rounding). Pixmap pixels are
//!   premultiplied RGBA8; the textured path writes premultiplied bytes
//!   directly (Phase 2 Pitfall 2 discipline).
//! - The font atlas is `TextureId::Managed(0)` as `ImageData::Font`
//!   (straight RGBA8); the palette uses no other textures.
//! - egui tessellates in logical points; the framebuffer is physical pixels —
//!   `pixels_per_point` scales positions (retina correctness).

use std::collections::HashMap;

use mybox_core::egui;
use mybox_core::tiny_skia;

/// A texture converted to a straight-RGBA8 byte buffer. Built once per
/// `paint` call; `FontImage` stores single-channel coverage data, which
/// `srgba_pixels` converts on demand.
struct TexturePixels {
    size: [usize; 2],
    /// Straight RGBA8, row-major.
    data: Vec<u8>,
}

fn texture_pixels(image: &egui::epaint::ImageData) -> TexturePixels {
    match image {
        egui::epaint::ImageData::Color(c) => {
            let mut data = Vec::with_capacity(c.pixels.len() * 4);
            for p in &c.pixels {
                data.extend_from_slice(&[p.r(), p.g(), p.b(), p.a()]);
            }
            TexturePixels { size: c.size, data }
        }
        egui::epaint::ImageData::Font(f) => {
            let mut data = Vec::with_capacity(f.size[0] * f.size[1] * 4);
            for p in f.srgba_pixels(None) {
                data.extend_from_slice(&[p.r(), p.g(), p.b(), p.a()]);
            }
            TexturePixels { size: f.size, data }
        }
    }
}

/// Paint tessellated egui primitives into a tiny-skia framebuffer.
///
/// `pixels_per_point` maps egui point coordinates to the framebuffer's
/// physical pixels (window scale factor).
pub fn paint(
    framebuffer: &mut tiny_skia::Pixmap,
    primitives: &[egui::ClippedPrimitive],
    textures: &HashMap<egui::TextureId, egui::epaint::ImageData>,
    pixels_per_point: f32,
) {
    // Convert the texture table once per frame (FontImage coverage → RGBA8).
    let texture_buffers: HashMap<egui::TextureId, TexturePixels> =
        textures.iter().map(|(id, img)| (*id, texture_pixels(img))).collect();

    for clipped in primitives {
        let egui::ClippedPrimitive {
            clip_rect,
            primitive,
        } = clipped;
        let egui::epaint::Primitive::Mesh(mesh) = primitive else {
            // The palette emits no Callback primitives.
            continue;
        };
        let clip_rect = *clip_rect;

        // Clip pixmap in physical pixels, clamped to the framebuffer.
        let min_x = (clip_rect.min.x * pixels_per_point).round() as u32;
        let min_y = (clip_rect.min.y * pixels_per_point).round() as u32;
        let max_x = (clip_rect.max.x * pixels_per_point).round() as u32;
        let max_y = (clip_rect.max.y * pixels_per_point).round() as u32;
        let max_x = max_x.min(framebuffer.width());
        let max_y = max_y.min(framebuffer.height());
        if max_x <= min_x || max_y <= min_y {
            continue;
        }
        let w = max_x - min_x;
        let h = max_y - min_y;
        let mut clip_pixmap = match tiny_skia::Pixmap::new(w, h) {
            Some(p) => p,
            None => continue,
        };

        let origin = clip_rect.min; // egui points
        for tri in mesh.indices.chunks_exact(3) {
            let v0 = &mesh.vertices[tri[0] as usize];
            let v1 = &mesh.vertices[tri[1] as usize];
            let v2 = &mesh.vertices[tri[2] as usize];
            if v0.color == v1.color && v1.color == v2.color {
                paint_solid_triangle(
                    &mut clip_pixmap,
                    v0,
                    v1,
                    v2,
                    origin,
                    pixels_per_point,
                );
            } else {
                paint_textured_triangle(
                    &mut clip_pixmap,
                    mesh,
                    v0,
                    v1,
                    v2,
                    origin,
                    pixels_per_point,
                    &texture_buffers,
                );
            }
        }

        // Composite the clip pixmap onto the framebuffer.
        framebuffer.draw_pixmap(
            min_x as i32,
            min_y as i32,
            clip_pixmap.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }
}

/// Convert an egui premultiplied `Color32` to a tiny-skia `Color`. tiny-skia
/// has no premultiplied constructor — un-premultiply to straight bytes and
/// let `from_rgba8` premultiply back (exact round-trip within rounding).
fn to_tiny_color(color: egui::Color32) -> tiny_skia::Color {
    let a = color.a();
    if a == 0 {
        return tiny_skia::Color::TRANSPARENT;
    }
    if a == 255 {
        return tiny_skia::Color::from_rgba8(color.r(), color.g(), color.b(), 255);
    }
    let un = |c: u8| (u32::from(c) * 255 / u32::from(a)).min(255) as u8;
    tiny_skia::Color::from_rgba8(un(color.r()), un(color.g()), un(color.b()), a)
}

/// Barycentric weights of `p` in triangle (a, b, c); `None` when degenerate.
fn barycentric(
    p: egui::Pos2,
    a: egui::Pos2,
    b: egui::Pos2,
    c: egui::Pos2,
) -> Option<(f32, f32, f32)> {
    let v0 = b - a;
    let v1 = c - a;
    let v2 = p - a;
    let d00 = v0.x * v0.x + v0.y * v0.y;
    let d01 = v0.x * v1.x + v0.y * v1.y;
    let d11 = v1.x * v1.x + v1.y * v1.y;
    let d20 = v2.x * v0.x + v2.y * v0.y;
    let d21 = v2.x * v1.x + v2.y * v1.y;
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-9 {
        return None;
    }
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;
    Some((u, v, w))
}

/// Solid fast path: a single-color triangle via `fill_path` (anti-aliased).
/// Coordinates are egui points; the `pixels_per_point` transform maps them to
/// the physical clip pixmap.
fn paint_solid_triangle(
    clip_pixmap: &mut tiny_skia::Pixmap,
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    origin: egui::Pos2,
    pixels_per_point: f32,
) {
    let mut path = tiny_skia::PathBuilder::new();
    path.move_to(v0.pos.x - origin.x, v0.pos.y - origin.y);
    path.line_to(v1.pos.x - origin.x, v1.pos.y - origin.y);
    path.line_to(v2.pos.x - origin.x, v2.pos.y - origin.y);
    path.close();
    let Some(path) = path.finish() else {
        return;
    };
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(to_tiny_color(v0.color));
    paint.anti_alias = true;
    let transform = if pixels_per_point == 1.0 {
        tiny_skia::Transform::identity()
    } else {
        tiny_skia::Transform::from_scale(pixels_per_point, pixels_per_point)
    };
    clip_pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        transform,
        None,
    );
}

/// Textured path: per-pixel barycentric sampling with bilinear UV fetch
/// (font atlas glyphs). Vertex colors are premultiplied; the texture buffer
/// is straight RGBA8. Both are multiplied in premultiplied space, matching
/// egui's shader semantics, and written as premultiplied bytes.
#[allow(clippy::too_many_arguments)]
fn paint_textured_triangle(
    clip_pixmap: &mut tiny_skia::Pixmap,
    mesh: &egui::epaint::Mesh,
    v0: &egui::epaint::Vertex,
    v1: &egui::epaint::Vertex,
    v2: &egui::epaint::Vertex,
    origin: egui::Pos2,
    pixels_per_point: f32,
    textures: &HashMap<egui::TextureId, TexturePixels>,
) {
    let Some(image) = textures.get(&mesh.texture_id) else {
        return;
    };
    let tex_w = image.size[0];
    let tex_h = image.size[1];
    if tex_w == 0 || tex_h == 0 {
        return;
    }

    let w = clip_pixmap.width() as usize;
    let h = clip_pixmap.height() as usize;
    let inv_ppp = 1.0 / pixels_per_point;

    for py in 0..h {
        for px in 0..w {
            // Physical pixel center → egui point.
            let p = egui::pos2(
                origin.x + (px as f32 + 0.5) * inv_ppp,
                origin.y + (py as f32 + 0.5) * inv_ppp,
            );
            let Some((u, v, wgt)) = barycentric(p, v0.pos, v1.pos, v2.pos) else {
                continue;
            };
            const EPS: f32 = -1e-3;
            if u < EPS || v < EPS || wgt < EPS {
                continue;
            }
            // Interpolated premultiplied vertex color (egui shader semantics).
            let r = u * f32::from(v0.color.r()) + v * f32::from(v1.color.r()) + wgt * f32::from(v2.color.r());
            let g = u * f32::from(v0.color.g()) + v * f32::from(v1.color.g()) + wgt * f32::from(v2.color.g());
            let b = u * f32::from(v0.color.b()) + v * f32::from(v1.color.b()) + wgt * f32::from(v2.color.b());
            let a = u * f32::from(v0.color.a()) + v * f32::from(v1.color.a()) + wgt * f32::from(v2.color.a());
            // Interpolated normalized UV → texel coordinates.
            let tu = (u * v0.uv.x + v * v1.uv.x + wgt * v2.uv.x) * tex_w as f32;
            let tv = (u * v0.uv.y + v * v1.uv.y + wgt * v2.uv.y) * tex_h as f32;
            let tex = sample_bilinear(image, tu, tv);

            // out_premul = tex_straight_premul * vert_premul.
            let sa = tex[3] * a / 255.0; // tex_a * vert_a
            let out_a = sa.clamp(0.0, 255.0);
            let out_r = (tex[0] * tex[3] / 255.0 * r / 255.0).clamp(0.0, 255.0);
            let out_g = (tex[1] * tex[3] / 255.0 * g / 255.0).clamp(0.0, 255.0);
            let out_b = (tex[2] * tex[3] / 255.0 * b / 255.0).clamp(0.0, 255.0);
            // Round to nearest (f32 barycentric interpolation sums are off by
            // ~1e-5; truncation would darken every pixel by one step).
            let src = [
                out_r.round() as u8,
                out_g.round() as u8,
                out_b.round() as u8,
                out_a.round() as u8,
            ];

            // Premultiplied over-blend onto the clip pixmap (glyph triangles
            // can overlap at the atlas edges).
            let idx = (py * w + px) * 4;
            let inv = 255 - src[3];
            let blend = {
                let dst = &clip_pixmap.data_mut()[idx..idx + 4];
                [
                    u16::from(src[0]) + (u16::from(dst[0]) * u16::from(inv) + 127) / 255,
                    u16::from(src[1]) + (u16::from(dst[1]) * u16::from(inv) + 127) / 255,
                    u16::from(src[2]) + (u16::from(dst[2]) * u16::from(inv) + 127) / 255,
                    u16::from(src[3]) + (u16::from(dst[3]) * u16::from(inv) + 127) / 255,
                ]
            };
            let dst = &mut clip_pixmap.data_mut()[idx..idx + 4];
            dst.copy_from_slice(&[
                blend[0] as u8,
                blend[1] as u8,
                blend[2] as u8,
                blend[3] as u8,
            ]);
        }
    }
}

/// Bilinear sample with edge clamping. Returns straight RGBA in 0..255.
fn sample_bilinear(image: &TexturePixels, u: f32, v: f32) -> [f32; 4] {
    let w = image.size[0];
    let h = image.size[1];
    let x = u.clamp(0.0, w as f32 - 1.0);
    let y = v.clamp(0.0, h as f32 - 1.0);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let at = |xx: usize, yy: usize| -> [f32; 4] {
        let i = (yy * w + xx) * 4;
        [
            f32::from(image.data[i]),
            f32::from(image.data[i + 1]),
            f32::from(image.data[i + 2]),
            f32::from(image.data[i + 3]),
        ]
    };
    let p00 = at(x0, y0);
    let p10 = at(x1, y0);
    let p01 = at(x0, y1);
    let p11 = at(x1, y1);
    let mut out = [0.0; 4];
    for i in 0..4 {
        let top = p00[i] * (1.0 - fx) + p10[i] * fx;
        let bottom = p01[i] * (1.0 - fx) + p11[i] * fx;
        out[i] = top * (1.0 - fy) + bottom * fy;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts;
    use std::sync::Arc;

    /// Count non-transparent pixels (proves painting happened).
    fn non_transparent(pixmap: &tiny_skia::Pixmap) -> usize {
        pixmap.data().chunks_exact(4).filter(|p| p[3] != 0).count()
    }

    #[test]
    fn paint_renders_chinese_label() {
        // Pitfall 5 headless detection: run a frame with a Chinese label,
        // tessellate, paint — non-background pixels must exist. On macOS the
        // CJK font is installed first so real glyphs render.
        let ctx = egui::Context::default();
        let _ = fonts::install_cjk_fonts(&ctx);
        let full = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    ui.label("截图");
                });
        });
        let mut textures = HashMap::new();
        for (id, change) in full.textures_delta.set {
            textures.insert(id, change.image);
        }
        let prims = ctx.tessellate(full.shapes, full.pixels_per_point);

        let mut framebuffer = tiny_skia::Pixmap::new(600, 128).expect("600x128 pixmap");
        paint(&mut framebuffer, &prims, &textures, full.pixels_per_point);

        assert!(
            non_transparent(&framebuffer) > 0,
            "Chinese label must produce visible pixels"
        );
    }

    #[test]
    fn solid_fast_path_matches_barycentric() {
        // A single-color triangle painted through both paths must agree on
        // every fully-covered (interior) pixel — the straight/premultiplied
        // conversion round-trip contract. Edge pixels differ (fill_path
        // anti-aliases, barycentric is hard-edged) and are excluded.
        let color = egui::Color32::from_rgba_premultiplied(0x80, 0x40, 0x20, 0xC0);
        let v0 = egui::epaint::Vertex {
            pos: egui::pos2(10.0, 10.0),
            uv: egui::pos2(0.0, 0.0),
            color,
        };
        let v1 = egui::epaint::Vertex {
            pos: egui::pos2(120.0, 10.0),
            uv: egui::pos2(1.0, 0.0),
            color,
        };
        let v2 = egui::epaint::Vertex {
            pos: egui::pos2(65.0, 90.0),
            uv: egui::pos2(0.5, 1.0),
            color,
        };

        // Solid path.
        let mut solid = tiny_skia::Pixmap::new(128, 100).expect("pixmap");
        paint_solid_triangle(&mut solid, &v0, &v1, &v2, egui::Pos2::ZERO, 1.0);

        // Textured path over a 1x1 white opaque texture.
        let mut textured = tiny_skia::Pixmap::new(128, 100).expect("pixmap");
        let mesh = egui::epaint::Mesh {
            indices: vec![0, 1, 2],
            vertices: vec![v0, v1, v2],
            texture_id: egui::TextureId::Managed(0),
            ..Default::default()
        };
        let mut textures = HashMap::new();
        textures.insert(
            egui::TextureId::Managed(0),
            egui::epaint::ImageData::Color(Arc::new(egui::epaint::ColorImage {
                size: [1, 1],
                pixels: vec![egui::Color32::WHITE],
            })),
        );
        let buffers: HashMap<_, _> = textures
            .iter()
            .map(|(id, img)| (*id, texture_pixels(img)))
            .collect();
        paint_textured_triangle(&mut textured, &mesh, &v0, &v1, &v2, egui::Pos2::ZERO, 1.0, &buffers);

        // Compare interior pixels (fully covered in both paths). The triangle
        // color is semi-transparent, so "fully covered" = alpha == 0xC0;
        // anti-aliased edge pixels (alpha < 0xC0) are excluded.
        let mut mismatches = 0;
        let mut compared = 0;
        for (a, b) in solid.data().chunks_exact(4).zip(textured.data().chunks_exact(4)) {
            if a[3] == 0xC0 && b[3] == 0xC0 {
                compared += 1;
                if a != b {
                    mismatches += 1;
                }
            }
        }
        assert!(compared > 100, "triangle must have a meaningful interior");
        assert_eq!(mismatches, 0, "interior pixels must match between paths");
    }

    #[test]
    fn textured_path_multiplies_white_texture_with_vertex_color() {
        // A white texture × vertex color must reproduce the vertex color
        // (premultiplied) at the interior centroid.
        let color = egui::Color32::from_rgba_premultiplied(0xFF, 0x80, 0x40, 0xFF); // opaque
        let v0 = egui::epaint::Vertex {
            pos: egui::pos2(0.0, 0.0),
            uv: egui::pos2(0.0, 0.0),
            color,
        };
        let v1 = egui::epaint::Vertex {
            pos: egui::pos2(100.0, 0.0),
            uv: egui::pos2(1.0, 0.0),
            color,
        };
        let v2 = egui::epaint::Vertex {
            pos: egui::pos2(0.0, 100.0),
            uv: egui::pos2(0.0, 1.0),
            color,
        };
        let mesh = egui::epaint::Mesh {
            indices: vec![0, 1, 2],
            vertices: vec![v0, v1, v2],
            texture_id: egui::TextureId::Managed(0),
            ..Default::default()
        };
        let mut textures = HashMap::new();
        textures.insert(
            egui::TextureId::Managed(0),
            egui::epaint::ImageData::Color(Arc::new(egui::epaint::ColorImage {
                size: [1, 1],
                pixels: vec![egui::Color32::WHITE],
            })),
        );
        let buffers: HashMap<_, _> = textures
            .iter()
            .map(|(id, img)| (*id, texture_pixels(img)))
            .collect();
        let mut pixmap = tiny_skia::Pixmap::new(100, 100).expect("pixmap");
        paint_textured_triangle(&mut pixmap, &mesh, &v0, &v1, &v2, egui::Pos2::ZERO, 1.0, &buffers);

        // Pixel (33, 33) is inside the triangle (hypotenuse x+y=100: 33+33<100).
        let centroid = (33 * 100 + 33) * 4;
        let p = &pixmap.data()[centroid..centroid + 4];
        // Opaque white × (FF, 80, 40, FF) = (FF, 80, 40, FF).
        assert_eq!([p[0], p[1], p[2], p[3]], [0xFF, 0x80, 0x40, 0xFF]);
    }

    #[test]
    fn solid_path_scales_with_pixels_per_point() {
        // Retina: the same point-space triangle at ppp=2 must cover ~4x the
        // physical pixels.
        let v0 = egui::epaint::Vertex {
            pos: egui::pos2(10.0, 10.0),
            uv: egui::pos2(0.0, 0.0),
            color: egui::Color32::WHITE,
        };
        let v1 = egui::epaint::Vertex {
            pos: egui::pos2(40.0, 10.0),
            uv: egui::pos2(0.0, 0.0),
            color: egui::Color32::WHITE,
        };
        let v2 = egui::epaint::Vertex {
            pos: egui::pos2(25.0, 40.0),
            uv: egui::pos2(0.0, 0.0),
            color: egui::Color32::WHITE,
        };
        let mut px1 = tiny_skia::Pixmap::new(80, 80).expect("pixmap");
        let mut px2 = tiny_skia::Pixmap::new(160, 160).expect("pixmap");
        paint_solid_triangle(&mut px1, &v0, &v1, &v2, egui::Pos2::ZERO, 1.0);
        paint_solid_triangle(&mut px2, &v0, &v1, &v2, egui::Pos2::ZERO, 2.0);
        let n1 = non_transparent(&px1);
        let n2 = non_transparent(&px2);
        assert!(n1 > 0);
        assert!(
            n2 > n1 * 3,
            "ppp=2 must cover roughly 4x the pixels (n1={n1}, n2={n2})"
        );
    }
}
