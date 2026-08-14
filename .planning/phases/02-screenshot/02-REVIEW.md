---
phase: 02-screenshot
reviewed: 2026-08-13T06:03:21Z
depth: standard
files_reviewed: 22
files_reviewed_list:
  - crates/modules/capture/Cargo.toml
  - crates/modules/capture/src/annotate.rs
  - crates/modules/capture/src/bin/capture_checks.rs
  - crates/modules/capture/src/capture.rs
  - crates/modules/capture/src/clipboard.rs
  - crates/modules/capture/src/lib.rs
  - crates/modules/capture/src/overlay.rs
  - crates/modules/capture/src/permission.rs
  - crates/modules/capture/src/selection.rs
  - crates/modules/capture/src/session.rs
  - crates/modules/capture/src/text.rs
  - crates/modules/capture/src/toolbar.rs
  - crates/modules/capture/tests/integration.rs
  - crates/modules/capture/tests/manual_checklist.md
  - crates/modules/test/src/lib.rs
  - crates/mybox-app/Cargo.toml
  - crates/mybox-app/src/main.rs
  - crates/mybox-core/src/app.rs
  - crates/mybox-core/src/bin/display_checks.rs
  - crates/mybox-core/src/context.rs
  - crates/mybox-core/src/lib.rs
  - crates/mybox-core/src/window.rs
findings:
  critical: 1
  warning: 5
  info: 5
  total: 11
status: issues_found
---

# Phase 02-screenshot: Code Review Report

**Reviewed:** 2026-08-13T06:03:21Z
**Depth:** standard
**Files Reviewed:** 22
**Status:** issues_found

## Summary

Reviewed the Phase 2 screenshot module (`mybox-capture`) and the small
`mybox-core` changes (the `on_draw` render chain, `WindowRequest::Redraw`,
`ModuleContext::bus()`, `UiThreadProxy`). The core architecture is sound: capture
runs on a named worker thread, results are marshalled to the main thread through
`UiThreadProxy`, and the session state is shared behind an `Arc<std::sync::Mutex>`
with careful capture-before-create ordering and drop-before-close teardown.
Raw-pixel handling (premultiply/unpremultiply, `crop_image` clamping, text
blending) is correct and bounds-checked — no memory-safety defects found, and no
`unsafe` blocks exist (the `objc2-core-graphics` permission calls are correctly
behind safe generated bindings).

The main concern is **cross-platform robustness of the text path**: `load_font()`
hard-panics on any non-macOS host (including Windows, which is a hard project
requirement) by `.expect()`-ing a macOS-only font path from the overlay draw
loop. This single panic, combined with the session mutex being held across the
draw and the `on_event` callback not being `catch_unwind`-wrapped, cascades into
a poisoned mutex and an eventual app crash. Secondary issues are a missing
re-entrancy guard (double-trigger leaks full-screen overlays) and one-way tool
selection (no path back to Select mode).

## Critical Issues

### CR-01: `load_font` unconditionally reads a macOS-only font path and panics on Windows

**File:** `crates/modules/capture/src/text.rs:22-23`

**Issue:** `load_font()` reads `/System/Library/Fonts/Supplemental/Arial.ttf` with
`.expect("system font Arial.ttf must be present on macOS (A4)")` and is not gated
by `#[cfg(target_os = "macos")]`. On Windows — a hard platform requirement of
this project — that path does not exist, so `std::fs::read` returns `Err` and
`load_font` panics. `load_font` is reached from the overlay draw loop
(`overlay.rs:132` via `draw_selection_overlay`, `overlay.rs:146` via
`draw_toolbar`, `toolbar.rs:81`, and `annotate.rs:133`), so **every** overlay
redraw with a selection panics on Windows. Worse, this panic occurs while the
session mutex is held (see WR-02), poisoning it; the next `on_event` input then
panics on `.lock().unwrap()` and crashes the app (see WR-01). The result is that
screenshot selection chrome, annotations, and the toolbar are entirely
non-functional on Windows after the first interaction.

**Fix:** Gate the macOS path and provide a non-panicking fallback (or propagate
the failure instead of `expect`-ing):

```rust
#[cfg(target_os = "macos")]
pub fn load_font() -> FontArc {
    static FONT: OnceLock<FontArc> = OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = std::fs::read("/System/Library/Fonts/Supplemental/Arial.ttf")
            .expect("system font Arial.ttf must be present on macOS (A4)");
        // ...
    }).clone()
}

#[cfg(not(target_os = "macos"))]
pub fn load_font() -> FontArc {
    // Windows fallback: a bundled/embedded font, or load a system font via a
    // cross-platform path (e.g. %WINDIR%\Fonts\arial.ttf) — never panic.
    // Until Phase 4, returning a safe default font (or making draw_text a no-op)
    // is preferable to aborting the frame.
}
```

At minimum, make `draw_overlay`/`draw_toolbar` tolerate a font-load failure so a
missing font degrades to "no text" rather than panicking.

## Warnings

### WR-01: `on_event` callback is not `catch_unwind`-wrapped (unlike `on_draw` and `Ui`)

**File:** `crates/mybox-core/src/app.rs:377-379`

**Issue:** `App::window_event` invokes the per-window `on_event` closure directly:

```rust
if let Some(cb) = &state.spec.on_event {
    cb(&event);
}
```

…but the draw closure is wrapped in `catch_unwind` (`app.rs:352`) and
`AppEvent::Ui` closures are too (`app.rs:405`). A panicking module `on_event`
(e.g. the `session.lock().unwrap()` after a poisoned mutex from CR-01) therefore
propagates out of `ApplicationHandler::window_event`, unwinding through the winit
event loop and terminating the process. The capture module's `on_event` is the
largest, most input-driven closure in the app, so it is the most likely to panic.

**Fix:** Wrap the callback the same way as the other paths:

```rust
if let Some(cb) = &state.spec.on_event {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(&event)));
}
```

### WR-02: Session mutex is poisoned by any panic-under-lock, and every access uses `.unwrap()`

**File:** `crates/modules/capture/src/session.rs` (throughout) and `crates/modules/capture/src/overlay.rs:116`

**Issue:** All session access is `self.state.lock().unwrap()`. `draw_overlay`
holds the lock for the entire frame — including `text::load_font()` (which can
panic, see CR-01) and annotation drawing — so a single panic while the lock is
held drops the guard mid-unwind and poisons the `std::sync::Mutex`. Every
subsequent `.lock().unwrap()` then panics, cascading the failure to unrelated
inputs and permanently wedging the session.

**Fix:** Use `.unwrap_or_else(|poisoned| poisoned.into_inner())` for lock access
(or `parking_lot::Mutex`, which is already a core dependency), and avoid doing
fallible work (font load) while the lock is held — snapshot what `draw_overlay`
needs under the lock, release it, then draw.

### WR-03: No re-entrancy guard — rapid double-trigger leaks full-screen overlays

**File:** `crates/modules/capture/src/lib.rs:196-232` and `crates/modules/capture/src/overlay.rs:100-104`

**Issue:** `start_capture` spawns a worker thread on every hotkey/menu trigger
with no "capture in progress" check. Two quick presses spawn two capture threads;
each calls `store_shots` + `create_overlays`, enqueuing `2N` overlay `Create`
requests while `pending_overlays` is *overwritten* to `N` (not `+= N`). The first
`N` `window-created` events drain `pending_overlays` to 0 and record `N` ids; the
remaining `N` events are ignored (pending already drained). On ESC/confirm only
`N` of the `2N` windows are destroyed, leaving `N` full-screen always-on-top
overlays on screen with **no tracked id and no way to dismiss them** (a second
ESC drains an empty id list). The user must force-quit the app.

**Fix:** Add an in-progress guard, e.g. an `AtomicBool` in `CaptureSession`
checked-and-set at the top of `start_capture` and cleared in `finish`/`cancel`
and on capture error:

```rust
if session.try_begin().is_err() { return; } // already capturing
```

Also consider having `store_shots` reset the prior selection/annotations/overlay
bookkeeping so a fresh capture never inherits stale state.

### WR-04: Tool selection is one-way — no path back to Select mode

**File:** `crates/modules/capture/src/toolbar.rs:39-47` and `crates/modules/capture/src/session.rs` (`current_tool`)

**Issue:** The toolbar exposes Confirm/Cancel/Undo/Rect/Arrow/Pen/Text but no
Select button ("no mode switch", D-03). `on_mouse_down` only starts a new drag
selection when `current_tool == Tool::Select` (`overlay.rs:264-267`). Once the
user picks any annotation tool, every subsequent press-drag starts an annotation,
so they can never start a *new* region selection again (only resize via handles).
`finish()` does not reset `current_tool`, so the trap persists across sessions.

**Fix:** Either add a Select button to the toolbar, or reset `current_tool` to
`Tool::Select` after a capture completes (see WR-05), or treat a drag that starts
on an existing handle/empty area as a new selection regardless of tool.

### WR-05: `finish()`/`cancel()` do not reset `current_tool` or `ctrl_down`

**File:** `crates/modules/capture/src/session.rs:386-398`

**Issue:** `finish()` (and `cancel()`, which delegates to it) clears shots,
selection, annotations, and overlay bookkeeping, but leaves `current_tool` and
`ctrl_down` untouched. A stale `ctrl_down == true` means a plain `z` press on the
next capture triggers an unintended undo (the overlay only sees `ModifiersChanged`
while it is live, so a modifier released between sessions may never be observed).
A stale `current_tool` compounds WR-04 by carrying the last annotation tool into
the next capture.

**Fix:** Reset both in `finish()`:

```rust
state.current_tool = Tool::Select;
state.ctrl_down = false;
```

## Info

### IN-01: `tool_action` Confirm/Cancel arms are dead code with stale logs

**File:** `crates/modules/capture/src/session.rs:291-296`

**Issue:** `handle_overlay_event` routes `ToolAction::Confirm`/`ToolAction::Cancel`
directly to `confirm_and_copy`/`cancel_overlays` (`overlay.rs:244-245`), so the
`tool_action` arms that log "clipboard copy wired in 02-04" are never reached and
the message is now stale/misleading.

**Fix:** Remove the dead arms (or forward them to the real teardown path) so the
code reflects the actual wiring.

### IN-02: `ConfirmSnapshot.monitor_index` is stored but never read

**File:** `crates/modules/capture/src/session.rs:41-46` and `crates/modules/capture/src/overlay.rs:334-379`

**Issue:** `confirm()` fills `monitor_index` (and uses it internally to select the
shot), but `confirm_and_copy` only consumes `rect`, `shot`, and `annotations`.
The field is redundant on the snapshot.

**Fix:** Drop the field from `ConfirmSnapshot` (keep the local index in
`confirm()`), or use it to assert/document which monitor is being copied.

### IN-03: Defensive `.expect()` panics in the draw path for zero-size inputs

**File:** `crates/modules/capture/src/overlay.rs:168` and `crates/modules/capture/src/overlay.rs:179`

**Issue:** `composite_frame` uses `Rect::from_xywh(0.0, 0.0, w, h).expect("non-zero overlay")`
and `blit_shot` uses `IntSize::from_wh(w, h).expect("capture has non-zero dims")`.
These are effectively unreachable because `TinySkiaSoftbufferRenderer::new`
rejects zero-size pixmaps, but they contradict the module's own T-2-05
"never panic in draw" convention.

**Fix:** Replace the `.expect` with the `if let Some(..) { .. }` guards already
used elsewhere (e.g. `fill_rect_safe`), returning silently on a degenerate size.

### IN-04: `apply_handle_drag` can produce negative coordinates for sub-minimum selections

**File:** `crates/modules/capture/src/selection.rs:87-113`

**Issue:** A click-without-drag produces a zero-size `Selected` selection. Dragging
a handle then computes e.g. `r.y0 = pos.y.min(r.y1 - MIN_SELECTION)`, and
`r.y1 - MIN_SELECTION` is negative when `r.y1 < 4.0`, so `y0`/`x0` can go
negative. Downstream clamps (`confirm_and_copy` clamps to `[0, iw/ih]`) and
`fill_rect_safe` prevent memory unsafety, but the size label and mask regions can
be briefly wrong.

**Fix:** Clamp handle drags to non-negative coordinates (and/or enforce
`MIN_SELECTION` at drag end) so the stored rect stays within the monitor.

### IN-05: `draw_overlay` holds the session lock across the full frame

**File:** `crates/modules/capture/src/overlay.rs:116-148`

**Issue:** The draw closure locks the session mutex and holds it through font
loading, full-frame compositing, and text rasterization — the longest critical
section in the capture path. No deadlock today (draw and `on_event` run
sequentially on the main thread; only `window_created` contends from the worker
thread), but it is fragile and widens the poisoning blast radius (WR-02).

**Fix:** Snapshot the needed fields (shot clone, selection, annotations clone,
tool) under the lock, release it, then composite/draw against the snapshot.

---

_Reviewed: 2026-08-13T06:03:21Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
