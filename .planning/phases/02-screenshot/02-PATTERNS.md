# Phase 2: 截图模块 - Pattern Map

**Mapped:** 2026-08-13
**Files analyzed:** 16 (12 new + 4 modified)
**Analogs found:** 14 / 16 (2 partial-with-no-direct-analog: text.rs, clipboard.rs — covered by RESEARCH patterns)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/modules/capture/Cargo.toml` (new) | config | request-response | `crates/modules/test/Cargo.toml` | exact |
| `crates/modules/capture/src/lib.rs` (new) | module (controller) | event-driven | `crates/modules/test/src/lib.rs` | exact |
| `crates/modules/capture/src/session.rs` (new) | service / state | event-driven | `crates/modules/test/src/lib.rs` (Arc-shared state test) + `crates/mybox-core/src/event.rs` tests (`Arc<Mutex<…>>`) | role-match |
| `crates/modules/capture/src/capture.rs` (new) | service | batch / async worker | `crates/mybox-core/src/event.rs` worker-thread + `crates/mybox-core/src/context.rs` `UiThreadProxy::run` | role-match |
| `crates/modules/capture/src/overlay.rs` (new) | component | event-driven + request-response | `crates/modules/test/src/lib.rs` (WindowSpec) + `crates/mybox-core/src/bin/display_checks.rs` (draw→present) | role-match |
| `crates/modules/capture/src/selection.rs` (new) | utility | transform (pure) | `crates/mybox-core/src/renderer/mod.rs` (pure fn + headless tests) | role-match |
| `crates/modules/capture/src/annotate.rs` (new) | model / service | CRUD (retained list) | `crates/mybox-core/src/tray.rs` `generate_icon_rgba` (tiny-skia path drawing) | role-match |
| `crates/modules/capture/src/toolbar.rs` (new) | component | transform | `crates/mybox-core/src/tray.rs` + `crates/mybox-core/src/renderer/mod.rs` pure-fn pattern | partial |
| `crates/modules/capture/src/text.rs` (new) | utility | transform | `crates/mybox-core/src/renderer/mod.rs` pure-fn pattern (ab_glyph is new) | partial |
| `crates/modules/capture/src/clipboard.rs` (new) | service | CRUD | `crates/mybox-core/src/error.rs` (thiserror) + RESEARCH arboard example | partial (new dep) |
| `crates/modules/capture/src/permission.rs` (new) | utility | request-response | `crates/mybox-core/src/app.rs` `#[cfg(target_os = "macos")]` block | partial |
| `crates/modules/capture/tests/` (new) | test | - | `crates/mybox-core/tests/integration.rs` + `crates/mybox-core/src/bin/display_checks.rs` | exact |
| `crates/mybox-core/src/window.rs` (modify) | config | request-response | self-analog: `WindowSpec.on_event` field + `WindowRequest` enum | exact |
| `crates/mybox-core/src/app.rs` (modify) | controller | event-driven | self-analog: `window_event` RedrawRequested + `about_to_wait` drain; `display_checks.rs` draw-then-present | exact |
| `Cargo.toml` workspace (modify) | config | - | self-analog: `[workspace] members` + `[workspace.dependencies]` | exact |
| `crates/mybox-app/src/main.rs` (modify) | config | - | self-analog: register module in `main()` | exact |

**Crate naming:** follow `mybox-test` precedent — directory `crates/modules/capture/`, package name **`mybox-capture`** (02-VALIDATION.md already uses `-p mybox-capture`).

---

## Pattern Assignments

### `crates/modules/capture/Cargo.toml` (config)

**Analog:** `crates/modules/test/Cargo.toml` (lines 1-8)

The module crate depends ONLY on `mybox-core` by path — the FRMW-02 module boundary. Copy the shape, then add the Phase 2 deps (xcap, arboard, ab_glyph, macOS-only objc2-core-graphics):

```toml
# crates/modules/test/Cargo.toml (lines 1-8) — copy this shape
[package]
name = "mybox-test"
version = "0.1.0"
edition = "2021"
description = "Test module used to validate the mybox module framework"

[dependencies]
mybox-core = { path = "../../mybox-core" }
```

```toml
# crates/modules/capture/Cargo.toml — Phase 2 additions (per RESEARCH Installation §)
[dependencies]
mybox-core = { path = "../../mybox-core" }
xcap = "0.9.8"          # or workspace dep once pinned; behind checkpoint:human-verify
arboard = "3.6.1"       # behind checkpoint:human-verify
ab_glyph = "0.2.32"     # behind checkpoint:human-verify

[target.'cfg(target_os = "macos")'.dependencies]
objc2-core-graphics = "0.3.2"   # already in tree via xcap/arboard — no new objc2 major
```

---

### `crates/modules/capture/src/lib.rs` (module/controller, event-driven)

**Analog:** `crates/modules/test/src/lib.rs` — EXACT template. This is the module skeleton: struct + `Module` impl (id/name/default_config/menu_items/init) + hotkey subscription + window enqueue + headless tests.

**Imports pattern** (`crates/modules/test/src/lib.rs` lines 13-20):
```rust
use mybox_core::anyhow;
use mybox_core::event::{EventFilter, EventPayload, FrameworkEvent};
use mybox_core::log;
use mybox_core::module::Module;
use mybox_core::toml;
use mybox_core::tray_icon;
use mybox_core::window::{WindowKind, WindowSpec};
use mybox_core::ModuleContext;
```

**Module impl skeleton** (lines 22-52):
```rust
pub struct CaptureModule;

impl Module for CaptureModule {
    fn id(&self) -> &'static str { "capture" }          // = event `from` + config section
    fn name(&self) -> &str { "截图" }
    fn default_config(&self) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert("hotkey".to_string(), toml::Value::String("Cmd+Shift+S".to_string()));
        table
    }
    fn menu_items(&self) -> Vec<tray_icon::menu::MenuItem> {
        vec![tray_icon::menu::MenuItem::with_id("capture.start", "开始截图", true, None)]
    }
    // ...
}
```

**Hotkey → bus event → window-request core pattern** (`init`, lines 54-78) — the capture module's `init` must clone BOTH the window handle AND the `UiThreadProxy` into the handler (the capture flow needs a main-thread hop, unlike TestModule which only enqueues):
```rust
fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()> {
    let windows = ctx.windows().clone();      // thread-safe enqueue (W2/W3)
    let ui = ctx.ui().clone();                 // NEW: capture needs main-thread hop
    ctx.on(
        EventFilter::kind("core", "hotkey.triggered"),
        Box::new(move |e| {
            if let EventPayload::Framework(FrameworkEvent::HotkeyTriggered { action, .. }) = &e.payload {
                if action == "start_screenshot" {
                    log::info!("capture: hotkey 'start_screenshot' triggered");
                    // Phase 2: permission preflight (CAP-08) → spawn capture thread
                    // → ui.run(...) → enqueue per-monitor WindowRequest::Create
                }
            }
        }),
    );
    Ok(())
}
```

**Test scaffold** (lines 81-209): `sample_context()` building a headless `ModuleContext` over a fresh `EventBus` + `WindowManagerHandle`; the `wait_until` polling helper (dispatch is async on the bus worker thread); emit a synthetic `core`/`hotkey.triggered` and poll `handle.try_recv()` for the enqueued `WindowRequest`. Copy wholesale.

---

### `crates/modules/capture/src/session.rs` (service, event-driven state machine)

**Analog (role-match):** shared-state pattern in `crates/mybox-core/src/event.rs` tests + `crates/modules/test/src/lib.rs` — `Arc<Mutex<…>>` state shared between the bus worker thread and the main thread.

**Shared-state pattern** (`crates/modules/test/src/lib.rs` lines 161-168 — Arc<Mutex> shared across threads):
```rust
let received: Arc<std::sync::Mutex<Option<mybox_core::WindowRequest>>> =
    Arc::new(std::sync::Mutex::new(None));
let got = received.clone();
// clone the Arc into the worker closure; lock() to read on the main thread
```

The session state machine (Idle → Selecting → Selected → Annotating → Confirm/Cancel) has **no direct analog** — it is new pure-logic. Pattern to follow:
- `pub struct CaptureSession { state: Arc<Mutex<SessionState>> }` — clone the `Arc` into every closure (bus handler, `on_event`, draw closure).
- `SessionState` holds: per-monitor captured `RgbaImage`s, selection rect, current tool, retained `Vec<Annotation>` (undo-by-pop), overlay `WindowId`s.
- All transitions are methods on the session (`start/on_mouse_down/on_mouse_move/confirm/cancel`) so they are headless unit-testable exactly like the `event_bus` tests use the `wait_until` + `Arc<Mutex>` pattern.
- Use `parking_lot::Mutex` (workspace-pinned, lower overhead) — consistent with `event.rs` / `context.rs`.

---

### `crates/modules/capture/src/capture.rs` (service, batch/async worker)

**Analog (role-match):** worker-thread spawn in `crates/mybox-core/src/event.rs` (lines 117-135) + main-thread hop via `UiThreadProxy` in `crates/mybox-core/src/context.rs` (lines 126-138).

**Worker-thread + main-thread handoff** (`context.rs` lines 126-138):
```rust
pub fn run(&self, f: Box<dyn FnOnce() + Send>) {
    let mut inner = self.inner.lock();
    if let Some(proxy) = &inner.proxy {
        let _ = proxy.send_event(AppEvent::Ui(f));
    } else {
        inner.pending.push(f);
    }
}
```

**Spawn-thread shape** (from `event.rs` lines 117-122 — named thread builder):
```rust
let worker = std::thread::Builder::new()
    .name("mybox-event-bus".to_string())
    .spawn(move || { /* ... */ })
    .expect("spawn event-bus worker thread");
```

**Phase 2 composition** (RESEARCH Pattern 4, lines 262-272 — capture runs off the main loop, results forwarded via `UiThreadProxy::run`, which becomes `AppEvent::Ui(f)` executed in `user_event`):
```rust
let ui = ctx.ui().clone();
std::thread::spawn(move || {
    let result: anyhow::Result<Vec<(MonitorGeom, RgbaImage)>> = capture_all_monitors();
    ui.run(Box::new(move || match result {
        Ok(shots) => { /* *session.state.lock() = ...; enqueue creates */ }
        Err(e) => log::error!("capture failed: {e:#}"),
    }));
});
```
**Invariant:** capture ALL monitors BEFORE any overlay window exists (Pitfall 1 — xcap uses `OptionAll` and would capture the overlay itself). Never capture inside `on_event`/draw (Pitfall 4).

---

### `crates/modules/capture/src/overlay.rs` (component, event-driven + request-response)

**Analog (role-match):** `crates/modules/test/src/lib.rs` WindowSpec construction (lines 67-72) + `crates/mybox-core/src/bin/display_checks.rs` WindowHarness draw→present (lines 86-99).

**WindowSpec struct-literal from a module crate** (`modules/test/src/lib.rs` lines 67-72):
```rust
windows.create(WindowSpec {
    kind: WindowKind::Panel,
    title: "mybox test".to_string(),
    inner_size: Some((400, 300)),
    ..Default::default()
});
```

**Phase 2 per-monitor overlay spec** (RESEARCH Pattern 3 — physical-pixel geometry; `WindowKind::Overlay` profile already exists in `window.rs` lines 76-82):
```rust
// One WindowSpec per monitor; position/size in PHYSICAL pixels
// geom = xcap monitor points × scale_factor (the ONLY logical→physical conversion)
let spec = WindowSpec {
    kind: WindowKind::Overlay,
    title: "capture-overlay".to_string(),
    inner_size: Some((geom.width, geom.height)),
    position: Some((geom.x, geom.y)),
    on_event: Some(Box::new(move |event| session.handle_event(event))),  // state machine
    // + on_draw (NEW core field, Phase 2): the draw closure
    ..Default::default()
};
```

**on_event routing already wired** — `app.rs` `window_event` (lines 366-369) calls `state.spec.on_event(&event)` before the renderer match; the module closure runs on the main thread (safe for clipboard + window destroy):
```rust
if let Some(state) = self.windows.get_mut_by_winit(id) {
    if let Some(cb) = &state.spec.on_event {
        cb(&event);
    }
    match event { /* ... */ }
}
```

**Draw-then-present shape** (the closest analog for the new draw chain — `display_checks.rs` lines 86-99):
```rust
if let WindowEvent::RedrawRequested = event {
    if let Some(state) = self.wm.get_mut_by_winit(id) {
        state.renderer.draw(&mut |pixmap, _w, _h| {
            pixmap.fill(tiny_skia::Color::from_rgba8(0x30, 0x90, 0xC0, 0xFF));
        });
        state.renderer.present().expect("present on RedrawRequested");
    }
}
```
**Phase 2 gap:** `app.rs` RedrawRequested (lines 371-375) currently calls only `present()`, NOT `draw()`. The module supplies the composite closure (capture blit → mask → selection → annotations → toolbar) via the new `WindowSpec.on_draw` field; the closure re-renders the full frame from `Arc<Mutex<SessionState>>` every redraw (immediate-mode, retained annotations — never accumulate pixels).

---

### `crates/modules/capture/src/selection.rs` (utility, pure transform)

**Analog (role-match):** pure-function + headless-test pattern in `crates/mybox-core/src/renderer/mod.rs` (`premul_rgba_to_u32`, lines 37-48, tests lines 50-109).

**Pure fn pattern** (`renderer/mod.rs` lines 37-48):
```rust
pub fn premul_rgba_to_u32(r: u8, g: u8, b: u8, a: u8) -> u32 {
    if a == 0 { return 0x0000_0000; }
    if a == 255 { return (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b); }
    // ...
}
```
- `selection.rs` = drag-select math + 8-handle hit-test + resize logic, written as pure functions/structs with `#[cfg(test)]` unit tests (headless, no winit). Same shape: `hit_test_handle(selection, pos) -> Option<Handle>`, `apply_handle_drag(...)`, `normalize(selection)`.
- Use `tiny_skia::Point`/`Rect` types directly (already a workspace dep).

---

### `crates/modules/capture/src/annotate.rs` (model/service, CRUD retained list)

**Analog (role-match):** tiny-skia path-drawing pattern in `crates/mybox-core/src/tray.rs` `generate_icon_rgba` (lines 55-77) — the only existing tiny-skia path/fill code in the repo.

**tiny-skia drawing pattern** (`tray.rs` lines 55-77):
```rust
let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("pixmap allocation");
pixmap.fill(tiny_skia::Color::TRANSPARENT);
let mut paint = tiny_skia::Paint::default();
paint.set_color_rgba8(255, 255, 255, 255);
let mut path_builder = tiny_skia::PathBuilder::new();
path_builder.push_circle(size as f32 / 2.0, size as f32 / 2.0, size as f32 * 0.32);
let path = path_builder.finish().expect("valid circle path");
pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding,
    tiny_skia::Transform::identity(), None);
```
- **New pattern (no analog):** `enum Annotation { Rect/Arrow/Pen/Text }` retained in a `Vec<Annotation>`; undo = `pop()` + full redraw from the retained list (never mutate pixels — RESEARCH Anti-Pattern). Draw via `PathBuilder` + `stroke_path`/`stroke_rect` on the `PixmapMut` inside the draw closure.
- Follow `tray.rs`'s separation: pure/headless-testable core fn + thin wrapper, so `annotate::tests` can assert pixel output without a window (same style as `renderer/mod.rs` tests asserting `premul_rgba_to_u32`).

---

### `crates/modules/capture/src/toolbar.rs` (component, transform)

**Analog (partial):** pure-fn pattern of `renderer/mod.rs` + tiny-skia drawing of `tray.rs`. No toolbar exists — it is new.
- Store button rects (tiny-skia `Rect`s) as plain data; draw with `fill_rect`/`stroke_rect`; resolve clicks by hit-testing stored rects inside `on_event` (RESEARCH §Overlay rendering — NO egui in the overlay).
- Keep layout + hit-test as pure functions (`layout_buttons(window_size) -> Vec<Button>`, `hit_test(buttons, pos)`) for headless unit tests — same discipline as `selection.rs`.

---

### `crates/modules/capture/src/text.rs` (utility, transform)

**Analog (partial):** pure-fn pattern of `renderer/mod.rs`. ab_glyph is a NEW dependency — no repo analog; use RESEARCH Code Example (lines 408-426): `FontArc::try_from_slice(include_bytes!(...))` with `/System/Library/Fonts/Supplemental/Arial.ttf` fallback (A4), `outline_glyph(...).draw(|gx, gy, cov| /* blend coverage into Pixmap */)`. Wrap as a pure function (e.g. `draw_text(pm, font, text, at, size)`) so text rendering is headless-testable.

---

### `crates/modules/capture/src/clipboard.rs` (service, CRUD)

**Analog (partial):** error-mapping pattern in `crates/mybox-core/src/error.rs` (lines 7-39) — map new library errors through typed errors; arboard is a NEW dependency. Follow the `MyboxError` From-bridge precedent (e.g. `From<softbuffer::SoftBufferError>` lines 57-61) if a capture/clipboard error enum is added, or return `anyhow::Result` at the module boundary (TestModule/CaptureModule `init` returns `anyhow::Result`).

**RESEARCH Code Example** (lines 357-370) — the confirmed arboard shape: create → set_image → drop in one confined main-thread scope (Pitfall 6, Windows thread-affinity):
```rust
{
    let mut cb = Clipboard::new().map_err(anyhow::Error::msg)?; // drop before loop/exit
    cb.set_image(ImageData { width: w as usize, height: h as usize, bytes: Cow::Owned(crop) })?;
    // dropped here — confined scope
}
```
- Crop = manual axis-aligned sub-rect copy of `RgbaImage.as_raw()` (RGBA8 straight — matches arboard `ImageData` exactly, no format conversion).
- **New helper (unit-tested, RESEARCH Don't-Hand-Roll):** `premultiply_rgba8(&[u8]) -> Vec<u8>` for the straight→premultiplied conversion when feeding `tiny_skia::Pixmap::from_vec` (Pitfall 2); reuse the `premul_rgba_to_u32` test style.

---

### `crates/modules/capture/src/permission.rs` (utility, request-response)

**Analog (partial):** `#[cfg(target_os = "macos")]` conditional block pattern in `crates/mybox-core/src/app.rs` (lines 199-204) and `config.rs` tests (lines 230-237):
```rust
#[cfg(target_os = "macos")]
{
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    builder.with_activation_policy(ActivationPolicy::Accessory);
}
```
**Phase 2** (RESEARCH Code Example lines 428-441): `CGPreflightScreenCaptureAccess()` / `CGRequestScreenCaptureAccess()` via objc2-core-graphics, gated by `#[cfg(target_os = "macos")]`, with a non-macOS stub returning `true` so the rest of the module compiles/tests on any host. Make the checker **injectable** (e.g. `permission::has_access()` behind a function pointer/trait) so `permission::tests` can simulate the denied path headlessly (02-VALIDATION CAP-08).

---

### `crates/modules/capture/tests/` (test)

**Analog:** `crates/mybox-core/tests/integration.rs` (subprocess-per-check) + `crates/mybox-core/src/bin/display_checks.rs` — EXACT template for the `#[ignore]` display suite.

**Subprocess-per-check harness** (`integration.rs` lines 22-38):
```rust
const CHECKS_BIN: &str = env!("CARGO_BIN_EXE_display_checks");
fn run_check(name: &str) {
    let status = Command::new(CHECKS_BIN).arg(name).status()
        .unwrap_or_else(|e| panic!("failed to spawn display_checks '{name}': {e}"));
    assert!(status.success(), "display_checks '{name}' exited with {status:?}");
}
#[test]
#[ignore]
fn overlay_window_creates() { run_check("overlay"); }
```
- **Why subprocess:** winit macOS requires `EventLoop` on the real main thread, one per process (documented `integration.rs` lines 12-20). The capture module's `#[ignore]` end-to-end (overlay shows capture, drag, Enter copies, ESC closes) must reuse this pattern — the capture checks bin runs one flow per subprocess on its own main thread.

---

### `crates/mybox-core/src/window.rs` (modify)

**Analog:** self — add a field to `WindowSpec` mirroring the existing `on_event` closure field, and a variant to `WindowRequest` mirroring the existing enum.

**Copy the `on_event` closure-field pattern** (`window.rs` lines 40-41) for the new `on_draw`:
```rust
/// Per-window event callback (D-07 routing target).
pub on_event: Option<Box<dyn Fn(&winit::event::WindowEvent) + Send + Sync>>,
// NEW (Phase 2):
// pub on_draw: Option<Box<dyn Fn(&mut tiny_skia::PixmapMut, u32, u32) + Send + Sync>>,
```
Add it to `impl Default` (lines 44-58: `on_draw: None`) and to the `WindowSpec` struct-literal test at lines 336-348 (which asserts the public-field contract). The existing `window_spec_default_is_panel_decorated_visible` test (lines 322-333) must gain an `on_draw.is_none()` assertion.

**Copy the `WindowRequest` enum shape** (`window.rs` lines 149-152):
```rust
pub enum WindowRequest {
    Create(WindowSpec),
    Destroy(WindowId),
    // NEW (Phase 2): Redraw(WindowId) — reuses the enqueue→drain architecture
}
```
RESEARCH Pattern 2 (lines 232-238): `WindowRequest::Redraw(id)` drained in `App::about_to_wait` calls `state.window.request_redraw()` — the module requests repaints from the bus thread without ever holding the winit `Window`.

**Also:** decide `batch_create` fate (RESEARCH Open Question 3) — the placeholder at lines 301-305 returns fake ids; only its own test (line 527) references it. Either delete the placeholder + test, or repurpose the module-side per-monitor loop as "the batch".

---

### `crates/mybox-core/src/app.rs` (modify)

**Analog:** self — extend the existing `RedrawRequested` arm and `about_to_wait` drain; the draw-then-present shape is already proven in `display_checks.rs`.

**Current RedrawRequested arm** (`app.rs` lines 370-375) — Phase 2 inserts `draw()` before `present()`:
```rust
winit::event::WindowEvent::RedrawRequested => {
    if let Err(e) = state.renderer.present() {
        log::warn!("renderer present failed: {e}");
    }
}
// becomes (RESEARCH Pattern 1):
//   if let Some(draw) = &state.spec.on_draw {
//       state.renderer.draw(&mut |pixmap, w, h| draw(pixmap, w, h));
//   }
//   if let Err(e) = state.renderer.present() { log::warn!("renderer present failed: {e}"); }
```

**Current about_to_wait drain** (`app.rs` lines 406-420) — add a `Redraw` arm:
```rust
fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
    while let Ok(req) = self.window_rx.try_recv() {
        match req {
            WindowRequest::Create(spec) => { /* create_window */ }
            WindowRequest::Destroy(id) => { self.windows.destroy(id); }
            // NEW: WindowRequest::Redraw(id) => if let Some(s) = self.windows.get_mut(id) {
            //   if let Some(w) = &s.window { w.request_redraw(); } }
        }
    }
    el.set_control_flow(winit::event_loop::ControlFlow::Wait);
}
```
**Test to add** (02-VALIDATION): `app::tests::redraw_draws_then_presents` using the existing `MockRenderer` (lines 431-438) — record `draw` and `present` calls and assert draw precedes present. Keep `ControlFlow::Wait` (no 60fps poll — Pitfall 3).

---

### `Cargo.toml` workspace (modify)

**Analog:** self. Add the member and pin the new deps version-locked in `[workspace.dependencies]`:
```toml
[workspace]
members = ["crates/mybox-core", "crates/mybox-app", "crates/modules/test", "crates/modules/capture"]
resolver = "2"

[workspace.dependencies]
# add (behind checkpoint:human-verify per Package Legitimacy Audit):
# xcap = "0.9.8"
# arboard = "3.6.1"
# ab_glyph = "0.2.32"
```
The header comment (lines 10-11) documents the version-lock discipline: "Do NOT bump any of these without re-verifying cross-compatibility."

---

### `crates/mybox-app/src/main.rs` (modify)

**Analog:** self (lines 9-16):
```rust
use mybox_core::App;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut builder = App::builder();
    builder.module(Box::new(mybox_test::TestModule))?;
    builder.module(Box::new(mybox_capture::CaptureModule))?;   // NEW
    builder.build()?.run()
}
```
Crate name: `mybox-capture` → Rust crate `mybox_capture`.

---

## Shared Patterns

### Module crate boundary (FRMW-02)
**Source:** `crates/modules/test/src/lib.rs` + `crates/mybox-core/src/lib.rs` re-exports (lines 19-41)
**Apply to:** All capture crate files
- Module crates depend ONLY on `mybox-core`; every type (Module, ModuleContext, events, WindowSpec, `toml`/`tray_icon`/`anyhow`/`log` re-exports) comes from the framework public API. `mybox_core::log`, `mybox_core::toml`, `mybox_core::anyhow` used unqualified — do NOT add the real crates to the module's deps.

### Module impl contract
**Source:** `crates/mybox-core/src/module.rs` (lines 12-36)
**Apply to:** `lib.rs` CaptureModule
- `id()` = event `from` + config section name = `"capture"`; `name()`, `default_config()` (D-13 section), `menu_items()`, `init()` (register handlers exactly once).

### Hotkey → bus event → enqueue window request
**Source:** `crates/modules/test/src/lib.rs` (lines 54-78); dispatch in `app.rs` `on_hotkey` (lines 285-299)
**Apply to:** `lib.rs` init handler
- Subscribe with `EventFilter::kind("core", "hotkey.triggered")`; match `EventPayload::Framework(FrameworkEvent::HotkeyTriggered { action, .. })`; branch on the action string (`"start_screenshot"`); clone `ctx.windows()` into the `Box<dyn Fn + Send + Sync>` closure (runs on the bus worker thread). Never execute arbitrary code from event payloads (T-1-02).

### Event filter / subscription
**Source:** `crates/mybox-core/src/event.rs` (lines 56-76 filter, 156-164 on)
**Apply to:** `lib.rs`, `session.rs`
- `EventFilter::kind(from, kind)` with `"*"` wildcards; handlers are `Box<dyn Fn(&Event) + Send + Sync>`; dispatch is async on the bus worker thread — tests must poll (the `wait_until` helper).

### Main-thread hop via UiThreadProxy
**Source:** `crates/mybox-core/src/context.rs` (lines 126-138) + `app.rs` `user_event` (lines 392-400, `AppEvent::Ui(f) => f()`)
**Apply to:** `capture.rs` (capture results), `overlay.rs` (clipboard confirm is already on the main thread via `on_event` — no hop needed there)
- `ctx.ui().clone()` → `ui.run(Box::new(move || { ... }))` forwards a closure to the winit main thread. Used to move capture results off the worker thread into `SessionState` and to enqueue window creates (which must stay main-thread-bound, W2).

### Threading discipline
**Source:** `app.rs` module docs (lines 10-20) + `event.rs` worker thread (lines 117-135)
**Apply to:** `capture.rs`, `session.rs`
- Bus dispatches on a worker thread; winit windows/`ActiveEventLoop` are main-thread-bound; heavy ops (xcap capture) spawn a named worker thread (Pitfall 4); window creates go through `WindowRequest` + wake hook, never called from the bus thread (W2/W3).

### Error handling
**Source:** `crates/mybox-core/src/error.rs` (thiserror `MyboxError`, lines 7-39) + module boundary returns `anyhow::Result` (`module.rs` line 22)
**Apply to:** `capture.rs`, `clipboard.rs`, `permission.rs`
- Core uses typed `thiserror` errors with `From` bridges (lines 57-61); module boundary (`init`, capture, clipboard) returns `anyhow::Result` with context. Never panic in event handlers (bus `catch_unwind` at `event.rs` lines 128-131); guard malformed state (empty pen lists, zero-size crops — Security Domain).

### Headless unit-test discipline
**Source:** `renderer/mod.rs` tests, `event.rs` `event_bus` module, `modules/test/src/lib.rs` tests, `config.rs` tests
**Apply to:** All capture sub-modules
- Pure logic (selection, annotation, toolbar hit-test, premultiply helper, crop) is unit-tested headlessly with `cargo nextest run -p mybox-capture <module>::tests`. Real-display behavior is `#[ignore]` + subprocess-per-check. Tests never touch the real user config dir (`config.rs` uses temp paths) and never call `init()` on OS-bound managers (`hotkey.rs` test comment line 132).

### Rendering pipeline
**Source:** `crates/mybox-core/src/renderer/mod.rs` (Renderer trait lines 18-28, `premul_rgba_to_u32` lines 37-48) + `tiny_skia_softbuffer.rs` (draw lines 66-68, present lines 70-84)
**Apply to:** `overlay.rs` draw closure, `annotate.rs`, `toolbar.rs`, `text.rs`
- Renderer `draw` receives `PixmapMut` + size; composite order (RESEARCH Architecture Diagram): capture blit → mask (4 semi-transparent `fill_rect`s) → selection border + handles + WxH → retained annotations → toolbar. Pixmap is premultiplied — convert straight `RgbaImage` via the new `premultiply_rgba8` helper (Pitfall 2).

## No Analog Found

Files with no close match in the codebase (planner should use RESEARCH.md code examples / patterns instead):

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/modules/capture/src/text.rs` | utility | transform | ab_glyph is a new dependency; no text rendering exists in the repo. Use RESEARCH Code Example (lines 408-426). |
| `crates/modules/capture/src/clipboard.rs` | service | CRUD | arboard is a new dependency; no clipboard code exists. Use RESEARCH Code Example (lines 357-370) + `error.rs` From-bridge style. |
| `crates/modules/capture/src/session.rs` (state machine body) | service | event-driven | No state machine exists in the repo (framework is stateless services). Pattern derived from Arc<Mutex> sharing + headless test discipline. |
| `crates/modules/capture/src/permission.rs` (FFI body) | utility | request-response | objc2-core-graphics calls are new; only the `#[cfg(target_os = "macos")]` gating has an analog (app.rs lines 199-204). |

## Metadata

**Analog search scope:** `crates/mybox-core/src/` (app.rs, window.rs, event.rs, context.rs, module.rs, config.rs, hotkey.rs, error.rs, tray.rs, renderer/, bin/display_checks.rs), `crates/mybox-core/tests/integration.rs`, `crates/modules/test/`, `crates/mybox-app/src/main.rs`, workspace `Cargo.toml`, Phase 1 plans/summaries.
**Files scanned:** 17 source files + 3 planning docs (01-CONTEXT, 01-04-SUMMARY, 02-RESEARCH, 02-VALIDATION)
**Pattern extraction date:** 2026-08-13
**Key gaps confirmed from source:** app.rs RedrawRequested calls only `present()` (no `draw()`, app.rs:371-375); `batch_create` is a fake-id placeholder (window.rs:301-305); `WindowSpec` has `on_event` but no `on_draw` (window.rs:29-42); `WindowRequest` has no `Redraw` variant (window.rs:149-152); no xcap/arboard/ab_glyph/objc2-core-graphics in workspace deps (Cargo.toml:12-28).
