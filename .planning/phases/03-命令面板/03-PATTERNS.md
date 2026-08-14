# Phase 3: 命令面板 - Pattern Map

**Mapped:** 2026-08-14
**Files analyzed:** 22 (10 new palette crate files + 1 new core file + 11 modified)
**Analogs found:** 22 / 22 (all files have an in-repo analog; several bodies are new code covered by RESEARCH patterns — flagged below)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/modules/palette/Cargo.toml` (new) | config | - | `crates/modules/capture/Cargo.toml` | exact |
| `crates/modules/palette/src/lib.rs` (new) | module/controller | event-driven | `crates/modules/capture/src/lib.rs` | exact |
| `crates/modules/palette/src/session.rs` (new) | service/state | event-driven state machine | `crates/modules/capture/src/session.rs` | exact |
| `crates/modules/palette/src/filter.rs` (new) | utility | transform (pure) | `crates/modules/capture/src/selection.rs` | role-match |
| `crates/modules/palette/src/ui.rs` (new) | component | transform (egui) | `crates/modules/capture/src/overlay.rs` (draw-closure discipline) — egui body is NEW | partial |
| `crates/modules/palette/src/raster.rs` (new) | utility | transform (pixel) | `crates/mybox-core/src/tray.rs` `generate_icon_rgba` + `renderer/mod.rs` `premul_rgba_to_u32` | role-match |
| `crates/modules/palette/src/position.rs` (new) | utility | request-response | `crates/modules/capture/src/capture.rs` (xcap geometry) + `permission.rs` (cfg-gated objc2) | role-match |
| `crates/modules/palette/src/execute.rs` (new) | service | event-driven (worker thread) | `crates/modules/capture/src/lib.rs` `start_capture` (lines 206-252) | exact |
| `crates/modules/palette/src/fonts.rs` (new) | utility | file-I/O | `crates/modules/capture/src/text.rs` (system font load from path) | role-match |
| `crates/modules/palette/src/bin/palette_checks.rs` + `tests/` (new) | test | - | `crates/mybox-core/src/bin/display_checks.rs` + `crates/mybox-core/tests/integration.rs` | exact |
| `crates/mybox-core/src/command.rs` (new) | model/service | CRUD registry | `crates/mybox-core/src/module.rs` `ModuleRegistry` + `event.rs` emit | role-match |
| `crates/mybox-core/src/module.rs` (modify, C1) | model | - | self: `menu_items()` default method (lines 30-32) | exact |
| `crates/mybox-core/src/window.rs` (modify, C3/C4/C6) | config | request-response | self: `on_event`/`on_draw` fields, `window_attributes` profile | exact |
| `crates/mybox-core/src/app.rs` (modify, C3/C4/C5) | controller | event-driven | self: `window_event` routing, `about_to_wait` drain, `AppEvent` enum | exact |
| `crates/mybox-core/src/context.rs` (modify, C2) | service facade | request-response | self: `bus()`/`config()` accessors | exact |
| `crates/mybox-core/src/lib.rs` (modify, D-01) | config | - | self: re-export block (lines 34-43) | exact |
| `crates/mybox-core/Cargo.toml` (modify, D-01) | config | - | workspace `[workspace.dependencies]` discipline | exact |
| `crates/modules/capture/src/lib.rs` (modify) | module | event-driven | self: `Module` impl (lines 67-195) | exact |
| `crates/mybox-app/src/main.rs` (modify, D-12) | config/app entry | - | self: `env_logger::init()` + module registration (lines 9-17) | exact |
| `crates/mybox-app/Cargo.toml` (modify) | config | - | self + Phase 2 deviation 5 precedent (mybox-capture dep) | exact |
| `Cargo.toml` workspace (modify) | config | - | self: members + `[workspace.dependencies]` | exact |
| `crates/modules/test/src/lib.rs` (modify, Pitfall 7) | module/test | - | self: existing `WindowRequest` match arms (lines 175-185) | exact |

**Crate naming:** follow `mybox-test`/`mybox-capture` precedent — directory `crates/modules/palette/`, package name **`mybox-palette`** (RESEARCH uses `-p mybox-palette` in the test matrix).

---

## Pattern Assignments

### `crates/mybox-core/src/command.rs` (new — model/service, CRUD registry)

**Analog:** `crates/mybox-core/src/module.rs` `ModuleRegistry` (registration + duplicate rejection, lines 38-74) + `event.rs` bus emit (lines 148-154).

**Registry shape to copy** (`module.rs` lines 39-58 — same duplicate-id discipline as `ModuleRegistry::register`):
```rust
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}
impl ModuleRegistry {
    pub fn register(&mut self, module: Box<dyn Module>) -> Result<()> {
        let id = module.id();
        if self.modules.iter().any(|m| m.id() == id) {
            return Err(MyboxError::Module(format!("duplicate module id '{id}'")));
        }
        self.modules.push(module);
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = &dyn Module> { /* ... */ }
}
```
CommandRegistry mirrors this: `Vec<Command>`, `register(Command)` rejecting duplicate `id`s (a new `MyboxError` variant or reuse `MyboxError::Module` — RESEARCH Pattern 3 says "duplicate command ids rejected like duplicate module ids"). Order: module commands first (registration order), then builtins appended.

**Command type** (RESEARCH Pattern 3, lines 299-311 — data + runner closure, `Arc` so `Command` is `Clone`):
```rust
pub struct Command {
    pub id: &'static str,
    pub name: String,               // "开始截图"
    pub description: String,        // non-empty (SPEC req 1)
    pub keywords: Vec<&'static str>,// ["截图", "capture", "screen", "jietu"]  ← pinyin for "jt"
    pub runner: CommandRunner,      // Arc<dyn Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> + Send + Sync>
    pub hide_before_execute: bool,  // true for capture.start
}
```

**Bus emit pattern for builtin.quit** (`event.rs` lines 148-154 — the exact emit shape):
```rust
pub fn emit(&self, event: Event) {
    if let Err(err) = self.inner.sender.send(event) {
        log::warn!("event bus send failed: {err:?}");
    }
}
// builtin.quit runner:
bus.emit(Event { from: "core", kind: "app-exit",
    payload: EventPayload::Framework(FrameworkEvent::AppExit) });
```
`FrameworkEvent::AppExit` already exists (`event.rs` line 47) but nothing handles it — C5 adds the handler (see app.rs section).

**Builtin open_config/open_log** use `std::process::Command::new("open").arg(path)` (macOS) / `"explorer"` (Windows), `#[cfg(target_os = ...)]`-gated, no shell (RESEARCH lines 313-316, 385). Config dir path from `mybox_core::config_dir()` (`config.rs` line 158: `pub fn config_dir() -> anyhow::Result<PathBuf>`).

**Error handling:** builtin runners return `anyhow::Result<()>` from the boxed future — the same module-boundary convention as `Module::init` (`module.rs` line 22). All four builtin bodies are `Box::pin(async { ... })` with no real awaits (RESEARCH Pattern 3).

**Tests** (Wave 0, RESEARCH lines 590/595): registry ≥5 commands, module-first order, duplicate-id rejection, non-empty name/description; quit emits `app-exit`; restart spawns `current_exe()` then emits exit; open_config/open_log invoke the platform opener with the right path — inject a spawner closure for headless tests (same injectable discipline as `CaptureFn` in capture lib.rs lines 34-51).

---

### `crates/mybox-core/src/module.rs` (modify — C1, trait extension)

**Analog:** self — `menu_items()` default method (lines 30-32) is the exact precedent for an additive defaulted trait method:
```rust
fn menu_items(&self) -> Vec<tray_icon::menu::MenuItem> {
    vec![]
}
```
**Add** (SPEC req 1, RESEARCH C1 — default `vec![]`, non-breaking):
```rust
/// Commands contributed to the command palette (Phase 3). Modules that expose
/// no commands keep the default empty list.
fn commands(&self) -> Vec<crate::command::Command> {
    vec![]
}
```
Update the `FakeModule` test impl (lines 97-107) — no change needed (default impl covers it). Existing `duplicate_id_is_rejected_with_id_in_message` etc. unaffected.

---

### `crates/mybox-core/src/window.rs` (modify — C3/C4/C6)

**Analog:** self — the `on_event` closure field is the template for the new `on_event_win` field.

**C3 — `on_event_win` field** (mirror `on_event` at lines 43-44):
```rust
/// Per-window event callback (D-07 routing target).
pub on_event: Option<Box<dyn Fn(&winit::event::WindowEvent) + Send + Sync>>,
// NEW (Phase 3, C3) — same contract plus the window itself (egui-winit needs
// &Window for on_window_event/take_egui_input):
// pub on_event_win: Option<Box<dyn Fn(&Arc<winit::window::Window>, &winit::event::WindowEvent) + Send + Sync>>,
```
Must be added to `impl Default` (lines 51-67, `on_event_win: None`), to the `window_spec_default_is_panel_decorated_visible` test (lines 385-397, add `assert!(spec.on_event_win.is_none());`), and to the struct-literal contract test (lines 400-412).

**C4 — Floating profile non-resizable + focus** (`window_attributes` lines 135-147 — the Floating arm):
```rust
WindowKind::Floating => {
    attrs = attrs
        .with_decorations(false)
        .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
}
// add: .with_resizable(false)   ← same as the Overlay arm (line 140); prevents
// the overlay-window-movable-at-edge bug class for borderless Floating.
```
Extend `window_attributes_floating_is_always_on_top` test (lines 478-489) with a non-resizable assertion (copy the Overlay test's comment style, lines 466-469).

**C6 — `round_floating_corners` (macOS, optional)** — follow `elevate_overlay_window` (lines 86-120), the only existing raw-window-handle → NSWindow pattern:
```rust
#[cfg(target_os = "macos")]
pub fn elevate_overlay_window(window: &winit::window::Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else { /* warn + return */ };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { /* warn */ };
    let view: &objc2_app_kit::NSView = unsafe { &*(appkit.ns_view.as_ptr() as *const _) };
    // C6: view.window() → NSWindow contentView layer setCornerRadius(12.0) +
    //     setMasksToBounds(true) — same shape, different setters.
}
```
Note: `objc2-app-kit` is already a transitive dep in lock (via winit/xcap); the palette crate promotes it to a direct macOS dep (RESEARCH lines 146-148). Fallback if the layer trick fails visually: square corners, Phase 4 polish (A2).

---

### `crates/mybox-core/src/app.rs` (modify — C3/C4/C5)

**Analog:** self — three established insertion points.

**C3 — route `on_event_win` in `window_event`** (lines 391-394, right after the `on_event` call):
```rust
if let Some(state) = self.windows.get_mut_by_winit(id) {
    if let Some(cb) = &state.spec.on_event {
        cb(&event);
    }
    // NEW (C3): egui-winit needs &Window — invoke right after on_event so the
    // framebuffer is fresh when RedrawRequested → handle_redraw presents.
    // if let Some(cb) = &state.spec.on_event_win {
    //     if let Some(w) = &state.window { cb(w, &event); }
    // }
    match event { /* ... */ }
}
```
`state.window` is `Option<Arc<winit::window::Window>>` (`window.rs` lines 174-180) — `Some` for any live winit window.

**C4 — extend the focus call in `create_window`** (lines 322-335 — currently `WindowKind::Overlay` only):
```rust
if spec.kind == WindowKind::Overlay {
    #[cfg(target_os = "macos")]
    crate::window::elevate_overlay_window(&window);
    window.focus_window();
}
// C4: also focus for Floating (Pitfall 1 — Accessory policy: hotkey does not
// activate the app; set_visible alone doesn't make the window key):
// if matches!(spec.kind, WindowKind::Overlay | WindowKind::Floating) {
//     window.focus_window();
// }
```
(No `elevate_overlay_window` for Floating — AlwaysOnTop is enough for the palette.)

**C5 — `AppEvent::Exit` + bus subscription.** `AppEvent` enum is at lines 53-66; add a `Exit` unit variant next to `WindowRequested`. Install the forwarder in `install_event_forwarders` (lines 261-274 — the existing three forwarder pattern):
```rust
// research C5: bus EventFilter::kind("core", "app-exit") handler →
// proxy.send_event(AppEvent::Exit)
```
Handle in `user_event` (lines 415-425): `AppEvent::Exit => el.exit()` — note `user_event`'s `_el` parameter must be un-underscored for this arm (the `el` is already in scope — rename `_el` to `el`). The bus→proxy hop is the same pattern as `on_hotkey` → `bus.emit` (lines 279-293), just in reverse direction. `catch_unwind` around `AppEvent::Ui(f)` (line 420) already exists and covers runner finalize closures (RESEARCH Security: "UiThreadProxy which core already wraps in catch_unwind (app.rs:420)").

---

### `crates/mybox-core/src/context.rs` (modify — C2, commands accessor)

**Analog:** self — the `bus()` accessor (lines 56-58) is the exact shape (added in Phase 2, same class of additive accessor):
```rust
/// Access the shared event bus ...
pub fn bus(&self) -> &Arc<EventBus> {
    &self.bus
}
// NEW (C2):
// pub fn commands(&self) -> &Arc<crate::command::CommandRegistry> { &self.commands }
```
Add a `pub(crate) commands: Arc<CommandRegistry>` field (struct at lines 18-24) and a parameter to `ModuleContext::new` (lines 32-46) — **breaking for the 3 existing `ModuleContext::new` test call sites** (`context.rs` line 174, `app.rs` line 585, `modules/test/src/lib.rs` line 104, `modules/capture/src/lib.rs` line 284): all pass a fresh `Arc::new(CommandRegistry::new())` (give `CommandRegistry` a `Default` like `ModuleRegistry`, lines 76-80). Registry is assembled in `AppBuilder::build` (see below) so it exists before `init()` runs — modules can read commands during init.

**`AppBuilder::build` assembly** (`app.rs` lines 118-155): after `init_modules` (line 136), build the registry from `registry.iter()` module `commands()` in order + `BuiltinCommands::build(...)`, store in `Arc`, pass into `ModuleContext::new`.

---

### `crates/mybox-core/src/lib.rs` (modify — D-01 re-exports)

**Analog:** self — the re-export block (lines 34-43):
```rust
pub use anyhow;
pub use log;
pub use tiny_skia;
pub use toml;
pub use tray_icon;
pub use winit;
```
**Add** (D-01: egui lives in core, re-exported so every module reuses ONE egui; fuzzy-matcher/pollster likewise):
```rust
pub mod command;                       // new module (near line 8-17 mod list)
pub use command::{BuiltinCommands, Command, CommandRegistry, run_command};
pub use egui;                          // D-01
pub use egui_winit;                    // D-01
pub use fuzzy_matcher;                 // palette filter
pub use pollster;                      // execute.rs block_on
```
Same rationale comment as lines 29-43 (FRMW-02 boundary — modules stay one-dep).

---

### `crates/mybox-core/Cargo.toml` + workspace `Cargo.toml` (modify — deps)

**Analog:** workspace `Cargo.toml` lines 10-31 — the version-lock discipline comment ("Do NOT bump any of these without re-verifying cross-compatibility"):
```toml
[workspace.dependencies]
egui = "0.30.0"        # D-02 lock — 0.30.0 is the ONLY 0.30.x release (RESEARCH verified)
egui-winit = "0.30.0"  # declares winit 0.30.5 → resolves to pinned 0.30.13
fuzzy-matcher = "0.3.7"
pollster = "1.0.1"
objc2-app-kit = "0.3.2"  # already in lock (via winit/xcap) — promoted to direct macOS dep
```
`crates/mybox-core/Cargo.toml` gains: `egui.workspace = true`, `egui-winit.workspace = true`, `fuzzy-matcher.workspace = true`, `pollster.workspace = true` (RESEARCH Installation lines 127-148). Workspace `members` add `"crates/modules/palette"` (line 7).

---

### `crates/modules/palette/Cargo.toml` (new — config)

**Analog:** `crates/modules/capture/Cargo.toml` (lines 1-15) — EXACT template:
```toml
[package]
name = "mybox-capture"
version = "0.1.0"
edition = "2021"
description = "Screenshot capture module for the mybox desktop toolbox"

[dependencies]
mybox-core = { path = "../../mybox-core" }
xcap.workspace = true
```
Palette version (RESEARCH Installation lines 141-148):
```toml
[package]
name = "mybox-palette"
version = "0.1.0"
edition = "2021"
description = "Command palette module for the mybox desktop toolbox"

[dependencies]
mybox-core = { path = "../../mybox-core" }   # egui/egui-winit/fuzzy-matcher/pollster via re-exports (D-01)
xcap.workspace = true                          # position.rs active-monitor enumeration

[target.'cfg(target_os = "macos")'.dependencies]
objc2-app-kit = "0.3.2"   # NSEvent::mouseLocation (already in lock)
```
FRMW-02 boundary: NO direct egui/egui-winit/fuzzy-matcher/pollster deps — all through `mybox_core::` re-exports (Phase 2 deviations 1-2 established the enforcement: un-re-exported crates fail compile).

---

### `crates/modules/palette/src/lib.rs` (new — module/controller, event-driven)

**Analog:** `crates/modules/capture/src/lib.rs` — EXACT template (module skeleton + hotkey subscription + deferred hotkey registration).

**Imports pattern** (`capture/src/lib.rs` lines 21-28):
```rust
use mybox_core::anyhow;
use mybox_core::event::{EventFilter, EventPayload, FrameworkEvent};
use mybox_core::log;
use mybox_core::module::Module;
use mybox_core::toml;
use mybox_core::window::WindowManagerHandle;
use mybox_core::{ConfigCenter, ModuleContext, UiThreadProxy};
```

**Module struct + injected fields** (`capture/src/lib.rs` lines 42-65 — the injectable-dependency discipline from 02-01 patterns):
```rust
pub struct CaptureModule {
    session: Arc<CaptureSession>,
    capture: CaptureFn,          // ← palette: inject a SummonHandler/position fn for headless tests
    access: AccessChecker,
    // ...
}
impl CaptureModule {
    pub fn new() -> Self { /* production real impls */ }
}
```

**Hotkey toggle subscription** — `init()` pattern (`capture/src/lib.rs` lines 110-130, hotkey branch on action string):
```rust
ctx.on(
    EventFilter::kind("core", "hotkey.triggered"),
    Box::new(move |e| {
        if let EventPayload::Framework(FrameworkEvent::HotkeyTriggered { action, .. }) = &e.payload {
            if action == "start_screenshot" { /* ... */ }
        }
    }),
);
```
Palette: action `"toggle_palette"`; toggle logic — session has a live window? → enqueue `Destroy` : summon (compute position → session.summon() with generation += 1 → enqueue `Create(WindowSpec { kind: Floating, on_event_win, on_draw, .. })`). RESEARCH Architecture Diagram lines 182-220 is the flow to encode.

**Deferred hotkey registration** (`capture/src/lib.rs` lines 185-191 — MUST copy; direct `register_str` in init fails "not initialized"):
```rust
let hotkeys = ctx.hotkeys().clone();
let hotkey_str = hotkey_from_config(ctx.config());
ctx.ui().run(Box::new(move || {
    if let Err(e) = hotkeys.register_str("start_screenshot", &hotkey_str) {
        log::warn!("failed to register start_screenshot hotkey: {e}");
    }
}));
```
Palette default: `"Cmd+Shift+Space"`, config key `[palette].hotkey`, read helper = clone of `hotkey_from_config` (`capture/src/lib.rs` lines 256-261 — `config.get("capture", "hotkey").and_then(|v| v.as_str()...)`). `default_config()` returns the table (`capture/src/lib.rs` lines 76-85 shape).

**`window-created` pairing** — clone of the `core/window-created` subscription (`capture/src/lib.rs` lines 166-177) so the palette records its window id and can destroy it later.

**Headless test scaffold** — `sample_context()` (`capture/src/lib.rs` lines 281-292) + `wait_until` (lines 270-278) + hotkey-routing test (lines 458-503, emit synthetic `core`/`hotkey.triggered`, poll `handle.try_recv()` for `WindowRequest::Create`). Copy wholesale.

---

### `crates/modules/palette/src/session.rs` (new — service/state, event-driven state machine)

**Analog:** `crates/modules/capture/src/session.rs` — EXACT template for shared lock-guarded state + headless-testable transitions.

**Arc<Mutex> shared-state skeleton** (`capture/session.rs` lines 158-180):
```rust
#[derive(Clone)]
pub struct CaptureSession {
    state: Arc<std::sync::Mutex<SessionState>>,
    bus: Arc<std::sync::OnceLock<Arc<EventBus>>>,   // palette: not needed unless emitting
}
impl CaptureSession {
    pub fn new() -> Self {
        Self { state: Arc::new(std::sync::Mutex::new(SessionState::default())), /* ... */ }
    }
}
```
**Use `std::sync::Mutex`, NOT `parking_lot`** — the module-crate precedent (comment at lines 161-163; Phase 2 deviation 1): parking_lot is not re-exported across FRMW-02.

**State shape** (RESEARCH Pattern 4 + UI-SPEC six states): `enum PaletteState { Hidden, Idle, Filtering, Empty, Executing, Error }` + fields: `input: String`, `selection: Option<usize>`, `filtered: Vec<Command>` (snapshot at summon), `window_id: Option<WindowId>`, `generation: u64` (per-summon counter — Pitfall 3), `executing_id: Option<&'static str>`, `error: Option<String>`, `framebuffer: tiny_skia::Pixmap`, `textures: HashMap<TextureId, ...>`, `egui_ctx: Mutex<egui::Context>` (egui Context is Send but NOT Sync — RESEARCH Anti-Patterns line 369; lock is only ever taken on the main thread). All transitions as methods (`summon/close/on_input/on_key/execute_begin/finalize`) so they are headless unit-testable like capture's session methods.

---

### `crates/modules/palette/src/filter.rs` (new — utility, transform/pure)

**Analog (role-match):** `crates/modules/capture/src/selection.rs` pure-fn discipline + `renderer/mod.rs` headless test style. Selection.rs is pure math with `#[cfg(test)]` tests, no winit — the same shape for fuzzy filter.

**Core** (RESEARCH lines 459-469 — use matcher API, NOT the deprecated top-level fns):
```rust
use fuzzy_matcher::skim::SkimMatcherV2;
let matcher = SkimMatcherV2::default().smart_case();
let (score, indices) = matcher.fuzzy_indices("开始截图", "jt")?;  // None = no match
// ranking: name fuzzy_match primary → description → max(keywords); tie-break by
// registration order (stable sort — UI-SPEC lifecycle rule 4).
```
Signature shape (pure + headless): `fn filter_commands<'a>(cmds: &'a [Command], query: &str) -> FilterResult<'a>` returning ranked list + per-command char-index highlights for the LayoutJob. Cap query length at ~64 chars (RESEARCH Security V5). Tests: "截图" and "jt" hit capture.start first (keyword `"jietu"` — Pitfall 6 data fix), no-match → Empty, empty input → all, tie-break stable (RESEARCH lines 591).

---

### `crates/modules/palette/src/ui.rs` (new — component, transform/egui)

**Analog (partial):** `crates/modules/capture/src/overlay.rs` draw-closure discipline (never accumulate pixels; render full frame from session state each redraw — 02-PATTERNS overlay section) + RESEARCH Code Examples. The egui closure body itself is NEW (no egui exists in the repo).

**Closure signature** (RESEARCH lines 444-457 — the frame body the `on_event_win` callback runs):
```rust
let full_output = egui_ctx.run(raw_input, |ctx| ui::draw(ctx, &session_state));
let primitives = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
```
UI-SPEC elements: rounded card (Raycast style, D-08), dark fixed theme `#202020` bg (D-09), `TextEdit` search input, `ScrollArea` of CommandRows (name + gray description + `#FF6000` LayoutJob highlight, D-10), StatusLine for Executing (D-04), Empty/Error states (D-05), ~600px fixed width / adaptive height ≤10 rows (D-11). LayoutJob highlight shape (RESEARCH lines 471-478).

---

### `crates/modules/palette/src/raster.rs` (new — utility, transform/pixel)

**Analog (role-match):** tiny-skia drawing pattern in `crates/mybox-core/src/tray.rs` `generate_icon_rgba` (PathBuilder + `fill_path` + `Paint::set_color_rgba8` — see 02-PATTERNS lines 260-273) + premultiply conversion in `renderer/mod.rs` `premul_rgba_to_u32` (lines 37-48).

**Contract to honor** (RESEARCH Pattern 1, lines 264-281): egui `Color32` is straight RGBA; tiny-skia Pixmap is premultiplied — convert when writing (Phase 2 Pitfall 2 discipline). Two paths: solid triangles via `fill_path` fast path; textured font glyphs (`TextureId::Managed(0)` = font atlas) via barycentric per-pixel sampling with bilinear UV fetch from the stored texture image. `on_draw` then blits the palette-owned framebuffer into the core PixmapMut — the 1-line blit, invoked before `present()` by the existing `handle_redraw` (`app.rs` lines 364-373). Headless-testable: tessellate a known frame with a Chinese label → assert non-background pixels (RESEARCH lines 596).

---

### `crates/modules/palette/src/position.rs` (new — utility, request-response)

**Analog (role-match):** xcap geometry in `crates/modules/capture/src/capture.rs` (`Monitor::all()`, points × `scale_factor()` — the ONLY logical→physical conversion point, 02-01 patterns) + the `#[cfg(target_os = "macos")]` gating in `crates/modules/capture/src/permission.rs` (objc2 call with non-macOS stub returning true — same gating style, macOS-only body):
```rust
#[cfg(target_os = "macos")]
let cursor = unsafe { objc2_app_kit::NSEvent::mouseLocation() };  // NSPoint, BOTTOM-left origin
// flip to top-left: (cursor.x, main_h - cursor.y) → find containing xcap Monitor
// → center = (monitor.x + w/2, monitor.y + h/2) × scale_factor, minus half panel size
// → WindowSpec { kind: Floating, inner_size: Some((w,h)), position: Some((x,y)) }
```
RESEARCH Pattern 5 lines 336-349 is the exact math (unit-test the origin flip — A3). Non-macOS fallback: first monitor (Windows = Phase 4). Non-macOS builds must still compile (gate the objc2 import).

---

### `crates/modules/palette/src/execute.rs` (new — service, event-driven worker)

**Analog:** `crates/modules/capture/src/lib.rs` `start_capture` (lines 206-252) — EXACT worker-thread + UiThreadProxy pattern:
```rust
std::thread::Builder::new()
    .name("mybox-capture".to_string())
    .spawn(move || {
        let result = capture();
        ui.run(Box::new(move || match result {
            Ok(shots) => { /* main-thread state write */ }
            Err(e) => { log::error!("capture failed: {e:#}"); }
        }));
    })
    .expect("spawn capture worker thread");
```
**Palette composition** (RESEARCH Pattern 4, lines 321-329):
```rust
session.set_executing(gen, cmd.id());                    // Executing state, input disabled (D-04)
if cmd.hide_before_execute { session.close(); }          // Destroy BEFORE runner (capture exception, Pitfall 4)
let (ui, session, gen) = (ui.clone(), session.clone(), session.generation());
std::thread::Builder::new().name(format!("mybox-cmd-{}", cmd.id)).spawn(move || {
    let result = pollster::block_on((cmd.runner)());     // pollster 1.0.1 — no tokio this phase
    ui.run(Box::new(move || session.finalize(gen, result)));
    // finalize: guard gen == session.generation() (Pitfall 3 stale-completion guard);
    // Ok → Hidden + enqueue Destroy; Err → Error state, window stays (D-05)
}).expect("spawn command thread");
```
Thread-naming via `std::thread::Builder` — same as capture (lines 236-238) and event bus (`event.rs` lines 117-119). `UiThreadProxy::run` shape is `context.rs` lines 138-145.

---

### `crates/modules/palette/src/fonts.rs` (new — utility, file-I/O)

**Analog (role-match):** `crates/modules/capture/src/text.rs` — loads a system font from an absolute path with fallback (Arial.ttf, Phase 2 verified pattern); palette loads `/System/Library/Fonts/Hiragino Sans GB.ttc` as TWO faces via `FontData { index }` (epaint 0.30.0 has `pub index: u32` — RESEARCH Pattern 6, lines 354-364):
```rust
let bytes = std::fs::read("/System/Library/Fonts/Hiragino Sans GB.ttc")?;
let mut defs = egui::FontDefinitions::default();
defs.font_data.insert("hiragino-w3".into(), egui::FontData::from_owned(bytes.clone()).into());   // index 0
defs.font_data.insert("hiragino-w6".into(), egui::FontData { index: 1, ..egui::FontData::from_owned(bytes) }.into());
// insert at head of Proportional family (Pitfall 5 — tofu boxes)
egui_ctx.set_fonts(defs);   // once, before the first frame
```
`#[cfg(target_os = "macos")]`-gated with ASCII-only fallback elsewhere (Windows font discovery = Phase 4). Return `anyhow::Result<()>` like the rest of the module boundary.

---

### `crates/modules/palette/src/bin/palette_checks.rs` + `tests/` (new — test harness)

**Analog:** `crates/mybox-core/tests/integration.rs` (lines 22-38) + `crates/mybox-core/src/bin/display_checks.rs` (lines 1-120) — EXACT subprocess-per-check template:
```rust
// tests/integration.rs (palette/tests/)
const CHECKS_BIN: &str = env!("CARGO_BIN_EXE_palette_checks");
fn run_check(name: &str) {
    let status = Command::new(CHECKS_BIN).arg(name).status()
        .unwrap_or_else(|e| panic!("failed to spawn palette_checks '{name}': {e}"));
    assert!(status.success(), "palette_checks '{name}' exited with {status:?}");
}
#[test]
#[ignore]
fn palette_summon_focus_type_enter_esc() { run_check("summon"); }
```
Why subprocess: winit macOS needs the EventLoop on the real main thread, one per process (integration.rs lines 11-20). The checks bin is an `ApplicationHandler` harness — copy `WindowHarness` (display_checks.rs lines 30-107) but with a real `App`/palette-driven flow (summon → focus → type → Enter → ESC → exit code). One check per subprocess; run via `cargo test -- --ignored -p mybox-palette` (RESEARCH lines 597).

---

### `crates/modules/capture/src/lib.rs` (modify — register 截图 command)

**Analog:** self — add `commands()` to the existing `Module` impl (lines 67-195). The runner must call the EXISTING `start_capture` (lines 206-252) so the re-entrancy guard (`begin_capture`, lines 216-220) stays intact. `commands()` takes `&self`; `CaptureModule` already holds `session: Arc<CaptureSession>` (line 46), so the runner clones it plus the injectable `capture/access/request/open` fields:
```rust
fn commands(&self) -> Vec<mybox_core::command::Command> {
    vec![mybox_core::command::Command {
        id: "capture.start",
        name: "开始截图".into(),
        description: "截取屏幕区域并复制/保存".into(),
        keywords: vec!["截图", "capture", "screen", "jietu"],  // "jietu" → "jt" matches (Pitfall 6)
        hide_before_execute: true,   // SPEC: panel must never appear in screenshots
        runner: Arc::new(/* clone session + start_capture, Box::pin(async move { Ok(()) }) */),
    }]
}
```
Keep the existing hotkey path (`Cmd+Shift+S`) — RESEARCH Open Question 4: keep both, no behavior removal. Test: `commands()` returns 1 command with the pinyin keyword; runner invokes the injected fake capture once (reuse the counting-closure style, lines 338-351).

---

### `crates/mybox-app/src/main.rs` (modify — D-12 dual-sink logger + palette registration)

**Analog:** self (lines 9-17):
```rust
use mybox_core::App;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut builder = App::builder();
    builder.module(Box::new(mybox_test::TestModule))?;
    builder.module(Box::new(mybox_capture::CaptureModule::new()))?;
    builder.build()?.run()
}
```
**D-12 change:** replace bare `env_logger::init()` with a dual-sink logger writing to `config_dir()/logs/mybox.log` (create dir first; `mybox_core::config_dir()` at `config.rs` line 158) + stderr. Must be initialized at startup so `builtin.open_log` can always open an existing file. **Registration:** add `builder.module(Box::new(mybox_palette::PaletteModule::new()))?;` + `mybox-app/Cargo.toml` gains `mybox-palette = { path = "../modules/palette" }` (Phase 2 deviation 5 precedent — the dep must be declared or `cargo check --workspace` fails).

---

### `crates/modules/test/src/lib.rs` (modify — Pitfall 7 fix)

**Analog:** self — the `WindowRequest` match in the hotkey test (lines 175-185) already has ALL four arms (Create/Destroy/Redraw/SetCursor) in its current form; verify against the enum (`window.rs` lines 212-221) and add any missing arms so `cargo nextest run` workspace-wide compiles (RESEARCH Pitfall 7: the documented break was the pre-Phase-2 version of this file; re-confirm at implementation time).

---

## Shared Patterns

### Module crate boundary (FRMW-02)
**Source:** `crates/modules/test/src/lib.rs` lines 8-11 + `crates/mybox-core/src/lib.rs` lines 29-43
**Apply to:** All palette crate files
- Module crates depend ONLY on `mybox-core`. Use `mybox_core::anyhow`, `mybox_core::log`, `mybox_core::tiny_skia`, `mybox_core::winit`, and (new) `mybox_core::egui`/`mybox_core::egui_winit`/`mybox_core::fuzzy_matcher`/`mybox_core::pollster` re-exports — never add those crates to the module's own deps (Phase 2 deviations 1-2 enforced this at compile time). Exception: `xcap` and `objc2-app-kit` are direct palette deps (RESEARCH Installation) — same status as capture's xcap/arboard/ab_glyph.
- Use `std::sync::Mutex`, not `parking_lot` (not re-exported across the boundary — `capture/session.rs` lines 161-163).

### Hotkey → bus event → toggle → window enqueue
**Source:** `crates/modules/capture/src/lib.rs` lines 110-130 (subscription), 185-191 (deferred registration); dispatch in `app.rs` `on_hotkey` lines 279-293
**Apply to:** `palette/src/lib.rs` init
- Subscribe `EventFilter::kind("core", "hotkey.triggered")`, match `EventPayload::Framework(FrameworkEvent::HotkeyTriggered { action, .. })`, branch on `"toggle_palette"`. Defer `register_str` via `ctx.ui().run` (init runs before `hotkeys.init()`). Never execute arbitrary code from event payloads (T-1-02).

### Build/destroy window lifecycle (W2/W3, re-entrancy)
**Source:** `app.rs` `about_to_wait` lines 431-459 (drain), `WindowManagerHandle` (`window.rs` lines 258-284 — create/destroy/redraw enqueue + wake hook)
**Apply to:** `palette/src/lib.rs`, `session.rs`, `execute.rs`
- Windows are created/destroyed ONLY via enqueued `WindowRequest`s drained on the main thread; modules never touch `winit::Window` off the main thread. Build-destroy per summon (D-06) + generation counter (Pitfall 3) prevents orphan windows and stale runner completions — the Phase 2 re-entrancy lesson generalized.

### Worker thread + UiThreadProxy main-thread hop
**Source:** `crates/modules/capture/src/lib.rs` lines 234-251; `context.rs` lines 138-145 (`run`); `app.rs` line 419-421 (`AppEvent::Ui` + catch_unwind)
**Apply to:** `palette/src/execute.rs`
- Heavy/blocking work (runner futures via `pollster::block_on`) runs on a named per-invocation thread; results hop back through `ui.run(...)`. Completion closures are already panic-guarded by core's catch_unwind.

### Event filter / subscription + wait_until test discipline
**Source:** `crates/mybox-core/src/event.rs` lines 61-76 (filter), 156-164 (on); `capture/src/lib.rs` tests lines 270-278 (`wait_until` polling — dispatch is async on the bus worker thread)
**Apply to:** `palette/src/lib.rs` tests, `session.rs` tests
- `EventFilter::kind(from, kind)` with `"*"` wildcards; handlers `Box<dyn Fn(&Event) + Send + Sync>`; tests poll with `wait_until`, never sleep-assert a single state.

### Error handling
**Source:** `crates/mybox-core/src/error.rs` lines 8-39 (thiserror `MyboxError` + `Result` alias); module boundary returns `anyhow::Result` (`module.rs` line 22)
**Apply to:** `command.rs` (core), `palette` submodules
- Core additions use typed `MyboxError` variants (add one if needed, e.g. a `Command(String)` variant in the style of `Module(String)` line 34-35). Module boundary (`init`, `fonts.rs`, `position.rs`) returns `mybox_core::anyhow::Result`. Runner futures return `anyhow::Result<()>`; never panic in handlers.

### Headless unit-test discipline
**Source:** `renderer/mod.rs` tests (lines 50-109), `modules/test/src/lib.rs` tests, `capture` tests; `hotkey.rs` test comment line 132 ("never call init() on OS-bound managers")
**Apply to:** All palette sub-modules
- Pure logic (filter, session state machine, position math, rasterizer on synthetic frames) unit-tested headlessly via `cargo nextest run -p mybox-palette`. Real-display behavior is `#[ignore]` + subprocess-per-check (`tests/` + `bin/palette_checks.rs`). Tests never touch the real user config dir; inject OS side effects (spawner for restart, position fn, capture fn).

### Premultiplied-alpha discipline
**Source:** `crates/mybox-core/src/renderer/mod.rs` lines 30-48 (`premul_rgba_to_u32`); Phase 2 Pitfall 2
**Apply to:** `palette/src/raster.rs`
- tiny-skia Pixmaps are premultiplied; egui `Color32` is straight RGBA — convert when writing pixels. macOS softbuffer drops alpha (`CGImageAlphaInfo::NoneSkipFirst`) → opaque `#202020` full-bleed window + NSWindow layer rounding for corners (C6), never per-pixel transparency.

---

## No Analog Found

No files are entirely analog-less — every file has an in-repo structural template. These files have NEW *bodies* (covered by RESEARCH patterns — planner must use RESEARCH.md code examples for the body while copying the repo analog for structure):

| File | Role | Body Source | Structural Analog |
|------|------|-------------|-------------------|
| `crates/modules/palette/src/ui.rs` | component (egui) | RESEARCH Code Examples lines 444-478 (egui frame, LayoutJob) + UI-SPEC | `overlay.rs` draw-closure discipline |
| `crates/modules/palette/src/raster.rs` | utility (egui→tiny-skia) | RESEARCH Pattern 1 lines 264-281 (tessellate→fill_path/barycentric) | `tray.rs` tiny-skia drawing + `renderer/mod.rs` premul tests |
| `crates/mybox-core/src/command.rs` (builtin bodies) | service | RESEARCH Pattern 3 lines 299-316 + Code Examples lines 480-502 | `module.rs` registry + `event.rs` emit |

Also note: egui/egui-winit/fuzzy-matcher/pollster are all NEW dependencies — no repo usage exists; all API signatures in RESEARCH are source-verified (RESEARCH §Sources, lines 639-648).

## Metadata

**Analog search scope:** `crates/mybox-core/src/` (app.rs, window.rs, event.rs, context.rs, module.rs, config.rs, hotkey.rs, error.rs, tray.rs, renderer/, lib.rs, bin/display_checks.rs), `crates/mybox-core/tests/integration.rs`, `crates/modules/test/`, `crates/modules/capture/` (lib.rs, session.rs, Cargo.toml), `crates/mybox-app/src/main.rs`, workspace `Cargo.toml`; planning docs: 03-CONTEXT.md, 03-SPEC.md, 03-RESEARCH.md, 02-PATTERNS.md, 02-01-SUMMARY.md.
**Files scanned:** 16 source files + 4 planning docs
**Pattern extraction date:** 2026-08-14
**Key facts confirmed from source:**
- `FrameworkEvent::AppExit` exists (`event.rs:47`) but no handler — C5 confirmed necessary.
- `WindowKind::Floating` profile lacks `.with_resizable(false)` and `create_window` focuses only `Overlay` (`app.rs:322-335`) — C4 confirmed.
- `WindowSpec` has `on_event`/`on_draw` but no `on_event_win` (`window.rs:29-49`) — C3 confirmed.
- `ModuleContext::new` is called in 4 places (2 prod + 2 test) — adding the commands param touches all 4.
- `AppBuilder::build` (`app.rs:118-155`) is the registry assembly point; `ModuleRegistry::register` duplicate-rejection (`module.rs:51-58`) is the CommandRegistry template.
- Capture `start_capture` (lib.rs:206-252) already holds the re-entrancy guard and injectable fns the palette capture-command runner must reuse.
- `config.rs:158 config_dir()` + `config.rs:102 get(module, key)` serve D-12 and `[palette].hotkey`.
- Workspace deps (`Cargo.toml:12-31`) contain no egui/fuzzy-matcher/pollster yet — RESEARCH versions verified, behind the version-lock comment.
