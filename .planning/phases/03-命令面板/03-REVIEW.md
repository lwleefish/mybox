---
phase: 03-命令面板
reviewed: 2026-08-15T01:56:00Z
depth: standard
files_reviewed: 19
files_reviewed_list:
  - crates/mybox-core/src/command.rs
  - crates/mybox-core/src/module.rs
  - crates/mybox-core/src/context.rs
  - crates/mybox-core/src/app.rs
  - crates/mybox-core/src/window.rs
  - crates/mybox-core/src/error.rs
  - crates/mybox-core/src/lib.rs
  - crates/modules/palette/src/lib.rs
  - crates/modules/palette/src/session.rs
  - crates/modules/palette/src/position.rs
  - crates/modules/palette/src/fonts.rs
  - crates/modules/palette/src/raster.rs
  - crates/modules/palette/src/ui.rs
  - crates/modules/palette/src/filter.rs
  - crates/modules/palette/src/execute.rs
  - crates/modules/palette/src/bin/palette_checks.rs
  - crates/modules/palette/tests/integration.rs
  - crates/modules/capture/src/lib.rs
  - crates/mybox-app/src/main.rs
findings:
  critical: 0
  warning: 4
  info: 7
  total: 11
status: issues_found
---

# Phase 03: Code Review Report

**Reviewed:** 2026-08-15T01:56:00Z
**Depth:** standard
**Files Reviewed:** 19
**Status:** issues_found

## Summary

Reviewed the full Phase 03 command-palette delivery: the core command model (`command.rs`, `module.rs`, `context.rs`, `app.rs`, `window.rs`, `error.rs`, `lib.rs`), the palette module (session state machine, fuzzy filter, keyboard router, egui rendering, rasterizer, positioning, fonts, execution lifecycle), the E2E harness (`palette_checks.rs` + `integration.rs`), the capture command contribution, and the app entry point.

Overall the implementation is high quality: the generation-guarded session state machine is well-designed (Pitfall 3 discipline carried through), the build-destroy window pairing prevents orphans, the filter/selection mapping through `filtered` space is correct and locked down by tests, and the security posture is clean — no shell invocation (T-3-07 honored: `open`/`explorer` with a single path argument), no injection vectors, no secrets, and the only `unsafe` blocks are the documented ObjC NSView casts following the Phase 2 pattern.

Four warnings were found: (1) the re-centering math in `sync_window_geometry` clamps negative monitor coordinates, breaking multi-monitor setups with displays left of/above the primary; (2) a zero-command palette gets an 80px window while its fallback content needs ~144px (clipped); (3) no panic containment around the command runner — a panicking runner leaves the palette permanently stuck in Executing; (4) the `hide_before_execute` "panel never in screenshots" guarantee rests on FIFO queue ordering that does not actually synchronize window teardown with the capture snapshot, and a hotkey re-summon during an in-flight capture can put a fresh panel on screen mid-capture.

No critical (security/data-loss/crash) issues found.

---

## Warnings

### WR-01: `sync_window_geometry` clamps negative monitor coordinates — re-centering jumps to the wrong screen

**File:** `crates/modules/palette/src/lib.rs:429-435`
**Issue:** The re-centering math computes the monitor-relative center, then clamps with `x.max(0)` / `y.max(0)`:

```rust
let x = mpos.x + (msize.width.saturating_sub(new_size.width) as i32) / 2;
let y = mpos.y + (msize.height.saturating_sub(new_size.height) as i32) / 2;
window.set_outer_position(winit::dpi::PhysicalPosition::new(x.max(0), y.max(0)));
```

Monitors to the left of (or above) the primary have negative `position().x`/`.y` (e.g. x = -1920). The clamp forces the window to x/y ≥ 0, which lands on the primary monitor — not centered on the monitor the palette lives on. The initial summon position is correct (`position::compute_geometry` does not clamp), but every height change (typing a filter query, Idle→Executing, Enter→Error — all of which call `sync_window_geometry` when `prev_state != session.state()`) re-centers and teleports the panel onto the primary screen.
**Fix:**
```rust
// Negative coordinates are valid for secondary monitors — never clamp.
let x = mpos.x + (msize.width.saturating_sub(new_size.width) as i32) / 2;
let y = mpos.y + (msize.height.saturating_sub(new_size.height) as i32) / 2;
window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
```

### WR-02: Zero-command palette gets an 80px window while its fallback block needs ~144px

**File:** `crates/modules/palette/src/lib.rs:185` and `crates/modules/palette/src/ui.rs:56-64, 190-201`
**Issue:** `summon_palette` computes `ui::window_height(PaletteState::Idle, all.len())`. With zero registered commands, `window_height(Idle, 0)` returns `80.0` (the `80 + 48·n` formula with n=0). But the zero-command fallback draws: input row (48) + 8px gap + the "没有可用的命令" state block (64px) + 24px panel margins ≈ 144px of content. The window is created at 80px and the fallback block is clipped. The Empty/Error states correctly use the fixed 144 height — only the Idle-with-zero-commands path is broken. (Reachable today via the public `summon_palette` API with an empty `CommandRegistry`; the production app always registers ≥5 commands, but the zero-command state is explicitly designed for in the UI-SPEC.)
**Fix:**
```rust
// ui.rs — treat zero visible rows as one row so the fallback block fits:
pub fn window_height(state: PaletteState, visible: usize) -> f32 {
    let n = visible.max(1).min(10) as f32;
    // ...
}
// lib.rs — same guard at summon:
let height = ui::window_height(PaletteState::Idle, all.len().max(1));
```

### WR-03: No panic containment around the command runner — a panicking runner leaves the palette stuck in Executing

**File:** `crates/mybox-core/src/command.rs:229-243`, `crates/modules/palette/src/execute.rs:54-76`
**Issue:** `run_command` drives the runner with `pollster::block_on((cmd.runner)())` and the completion (`on_done`, which calls `session.finalize`) only runs on the success path. If the runner future panics (the doc comment on `Command` says "the runner must never panic… caught nowhere"), the worker thread unwinds, `on_done` is dropped, and the palette remains in `Executing` permanently: input disabled, ESC ignored (by design in Executing), Enter a no-op. The only recovery is the global hotkey toggle. This is not purely theoretical — `capture::start_capture` contains `.expect("spawn capture worker thread")` (capture/src/lib.rs:307) which panics the runner thread on spawn failure, and any future module runner can panic. Additionally `run_command`'s own `.expect("spawn command runner thread")` runs on the *main* thread inside `on_event_win` (which `App::window_event` does not wrap in `catch_unwind`), so spawn failure kills the whole event loop.
**Fix:**
```rust
.spawn(move || {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pollster::block_on((cmd.runner)())
    }))
    .unwrap_or_else(|_| Err(anyhow::anyhow!("command runner panicked")));
    ui.run(Box::new(move || on_done(result)));
})
```
(and replace the spawn `expect` with a returned error or a guarded log so the main thread cannot die here).

### WR-04: `hide_before_execute` does not actually synchronize window teardown with the capture snapshot

**File:** `crates/modules/palette/src/execute.rs:44-51`, `crates/modules/palette/src/lib.rs:159-171`, `crates/modules/capture/src/lib.rs:290-307`
**Issue:** The "panel can never appear in screenshots" guarantee (UI-SPEC lifecycle rule 1) rests on enqueuing `Destroy` before the runner starts. That FIFO ordering guarantees Destroy-before-*Create*, but **not** Destroy-before-*capture*: the capture runs on a freshly spawned worker thread with no synchronization against the main thread draining the `WindowRequest::Destroy` in `about_to_wait`. A delayed main loop can still photograph the live panel. Second hole: after `close()` the session is `Hidden` with no window id and no pending close, so `has_live_window()` returns false — a second hotkey press during the (hundreds-of-ms) capture summons a *fresh* palette while xcap is photographing, putting the new panel in the screenshot. The capture module's `begin_capture` guard does not cover the palette module.
**Fix:** Suppress re-summon while a `hide_before_execute` command is in flight — e.g., track `suppress_summon`/`capture_in_flight` in `PaletteSession` (set on Enter for `hide_before_execute` commands, cleared by the finalize hop), and have `toggle_palette` ignore summon while it is set. For the teardown-vs-snapshot ordering, the capture runner should wait for a destroy-acknowledged signal (or the App should destroy hide-first windows synchronously on the main thread before the runner is released).

---

## Info

### IN-01: Runner thread names exceed the 15-byte pthread limit

**File:** `crates/mybox-core/src/command.rs:236-237`
**Issue:** `format!("mybox-cmd-{}", cmd.id)` produces names like `mybox-cmd-capture.start` (23 bytes). macOS/Linux `pthread_setname_np` silently truncates to 15 bytes + NUL, so the names are mangled (and the `run_command_runs_runner_on_named_thread` test's exact-match assertion will fail on those platforms once ids exceed ~7 chars).
**Fix:** Truncate the id portion (e.g. `cmd.id.chars().take(7).collect::<String>()`) or accept `&cmd.id[..min(7, len)]` before formatting.

### IN-02: `config_dir().unwrap_or_default()` silently produces a relative log path

**File:** `crates/mybox-core/src/app.rs:145-146`
**Issue:** If `config_dir()` ever fails, `config_dir` becomes `PathBuf::new()` and `log_path` becomes the relative `"logs/mybox.log"` — the `builtin.open_log` command would then open a wrong file (or fail) instead of surfacing the error. (`main.rs` calls `config_dir()?` first so this is mostly unreachable in production, but the fallback hides errors in any other embedding of `AppBuilder::build`.)
**Fix:** Propagate the error: `let config_dir = crate::config::config_dir().map_err(|e| anyhow::anyhow!("config dir: {e}"))?;`

### IN-03: CommandRegistry does not enforce the non-empty name/description contract

**File:** `crates/mybox-core/src/command.rs:60-66`
**Issue:** SPEC req 1 states every command has a non-empty name and description, and the doc comment repeats it — but `CommandRegistry::register` only checks for duplicate ids. A module can register a command with an empty name/description, producing empty rows and degenerate skim choices (empty-string `choice` in `fuzzy_indices`). Only the builtins' unit test asserts non-emptiness.
**Fix:** In `register`, validate `!cmd.name.is_empty() && !cmd.description.is_empty()` and return `MyboxError::Command("name/description must be non-empty")`.

### IN-04: `builtin.restart` spawns `current_exe()` — fails inside a macOS .app bundle

**File:** `crates/mybox-core/src/command.rs:154-170`
**Issue:** When mybox ships as a bundled `.app`, `current_exe()` points at the bare binary inside `Contents/MacOS/`, which generally cannot be launched standalone (bundle resources/plist missing). The child would fail to start and the parent exits anyway — a broken "restart". Documented as dev-mode behavior (D-13), but worth a Phase 4 item to resolve the bundle executable path.
**Fix:** On macOS resolve the `Foo.app` bundle root (via `objc2_foundation::NSBundle` or `../..` from the exe) and `open`/`NSWorkspace`-launch the bundle instead of the inner binary.

### IN-05: Windows log opener uses `explorer <file>` — inconsistent for files

**File:** `crates/mybox-core/src/command.rs:202-208`
**Issue:** `platform_opener` on Windows runs `explorer <path>`. For directories (open_config) this works, but `explorer` with a *file* argument (open_log) may open the parent folder in a new window instead of the file with its default app. Already anticipated as a Phase 4 Windows item; flagged here for tracking.
**Fix:** On Windows, dispatch on `path.is_dir()` — dir → `explorer`, file → `cmd /c start "" <path>` (quoted) or the shell-free `ShellExecuteW` equivalent.

### IN-06: A failing `hide_before_execute` command's error is invisible

**File:** `crates/modules/palette/src/execute.rs:57-76`
**Issue:** For `hide_before_execute` commands the panel closes before the runner runs, so a runner `Err` hits the "wrong state" branch of `finalize` (state already `Hidden`) and only a log line is produced — the in-panel Error block (D-05) can never display for this command class. A failed capture therefore gives the user no visible feedback (the capture module logs internally, but palette-level feedback is impossible by design). Consider whether the SPEC intends an error surface for this class (e.g. toast/notification) — worth confirming before Phase 4.

### IN-07: IME commits can bypass the Error-state "any key closes" contract

**File:** `crates/modules/palette/src/ui.rs:152-186`, `crates/modules/palette/src/lib.rs:356-406`
**Issue:** In `Error` state the input `TextEdit` is still constructed (only `Executing` uses the static branch). The `on_palette_key` router swallows all `KeyboardInput` before egui, so physical typing closes the panel as specified — but winit `Ime` events are not `KeyboardInput` and flow through `on_window_event` into the TextEdit; an IME commit then triggers `session.set_input`, silently transitioning `Error → Filtering/Empty` instead of closing. Edge case (CJK IME users committing text during the error display), but a contract inconsistency.
**Fix:** Gate the TextEdit branch on `state != PaletteState::Error` (render the input statically in Error too), or early-return in `set_input` when the state is `Error`.

---

_Reviewed: 2026-08-15T01:56:00Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
