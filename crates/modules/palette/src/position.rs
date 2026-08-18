//! Summon positioning: center the palette on the monitor containing the cursor
//! (RESEARCH Pattern 5 — winit 0.30 has no cursor-position API, so the palette
//! computes the physical center itself at summon time from xcap monitors +
//! `NSEvent::mouseLocation` on macOS).

use mybox_core::anyhow;

/// Physical-pixel geometry for the palette window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelGeometry {
    pub inner_size: (u32, u32),
    pub position: (i32, i32),
}

/// Pure geometry math (headless-testable, A3).
///
/// `monitors` is `(x, y, width, height, scale_factor)` in points (top-left
/// origin); `cursor` is in points (top-left origin). The monitor containing
/// the cursor is centered on; when the cursor is outside every monitor, the
/// first monitor is used (fallback). `panel_logical` is the panel size in
/// logical px (width is fixed at 600 per D-11).
pub fn compute_geometry(
    monitors: &[(f64, f64, f64, f64, f64)],
    cursor: (f64, f64),
    panel_logical: (f32, f32),
) -> Option<PanelGeometry> {
    let (cx, cy) = cursor;
    let monitor = monitors
        .iter()
        .find(|(x, y, w, h, _)| cx >= *x && cx <= *x + *w && cy >= *y && cy <= *y + *h)
        .or_else(|| monitors.first())?;
    let (mx, my, mw, mh, scale) = monitor;
    let scale = *scale;
    let pw = (f64::from(panel_logical.0) * scale).round();
    let ph = (f64::from(panel_logical.1) * scale).round();
    let px = ((mx + mw / 2.0) * scale - pw / 2.0).round() as i32;
    let py = ((my + mh / 2.0) * scale - ph / 2.0).round() as i32;
    Some(PanelGeometry {
        inner_size: (pw as u32, ph as u32),
        position: (px, py),
    })
}

/// Flip a bottom-left-origin point (NSPoint, `NSEvent::mouseLocation`) to a
/// top-left-origin point using the total desktop height (A3 — the one
/// coordinate-conversion point; unit-tested).
pub(crate) fn flip_cursor_origin(mouse: (f64, f64), desktop_height: f64) -> (f64, f64) {
    (mouse.0, desktop_height - mouse.1)
}

/// Production geometry: enumerate xcap monitors (points, top-left origin),
/// locate the cursor, and center the panel.
pub fn summon_geometry(panel_logical: (f32, f32)) -> anyhow::Result<PanelGeometry> {
    let monitors: Vec<(f64, f64, f64, f64, f64)> = xcap::Monitor::all()?
        .iter()
        .map(|m| {
            Ok((
                f64::from(m.x()?),
                f64::from(m.y()?),
                f64::from(m.width()?),
                f64::from(m.height()?),
                f64::from(m.scale_factor()?),
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    if monitors.is_empty() {
        anyhow::bail!("palette: no monitors found");
    }
    let cursor = cursor_position(&monitors);
    compute_geometry(&monitors, cursor, panel_logical)
        .ok_or_else(|| anyhow::anyhow!("palette: failed to compute panel geometry"))
}

/// Cursor position in points, top-left origin.
#[cfg(target_os = "macos")]
fn cursor_position(monitors: &[(f64, f64, f64, f64, f64)]) -> (f64, f64) {
    // NSEvent::mouseLocation returns an NSPoint with a BOTTOM-left origin;
    // flip it to top-left using the total desktop height = max(y + height)
    // (avoids pulling in objc2-core-graphics for CGDisplayBounds).
    let mouse = objc2_app_kit::NSEvent::mouseLocation();
    let desktop_height = monitors
        .iter()
        .map(|(_, y, _, h, _)| y + h)
        .fold(f64::MIN, f64::max);
    flip_cursor_origin((mouse.x, mouse.y), desktop_height)
}

#[cfg(not(target_os = "macos"))]
fn cursor_position(monitors: &[(f64, f64, f64, f64, f64)]) -> (f64, f64) {
    // Windows positioning is Phase 4: fall back to the first monitor center.
    let (x, y, w, h, _) = monitors[0];
    (x + w / 2.0, y + h / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two monitors side by side: A (0,0 1920x1080 @1x), B (1920,0 2560x1440 @2x).
    fn monitors() -> Vec<(f64, f64, f64, f64, f64)> {
        vec![(0.0, 0.0, 1920.0, 1080.0, 1.0), (1920.0, 0.0, 2560.0, 1440.0, 2.0)]
    }

    #[test]
    fn cursor_in_monitor_b_centers_on_b() {
        let g = compute_geometry(&monitors(), (2200.0, 700.0), (600.0, 560.0))
            .expect("geometry computed");
        // B center (points): (1920 + 1280, 720) → physical ×2 → (6400, 1440).
        // Panel physical: (1200, 1120) → position (6400-600, 1440-560).
        assert_eq!(g.inner_size, (1200, 1120));
        assert_eq!(g.position, (5800, 880));
    }

    #[test]
    fn cursor_outside_all_monitors_falls_back_to_first() {
        let g = compute_geometry(&monitors(), (-500.0, -500.0), (600.0, 560.0))
            .expect("fallback geometry computed");
        // A center (points): (960, 540) → ×1 → panel (600, 560) → (660, 260).
        assert_eq!(g.inner_size, (600, 560));
        assert_eq!(g.position, (660, 260));
    }

    #[test]
    fn origin_flip_math_converts_bottom_left_to_top_left() {
        // A3: NSPoint (bottom-left) → top-left. A single 1080-high display:
        // mouse at bottom-left NSPoint (0, 0) → top-left (0, 1080).
        assert_eq!(flip_cursor_origin((0.0, 0.0), 1080.0), (0.0, 1080.0));
        assert_eq!(flip_cursor_origin((100.0, 1080.0), 1080.0), (100.0, 0.0));
        // Two monitors stacked: desktop height = max(y + h).
        let stacked = vec![(0.0, 0.0, 1920.0, 1080.0, 1.0), (0.0, 1080.0, 1920.0, 1080.0, 1.0)];
        let desktop_height = stacked.iter().map(|(_, y, _, h, _)| y + h).fold(f64::MIN, f64::max);
        assert_eq!(desktop_height, 2160.0);
        // Bottom display (y in [1080, 2160]) maps to top-left y in [0, 1080].
        assert_eq!(flip_cursor_origin((500.0, 1620.0), desktop_height), (500.0, 540.0));
    }

    #[test]
    fn scale_factor_doubles_size_and_position() {
        // A 2x monitor centered at (1000, 500) points: physical center is
        // (2000, 1000) — the panel's physical size and offset must both scale.
        let g = compute_geometry(&[(0.0, 0.0, 2000.0, 1000.0, 2.0)], (1000.0, 500.0), (600.0, 560.0))
            .expect("geometry computed");
        assert_eq!(g.inner_size, (1200, 1120));
        assert_eq!(g.position, (1400, 440));
    }

    #[test]
    fn empty_monitors_returns_none() {
        assert!(compute_geometry(&[], (0.0, 0.0), (600.0, 560.0)).is_none());
    }

    #[test]
    fn scale_1_5_matches_hand_computed_geometry() {
        // 150% scale — the Windows 150% DPI case of success criterion 4.
        // Hand-computed chain: panel logical (600, 560) × 1.5 → (900, 840);
        // monitor center (960, 540) × 1.5 = (1440, 810); position =
        // (1440 − 450, 810 − 420) = (990, 390).
        let g = compute_geometry(&[(0.0, 0.0, 1920.0, 1080.0, 1.5)], (960.0, 540.0), (600.0, 560.0))
            .expect("geometry computed");
        assert_eq!(g.inner_size, (900, 840));
        assert_eq!(g.position, (990, 390));
    }

    #[test]
    fn off_center_cursor_keeps_panel_centered_on_monitor_at_1_5() {
        // A non-center cursor inside the same monitor must not move the panel:
        // the panel is centered on the MONITOR (not the cursor). With cursor
        // (300, 200) the monitor center chain is unchanged → same position as
        // the center-cursor case.
        let center = compute_geometry(&[(0.0, 0.0, 1920.0, 1080.0, 1.5)], (960.0, 540.0), (600.0, 560.0))
            .expect("center geometry computed");
        let off = compute_geometry(&[(0.0, 0.0, 1920.0, 1080.0, 1.5)], (300.0, 200.0), (600.0, 560.0))
            .expect("off-center geometry computed");
        assert_eq!(off.inner_size, center.inner_size);
        assert_eq!(off.position, center.position);
        assert_eq!(off.inner_size, (900, 840));
        assert_eq!(off.position, (990, 390));
    }
}
