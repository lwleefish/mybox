---
phase: 01-framework
reviewed: 2026-08-12T00:00:00Z
depth: standard
files_reviewed: 24
files_reviewed_list:
  - .gitignore
  - Cargo.lock
  - Cargo.toml
  - crates/modules/test/Cargo.toml
  - crates/modules/test/src/lib.rs
  - crates/mybox-app/Cargo.toml
  - crates/mybox-app/src/main.rs
  - crates/mybox-app/tests/manual_checklist.md
  - crates/mybox-core/Cargo.toml
  - crates/mybox-core/src/app.rs
  - crates/mybox-core/src/bin/display_checks.rs
  - crates/mybox-core/src/config.rs
  - crates/mybox-core/src/context.rs
  - crates/mybox-core/src/error.rs
  - crates/mybox-core/src/event.rs
  - crates/mybox-core/src/hotkey.rs
  - crates/mybox-core/src/lib.rs
  - crates/mybox-core/src/module.rs
  - crates/mybox-core/src/renderer/mod.rs
  - crates/mybox-core/src/renderer/tiny_skia_softbuffer.rs
  - crates/mybox-core/src/tray.rs
  - crates/mybox-core/src/window.rs
  - crates/mybox-core/tests/integration.rs
findings:
  critical: 0
  warning: 10
  info: 8
  total: 18
status: issues_found
---

# Phase 1: Code Review Report

**Reviewed:** 2026-08-12
**Depth:** standard
**Files Reviewed:** 24
**Status:** issues_found

## Summary

Reviewed the mybox Phase-1 framework skeleton: the core (`mybox-core`), the app
entry point (`mybox-app`), the test module (`mybox-test`), the renderer backend,
and the display/integration harness. The workspace compiles cleanly
(`cargo check --workspace --all-targets` passes; the only compiler warnings are
two unused-must-use `Result`s in `module.rs` tests). The architecture is sound:
the module boundary (FRMW-02), the event bus worker-thread dispatch with
`catch_unwind` protection (T-1-04), the main-thread window creation flow (W2/W3),
and the config first-run generation are all well-structured and heavily tested.

**Security:** No critical/security findings. No `unsafe` blocks, no injection
surfaces, no hardcoded secrets, no dangerous deserialization; the only external
inputs are config TOML, hotkey strings, and menu ids, all of which are parsed
strictly and never used to execute code.

No CRITICAL/BLOCKER findings. The warnings below are functional gaps and
robustness defects — most notably a tray menu item and an "exit" hotkey that are
wired to nothing, an unwired `Renderer::draw` content path in the running app,
stale-pixel risk in the present pipeline, and several documented-but-unsafe API
behaviors. These do not crash or lose data in the Phase-1 happy path, but
several will bite as soon as a real (Phase-2/3) module or the Windows target
arrives.

## Warnings

### WR-01: Tray menu item "打开测试窗口" has no consumer — clicking it does nothing

**File:** `crates/modules/test/src/lib.rs:44-77`
**Issue:** `TestModule::menu_items()` contributes a tray item with id
`test.open_window`, and the App forwards every tray click as a
`menu.triggered` bus event with a `{"menu_id": ...}` payload
(`crates/mybox-core/src/app.rs:304-310`). But `TestModule::init` subscribes only
to `hotkey.triggered` — nothing anywhere subscribes to `menu.triggered`. A user
clicking the tray item sees zero response, which is exactly the interaction the
walking skeleton exists to prove (the menu id even advertises the intent).
**Fix:** subscribe to `menu.triggered` in `TestModule::init` and reuse the same
window-creation logic:
```rust
let windows2 = ctx.windows().clone();
ctx.on(
    EventFilter::kind("core", "menu.triggered"),
    Box::new(move |e| {
        if let EventPayload::Module(v) = &e.payload {
            if v.get("menu_id").and_then(|m| m.as_str()) == Some("test.open_window") {
                windows2.create(WindowSpec {
                    kind: WindowKind::Panel,
                    title: "mybox test".to_string(),
                    inner_size: Some((400, 300)),
                    ..Default::default()
                });
            }
        }
    }),
);
```

### WR-02: Default `exit` hotkey (Cmd+Shift+Q) is registered but has no consumer

**File:** `crates/mybox-core/src/config.rs:83-86` (default), `crates/mybox-core/src/app.rs:285-299` (dispatch)
**Issue:** First-run config generation writes `exit = "Cmd+Shift+Q"` into
`[hotkeys]`. The App registers it, and on trigger `on_hotkey` emits
`hotkey.triggered` with `action = "exit"`. No module or framework code handles
`action == "exit"`; `FrameworkEvent::AppExit` is never emitted and
`event_loop.exit()` is never called. Pressing the advertised quit hotkey does
nothing, and users will reasonably expect it to quit.
**Fix:** either drop the default binding, or wire it — in `on_hotkey`, emit
`AppExit` for `action == "exit"`, and in `user_event` handle `AppExit` by calling
`_el.exit()`. (The doc comment at `app.rs:26-30` explicitly notes the quit path
is a v2 item on non-macOS, but the config default is shipped *now* on all
platforms and should not advertise a dead binding.)

### WR-03: `present()` leaves stale/uninitialized pixels when the buffer is larger than the pixmap

**File:** `crates/mybox-core/src/renderer/tiny_skia_softbuffer.rs:75-81`
**Issue:** `count = bw * bh` bounds the destination index, but the source
iterator `px.chunks_exact(4)` yields at most `pixmap_len` chunks. When the
softbuffer buffer is larger than the pixmap (transient resize races, DPI
rounding, or the zero-size resize guard at lines 58-63 keeping the old pixmap),
the tail `out[pixmap_len..bw*bh]` is never written and retains previous-frame or
uninitialized content — garbage on screen. The comment at line 74 claims "copy
only as many pixels as the buffer actually has", which the code does not
achieve in the larger-buffer direction.
**Fix:** iterate over the destination and clamp the source:
```rust
let px = self.pixmap.data();
let out = &mut *buffer;
for (i, slot) in out.iter_mut().enumerate() {
    let off = i * 4;
    let (r, g, b, a) = if off + 3 < px.len() {
        (px[off], px[off + 1], px[off + 2], px[off + 3])
    } else {
        (0, 0, 0, 0)
    };
    *slot = premul_rgba_to_u32(r, g, b, a);
}
```

### WR-04: `ConfigCenter::set` silently drops the write when the section is not a table

**File:** `crates/mybox-core/src/config.rs:115-123`
**Issue:** If `table[module]` already holds a non-table value (e.g. a scalar), the
`entry(...)` call returns it unchanged, `as_table_mut()` returns `None`, and the
`insert` is skipped with no error — the caller believes the value was stored but
`get` later returns the old/nothing. Silent partial failure on the module config
API that every future module will use.
**Fix:** make `set` fallible or overwrite the collision:
```rust
pub fn set(&self, module: &str, key: &str, value: toml::Value) -> Result<()> {
    let mut table = self.table.write();
    let entry = table.entry(module.to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    match entry {
        toml::Value::Table(section) => { section.insert(key.to_string(), value); Ok(()) }
        _ => Err(MyboxError::ConfigParse(format!("section '{module}' is not a table"))),
    }
}
```

### WR-05: App's `RedrawRequested` path never calls `Renderer::draw` — no content pipeline in the running app

**File:** `crates/mybox-core/src/app.rs:371-375`
**Issue:** `create_window` calls `window.request_redraw()`; on `RedrawRequested`
the App only calls `state.renderer.present()`. Nothing in the shipped app ever
calls `Renderer::draw` (a grep across the crates finds `draw` invoked only by the
`display_checks` harness, `crates/mybox-core/src/bin/display_checks.rs:90`). The
hotkey-opened test window therefore presents the initial empty pixmap — the
manual checklist's "appears with opaque content" is only accidentally satisfied
by a black/empty frame. The framework's own content abstraction is unreachable
from the runtime path, so a Phase-2 module has no wired way to draw.
**Fix:** either give modules a per-window draw callback that the App invokes on
`RedrawRequested` before `present()`, or explicitly document (and update the
checklist) that content generation is deferred; don't leave `draw` dead in the
runtime path.

### WR-06: Main-thread `AppEvent::Ui` closures are not panic-protected

**File:** `crates/mybox-core/src/app.rs:396`
**Issue:** Bus handlers are wrapped in `catch_unwind` (T-1-04,
`crates/mybox-core/src/event.rs:128-130`) so a panicking handler cannot kill the
dispatch worker, but `AppEvent::Ui(f) => f()` runs the module closure directly on
the winit main thread. A panicking `Ui` closure unwinds through the
`ApplicationHandler` callback and aborts the whole app — inconsistent robustness
with the bus path, and modules are told `ctx.ui().run(...)` is the safe
main-thread hop.
**Fix:**
```rust
AppEvent::Ui(f) => {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
}
```

### WR-07: Platform-specific hotkey defaults are hardcoded unconditionally

**File:** `crates/mybox-core/src/config.rs:79-87`
**Issue:** The first-run config writes `Cmd+Shift+T` / `Cmd+Shift+Q` on every
platform. Windows is a declared hard requirement, and `global-hotkey` interprets
`Cmd` on Windows as the Meta/Win key (or fails to parse), so Windows users get
wrong or silently-unregistered hotkeys out of the box — exactly the class of
breakage the workspace comment at `Cargo.toml:10-11` says to avoid.
**Fix:** make the defaults target-aware:
```rust
#[cfg(target_os = "macos")]
let (open, exit) = ("Cmd+Shift+T", "Cmd+Shift+Q");
#[cfg(target_os = "windows")]
let (open, exit) = ("Ctrl+Shift+T", "Ctrl+Shift+Q");
```

### WR-08: Duplicate hotkey combos silently overwrite the id→action mapping

**File:** `crates/mybox-core/src/hotkey.rs:77-82`
**Issue:** `register_str` records `map[id] = action`, and two actions bound to the
same combo produce the same `hotkey.id()`; the second registration overwrites the
first mapping with no warning (or if the OS rejects the duplicate, the map is
unaffected but the user gets no config-level diagnostic). A config typo that binds
two actions to one combo silently makes one action unreachable.
**Fix:** detect the collision and surface it:
```rust
let id = hotkey.id();
let mut map = self.map.lock().unwrap();
if let Some(prev) = map.get(&id) {
    log::warn!("hotkey '{action}' collides with '{prev}' for combo '{hotkey_str}'; the earlier binding is shadowed");
}
map.insert(id, action.to_string());
```

### WR-09: `batch_create` placeholder returns ids that will collide with real allocations

**File:** `crates/mybox-core/src/window.rs:301-305`
**Issue:** `batch_create` returns `next_id .. next_id+count` without advancing the
counter (it takes `&self`), so a subsequent `next_id()` re-issues the same ids
and `register` silently overwrites an existing `WindowState`. It is documented as
a Phase-2 placeholder, but it is public API and its ids are indistinguishable from
real ones today.
**Fix:** either take `&mut self` and reserve the range, or return an empty/err
result until Phase 2 implements real batch creation.

### WR-10: `FrameworkEvent::WindowDestroyed`/`ModuleLoaded`/`AppReady`/`AppExit` are declared but never constructed; window close emits no event

**File:** `crates/mybox-core/src/event.rs:40-48`; `crates/mybox-core/src/app.rs:379-382`
**Issue:** The framework advertises typed lifecycle events that no code ever
emits (verified by grep — only `WindowCreated` and `HotkeyTriggered` are
constructed). In particular, `CloseRequested` destroys the window state without
emitting `WindowDestroyed(id)`, so modules cannot observe window teardown despite
the API promising it. This is dead API surface that misleads module authors.
**Fix:** emit `WindowDestroyed(wid)` in the `CloseRequested` branch before
destroying, and either implement or remove the `ModuleLoaded`/`AppReady`/`AppExit`
variants so the enum reflects reality.

## Info

### IN-01: Unused-must-use `Result` in tests
**File:** `crates/mybox-core/src/module.rs:136-137`
**Issue:** `reg.register(...)` results are ignored; `cargo check` emits two
`unused_must_use` warnings.
**Fix:** add `let _ = ...` or `.expect("register ok")` as done elsewhere in the same file.

### IN-02: `std::sync::Mutex` with `.unwrap()` in hotkey manager (poison panic risk)
**File:** `crates/mybox-core/src/hotkey.rs:4,27,82,88`
**Issue:** The codebase standardizes on `parking_lot` (poisoning-free), but the
id→action map uses `std::sync::Mutex` with `.unwrap()`. A panic while holding it
poisons the mutex and every later `.unwrap()` panics the app.
**Fix:** switch to `parking_lot::Mutex` (no `.unwrap()` needed), consistent with the rest of the crate.

### IN-03: Hardcoded tray icon size
**File:** `crates/mybox-core/src/app.rs:218`
**Issue:** `tray.build(module_items, 32)` — the `32` is a magic number.
**Fix:** hoist to a named const (e.g. `const TRAY_ICON_SIZE: u32 = 32;`).

### IN-04: Duplicate `tiny-skia` versions in the lockfile
**File:** `Cargo.lock:2406-2421`
**Issue:** Both `tiny-skia 0.11.4` and `0.12.0` are locked, so the binary carries
two copies of the renderer stack (a transitive dep pulls 0.11).
**Fix:** audit which dependency requires 0.11 and bump/unify it.

### IN-05: `WindowSpec.transparent`/`decorations` are silently ignored for `Panel`
**File:** `crates/mybox-core/src/window.rs:88-91`
**Issue:** The `Panel` profile forces decorations and never applies
`spec.transparent`, so a Panel cannot opt into transparency — surprising given
both fields are `pub` and defaulted. Documented in the function doc, but easy to
misuse.
**Fix:** either apply `spec.transparent` for Panel too, or rename/document the
fields as overlay-only.

### IN-06: `ConfigCenter::save()` is a non-atomic write and fails on a default instance
**File:** `crates/mybox-core/src/config.rs:126-130`
**Issue:** `std::fs::write` truncates-in-place; a crash mid-write corrupts the
user's config. Also `ConfigCenter::default()` has an empty `path`, so `save()` on
it fails with an I/O error.
**Fix:** write to a temp file in the same dir then rename (atomic on macOS/Windows),
and consider erroring early on an empty path.

### IN-07: `check_overlay` doesn't verify what its docstring claims
**File:** `crates/mybox-core/src/bin/display_checks.rs:154-157`
**Issue:** The check asserts `state.spec.transparent` (a field round-trip), not
that the created window is actually transparent — and since `window_attributes`
ignores `spec.transparent` for the Overlay profile (window.rs:78-82), the check
is asserting a value that doesn't drive behavior. Real alpha is deferred to
Phase 2, but the check name/comment overstate coverage.
**Fix:** rename the check to "overlay profile registered" or assert on the
attributes actually produced (`window_attributes(&spec).transparent`).

### IN-08: `EventBus::new` panics on worker-thread spawn failure
**File:** `crates/mybox-core/src/event.rs:135`
**Issue:** `.expect("spawn event-bus worker thread")` — a thread-spawn failure
aborts at startup instead of returning an error, inconsistent with the
error-returning constructors elsewhere.
**Fix:** make `EventBus::new` fallible or return a `Result`.

---

_Reviewed: 2026-08-12_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
