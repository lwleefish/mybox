//! Palette UI (03-01 + 03-02): the egui frame body per the UI-SPEC design
//! contract — colors, spacing, typography, panel geometry, and the
//! state-dispatched command list (Idle/Filtering/Empty/Executing/Error).
//!
//! The palette card is custom-painted (Frame::none + painter): opaque `#202020`
//! full-bleed with a 12px radius and a hairline border. The render closures
//! (raster::paint → on_draw blit) live in lib.rs; this module only draws.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use mybox_core::command::Command;
use mybox_core::egui;
use mybox_core::window::WindowManagerHandle;
use mybox_core::UiThreadProxy;

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
///
/// `windows`/`ui_proxy` (03-06, GAP-5): the row click-execute chain — a click
/// on a command row routes through `execute::execute` exactly like Enter.
pub fn draw(
    ctx: &egui::Context,
    session: &Arc<PaletteSession>,
    windows: &Arc<WindowManagerHandle>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
) {
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
            // 03-06 (GAP-4): the card packs EXACTLY per the UI-SPEC geometry
            // table — item spacing is zeroed so the painted 48px input box +
            // 8px gap + 48px rows sum to the window height with no slack
            // (any slack becomes phantom ScrollArea scroll space).
            ui.spacing_mut().item_spacing.y = 0.0;
            let input_rect = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width(), SP_2XL),
            );
            dim.rect_filled(input_rect, RADIUS_CONTROL, ROW_HOVERED);
            let text_rect = input_rect.shrink(SP_MD);
            // Reserve the exact 48px input row. The TextEdit is placed in a
            // child ui that does NOT advance the cursor — its intrinsic
            // height is font-dependent (~37px) and the old `ui.put` advance
            // left the painted box only partially accounted, so the list
            // started at y=60 (touching the box) instead of y=68.
            ui.allocate_rect(input_rect, egui::Sense::hover());

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
                // The TextEdit lives in its own child ui (`new_child` never
                // advances the parent cursor) so the reserved 48px input row
                // above stays intact regardless of the widget's intrinsic
                // height (03-06: exact card packing).
                let mut input_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
                let input_resp = input_ui.add(
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
                    draw_command_list(
                        ui,
                        &commands,
                        &filtered,
                        selection,
                        &highlights,
                        session,
                        windows,
                        ui_proxy,
                    );
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
                    draw_command_list(
                        ui,
                        &commands,
                        &filtered,
                        selection,
                        &highlights,
                        session,
                        windows,
                        ui_proxy,
                    );
                }
                PaletteState::Hidden => {}
            }
        });
}

/// The command list: rows in `filtered` order, selection background, hover
/// background, keyword highlight (Filtering), auto-scroll to keep the
/// selection visible.
///
/// 03-06 (GAP-4): rows are drawn with the ScrollArea **content ui's own
/// painter** — row rects live in content coordinates (translated by the
/// scroll offset) and the old outer CentralPanel painter's screen space only
/// coincides with them at offset 0. `item_spacing.y` is zeroed because the
/// geometry table packs rows at exactly 48px — any spacing would make n rows
/// overflow the viewport (5 rows = 252px > 240px) and create a phantom scroll
/// offset. The outer `dim`/`full` painters stay untouched: the input box and
/// the status/error/empty blocks live in CentralPanel space, not inside the
/// ScrollArea.
fn draw_command_list(
    ui: &mut egui::Ui,
    commands: &[Command],
    filtered: &[usize],
    selection: Option<usize>,
    highlights: &HashMap<usize, Match>,
    session: &Arc<PaletteSession>,
    windows: &Arc<WindowManagerHandle>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // GAP-4: exact 48px packing — no phantom scroll.
            ui.spacing_mut().item_spacing.y = 0.0;
            // GAP-4: content-space painter (same coordinate system as the
            // row rects). Opacity carries the D-04 dim while Executing.
            let mut row_painter = ui.painter().clone();
            row_painter.set_opacity(if session.state() == PaletteState::Executing {
                0.5
            } else {
                1.0
            });
            for (pos, &idx) in filtered.iter().enumerate() {
                if let Some(cmd) = commands.get(idx) {
                    // Selection is a FILTERED-space position — compare with the
                    // loop position, never the command index (Filtering
                    // reorders the list).
                    let hl = highlights.get(&idx);
                    draw_command_row(
                        ui,
                        &row_painter,
                        cmd,
                        selection == Some(pos),
                        hl.map(|m| m.name_indices.as_slice()).unwrap_or(&[]),
                        hl.map(|m| m.description_indices.as_slice()).unwrap_or(&[]),
                        hl.and_then(|m| m.keyword_hit.as_ref()),
                        session,
                        windows,
                        ui_proxy,
                    );
                }
            }
        });
}

/// A CommandRow (03-06, GAP-4/GAP-5 rewrite): 48px tall, name 14/600 on top,
/// description 12/400 below with a 4px gap; selected bg #404040, hovered bg
/// #2E2E2E, radius 8 (D-08/D-10). Matched characters render in `#FF6000` via
/// a LayoutJob (color only — size and weight stay identical, UI-SPEC lines
/// 62-63). The selected row is auto-scrolled into view (`scroll_to_rect`,
/// no-op when already visible).
///
/// GAP-4: painting uses the ScrollArea content ui's own painter (passed in),
/// so hover/selected backgrounds share the exact row rect coordinate system;
/// the layout puts the name at top+8 and the description 4px below the name
/// line (the old layout had name at +20 and the description bottom at ≈+57.8,
/// spilling past the 48px row into the next row).
///
/// GAP-5: rows interact with `Sense::click` under a stable per-command
/// widget id (T-03-13); a click selects and executes the command with the
/// same semantics as Enter (guarded by `execute`'s `set_executing`
/// re-entrancy check — Executing/Empty/Error-state clicks are rejected).
/// Returns the row `Response` so headless tests can drive hover/click.
fn draw_command_row(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    cmd: &Command,
    selected: bool,
    name_hl: &[usize],
    description_hl: &[usize],
    keyword_hit: Option<&filter::KeywordHit>,
    session: &Arc<PaletteSession>,
    windows: &Arc<WindowManagerHandle>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
) -> egui::Response {
    let row_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), SP_2XL),
    );
    // GAP-5: `Sense::click` under a stable per-command id (the old hover-only
    // sense could never produce a click — no clicked() branch existed).
    // `interact` does not advance the cursor; the explicit advance keeps rows
    // packed at exactly 48px (item_spacing.y is zeroed by draw_command_list).
    let id = ui.make_persistent_id(("palette-row", cmd.id));
    let resp = ui.interact(row_rect, id, egui::Sense::click());
    ui.advance_cursor_after_rect(row_rect);
    if !ui.is_rect_visible(row_rect) {
        return resp;
    }
    if selected {
        painter.rect_filled(row_rect, RADIUS_CONTROL, ROW_SELECTED);
        // Auto-scroll: keep the highlighted row visible (minimal scroll when
        // it is already on screen).
        ui.scroll_to_rect(row_rect, None);
    } else if resp.hovered() {
        painter.rect_filled(row_rect, RADIUS_CONTROL, ROW_HOVERED);
    }
    // GAP-5: click = select + execute, the same semantics as Enter. The
    // re-entrancy guard inside `execute` rejects clicks outside Idle/Filtering
    // (T-03-11/T-03-12); a headless session (proxy not yet injected) skips
    // execution — the same discipline as the Enter arm in `on_palette_key`.
    if resp.clicked() {
        if let Some(proxy) = ui_proxy.get() {
            crate::execute::execute(session, proxy, windows, cmd.clone());
        }
    }
    // GAP-4 layout: 12px horizontal padding, 8px top padding; the description
    // sits 4px below the name line — both lines fit inside the 48px row.
    let name_pos = egui::pos2(row_rect.left() + SP_MD, row_rect.top() + SP_SM);
    let desc_pos = name_pos + egui::vec2(0.0, FONT_SIZE_NAME * 1.3 + SP_XS);
    let name_job = highlight_job(&cmd.name, name_hl, FONT_SIZE_NAME, TEXT);
    let name_galley = painter.layout_job(name_job);
    painter.galley(name_pos, name_galley, TEXT);
    // Gap 1 (UAT test 5) render layer: a keyword-tier hit appends the
    // ` · {keyword}` tag to the END of the description line — same line, no
    // new row (row_geometry_fits_48px invariant). The tag sections slice
    // `tag` by their byte ranges and merge into ONE desc galley.
    let mut desc_job = highlight_job(&cmd.description, description_hl, FONT_SIZE_DESC, TEXT_DIM);
    if let Some(kh) = keyword_hit {
        let (tag, tag_job) = keyword_tag_job(kh.keyword, &kh.indices, FONT_SIZE_DESC);
        for section in &tag_job.sections {
            desc_job.append(
                &tag[section.byte_range.clone()],
                0.0,
                section.format.clone(),
            );
        }
    }
    let desc_galley = painter.layout_job(desc_job);
    painter.galley(desc_pos, desc_galley, TEXT_DIM);
    resp
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

/// Build the keyword-tag LayoutJob: `" · {keyword}"` rendered at the end of
/// the description line (Gap 1 / UAT test 5 — Route A inline rendering).
/// The ` · ` separator is TEXT_DIM; the keyword's query-hit chars (per
/// `fuzzy_indices`) are ACCENT and the rest TEXT_DIM — color ONLY, never size
/// or weight (UI-SPEC L63 invariant). Returns the tag STRING alongside the
/// job because `LayoutJob.text` is a pub field whose `sections` byte ranges
/// index into it — the caller slices `tag` per section and appends each into
/// the description job (one merged galley, same line, no new row).
fn keyword_tag_job(keyword: &str, indices: &[usize], size: f32) -> (String, egui::text::LayoutJob) {
    let tag = format!(" · {keyword}");
    let fmt = |color: egui::Color32| egui::TextFormat {
        color,
        font_id: egui::FontId::new(size, egui::FontFamily::Proportional),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    // " · " is bytes 0..3 of `tag`; the keyword starts at byte 3.
    job.append(&tag[0..3], 0.0, fmt(TEXT_DIM));
    let mut cursor = 3;
    for (start, end) in char_indices_to_byte_ranges(keyword, indices) {
        let start = start + 3;
        let end = end + 3;
        if start > cursor {
            job.append(&tag[cursor..start], 0.0, fmt(TEXT_DIM));
        }
        job.append(&tag[start..end], 0.0, fmt(ACCENT));
        cursor = end;
    }
    if cursor < tag.len() {
        job.append(&tag[cursor..], 0.0, fmt(TEXT_DIM));
    }
    (tag, job)
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

    #[test]
    fn keyword_tag_job_marks_matched_chars_accent() {
        // Gap 1 (UAT test 5): the keyword tag " · jietu" marks the query-hit
        // chars ACCENT (#FF6000) and everything else TEXT_DIM — color only,
        // the UI-SPEC L63 invariant. "jt" hits j at char 0 and t at char 3 of
        // "jietu" → tag byte offsets 3 and 6 (the " · " separator is 0..3).
        let (tag, job) = keyword_tag_job("jietu", &[0, 3], FONT_SIZE_DESC);
        assert_eq!(tag, " · jietu", "tag = separator + keyword");
        let sections: Vec<(usize, egui::Color32)> = job
            .sections
            .iter()
            .map(|s| (s.byte_range.start, s.format.color))
            .collect();
        assert_eq!(sections[0], (0, TEXT_DIM), "the ' · ' separator is TEXT_DIM");
        assert_eq!(sections[1], (3, ACCENT), "j (char 0 of jietu) is ACCENT");
        assert_eq!(sections[2], (4, TEXT_DIM), "the 'ie' between hits is TEXT_DIM");
        assert_eq!(sections[3], (6, ACCENT), "t (char 3 of jietu) is ACCENT");
        assert_eq!(sections[4], (7, TEXT_DIM), "the trailing 'u' is TEXT_DIM");
        // Empty indices → no ACCENT anywhere: the separator + keyword runs are
        // all TEXT_DIM (Gap 1 plan: "全 TEXT_DIM 无 ACCENT").
        let (plain_tag, plain) = keyword_tag_job("jietu", &[], FONT_SIZE_DESC);
        assert_eq!(plain_tag, " · jietu");
        assert!(
            plain.sections.iter().all(|s| s.format.color == TEXT_DIM),
            "no hits → every section TEXT_DIM"
        );
        assert!(
            plain.sections.iter().all(|s| s.format.color != ACCENT),
            "no ACCENT without indices"
        );
    }

    #[test]
    fn row_geometry_fits_48px() {
        // GAP-4 arithmetic lock: name top (SP_SM=8) + name line height
        // (14·1.3=18.2) + gap (SP_XS=4) + description line height (12·1.3=15.6)
        // must fit inside the 48px row — the old layout (name at +20, desc
        // bottom ≈+57.8) spilled the description into the next row.
        let content_h = SP_SM + FONT_SIZE_NAME * 1.3 + SP_XS + FONT_SIZE_DESC * 1.3;
        assert!(
            content_h <= SP_2XL,
            "row content must fit in {SP_2XL}px, got {content_h}"
        );
        // The description line must also end above the row bottom (the second
        // half of GAP-4's spill): desc top = name top + name line + gap, so
        // desc bottom = desc top + desc line — same arithmetic, one line lower.
        let desc_bottom = SP_SM + FONT_SIZE_NAME * 1.3 + SP_XS + FONT_SIZE_DESC * 1.3;
        assert!(
            desc_bottom <= SP_2XL,
            "description must end inside the row, got {desc_bottom}"
        );
    }

    #[test]
    fn row_interact_hovers_and_clicks_execute() {
        use std::cell::RefCell;
        use std::rc::Rc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // A counting command: the click must route through execute →
        // set_executing → run_command (the runner increments on its worker
        // thread) — GAP-5's click = select + execute chain.
        let count = Arc::new(AtomicUsize::new(0));
        let cmd = Command {
            id: "test.row",
            name: "Row Command".to_string(),
            description: "click executes".to_string(),
            keywords: vec![],
            hide_before_execute: false,
            runner: {
                let c = Arc::clone(&count);
                Arc::new(move || {
                    let c = Arc::clone(&c);
                    Box::pin(async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                })
            },
        };
        let session = Arc::new(PaletteSession::new());
        session.summon(vec![cmd.clone()]);
        let windows = Arc::new(WindowManagerHandle::new());
        let proxy = Arc::new(OnceLock::new());
        proxy.set(UiThreadProxy::new()).ok();

        // Panel layout mirrors `draw` (exact card packing): 12px margin →
        // 48px input rect → 8px gap → row 1 at y 68..116 (center (300, 92))
        // on a 600×200 screen.
        let response: Rc<RefCell<Option<egui::Response>>> = Rc::new(RefCell::new(None));
        let run_row = |ctx: &egui::Context, response: &Rc<RefCell<Option<egui::Response>>>| {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().inner_margin(SP_MD))
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let input_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), SP_2XL),
                    );
                    ui.allocate_rect(input_rect, egui::Sense::hover());
                    ui.add_space(SP_SM);
                    let row_painter = ui.painter().clone();
                    *response.borrow_mut() = Some(draw_command_row(
                        ui,
                        &row_painter,
                        &cmd,
                        false,
                        &[],
                        &[],
                        None, // keyword_hit — this test command has no keywords
                        &session,
                        &windows,
                        &proxy,
                    ));
                });
        };
        let screen_rect =
            Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 200.0)));
        let egui_ctx = session.egui_ctx();

        // Frame 1: the pointer moves onto row 1's center. egui 0.30 hit-tests
        // the PREVIOUS pass's widgets (interaction lags one frame), so this
        // frame registers the row + establishes the pointer position — the
        // hover/click assertions run on frame 2's response.
        let _ = egui_ctx.run(
            egui::RawInput {
                screen_rect,
                events: vec![egui::Event::PointerMoved(egui::pos2(300.0, 92.0))],
                ..Default::default()
            },
            |ctx| run_row(ctx, &response),
        );
        assert!(
            !response.borrow().as_ref().expect("row response").clicked(),
            "a mere pointer move must not click the row"
        );
        assert_eq!(session.state(), PaletteState::Idle, "a pointer move must not execute");
        assert_eq!(count.load(Ordering::SeqCst), 0, "a pointer move must not run the runner");

        // Frame 2: press + release on row 1. The row widget was registered in
        // frame 1, so this frame's hit-test sees it: the response is both
        // hovered (pointer sits inside the row rect) and clicked (press +
        // release inside the rect) — and the click routes through execute.
        let _ = egui_ctx.run(
            egui::RawInput {
                screen_rect,
                events: vec![
                    egui::Event::PointerButton {
                        pos: egui::pos2(300.0, 92.0),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: egui::pos2(300.0, 92.0),
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| run_row(ctx, &response),
        );
        assert!(
            response.borrow().as_ref().expect("row response").hovered(),
            "the pointer inside row 1 must hover the row"
        );
        assert!(
            response.borrow().as_ref().expect("row response").clicked(),
            "the click on row 1 must register on the row response"
        );
        assert_eq!(
            session.state(),
            PaletteState::Executing,
            "the click must route through execute (Idle → Executing)"
        );
        // The runner completes on its worker thread; the finalize hop is
        // stashed by the headless proxy (no event loop), so the session stays
        // Executing while the counter proves the runner ran.
        for _ in 0..200 {
            if count.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(count.load(Ordering::SeqCst), 1, "the click must run the runner");
    }
}
