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

/// Logical→physical point conversion (RESEARCH Pattern 3 — the ONLY
/// logical-to-physical conversion point). Round-half-away: the same
/// `.round()` semantics as the pre-extraction code.
pub fn point_to_physical(value: f64, scale: f64) -> i32 {
    (value * scale).round() as i32
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
            x: point_to_physical(f64::from(monitor.x()?), f64::from(scale)),
            y: point_to_physical(f64::from(monitor.y()?), f64::from(scale)),
            width: img.width(),
            height: img.height(),
        };
        shots.push((geom, img));
    }
    Ok(shots)
}

#[cfg(test)]
mod tests {
    use super::point_to_physical;

    /// The four scales that cover the DPI-relevant range: 1.0 (no scaling),
    /// 1.25/1.5 (Windows 125%/150% — the 150% case of success criterion 4),
    /// 2.0 (macOS Retina / Windows 200%).
    #[test]
    fn four_scales_convert_logical_to_physical() {
        assert_eq!(point_to_physical(100.0, 1.0), 100);
        assert_eq!(point_to_physical(100.0, 1.25), 125);
        assert_eq!(point_to_physical(100.0, 1.5), 150);
        assert_eq!(point_to_physical(100.0, 2.0), 200);
    }

    /// Round-half-away semantics: the pre-extraction code used `.round()`, so
    /// the extracted function must keep the same behavior for fractional
    /// results (49.5 → 50, 66.0 → 66, 10.0 → 10).
    #[test]
    fn rounding_matches_pre_extraction_round() {
        assert_eq!(point_to_physical(33.0, 1.5), 50);
        assert_eq!(point_to_physical(33.0, 2.0), 66);
        assert_eq!(point_to_physical(20.0, 0.5), 10);
    }

    /// Negative coordinates (virtual-screen multi-monitor layouts place
    /// monitors left/above the primary): the sign must survive scaling.
    #[test]
    fn negative_coordinates_survive_scaling() {
        assert_eq!(point_to_physical(-1920.0, 1.0), -1920);
        assert_eq!(point_to_physical(-100.0, 1.5), -150);
    }
}
