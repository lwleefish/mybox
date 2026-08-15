---
phase: 03-命令面板
plan: 06
subsystem: ui
tags: [egui, winit, hover, click, execute, coordinate-space, item-spacing, palette, gap-closure]

requires:
  - phase: 03-02
    provides: PaletteSession 状态机 / set_executing 防重入守卫 / execute 生命周期
  - phase: 03-04
    provides: palette_checks 探针脚手架 / 帧缓冲读取与校准纪律
  - phase: 03-05
    provides: 修订计数触发器 / resize_framebuffer / position_stable_on_filter 探针模式
provides:
  - draw_command_row 重写：content-ui painter + Sense::click + clicked→execute（GAP-4/GAP-5 根因消除）
  - 卡片级精确打包：item_spacing.y 归零 + 输入区确定性光标（UI-SPEC 几何表逐像素吻合）
  - headless 交互单测 row_interact_hovers_and_clicks_execute（egui RawInput 驱动）
  - E2E 探针 hover_click_alignment（真实窗口合成指针事件全链路断言）
affects: [03-07 Ctrl+P/N 导航, 03-08 IME 中文输入, UAT 测试 7/8 复验, Phase 4 跨平台]

tech-stack:
  added: []
  patterns:
    - "ScrollArea 内容 ui 自带 painter：行交互与绘制必须同属内容坐标空间（外层 CentralPanel painter 仅在偏移 0 时重合）"
    - "卡片级精确打包：item_spacing.y 归零 + allocate_rect 显式预留 + new_child 放置不推进光标的 widget——几何表逐像素吻合、幻影滚动零空间"
    - "合成指针事件注入：CursorMoved/MouseInput 可外部构造（KeyEvent 不可），经真实 on_event_win 闭包覆盖 egui-winit→命中测试→clicked→execute 全链路"

key-files:
  created: []
  modified:
    - crates/modules/palette/src/ui.rs
    - crates/modules/palette/src/lib.rs
    - crates/modules/palette/src/bin/palette_checks.rs
    - crates/modules/palette/tests/integration.rs

key-decisions:
  - "行交互用 ui.interact(Sense::click) + make_persistent_id(('palette-row', cmd.id))（T-03-13 稳定 id；interact 不推进光标，显式 advance_cursor_after_rect 保持 48px 精确打包）"
  - "点击执行直接复用 execute::execute（set_executing 防重入守卫拒绝 Executing/Empty/Error 态点击；headless proxy 未注入时跳过，与 on_palette_key Enter 臂同纪律）"
  - "输入区光标改确定性：卡片级 item_spacing.y=0 + allocate_rect 预留 48px + TextEdit 放入 new_child（不推进父光标）——消除 TextEdit 固有高度（~37px）造成的打包漂移"

patterns-established:
  - "hover/click 命中测试滞后一帧（egui 0.30 hit-test 使用 prev_pass.widgets）——headless 测试第 1 帧注册 widget + 建立指针位置，第 2 帧断言 hover/click"
  - "行带像素断言：hover 填充精确色 #2E2E2E 计数（带内 ≥100、行上带 ==0）+ 带内非 chrome 文字像素 >0——hover 高亮与文字同矩形"
  - "探针实测值打印纪律：断言失败消息携带三项实测值与 scale，桌面会话校准（03-04 纪律延续）"

requirements-completed: [PAL-04]

duration: 22 min
completed: 2026-08-15
---

# Phase 3 Plan 6: GAP-4/5 行交互重写 Summary

**命令项行交互全面重写：hover 高亮与文字精确同矩形（content-ui painter + 卡片级精确打包消除 GAP-4）、点击命令项经 execute 执行命令（Sense::click + clicked 接线消除 GAP-5），headless 单测 + 真实窗口 E2E 探针双层锁定，桌面会话 8/8 通过**

## Performance

- **Duration:** 22 min
- **Started:** 2026-08-15T07:58:29Z
- **Completed:** 2026-08-15T08:20:02Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- **GAP-4 根因类消除**：行矩形、hover/selected 高亮、文字全部统一到 ScrollArea 内容 ui 的 painter（同一坐标系，偏移恒为 0 时也不再依赖巧合）；`item_spacing.y` 归零后 5 行内容 240px == 视口 240px，幻影滚动空间归零；行内布局修正——name 在 row.top+8、desc 在 name 底 +4px，内容总高 45.8 ≤ 48（旧布局 name +20、desc 底部 ≈+57.8 压入下一行）
- **GAP-5 根因消除**：行交互从 `Sense::hover()`（无任何 clicked 分支）改为 `ui.interact(Sense::click)` + 稳定 per-command widget id（T-03-13），`resp.clicked()` → `execute::execute`（与 Enter 同语义，受 `set_executing` 防重入守卫保护；headless proxy 未注入时跳过）
- **卡片级精确打包**（探针校准中发现的相邻缺陷一并修复）：`ui.put` 的 TextEdit 固有高度推进（~37px）使列表从 y=60 开始贴着输入框、Executing 态行与输入框重叠——现在卡片级 item_spacing 归零 + `allocate_rect` 预留 48px 输入行 + TextEdit 放入 `new_child`（不推进父光标），五个状态的几何逐像素吻合 UI-SPEC 表（Idle 68..308、Executing 100..340、Empty/Error 68..132）
- **E2E 探针 `hover_click_alignment`**：真实窗口 + 真实 on_event_win 闭包注入合成 CursorMoved/MouseInput，行带实测 `hover_px_in_band=94049 / hover_px_above_band=0 / text_px_in_band=16549`（@2x），点击经完整链路进入 Executing、gated runner 释放后真实 finalize hop 销毁窗口——桌面会话 8/8 通过

## Task Commits

Each task was committed atomically:

1. **Task 1: ui.rs 行交互重写 + lib.rs 接线** - `5c01a03` (feat)
2. **Task 2: 编译健康验证** - 无独立提交（接线已并入 Task 1，见偏差 1）
3. **Task 3 偏差修复: 卡片级精确打包** - `3c7f55d` (fix)
4. **Task 3: E2E 探针 hover_click_alignment + 集成接线** - `f4df754` (feat)

**Plan metadata:** 见最终 docs commit

## Files Created/Modified

- `crates/modules/palette/src/ui.rs` - `draw` 签名扩展（windows/ui_proxy）+ 卡片级精确打包（item_spacing 归零、allocate_rect 输入行预留、TextEdit new_child）+ `draw_command_list` 重写（content-ui painter、0.5 opacity Executing 降暗）+ `draw_command_row` 重写（Sense::click + persistent id + clicked→execute + 48px 行内布局）+ 2 个新单测
- `crates/modules/palette/src/lib.rs` - 帧循环 `ui::draw(ctx, &session, &windows, &ui_proxy)` 接线（点击执行链路）
- `crates/modules/palette/src/bin/palette_checks.rs` - `check_hover_click_alignment` 探针（四阶段：基线/悬停测量/按压/释放执行 + gated runner 收尾）+ main() 分发/usage
- `crates/modules/palette/tests/integration.rs` - `palette_hover_click_alignment` `#[ignore]` 测试接线（PAL-04/GAP-4/GAP-5 回归）

## Verification Evidence

- `cargo nextest run -p mybox-palette ui` — 6/6 PASS（含 2 个新单测，零 warning）
- `cargo nextest run -p mybox-palette` — 59/59 PASS（8 skipped）
- `cargo nextest run --workspace` — 220/220 PASS（16 skipped）
- `cargo check --workspace` — exit 0，零 warning
- 桌面会话 `cargo test -p mybox-palette --test integration -- --ignored` — **8/8 PASS**，其中 `hover_click_alignment` 实测（scale=2）：`hover_px_in_band=94049`（≥100 ✓）、`hover_px_above_band=0`（行上带零高亮 ✓）、`text_px_in_band=16549`（文字与高亮同带 ✓）；点击 → Executing → gate 释放 → Destroy(created_id) + Hidden + runner 恰好一次

## Decisions Made

- 行交互改用 `ui.interact` + `make_persistent_id(("palette-row", cmd.id))`（T-03-13）；`interact` 不推进光标，显式 `advance_cursor_after_rect` 保持 48px 精确打包
- 点击执行复用 `execute::execute`（与 Enter 完全同语义同守卫，不新增路径）
- 输入区光标确定性方案：`new_child` 放置 TextEdit（不推进父光标）+ `allocate_rect` 显式预留——消除字体相关的固有高度漂移

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 2 的 lib.rs 接线并入 Task 1 提交**
- **Found during:** Task 1（ui.rs `draw` 签名扩展）
- **Issue:** 计划将 draw 签名扩展（Task 1）与唯一调用点 lib.rs 接线（Task 2）拆为两个任务——Task 1 的验收命令 `cargo nextest run -p mybox-palette ui` 需要整个 crate 编译，两者拆开会使两次提交之间工作树不可编译。
- **Fix:** Task 1 提交同时包含 ui.rs 与 lib.rs 接线（含计划要求的注释），Task 2 降级为纯验证（`cargo nextest run -p mybox-palette` 59/59 + `cargo check --workspace` 零 warning，全部通过）。
- **Files modified:** crates/modules/palette/src/ui.rs, crates/modules/palette/src/lib.rs
- **Verification:** Task 2 的两条验收命令均 exit 0；`ui::draw(ctx, &session, &windows, &ui_proxy)` 精确字符串存在。
- **Committed in:** 5c01a03

**2. [Rule 1 - Bug] headless 测试帧编排与 egui 0.30 交互滞后语义不符**
- **Found during:** Task 1（`row_interact_hovers_and_clicks_execute` 实现）
- **Issue:** 计划指定帧 1 断言 `resp.hovered()`——但 egui 0.30 的 hit-test 使用 `prev_pass.widgets`（交互滞后一帧），首帧注册的 widget 在当帧不可能 hover（实测 FAIL）。
- **Fix:** 帧 1 仅建立指针位置 + 断言未误触发（!clicked / Idle / count==0），帧 2（press+release）同时断言 `hovered()` 与 `clicked()`（该帧的 hit-test 使用帧 1 注册的 widget）。
- **Files modified:** crates/modules/palette/src/ui.rs（tests 模块）
- **Verification:** 单测通过（6/6），零 warning。
- **Committed in:** 5c01a03

**3. [Rule 1 - Bug] 生产输入区光标推进未覆盖绘制的 48px 输入框**
- **Found during:** Task 3（`hover_click_alignment` 探针校准——实测 hover 填充带在 y 60..106 而非计划假定的 68..116）
- **Issue:** `ui.put` 按 TextEdit 固有高度（~37px，字体相关）推进光标，列表从 y=60 开始贴着输入框底部（UI-SPEC 要求 8px 间隙）；Executing 分支无任何推进，行直接与绘制输入框重叠。计划的探针行带（68..116）、真值 #4（n 行内容高度 == 视口高度）均以此为前提。
- **Fix:** 卡片级 `item_spacing.y = 0.0` + `allocate_rect(input_rect, hover)` 显式预留 48px + TextEdit 移入 `ui.new_child`（不推进父光标）。五个状态几何逐像素吻合 UI-SPEC 表：Idle/Filtering 输入 12..60 + 8 间隙 + 行 68..308（=320 窗口高）；Executing 行 100..340（=352）；Empty/Error 块 68..132（=144）。视口高度与内容高度精确相等，幻影滚动空间归零。
- **Files modified:** crates/modules/palette/src/ui.rs
- **Verification:** 桌面探针实测 hover 填充精确落在 68..116 行带（`hover_px_above_band=0`）；8/8 集成测试通过；全部单测绿。
- **Committed in:** 3c7f55d

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bugs)
**Impact on plan:** 偏差 1/2 为计划描述的编排修正（任务边界、帧编排），行为与验收目标不变；偏差 3 修复了探针前提所依赖的生产布局缺陷（UI-SPEC 几何表精确吻合），是 must_have 真值成立的必要条件。无范围蔓延。

## Issues Encountered

- 探针首次实跑失败（hover 填充出现在行带上带）——经帧缓冲实测校准定位为生产输入区光标推进缺陷（偏差 3），修复后实测值全部达标。无其他问题。

## Known Stubs

None — 所有修改文件均为完整实现，无占位符/TODO/空数据流。

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- GAP-4/GAP-5 已关闭：`hover_click_alignment` 探针可在桌面会话重复运行（`cargo test -p mybox-palette --test integration -- --ignored`，8/8）；人工最终复验见 UAT 测试 7（hover 高亮与文字精确重叠）/测试 8（点击执行命令）
- 03-07（GAP-6 Ctrl+P/N 导航）无阻塞——本计划未触碰 on_palette_key 与 Modifiers 处理路径；注意 `draw` 签名已扩展，03-07 如扩展键盘路由需经 `ui_proxy` 传递
- 03-08（GAP-7 中文输入）无阻塞——TextEdit 迁移到 `new_child` 后焦点/IME 行为经 glyph_shape 探针（8/8 含 03-04 探针）确认无回归

---

*Phase: 03-命令面板*
*Completed: 2026-08-15*

## Self-Check: PASSED

- [x] 4 个修改文件均存在磁盘（ui.rs / lib.rs / palette_checks.rs / integration.rs）
- [x] 4 个任务提交均存在 git 历史：`5c01a03`、`3c7f55d`、`f4df754`（Task 2 无独立提交，见偏差 1）
- [x] 全部 acceptance_criteria 验证通过（源码断言 + 测试命令 + 桌面会话 8/8）
- [x] `cargo check --workspace` 零 warning
