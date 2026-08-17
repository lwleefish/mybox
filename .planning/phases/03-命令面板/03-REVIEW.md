---
phase: 03-命令面板
reviewed: 2026-08-17T06:43:32Z
depth: standard
files_reviewed: 6
files_reviewed_list:
  - crates/modules/palette/src/filter.rs
  - crates/modules/palette/src/ui.rs
  - crates/modules/palette/src/lib.rs
  - crates/modules/palette/src/bin/palette_checks.rs
  - crates/modules/palette/tests/integration.rs
  - .planning/phases/03-命令面板/03-UI-SPEC.md
findings:
  critical: 1
  warning: 3
  info: 6
  total: 10
status: issues_found
---

# Phase 03: Code Review Report

**Reviewed:** 2026-08-17T06:43:32Z
**Depth:** standard
**Files Reviewed:** 6
**Status:** issues_found

## Summary

Review of plan 03-10 (keyword-tier highlight + click-path sync-hide), the final gap-closure wave of Phase 03, diff `c09317a..HEAD` over `crates/`. The filter-layer data channel (`Match.keyword_hit` / `KeywordHit`), the lib.rs Hidden frame-loop guard (`set_visible(false)` + early return), and both new E2E probes are structurally sound — the guard placement (after `egui_ctx.run`, before `apply_textures`) correctly skips paint/present/`request_redraw`, and the click probe's gated-runner ordering assertion (`is_visible()==Some(false)` + `counter==0` before the read-screen) is a genuine improvement over the headless-only coverage that let GAP-2 ship.

**However, the gap-1 render layer has a critical defect that the entire verification chain misses:**

- **CR-01** — `keyword_tag_job` (ui.rs:481-505) assumes the `" · "` separator is 3 bytes, but it is **4 bytes** (`' '` + U+00B7 middle dot, 2 bytes + `' '`). Every ACCENT byte range is shifted 1 byte left. For the flagship case — query `jt` → keyword `jietu`, indices `[0, 3]` — the code colors the **trailing space (invisible)** at byte 3 and the **`e`** at byte 6 ACCENT, instead of `j` (byte 4) and `t` (byte 7). The unit test at ui.rs:675-677 *asserts these wrong offsets as correct* ("j (char 0 of jietu) is ACCENT" at byte 3), and the E2E probe only asserts "> 0 ACCENT pixels in the band" — so 12/12 integration tests and the 19/39 measured ACCENT px all pass while the shipped highlight is visibly wrong (UAT test 5 will fail). Worse, for **any multi-byte (CJK) keyword-tier hit** the misaligned slicing panics at runtime — verified: `byte index 6 is not a char boundary` for `keyword_tag_job("截图", &[0,1])` — and that panic runs inside the `on_event_win` closure, which is not `catch_unwind`-wrapped (prior WR-02), killing the whole event loop. CJK keyword-tier hits are shadowed by name-tier matches only by coincidence of the current 5-command inventory; any future module command with a CJK keyword and a query that misses name/description reaches the panic through the public `filter_commands`/`Match` API.

Prior-review items WR-01 (create-window failure wedges the session), WR-02 (event callbacks not panic-isolated — now *reachable* via CR-01), WR-03 (zero-command fallback clipped at 128px) and IN-01..IN-05 were untouched by 03-10 and remain open. No secrets, no injection surfaces, no new unsafe code (the two existing `objc2` blocks are untouched and documented). The Hidden guard's interaction with `repaint()`/`about_to_wait` Destroy draining was traced: the guard runs before the Destroy drains (window Arc alive), `set_visible(false)` is idempotent on the ESC path, and `repaint()` is a no-op after `close()` — no ordering regression.

## Critical Issues

### CR-01: `keyword_tag_job` misaligns every ACCENT range by 1 byte — wrong highlight, and a guaranteed panic on CJK keyword-tier hits

**File:** `crates/modules/palette/src/ui.rs:481-505` (wrong assumption at L489-490, offset math at L492-499); test lock-in at `ui.rs:662-691`; probe blind spot at `crates/modules/palette/src/bin/palette_checks.rs:2048-2066`

**Issue:** The separator `" · "` is `' '` (1 byte) + `·` U+00B7 (2 bytes UTF-8: `0xC2 0xB7`) + `' '` (1 byte) = **4 bytes**. The code hardcodes 3:
```rust
// " · " is bytes 0..3 of `tag`; the keyword starts at byte 3.   // WRONG: keyword starts at byte 4
job.append(&tag[0..3], 0.0, fmt(TEXT_DIM));
let mut cursor = 3;
for (start, end) in char_indices_to_byte_ranges(keyword, indices) {
    let start = start + 3;   // WRONG: should be +4
    let end = end + 3;
    ...
}
```
Two concrete manifestations:

1. **Misplaced highlight (the phase deliverable).** For `keyword_tag_job("jietu", &[0, 3])`, byte offsets come out `[(3,4), (6,7)]` instead of `[(4,5), (7,8)]`: the **space** (byte 3, no glyph ink) and the **`e`** (byte 6) are ACCENT; the actual matched characters `j` (byte 4) and `t` (byte 7) render TEXT_DIM. The user types `jt` and sees ` · jietu` with an orange `e` — not the matched chars. The text is complete (sections cover all 9 bytes), so only the coloring is wrong — which is why both automated checks pass:
   - `ui.rs:675-677` unit test asserts `(3, ACCENT)` as "j" and `(6, ACCENT)` as "t" — **the test validates the buggy offsets as correct**, so the suite is green with wrong behavior.
   - `accent_pixels_in_row_band` (palette_checks.rs:2055-2065) counts pixels where `data[i]==0xFF && data[i+1]==0x60 && data[i+2]==0x00` — the misplaced `e` glyph supplies exactly those pixels, so the probe's "19 ACCENT px / 39 ACCENT px" evidence is produced by the wrong character.

2. **Guaranteed panic on non-ASCII keyword hits.** For any keyword containing multi-byte characters, the +3 shift slices at a non-char-boundary and panics. Verified with the exact logic: `keyword_tag_job("截图", &[0, 1])` → `char_indices_to_byte_ranges` yields `[(0,3),(3,6)]` → +3 → `tag[3..6]` → `end byte index 6 is not a char boundary; it is inside '截'`. This is reachable today via the public API (`filter_commands` is `pub`, `KeywordHit.keyword` is `pub`) and becomes trivially reachable as soon as any future module registers a command whose CJK keyword is hit while name/description miss (the current 5-command inventory shadows all CJK keyword-tier hits by name-tier matches only by naming coincidence). The panic occurs inside the frame loop of `on_event_win`, which per prior WR-02 is not wrapped in `catch_unwind` — it kills the entire event loop.

**Fix:** Derive the separator length from the string itself (or hardcode 4 with a comment tying it to the byte layout), and use it consistently; then correct the unit-test offsets so the test guards the *right* characters:
```rust
fn keyword_tag_job(keyword: &str, indices: &[usize], size: f32) -> (String, egui::text::LayoutJob) {
    let sep = " · ";
    let sep_len = sep.len();            // 4 bytes: ' ' (1) + U+00B7 (2) + ' ' (1)
    let tag = format!("{sep}{keyword}");
    let fmt = |color: egui::Color32| egui::TextFormat {
        color,
        font_id: egui::FontId::new(size, egui::FontFamily::Proportional),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    job.append(sep, 0.0, fmt(TEXT_DIM));
    let mut cursor = sep_len;
    for (start, end) in char_indices_to_byte_ranges(keyword, indices) {
        let start = start + sep_len;
        let end = end + sep_len;
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
```
And fix the unit test to assert the true layout — `sections[1] == (4, ACCENT)` ("j"), `sections[3] == (7, ACCENT)` ("t") — so the test locks the correct offsets instead of the buggy ones. Consider strengthening the E2E probe to assert ACCENT pixels at the keyword's expected x-position rather than "> 0 anywhere in the band", which is position-blind and let this defect ship green.

## Warnings

### WR-01: A failed `App::create_window` wedges the palette session permanently — no recovery path exists

**File:** `crates/mybox-core/src/app.rs:518-521` (error is only logged); `crates/modules/palette/src/session.rs:198-203` (`has_live_window`), `session.rs:210-223` (`close`)
**Issue:** Unchanged since the previous review — 03-10 did not touch this path. If `create_window` fails after `summon()` moved the session to `Idle` (window_id `None`), the error is only logged; `has_live_window()` stays `true` forever (`pending_close` never clears), so `toggle_palette` can never summon again until app restart. Reachable on headless/SSH, GPU-driver-failure, or display-disconnect systems.
**Fix:** Add an `on_create_failed` callback on `WindowSpec` (mirroring `on_created`) invoked from `create_window`'s error path, resetting the session to `Hidden` and clearing `pending_close`. (The naive "clear `pending_close` in `close()`'s else branch" is unsafe — it would let a legitimate in-flight create survive a user-intended close.)

### WR-02: `on_event` / `on_event_win` module callbacks are NOT wrapped in `catch_unwind` — a panicking module event closure kills the event loop (now reachable via CR-01)

**File:** `crates/mybox-core/src/app.rs:466-468` (`on_event`), `app.rs:473-476` (`on_event_win`)
**Issue:** Unchanged since the previous review, but its severity has risen: CR-01's CJK-keyword panic executes inside `on_event_win` (the frame loop → `ui::draw` → `draw_command_row` → `keyword_tag_job`), so the "panic kills the whole event loop with no recovery" scenario has a concrete, verified trigger. The palette closure contains many additional panic surfaces (`Mutex::lock().unwrap()`, `expect("ensure_winit_state ran")`, `egui_ctx.run`/`tessellate`/`raster::paint`).
**Fix:** Wrap both callbacks in `catch_unwind`, matching `handle_redraw` and the `AppEvent::Ui` dispatch:
```rust
if let Some(cb) = &state.spec.on_event_win {
    if let Some(w) = &state.window {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(w, &event)));
    }
}
```

### WR-03: Zero-command fallback block (144px) still clipped in the 128px window — `max(1)` achieved consistency, not correctness

**File:** `crates/modules/palette/src/lib.rs:181` (`summon_palette` height), `crates/modules/palette/src/ui.rs:216-227` (zero-command fallback), `ui.rs:59-67` (`window_height`)
**Issue:** Unchanged since the previous review. Both height sites compute `80 + 48·max(1, n)` = 128 for zero commands, but the fallback block (12 top margin + 48 input + 8 gap + 64 `SP_3XL` block + 12 bottom margin = 144px) is still clipped by ~16px. Latent today (the App always registers ≥4 builtins) but the zero-command state is explicitly supported by the UI code.
**Fix:** Use the `Empty` geometry (144px) when `commands.is_empty()` at summon and mirror the same conditional in `sync_window_geometry`, or shrink the fallback block to fit 128px.

## Info

### IN-01: `run_command` panics on the main thread if the worker thread cannot spawn

**File:** `crates/mybox-core/src/command.rs:245` (`.expect("spawn command runner thread")`)
**Issue:** Unchanged. Thread-spawn failure on the main-thread execute path panics the app instead of surfacing the D-05 Error state.
**Fix:** Handle `Builder::spawn` `Err` and hop the failure through `ui.run` so `finalize(gen, Err(...))` renders the Error state.

### IN-02: Opposite lock orderings on `SessionInner` and `egui_ctx` — safe today, fragile tomorrow

**File:** `crates/modules/palette/src/session.rs:493-506` (`ensure_winit_state`: state → egui_ctx); `crates/modules/palette/src/lib.rs:293-322` (frame loop: egui_ctx → state via `ui::draw`)
**Issue:** Unchanged. Both orderings are main-thread-only and never nest today, but the invariant is undocumented.
**Fix:** Document "never call `ensure_winit_state` while holding `egui_ctx()`", or clone the `egui::Context` before locking state so both paths take egui_ctx → state.

### IN-03: `PaletteHarness::realize_window` comment claims dropping the previous window closes it — the harness WindowManager keeps it alive

**File:** `crates/modules/palette/src/bin/palette_checks.rs:81-124`
**Issue:** Unchanged. The doc comment is inaccurate: `wm.register` retains an `Arc<Window>` for every round's window and nothing calls `wm.destroy`, so replaced windows stay alive (and visible) for the check duration. Harmless to assertions (`current_winit_id` filters stale events), but misleading and stacks live palette windows on the desktop during multi-round probes.
**Fix:** Call `self.wm.destroy(self.created_id.take())` before re-registering, or correct the comment.

### IN-04: `config_dir().unwrap_or_default()` degrades the config/log builtins to an empty path

**File:** `crates/mybox-core/src/app.rs:145-146`
**Issue:** Unchanged. On config-dir failure, `open_config` opens `""` and `open_log` opens a CWD-relative `logs/mybox.log` — silently wrong rather than failing loudly.
**Fix:** Keep the `Option` and `bail!` in the builtin runners when the config dir is unavailable.

### IN-05: 64-char query cap makes pasted long input visibly snap back with no feedback

**File:** `crates/modules/palette/src/ui.rs:175-207` + `crates/modules/palette/src/session.rs:290-301`
**Issue:** Unchanged. The TextEdit buffer is rebuilt every frame from the *truncated* `session.input()`, so pasting >64 chars shows the full paste for exactly one frame, then snaps back with no hint.
**Fix:** Use `egui::TextEdit::singleline(&mut text).char_limit(filter::MAX_QUERY_LEN)` so the cap is applied at the point of input.

### IN-06: New E2E probe band geometry and accent check are magic-number-coupled to the row layout and position-blind

**File:** `crates/modules/palette/src/bin/palette_checks.rs:2048-2066` (`accent_pixels_in_row_band`), used at `palette_checks.rs:2140-2160` and `palette_checks.rs:2195-2215`
**Issue:** The band (`y 68..116` = input 12..60 + 8 gap + row 1) and the exact `0xFF,0x60,0x00` pixel triple are hardcoded. Any future layout change (input height, row height, `PANEL_WIDTH`, ACCENT color) silently turns the assertion into either a false failure or — as CR-01 demonstrated — a false *pass*, because the probe asserts pixel *presence* in a band, never pixel *position*. The comment at L2053-2054 even mis-attributes the (buggy) evidence: "the keyword tag's matched glyphs ... render the pure ACCENT color".
**Fix:** Assert the ACCENT pixels at the expected x-range of the tag's matched chars (computed from `PANEL_WIDTH - text width`), or at minimum extract the band geometry and color into named constants shared with the layout arithmetic, and add a comment that presence-only assertions cannot detect a shift.

---

_Reviewed: 2026-08-17T06:43:32Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
