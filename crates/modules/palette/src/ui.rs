//! Palette UI (03-01 + 03-02): the egui frame body per the UI-SPEC design
//! contract — colors, spacing, typography, panel geometry, and the
//! state-dispatched command list (Idle/Filtering/Empty/Executing/Error).
//!
//! The palette card is custom-painted (Frame::none + painter): opaque `#202020`
//! full-bleed with a 12px radius and a hairline border. The render closures
//! (raster::paint → on_draw blit) live in lib.rs; this module only draws.

use std::collections::HashMap;

use mybox_core::command::Command;
use mybox_core::egui;

use crate::filter::{self, Match};
use crate::session::{PaletteSession, PaletteState};

// ─── Color tokens (UI-SPEC — Phase 2 trace carried forward verbatim) ───────
pub const BG: egui::Color32 = egui::Color32::from_rgb(0x20, 0x20, 0x20);
pub const ROW_SELECTED: egui::Color32 = egui::Color32::from_rgb(0x40, 0x40, 0x40);
pub const ROW_HOVERED: egui::Color32 = egui::Color32::from_rgb(0x2E, 0x2E, 0x2E);
/// Matched-keyword highlight ONLY (D-10, #FF6000 — only the color changes,
/// never size or weight; UI-SPEC lines 62-63).
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

/// The egui frame body: card + search input + the state-dispatched body
/// (Filtering highlight rows / Empty / Executing status + dimmed list /
/// Error block / zero-command fallback).
pub fn draw(ctx: &egui::Context, session: &PaletteSession) {
    // Custom-painted card root: opaque BG full-bleed, radius 12, hairline.
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.rect_filled(rect, RADIUS_CARD, BG);
    painter.rect_stroke(rect, RADIUS_CARD, egui::Stroke::new(1.0_f32, HAIRLINE));

    egui::CentralPanel::default()
        .frame(egui::Frame::none().inner_margin(SP_MD))
        .show(ctx, |ui| {
            let state = session.state();
            let commands = session.commands();
            let filtered = session.filtered();
            let selection = session.selection();
            let input = session.input();
            let executing = state == PaletteState::Executing;

            // Highlight lookup for Filtering rows (filter is deterministic and
            // cheap — recomputed per frame; the session stores only the ranked
            // command indices).
            let highlights: HashMap<usize, Match> = if state == PaletteState::Filtering {
                filter::filter_commands(&commands, &input)
                    .into_iter()
                    .map(|m| (m.cmd_index, m))
                    .collect()
            } else {
                HashMap::new()
            };

            // Dimmable painter (input + rows — 50% alpha while Executing,
            // UI-SPEC "disabled" token) and a full-alpha painter for the
            // status/error/empty blocks.
            let mut dim = ui.painter_at(ui.clip_rect());
            dim.set_opacity(if executing { 0.5 } else { 1.0 });
            let full = ui.painter_at(ui.clip_rect());

            // ── SearchInput (48px, #2E2E2E radius 8, 12px padding, 16/400) ──
            let input_rect = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width(), SP_2XL),
            );
            dim.rect_filled(input_rect, RADIUS_CONTROL, ROW_HOVERED);
            let text_rect = input_rect.shrink(SP_MD);

            if executing {
                // Static input render: no TextEdit exists while Executing, so
                // input is impossible by construction (anti-reentrancy, D-04)
                // and the dim applies uniformly to the input content.
                if input.is_empty() {
                    dim.text(
                        text_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        "输入命令…",
                        egui::FontId::new(FONT_SIZE_INPUT, egui::FontFamily::Proportional),
                        PLACEHOLDER,
                    );
                } else {
                    dim.text(
                        text_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        &input,
                        egui::FontId::new(FONT_SIZE_INPUT, egui::FontFamily::Proportional),
                        TEXT,
                    );
                }
                // Consume (no widget to focus while Executing).
                let _ = session.take_focus_request();
            } else {
                let mut text = input.clone();
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
                    dim.text(
                        text_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        "输入命令…",
                        egui::FontId::new(FONT_SIZE_INPUT, egui::FontFamily::Proportional),
                        PLACEHOLDER,
                    );
                }
                // PAL-03 key link: the writeback must go through the FILTERING
                // `set_input` (state transitions + highlight reset), never the
                // raw setter.
                if input_resp.changed() {
                    session.set_input(&text);
                }
                if session.take_focus_request() {
                    input_resp.request_focus();
                }
            }
            ui.add_space(SP_SM);

            // ── Body: state dispatch (UI-SPEC States table) ──────────────────
            if commands.is_empty() {
                // Zero-commands fallback (UI-SPEC copywriting contract).
                draw_state_block(
                    ui,
                    &full,
                    &[
                        ("没有可用的命令".to_string(), FONT_SIZE_NAME, TEXT),
                        ("应用尚未注册任何命令".to_string(), FONT_SIZE_DESC, TEXT_DIM),
                    ],
                );
                return;
            }
            match state {
                PaletteState::Executing => {
                    // StatusLine (D-04 locked copy), full alpha, 4px gaps.
                    ui.add_space(SP_XS);
                    let name = command_name(session, &commands);
                    let status_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), SP_XL),
                    );
                    ui.allocate_rect(status_rect, egui::Sense::hover());
                    full.text(
                        status_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        format!("正在执行：{name}…"),
                        egui::FontId::new(FONT_SIZE_DESC, egui::FontFamily::Proportional),
                        TEXT_DIM,
                    );
                    ui.add_space(SP_XS);
                    // Dimmed frozen list (selection stays visible).
                    draw_command_list(ui, &dim, &commands, &filtered, selection, &highlights);
                }
                PaletteState::Empty => {
                    draw_state_block(
                        ui,
                        &full,
                        &[
                            ("没有匹配的命令".to_string(), FONT_SIZE_NAME, TEXT),
                            (
                                "换个关键词试试，清空输入可显示全部命令".to_string(),
                                FONT_SIZE_DESC,
                                TEXT_DIM,
                            ),
                        ],
                    );
                }
                PaletteState::Error => {
                    let name = command_name(session, &commands);
                    let error = session.error().unwrap_or_default();
                    draw_state_block(
                        ui,
                        &full,
                        &[
                            (format!("执行「{name}」失败"), FONT_SIZE_NAME, ERROR),
                            (error, FONT_SIZE_DESC, TEXT_DIM),
                            ("按任意键或 ESC 关闭".to_string(), FONT_SIZE_DESC, PLACEHOLDER),
                        ],
                    );
                }
                PaletteState::Idle | PaletteState::Filtering => {
                    draw_command_list(ui, &dim, &commands, &filtered, selection, &highlights);
                }
                PaletteState::Hidden => {}
            }
        });
}

/// The command list: rows in `filtered` order, selection background, hover
/// background, keyword highlight (Filtering), auto-scroll to keep the
/// selection visible.
fn draw_command_list(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    commands: &[Command],
    filtered: &[usize],
    selection: Option<usize>,
    highlights: &HashMap<usize, Match>,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (pos, &idx) in filtered.iter().enumerate() {
                if let Some(cmd) = commands.get(idx) {
                    // Selection is a FILTERED-space position — compare with the
                    // loop position, never the command index (Filtering
                    // reorders the list).
                    let hl = highlights.get(&idx);
                    draw_command_row(
                        ui,
                        painter,
                        cmd,
                        selection == Some(pos),
                        hl.map(|m| m.name_indices.as_slice()).unwrap_or(&[]),
                        hl.map(|m| m.description_indices.as_slice()).unwrap_or(&[]),
                    );
                }
            }
        });
}

/// A CommandRow: 48px tall, name 14/600 on top, description 12/400 below with
/// a 4px gap; selected bg #404040, hovered bg #2E2E2E, radius 8 (D-08/D-10).
/// Matched characters render in `#FF6000` via a LayoutJob (color only — size
/// and weight stay identical, UI-SPEC lines 62-63). The selected row is
/// auto-scrolled into view (`scroll_to_rect`, no-op when already visible).
fn draw_command_row(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    cmd: &Command,
    selected: bool,
    name_hl: &[usize],
    description_hl: &[usize],
) {
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), SP_2XL),
    );
    let resp = ui.allocate_rect(row_rect, egui::Sense::hover());
    if !ui.is_rect_visible(row_rect) {
        return;
    }
    if selected {
        painter.rect_filled(row_rect, RADIUS_CONTROL, ROW_SELECTED);
        // Auto-scroll: keep the highlighted row visible (minimal scroll when
        // it is already on screen).
        ui.scroll_to_rect(row_rect, None);
    } else if resp.hovered() {
        painter.rect_filled(row_rect, RADIUS_CONTROL, ROW_HOVERED);
    }
    let inner = row_rect.shrink(SP_MD);
    let name_pos = inner.left_top() + egui::vec2(0.0, SP_SM);
    let desc_pos = name_pos + egui::vec2(0.0, FONT_SIZE_NAME * 1.3 + SP_XS);
    let name_job = highlight_job(&cmd.name, name_hl, FONT_SIZE_NAME, TEXT);
    let name_galley = painter.layout_job(name_job);
    painter.galley(name_pos, name_galley, TEXT);
    let desc_job = highlight_job(&cmd.description, description_hl, FONT_SIZE_DESC, TEXT_DIM);
    let desc_galley = painter.layout_job(desc_job);
    painter.galley(desc_pos, desc_galley, TEXT_DIM);
}

/// Build a LayoutJob with the query-hit characters colored `#FF6000`
/// (`indices` are **char** positions from `fuzzy_indices` — converted to byte
/// ranges here, since the UI strings are UTF-8).
fn highlight_job(text: &str, indices: &[usize], size: f32, base: egui::Color32) -> egui::text::LayoutJob {
    let fmt = |color: egui::Color32| egui::TextFormat {
        color,
        font_id: egui::FontId::new(size, egui::FontFamily::Proportional),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    let mut cursor = 0;
    for (start, end) in char_indices_to_byte_ranges(text, indices) {
        if start > cursor {
            job.append(&text[cursor..start], 0.0, fmt(base));
        }
        job.append(&text[start..end], 0.0, fmt(ACCENT));
        cursor = end;
    }
    if cursor < text.len() {
        job.append(&text[cursor..], 0.0, fmt(base));
    }
    job
}

/// Convert fuzzy char positions to UTF-8 byte ranges (empty/out-of-range
/// positions are skipped; ranges are sorted and contiguous runs merged so a
/// consecutive hit sequence becomes one accent section).
fn char_indices_to_byte_ranges(text: &str, indices: &[usize]) -> Vec<(usize, usize)> {
    let starts: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let mut ranges = Vec::with_capacity(indices.len());
    for &ci in indices {
        if let Some(&start) = starts.get(ci) {
            let end = starts.get(ci + 1).copied().unwrap_or(text.len());
            ranges.push((start, end));
        }
    }
    ranges.sort_unstable();
    ranges.dedup();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if last.1 == start {
                last.1 = end;
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

/// A centered, vertically-middle multi-line block (Empty/Error/zero-command
/// states — 64px tall, each line centered horizontally).
fn draw_state_block(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    lines: &[(String, f32, egui::Color32)],
) {
    let block_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), SP_3XL),
    );
    ui.allocate_rect(block_rect, egui::Sense::hover());
    let wrap = ui.available_width();
    let galleys: Vec<_> = lines
        .iter()
        .map(|(text, size, color)| {
            painter.layout(
                text.clone(),
                egui::FontId::new(*size, egui::FontFamily::Proportional),
                *color,
                wrap,
            )
        })
        .collect();
    let total_h: f32 = galleys.iter().map(|g| g.size().y).sum::<f32>()
        + SP_XS * (lines.len().saturating_sub(1)) as f32;
    let mut y = block_rect.center().y - total_h / 2.0;
    for galley in &galleys {
        let x = block_rect.center().x - galley.size().x / 2.0;
        painter.galley(egui::pos2(x, y), std::sync::Arc::clone(galley), TEXT);
        y += galley.size().y + SP_XS;
    }
}

/// The executing/failed command's display name (looked up by id; falls back
/// to the id itself — the name is always registered, the fallback is purely
/// defensive).
fn command_name(session: &PaletteSession, commands: &[Command]) -> String {
    session
        .executing_id()
        .and_then(|id| commands.iter().find(|c| c.id == id).map(|c| c.name.clone()))
        .or_else(|| session.executing_id().map(str::to_string))
        .unwrap_or_default()
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
        // Executing: 112 + 48·n (status line adds 32px).
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

    #[test]
    fn char_indices_convert_to_utf8_byte_ranges() {
        // 开始截图: 开(0..3) 始(3..6) 截(6..9) 图(9..12) — 3 bytes per char.
        let text = "开始截图";
        assert_eq!(char_indices_to_byte_ranges(text, &[2, 3]), vec![(6, 12)], "contiguous merged");
        assert_eq!(char_indices_to_byte_ranges(text, &[0]), vec![(0, 3)]);
        assert_eq!(char_indices_to_byte_ranges(text, &[0, 2]), vec![(0, 3), (6, 9)]);
        assert_eq!(char_indices_to_byte_ranges(text, &[]), vec![]);
        assert_eq!(char_indices_to_byte_ranges(text, &[99]), vec![], "out of range skipped");
        // ASCII: 1 byte per char.
        assert_eq!(char_indices_to_byte_ranges("jietu", &[0, 3]), vec![(0, 1), (3, 4)]);
    }

    #[test]
    fn highlight_job_colors_only_matched_chars() {
        // Middle match: 始 at char 1 → base(开) + accent(始) + base(截图).
        let job = highlight_job("开始截图", &[1], 14.0, TEXT);
        let sections: Vec<(usize, egui::Color32)> = job
            .sections
            .iter()
            .map(|s| (s.byte_range.start, s.format.color))
            .collect();
        assert_eq!(sections.len(), 3, "base + accent + base");
        assert_eq!(sections[0], (0, TEXT));
        assert_eq!(sections[1], (3, ACCENT), "始 (bytes 3..6) highlighted");
        assert_eq!(sections[2], (6, TEXT));
        // Consecutive matched chars merge into a single accent run.
        let tail = highlight_job("开始截图", &[2, 3], 14.0, TEXT);
        let tail_sections: Vec<(usize, egui::Color32)> = tail
            .sections
            .iter()
            .map(|s| (s.byte_range.start, s.format.color))
            .collect();
        assert_eq!(tail_sections, vec![(0, TEXT), (6, ACCENT)]);
        // No highlights → a single base-color run.
        let plain = highlight_job("开始截图", &[], 14.0, TEXT);
        assert_eq!(plain.sections.len(), 1);
        assert_eq!(plain.sections[0].format.color, TEXT);
    }
}
