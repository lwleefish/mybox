//! Palette UI (03-01): the egui frame body per the UI-SPEC design contract —
//! colors, spacing, typography, panel geometry, and the command-row list.
//!
//! The palette card is custom-painted (Frame::none + painter): opaque `#202020`
//! full-bleed with a 12px radius and a hairline border. The render closures
//! (raster::paint → on_draw blit) live in lib.rs; this module only draws.

use mybox_core::command::Command;
use mybox_core::egui;

use crate::session::{PaletteSession, PaletteState};

// ─── Color tokens (UI-SPEC — Phase 2 trace carried forward verbatim) ───────
pub const BG: egui::Color32 = egui::Color32::from_rgb(0x20, 0x20, 0x20);
pub const ROW_SELECTED: egui::Color32 = egui::Color32::from_rgb(0x40, 0x40, 0x40);
pub const ROW_HOVERED: egui::Color32 = egui::Color32::from_rgb(0x2E, 0x2E, 0x2E);
/// Matched-keyword highlight ONLY (03-02 LayoutJob uses it; reserved here).
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x60, 0x00);
pub const ERROR: egui::Color32 = egui::Color32::from_rgb(0xE5, 0x48, 0x4D);
pub const TEXT: egui::Color32 = egui::Color32::WHITE;
pub const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(0xA8, 0xA8, 0xA8);
pub const PLACEHOLDER: egui::Color32 = egui::Color32::from_rgb(0x6E, 0x6E, 0x6E);
/// 1px card border over arbitrary screen content (alpha 0.08 ≈ 20/255).
pub const HAIRLINE: egui::Color32 = egui::Color32::from_rgba_premultiplied(20, 20, 20, 20);

// ─── Spacing tokens (UI-SPEC spacing scale, logical px) ─────────────────────
pub const SP_XS: f32 = 4.0;
pub const SP_SM: f32 = 8.0;
pub const SP_MD: f32 = 12.0;
pub const SP_LG: f32 = 16.0;
pub const SP_XL: f32 = 24.0;
pub const SP_2XL: f32 = 48.0;
pub const SP_3XL: f32 = 64.0;

// ─── Typography (exactly 3 sizes / 2 weights per UI-SPEC) ───────────────────
pub const FONT_SIZE_INPUT: f32 = 16.0;
pub const FONT_SIZE_NAME: f32 = 14.0;
pub const FONT_SIZE_DESC: f32 = 12.0;

/// Fixed panel width in logical px (D-11).
pub const PANEL_WIDTH: f32 = 600.0;

/// Corner radius: card 12, input box + rows 8 (UI-SPEC).
pub const RADIUS_CARD: f32 = 12.0;
pub const RADIUS_CONTROL: f32 = 8.0;

/// Panel height per state + visible row count (UI-SPEC geometry table).
///
/// Idle/Filtering: `80 + 48·min(n, 10)` → 128 (1 row) … 560 (10 rows);
/// Empty/Error: 144 fixed; Executing: `112 + 48·min(n, 10)`; Hidden: 0.
pub fn window_height(state: PaletteState, visible: usize) -> f32 {
    let n = visible.min(10) as f32;
    match state {
        PaletteState::Hidden => 0.0,
        PaletteState::Idle | PaletteState::Filtering => 80.0 + 48.0 * n,
        PaletteState::Empty | PaletteState::Error => 144.0,
        PaletteState::Executing => 112.0 + 48.0 * n,
    }
}

/// Dark fixed theme (D-09) + palette overrides. Called once at module
/// construction, before the first frame.
pub fn configure_egui_ctx(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    // The card is custom-painted; the egui root must not draw its own panel.
    visuals.panel_fill = egui::Color32::TRANSPARENT;
    visuals.window_rounding = egui::Rounding::same(RADIUS_CARD);
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    // Scrollbar thumb (#404040 per UI-SPEC) — best-effort via widget visuals;
    // the scrollbar only appears above 10 rows. (The input placeholder is
    // painted manually in `draw` with the exact #6E6E6E token — egui derives
    // its hint color from weak_text_color and offers no exact override.)
    visuals.widgets.inactive.weak_bg_fill = ROW_SELECTED;
    ctx.set_visuals(visuals);
}

/// The egui frame body: card + search input + command list (03-01 renders the
/// Idle state; Filtering/Empty/Executing/Error variants layer on in 03-02).
pub fn draw(ctx: &egui::Context, session: &PaletteSession) {
    // Custom-painted card root: opaque BG full-bleed, radius 12, hairline.
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.rect_filled(rect, RADIUS_CARD, BG);
    painter.rect_stroke(rect, RADIUS_CARD, egui::Stroke::new(1.0_f32, HAIRLINE));

    egui::CentralPanel::default()
        .frame(egui::Frame::none().inner_margin(SP_MD))
        .show(ctx, |ui| {
            let commands = session.commands();
            let filtered = session.filtered();
            let selection = session.selection();

            // ── SearchInput (48px, #2E2E2E radius 8, 12px padding, 16/400) ──
            let input_rect = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width(), SP_2XL),
            );
            ui.painter().rect_filled(input_rect, RADIUS_CONTROL, ROW_HOVERED);
            let text_rect = input_rect.shrink(SP_MD);

            let mut text = session.input();
            let input_resp = ui.put(
                text_rect,
                egui::TextEdit::singleline(&mut text)
                    .id(egui::Id::new("palette-input"))
                    .font(egui::FontId::new(
                        FONT_SIZE_INPUT,
                        egui::FontFamily::Proportional,
                    ))
                    .text_color(TEXT)
                    .frame(false)
                    .margin(egui::Margin::ZERO),
            );
            // Placeholder (UI-SPEC copywriting + exact #6E6E6E token — egui's
            // hint color is derived from weak_text_color, so paint it manually).
            if text.is_empty() {
                ui.painter().text(
                    text_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    "输入命令…",
                    egui::FontId::new(FONT_SIZE_INPUT, egui::FontFamily::Proportional),
                    PLACEHOLDER,
                );
            }
            if input_resp.changed() {
                session.set_input_raw(text);
            }
            if session.take_focus_request() {
                input_resp.request_focus();
            }
            ui.add_space(SP_SM);

            // ── Command list (registration order; scrolls above 10 rows) ────
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for &idx in &filtered {
                        if let Some(cmd) = commands.get(idx) {
                            draw_command_row(ui, cmd, selection == Some(idx));
                        }
                    }
                });
        });
}

/// A CommandRow: 48px tall, name 14/600 on top, description 12/400 below with
/// a 4px gap; selected bg #404040, hovered bg #2E2E2E, radius 8 (D-08/D-10).
/// Keyword highlighting (LayoutJob, #FF6000) arrives in 03-02.
fn draw_command_row(ui: &mut egui::Ui, cmd: &Command, selected: bool) {
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), SP_2XL),
    );
    let resp = ui.allocate_rect(row_rect, egui::Sense::hover());
    if !ui.is_rect_visible(row_rect) {
        return;
    }
    let painter = ui.painter();
    if selected {
        painter.rect_filled(row_rect, RADIUS_CONTROL, ROW_SELECTED);
    } else if resp.hovered() {
        painter.rect_filled(row_rect, RADIUS_CONTROL, ROW_HOVERED);
    }
    let inner = row_rect.shrink(SP_MD);
    let name_pos = inner.left_top() + egui::vec2(0.0, SP_SM);
    painter.text(
        name_pos,
        egui::Align2::LEFT_TOP,
        &cmd.name,
        egui::FontId::new(FONT_SIZE_NAME, egui::FontFamily::Proportional),
        TEXT,
    );
    let desc_pos = name_pos + egui::vec2(0.0, FONT_SIZE_NAME * 1.3 + SP_XS);
    painter.text(
        desc_pos,
        egui::Align2::LEFT_TOP,
        &cmd.description,
        egui::FontId::new(FONT_SIZE_DESC, egui::FontFamily::Proportional),
        TEXT_DIM,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_height_full_table() {
        // Idle/Filtering: 80 + 48·n.
        assert_eq!(window_height(PaletteState::Idle, 0), 80.0);
        assert_eq!(window_height(PaletteState::Idle, 1), 128.0);
        assert_eq!(window_height(PaletteState::Idle, 2), 176.0);
        assert_eq!(window_height(PaletteState::Idle, 5), 320.0);
        assert_eq!(window_height(PaletteState::Idle, 10), 560.0);
        assert_eq!(window_height(PaletteState::Idle, 15), 560.0, "cap at 10 rows");
        assert_eq!(window_height(PaletteState::Filtering, 3), 224.0);
        // Empty/Error: 144 fixed.
        assert_eq!(window_height(PaletteState::Empty, 0), 144.0);
        assert_eq!(window_height(PaletteState::Error, 5), 144.0);
        // Executing: 112 + 48·n.
        assert_eq!(window_height(PaletteState::Executing, 1), 160.0);
        assert_eq!(window_height(PaletteState::Executing, 10), 592.0);
        assert_eq!(window_height(PaletteState::Executing, 12), 592.0, "cap at 10 rows");
        // Hidden: 0.
        assert_eq!(window_height(PaletteState::Hidden, 10), 0.0);
    }

    #[test]
    fn ui_spec_constants_match_contract() {
        // Spot-check the UI-SPEC color/spacing/typography table.
        assert_eq!(BG, egui::Color32::from_rgb(0x20, 0x20, 0x20));
        assert_eq!(ROW_SELECTED, egui::Color32::from_rgb(0x40, 0x40, 0x40));
        assert_eq!(ROW_HOVERED, egui::Color32::from_rgb(0x2E, 0x2E, 0x2E));
        assert_eq!(ACCENT, egui::Color32::from_rgb(0xFF, 0x60, 0x00));
        assert_eq!(TEXT_DIM, egui::Color32::from_rgb(0xA8, 0xA8, 0xA8));
        assert_eq!(PLACEHOLDER, egui::Color32::from_rgb(0x6E, 0x6E, 0x6E));
        assert_eq!(SP_MD, 12.0);
        assert_eq!(SP_2XL, 48.0);
        assert_eq!(FONT_SIZE_INPUT, 16.0);
        assert_eq!(FONT_SIZE_NAME, 14.0);
        assert_eq!(FONT_SIZE_DESC, 12.0);
        assert_eq!(PANEL_WIDTH, 600.0);
    }
}
