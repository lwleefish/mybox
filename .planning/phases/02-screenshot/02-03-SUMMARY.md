---
phase: 02-screenshot
plan: 03
subsystem: screenshot
tags: [annotation, toolbar, undo, ctrl-z, tiny-skia, retained-list, no-modes, immediate-mode, ab-glyph]

# Dependency graph
requires:
  - phase: 02-screenshot
    plan: 02
    provides: overlay per-monitor windows + immediate-mode compositing, selection state machine (Phase/SelectionRect/drag_anchor/8 handles), text.rs draw_text (Arial.ttf system font)
provides:
  - annotate.rs: Annotation::draw (rect/arrow/pen/text) + AnnotationList (push/undo/is_empty/iter) — retained model + drawing (CAP-06) + undo stack (CAP-07)
  - toolbar.rs: ToolAction/ToolbarButton/layout_buttons (7 buttons)/hit_test/draw_toolbar — unified no-modes toolbar drawn with tiny-skia (D-03, no egui)
  - session.rs: current_tool + AnnotationList-backed annotations + pending_annotation + tool_action/undo/push_annotation/on_annotation_* + ctrl_down tracking
  - overlay.rs: toolbar hit-test + tool-routed annotation input + Ctrl+Z in on_event; on_draw renders pending + annotations + toolbar
affects: 02-04 (clipboard confirm + Cancel wiring + manual checklist), Phase 3 (command palette), Phase 4 (Windows port: font discovery + window level)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "retained Vec-backed AnnotationList + undo pop + full redraw every frame (immediate-mode — never bake annotations into pixels, RESEARCH Anti-Pattern, T-2-10)"
    - "annotation orange 0xFF6000 with 3px round-cap Stroke; arrowhead as a filled triangle (tip at b, base perpendicular to b-a, ~10px)"
    - "Ctrl+Z via ModifiersChanged state stored in SessionState.ctrl_down (on_event closures are Fn not FnMut, so no local mutable) + logical_key == \"z\""
    - "pending_annotation holds the in-progress rect/arrow/pen drag; on release it is committed into annotations (Text is placed immediately — A6 no editing)"
    - "toolbar drawn with tiny-skia fill_rect/stroke_path + text glyphs, resolved by hit-testing stored Rect buttons (no egui — RESEARCH)"
key-files:
  created:
    - crates/modules/capture/src/annotate.rs
    - crates/modules/capture/src/toolbar.rs
  modified:
    - crates/modules/capture/src/session.rs
    - crates/modules/capture/src/text.rs
    - crates/modules/capture/src/overlay.rs
    - crates/modules/capture/src/lib.rs

key-decisions:
  - "Toolbar is 7 buttons (Confirm/Cancel/Undo/Rect/Arrow/Pen/Text) in a horizontal row anchored 6px below the selection bottom-left, clamped left at the right screen edge"
  - "Confirm/Cancel are logged no-ops this plan (clipboard + teardown land in 02-04 per plan Task 2); Undo pops the last annotation"
  - "The text tool places a fixed ASCII \"Text\" annotation (DEFAULT_TEXT_ANNOTATION) — ASCII guarantees a glyph in the system Arial.ttf, and A6 defers text editing"
  - "MacOS also accepts Cmd+Z: ctrl_down tracks ModifiersState.control_key() || super_key()"
  - "Toolbar + annotations render only on the monitor that owns the selection (same discipline as 02-02's selection chrome)"

patterns-established:
  - "Tool-driven input routing in on_event: toolbar hit-test → handle hit-test → else (Select ⇒ new selection | tool ⇒ on_annotation_start)"
  - "on_mouse_move updates the pending annotation in parallel with the selection/handle drag (D-03: handles and tools coexist)"
  - "draw_text gained a color parameter (WxH label passes white; text annotation passes orange) — generalizes the old blend_white to blend_color"

requirements-completed: [CAP-06, CAP-07]

# Metrics
duration: single-session (~30 min)
completed: 2026-08-13
---

# Phase 2 Plan 3: 标注工具（矩形/箭头/画笔/文字）+ 统一工具栏 + Ctrl+Z 撤销 Summary

**选区上绘制四类标注（标注橙 0xFF6000、圆角线帽），拖拽结束后选区旁出现统一 no-modes 工具栏，点选切换工具，Ctrl+Z（macOS 亦支持 Cmd+Z）连续撤销到原始图像——CAP-06/07 + D-03**

## Performance

- **Duration:** single-session (~30 min)
- **Started:** 2026-08-13（续 02-02 后同一会话）
- **Completed:** 2026-08-13T13:24:00Z
- **Tasks:** 2
- **Files modified:** 6（2 created + 4 modified）

## Accomplishments

- `annotate.rs`：`Annotation::draw` 覆盖 Rect（归一化矩形描边）/Arrow（直线 + 实心三角箭头）/Pen（多段折线路径）/Text（ab_glyph 光栅化，64 字符上限 T-2-09）四变体，统一标注橙 + 3px 圆角线帽；空 Pen/零尺寸 Rect 安全返回不 panic（T-2-08）；`AnnotationList`（push/undo/is_empty/iter）撤销栈（pop 语义，CAP-07）
- `toolbar.rs`：`ToolAction`（Confirm/Cancel/Undo/Tool(t)）+ `ToolbarButton` + 纯函数 `layout_buttons`（7 按钮 32x32/间距 4px，锚定选区左下外侧，越界右移 clamp）/`hit_test`/`draw_toolbar`（深灰底白描边，当前工具橙底高亮；图标用 tiny-skia 路径 + text 单字符，无 egui）
- `session.rs`：`Tool` 增 derives；`annotations` 改为 `AnnotationList`；新增 `pending_annotation`、`ctrl_down`、`tool_action`/`undo`/`push_annotation`/`current_tool`/`on_annotation_start`/`on_annotation_update`/`on_annotation_finish`/`set_ctrl_down`/`ctrl_down`；`on_mouse_move` 同步更新进行中标注
- `text.rs`：`draw_text` 增加 `color: Color` 参数（`blend_white` → 通用 `blend_color` 预乘混合）；WxH 标签调用点同步传白
- `overlay.rs`：`on_event` 增工具栏命中（优先）→ 手柄命中 → 按 current_tool 分流（Select 重新选区 / 工具开始标注）；Ctrl+Z（ModifiersChanged CONTROL|SUPER + 'z' → undo）；`on_draw` 在选区之后追加 pending + 全部 annotations + 工具栏

## Task Commits

Each task was committed atomically:

1. **Task 1: Annotation 模型 + tiny-skia 绘制 + 撤销栈（CAP-06 绘制, CAP-07 模型）** - `c17ef10` (feat)
2. **Task 2: 工具栏 + 工具输入 + Ctrl+Z 撤销接线（CAP-06, CAP-07, D-03）** - `22b3f76` (feat)

**Plan metadata:** 本 SUMMARY 由 executor 独立提交（orchestrator 负责 STATE.md/ROADMAP.md/REQUIREMENTS.md 写回，见 working-tree 约定）。

## Files Created/Modified

- `crates/modules/capture/src/annotate.rs` - `Annotation::draw`（rect/arrow/pen/text）+ 箭头三角几何 + `annotation_color`/`annotation_paint`/`annotation_stroke` + `AnnotationList` + 9 个单测（像素断言 + undo 语义）
- `crates/modules/capture/src/toolbar.rs` - `ToolAction`/`ToolbarButton`/`layout_buttons`（7 按钮）/`hit_test`/`draw_toolbar`/`draw_button_icon`（路径图标）+ 5 个单测
- `crates/modules/capture/src/session.rs` - `Tool`/`Annotation` derives + `AnnotationList` 字段 + `pending_annotation`/`ctrl_down` + 7 个新方法 + `update_pending_annotation` + `DEFAULT_TEXT_ANNOTATION`/`DEFAULT_TEXT_SIZE` + 4 个新单测
- `crates/modules/capture/src/text.rs` - `draw_text` 增 `color` 参数 + `blend_color`（预乘 src-over）
- `crates/modules/capture/src/overlay.rs` - `handle_overlay_event` 增 `screen_w` 参数 + 工具栏/工具/Ctrl+Z 路由 + `draw_overlay` 渲染标注层与工具栏 + Color import
- `crates/modules/capture/src/lib.rs` - 注册 `annotate`/`toolbar` 模块

## Decisions Made

- **标注视觉：** 标注橙 `0xFF6000`、3px 圆角线帽；箭头头为实心三角（尖端在终点 b、底边垂直 b-a、长 ~10px）
- **工具栏布局：** 7 按钮横向排列，锚定选区左下外侧 6px，右越界时向左 clamp；当前工具按钮橙底高亮
- **文字工具（A6）：** 点击放置固定 `"Text"` 标注（无编辑）——ASCII 保证系统 Arial.ttf 有字形；文本编辑留待后续计划
- **Ctrl+Z 检测：** `ModifiersChanged` 状态存入 `SessionState.ctrl_down`（on_event 闭包是 `Fn` 非 `FnMut`，无法持有局部可变量）；macOS 同时接受 Cmd（`control_key() || super_key()`）
- **进行中标注：** `pending_annotation` 保存绘制中的 rect/arrow/pen，释放时提交进 `annotations`；Text 直接 push（无 pending）

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] tiny-skia 0.12 无 `stroke_rect`（计划接口片断误差）**
- **Found during:** Task 1（annotate.rs Rect 绘制）
- **Issue:** 计划写 `pm.stroke_rect(r, &paint, &stroke, …)`，但 tiny-skia 0.12 无此方法（02-02 已记录同一边界）。
- **Fix:** 改用 `PathBuilder::from_rect(rect)` + `pm.stroke_path(...)`（与 02-02 的选区边框实现一致）。
- **Files modified:** crates/modules/capture/src/annotate.rs
- **Committed in:** c17ef10

**2. [Rule 3 - Blocking] `Rect::from_ltrb` 返回 `Option<Rect>`**
- **Found during:** Task 1（annotate.rs Rect 归一化）
- **Issue:** 计划写 `tiny_skia::Rect::from_ltrb(...)` 直接当 `Rect` 用，实际返回 `Option<Rect>`（零尺寸返回 None）。
- **Fix:** `if let Some(rect) = Rect::from_ltrb(...)` —— 顺带实现 T-2-08 零尺寸 guard。
- **Files modified:** crates/modules/capture/src/annotate.rs
- **Committed in:** c17ef10

**3. [Rule 1 - Bug] `Paint<'a>` 带生命周期，helper 返回类型需显式标注**
- **Found during:** Task 1（annotate.rs `annotation_paint` helper）
- **Issue:** `fn annotation_paint() -> Paint` 报 E0106（`Paint` 在返回位需生命周期）。
- **Fix:** 改为 `-> Paint<'static>`（`Paint::default()` 即 `'static`）。
- **Files modified:** crates/modules/capture/src/annotate.rs
- **Committed in:** c17ef10

**4. [Rule 1 - Bug] 测试中 `f32::from(i32)` 不可用**
- **Found during:** Task 2（session.rs `undo_to_empty_equals_original_image` 测试）
- **Issue:** `f32::from(i)`（i 为 i32）无 `From<i32>` 实现。
- **Fix:** 改用 `i as f32`。
- **Files modified:** crates/modules/capture/src/session.rs
- **Committed in:** 22b3f76

---

**Total deviations:** 4 auto-fixed（2 个 API 形状误差 + 2 个编译正确性）
**Impact on plan:** 全部为编译/API 正确性必需的修正；无范围蔓延，依赖清单保持在 02-01 已锁定的 xcap/arboard/ab_glyph 内。

**Note:** 计划 Task 1 写「draw_text 保持签名兼容、加 color 参数（默认橙色）」——Rust 无默认参数，故 `draw_text` 显式增加 `color: Color`，WxH 标签调用点传白、标注传橙（记录于 Decisions，未视为偏差）。

## Known Stubs

- `crates/modules/capture/src/session.rs` — `DEFAULT_TEXT_ANNOTATION = "Text"`：文字工具点击放置固定 "Text" 标注（A6：MVP 无文本编辑，点击一次放置）。文字输入 UI 留待后续计划（CAP-06 标注绘制能力已达标）。
- `crates/modules/capture/src/session.rs` — `tool_action` 的 Confirm/Cancel 仅记录日志：剪贴板复制 + 覆盖窗销毁由 02-04 接线（计划 Task 2 明确 defer，非本计划范围）。

## Issues Encountered

- **`Paint<'a>` 生命周期：** helper 函数返回 `Paint` 需显式 `'static`（`Paint` 泛型带生命周期，`Default` 实现于 `Paint<'_>`）。
- **`on_event` 闭包为 `Fn` 非 `FnMut`：** Ctrl+Z 的 `ctrl_down` 无法用闭包局部可变量跟踪，改存 `SessionState.ctrl_down`（与 `last_cursor` 同类先例）。

## User Setup Required

None - no external service configuration required.

macOS Screen Recording 权限为一次性系统授权（用户操作，非工具），由 02-01 的 `CGPreflightScreenCaptureAccess` 预检（CAP-08）在捕获前拦截；覆盖窗口层级的已知限制（AlwaysOnTop-only，A3）不阻塞本 plan，Phase 4 重评。

## Next Phase Readiness

- 02-03 端到端接线完成：选区完成后工具栏出现 → 点选矩形/箭头/画笔/文字 → 在选区上绘制 → 手柄仍可拖动调整（D-03）→ Ctrl+Z 逐步撤销到原始图像
- 02-04（剪贴板确认）可直接复用：`session.tool_action(Confirm)` 当前日志点（改为裁剪选区 + 标注烘焙 + arboard set_image + 销毁全部覆盖窗）；`session.cancel()` 的 `overlay_ids` 取出模式已备；标注列表 `annotations` 可用于确认时把标注合成进最终图像
- 威胁缓解落实：T-2-08（空 Pen/零尺寸 Rect 安全返回）、T-2-09（文字 64 字符上限）、T-2-10（retained 列表 + undo-pop + 全量重画，绝不烘焙像素）、T-2-11（Pen 每移动事件最多累积 1 点 + 批量 redraw 无轮询）
- 已知限制：文字工具放置固定 "Text"（A6，无编辑）；覆盖窗口 AlwaysOnTop-only（A3）

## Self-Check: PASSED

- Files verified: `crates/modules/capture/src/{annotate,toolbar,session,text,overlay,lib}.rs`
- Commits verified: `c17ef10`（Task 1）, `22b3f76`（Task 2）
- `cargo nextest run -p mybox-capture`: 46 passed, 0 failed (exit 0)
- `cargo nextest run -p mybox-core -p mybox-capture`: 118 passed, 4 skipped (exit 0)
- `cargo check --workspace`: exit 0, 无 warning

---
*Phase: 02-screenshot*
*Completed: 2026-08-13*
