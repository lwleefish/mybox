# Phase 2: 截图模块 - Research

**Researched:** 2026-08-13
**Domain:** Rust native screen capture, fullscreen overlay interaction, tiny-skia annotation, clipboard, macOS permissions
**Confidence:** HIGH

## Summary

Phase 2 builds the first real feature module on the Phase 1 framework (winit 0.30.13 + tiny-skia 0.12.0 + softbuffer 0.4.8 + crossbeam event bus). The module lives in a new crate `crates/modules/capture/`, follows the TestModule pattern (subscribe `core/hotkey.triggered` -> capture -> create overlay windows -> interact -> clipboard -> destroy), and exercises five Phase 1 extension points that were deliberately left for Phase 2: the `Renderer::draw` call chain, `WindowManager::batch_create` (D-09 per-monitor overlay), `WindowSpec.on_event` mouse/keyboard handling, `UiThreadProxy` for capture-thread results, and new dependencies (xcap, arboard, ab_glyph).

**Screen capture: use `xcap 0.9.8`, not `screenshots` or `scrap`.** The `screenshots` crate's own crates.io description now reads "Move to XCap" — the ecosystem has consolidated on xcap, which ships `Monitor::all()` (x/y/width/height/scale_factor per monitor) and `capture_image() -> image::RgbaImage` (RGBA8 straight alpha), uses the same objc2 0.6.x stack already locked in the workspace, and handles macOS via `CGWindowListCreateImage` (Screen Recording permission required, silent black output without it). Verified by reading the cached 0.9.8 source.

**Overlay rendering: pure tiny-skia, NO egui in the overlay window.** `egui-tiny-skia` does not exist on crates.io (verified — D-01's Phase-1 reference to it was inaccurate; Phase 1's own SKELETON records "egui-tiny-skia 不存在"). Pitfall 5 recommends exactly this split (截图覆盖窗口只用 tiny-skia, egui for Phase 3 palette). The toolbar (rect/arrow/pen/text/undo/confirm/cancel) is drawn with tiny-skia `fill_rect`/`stroke_rect` + text, and clicks are resolved by hit-testing stored button rects in `on_event`. This avoids the egui manual-tessellation problem entirely and keeps the overlay dependency-light.

**The critical architecture insight: the capture overlay window does NOT need real macOS transparency.** softbuffer on macOS drops per-pixel alpha (`CGImageAlphaInfo::NoneSkipFirst` — Phase 1 SKELETON deferred "真实 alpha" to Phase 2), but the overlay never needs window transparency: it is sized exactly to a monitor and every pixel is either the captured screen image or mask/annotation drawn *on top of* the image inside the tiny-skia Pixmap. The semi-transparent black mask is composited in-Pixmap (tiny-skia supports alpha internally), then presented as an opaque framebuffer. Capture-then-create ordering (capture all monitors first, then create windows) also avoids the self-capture gotcha (Pitfall integration table).

**Required core changes are small and well-scoped** (all flagged as Phase 2 gaps in CONTEXT.md): (1) `App::window_event` `RedrawRequested` must call `renderer.draw(spec.on_draw_closure)` before `present()`; (2) a redraw-trigger path so the module can request repaints on input (recommend extending `WindowRequest` with a `Redraw(WindowId)` variant drained in `about_to_wait`, or an `on_created` callback handing the module the `Arc<Window>`); (3) `batch_create` real implementation — realized as the module enqueuing one `WindowRequest::Create` per monitor with per-monitor geometry/capture, rather than a new core batch API (window creation must stay on the main thread per W2).

**Primary recommendation:** New crate `crates/modules/capture/` (id `"capture"`) with a `CaptureSession` state machine (Idle -> Selecting -> Selected -> Annotating -> Confirm/Cancel), retained annotation list + undo-by-pop (never mutate pixels — redraw from the retained list), xcap on a background thread with results forwarded via `UiThreadProxy`, arboard `set_image` in a confined scope on the main thread at confirm time, and `CGPreflightScreenCaptureAccess`/`CGRequestScreenCaptureAccess` via `objc2-core-graphics` for CAP-08.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### 截图流程编排
- **D-01:** 整体流程为 Select -> Annotate -> Confirm。用户按热键触发截图 -> 捕获屏幕画面 -> 覆盖窗口出现 -> 拖拽选择区域 -> 标注工具栏出现 -> 用户可选标注 -> 按 Enter 或工具栏确认按钮 -> 当前图像（含标注）复制到剪贴板 -> 覆盖窗口关闭。不标注直接确认则复制原始选区图像。
- **D-02:** 选择区域后进入"可调整选择"阶段。拖拽完成后显示 8 个拖拽手柄（四角 + 四边中点），用户可在开始标注前或标注过程中随时调整选区位置和大小。不设显式模式切换。
- **D-03:** 工具栏采用统一模式（no modes）。拖拽完成后在选区附近显示工具栏，同时包含标注工具按钮和选择手柄。用户可拖拽手柄调整选区，或点击工具按钮开始标注——当前工具选择决定操作行为，无需显式模式切换。选区手柄和标注工具可同时使用。
- **D-04:** 确认方式为 Enter 键或工具栏确认按钮。确认后当前选区图像（含标注）复制到剪贴板，覆盖窗口立即关闭。ESC 键取消整个截图流程，覆盖窗口立即关闭，不复制任何内容。行为简单可预测——ESC 不是分步撤销，而是一键取消全部。

### Claude's Discretion
- 屏幕捕获库选择（xcap vs screenshots vs scrap）— 技术决策，由 researcher/planner 决定
- 标注工具的具体绘制实现（tiny-skia 路径/形状 API）
- 工具栏的 UI 布局和视觉设计（egui 集成方式）
- 撤销栈的内部数据结构
- 选区手柄的视觉样式
- 尺寸标签（WxH）的显示位置和格式
- 覆盖窗口的渲染管线集成方式（如何将捕获画面 + 遮罩 + 选区 + 标注通过 Renderer::draw 闭包合成）
- batch_create 的真实实现方式（D-09 每屏一窗策略落地）
- macOS 权限检测的具体 API 调用（CGPreflightScreenCaptureAccess 等）
- 剪贴板复制的具体实现（arboard 库集成）

### Deferred Ideas (OUT OF SCOPE)
None - discussion stayed within phase scope.

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CAP-01 | 用户按热键触发截图，捕获所有显示器画面到内存 | xcap `Monitor::all()` + `capture_image()` on a background thread (Pitfall 4); results forwarded via `UiThreadProxy`; capture-before-window-create ordering; multi-monitor verified via per-monitor geometry. |
| CAP-02 | 全屏透明覆盖窗口显示，遮罩半透明黑色，选区内显示原始画面 | Overlay `WindowKind` profile already exists; in-Pixmap mask compositing (4 fill_rects outside selection); captured image `draw_pixmap` fills window; **no window-level transparency needed** (opaque framebuffer). Requires `Renderer::draw` call-chain fix in App. |
| CAP-03 | 用户通过鼠标拖拽选择截图区域，实时显示选区边框和尺寸（WxH 像素） | `WindowEvent::CursorMoved` gives `PhysicalPosition<f64>` (verified, 1:1 with capture pixels); selection state machine; border `stroke_rect` + WxH text label (ab_glyph). |
| CAP-04 | 用户确认截图后，选区图像复制到系统剪贴板 | arboard 3.6.1 `Clipboard::set_image(ImageData { width, height, bytes: RGBA8 straight })` (verified from source); xcap `RgbaImage` is RGBA8 straight — direct bytes; do on main thread in a confined scope; crop = manual sub-rect copy. |
| CAP-05 | 用户按 ESC 取消截图，覆盖窗口销毁 | KeyboardInput logical key ESC -> cancel state -> destroy all overlay windows via `WindowManagerHandle::destroy(id)` per window; ESC is full cancel (D-04). |
| CAP-06 | 截图标注工具：矩形框、箭头、画笔（自由路径）、文字 | tiny-skia `PathBuilder`/`stroke_path`/`fill_path` for rect/arrow/pen (API verified); text via ab_glyph `OutlinedGlyph::draw` coverage compositing (tiny-skia has NO text module — verified). |
| CAP-07 | 标注支持撤销（Ctrl+Z），可撤销到截图原始状态 | Retained `Vec<Annotation>` + undo = pop + full redraw from retained list (never mutate pixels); Ctrl+Z via `ModifiersChanged` state + logical key 'z'. |
| CAP-08 | macOS 首次截图时检测 Screen Recording 权限并引导用户授权 | `CGPreflightScreenCaptureAccess()` / `CGRequestScreenCaptureAccess()` exist in objc2-core-graphics 0.3.2 (verified from source); guidance UI + deep link `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`; xcap returns black silently without permission (must preflight, not silently produce black screenshots). |
</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Screen capture (all monitors) | Module layer (worker thread) | — | Capture is a heavy op (Pitfall 4) — must run off the main event-loop thread; module owns xcap calls. |
| Overlay window lifecycle (per monitor) | Framework (WindowManager / App main thread) | Module (enqueues requests) | winit windows are main-thread-bound (W2); module can only enqueue `WindowRequest`; App creates in `about_to_wait`. |
| Overlay content rendering (image + mask + selection + annotations + toolbar) | Framework render hook (`Renderer::draw` via `WindowSpec.on_draw`) | Module (supplies the draw closure + state) | Renderer owns the tiny-skia Pixmap + softbuffer present; module supplies content via the draw-closure slot (D-03). |
| Mouse/keyboard interaction (drag select, handles, annotation drawing, confirm/cancel) | Module (via `WindowSpec.on_event`) | Framework (routes events by winit id, D-07) | `on_event` is the per-window callback; module implements the state machine there. |
| Selection/annotation state + undo | Module (retained model) | — | Pure module logic; no framework involvement. |
| Clipboard write | Module (main thread, confirm path) | arboard | Confirm fires in `on_event` (main thread) — safest place for clipboard (Windows thread-affine). |
| macOS Screen Recording permission | Module (macOS-only) | objc2-core-graphics | Preflight/request on hotkey; guidance UI in module. |
| DPI / coordinate correctness | Module (physical-pixel discipline) | winit (PhysicalPosition) | All coordinates physical; CursorMoved is already physical; xcap monitor geometry in points must be scaled by `scale_factor()`. |

## Standard Stack

### Core (new dependencies for Phase 2)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| xcap | 0.9.8 | Screen capture (monitor enumeration + `RgbaImage`) | The `screenshots` crate itself redirects here ("Move to XCap"); active development; `Monitor::all()` + `capture_image() -> image::RgbaImage` (RGBA8 straight); macOS via objc2-core-graphics `CGWindowListCreateImage`; multi-monitor friendly. |
| arboard | 3.6.1 | Clipboard image write | `Clipboard::set_image(ImageData { width, height, bytes })` with **RGBA8 straight-alpha** bytes (verified from source + doc example) — matches xcap's `RgbaImage` exactly; macOS NSImage / Windows CF_DIB. |
| ab_glyph | 0.2.32 | Text rasterization for annotations + size label | tiny-skia has no text module (verified); ab_glyph is the minimal, dependency-light rasterizer already in the dependency tree (transitive of egui 0.36); `OutlinedGlyph::draw` coverage callback composites into the Pixmap. |
| objc2-core-graphics | 0.3.2 | macOS Screen Recording permission FFI | `CGPreflightScreenCaptureAccess()` / `CGRequestScreenCaptureAccess()` verified present in generated bindings; already in the tree via xcap/arboard/tray-icon — no version conflict (objc2 0.6.4). |

### Supporting (already in workspace, reused unchanged)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| winit | 0.30.13 | Window + event loop + `PhysicalPosition` input | Overlay windows (Overlay profile), monitor enumeration, input routing. |
| tiny-skia | 0.12.0 | CPU rendering: image blit, mask, selection, annotations, toolbar | All overlay drawing via `Renderer::draw`. |
| softbuffer | 0.4.8 | Framebuffer present | Unchanged backend. |
| crossbeam-channel | 0.5.16 | `WindowRequest` queue, capture result handoff | Existing plumbing. |
| tiny-skia softbuffer renderer | in core | `TinySkiaSoftbufferRenderer` | Reused as-is. |
| log / anyhow / thiserror | pinned | Errors + logging | INFRA-03 discipline. |
| image (via `xcap::image`) | 0.25 | `RgbaImage` type only | Use xcap's re-export (`xcap::image::RgbaImage`); crop manually (axis-aligned sub-rect copy) to avoid new codec surface. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| xcap 0.9.8 | screenshots 0.8.10 | screenshots crate self-describes "Move to XCap" — maintenance is winding down; xcap has per-monitor geometry + scale_factor. |
| xcap 0.9.8 | scrap 0.5.0 | scrap is stale (0.5.0, last active ~2023-era), lower-level, macOS support weaker. |
| xcap 0.9.8 | cc-xcap 0.1.8 | fork of xcap — prefer upstream. |
| ab_glyph | fontdue | fontdue is fine but ab_glyph is already in the tree (no new transitive deps) and has a clean `OutlinedGlyph::draw` coverage API. |
| ab_glyph | cosmic-text / swash | heavier shaping stack; overkill for single-line MVP annotation text. |
| tiny-skia toolbar | egui (egui-winit + wgpu/glow) | egui-tiny-skia does NOT exist on crates.io (verified); egui wgpu/glow violates the CPU-rendering constraint; egui manual tessellation into Pixmap is a Phase 3 decision (SKELETON). |
| `WindowRequest::Redraw` redraw path | `on_created(WindowId, Arc<Window>)` callback | Both work; Redraw keeps module fully decoupled from winit windows and is consistent with the enqueue/drain architecture. |

**Installation:**
```bash
# in crates/modules/capture/Cargo.toml
cargo add xcap --no-default-features --features png   # NOTE: verify feature name at add time
cargo add arboard
cargo add ab_glyph
cargo add objc2-core-graphics --target x86_64-apple-darwin  # macOS only (aarch64 host: aarch64-apple-darwin)
```
Note: `xcap`'s macOS deps (objc2-app-kit 0.3.2, objc2-core-graphics 0.3.2, objc2-core-foundation 0.3.2) unify with the already-locked objc2 0.6.4 — no duplicate-objective-c crate. Verify with `cargo tree` after adding.

**Version verification (run before finalizing the plan):**
```bash
cargo search xcap        # 0.9.8 as of 2026-08-13
cargo search arboard     # 3.6.1
cargo search ab_glyph    # 0.2.32
cargo tree -p mybox-capture | grep objc2   # confirm single objc2 major
```

## Package Legitimacy Audit

> slopcheck could not be installed at research time (install blocked in this environment). Per the legitimacy protocol's graceful-degradation rule, **all newly recommended packages are tagged `[ASSUMED]`** and the planner must gate each install behind a `checkpoint:human-verify` task. Registry existence + source inspection below is strong supporting evidence, but does not confer `[VERIFIED]` without the slopcheck gate.

| Package | Registry | Source Repo | Evidence at Research Time | slopcheck | Disposition |
|---------|----------|-------------|---------------------------|-----------|-------------|
| xcap 0.9.8 | crates.io | github.com/nashaofu/xcap | `cargo info` OK; 0.9.8 cached locally; README claims macOS/Windows/Linux; used by `screenshots` crate's own redirect note | unavailable | Flagged — planner adds `checkpoint:human-verify` |
| arboard 3.6.1 | crates.io | github.com/1Password/arboard | `cargo info` OK; 3.6.1 cached locally; 1Password-maintained; MIT OR Apache-2.0 | unavailable | Flagged — planner adds `checkpoint:human-verify` |
| ab_glyph 0.2.32 | crates.io | github.com/alexheretic/ab-glyph | `cargo info` (via search) OK; 0.2.32 cached locally; already transitive in tree via egui 0.36 | unavailable | Flagged — planner adds `checkpoint:human-verify` |
| objc2-core-graphics 0.3.2 | crates.io | github.com/madsmtm/objc2 | already in Cargo.lock (verified); pulled by tray-icon/xcap/arboard | unavailable | Already in tree — no new gate |

**Packages removed due to slopcheck [SLOP] verdict:** none (slopcheck unavailable)
**Packages flagged as suspicious [SUS]:** none flagged; all four above are long-lived, widely-adopted crates. Still gated as `[ASSUMED]` per protocol.
**Cross-ecosystem verification:** All four are **Rust crates** — confirmed on crates.io via `cargo search`/`cargo info`, NOT the Python packages `egui` (0.0.7 on PyPI) / `image` (1.5.33 on PyPI) that the pip probe returned. The planner's `cargo add` commands must target crates.io only.

## Architecture Patterns

### System Architecture Diagram

```
Hotkey (Cmd+Shift+S, configurable)
  │  global-hotkey listener thread → EventLoopProxy → AppEvent::Hotkey
  ▼
App.on_hotkey → bus emit core/hotkey.triggered { action: "start_screenshot" }
  │
  ▼
CaptureModule handler (bus worker thread)
  │  1. CAP-08: CGPreflightScreenCaptureAccess() → false → request + guidance (macOS)
  │  2. spawn capture thread: xcap Monitor::all() → capture_image() per monitor
  │  3. UiThreadProxy::run(Box<FnOnce>) → main thread:
  │       store Vec<(monitor_geometry, RgbaImage)> in shared SessionState
  │       enqueue one WindowRequest::Create per monitor (physical geometry)
  ▼
App.about_to_wait drains WindowRequest → create_window per monitor (Overlay profile)
  │  registers WindowState; request_redraw()
  │
  ▼
winit event loop (main thread)
  ├─ WindowEvent::CursorMoved / MouseInput / KeyboardInput / ModifiersChanged
  │     → state.spec.on_event(module closure) → update SessionState → request redraw
  ├─ WindowEvent::RedrawRequested
  │     → state.renderer.draw(|pixmap,w,h| module_draw_closure(pixmap, SessionState))
  │        1. draw_pixmap: captured RgbaImage (premultiplied) fills window
  │        2. mask: 4 semi-transparent black fill_rects outside selection
  │        3. selection border stroke_rect + 8 handles + WxH label
  │        4. retained annotations (rect/arrow/pen/text) via tiny-skia + ab_glyph
  │        5. toolbar buttons (tiny-skia rects + text)
  │     → renderer.present() → softbuffer → screen
  └─ Enter / toolbar confirm
        → module: crop selection from captured RgbaImage + bake annotations (option)
        → arboard Clipboard::set_image (confined scope, main thread)
        → enqueue WindowRequest::Destroy per overlay window
        → bus emit capture/screenshot-taken
```

### Recommended Project Structure

```
crates/modules/capture/
├── Cargo.toml              # deps: mybox-core, xcap, arboard, ab_glyph, (+ objc2-core-graphics macOS)
└── src/
    ├── lib.rs              # CaptureModule (id "capture") + Module impl: default_config (hotkey), menu_items
    ├── session.rs          # CaptureSession state machine + shared Arc<Mutex<SessionState>>
    │                       #   phases: Idle/Selecting/Selected/Annotating/Confirm; selection rect; current tool
    ├── capture.rs          # background-thread screen capture (xcap), returns Vec<(MonitorGeom, RgbaImage)>
    ├── overlay.rs          # builds one WindowSpec per monitor; on_event handler; draw closure; redraw requests
    ├── selection.rs        # drag-select + 8-handle hit-test + resize logic (pure, unit-testable)
    ├── annotate.rs         # Annotation enum + drawing (rect/arrow/pen/text) + undo stack
    ├── toolbar.rs          # toolbar button layout + hit-testing (tiny-skia drawn, no egui)
    ├── text.rs             # ab_glyph text rendering helper (size label + text tool)
    ├── clipboard.rs        # crop + annotation bake + arboard set_image (RGBA8 straight)
    └── permission.rs       # #[cfg(macos)] CGPreflight/CGRequest + guidance (injectable for tests)
```

### Pattern 1: Renderer::draw content hook (core change — closes the Phase 2 gap)

**What:** The App's `RedrawRequested` handler currently calls only `renderer.present()` (verified in `app.rs`). Phase 2 wires the `draw` slot: add an `on_draw` closure to `WindowSpec` and call `renderer.draw` before `present`.
**When to use:** Required — this is the documented Phase 2 gap ("App 的 RedrawRequested handler 只调 present()，未调 draw()").
**Example (shape for the planner to finalize; source: app.rs + renderer/mod.rs):**
```rust
// WindowSpec gains (mirrors existing `on_event`):
pub on_draw: Option<Box<dyn Fn(&mut tiny_skia::PixmapMut, u32, u32) + Send + Sync>>,

// App::window_event RedrawRequested arm becomes:
WindowEvent::RedrawRequested => {
    if let Some(draw) = &state.spec.on_draw {
        state.renderer.draw(&mut |pixmap, w, h| draw(pixmap, w, h));
    }
    if let Err(e) = state.renderer.present() {
        log::warn!("renderer present failed: {e}");
    }
}
```
Key design note: the module's draw closure must be able to re-render from current state on every redraw (immediate-mode) — it reads `Arc<Mutex<SessionState>>` and paints the full frame. Do not accumulate pixels; annotations are a retained list redrawn every frame.

### Pattern 2: redraw request path (core change)

**What:** The module needs to repaint when the user drags. With no draw call in the loop and `ControlFlow::Wait` (idle-zero-cost), the module must actively request a redraw. The module never holds the winit `Window` (W2), so route through the existing request channel.
**Recommended (keeps module decoupled, any-thread safe):**
```rust
pub enum WindowRequest { Create(WindowSpec), Destroy(WindowId), Redraw(WindowId) }
// App::about_to_wait drain:
WindowRequest::Redraw(id) => {
    if let Some(state) = self.windows.get_mut(id) {
        if let Some(w) = &state.window { w.request_redraw(); }
    }
}
```
Alternative: add `WindowSpec.on_created: Option<Box<dyn Fn(WindowId, Arc<Window>) + Send + Sync>>` and let the module hold `Arc<Window>` to call `request_redraw()` directly. Either satisfies the requirement; the `Redraw` variant reuses the existing enqueue→drain architecture and works from the bus thread too. Flag as a planner decision.

### Pattern 3: per-monitor overlay creation (batch_create, D-09)

**What:** One overlay window per monitor. The real implementation is NOT a new core batch API — window creation must stay in `create_window` on the main thread (W2: `ActiveEventLoop` is not `Send`). The module loops over its own monitor list (from xcap, which needs no main thread) and enqueues one `WindowRequest::Create` per monitor with **physical-pixel** geometry. `WindowManager::batch_create`'s current placeholder signature (`&self`) cannot create windows — the planner should either remove the placeholder or re-purpose the module-side loop as the "batch".
**When to use:** Always for capture flow (Pitfall 3 — one fullscreen overlay on the primary display only is insufficient).
**Example (geometry math; xcap points → winit physical pixels):**
```rust
// per monitor from xcap (verified: Monitor::x/y/width/height in points on macOS, scale_factor() available)
let geom = MonitorGeom {
    x:       (m.x()? as f64 * m.scale_factor()? as f64).round() as i32,
    y:       (m.y()? as f64 * m.scale_factor()? as f64).round() as i32,
    width:   img.width(),   // capture_image() dims == physical backing resolution
    height:  img.height(),
};
// WindowSpec { kind: Overlay, position: Some((geom.x, geom.y)), inner_size: Some((geom.width, geom.height)), on_event: ..., on_draw: ... }
```

### Pattern 4: capture on background thread, results via UiThreadProxy (Pitfall 4)

**What:** xcap capture (esp. multi-monitor) is slow and must not block the main loop. The module receives the hotkey on the bus worker thread, spawns a capture thread, and hands the images to the main thread through the existing `UiThreadProxy::run(Box<dyn FnOnce() + Send>)` — which becomes `AppEvent::Ui(f)` executed in `user_event` (verified in context.rs/app.rs).
**When to use:** Always for capture (FRMW-05).
**Example:**
```rust
let ui = ctx.ui().clone();
std::thread::spawn(move || {
    let result: anyhow::Result<Vec<(MonitorGeom, RgbaImage)>> = capture_all_monitors();
    ui.run(Box::new(move || match result {
        Ok(shots) => { *session.state.lock() = SessionState::from_shots(shots); /* enqueue creates */ }
        Err(e) => log::error!("capture failed: {e:#}"),
    }));
});
```

### Anti-Patterns to Avoid
- **Baking annotations into pixels as they are drawn:** makes undo impossible and forces re-capture. Keep a retained `Vec<Annotation>` and redraw every frame (undo = pop + redraw).
- **Capturing the screen inside the event loop or inside the draw closure:** blocks the loop (Pitfall 4) or, worse, captures the overlay window itself. Always capture before windows exist, on a worker thread.
- **Putting egui in the overlay window:** egui-tiny-skia does not exist; mixing backends causes the Pitfall 5 flicker/overdraw class. Toolbar = tiny-skia + hit-testing.
- **Creating overlay windows from the bus thread / module init:** winit windows are main-thread-bound (W2). Always enqueue `WindowRequest` and let `about_to_wait` create.
- **Leaving a long-lived `arboard::Clipboard`:** arboard docs require dropping it before app exit (winit owns the loop); Windows clipboard is thread-affine and parallel ops risk `ClipboardOccupied`. Create + set + drop in a confined scope at confirm time.
- **Silently shipping black screenshots on macOS:** without Screen Recording permission xcap returns an empty/black image with no error. Always preflight (CAP-08) before capture and surface a clear message.
- **Mixing logical/physical coordinates:** `CursorMoved.position` is already physical; `WindowSpec` position/size are physical; xcap monitor x/y are points — the ONLY conversion point is `xcap points × scale_factor` for window placement. Everything else stays physical (Pitfall 6).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Screen capture + monitor enumeration | Custom CG/Win32 capture | xcap 0.9.8 | Cross-platform (macOS/Windows/Linux), handles permission plumbing, multi-monitor geometry, BGRA→RGBA, row padding. |
| Clipboard image write | Custom NSPasteboard/CF_DIB code | arboard 3.6.1 | Handles NSImage (macOS), CF_DIB (Windows), PNG (Linux), history-exclusion; verified RGBA8-straight input matches xcap output. |
| Text rendering | Manual glyph parsing | ab_glyph 0.2.32 | TrueType parsing + hinting + coverage rasterization; already in tree; tiny-skia has no text at all. |
| Hotkey string parsing | Custom "Cmd+Shift+S" parser | global-hotkey `HotKey: FromStr` (Phase 1, D-11) | Already built and verified in Phase 1. |
| Window creation/destruction | Direct `ActiveEventLoop::create_window` from module | `WindowManagerHandle::create/destroy` + App drain (W2/W3) | Main-thread binding; wake hook; state registration all handled. |
| Pixel format conversion | Custom BGRA↔RGBA↔premul for softbuffer | Existing `premul_rgba_to_u32` + xcap already returns RGBA | One premultiply helper for `RgbaImage`→Pixmap is all that is needed (new, small, unit-tested). |

**Key insight:** nearly all hard cross-platform surface (capture, clipboard, hotkeys, windows) is already solved by the Phase 1 stack; the genuinely new code is the *interaction state machine* (selection, handles, annotation) and the *compositing order* — both pure logic over tiny-skia, both unit-testable headlessly. Resist the urge to hand-roll platform I/O.

## Common Pitfalls

### Pitfall 1: Overlay window captures itself (black screenshot of the overlay)
**What goes wrong:** First screenshot contains the overlay's own dark mask or is black.
**Why it happens:** xcap/`CGWindowListCreateImage` captures *after* the overlay windows are created, including them in `OptionAll`.
**How to avoid:** Strict ordering — capture ALL monitors first, then create overlay windows (verified: `capture_image` uses `CGWindowListOption::OptionAll`).
**Warning signs:** first shot is dark/black or shows a window outline.

### Pitfall 2: `RgbaImage` (straight alpha) vs tiny-skia Pixmap (premultiplied)
**What goes wrong:** colors look washed-out or darkened when blitting the capture into the overlay.
**Why it happens:** xcap returns straight RGBA8; tiny-skia Pixmap expects premultiplied RGBA8; copying raw bytes misinterprets every semi-transparent pixel.
**How to avoid:** a small `premultiply_rgba8(&[u8]) -> Vec<u8>` helper feeding `tiny_skia::Pixmap::from_vec(...)`; unit-test with known colors (reuse the Phase 1 `premul_rgba_to_u32` test style).
**Warning signs:** white/light pixels tinted on the overlay only.

### Pitfall 3: Redraw never fires (stale overlay)
**What goes wrong:** drag selection doesn't repaint; overlay frozen after first frame.
**Why it happens:** Phase 1 draws nothing and `ControlFlow::Wait` never loops; without the `draw` call-chain fix and an explicit redraw request, nothing re-paints.
**How to avoid:** wire `on_draw` in `RedrawRequested`; request a redraw on every input event that changes state; keep `ControlFlow::Wait` (no continuous 60fps poll).
**Warning signs:** first frame appears, subsequent drags show nothing.

### Pitfall 4: macOS Screen Recording permission silently missing
**What goes wrong:** user triggers screenshot, gets black image or blank overlay, no error.
**Why it happens:** xcap returns empty/black data without permission; no automatic prompt on modern macOS.
**How to avoid:** CAP-08 preflight on hotkey; if denied, `CGRequestScreenCaptureAccess()` once, then show guidance (text + "open System Settings" deep link `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`); after grant, app may need restart on some macOS versions — state that in the guidance. Never treat an all-black capture as success (validate that the captured buffer isn't uniformly empty).
**Warning signs:** uniform black RgbaImage from `capture_image()`.

### Pitfall 5: Multi-monitor overlay only on primary / wrong position
**What goes wrong:** overlay only covers the primary display, or is offset on a second display.
**Why it happens:** a single fullscreen overlay covers one monitor (Pitfall 3); using xcap's point geometry directly as physical pixels misplaces Retina windows.
**How to avoid:** per-monitor windows (D-09) sized/positioned in physical pixels (`xcap points × scale_factor`); verify at runtime that `window.inner_size() == capture_image dims`.
**Warning signs:** overlay smaller than screen, offset, or absent on the secondary display.

### Pitfall 6: Clipboard operation fails with `ClipboardOccupied` / deadlock on Windows
**What goes wrong:** confirm produces no image or an error, or the app hangs on quit.
**Why it happens:** Windows clipboard opens one thread at a time; parallel/long-lived `Clipboard` instances collide; long-lived instances must be dropped before process exit.
**How to avoid:** create `Clipboard`, `set_image`, drop — all in one confined main-thread scope at confirm time; never hold it across the winit loop. (Arboard source docs, verified.)
**Warning signs:** `ClipboardOccupied` error at confirm.

### Pitfall 7: ESC / Enter swallowed because overlay window lacks keyboard focus
**What goes wrong:** ESC/Enter do nothing during selection.
**Why it happens:** on macOS an Accessory app's window can lose focus; Overlay is always-on-top but focus isn't guaranteed (Pitfall 7 in PITFALLS.md).
**How to avoid:** request focus at overlay creation (macOS `NSApp.activate(ignoringOtherApps: true)` via objc2-app-kit — already partly handled by Accessory policy in Phase 1; verify keyboard focus on the overlay in the manual checklist).
**Warning signs:** mouse selection works but keys are dead.

## Code Examples

Verified patterns from official/source-confirmed references (paths point at the cached crate sources read for this research):

### Capture all monitors with xcap
```rust
// Source: xcap-0.9.8/src/monitor.rs + macos/impl_monitor.rs (verified API)
use xcap::Monitor;                       // xcap::image is re-exported (xcap/src/lib.rs: pub use image;)
for monitor in Monitor::all()? {         // CGGetActiveDisplayList on macOS
    let x = monitor.x()?;                // i32, GLOBAL display points
    let y = monitor.y()?;                // i32
    let scale = monitor.scale_factor()?; // f32 (pixel_width / point_width)
    let img: image::RgbaImage = monitor.capture_image()?; // pixel-resolution, RGBA8 straight
    // physical window geometry = (x*scale, y*scale, img.width(), img.height())
}
```

### Copy region to clipboard with arboard
```rust
// Source: arboard-3.6.1/src/lib.rs + common.rs (verified: ImageData is RGBA8 straight-alpha)
use arboard::{Clipboard, ImageData};
use std::borrow::Cow;

// crop (axis-aligned) from the captured RgbaImage → RGBA8 straight bytes
let crop: Vec<u8> = /* manual sub-rect copy of img.as_raw() */;
{
    let mut cb = Clipboard::new().map_err(anyhow::Error::msg)?; // drop before loop/exit
    cb.set_image(ImageData { width: w as usize, height: h as usize, bytes: Cow::Owned(crop) })?;
    // dropped here — confined scope (arboard doc: must drop before program exit)
}
```

### Draw a retained annotation list with tiny-skia
```rust
// Source: tiny-skia-0.12.0/src/lib.rs exports + painter.rs (verified API)
use tiny_skia::{Color, LineCap, Paint, PathBuilder, PixmapMut, Point, Stroke, Transform};
use tiny_skia::path::Path;

enum Annotation {
    Rect { a: Point, b: Point },
    Arrow { a: Point, b: Point },
    Pen { pts: Vec<Point> },
    Text { at: Point, s: String, size: f32 },
}
let mut paint = Paint::default();
paint.set_color_rgba8(0xFF, 0x60, 0x00, 0xFF);       // annotation orange
let stroke = Stroke { width: 3.0, line_cap: LineCap::Round, ..Stroke::default() };
match ann {
    Annotation::Rect { a, b } => {
        let r = tiny_skia::Rect::from_ltrb(a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)).unwrap();
        pm.stroke_rect(r, &paint, &stroke, Transform::identity(), None);
    }
    Annotation::Arrow { a, b } => {
        let mut pb = PathBuilder::new();
        pb.move_to(a.x, a.y).line_to(b.x, b.y);
        // + filled triangle head: move_to/line_to close, fill_path
        pm.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);
    }
    Annotation::Pen { pts } => {
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].x, pts[0].y);
        for p in &pts[1..] { pb.line_to(p.x, p.y); }
        pm.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);
    }
    // Text: ab_glyph OutlinedGlyph::draw coverage → composite into pm
}
```

### Rasterize text into the Pixmap (ab_glyph)
```rust
// Source: ab_glyph-0.2.32/src/font.rs (glyph_id/outline_glyph) + outlined.rs (draw) — verified
use ab_glyph::{Font, FontArc, PxScale, point};
let font = FontArc::try_from_slice(include_bytes!("assets/DejaVuSans.ttf"))
    .or_else(|_| FontArc::try_from_slice(&std::fs::read("/System/Library/Fonts/Supplemental/Arial.ttf")?))
    ?; // macOS verified present; Windows path added Phase 4
let scale = PxScale::from(24.0);
for ch in "1234 × 567".chars() {
    let g = font.scaled_glyph(glyph_id_for(ch));       // font.glyph_id(ch) → into scaled glyph
    if let Some(og) = font.outline_glyph(g) {
        let bounds = og.px_bounds();
        og.draw(|gx, gy, cov| {
            let px = (bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32);
            // blend cov into pixmap at px (coverage → alpha)
        });
    }
}
```

### macOS permission preflight (objc2-core-graphics)
```rust
// Source: objc2-core-graphics-0.3.2/src/generated/CGWindow.rs (verified present)
#[cfg(target_os = "macos")]
fn has_screen_recording_access() -> bool {
    unsafe { objc2_core_graphics::CGPreflightScreenCaptureAccess() }
}
#[cfg(target_os = "macos")]
fn request_screen_recording_access() -> bool {
    unsafe { objc2_core_graphics::CGRequestScreenCaptureAccess() }
}
// Guidance deep link (macOS): 
//   open x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `screenshots` crate as primary capture lib (STACK.md 0.5+) | `xcap 0.9.8` (screenshots README now says "Move to XCap") | 2024-2025 | One active capture library; per-monitor geometry + scale_factor built in. |
| `screenshots`/`xcap` listed at 0.5/0.4 in STACK.md | current 0.8.10 / 0.9.8 | as of 2026-08-13 | Version table in STACK.md is stale — planner must use the versions above. |
| egui integrated into every window (D-01 Phase 1 draft) | egui deferred to Phase 3; overlays pure tiny-skia | Phase 1 (SKELETON: "egui-tiny-skia 不存在") | Overlay stays dependency-light; no tessellation work this phase. |
| objc2 permission calls via raw `msg_send!` | generated safe-ish FFI `CGPreflightScreenCaptureAccess()` | objc2 0.6.x era | Cleaner, compile-checked permission calls. |

**Deprecated/outdated:**
- `scrap` (0.5.0): stale, low-level; not recommended (was listed as an alternative in STACK.md).
- Direct `CGDisplayStream`/`core-graphics` capture: xcap wraps it; don't hand-roll.
- `egui-tiny-skia` / `egui_tiny_skia`: does not exist on crates.io (verified 2026-08-13); Phase 1's D-01 reference to it was aspirational.

## Assumptions Log

> All claims tagged `[ASSUMED]` below need user confirmation before becoming locked decisions.

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `CGRequestScreenCaptureAccess()` reliably triggers the system prompt on current macOS (10.15+ behavior varies by version; on some versions it only opens Settings or requires a restart to take effect) | Common Pitfalls / permission | Guidance may need manual deep-link fallback; must not depend on the prompt alone. Validate at runtime on the dev Mac. |
| A2 | The overlay window at monitor physical geometry exactly matches the xcap capture image (window.inner_size() == capture dims); mismatches (edge scaling cases) are rare | Per-monitor pattern | If sizes differ, blit must scale the capture → extra code + visual blur. Verify on the dev Mac (Retina) in plan 02-01. |
| A3 | Overlay covers the macOS menu bar / fullscreen apps only after raising to `screenSaver` window level via objc2-app-kit `NSWindow.setLevel` (winit `AlwaysOnTop` ≠ screenSaver — SKELETON deferred this to Phase 2) | SKELETON/Common Pitfalls | If skipped, overlay may not appear over fullscreen apps; MVP may accept AlwaysOnTop-only and note the limitation. |
| A4 | System font at `/System/Library/Fonts/Supplemental/Arial.ttf` is an acceptable text-annotation font for MVP (macOS-first); Windows path deferred to Phase 4 | Code examples | On macOS the path is verified present; if licensing of a bundled font is preferred, embed a OFL font via `include_bytes!` instead. |
| A5 | xcap 0.9.8 `--no-default-features --features png` (or plain default) is the right feature set; default features include `image` with `png` — verify at `cargo add` time | Installation | Wrong feature set could pull unnecessary codecs; low impact. |
| A6 | Annotation text uses straight (non-outlined) glyph coverage; text hit-testing/editing is out of MVP scope (place once at click) | Text tool | Users cannot edit text after placement — acceptable for MVP, matches Snipaste-basic. |
| A7 | `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture` deep link opens the correct pane on current macOS | permission | If the URL scheme changes, guidance button breaks — fallback: instruct user to navigate manually. |

## Open Questions

> All questions RESOLVED at planning time (2026-08-13). Each carries the resolution that landed in the phase plans; no open question remains unresolved for the executor.

1. **Does the capture overlay need `screenSaver` window level for MVP? (A3) — (RESOLVED)**
   - Resolution: **Descoped to AlwaysOnTop-only for MVP.** The overlay uses winit `AlwaysOnTop` (Overlay profile) as-is; the `screenSaver` window level via objc2-app-kit `NSWindow.setLevel(NSWindowLevel::ScreenSaver)` is deferred. Success criteria (overlay over normal apps) pass with AlwaysOnTop on the dev machine.
   - **Known limitation (documented in plan 02-02 objective + 02-04 manual checklist):** the overlay may not appear above macOS fullscreen apps or the menu bar on some configurations. This is an accepted MVP limitation, not a regression; re-evaluate in Phase 4 if user feedback requires it.
   - Tracked by Assumption A3; no dedicated task in plan 02-01 (was originally "small explicit task" — descoped per the Recommendation's own fallback).

2. **Redraw trigger: `WindowRequest::Redraw` vs `WindowSpec.on_created(WindowId, Arc<Window>)`? — (RESOLVED)**
   - Resolution: `WindowRequest::Redraw(WindowId)` chosen (RESEARCH Pattern 2). Reuses the enqueue→drain architecture, is thread-safe from the bus thread, and keeps the module decoupled from the winit `Window`. `on_created` not needed this phase; revisit for Phase 3 if it needs focus/layout tracking.
   - Landed: plan 02-01 Task 2 (core), consumed by 02-02/02-03/02-04.

3. **Should `WindowManager::batch_create` placeholder be removed or reimplemented? — (RESOLVED)**
   - Resolution: **Removed.** The module-side per-monitor loop over xcap monitors (one `WindowRequest::Create` per monitor) is the real "batch" (RESEARCH Pattern 3); the dead `batch_create(&self)` placeholder and its `renderer_factory` field are deleted (also fixes WR-09 id-collision risk).
   - Landed: plan 02-01 Task 2.

4. **Should overlay mask allow the user to see through to live screen behind the overlay? — (RESOLVED)**
   - Resolution: No — **static snapshot accepted and documented** (D-01: 覆盖窗口出现后显示屏幕画面). Snipaste/Shottr also show a static dimmed snapshot; live-refresh would need re-capture loops (out of scope).
   - Landed: plan 02-02 Task 1 (capture-then-create ordering per Pitfall 1).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | whole phase | ✓ | rustc 1.97.1 / cargo 1.97.1 (host aarch64-apple-darwin) | — (≥1.75 required, far exceeded) |
| cargo-nextest | test runs | ✓ | 0.9.143 | `cargo test` |
| Xcode / objc2 toolchain | objc2-core-graphics + macOS capture FFI | ✓ | Xcode.app present | — |
| macOS system font Arial.ttf | text annotation (A4) | ✓ | /System/Library/Fonts/Supplemental/Arial.ttf | Helvetica.ttc (also present) |
| xcap / arboard / ab_glyph crates | capture / clipboard / text | ✓ (sources pre-fetched in cargo cache) | 0.9.8 / 3.6.1 / 0.2.32 | offline `cargo add` may still hit network |
| winit 0.30.13, tiny-skia 0.12.0, softbuffer 0.4.8 | framework | ✓ | pinned in workspace | — |
| Real macOS desktop session (display) | integration/manual verification of overlay + input + permission | ✓ (dev machine) | macOS 26-era | `#[ignore]` display tests only run on demand |
| Screen Recording permission on dev machine | CAP-08 end-to-end | needs one-time grant | — | manual checklist item |

**Missing dependencies with no fallback:** none — the stack is fully available on the dev Mac.
**Missing dependencies with fallback:** Screen Recording permission is a one-time OS grant (user action), not a tool; guidance flow (CAP-08) covers it.

## Validation Architecture

> Nyquist validation enabled (`workflow.nyquist_validation: true` in `.planning/config.json`).

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo-nextest 0.9.143 (unit/integration) + `cargo test -- --ignored` (display/OS tests, subprocess-per-check pattern from Phase 1) |
| Config file | workspace `Cargo.toml` + per-crate `[dev-dependencies]` |
| Quick run command | `cargo nextest run` |
| Full suite command | `cargo nextest run && cargo test -- --ignored` |
| Estimated runtime | ~10s quick; ~60s full (includes display checks) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CAP-01 | capture spawns on worker thread, returns RgbaImage per monitor | unit (mockable capture fn) + integration (real capture, #[ignore]) | `cargo nextest run -p mybox-capture capture::tests` | ❌ Wave 0 |
| CAP-02 | draw closure composites image + mask (mask rects correct; selected region un-dimmed) | unit (headless Pixmap pixel asserts) | `cargo nextest run -p mybox-capture session::tests` | ❌ Wave 0 |
| CAP-03 | selection state machine: drag updates selection; border+WxH present; synthetic CursorMoved → state change | unit | `cargo nextest run -p mybox-capture selection::tests` | ❌ Wave 0 |
| CAP-04 | crop→ImageData conversion (RGBA8 straight, dims/bytes exact); arboard set_image on real session | unit + #[ignore] display | `cargo nextest run -p mybox-capture clipboard::tests` + `cargo test -- --ignored -p mybox-capture` | ❌ Wave 0 |
| CAP-05 | ESC → cancel state → Destroy requests enqueued for all overlay windows | unit | `cargo nextest run -p mybox-capture session::tests::cancel` | ❌ Wave 0 |
| CAP-06 | rect/arrow/pen path construction + pixel output; text coverage composite | unit (headless tiny-skia) | `cargo nextest run -p mybox-capture annotate::tests` | ❌ Wave 0 |
| CAP-07 | undo pops last annotation; Ctrl+Z (ModifiersChanged + 'z') triggers undo; undo to empty == original image | unit | `cargo nextest run -p mybox-capture annotate::tests::undo` | ❌ Wave 0 |
| CAP-08 | permission gate called before capture; denied path shows guidance (injectable checker) | unit (injectable) + manual checklist | `cargo nextest run -p mybox-capture permission::tests` | ❌ Wave 0 |
| Core draw-chain fix (not a CAP, prerequisite) | RedrawRequested calls draw then present (MockRenderer records calls) | unit | `cargo nextest run -p mybox-core app::tests::redraw_draws_then_presents` | ❌ Wave 0 |
| Overlay display + real input end-to-end | overlay shows capture, drag selects, Enter copies, ESC closes | integration (#[ignore], subprocess-per-check) + manual | `cargo test -- --ignored` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo nextest run -p mybox-core -p mybox-capture`
- **Per wave merge:** `cargo nextest run && cargo test -- --ignored`
- **Phase gate:** Full suite green before `/gsd:verify-work`; manual checklist for success criteria 1-6 (visible desktop session).

### Wave 0 Gaps
- [ ] `crates/modules/capture/` — new module crate (lib.rs, session/selection/annotate/clipboard/permission modules)
- [ ] `crates/modules/capture/tests/` — unit test dirs per module; `#[ignore]` display integration (subprocess-per-check harness like `mybox-core/src/bin/display_checks.rs`)
- [ ] `crates/mybox-core/src/` — `WindowSpec.on_draw` field + `WindowRequest::Redraw` variant (or `on_created`) + App wiring; extend existing `window.rs`/`app.rs` unit tests
- [ ] Framework-installed: cargo-nextest already installed (0.9.143) — no action
- [ ] `cargo add xcap arboard ab_glyph` (+ macOS `objc2-core-graphics`) — behind `checkpoint:human-verify` per Package Legitimacy Audit

## Security Domain

> `security_enforcement` is absent from config.json (absent = enabled). Phase 2 has no network/auth, so the surface is small but the *data* is sensitive (screen content).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — (no user auth) |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | partial | Annotation text is user-controlled but rendered internally only (no injection surface); validate string length for size label; never execute from event payloads (Phase 1 T-1-02 discipline — `on_hotkey`/`on_menu` only forward ids/actions). |
| V6 Cryptography | no | — (no keys; clipboard pasteboard not encrypted) |

### Known Threat Patterns for the capture stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Screen content captured and left in memory longer than needed | Information Disclosure | Drop captured `RgbaImage`s when the overlay closes; keep per-session, not per-app; note sensitive-content handling in clipboard (content is inherently the user's request). |
| Black-image silent failure (permission denied) treated as success | Spoofing/Integrity | Preflight (CAP-08) + validate captured buffer is not uniformly empty; surface clear error instead of a black screenshot. |
| Clipboards containing screenshots linger in system history | Information Disclosure | Optional `arboard` `exclude_from_history` (macOS `org.nspasteboard.ConcealedType` — verified available in arboard 3.6.1) — recommend enabling for screenshot copies (Claude's discretion). |
| Clipboard operation on wrong thread (Windows thread-affinity) | Availability | All clipboard ops in the main-thread confirm path in a confined scope (documented in Pitfall 6). |
| Panicking event handler killing the loop | Availability | Existing bus `catch_unwind` (Phase 1) + module draw/on_event handlers should not panic on malformed state (guard empty pen point lists, zero-size crops). |

## Sources

### Primary (HIGH confidence — source-verified this session)
- [crates.io registry cache] xcap-0.9.8 `src/monitor.rs`, `src/lib.rs`, `src/macos/capture.rs`, `src/macos/impl_monitor.rs` — `Monitor::all/x/y/width/height/scale_factor/capture_image`, `RgbaImage` (RGBA8 straight), `CGWindowListCreateImage` (permission required, `OptionAll` self-capture risk), BGRA→RGBA + row-padding handling
- [crates.io registry cache] arboard-3.6.1 `src/lib.rs`, `src/common.rs`, `src/platform/osx.rs` — `Clipboard::set_image(ImageData { width, height, bytes })`, RGBA8 straight, macOS NSImage via `CGImageCreate` (alpha Last / straight), `exclude_from_history` (ConcealedType), drop-before-exit + Windows thread-affinity notes
- [crates.io registry cache] objc2-core-graphics-0.3.2 `src/generated/CGWindow.rs` — `CGPreflightScreenCaptureAccess()` / `CGRequestScreenCaptureAccess()` verified present
- [crates.io registry cache] tiny-skia-0.12.0 `src/lib.rs`, `src/painter.rs` — `PathBuilder`, `fill_path`/`stroke_path`/`stroke_rect`, `Color::from_rgba8`, `Pixmap::from_vec`, premultiplied Pixmap contract; NO text module (verified absence)
- [crates.io registry cache] ab_glyph-0.2.32 `src/font.rs`, `src/outlined.rs`, `src/font_arc.rs` — `FontArc::try_from_slice`, `glyph_id`, `outline_glyph`, `OutlinedGlyph::draw(coverage callback)`
- [crates.io registry cache] winit-0.30.13 `src/event.rs`, `src/event_loop.rs`, `src/monitor.rs` — `CursorMoved{ position: PhysicalPosition<f64> }`, `KeyboardInput`, `ModifiersChanged`, `available_monitors()`/`MonitorHandle::position/size/scale_factor`
- [repo] crates/mybox-core `window.rs`, `app.rs`, `renderer/mod.rs`, `renderer/tiny_skia_softbuffer.rs`, `context.rs`, `event.rs`, `module.rs`; crates/modules/test/src/lib.rs; crates/mybox-core/src/bin/display_checks.rs — exact Phase 1 API surface the module builds on
- [repo] .planning/phases/01-framework/01-SKELETON.md, 01-04-SUMMARY.md — "egui-tiny-skia 不存在", transparent-alpha limitation, screenSaver-level deferral, batch_create deferral, draw-call-chain gap

### Secondary (MEDIUM confidence)
- crates.io metadata via `cargo search` / `cargo info` (xcap 0.9.8, arboard 3.6.1, ab_glyph 0.2.32, screenshots 0.8.10 "Move to XCap", scrap 0.5.0) — registry-existence + version currency
- [CITED: docs.rs/arboard] arboard API documentation referenced via cached source (identical to published docs)
- [CITED: Apple documentation (training)] macOS Screen Recording permission semantics for `CGPreflight/CGRequestScreenCaptureAccess`, deep-link scheme — behavior nuances tagged `[ASSUMED]` (A1/A7)

### Tertiary (LOW confidence — flagged)
- Apple `CGRequestScreenCaptureAccess` prompt behavior across macOS versions (A1) — needs runtime validation on the dev machine during plan 02-03
- Window-level equivalence `xcap points × scale_factor == winit physical` on all display arrangements (A2) — verify at runtime in plan 02-01

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all four new crates verified by reading the cached crate source + registry metadata this session; version numbers current (2026-08-13)
- Architecture: HIGH — draw-chain gap, threading, per-monitor overlay, retained-annotation/undo all grounded in the actual Phase 1 source and existing pitfalls research
- Pitfalls: HIGH for the source-confirmed ones (black capture, premul mismatch, clipboard threading, redraw); MEDIUM for macOS permission-prompt nuances (A1) which need runtime validation

**Research date:** 2026-08-13
**Valid until:** 2026-09-12 (30 days — crate versions are fast-moving; re-verify `xcap`/`arboard`/`ab_glyph` before install)
