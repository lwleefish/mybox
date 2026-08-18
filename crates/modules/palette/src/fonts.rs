//! CJK font loading (UI-SPEC hard requirement / RESEARCH Pitfall 5).
//!
//! egui's built-in fonts contain no CJK glyphs — without a system CJK font
//! inserted at the head of the Proportional family, every Chinese command name
//! renders as tofu boxes (□). The Hiragino Sans GB TTC is loaded as two faces
//! (index 0 = W3 regular, index 1 = W6 bold — A1) and installed once, before
//! the first frame.

use mybox_core::anyhow;
use mybox_core::egui;

/// Install the system CJK font at the head of the Proportional family.
///
/// Must run once before the first frame (egui caches the font atlas). Failure
/// is a warn-and-continue at the call site (ASCII fallback) — the caller logs.
#[cfg(target_os = "macos")]
pub fn install_cjk_fonts(ctx: &egui::Context) -> anyhow::Result<()> {
    let bytes = std::fs::read("/System/Library/Fonts/Hiragino Sans GB.ttc")?;
    let mut defs = egui::FontDefinitions::default();
    // index 0 = W3 (regular).
    defs.font_data.insert(
        "hiragino-w3".to_string(),
        egui::FontData::from_owned(bytes.clone()).into(),
    );
    // index 1 = W6 (bold) — epaint 0.30 exposes `FontData.index` for TTC face
    // selection (source-verified).
    defs.font_data.insert(
        "hiragino-w6".to_string(),
        egui::FontData {
            index: 1,
            ..egui::FontData::from_owned(bytes)
        }
        .into(),
    );
    if let Some(family) = defs.families.get_mut(&egui::FontFamily::Proportional) {
        // Head of the family — first entry wins for missing glyphs; the egui
        // defaults stay as ASCII/emoji fallback behind them.
        family.insert(0, "hiragino-w3".to_string());
        family.insert(1, "hiragino-w6".to_string());
    }
    ctx.set_fonts(defs);
    Ok(())
}

/// Install the system CJK font at the head of the Proportional family
/// (Windows).
///
/// Fallback chain: Microsoft YaHei (primary) → SimHei → SimSun, all in the
/// standard Windows font directory. The paths are ASSUMED (04-RESEARCH A4) —
/// the chain covers a miss, and a total failure is a warn-and-continue at the
/// call site (same as macOS).
#[cfg(target_os = "windows")]
pub fn install_cjk_fonts(ctx: &egui::Context) -> anyhow::Result<()> {
    for path in [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            let mut defs = egui::FontDefinitions::default();
            defs.font_data.insert(
                "cjk".to_string(),
                egui::FontData::from_owned(bytes).into(),
            );
            if let Some(family) = defs.families.get_mut(&egui::FontFamily::Proportional) {
                // Head of the family — first entry wins for missing glyphs; the
                // egui defaults stay as ASCII/emoji fallback behind them.
                family.insert(0, "cjk".to_string());
            }
            ctx.set_fonts(defs);
            return Ok(());
        }
    }
    anyhow::bail!("no CJK font found in C:\\Windows\\Fonts")
}

/// Other platforms: ASCII-only fallback (no system CJK font loaded).
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn install_cjk_fonts(_ctx: &egui::Context) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn install_cjk_fonts_populates_font_data() {
        let ctx = egui::Context::default();

        // Control: egui's built-in fonts carry no CJK glyphs (the Pitfall 5
        // premise this whole module exists for).
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let before = ctx.fonts(|f| {
            f.has_glyphs(
                &egui::FontId::new(14.0, egui::FontFamily::Proportional),
                "截图",
            )
        });
        assert!(!before, "egui defaults must not have CJK glyphs (test premise)");

        install_cjk_fonts(&ctx).expect("system TTC must load on macOS");

        // New fonts become active at the start of the next pass.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let after = ctx.fonts(|f| {
            f.has_glyphs(
                &egui::FontId::new(14.0, egui::FontFamily::Proportional),
                "截图",
            )
        });
        assert!(after, "hiragino (head of Proportional) must provide CJK glyphs");
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn install_cjk_fonts_populates_font_data() {
        let ctx = egui::Context::default();

        // Control: egui's built-in fonts carry no CJK glyphs (the Pitfall 5
        // premise this whole module exists for).
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let before = ctx.fonts(|f| {
            f.has_glyphs(
                &egui::FontId::new(14.0, egui::FontFamily::Proportional),
                "截图",
            )
        });
        assert!(!before, "egui defaults must not have CJK glyphs (test premise)");

        install_cjk_fonts(&ctx).expect("system CJK font must load on Windows");

        // New fonts become active at the start of the next pass.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let after = ctx.fonts(|f| {
            f.has_glyphs(
                &egui::FontId::new(14.0, egui::FontFamily::Proportional),
                "截图",
            )
        });
        assert!(after, "CJK font (head of Proportional) must provide CJK glyphs");
    }
}
