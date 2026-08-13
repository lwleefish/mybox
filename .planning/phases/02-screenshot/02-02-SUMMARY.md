---
phase: 02-screenshot
plan: 02
subsystem: screenshot
tags: [overlay, per-monitor, region-selection, resize-handles, tiny-skia, ab_glyph, esc-cancel, premultiply, immediate-mode]

# Dependency graph
requires:
  - phase: 02-screenshot
    plan: 01
    provides: xcap capture backend (SessionState.shots + MonitorGeom), WindowSpec.on_draw/on_event + WindowRequest::Redraw render chain, capture-before-create ordering (Pitfall 1)
provides:
  - overlay.rs: per-monitor Overlay windows + immediate-mode compositing (capture blit + mask + selection border/handles/WxH) + on_event interaction routing
  - selection.rs: pure drag-select + 8-handle geometry (normalize/handle_rect/hit_test_handle/apply_handle_drag/drag_start/drag_update)
  - text.rs: ab_glyph text rasterization (WxH size label, reusable for the text tool)
  - session.rs: selection state machine (Idle/Selecting/Selected) + cancel() that drains overlay_ids for teardown
  - lib.rs: capture-complete → create_overlays wiring + core/window-created → window_created pairing
affects: 02-03 (annotation toolbar — retained Annotation list already declared), 02-04 (clipboard confirm + manual checklist), Phase 3 (command palette), Phase 4 (Windows port: font discovery + screenSaver window level)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "immediate-mode overlay compositing: every redraw re-blits the capture + re-derives the mask from SessionState (never accumulate pixels — RESEARCH Anti-Pattern)"
    - "premultiply_rgba8 (straight RGBA8 → premultiplied) feeding tiny_skia::Pixmap::from_vec (Pitfall 2)"
    - "selection state machine on the shared Arc<std::sync::Mutex<SessionState>>, driven from the main-thread on_event closure"
    - "drag_anchor stored separately so a normalized selection rect doesn't flip mid-drag"
    - "ESC teardown via overlay_ids drained by cancel() → WindowManagerHandle::destroy per id (idempotent, T-2-06)"
    - "ab_glyph FontVec::try_from_vec → FontArc::from for a runtime-loaded system font (FontArc::try_from_slice needs 'static bytes)"

key-files:
  created:
    - crates/modules/capture/src/overlay.rs
    - crates/modules/capture/src/selection.rs
    - crates/modules/capture/src/text.rs
  modified:
    - crates/modules/capture/src/session.rs
    - crates/modules/capture/src/lib.rs
    - crates/mybox-core/src/lib.rs

key-decisions:
  - "Font is the macOS system Arial.ttf (A4), loaded via FontVec::try_from_vec → FontArc::from; no bundled font asset exists, and FontArc::try_from_slice requires 'static bytes"
  - "The drag anchor is tracked in SessionState.drag_anchor because the stored selection rect is normalized (x0<=x1) and would otherwise lose the fixed corner on the second mouse-move"
  - "Hit-testing 8 handles uses last_cursor tracked on CursorMoved, since MouseInput carries no position"
  - "Redraw is batched over overlay_ids (one redraw request per input event), keeping ControlFlow::Wait (Pitfall 3, T-2-07)"

patterns-established:
  - "Pure selection geometry + headless unit tests (selection::tests) — same discipline as renderer::premul_rgba_to_u32"
  - "Overlay compositing factored into a testable composite_frame (blit + mask) + draw_selection_overlay (border/handles/WxH)"
  - "Every input → session transition → redraw_all_overlays; immediate-mode full repaint"

requirements-completed: [CAP-02, CAP-03, CAP-05]

# Metrics
duration: single-session (~45 min)
completed: 2026-08-13
---

# Phase 2 Plan 2: 覆盖窗口 + 区域选择 Summary

**每屏覆盖窗口显示捕获画面与遮罩、拖拽选择（实时边框 + WxH 标签）、8 手柄可调整、ESC 一键取消——Phase 2 第一条端到端用户能力（CAP-02/03/05, D-02/D-04）**

## Performance

- **Duration:** single-session (~45 min)
- **Started:** 2026-08-13（续 02-01 后同一会话）
- **Completed:** 2026-08-13T05:06:07Z
- **Tasks:** 3
- **Files modified:** 6（3 created + 3 modified）

## Accomplishments

- `overlay.rs`：`premultiply_rgba8`（straight→premultiplied，Pitfall 2）+ `create_overlays` 按每屏物理几何构建 `WindowKind::Overlay` 窗口，`on_draw` 合成「图像 blit → 遮罩（无选区全屏遮罩 / 有选区 4 块外侧遮罩）→ 选区边框 + 8 手柄 + WxH」，`on_event` 路由 CursorMoved/MouseInput/KeyboardInput(Escape)
- `selection.rs`：纯选区几何——`Handle`（8 变体）+ `normalize`/`handle_rect`/`hit_test_handle`/`apply_handle_drag`（最小 4px clamp）/`drag_start`/`drag_update`，全部无头单测
- `session.rs`：`Phase`（Idle/Selecting/Selected）状态机 + `on_mouse_down/move/up` + `drag_anchor`（防止拖拽翻转）+ `cancel()`（重置 Idle、清选区、取出并清空 `overlay_ids`，幂等 T-2-06）
- `text.rs`：`load_font()`（macOS Arial.ttf + `OnceLock` 缓存）+ `draw_text()`（ab_glyph `outline_glyph` 覆盖率 src-over 混合，绘制 WxH 标签）
- `lib.rs`：捕获完成路径接 `overlay::create_overlays`；订阅 `core/window-created` → `session.window_created` 配对框架窗口 id
- `mybox-core/src/lib.rs`：重导出 `winit`（模块 crate 需命名 winit 事件类型，FRMW-02 边界）

## Task Commits

Each task was committed atomically:

1. **Task 1: 覆盖窗口创建 + 画面/遮罩合成（CAP-02）** - `c58a043` (feat)
2. **Task 2: 选区状态机 + 8 手柄（纯逻辑, CAP-03, D-02）** - `06b1ca5` (feat)
3. **Task 3: 选区渲染 + WxH 标签 + ESC 取消（CAP-03, CAP-05, D-04）** - `49128b7` (feat)

**Plan metadata:** 本 SUMMARY 由 executor 独立提交（orchestrator 负责 STATE.md/ROADMAP.md 写回，见 working-tree 约定）。

## Files Created/Modified

- `crates/modules/capture/src/overlay.rs` - `premultiply_rgba8` + `create_overlays`（每屏一窗）+ `composite_frame`（blit+遮罩）+ `draw_selection_overlay`（边框/手柄/WxH）+ `handle_overlay_event`（拖拽/手柄/ESC）+ 6 个单测
- `crates/modules/capture/src/selection.rs` - `Handle` 枚举 + 6 个纯函数 + 10 个单测（命中/移动/clamp/归一化）
- `crates/modules/capture/src/text.rs` - `load_font`（Arial.ttf + OnceLock）+ `draw_text`（ab_glyph 覆盖率混合）+ 2 个单测
- `crates/modules/capture/src/session.rs` - `Phase` 枚举 + `SelectionRect` derives + `CaptureSession::Clone` + `window_created`/`on_mouse_down`/`on_mouse_move`/`on_mouse_up`/`set_active_handle`/`cancel` + 4 个单测
- `crates/modules/capture/src/lib.rs` - 注册 overlay/selection/text 模块；`start_capture` 增 `windows` 参数并接 `create_overlays`；订阅 `core/window-created`
- `crates/mybox-core/src/lib.rs` - `pub use winit;`（模块 crate 可命名 winit 事件类型）

## Decisions Made

- **字体加载：** 使用 macOS 系统 `Arial.ttf`（A4），经 `FontVec::try_from_vec` → `FontArc::from`（`FontArc::try_from_slice` 需 `'static` 字节，运行时 `fs::read` 不满足）；仓库无内置字体资源
- **拖拽锚点：** `drag_anchor` 单独存于 SessionState——存储的选区恒归一化（x0<=x1），否则第二次 mouse-move 会丢失固定角（计划的 `drag_update(sel, pos)` 签名不足以跨多步拖拽保锚）
- **手柄命中：** 用 `last_cursor`（CursorMoved 时记录）做鼠标按下命中测试——`MouseInput` 事件不带坐标
- **重绘策略：** 每次输入事件后对 `overlay_ids` 批量 `redraw`（T-2-07 无风暴、Pitfall 3 保持 `ControlFlow::Wait`）

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `winit` 重导出到 mybox-core**
- **Found during:** Task 1（on_event 闭包编写）
- **Issue:** 模块需构造 `WindowSpec.on_event`（`Box<dyn Fn(&winit::event::WindowEvent)…>`）并在闭包内匹配 `WindowEvent::CursorMoved` 等变体，但 mybox-core 未重导出 `winit`、mybox-capture 也未声明 winit 依赖，`winit::` 路径无法解析。
- **Fix:** 在 `mybox-core/src/lib.rs` 增加 `pub use winit;`（与 `tiny_skia`/`tray_icon` 重导出同理——模块通过 mybox-core 单一依赖访问框架 API，FRMW-02）。
- **Files modified:** crates/mybox-core/src/lib.rs
- **Verification:** `cargo check --workspace` 退出 0
- **Committed in:** c58a043

**2. [Rule 3 - Blocking] 内置字体缺失 → 直接使用系统 Arial.ttf**
- **Found during:** Task 3（text.rs `load_font`）
- **Issue:** 计划写 `include_bytes!("assets/DejaVuSans.ttf")`，但仓库无该资源，`include_bytes!` 会在编译期失败；且 `FontArc::try_from_slice` 要求 `&'static [u8]`。
- **Fix:** 直接 `std::fs::read("/System/Library/Fonts/Supplemental/Arial.ttf")`（A4，macOS 已验证存在）→ `FontVec::try_from_vec` → `FontArc::from`，`OnceLock` 缓存。
- **Files modified:** crates/modules/capture/src/text.rs
- **Verification:** `cargo nextest run -p mybox-capture text::tests` 2 passed
- **Committed in:** 49128b7

**3. [Rule 1 - Bug/Correctness] 拖拽锚点跨多步丢失**
- **Found during:** Task 2（selection/session 设计）
- **Issue:** 计划的 `drag_update(sel, pos)` 若在每次 mouse-move 后归一化存储选区，第二次 mouse-move 时 `sel.x0/y0` 已被翻转，锚点丢失——从 (100,100) 拖到 (50,50) 再拖到 (40,40) 会得到 (40,50) 而非 (40,100)。
- **Fix:** SessionState 增加 `drag_anchor: Option<Point>`，`drag_update` 的入参始终是锚点矩形；归一化仅作用于显示。
- **Files modified:** crates/modules/capture/src/session.rs
- **Verification:** `cargo nextest run -p mybox-capture selection:: session::` 全绿
- **Committed in:** 06b1ca5

**4. [Rule 3 - Blocking] tiny-skia/ab_glyph API 形状与计划接口片断不符**
- **Found during:** Task 1/3（overlay.rs/text.rs 编写）
- **Issue:** 计划的 `Pixmap::from_vec(bytes, w, h)`（实际签名 `from_vec(Vec<u8>, IntSize)`）、`stroke_rect`（tiny-skia 0.12 无此方法，需 `PathBuilder::from_rect` + `stroke_path`）、`draw_pixmap(…, &Paint::default(), …)`（实际为 `&PixmapPaint`）以及 ab_glyph `scaled_glyph`（`outline_glyph` 需带 `position` 的 `Glyph`）均与真实 API 有出入。
- **Fix:** 按缓存源码核对后的真实 API 实现（`IntSize::from_wh`、`PathBuilder::from_rect` + `stroke_path`、`PixmapPaint::default`、`glyph.position = point(pen_x, 0.0)` 逐字形推进）。
- **Files modified:** crates/modules/capture/src/overlay.rs, text.rs
- **Verification:** `cargo nextest run -p mybox-capture` 30 passed
- **Committed in:** c58a043 / 49128b7

---

**Total deviations:** 4 auto-fixed（3 blocking API/boundary + 1 drag-anchor correctness）
**Impact on plan:** 全部为编译正确性与拖拽行为正确性必需的修正；无范围蔓延，依赖清单保持在 02-01 已锁定的 xcap/arboard/ab_glyph 内。

## Issues Encountered

- **on_event 闭包需同时捕获 session 与 windows 的 Clone：** `WindowManagerHandle` 非 `Clone`，故 `create_overlays` 接收 `&Arc<WindowManagerHandle>` 并在每窗闭包内 `Arc::clone`；`CaptureSession` 增 `#[derive(Clone)]` 共享同一 `Arc<Mutex<…>>`。
- **临时 Arc 借用（E0716）：** session 测试里 `session.state().lock()` 产生临时 Arc 被 guard 借用，改为先绑定 `let state_arc = session.state();`。

## User Setup Required

None - no external service configuration required.

macOS Screen Recording 权限为一次性系统授权（用户操作，非工具），由 02-01 的 `CGPreflightScreenCaptureAccess` 预检（CAP-08）在捕获前拦截；覆盖窗口层级的已知限制（AlwaysOnTop-only，A3）不阻塞本 plan，Phase 4 重评。

## Next Phase Readiness

- 覆盖窗口端到端已接线：热键/托盘 → 捕获 → 每屏覆盖窗（画面+遮罩）→ 拖拽选区（边框+WxH）→ 8 手柄调整 → ESC 一键取消
- 02-03（标注）可直接消费 `SessionState.annotations`（`Annotation` 枚举已声明）与 `text::draw_text`/`draw_selection_overlay` 的合成顺序（在遮罩+选区之后追加标注层）
- 02-04（剪贴板确认）可复用 `session.cancel()` 的 `overlay_ids` 取出模式实现「确认后销毁全部覆盖窗」；`premultiply_rgba8`/裁剪逻辑沿用
- 威胁缓解落实：T-2-01（cancel 清空 selection 与 overlay_ids，shots 清空留待 02-04）、T-2-05（draw 空 shots 守卫 + fill_rect_safe 跳零尺寸 + blend 越界忽略）、T-2-06（cancel 幂等）、T-2-07（每输入一次批量 redraw）
- 已知限制：覆盖窗口 AlwaysOnTop-only（非 screenSaver 层级，A3），对全屏应用/菜单栏可能不覆盖——MVP 接受，Phase 4 重评

## Self-Check: PASSED

- Files verified: `crates/modules/capture/src/{overlay,selection,text,session,lib}.rs`, `crates/mybox-core/src/lib.rs`
- Commits verified: `c58a043`（Task 1）, `06b1ca5`（Task 2）, `49128b7`（Task 3）
- `cargo nextest run -p mybox-capture`: 30 passed, 0 failed (exit 0)
- `cargo nextest run -p mybox-core -p mybox-capture`: 102 passed, 4 skipped (exit 0)
- `cargo check --workspace`: exit 0, 无 warning

---
*Phase: 02-screenshot*
*Completed: 2026-08-13*
