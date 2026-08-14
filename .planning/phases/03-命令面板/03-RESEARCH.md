# Phase 3: 命令面板 - Research

**Researched:** 2026-08-14
**Domain:** egui 0.30 on a tiny-skia CPU render pipeline, global-hotkey summon UX, fuzzy command filtering, async command execution
**Confidence:** HIGH

## Summary

Phase 3 turns mybox into a hotkey-summonable command palette: `Cmd+Shift+Space` summons a borderless, always-on-top, centered Floating window rendered with egui 0.30; a fuzzy filter (fuzzy-matcher) narrows the command list as the user types; ↑/↓/Enter/ESC drive a six-state interaction machine; commands run as async futures on worker threads while the panel shows status. The palette consumes commands registered by modules (capture module's `capture.start`) plus four framework builtins (quit / open-config / restart / open-log).

**The single most important verified finding:** D-03's "核心框架代码零改动" is **not achievable as literally stated**. `egui_winit::State::on_window_event` and `take_egui_input` both require `&winit::window::Window`, but the palette module's `WindowSpec.on_event` closure only receives `&WindowEvent` (verified in egui-winit 0.30.0 source, lines 232/263). The window access gap, plus focus-on-Floating and app-exit plumbing, require **six small, additive core changes** (detailed in Architecture Patterns §Pattern 2). All are additive extension points of exactly the kind Phase 2 already added (`on_draw`, `WindowRequest::Redraw`, `ModuleContext::bus()`); none modify existing behavior.

**The second critical finding:** there is **no maintained crate** that renders egui into a tiny-skia Pixmap. `egui_tiny_skia` does not exist on crates.io (re-verified 2026-08-14), `egui_skia` 0.4.0 is an abandoned SkiaRust-era crate, and `egui_software_backend` 0.0.3 pins egui 0.34 (D-02 locks 0.30). The palette therefore ships a small (~250-line) tessellate→tiny-skia rasterizer as part of the module. This is bounded, well-understood code: solid triangles via `fill_path`, textured font glyphs via barycentric per-pixel sampling of the font atlas.

**Third:** egui 0.30.x exists as exactly **0.30.0** (no patch releases — verified via the sparse index), and egui-winit 0.30.0 declares `winit = "0.30.5"` which resolves to our pinned 0.30.13. epaint 0.30.0 needs `ab_glyph ≥0.2.11` (workspace has 0.2.32) and egui-winit needs `raw-window-handle 0.6` (already the workspace version). No version conflicts; D-02's lock is satisfiable as-is.

**Primary recommendation:** New crate `crates/modules/palette` (id `"palette"`), new core module `command.rs` (Command/CommandRegistry/BuiltinCommands + pollster-based runner), additive core extension `WindowSpec.on_event_win` for window access, egui run+rasterize into a palette-owned framebuffer Pixmap inside `on_event_win` with a 1-line blit in `on_draw`, fuzzy-matcher SkimMatcherV2 for filter+highlight, and `pollster::block_on` on a per-invocation worker thread for the async runner (no full async runtime this phase — tokio is the v2 upgrade path).

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### egui 归属与集成方式
- **D-01:** egui/egui-winit 依赖引入 mybox-core（可 re-export 供所有模块复用）。未来模块（如 v2 AI 对话助手）依赖 core 的 egui re-export，不各自引入。
- **D-02:** egui 版本锁定 0.30（researcher 需验证与 winit 0.30 / softbuffer 的兼容组合及 CPU 软件渲染方案）。
- **D-03:** palette 模块自持有 egui 集成：通过 `WindowSpec.on_event` 把 winit 事件转发给 egui-winit，通过 `on_draw` 软件渲染到 Pixmap。核心框架代码零改动，符合"加模块不改核心"约束。

#### 执行期间面板表现
- **D-04:** 命令执行期间：输入框下方显示状态行（如「正在执行：开始截图…」），列表和输入禁用（防重入），runner 完成后面板关闭。截图命令例外：SPEC 要求触发截图前先隐藏/关闭面板（避免面板被拍进截图）。
- **D-05:** 执行失败的错误提示在面板内：列表区显示错误消息，用户按任意键或 ESC 关闭面板。不使用系统通知 API。
- **D-06:** 面板窗口生命周期采用建销模式：每次唤出创建 Floating 窗口，关闭/执行完成后销毁。与 Phase 2 截图 overlay 模式一致，无残留状态。
- **D-07:** `Command.runner` 为**异步**签名（返回 Future），框架接入 async 运行时。用户明确选择异步（覆盖同步推荐），理由：为 v2 AI 对话类慢命令预留。运行时选型（tokio/smol 等）与执行线程模型由 researcher/planner 决定。

#### 面板视觉形态
- **D-08:** 视觉风格参考 Raycast：大圆角卡片、大号列表项、宽松间距。
- **D-09:** 深色固定主题（背景深灰、文字浅色），与截图遮罩暗色风格一致，不跟随系统。
- **D-10:** 列表项显示：命令名称 + 灰色描述，命中关键词的字符高亮。
- **D-11:** 面板约 600px 宽固定，高度按列表条数自适应（上限约 10 行）。居中、不可 resize（SPEC 已锁）。

#### 内置命令实现细节
- **D-12:** 「打开日志文件」依赖日志落盘：日志写到配置目录 `logs/mybox.log`（macOS: `~/Library/Application Support/mybox/logs/mybox.log`），应用启动即开始写文件（当前 env_logger 仅 stderr，需加文件 sink）。命令直接打开该文件，必然存在。
- **D-13:** 「重启应用」机制：spawn 当前可执行文件为新进程 + 当前进程正常退出。dev 模式（cargo run）下需处理 spawn 编译产物路径。

### the agent's Discretion
- async 运行时选型（tokio vs smol vs 其他）及 runner 执行线程模型
- egui CPU 软件渲染后端的具体方案（egui-tiny-skia 或 egui 0.30 的 softbuffer 集成等），researcher 验证
- 截图命令与面板的衔接：capture 模块注册自己的 runner（含先隐藏面板再触发截图的时序落地）
- fuzzy-matcher 的具体评分参数与关键词权重
- 面板具体视觉参数（行高、内边距、圆角、颜色值、高亮色）
- `BuiltinCommands` 的具体实现形态与 4 个内置命令的注册位置
- 「退出应用」命令复用托盘退出路径（INFRA-02 已有退出菜单）
- 热键配置键名与读取方式（沿用 D-11 字符串格式 + ConfigCenter 解析）

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.

> **Research note on D-03:** The locked "核心框架代码零改动" is infeasible — egui-winit 0.30's API requires `&Window` which `WindowSpec.on_event` does not provide (source-verified). See §Architecture Patterns, Pattern 2 for the minimal additive-change list. The planner should surface this to the user as a D-03 refinement; all changes are additive framework extension points (same class as Phase 2's `on_draw`), and none alter existing window/event behavior.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PAL-01 | 用户按全局快捷键唤出命令面板浮窗 | `HotkeyManager::register_str("toggle_palette", "Cmd+Shift+Space")` — global-hotkey 0.8.0 parses `Space` via keyboard-types 0.7.0 `Code::Space` FromStr (verified in cached source, code.rs:758). Registration follows the capture module's deferred `ctx.ui().run` pattern (init runs before `hotkeys.init()`). Default in `[palette].hotkey` via `default_config()`; toggle semantics (visible→close, hidden→summon). |
| PAL-02 | 命令面板列出所有模块注册的命令 | `Module::commands() -> Vec<Command>` (SPEC-sanctioned trait extension, default `vec![]`); `CommandRegistry` assembled in `AppBuilder::build` (module commands in registration order + 4 builtins); exposed via new `ModuleContext::commands()` accessor. Palette snapshots the list at summon. ≥5 commands after assembly (1 capture + 4 builtins). |
| PAL-03 | 用户输入关键词模糊过滤命令列表 | `fuzzy-matcher 0.3.7` `SkimMatcherV2` — `fuzzy_match`/`fuzzy_indices` (top-level fns deprecated since 0.3.5 — use the matcher). Name matches weighted above description/keyword matches; `fuzzy_indices` returns char indices for the `#FF6000` LayoutJob highlight. **"jt" acceptance requires the pinyin keyword `"jietu"` on the capture command** — fuzzy-matcher does no pinyin conversion (subsequence `j..t` of `jietu` matches). |
| PAL-04 | 方向键导航选择命令，回车执行 | Pure navigation state machine (selection reset on input change, wrap-around ↑/↓, Enter-executes-first-when-no-selection) per UI-SPEC interaction contract; execution lifecycle: `Executing` state + worker thread + `pollster::block_on` + `UiThreadProxy::run` finalize; generation counter guards stale runner completion against re-summoned windows. |
| PAL-05 | 用户按 ESC 关闭命令面板 | ESC handled in `on_event_win` (keyboard events reach the palette after `focus_window()` on Floating creation); destroys the window via `WindowManagerHandle::destroy`; instance/generation tracking guarantees 5× summon-ESC leaves zero orphan windows (Phase 2 re-entrancy lesson). |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Command registration (module + builtin) | Framework (core `command.rs`) | Modules (implement `commands()`) | The `Command` type must live in core for the `Module` trait to reference it; builtins need bus/config services only core has. |
| Command enumeration for the palette | Framework (CommandRegistry in ModuleContext) | Palette module (reads `ctx.commands()`) | Registry assembled once in `AppBuilder::build` (after all modules registered); modules see it through the FRMW-02 facade. |
| Palette window lifecycle (summon/destroy) | Framework (WindowManager main thread) | Palette module (enqueues Create/Destroy) | winit windows are main-thread-bound (W2); module enqueues, App drains — unchanged architecture. |
| Palette content rendering (egui → Pixmap) | Module (egui run + tessellate + rasterize in `on_event_win`, blit in `on_draw`) | Framework (Renderer draw/present chain, `on_draw` already wired in Phase 2) | Core's renderer presents; module generates content — the Phase 2 `on_draw` split, extended with window access. |
| winit → egui input translation | Module (egui-winit State in `on_event_win`) | Framework (routes events by winit id, provides `&Arc<Window>`) | egui-winit needs `&Window`; core supplies it via the additive `on_event_win` callback. |
| Fuzzy filtering + sorting + highlight indices | Module (pure logic, `filter.rs`) | fuzzy-matcher (scoring) | Pure headless-testable logic; library only supplies the scoring primitive. |
| Navigation + palette state machine | Module (pure logic, `session.rs`) | — | No framework involvement; headless-testable exactly like Phase 2's `selection.rs`. |
| Async command execution | Module (worker thread + pollster::block_on) | Framework (UiThreadProxy for main-thread finalize) | FRMW-05: heavy work off the event loop; completion hops back via the existing proxy. |
| App exit / restart | Framework (App listens for `core/app-exit` → `el.exit()`) | Builtin runner (emits the event) | `ActiveEventLoop` only exists inside winit callbacks; runners run on worker threads — bus event + proxy hop is the existing pattern (D-08). |
| Positioning (active-monitor center) | Module (`position.rs`: xcap monitors + NSEvent cursor) | — | winit 0.30 has **no** `ActiveEventLoop::cursor_position` (verified absent in 0.30.13 source); xcap + objc2-app-kit work from the bus thread with zero core changes. |
| CJK font loading | Module (`fonts.rs`, egui FontDefinitions) | — | egui built-in fonts have no CJK glyphs (UI-SPEC hard requirement); system TTC faces loaded via `FontData { index }` (verified in epaint 0.30.0). |
| Log file sink (D-12) | App entry (`mybox-app/src/main.rs`) | — | `env_logger` is initialized in main.rs before the config dir is known to the App; dual-sink logger lives there, not in core. |

## Standard Stack

### Core (new dependencies for Phase 3)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| egui | 0.30.0 | Immediate-mode UI for the palette | **Locked by D-02.** 0.30.0 is the ONLY 0.30.x release (sparse index verified 2026-08-14; latest overall is 0.36.1). Source downloaded and inspected. |
| egui-winit | 0.30.0 | winit event → egui RawInput translation, IME/text handling, platform output | Same-version counterpart of egui; declares `winit = "0.30.5"` → resolves to workspace 0.30.13; `raw-window-handle 0.6` matches workspace. `State::on_window_event(&mut self, window, event)` / `take_egui_input(window)` / `handle_platform_output(window, _)` — all require `&Window` (verified, lines 232/263/818). |
| fuzzy-matcher | 0.3.7 | Subsequence fuzzy scoring + match indices for highlight | The standard skim-family matcher (lotabout, same author as skim); `SkimMatcherV2::fuzzy_match`/`fuzzy_indices`; Unicode-char-based (CJK-safe); `element_limit` bounds cost. |
| pollster | 1.0.1 | `block_on` for driving command futures on worker threads | The minimal "async runtime" for D-07: std `Pin<Box<dyn Future>>` type erasure + pollster on a per-invocation thread. No runtime lifecycle to manage on quit/restart. Maintained by parasyte (winit co-maintainer). |
| xcap | 0.9.8 (already workspace-pinned) | Active-monitor enumeration for centering | Reused from Phase 2 — `Monitor::all()` returns per-monitor x/y/width/height (points) + `scale_factor()`; callable from the bus worker thread (Phase 2 verified). No new version. |
| objc2-app-kit | 0.3.2 (in lock, new direct dep) | `NSEvent::mouseLocation()` for cursor position (macOS) | Already in Cargo.lock (via winit/xcap); promoted to a direct macOS-target dependency of the palette crate only. |

### Supporting (already in workspace, reused unchanged)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| winit | 0.30.13 | Window/event loop, `MonitorHandle`, `Window::request_inner_size/set_outer_position/focus_window/set_ime_allowed` | Floating window + resize-on-filter-change + focus. |
| tiny-skia | 0.12.0 | Pixmap target for the egui rasterizer | `fill_path` for solid triangles; `draw_pixmap` for clip-rect blits; premultiplied contract (Phase 2 Pitfall 2). |
| softbuffer | 0.4.8 | Present | Unchanged. **macOS drops per-pixel alpha** (`CGImageAlphaInfo::NoneSkipFirst`, verified cg.rs:328) — the palette window must be opaque; rounded corners need the NSWindow-layer trick, not transparency. |
| global-hotkey | 0.8.0 | Summon hotkey | `"Cmd+Shift+Space"` parses (`Code::Space` FromStr verified in keyboard-types 0.7.0). |
| parking_lot / crossbeam-channel / log / anyhow | pinned | State sharing, request queue, logging | Existing discipline. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled rasterizer (recommended) | `egui_software_backend` 0.0.3 | It pins **egui 0.34** — violates locked D-02 (egui 0.30). Also 0.0.x maturity and its own winit/softbuffer integration would fight ours. |
| Hand-rolled rasterizer | `egui_skia` 0.4.0 | Abandoned SkiaRust-era crate (skia-safe 0.53, egui >=0.20); no tiny-skia backend in that release. Rejected. |
| Hand-rolled rasterizer | `egui_tiny_skia` | **Does not exist on crates.io** (re-verified 2026-08-14). |
| pollster 1.0.1 | tokio 1.x | Full runtime adds lifecycle management to quit/restart paths (must drop the runtime cleanly) and ~40 crates, for commands that are all fast sync-ish ops. The async *signature* satisfies D-07's v2 intent; a real runtime arrives with the v2 AI module. |
| pollster | `futures-executor::block_on` | Equivalent; pollster is dependency-free and purpose-built. Either is acceptable — pick pollster. |
| `WindowSpec.on_event_win` (additive) | Changing `on_event` signature | Would break the capture overlay's existing closure contract. Additive is non-breaking. |
| xcap + NSEvent positioning | Core-side centering | winit 0.30 has no cursor-position API (verified absent); core-side would need a new cross-platform cursor dependency anyway. Module-side keeps the "add module, don't change core" property. |

**Installation:**
```toml
# workspace Cargo.toml [workspace.dependencies] (version-lock discipline — do NOT bump without re-verifying)
egui = "0.30.0"
egui-winit = "0.30.0"
fuzzy-matcher = "0.3.7"
pollster = "1.0.1"
objc2-app-kit = "0.3.2"   # already in lock at this version

# crates/mybox-core/Cargo.toml (D-01: egui lives in core, re-exported)
egui = { workspace = true }
egui-winit = { workspace = true }
fuzzy-matcher = { workspace = true }
pollster = { workspace = true }          # command execution helper lives in core (D-07)

# crates/modules/palette/Cargo.toml
mybox-core = { path = "../../mybox-core" }   # egui/egui-winit/fuzzy-matcher via re-exports (D-01)
xcap = { workspace = true }

# crates/modules/palette/Cargo.toml — macOS only
[target.'cfg(target_os = "macos")'.dependencies]
objc2-app-kit = { workspace = true }
```

**Version verification (performed 2026-08-14):**
```bash
cargo search egui            # latest 0.36.1; D-02 locks 0.30 → sparse index confirms 0.30.0 is the sole 0.30.x
cargo search fuzzy-matcher   # 0.3.7 ✓
cargo search pollster        # 1.0.1 ✓
curl -s https://index.crates.io/eg/ui/egui-winit   # 0.30.0 exists; declares winit 0.30.5 (unifies to 0.30.13)
# epaint 0.30.0 deps: ab_glyph >=0.2.11 (workspace 0.2.32 ✓), raw-window-handle 0.6 ✓ — no conflicts
```

## Package Legitimacy Audit

> slopcheck 0.6.1 was installed and executed successfully this session.

| Package | Registry | Age | Downloads | Source Repo | slopcheck | Disposition |
|---------|----------|-----|-----------|-------------|-----------|-------------|
| egui 0.30.0 | crates.io | 5+ yrs (0.30.0: 2025-05) | 10M+ total (egui ecosystem) | github.com/emilk/egui | [OK] | Approved |
| egui-winit 0.30.0 | crates.io | 5+ yrs | 10M+ total | github.com/emilk/egui | [OK] | Approved |
| fuzzy-matcher 0.3.7 | crates.io | 6+ yrs | millions | github.com/lotabout/fuzzy-matcher | [OK] | Approved |
| pollster 1.0.1 | crates.io | 5+ yrs | millions | github.com/zesterer/pollster | [OK] | Approved |
| objc2-app-kit 0.3.2 | crates.io | years | millions | github.com/madsmtm/objc2 | [OK] | Approved — already in Cargo.lock |
| xcap 0.9.8 | crates.io | years | — | github.com/nashaofu/xcap | [SUS] | **Already in workspace (Phase 2 approved)** — flag is a name-similarity false positive vs `clap`; no new gate needed |

**Packages removed due to slopcheck [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** xcap (false positive — typosquat heuristic on "clap" name similarity; it is the Phase 2-locked screen-capture crate already in Cargo.lock, not a new install).
**Cross-ecosystem verification:** all Rust crates; versions confirmed against the crates.io sparse index + downloaded crate sources from static.crates.io (the crates.io REST API is rate-blocked in this environment — index + static.crates.io + local registry cache were used instead, same authoritative data).
**Postinstall check (Node-only):** not applicable — Rust phase; no lifecycle scripts.
**Note:** the slopcheck run attempted a stray `cargo add` in the workspace root which failed harmlessly ("could not determine which package") — workspace Cargo.toml verified unmodified.

## Architecture Patterns

### System Architecture Diagram

```
Global hotkey Cmd+Shift+Space (global-hotkey listener thread)
  │  AppEvent::Hotkey → App::on_hotkey → bus emit core/hotkey.triggered { action: "toggle_palette" }
  ▼
PaletteModule handler (bus worker thread)
  │  toggle: session has window? → enqueue Destroy (close) : summon
  │  summon:  1. compute position: xcap Monitor::all() + NSEvent::mouseLocation (macOS)
  │              → monitor containing cursor → physical center (points × scale_factor)
  │           2. session.summon() → generation += 1, state = Idle
  │           3. enqueue WindowRequest::Create(WindowSpec { kind: Floating, inner_size: (600×s, h),
  │              position: Some(center), on_event_win: <egui-winit forward + egui run>, on_draw: <blit> })
  ▼
App.about_to_wait drains Create → create_window (main thread)
  │  Floating profile + window.focus_window()  (keyboard input under Accessory policy)
  │  (macOS) round_floating_corners(window, 12.0) — NSWindow layer cornerRadius
  │  emits core/window-created { id } → palette records window id
  ▼
winit event loop (main thread)
  ├─ on_event_win(w, event):                     ← NEW additive core callback
  │     egui_winit::State::on_window_event(w, event)
  │     if EventResponse.repaint → windows.redraw(id)
  │     ESC (palette-local check) → close_palette (destroy + state Hidden)
  │     if event == RedrawRequested:
  │        raw = state.take_egui_input(w)        (native_ppp from window.scale_factor)
  │        out = egui_ctx.run(raw, |ctx| ui::draw(ctx, &session_state))
  │        session.apply_textures_delta(out.textures_delta)
  │        prims = egui_ctx.tessellate(out.shapes, out.pixels_per_point)
  │        raster::paint(&mut session.framebuffer_pixmap, &prims)   ← tiny-skia rasterizer
  │        state.handle_platform_output(w, out.platform_output)
  ├─ on_draw(pixmap, w, h):                      ← existing Phase 2 hook
  │     pixmap.draw_pixmap(0,0, session.framebuffer.as_ref(), ..)   (1-line blit)
  │     → renderer.present()
  └─ Enter (in on_event_win, state Idle/Filtering) → session.execute(selected)
        ├─ command.hide_before_execute (capture.start) → enqueue Destroy FIRST, state Hidden
        └─ spawn "mybox-cmd-<id>" thread: pollster::block_on((cmd.runner)())
             → ui.run(move || finalize(gen, result)):
                 Ok  → state Hidden + enqueue Destroy (guarded by generation)
                 Err → state Error (window stays; any key closes — D-05)
```

### Minimal Core Changes (replaces D-03's "零改动" — all additive, all framework extension points)

| # | Change | Where | Breaking? | Justification |
|---|--------|-------|-----------|---------------|
| C1 | `Module::commands(&self) -> Vec<Command>` (default `vec![]`) | `module.rs` | No (default impl) | SPEC requirement 1 — explicitly sanctioned trait extension. |
| C2 | New `command.rs`: `Command`, `CommandRunner = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> + Send + Sync>`, `CommandRegistry`, `BuiltinCommands`, `run_command(cmd, ui, on_done)`; `ModuleContext::commands()` accessor + field | core | No | Command type must live in core (trait reference); registry assembled in `AppBuilder::build`. |
| C3 | `WindowSpec.on_event_win: Option<Box<dyn Fn(&Arc<winit::window::Window>, &winit::event::WindowEvent) + Send + Sync>>` + invocation in `App::window_event` (alongside `on_event`) | `window.rs`, `app.rs` | No (new optional field) | egui-winit requires `&Window` (source-verified). Only way to keep existing `on_event` contract intact. |
| C4 | `create_window`: extend the `focus_window()` call to `WindowKind::Floating`; add `with_resizable(false)` to the Floating profile in `window_attributes` | `app.rs`, `window.rs` | No (Floating has zero existing users) | Keyboard input under Accessory policy (Phase 2 Pitfall 7 lesson); prevents the `overlay-window-movable-at-edge` bug class for borderless Floating. |
| C5 | `AppEvent::Exit` + bus subscription `core/app-exit` → `el.exit()` in `user_event` | `app.rs` | No (new variant) | Quit/restart builtins run on worker threads; `FrameworkEvent::AppExit` already exists but nothing handles it. |
| C6 | (macOS, optional) `round_floating_corners(&Window, f32)` — NSWindow contentView layer `cornerRadius + masksToBounds` via objc2-app-kit/objc2-quartz-core (both in lock) | `window.rs` | No | softbuffer drops alpha on macOS — per-pixel rounded corners impossible (verified cg.rs:328). OS-layer rounding is the D-08 圆角 path. Fallback: square corners, Phase 4 polish. |

Plus app-level (not core): `mybox-app/src/main.rs` dual-sink logger (D-12) + palette module registration; `crates/modules/test/src/lib.rs` fix of the pre-existing broken `WindowRequest` match (Phase 2 known issue) so the full workspace test suite compiles.

### Recommended Project Structure

```
crates/mybox-core/src/
├── command.rs            # NEW: Command, CommandRunner, CommandRegistry, BuiltinCommands, run_command
├── module.rs             # + commands() default method (C1)
├── window.rs             # + on_event_win field (C3), Floating resizable(false) (C4), round_floating_corners (C6)
├── context.rs            # + commands() accessor (C2)
└── app.rs                # + on_event_win routing, Floating focus, AppEvent::Exit (C3/C4/C5)

crates/modules/palette/
├── Cargo.toml            # deps: mybox-core only + xcap (+ objc2-app-kit macOS)
└── src/
    ├── lib.rs            # PaletteModule (id "palette"): hotkey register + toggle handler, command snapshot, lifecycle
    ├── session.rs        # PaletteSession: Hidden/Idle/Filtering/Empty/Executing/Error + selection index + input buffer
    │                     #   + generation counter + window id + framebuffer Pixmap + textures map (pure/headless-testable)
    ├── filter.rs         # fuzzy ranking + LayoutJob highlight indices (SkimMatcherV2; name>description>keywords weights)
    ├── ui.rs             # egui closure per UI-SPEC: card, SearchInput, CommandRow (LayoutJob), StatusLine, Empty/ErrorState
    ├── raster.rs         # tessellate → tiny-skia rasterizer (solid fast path + textured barycentric)
    ├── position.rs       # active-monitor center (xcap + NSEvent, macOS; first-monitor fallback)
    ├── execute.rs        # runner dispatch: worker thread + pollster::block_on + UiThreadProxy finalize
    └── fonts.rs          # CJK font loading (Hiragino Sans GB.ttc W3/W6 faces, FontData.index)
    └── bin/palette_checks.rs + tests/   # subprocess-per-check #[ignore] harness (Phase 2 pattern)
```

### Pattern 1: egui-on-tiny-skia rendering (the D-03 mechanism)

**What:** egui runs in `on_event_win` on `RedrawRequested`; the palette rasterizes tessellated output into its own `tiny_skia::Pixmap` framebuffer; `on_draw` blits the framebuffer into the core Pixmap. Because `on_event` runs before the renderer match in `window_event` (app.rs:391-408), the framebuffer is fresh when `handle_redraw` presents.

**Rasterizer (raster.rs, ~250 lines):**
```rust
// Source: epaint-0.30.0/src/{lib.rs:126,mesh.rs:12-48,textures.rs:277} + tiny-skia 0.12 (workspace)
let full_output = egui_ctx.run(raw_input, |ctx| ui::draw(ctx, &state));
let primitives = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
// apply full_output.textures_delta.set before drawing (font atlas updates)
for ClippedPrimitive { clip_rect, primitive } in primitives {
    let Primitive::Mesh(mesh) = primitive else { continue }; // no Callback primitives in palette
    // 1. small Pixmap for the clip rect (rounded rects clip via egui clip_rect)
    // 2. for each triangle (indices triples):
    //    - solid fast path: v0.color==v1.color==v2.color → PathBuilder + fill_path (Paint::set_color_rgba8)
    //    - textured (TextureId::Managed(0) = font atlas) or gradient: barycentric per-pixel sample,
    //      uv-bilinear from the stored texture image, multiply by vertex Color32, premultiply (reuse
    //      core premul_rgba_to_u32), write pixel
    // 3. window_pixmap.draw_pixmap(clip.min.x, clip.min.y, clip_pixmap.as_ref(), ..)
}
```
**Key contracts (verified):** egui `Color32` is straight (non-premultiplied) RGBA; tiny-skia Pixmaps are premultiplied — convert when writing (Phase 2 Pitfall 2 discipline). Font atlas = `TextureId::Managed(0)`, RGBA8 `ImageData::Font(FontImage)`. Palette uses no other textures (no `egui_extras` image loaders). Performance: panel ≤ 600×560 px, text ≈ tens of thousands of glyph pixels per frame — barycentric sampling is single-digit-ms worst case, and repaints happen only on input events (`ControlFlow::Wait` preserved).

### Pattern 2: window access via additive `on_event_win` (C3)

**What:** The module's egui-winit `State` must be created with `&dyn HasDisplayHandle` and every input call needs `&Window` (source-verified). Core adds a second optional per-window callback that receives the window, invoked right after `on_event`:
```rust
// window.rs — additive field, mirror of on_event:
pub on_event_win: Option<Box<dyn Fn(&Arc<winit::window::Window>, &winit::event::WindowEvent) + Send + Sync>>,
// app.rs window_event:
if let Some(cb) = &state.spec.on_event_win {
    if let Some(w) = &state.window { cb(w, &event); }   // state.window is Some for any live winit window
}
```
**Palette usage:** lazily construct `egui_winit::State::new(egui_ctx.clone(), ViewportId::ROOT, w.as_ref(), None, None, None)` on the first call; then `state.on_window_event(w, event)`, and on `RedrawRequested` `take_egui_input(w)` → `ctx.run` → rasterize. The `Arc<Window>` also enables programmatic `request_inner_size` (adaptive height per filter count, D-11) and `set_outer_position` (re-center after resize) — no new WindowRequest variants needed.

### Pattern 3: command registry + builtins (C1/C2)

**What:** `Command` is data + a runner closure; the registry is a core service assembled before module init.
```rust
// command.rs
pub struct Command {
    pub id: &'static str,
    pub name: String,               // "开始截图"
    pub description: String,        // non-empty (SPEC req 1)
    pub keywords: Vec<&'static str>,// e.g. ["截图", "capture", "screen", "jietu"]  ← pinyin for PAL-03 "jt"
    pub runner: CommandRunner,      // Arc<dyn Fn() -> BoxFuture + Send + Sync> — Arc makes Command Clone
    pub hide_before_execute: bool,  // true for capture.start (SPEC: panel must never appear in screenshots)
}
pub struct CommandRegistry { commands: Vec<Command> }   // module commands first (registration order), then builtins
impl CommandRegistry { pub fn all(&self) -> Vec<Command>; }
```
`AppBuilder::build`: after module registration → `registry.commands()` per module → append `BuiltinCommands::build(bus, config_dir, log_path)` → store `Arc<CommandRegistry>` in ModuleContext. Duplicate command ids rejected like duplicate module ids. Builtin runners (all `Box::pin(async { ... })`, no real IO awaits):
- `builtin.quit` — emit `core/app-exit` (FrameworkEvent::AppExit already exists) → App → `el.exit()` (C5). Reuses the tray quit path semantics.
- `builtin.open_config` — `Command::new("open").arg(config_dir)` (macOS) / `explorer` (Windows, Phase 4 verifies).
- `builtin.restart` — `Command::new(std::env::current_exe()?).spawn()` then emit `app-exit`. `current_exe()` resolves to `target/debug/mybox-app` under `cargo run` (D-13 handled automatically); child survives parent exit.
- `builtin.open_log` — `open`/`explorer` on `<config_dir>/logs/mybox.log` (exists from startup — D-12).

### Pattern 4: async execution lifecycle (D-04/D-07)

**What:** The runner is a std `Future`; each execution gets its own named worker thread; completion hops back through the existing `UiThreadProxy`. A **generation counter** prevents stale completions from touching a re-summoned window (the re-entrancy lesson generalized):
```rust
// execute.rs
session.set_executing(gen, cmd.id());            // Executing state; input disabled (D-04)
if cmd.hide_before_execute { session.close(); }  // destroy window BEFORE runner (capture exception)
let (ui, session, gen) = (ui.clone(), session.clone(), session.generation());
std::thread::Builder::new().name(format!("mybox-cmd-{}", cmd.id)).spawn(move || {
    let result = pollster::block_on((cmd.runner)());   // pollster 1.0.1
    ui.run(Box::new(move || session.finalize(gen, result))); // Ok→Hidden+destroy; Err→Error state (D-05)
}).expect("spawn command thread");
```
**Runtime decision (discretion resolved):** pollster + per-invocation thread — **no tokio/smol this phase**. Rationale: all four builtins + capture are fast OS/CPU-bound ops; a full runtime would need clean shutdown handling on the quit/restart paths (drop the runtime or hang on exit) for zero benefit. The async *signature* is the v2 enabler (D-07's stated reason); a streaming AI command in v2 can introduce tokio then without touching the `Command` type. Documented risk: a future that internally expects a reactor (e.g. spawns tokio tasks) cannot run under `block_on` — irrelevant for Phase 3 commands.

### Pattern 5: summon positioning (active-monitor center, no core change)

**What:** winit 0.30 has **no** `ActiveEventLoop::cursor_position` (verified absent in 0.30.13). The palette computes the physical center itself at summon time (bus thread — xcap is thread-safe):
```rust
// position.rs (macOS; Windows = Phase 4)
use xcap::Monitor;                       // points, top-left origin, per-monitor
let cursor = unsafe { objc2_app_kit::NSEvent::mouseLocation() }; // NSPoint, points, BOTTOM-left origin
let main_h = /* CGDisplayBounds(CGMainDisplayID()).height via objc2-core-graphics, or max(monitor.y+height) */;
let (cx, cy) = (cursor.x, main_h - cursor.y);                    // normalize to top-left origin
let monitor = Monitor::all()?.into_iter().find(|m| contains(m, cx, cy))
    .unwrap_or_else(|| first_monitor());                         // fallback: primary/first
let scale = monitor.scale_factor()? as f64;
let (w, h) = (600.0 * scale, height_logical * scale);            // logical → physical (UI-SPEC: round(600×sf))
let pos = ((monitor.x() as f64 + monitor.width() as f64 / 2.0) * scale - w / 2.0,
           (monitor.y() as f64 + monitor.height() as f64 / 2.0) * scale - h / 2.0);
// → WindowSpec { kind: Floating, inner_size: Some((w, h)), position: Some((pos)) }
```
Height adapts per filter count via `window.request_inner_size` + `set_outer_position` (re-center) from inside `on_event_win` (Pattern 2). Note the coordinate-origin flip (NSEvent bottom-left vs xcap top-left) — the one conversion point; unit-test the math, verify visually on the dev Mac.

### Pattern 6: CJK fonts (UI-SPEC hard requirement)

**What:** egui 0.30 ships ASCII-only default fonts. Load Hiragino Sans GB.ttc (verified present at `/System/Library/Fonts/Hiragino Sans GB.ttc`) as **two faces** into the head of the Proportional family:
```rust
// fonts.rs — epaint 0.30.0 FontData has `pub index: u32` (fonts.rs:119, VERIFIED) → TTC face selection
let bytes = std::fs::read("/System/Library/Fonts/Hiragino Sans GB.ttc")?;
let mut defs = egui::FontDefinitions::default();
defs.font_data.insert("hiragino-w3".into(), egui::FontData::from_owned(bytes.clone()).into());          // index 0
defs.font_data.insert("hiragino-w6".into(), egui::FontData { index: 1, ..egui::FontData::from_owned(bytes) }.into());
defs.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "hiragino-w3".into());
defs.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(1, "hiragino-w6".into());
egui_ctx.set_fonts(defs);   // once, before first frame; keep the default family as fallback
```
Windows font discovery is deferred to Phase 4 (UI-SPEC). The TTC is ~40MB resident — accepted for MVP.

### Anti-Patterns to Avoid
- **Giving the palette the `ActiveEventLoop` or touching winit windows from the bus thread:** windows/loop are main-thread-bound (W2). Everything the palette does with the window happens inside `on_event_win` (main thread).
- **Making `egui::Context` part of `Send + Sync` state without a mutex:** `egui::Context` is `Send` but NOT `Sync`; the `WindowSpec` closures are `Send + Sync`. Store it as `parking_lot::Mutex<egui::Context>` (only ever locked on the main thread — zero contention).
- **Rasterizing into the core PixmapMut directly from `on_event`:** the draw must happen in the renderer's `draw` call (before `present`). Rasterize into the palette-owned framebuffer Pixmap on `RedrawRequested`, blit in `on_draw`.
- **Hand-rolling winit→egui input translation:** egui-winit handles IME events, `KeyboardInput.text`, scale-factor, focus — exactly the surfaces a text-input palette cannot afford to get wrong (Chinese IME works through the standard path; UI-SPEC only excludes *special* IME handling).
- **Runner completion touching a re-summoned window:** always guard finalize with the generation counter captured at summon.
- **Using per-pixel alpha for rounded corners on macOS:** softbuffer drops alpha (`CGImageAlphaInfo::NoneSkipFirst`, cg.rs:328) — transparent corners render black. Opaque window + NSWindow layer rounding (C6), or accept square corners.
- **Reusing the `on_event` closure for egui-winit:** it has no `&Window` — silently hand-rolling the missing scale-factor handling there is the flicker/misplaced-text bug class. Use the new `on_event_win`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Fuzzy scoring + match indices | Custom subsequence scorer | fuzzy-matcher 0.3.7 `SkimMatcherV2` | Battle-tested skim scoring (boundary/camel/consecutive bonuses), `fuzzy_indices` for highlight, `element_limit` safety. |
| winit → egui input translation | Manual RawInput from KeyboardInput/Mouse events | egui-winit 0.30 `State` | IME composition, `event.text` passthrough, scale-factor, pointer wrapping, focus tracking — all edge cases. |
| Blocking on futures | Custom poll loop / thread::yield | pollster 1.0.1 `block_on` | Correct Waker handling (~100-line crate, no deps). |
| Hotkey string parsing | Custom "Cmd+Shift+Space" parser | global-hotkey `HotKey: FromStr` (Phase 1 D-11) | Already built and verified; `Space` parses (keyboard-types Code::Space). |
| UI framework itself | Custom widget toolkit on tiny-skia | egui 0.30 (tessellate + small rasterizer) | Text layout, LayoutJob rich text/highlight, input box, scroll area — the rasterizer is ~250 lines vs. re-implementing egui. |
| Opening files/dirs cross-platform | Custom file-manager invocation | `std::process::Command` `open` (macOS) / `explorer` (Windows) | No shell, no injection surface; Phase 4 verifies the Windows path. |

**Key insight:** the only genuinely hand-rolled piece is the egui-mesh→tiny-skia rasterizer, and that is forced: no maintained crate exists for egui 0.30 + tiny-skia 0.12 (all candidates verified dead/wrong-version). It is bounded, pure, and unit-testable headlessly (tessellate a known frame → assert pixel colors).

## Common Pitfalls

### Pitfall 1: Palette window never receives keyboard input (macOS Accessory)
**What goes wrong:** panel summons and renders, but typing does nothing.
**Why it happens:** with `ActivationPolicy::Accessory` a global hotkey does not activate the app; `set_visible` alone doesn't make the window key. Phase 2 hit exactly this for the overlay (Pitfall 7, fixed by `focus_window()` on creation for Overlay only).
**How to avoid:** C4 — extend the existing `focus_window()` call in `create_window` to `WindowKind::Floating`. Verify in the manual checklist (type immediately after summon).
**Warning signs:** mouse hover works, keys dead; fixed by clicking once.

### Pitfall 2: Rounded corners render as black squares on macOS
**What goes wrong:** D-08 圆角卡片 shows square opaque corners.
**Why it happens:** softbuffer's macOS backend uses `CGImageAlphaInfo::NoneSkipFirst` (cg.rs:328) — per-pixel alpha is dropped; painting transparent corners yields black.
**How to avoid:** keep the window opaque (`#202020` full-bleed) and round the actual NSWindow layer: contentView layer `setCornerRadius(12.0)` + `setMasksToBounds(true)` via objc2-app-kit/objc2-quartz-core (C6, pattern of `elevate_overlay_window`). Fallback if the layer trick fails visually: square corners, logged as Phase 4 polish.
**Warning signs:** dark corner triangles over light wallpapers in the manual checklist.

### Pitfall 3: Stale runner completion resurrecting/deleting the wrong window
**What goes wrong:** user executes a command, presses the hotkey to close (runner continues — UI-SPEC), re-summons, and the old runner's completion destroys the NEW window (or errors into a fresh palette).
**Why it happens:** completion is asynchronous via `UiThreadProxy`; without identity tracking, the finalize closure can't tell which palette instance it belongs to.
**How to avoid:** generation counter incremented per summon (Pattern 4); finalize acts only when `gen == session.generation()`. Also covers the capture exception path (window already destroyed before runner starts).
**Warning signs:** panel closes spontaneously right after re-summon; error text appears in a fresh panel.

### Pitfall 4: Ordering between palette Destroy and capture overlay Create
**What goes wrong:** palette window appears in the screenshot.
**Why it happens:** if capture overlays were created while the palette Destroy request is still queued, the screenshot includes the panel.
**How to avoid:** `hide_before_execute` enqueues Destroy **before** the runner is invoked (single FIFO crossbeam channel → Destroy drains before any later Create). The capture runner's own `capture_all_monitors` runs before its overlays are created, so the ordering is doubly safe. Verify with the SPEC acceptance flow.
**Warning signs:** palette visible in the captured overlay image.

### Pitfall 5: Text renders as tofu boxes (□)
**What goes wrong:** all Chinese text missing despite correct code.
**Why it happens:** egui's default fonts are ASCII-only; the CJK font must be inserted as the FIRST Proportional family entry (UI-SPEC hard requirement), and it must happen before the first frame (or egui re-rasterizes the atlas).
**How to avoid:** Pattern 6 — `set_fonts` once at context setup, both W3/W6 faces via `FontData.index`. Windows font discovery is Phase 4 — gate the load `#[cfg(target_os = "macos")]` with ASCII-only fallback elsewhere.
**Warning signs:** squares in rows/input; also visible headlessly (unit-test: run a frame with Chinese text, assert atlas upload contains >0 glyphs).

### Pitfall 6: "jt" doesn't match 开始截图
**What goes wrong:** SPEC acceptance "输入'截图'或'jt'均能命中截图命令" fails for "jt".
**Why it happens:** fuzzy-matcher matches char subsequences of the strings it is given; `j`,`t` are not a subsequence of "开始截图" or "capture"/"screen".
**How to avoid:** the capture command's keywords must include the pinyin `"jietu"` (`j-i-e-t-u` contains `j..t`). Fuzzy scoring compares the pattern against name, description, AND keywords (weighted). This is a data fix, not a code fix.
**Warning signs:** "截图" matches, "jt" returns empty state.

### Pitfall 7: Full workspace test suite fails to compile (pre-existing)
**What goes wrong:** `cargo nextest run` (workspace-wide) fails on mybox-test's `WindowRequest` match — no `Redraw(_)`/`SetCursor(_, _)` arms (documented in 02-04-SUMMARY "Out-of-scope discovery").
**Why it happens:** Phase 2 added variants without updating the test module's test code.
**How to avoid:** fix the two missing arms in Phase 3 (2 lines) so the full suite is green — acceptance criterion 9 requires workspace-wide health.
**Warning signs:** `-p mybox-test` nextest compile error while `cargo check --workspace` passes.

### Pitfall 8: Frozen palette after first frame
**What goes wrong:** typing shows nothing; the panel renders once then stops.
**Why it happens:** `ControlFlow::Wait` + no redraw request on input — the Phase 2 "Redraw never fires" class.
**How to avoid:** in `on_event_win`, when `EventResponse.repaint` is true, enqueue `WindowManagerHandle::redraw(id)` (existing mechanism). Also request redraw after `request_inner_size` height changes.
**Warning signs:** panel updates only when re-summoned.

## Code Examples

Verified patterns from source-inspected crates (all paths downloaded this session):

### egui frame in on_event_win (egui-winit 0.30.0, lines 232/263)
```rust
// Source: egui-winit-0.30.0/src/lib.rs (verified signatures)
let mut winit_state = egui_winit::State::new(
    ctx.clone(), egui::ViewportId::ROOT, window.as_ref(), None, None, None);
// on every event:
let resp = winit_state.on_window_event(window, event);     // accumulates into internal egui_input
if resp.repaint { windows.redraw(id); }
// on RedrawRequested:
let raw_input = winit_state.take_egui_input(window);       // sets native_pixels_per_point from window.scale_factor()
let full_output = ctx.run(raw_input, |ctx| ui::draw(ctx, &session_state));
let primitives = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
raster::paint(&mut framebuffer_pixmap, &primitives, &textures);   // raster.rs
winit_state.handle_platform_output(window, full_output.platform_output);
```

### Fuzzy filter with highlight indices (fuzzy-matcher 0.3.7)
```rust
// Source: fuzzy-matcher-0.3.7/src/skim.rs (SkimMatcherV2; top-level fns deprecated since 0.3.5)
use fuzzy_matcher::skim::SkimMatcherV2;
let matcher = SkimMatcherV2::default().smart_case();       // ignore_case() for ASCII case-insensitive
let (score, indices) = matcher.fuzzy_indices("开始截图", "jt")?;  // None = no match
// indices: Vec<usize> = CHAR positions (IndexType = usize) — convert to byte ranges for LayoutJob:
// ranking: name_score = matcher.fuzzy_match(name, pattern) → primary; then description, then max(keywords).
// Tie-break by registration order (stable sort) — UI-SPEC lifecycle rule 4.
```
For `"jt"`: `fuzzy_match("jietu", "jt")` = Some(score) (subsequence j..t) — hence the pinyin keyword.

### LayoutJob highlight (egui 0.30)
```rust
// Source: egui-0.30.0 (LayoutJob/TextFormat stable API)
let mut job = egui::text::LayoutJob::default();
job.append(&name[..byte_range.start], 0.0, TextFormat { color: WHITE, font_id: FontId::new(14.0, FontFamily::Proportional) });
job.append(&name[byte_range], 0.0, TextFormat { color: ACCENT /* #FF6000 */, font_id: FontId::new(14.0, FontFamily::Proportional) });
ui.label(job);
```

### Builtin runner shapes (core command.rs)
```rust
// Source: project patterns (EventBus emit, std::process::Command)
Command {
    id: "builtin.quit", name: "退出应用".into(), description: "退出 mybox 应用".into(),
    keywords: vec!["退出", "quit", "exit"], hide_before_execute: false,
    runner: Arc::new(|| Box::pin(async move {
        bus.emit(Event { from: "core", kind: "app-exit",
            payload: EventPayload::Framework(FrameworkEvent::AppExit) });
        Ok(())
    })),
}
// restart: std::process::Command::new(std::env::current_exe()?).spawn()?; then emit app-exit
// open_config / open_log: #[cfg(target_os)] "open"/"explorer" with the path arg — no shell
```

### App exit handling (C5)
```rust
// app.rs — AppEvent::Exit arm (el is the ActiveEventLoop already available in user_event):
AppEvent::Exit => el.exit(),
// subscription installed in App::run (alongside the other forwarders):
//   self.bus.on(EventFilter::kind("core", "app-exit"), handler → proxy.send_event(AppEvent::Exit))
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| STACK.md "egui 0.29+ / egui-winit 0.29+" | egui 0.30.0 + egui-winit 0.30.0 (D-02), both winit-0.30-based | 2025-05 (egui 0.30) | 0.30.0 is the sole 0.30.x — no patch churn; egui latest is 0.36.1 (do NOT drift — D-02 lock + version-lock discipline). |
| "egui-tiny-skia 不存在" (Phase 1/2) | still true in 2026-08 (re-verified); `egui_software_backend` 0.0.3 exists but pins egui 0.34 | — | The hand-rolled rasterizer remains the only D-02-compatible path. |
| winit `ActiveEventLoop::cursor_position` (assumed available) | **absent in winit 0.30.13** (verified) | — | Positioning must use xcap + NSEvent (module-side) — no core change. |
| top-level `fuzzy_matcher::fuzzy_match` | `SkimMatcherV2` methods (top-level fns deprecated since 0.3.5) | 0.3.5 | Planner must use the matcher API, not the deprecated fns. |
| Async runners imply tokio | std `BoxFuture` + pollster::block_on per invocation | — | Async signature without runtime lifecycle cost; tokio arrives with v2 AI module. |

**Deprecated/outdated:**
- `egui_skia` 0.4.0: SkiaRust-era, effectively abandoned — do not use.
- `egui_software_backend` 0.0.3: egui 0.34 pin conflicts with D-02 — do not use.
- `fuzzy_matcher::{fuzzy_match, fuzzy_indices}` (top-level): deprecated since 0.3.5 — use `SkimMatcherV2`.

## Assumptions Log

> All claims tagged `[ASSUMED]` below need user confirmation before becoming locked decisions.

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Hiragino Sans GB.ttc face order is index 0 = W3 (regular), index 1 = W6 (bold) | Pattern 6 (fonts) | Wrong index → bold/regular swapped. Verify visually at runtime; fallback: use W3 for both weights. |
| A2 | macOS NSWindow contentView layer `cornerRadius + masksToBounds` clips the softbuffer-rendered content | Pattern 2 / Pitfall 2 (C6) | If the layer trick doesn't clip, corners render square — fallback: accept square corners, log as Phase 4 polish (D-08 partially unmet on macOS MVP). |
| A3 | NSEvent.mouseLocation ↔ xcap coordinate conversion math (bottom-left vs top-left origin) is correct | Pattern 5 (positioning) | Wrong flip → panel off-center vertically. Single-monitor dev Mac makes verification easy; unit-test the math, verify visually. Multi-monitor correctness deferred (SPEC). |
| A4 | `window.focus_window()` on Floating grants keyboard focus under Accessory policy — same mechanism verified for Overlay in Phase 2 | Pitfall 1 (C4) | If focus is still flaky, macOS `NSApp.activateIgnoringOtherApps` escalation is the next lever (Phase 2 Pitfall 7 fallback). Manual checklist verifies. |
| A5 | pollster::block_on on a per-invocation thread is sufficient for all Phase 3 runners; no command needs a reactor | Pattern 4 (D-07 runtime) | A runner that internally spawns tasks onto a runtime would hang. Phase 3 runners are sync-style; tokio is the documented v2 upgrade. |
| A6 | `"Cmd+Shift+Space"` registers successfully with the OS (parseability verified via keyboard-types `Code::Space`; OS-level reservation is separate) | Standard Stack | If the OS reserves the combo, registration fails → warn-and-continue (existing pattern), user overrides in `[palette].hotkey`. |
| A7 | Adding `"jietu"` to the capture command's keywords satisfies the "jt" acceptance criterion (subsequence of pinyin) | PAL-03 / Pitfall 6 | If the user intended literal "jt" initials of a different form, the keyword list is data — trivially adjustable. |
| A8 | egui 0.30.0 compiles cleanly against workspace deps (ab_glyph 0.2.32, raw-window-handle 0.6, winit 0.30.13) — deps verified at declaration level, actual resolution happens at `cargo add` time | Standard Stack | A transitive conflict would surface at build; all declarations inspected show compatible ranges. |
| A9 | 600px logical width × scale_factor is the correct physical size (UI-SPEC: `round(600 × scale_factor)`) | Pattern 5 | Retina non-integer scaling is standard; verify crispness in the manual checklist. |

## Open Questions (RESOLVED)

> Resolution recorded 2026-08-14 at plan-check time. Every recommendation below is implemented by a plan task.

1. **Rounded corners on macOS MVP (A2) — is the NSWindow-layer trick acceptable as the D-08 圆角 implementation?**
   - RESOLVED: implement C6 with the square-corner fallback; log outcome in the phase summary. → 03-01 Task 2
   - What we know: softbuffer drops alpha (verified); layer rounding is the standard objc2 approach and both objc2-app-kit/objc2-quartz-core 0.3.2 are in the lock file.
   - What's unclear: whether it visually clips exactly (needs a 5-minute manual check after implementation).

2. **Fix the pre-existing mybox-test compile break in Phase 3 scope?**
   - RESOLVED: yes — 2-line fix (missing `Redraw`/`SetCursor` match arms), prevents Phase 3 test noise. → 03-02 Task 3
   - What we know: 02-04-SUMMARY documents it as out-of-scope-then; acceptance criterion 9 requires `cargo check --workspace` (which passes) but full test runs would fail.

3. **Windows cross-check without an installed target**
   - RESOLVED: `rustup target add x86_64-pc-windows-msvc` + `cargo check --target x86_64-pc-windows-msvc` (check needs no linker). If the network blocks the toolchain download, record as Phase 4. → 03-02 Task 3
   - What we know: `x86_64-pc-windows-msvc` target not installed (only aarch64-apple-darwin + wasm32). Acceptance allows "等价检查通过或记录为 Phase 4 事项".

4. **Should capture's hotkey path (`Cmd+Shift+S`) remain in parallel with the palette's capture command?**
   - RESOLVED: keep both (no behavior removal in this phase); the capture command runner reuses the same `start_capture` internals with the session's re-entrancy guard intact. → 03-01 Task 2
   - What we know: SPEC does not remove the Phase 2 hotkey; the palette is the "统一入口" going forward but hotkeys are additive.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | whole phase | ✓ | rustc/cargo 1.97.1 | — |
| cargo-nextest | test runs | ✓ | 0.9.143 | `cargo test` |
| Xcode + macOS toolchain | objc2-app-kit calls, build | ✓ | Xcode 26.3 | — |
| Hiragino Sans GB.ttc | CJK font (UI-SPEC) | ✓ | /System/Library/Fonts/Hiragino Sans GB.ttc | — |
| Arial.ttf (ASCII fallback) | — | ✓ | present (Phase 2 verified) | — |
| crates.io network (index + static.crates.io) | adding egui/egui-winit/fuzzy-matcher/pollster | ✓ | reachable (REST API blocked; index + downloads work — same data) | local registry cache has egui 0.36 only — 0.30.0 must be downloaded at `cargo add` time |
| Real macOS desktop session | manual checklist + `#[ignore]` integration | ✓ | dev machine | — |
| `x86_64-pc-windows-msvc` target | Windows cross-check (acceptance 10) | ✗ | — | `rustup target add x86_64-pc-windows-msvc`, or record as Phase 4 per acceptance text |
| slopcheck | package legitimacy gate | ✓ | 0.6.1 (installed this session) | — |

**Missing dependencies with no fallback:** none blocking — all core tooling present.
**Missing dependencies with fallback:** Windows check target (rustup add or Phase 4 deferral); egui 0.30.0 crate download requires network at first `cargo add` (network verified working).

## Validation Architecture

> Nyquist validation enabled (`workflow.nyquist_validation: true` in `.planning/config.json`).

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo-nextest 0.9.143 (unit/integration) + `cargo test -- --ignored` (display/OS, subprocess-per-check) |
| Config file | workspace `Cargo.toml` + per-crate `[dev-dependencies]` |
| Quick run command | `cargo nextest run -p mybox-core -p mybox-palette` |
| Full suite command | `cargo nextest run && cargo test -- --ignored` |
| Estimated runtime | ~15s quick; ~90s full |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PAL-01 | toggle state machine: summon creates (position computed), re-trigger closes, 5× summon/ESC no residue | unit (headless session + fake handles) | `cargo nextest run -p mybox-palette session::tests` | ❌ Wave 0 |
| PAL-02 | registry: ≥5 commands, module-first order, duplicate id rejected, non-empty name/description | unit | `cargo nextest run -p mybox-core command::tests` | ❌ Wave 0 |
| PAL-03 | filter: "截图" and "jt" hit capture.start first; no-match → Empty; empty input → all; highlight indices correct; tie-break stable | unit (pure) | `cargo nextest run -p mybox-palette filter::tests` | ❌ Wave 0 |
| PAL-04 | navigation: selection reset on input, ↑/↓ wrap, Enter executes first when none; execute: Executing state, runner runs off main thread, Ok→close, Err→Error state, generation guard | unit (fake runner + counting) | `cargo nextest run -p mybox-palette session::tests execute::tests` | ❌ Wave 0 |
| PAL-05 | ESC → destroy enqueued, no command executed | unit | `cargo nextest run -p mybox-palette session::tests::esc` | ❌ Wave 0 |
| Capture exception | hide-before-execute enqueues Destroy before runner invocation (queue order assertion) | unit | `cargo nextest run -p mybox-palette execute::tests` | ❌ Wave 0 |
| Builtins | quit emits app-exit; restart spawns current_exe then exits; open_config/open_log invoke platform opener with the right path (injectable spawner) | unit | `cargo nextest run -p mybox-core command::tests` | ❌ Wave 0 |
| Rasterizer | headless egui frame (Chinese label) → tessellate → framebuffer has non-background pixels; solid fast path == barycentric path on solid triangle | unit (no window — egui Context is headless-safe) | `cargo nextest run -p mybox-palette raster::tests` | ❌ Wave 0 |
| End-to-end | real window: summon/focus/type/enter/esc; capture.start hides palette before overlay appears | integration (#[ignore], subprocess-per-check) + manual checklist | `cargo test -- --ignored -p mybox-palette` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo nextest run -p mybox-core -p mybox-palette`
- **Per wave merge:** `cargo nextest run && cargo test -- --ignored`
- **Phase gate:** full suite green + `cargo check --workspace` + manual checklist (5 success criteria, real desktop session).

### Wave 0 Gaps
- [ ] `crates/modules/palette/` — new crate (lib/session/filter/ui/raster/position/execute/fonts + tests + bin/palette_checks.rs)
- [ ] `crates/mybox-core/src/command.rs` — Command/CommandRegistry/BuiltinCommands/run_command + tests
- [ ] Core additive changes C1–C6 with tests (on_event_win routing, Floating profile assertions, AppEvent::Exit)
- [ ] `crates/modules/capture/src/lib.rs` — `commands()` impl (runner reusing start_capture) + keyword `"jietu"`
- [ ] `crates/mybox-app/src/main.rs` — dual-sink logger (D-12) + palette module registration
- [ ] `crates/modules/test/src/lib.rs` — fix pre-existing WindowRequest match arms (Pitfall 7)
- [ ] `cargo add egui egui-winit fuzzy-matcher pollster` (behind slopcheck — already [OK]-verified this session)
- [ ] `rustup target add x86_64-pc-windows-msvc` (or Phase 4 deferral note)

## Security Domain

> `security_enforcement` is absent from config.json (absent = enabled). No network/auth surface; the palette processes local user input only.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — (single local user, no auth) |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | Fuzzy pattern length cap (e.g. 64 chars) before matching; `fuzzy-matcher` `element_limit` bounds scoring cost; TextEdit content is rendered internally only (no injection surface — egui escapes/lays out text, no HTML/format-string context); never parse command payloads from user input. |
| V6 Cryptography | no | — |

### Known Threat Patterns for the palette stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malicious/erroneous event payloads triggering code paths | Elevation of Privilege | T-1-02 discipline: the palette only reacts to `core/hotkey.triggered` action strings it recognizes; command ids execute only via the compile-time registry (no dynamic command injection). |
| `open`/`explorer` invoked with attacker-controlled paths | Injection | Paths come from `directories::ProjectDirs` (config dir) and `current_exe()` — compile-time/OS-derived, never from user input or events; `Command::new(arg)` (no shell). |
| Runner panics killing the event loop | Availability | Runner runs on its own worker thread (`block_on` inside spawn); finalize executes via `UiThreadProxy` which core already wraps in `catch_unwind` (app.rs:420). |
| Log file content disclosure | Information Disclosure | Log dir is the standard user config dir (user-owned); no secrets are logged by core (existing INFRA-03 discipline). |
| Restart/quit leaving zombie processes | Availability/Integrity | Restart spawns `current_exe()` then exits via the normal app-exit path (no forced `process::exit`); child is detached from parent lifecycle by design (survives parent exit on both platforms). |

## Sources

### Primary (HIGH confidence — source-verified this session)
- [crates.io sparse index + static.crates.io] egui-0.30.0, egui-winit-0.30.0, epaint-0.30.0 (downloaded + inspected): `State::new/on_window_event/take_egui_input/handle_platform_output` signatures (lib.rs:113/232/263/818), winit 0.30.5 dep, `Context::run/tessellate/set_fonts` (context.rs:802/1753/1953/2521), `FontData { index: u32 }` (fonts.rs:119), `Vertex { pos, uv, color }`/`Mesh`/`ClippedPrimitive`/`TexturesDelta` (mesh.rs:12-48, lib.rs:126, textures.rs:277), ab_glyph >=0.2.11
- [crates.io sparse index + static.crates.io] fuzzy-matcher-0.3.7: `SkimMatcherV2::{fuzzy_match, fuzzy_indices, smart_case, ignore_case, element_limit}` (skim.rs:65/602-662); top-level fns deprecated since 0.3.5
- [crates.io sparse index + static.crates.io] pollster-1.0.1: `block_on` (lib.rs:70)
- [local registry src] winit-0.30.13: `available_monitors()` (event_loop.rs:398), `MonitorHandle::{position,size,scale_factor}` (monitor.rs:118-154), **absence of `ActiveEventLoop::cursor_position`** (whole-src grep)
- [local registry src] softbuffer-0.4.8 `src/backends/cg.rs:328`: `CGImageAlphaInfo::NoneSkipFirst` (macOS alpha dropped)
- [local registry src] keyboard-types-0.7.0 `src/code.rs:161/532/758`: `Code::Space` + FromStr `"Space"`
- [local registry src] objc2-app-kit-0.3.2 `src/generated/NSEvent.rs:1142`: `mouseLocation() -> NSPoint`
- [project source] mybox-core: window.rs (WindowSpec/WindowRequest/on_draw/floating profile/focus), app.rs (window_event routing, user_event, AppEvent enum, create_window), module.rs, context.rs, config.rs, hotkey.rs, event.rs (FrameworkEvent::AppExit), renderer/mod.rs (premul_rgba_to_u32), lib.rs re-exports; capture/src/lib.rs (start_capture + injectable pattern); workspace Cargo.toml/Cargo.lock (pinned versions)

### Secondary (MEDIUM confidence)
- [cargo search] egui latest 0.36.1 / egui-winit 0.36.1 (also winit 0.30.13-based — ecosystem-wide winit 0.30 alignment confirmed)
- [cargo search + cache] egui_skia 0.4.0 (abandoned), egui_software_backend 0.0.3 (egui 0.34 pin)

### Tertiary (LOW confidence — flagged)
- slopcheck [SUS] on xcap: name-similarity heuristic vs "clap" — judged false positive (already Phase 2-approved, in Cargo.lock); no action beyond documentation.
- A1 (TTC face indices), A2 (layer rounding behavior), A3 (coordinate conversion) — runtime-verifiable only; each has a fallback documented in Assumptions Log.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every version verified against sparse index/downloaded sources; slopcheck [OK] on all new packages.
- Architecture: HIGH — all integration points read directly from current core source (post-Phase-2 state); egui-winit API signatures source-verified.
- Pitfalls: HIGH — grounded in Phase 2's documented bug history (focus, re-entrancy, redraw, premultiplied alpha) plus new source-verified facts (softbuffer alpha, no winit cursor API, deprecated fuzzy fns).

**Research date:** 2026-08-14
**Valid until:** 2026-09-14 (stable ecosystem; egui 0.30.0 is frozen, workspace pins are static)

---

*Phase: 03-命令面板*
*Next step: /gsd-plan-phase 3 — the planner consumes this research for 03-01 (命令面板窗口 + 命令注册系统) and 03-02 (模糊搜索 + 键盘导航 + 命令执行).*
