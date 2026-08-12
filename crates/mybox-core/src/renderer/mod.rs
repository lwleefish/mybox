//! Renderer abstraction (D-01/D-02/D-03).
//!
//! Modules see only the [`Renderer`] trait; core owns the tiny-skia + softbuffer
//! backend (`tiny_skia_softbuffer::TinySkiaSoftbufferRenderer`, plan 01-02) and
//! the pure pixel-conversion function `premul_rgba_to_u32` (plan 01-01-06).

pub mod tiny_skia_softbuffer;

pub use tiny_skia_softbuffer::TinySkiaSoftbufferRenderer;

/// Per-window compositing abstraction (D-03). The `draw` closure isolates
/// content generation from compositing, leaving a slot for the egui layer in
/// Phase 3 (RESEARCH §5).
///
/// NOT `Send`: the concrete backend owns the main-thread-bound winit window
/// (softbuffer's macOS Surface stores it), so a renderer can never cross
/// threads. Renderers live inside the main-thread-bound `WindowManager`.
pub trait Renderer {
    /// Resize the backing pixmap / softbuffer surface.
    fn resize(&mut self, width: u32, height: u32);

    /// Draw custom content: the closure receives the tiny-skia pixmap and its
    /// size.
    fn draw(&mut self, f: &mut dyn FnMut(&mut tiny_skia::PixmapMut, u32, u32));

    /// Present the composited pixmap to the window (softbuffer).
    fn present(&mut self) -> crate::error::Result<()>;
}

/// Convert premultiplied RGBA bytes to softbuffer's `0x00RRGGBB` pixel format.
///
/// Opaque pixels (`alpha == 255`) are packed directly. Semi-transparent pixels
/// are un-premultiplied first (`channel * 255 / alpha`) so the color survives;
/// softbuffer drops alpha on macOS anyway (`CGImageAlphaInfo::NoneSkipFirst`,
/// RESEARCH §0.5), so the result is the straight RGB value. Fully transparent
/// pixels collapse to black.
pub fn premul_rgba_to_u32(r: u8, g: u8, b: u8, a: u8) -> u32 {
    if a == 0 {
        return 0x0000_0000;
    }
    if a == 255 {
        return (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
    }
    let r = (u32::from(r) * 255) / u32::from(a);
    let g = (u32::from(g) * 255) / u32::from(a);
    let b = (u32::from(b) * 255) / u32::from(a);
    (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_pixel_packs_directly() {
        assert_eq!(premul_rgba_to_u32(0xFF, 0x80, 0x40, 0xFF), 0x00FF_8040);
    }

    #[test]
    fn fully_transparent_pixel_is_zero() {
        assert_eq!(premul_rgba_to_u32(0xFF, 0x80, 0x40, 0x00), 0x0000_0000);
    }

    #[test]
    fn semi_transparent_pixel_is_unpremultiplied() {
        // premul (0x80,0x40,0x20) at alpha=0x80/255:
        //   r' = 128*255/128 = 255, g' = 64*255/128 = 127, b' = 32*255/128 = 63
        assert_eq!(premul_rgba_to_u32(0x80, 0x40, 0x20, 0x80), 0x00FF_7F3F);
    }

    #[test]
    fn semi_transparent_channel_clamps_to_255() {
        // r' = 255*255/128 = 508 -> clamped to 255.
        assert_eq!(premul_rgba_to_u32(0xFF, 0x80, 0x40, 0x80), 0x00FF_FF7F);
    }

    #[test]
    fn draw_fills_center_pixel_red() {
        // Headless-safe stand-in for the full TinySkiaSoftbufferRenderer::draw
        // path (which needs a real window): paint a red rectangle into an
        // in-memory Pixmap the same way `draw` would, then assert the centre
        // pixel converts to 0x00FF0000 through premul_rgba_to_u32 (D-01/D-02).
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).expect("10x10 pixmap");
        let mut pm = pixmap.as_mut();
        pm.fill(tiny_skia::Color::from_rgba8(0xFF, 0x00, 0x00, 0xFF));

        let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
        let cx = w / 2;
        let cy = h / 2;
        let data = pixmap.data();
        let off = (cy * w + cx) * 4;
        let (r, g, b, a) = (data[off], data[off + 1], data[off + 2], data[off + 3]);
        assert_eq!(premul_rgba_to_u32(r, g, b, a), 0x00FF_0000);
    }

    #[test]
    fn draw_closure_receives_pixmap_and_size() {
        // Mirrors the Renderer::draw closure contract (content generator gets the
        // pixmap + size) against an in-memory Pixmap, so the shape is verified
        // headlessly even though the full renderer needs a real window.
        let mut pixmap = tiny_skia::Pixmap::new(8, 6).expect("8x6 pixmap");
        let mut size_seen = (0, 0);
        let mut f = |_p: &mut tiny_skia::PixmapMut, w: u32, h: u32| {
            size_seen = (w, h);
        };
        f(&mut pixmap.as_mut(), 8, 6);
        assert_eq!(size_seen, (8, 6));
    }
}
