//! Display / OS integration tests for the palette module (plan 03-02-03).
//!
//! Every test here is `#[ignore]`: they need a real macOS GUI session. They are
//! excluded from `cargo nextest run`; run them explicitly with:
//!
//! ```text
//! cargo test -- --ignored -p mybox-palette
//! ```
//!
//! ## Why these tests spawn a helper binary
//!
//! winit on macOS requires the `EventLoop` to be created on the **real main
//! thread** (`MainThreadMarker` panics otherwise) and allows only one
//! `EventLoop` per process. Rust's `cargo test` harness runs each `#[test]` on
//! a spawned worker thread, so the checks cannot create an `EventLoop` inline.
//! Each test therefore spawns the `palette_checks` binary (see
//! `crates/modules/palette/src/bin/palette_checks.rs`) in its own process,
//! where the check runs on that process's main thread (mirrors
//! `crates/modules/capture/tests/integration.rs`, 02-04).

use std::process::Command;

/// Path to the built `palette_checks` binary (cargo sets `CARGO_BIN_EXE_*` for
/// integration tests of the same package).
const CHECKS_BIN: &str = env!("CARGO_BIN_EXE_palette_checks");

/// Run one palette check in a fresh subprocess and assert it exited cleanly.
fn run_check(name: &str) {
    let status = Command::new(CHECKS_BIN)
        .arg(name)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn palette_checks '{name}': {e}"));
    assert!(
        status.success(),
        "palette_checks '{name}' exited with {status:?}"
    );
}

/// Test 1 — summon creates a Floating window, the frame loop rasterizes real
/// content into the framebuffer, the render chain presents, and ESC closes
/// with a paired Destroy (PAL-01/05 render path).
#[test]
#[ignore]
fn palette_summon_render() {
    run_check("summon_render");
}

/// Test 2 — fuzzy filter reorders the list (name tier before keyword tier),
/// ArrowDown selects filtered position 1, Enter executes the command mapped
/// through `filtered` (idx 2, not idx 1 — the Filtering-reorder regression),
/// and the real finalize hop destroys the window (PAL-03/04).
#[test]
#[ignore]
fn palette_fuzzy_navigation_execute() {
    run_check("fuzzy_navigation_execute");
}

/// Test 3 — a `hide_before_execute` command enqueues the Destroy BEFORE the
/// runner runs, and produces no second Destroy after completion (the capture
/// screenshot-order hard constraint, Pitfall 4).
#[test]
#[ignore]
fn palette_capture_hides_first() {
    run_check("capture_hides_first");
}

/// Test 4 — five consecutive summon/ESC rounds pair 5 Creates with 5 Destroys,
/// advance the generation 5 times, and leave zero residue (the Phase 2
/// re-entrancy lesson generalized, PAL-05).
#[test]
#[ignore]
fn palette_five_summon_esc_no_residue() {
    run_check("five_summon_esc_no_residue");
}

/// Test 5 — three consecutive summon/close rounds on real windows/event loop:
/// each round observes ≥2 frames with NO Destroy (the panel stays visible — the
/// direct PAL-01/GAP-1 flash-close regression), then ESC pairs the Destroy with
/// zero residue; a final summon is observed for ≥3 frames before the last close
/// (PAL-01 gap closure, 03-03).
#[test]
#[ignore]
fn palette_consecutive_summon_close() {
    run_check("consecutive_summon_close");
}

/// Test 6 — real-window glyph rendering (PAL-02 / GAP-2 regression, 03-04):
/// three frames with Ime::Commit injections between them force incremental
/// glyph rasterization (partial atlas deltas — the apply_textures in-place
/// patch path), then asserts the frame diff > 0 and glyph STRUCTURE on the
/// final framebuffer (bbox ≥8x8, sparse coverage <0.7, ≥16 distinct values,
/// ≥1 fully-covered pixel) — the direct regression of "all text renders as
/// solid gray blocks".
#[test]
#[ignore]
fn palette_glyph_shape() {
    run_check("glyph_shape");
}

/// Test 7 — GAP-3 / WR-01 / WR-02 regression (03-05): on a real window, the
/// filter shrink (5 commands → 1 match, 320→128), the input restore (1 → 5,
/// 128→320), and the Executing growth (status line +32px, 320→352) must each
/// change the window height while the outer position stays EXACTLY at the
/// summon position (no re-centering — GAP-3 "panel falls" drift), and the
/// session framebuffer must cover the window's physical size at every stage
/// (PAL-03 / GAP-3 + WR-01/WR-02).
#[test]
#[ignore]
fn palette_position_stable_on_filter() {
    run_check("position_stable_on_filter");
}

/// Test 8 — GAP-4 / GAP-5 regression (03-06, PAL-04): on a real window,
/// synthetic CursorMoved + MouseInput events drive the full egui-winit → egui
/// hit-testing → clicked → execute chain. The hover frame must put the
/// ROW_HOVERED fill exactly inside row 1's band (≥100 pixels), zero highlight
/// pixels above the band, and row text pixels inside the same band (highlight
/// and text overlap — GAP-4); the click must enter Executing through the
/// re-entrancy-guarded execute path and the runner must run exactly once after
/// the gate release (GAP-5: the old hover-only sense could never click).
#[test]
#[ignore]
fn palette_hover_click_alignment() {
    run_check("hover_click_alignment");
}

/// Test 9 — GAP-6 regression (03-07, PAL-04): on a real window, a REAL
/// `WindowEvent::ModifiersChanged` injection flows through the production
/// on_event_win closure into the session modifier tracking, and Ctrl+P /
/// Ctrl+N route through the key router exactly like ↑/↓ (wrap-around in Idle:
/// Ctrl+P → last entry, Ctrl+N → index 0), while the unmodified press_key
/// path (ESC) still closes the panel. The OS physical Ctrl+P keypress → winit
/// event stream is re-verified by human UAT test 9 on the desktop.
#[test]
#[ignore]
fn palette_ctrl_pn_navigation() {
    run_check("ctrl_pn_navigation");
}

/// Test 10 — GAP-7 + GAP-8 regression (03-08/03-09, PAL-01/PAL-03): on a real
/// window, synthetic `Ime::Preedit`/`Ime::Commit` events drive the full
/// egui-winit → egui `Event::Ime` → TextEdit → `session.set_input` chain.
/// The first window event must set the explicit IME-enable flag
/// (`ime_allowed` — GAP-7's code-level fix); the committed Chinese text
/// "截图" must reach `session.input`, move the state to Filtering and filter
/// to [0] (capture.start); `set_input("tuichu")` must hit builtin.quit via
/// the new pinyin keyword alias (the no-IME prefix-discovery path → filtered
/// [1]); ESC closes with a paired Destroy. **Re-summon extension (03-09,
/// GAP-8 / REVIEW WR-01):** ESC close then `summon_palette` re-summons a
/// SECOND window; the probe asserts `ime_allowed` was reset to false before
/// the second window's first event (summon reset evidence) AND re-set to
/// true after the first event is processed through the real production
/// closure (REVIEW WR-01's exact defect path that 03-08's probe missed) —
/// the GAP-8 coverage-hole fix. A second Chinese IME flow — Preedit
/// "重新截图" (the composition candidate buffer the OS input method displays)
/// + Commit "截图" (matching `开始截图`'s name tier) — verifies zero-regression
/// on the second window's freshly-built egui-winit State; ESC closes the
/// second window with a paired Destroy. The OS candidate-window appearance
/// (first AND re-summon scenarios) is re-verified by human UAT test 10.
#[test]
#[ignore]
fn palette_ime_commit_updates_input() {
    run_check("ime_commit_updates_input");
}

/// Test 11 — Gap 1 / UAT test 5 regression (03-10, PAL-03): on a real window,
/// the production frame loop renders the keyword-tier highlight end to end —
/// "jt" filters capture.start first via the "jietu" pinyin keyword and the
/// " · jietu" tag's matched glyphs paint exact #FF6000 (ACCENT) pixels inside
/// row 1's band; "tuichu" repeats the assertion for builtin.quit (the WHOLE
/// keyword tier renders the tag, not just capture.start — the filter-layer
/// index assertions for all five pinyin keywords live in the Task-1 unit
/// tests). ESC closes with a paired Destroy. The OS-level "the user's eye
/// sees the orange highlight" truth is re-verified by human UAT test 5 on the
/// desktop.
#[test]
#[ignore]
fn palette_keyword_highlight() {
    run_check("keyword_highlight");
}

/// Test 12 — Gap 2 / UAT test 11 regression (03-10, PAL-04/PAL-05): on a real
/// window, synthetic CursorMoved + MouseInput events click capture.start
/// (hide_before_execute + a gated read-screen runner). The click frame must
/// close the panel (Hidden) AND synchronously hide the window at the window
/// server (`is_visible() == Some(false)`) BEFORE the gated runner starts
/// (counter == 0 — the read-screen never saw the panel), with the Destroy
/// already enqueued; releasing the gate runs the runner exactly once with NO
/// second Destroy — the Enter/click timing convergence (the panel is
/// off-screen before any screenshot read, UAT 11's direct regression).
#[test]
#[ignore]
fn palette_click_hide_before_capture() {
    run_check("click_hide_before_capture");
}
