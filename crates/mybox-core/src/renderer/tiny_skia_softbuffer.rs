//! Tiny-skia + softbuffer rendering backend (D-01/D-02/D-03).
//!
//! Pipeline: draw into a `tiny_skia::Pixmap` → convert premultiplied RGBA bytes
//! to softbuffer's `0x00RRGGBB` words (`premul_rgba_to_u32`) → `present()`.
//!
//! Ownership (W5 refinement): softbuffer's macOS backend stores the window
//! handle source (`window_handle: W`) inside the surface, and `Context::new`
//! consumes its `D` value — so the window must be shared as `Arc<winit::Window>`
//! (the canonical softbuffer+winit pattern). `Arc<Window>` implements both
//! `HasDisplayHandle` and `HasWindowHandle` (raw-window-handle 0.6). The
//! concrete surface type is `Surface<Arc<Window>, Arc<Window>>`; the renderer
//! keeps the window alive via its own `Arc` clone.

use std::num::NonZeroU32;
use std::sync::Arc;

use crate::error::{MyboxError, Result};
use crate::renderer::{premul_rgba_to_u32, Renderer};

/// CPU-rendered window backend: a tiny-skia pixmap composited through softbuffer.
///
/// Not `Send`/`Sync` (it holds the main-thread-bound winit window) and must
/// only ever be used on the main thread.
pub struct TinySkiaSoftbufferRenderer {
    surface: softbuffer::Surface<Arc<winit::window::Window>, Arc<winit::window::Window>>,
    pixmap: tiny_skia::Pixmap,
    width: u32,
    height: u32,
}

impl TinySkiaSoftbufferRenderer {
    /// Build a renderer for an already-created winit window (shared via `Arc`
    /// so `WindowState` can hold its own clone of the same window).
    ///
    /// `Context`/`Surface` errors map through `MyboxError::Softbuffer` (01-01-02).
    pub fn new(window: Arc<winit::window::Window>) -> Result<Self> {
        let context = softbuffer::Context::new(Arc::clone(&window))?;
        let surface = softbuffer::Surface::new(&context, Arc::clone(&window))?;
        let size = window.inner_size();
        let (width, height) = (size.width, size.height);
        let pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| MyboxError::Window(format!("could not allocate {width}x{height} pixmap")))?;
        Ok(Self {
            surface,
            pixmap,
            width,
            height,
        })
    }
}

impl Renderer for TinySkiaSoftbufferRenderer {
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        // A hidden/zero-size window has no surface or pixmap to build; keep the
        // previous buffers until a real size arrives.
        if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            let _ = self.surface.resize(w, h);
            if let Some(pixmap) = tiny_skia::Pixmap::new(width, height) {
                self.pixmap = pixmap;
            }
        }
    }

    fn draw(&mut self, f: &mut dyn FnMut(&mut tiny_skia::PixmapMut, u32, u32)) {
        f(&mut self.pixmap.as_mut(), self.width, self.height);
    }

    fn present(&mut self) -> Result<()> {
        let mut buffer = self.surface.buffer_mut()?;
        let (bw, bh) = (buffer.width().get() as usize, buffer.height().get() as usize);
        // T-1-06: copy only as many pixels as the buffer actually has — never
        // assume the pixmap and buffer are the same size.
        let count = bw * bh;
        let px = self.pixmap.data();
        let out = &mut *buffer; // &mut [u32] via DerefMut
        for (i, chunk) in px.chunks_exact(4).take(count).enumerate() {
            let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
            out[i] = premul_rgba_to_u32(r, g, b, a);
        }
        buffer.present()?;
        Ok(())
    }
}
