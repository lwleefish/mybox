//! Screen-capture backend (CAP-01): enumerate and capture every monitor via
//! xcap on a worker thread (Pitfall 4 — never on the event loop, and never
//! inside the draw closure).

use std::sync::Arc;

use mybox_core::anyhow;

/// Physical-pixel geometry of one monitor in global (virtual-screen) origin.
pub struct MonitorGeom {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Injectable capture function.
///
/// `Arc` (not `Box`) so the same capture closure can be cloned into multiple
/// `'static` event-handler closures — `EventBus` handlers must be `Send + Sync`
/// (event.rs). Production uses `Arc::new(capture_all_monitors)`; tests inject a
/// fake `Arc::new(move || ...)`.
pub type CaptureFn =
    Arc<dyn Fn() -> anyhow::Result<Vec<(MonitorGeom, xcap::image::RgbaImage)>> + Send + Sync>;

/// Capture every monitor's current image.
///
/// Geometry is converted to physical pixels: xcap reports `x`/`y` in points, so
/// each is multiplied by `scale_factor()` (RESEARCH Pattern 3 — the only
/// logical-to-physical conversion point). `capture_image()` already returns a
/// pixel-resolution `RgbaImage` (RGBA8 straight alpha).
pub fn capture_all_monitors() -> anyhow::Result<Vec<(MonitorGeom, xcap::image::RgbaImage)>> {
    let mut shots = Vec::new();
    for monitor in xcap::Monitor::all()? {
        let scale = monitor.scale_factor()?;
        let img = monitor.capture_image()?;
        let geom = MonitorGeom {
            x: (f64::from(monitor.x()?) * f64::from(scale)).round() as i32,
            y: (f64::from(monitor.y()?) * f64::from(scale)).round() as i32,
            width: img.width(),
            height: img.height(),
        };
        shots.push((geom, img));
    }
    Ok(shots)
}
