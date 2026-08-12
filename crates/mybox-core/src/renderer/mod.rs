//! Renderer abstraction (D-01/D-02/D-03).
//!
//! Modules see only the [`Renderer`] trait; core owns the tiny-skia + softbuffer
//! backend. The concrete `TinySkiaSoftbufferRenderer` lands in plan 01-02, and
//! the pure pixel-conversion function `premul_rgba_to_u32` lands in plan 01-01-06.

/// Per-window compositing abstraction (D-03). The `draw` closure isolates
/// content generation from compositing, leaving a slot for the egui layer in
/// Phase 3 (RESEARCH §5).
pub trait Renderer: Send {
    /// Resize the backing pixmap / softbuffer surface.
    fn resize(&mut self, width: u32, height: u32);

    /// Draw custom content: the closure receives the tiny-skia pixmap and its
    /// size.
    fn draw(&mut self, f: &mut dyn FnMut(&mut tiny_skia::PixmapMut, u32, u32));

    /// Present the composited pixmap to the window (softbuffer).
    fn present(&mut self) -> crate::error::Result<()>;
}
