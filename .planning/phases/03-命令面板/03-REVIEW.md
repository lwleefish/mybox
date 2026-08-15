---
phase: 03-命令面板
reviewed: 2026-08-15T08:56:45Z
depth: standard
files_reviewed: 9
files_reviewed_list:
  - crates/modules/palette/src/bin/palette_checks.rs
  - crates/modules/palette/src/lib.rs
  - crates/modules/palette/src/raster.rs
  - crates/modules/palette/src/session.rs
  - crates/modules/palette/src/ui.rs
  - crates/modules/palette/tests/integration.rs
  - crates/mybox-core/src/app.rs
  - crates/mybox-core/src/command.rs
  - crates/mybox-core/src/window.rs
findings:
  critical: 0
  warning: 4
  info: 5
  total: 9
status: issues_found
---

# Phase 03: Code Review Report

**Reviewed:** 2026-08-15T08:56:45Z
**Depth:** standard
**Files Reviewed:** 9
**Status:** issues_found

## Summary

Adversarial review of the complete Phase 3 command-palette implementation across the palette module (`lib.rs`, `session.rs`, `raster.rs`, `ui.rs`, the `palette_checks` binary, integration tests) and the core support files (`app.rs`, `command.rs`, `window.rs`).

The implementation is overall high-quality and heavily regression-locked:

- The **rasterizer** (`raster.rs`) is the most scrutinized piece and checks out: the GAP-2 UV-based dispatch follows the epaint contract (`WHITE_UV` + default texture = solid; anything else = textured), the premultiplied blend math was hand-verified (the over-blend cannot overflow u8 because the premultiplied invariant `rgb ≤ a` is preserved through the straight→premultiplied texture multiply), bounds checks are in place, and the −0.5 texel-center UV offset matches GL_LINEAR semantics.
- The **session state machine** (`session.rs`) guards are sound: generation-guarded `finalize`, `pending_close` build-destroy pairing, bounds-checked in-place texture patching, and the `geometry_revision` counter correctly fixes the old WR-01/WR-02 height-sync and framebuffer-cover regressions.
- The **GAP-1 pairing path** (`app.rs` `create_window` → `on_created` before the broadcast, drained in the same `about_to_wait` pass) is correct, and the hotkey Pressed-only filter is regression-locked.
- No secrets, no injection surfaces, no unsafe blocks without SAFETY documentation (the two raw-window-handle `unsafe` blocks in `window.rs` are commented and main-thread-only).

No critical issues. Four warnings follow: one is a genuine GAP-7 regression (IME enable is per-session, not per-window — provable from the code and verified against the vendored egui-winit 0.30 source), one is a session wedge on window-creation failure, and two are latent edge-case inconsistencies.

## Warnings

### WR-01: GAP-7 IME enable and the egui-winit State are per-session, not per-window — IME is likely dead on the second (and later) summons

**File:** `crates/modules/palette/src/session.rs:130-149` (`summon`), `session.rs:478-503` (`ensure_winit_state`)
**Issue:** `ensure_winit_state`'s doc contract says the explicit `set_ime_allowed(true)` runs "the first time the window ever receives an event", and GAP-7 exists because egui-winit's focus-driven multi-frame IME sequence is unreliable on the desktop. But both gates are per-*session*, never reset by `summon()` or `close()`:

1. `ime_allowed` starts `false` and is set `true` once, forever. Every re-summoned window therefore skips the explicit enable (`enable_ime == false`).
2. `winit_state` is created on the first window's first event and **never torn down** — re-summoned windows reuse the `egui_winit::State` built against the *old* window, including its `allow_ime` debounce flag.

Verified against the vendored egui-winit 0.30.0 source (`lib.rs:848-852`): `handle_platform_output` calls `window.set_ime_allowed()` **only on an `allow_ime` transition**. The standard flow — summon → TextEdit focus (State.allow_ime flips to `true`) → ESC close from Idle → re-summon → new window, TextEdit focused again — produces no transition (`true → true`), so **no code path ever calls `set_ime_allowed(true)` on the second window**, and winit's macOS backend defaults IME to disabled per window. The user cannot type Chinese in the re-summoned palette. (The `five_summon_esc` E2E and human UAT 10 only cover the first summon.) The reused State also carries stale `screen_rect`/focus from the old window — mostly self-healing via Resized events, but the IME flag is not.

**Fix:** Reset the per-window bits on `summon()` (and/or `close()`):
```rust
// in summon(), next to the existing modifiers reset:
inner.ime_allowed = false;
inner.winit_state = None;
```
This makes `ensure_winit_state` rebuild the State per window and re-run the explicit IME enable per window — matching the documented GAP-7 contract exactly.

### WR-02: Zero-command summon height is computed inconsistently — window jumps 80 → 128 and the fallback block is clipped

**File:** `crates/modules/palette/src/lib.rs:176` vs `lib.rs:491`
**Issue:** Two height computations disagree on the minimum row count. `summon_palette` sizes the initial window with `ui::window_height(PaletteState::Idle, all.len())` — with an empty registry that is `80 + 48·0 = 80`. The frame loop's `sync_window_geometry` uses `session.filtered().len().max(1)` — for the same state that is `80 + 48·1 = 128`. On the first frame the window jumps 80 → 128 logical px. Worse, the zero-command fallback (`ui.rs:216-227`, "没有可用的命令") needs 48 (input) + 8 (gap) + 64 (block) + 24 (margins) = 144 logical px, so even at 128 its bottom ~16px is clipped. Unreachable in production today only because `AppBuilder::build` always registers the four builtins — but the code explicitly supports the zero-command state (the UI fallback exists), so the mismatch is a latent defect.

**Fix:** Use the same min-count rule at both sites (the `max(1)` semantics is the correct one for the geometry table), e.g. in `summon_palette`:
```rust
let height = ui::window_height(PaletteState::Idle, all.len().max(1));
```

### WR-03: A failed `App::create_window` wedges the palette session permanently — no recovery path exists

**File:** `crates/mybox-core/src/app.rs:518-521` (`about_to_wait` logs the error only), `crates/modules/palette/src/session.rs:183-208` (`has_live_window`/`close`)
**Issue:** If window creation fails (e.g. `TinySkiaSoftbufferRenderer::new` fails — the renderer is built *before* `spec.on_created.take()` runs at `app.rs:402-406`), the error is only logged. The session was already moved to `Idle` by `summon`, `window_id` is `None`, and `on_created` never fires, so:

- `has_live_window()` stays `true` forever (state `Idle`).
- The next hotkey toggle takes the close branch: `close()` sees `was_visible == true`, no window id → sets `pending_close = true` and returns `None` — no window will ever arrive to consume it.
- Every subsequent toggle hits the `else` branch of `close()` (state already `Hidden`, no id, `was_visible == false`) → returns `None`, `pending_close` stays set, and `has_live_window()` keeps returning `true` — so `toggle_palette` can **never summon again**. The palette is dead until app restart.

**Fix:** Give the session a failure path and call it from `about_to_wait`'s error arm, e.g. add `SessionInner::on_create_failed()` (clears `pending_close`, moves `Idle → Hidden`) and, when `create_window` fails, the App cannot know the session — so the minimal robust option is for `WindowSpec` to carry an `on_create_failed` callback, or for `summon_palette`'s session state to reset via a timeout/generation check. At minimum, `close()` should also clear `pending_close` when `was_visible == false` so a later toggle can re-summon.

### WR-04: `sync_window_geometry` resizes the window to 1px when the frame runs in `Hidden` state

**File:** `crates/modules/palette/src/lib.rs:486-504`
**Issue:** `sync_window_geometry` computes `window_height(session.state(), ...)`; for `PaletteState::Hidden` that is `0.0`, and `physical_h.max(1)` turns it into a **1px** `request_inner_size` plus a 1px framebuffer reallocation. This is reachable: the click-execute path for `hide_before_execute` commands (`ui.rs:415-419` → `execute::execute` → `set_executing` bumps `geometry_revision` → `close()` moves to `Hidden`) all happens *inside* the frame, and the same frame's revision check then runs the sync. The 1px resize is transient (the Destroy drains moments later) and the next summon reinstalls the framebuffer, so there is no lasting corruption — but the window is briefly resized to 1px and the framebuffer reallocated on every `capture.start` execution.

**Fix:** Early-return when there is nothing to size:
```rust
fn sync_window_geometry(...) {
    if session.state() == PaletteState::Hidden {
        return;
    }
    ...
}
```

## Info

### IN-01: `run_command` panics on the main thread if the worker thread cannot spawn

**File:** `crates/mybox-core/src/command.rs:239-245`
**Issue:** `.expect("spawn command runner thread")` runs synchronously in the `execute` chain (`on_palette_key` Enter → `execute::execute` → `run_command`), which is the main-thread event path. Thread-spawn failure (resource exhaustion) panics the whole app instead of surfacing the D-05 Error state.
**Fix:** Use `std::thread::Builder::spawn(...)` and on `Err`, hop the failure through `ui.run` so `finalize(gen, Err(...))` renders the Error state instead of panicking.

### IN-02: Opposite lock orderings on `SessionInner` and `egui_ctx` — safe today, fragile tomorrow

**File:** `crates/modules/palette/src/session.rs:481-490` (state → egui_ctx) vs `crates/modules/palette/src/lib.rs:288-292` (egui_ctx guard held while `ui::draw` locks state)
**Issue:** `ensure_winit_state` locks the session state and then clones the egui context (state → egui_ctx); the frame loop holds the `egui_ctx()` guard across `egui_ctx.run(...)`, inside which `ui::draw` takes the state lock (egui_ctx → state). The two orderings never nest today (both are main-thread sequential and `ensure_winit_state` releases before the run call), so no live deadlock — but any future code that calls a session method from inside a draw closure while also creating winit state would deadlock. Note that WR-01's fix (rebuilding `winit_state` per window) widens the `ensure_winit_state` lock window.
**Fix:** Document the invariant on both methods ("never call `ensure_winit_state` while holding `egui_ctx()`"), or build the egui-winit State outside the state lock entirely (clone ctx first, then lock).

### IN-03: `PaletteHarness::realize_window`'s comment claims dropping the previous window closes it — the harness WindowManager keeps it alive

**File:** `crates/modules/palette/src/bin/palette_checks.rs:82-124`
**Issue:** The comment on `realize_window` says replacing `self.window` "drops it closes it", but `wm.register(id, ..., Some(Arc::clone(&window)), ...)` retains an Arc for every round's window in the self-managed `WindowManager`, and nothing ever calls `wm.destroy`. Replaced windows therefore stay alive (and visible on screen) for the duration of the check. Test-harness-only and harmless to the assertions, but the comment is inaccurate and multi-round checks stack up to 5 live palette windows on the desktop.
**Fix:** Either call `self.wm.destroy(self.created_id.take()...)` before re-registering, or correct the comment.

### IN-04: `config_dir().unwrap_or_default()` degrades the config/log builtins to an empty path

**File:** `crates/mybox-core/src/app.rs:145-146`
**Issue:** On config-dir failure, `open_config` opens `""` (macOS `open` with an empty argument errors → the command lands in the Error state) and `open_log` opens the relative path `logs/mybox.log` from the process CWD. The fallback is silently wrong rather than failing loudly.
**Fix:** Keep the `Option` and have the builtin runners `bail!` with a clear message when the config dir is unavailable, instead of falling back to a bogus path.

### IN-05: 64-char query cap makes pasted long input visibly snap back with no feedback

**File:** `crates/modules/palette/src/ui.rs:175-207` + `crates/modules/palette/src/session.rs:275-301`
**Issue:** The TextEdit buffer is rebuilt every frame from the *truncated* `session.input()` (Security V5), so pasting >64 chars shows the full paste for exactly one frame and then snaps back to 64 chars — no hint that the input was capped. Spec-compliant (the bound is required), but the silent truncation is a UX wart and can fight IME preedit length.
**Fix:** Clamp at the widget level too (truncate `text` before it is edited / use a `TextEdit` max-length via `char_limit`) so the cap is visible at the point of input rather than a frame later.

---

_Reviewed: 2026-08-15T08:56:45Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
