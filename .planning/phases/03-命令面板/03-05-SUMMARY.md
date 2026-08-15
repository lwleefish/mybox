---
phase: 03-命令面板
plan: 05
subsystem: ui
tags: [winit, egui, tiny-skia, geometry-sync, revision-counter, palette, gap-closure]

requires:
  - phase: 03-02
    provides: palette session 状态机 / set_input / set_executing / finalize 语义
  - phase: 03-04
    provides: on_event_win 帧循环、帧缓冲 fill、palette_checks 探针脚手架
provides:
  - geometry_revision 修订计数（确定性高度同步触发器，WR-01 修复）
  - resize_framebuffer 帧缓冲运行时伸缩（WR-02 修复）
  - sync_window_geometry 只 request_inner_size 不重定位（GAP-3 根因消除）
  - E2E 探针 position_stable_on_filter（真实窗口三阶段位置零漂移断言）
affects: [03-06 hover 点击交互, 03-07 Ctrl+P/N 导航, 03-08 IME 中文输入, 04-02 错误处理打磨, Phase 4 多显示器]

tech-stack:
  added: []
  patterns:
    - "修订计数触发器：帧外状态转变（KeyboardInput / UiThreadProxy hop）用单调递增计数捕获，帧循环比对 last_seen"
    - "窗口几何同步纪律：request_inner_size 改尺寸、绝不 set_outer_position 改位置（位置由 summon 决定并保持）"
    - "帧缓冲随窗口物理尺寸伸缩：每次高度同步后 resize_framebuffer，同尺寸零分配"

key-files:
  created: []
  modified:
    - crates/modules/palette/src/session.rs
    - crates/modules/palette/src/lib.rs
    - crates/modules/palette/src/bin/palette_checks.rs
    - crates/modules/palette/tests/integration.rs

key-decisions:
  - "高度同步触发从帧内 prev/next 快照比较改为 geometry_revision 修订计数（WR-01：帧外转变确定性捕获）"
  - "sync_window_geometry 只 request_inner_size、绝不 set_outer_position（GAP-3 根因：收缩后重居中使顶边下移）"
  - "帧缓冲随窗口伸缩 resize_framebuffer（WR-02：增高后新区域可绘制；同尺寸零分配、失败保留旧缓冲）"

patterns-established:
  - "geometry_revision 修订计数：summon/set_input/set_executing-成功/finalize-Err 四类转变递增，帧循环比对 last_revision 触发同步"
  - "帧缓冲覆盖契约：同步后 Pixmap 尺寸 == 窗口物理尺寸（宽 = PANEL_WIDTH·scale，高 = window_height·scale）"
  - "探针窗口服务器断言：outer_position/inner_size 直接读 winit Window，位置断言精确相等"

requirements-completed: [PAL-03, PAL-04]

duration: 12 min
completed: 2026-08-15
---

# Phase 3 Plan 5: GAP-3 面板位置漂移修复 Summary

**几何子系统重做：高度同步只 request_inner_size 不重居中（GAP-3）、修订计数触发器（WR-01）、帧缓冲随窗伸缩（WR-02），E2E 探针 `position_stable_on_filter` 在真实窗口断言三阶段位置零漂移**

## Performance

- **Duration:** 12 min
- **Started:** 2026-08-15T07:40:14Z
- **Completed:** 2026-08-15T07:52:01Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- **GAP-3 根因消除**：`sync_window_geometry` 删除重居中块（`set_outer_position`），高度变化只 `request_inner_size` —— 过滤收缩后顶边不再下移，窗口位置由 summon 时的 `summon_geometry` 决定并保持到销毁
- **WR-01 修复**：`SessionInner.geometry_revision: u64` 修订计数——summon/set_input/set_executing-成功/finalize-Err 四类几何相关转变递增；帧循环由 `geometry_revision() != last_revision` 触发同步，帧外转变（Enter 按键事件、finalize UiThreadProxy hop）确定性捕获（Executing +32px 状态行、Error 收缩 144 均生效）
- **WR-02 修复**：`session.resize_framebuffer(w, h)`——每次高度同步后帧缓冲与窗口物理尺寸一致，窗口增高后新区域可绘制；同尺寸调用保留 Pixmap 实例（零分配），分配失败 warn 并保留旧缓冲（绝不 panic）
- **E2E 探针 `position_stable_on_filter`**：真实窗口 + 真实 on_event_win 闭包驱动三阶段几何变化（过滤收缩 320→128、输入恢复 128→320、Executing 增高 320→352），断言窗口服务器可见的 outer_position 与召唤原点**精确相等**（GAP-3 直接回归）且帧缓冲尺寸覆盖窗口全尺寸（WR-02 回归），桌面会话实跑 7/7 通过

## Task Commits

Each task was committed atomically:

1. **Task 1: session 修订计数 + 帧缓冲伸缩** - `2235c5f` (feat)
2. **Task 2: sync_window_geometry 去重居中 + 修订计数触发** - `813e9ad` (feat)
3. **Task 3: E2E 探针 position_stable_on_filter + 集成测试接线** - `f481943` (feat)

**Plan metadata:** 见最终 docs commit

## Files Created/Modified

- `crates/modules/palette/src/session.rs` - `geometry_revision` 字段/访问器 + 四类转变递增（set_executing 仅成功分支、finalize 仅 Err 分支）+ `resize_framebuffer` 伸缩 + 3 个新单测
- `crates/modules/palette/src/lib.rs` - `sync_window_geometry` 去重居中 + `resize_framebuffer` 调用；帧循环触发器改修订计数（`last_revision` Mutex，初始化 0 首帧必同步一次被 last_height 门控去重）
- `crates/modules/palette/src/bin/palette_checks.rs` - `window_outer_position`/`window_inner_size` harness 辅助 + `check_position_stable_on_filter` 探针（四阶段 driver 状态机）+ main() 分发/usage 接线
- `crates/modules/palette/tests/integration.rs` - `palette_position_stable_on_filter` `#[ignore]` 测试接线

## Verification Evidence

- `cargo nextest run -p mybox-palette session` — 21/21 PASS（含 3 个新单测）
- `cargo nextest run -p mybox-palette` — 57/57 PASS（6 skipped）
- `cargo nextest run --workspace` — 218/218 PASS（15 skipped）
- `cargo check --workspace` — exit 0，无 warning
- 桌面会话 `cargo test -p mybox-palette --test integration -- --ignored` — **7/7 PASS**，其中 `position_stable_on_filter` 实测三阶段高度 128→320→352（@2x scale）outer_position 与召唤原点精确相等、帧缓冲 1200×{256,640,704} 全程覆盖

## Decisions Made

- 高度同步触发从帧内 prev/next 快照比较改为 `geometry_revision` 修订计数（WR-01 根因：帧外转变在下一帧快照时 prev==current、同步永不触发）
- `sync_window_geometry` 只 `request_inner_size`、绝不 `set_outer_position`（GAP-3 根因：收缩后重居中使顶边下移——"面板下降"）
- 帧缓冲随窗口高度同步伸缩（WR-02 根因：帧缓冲仅 summon 分配一次，增高后新区域无绘制）

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] 探针 stage 2/3 轮询入口补 inject（计划描述遗漏触发帧）**
- **Found during:** Task 3（`check_position_stable_on_filter` 实现）
- **Issue:** 计划对 stage 1 明确写 `h.inject(RedrawRequested)` 触发修订同步，但 stage 2/3 只写"轮询 inner_size 高度"——若严格照抄，过滤恢复与 Executing 增高的同步帧永远不会运行（harness 拦截真实窗口事件转交 driver，帧循环只由 inject 驱动），轮询 20 次必超时。
- **Fix:** 每个轮询 driver 重入先 `h.inject(RedrawRequested)`（运行帧循环→修订同步）再读 `window_inner_size` 比对目标高度，与 stage 1 的既定结构一致。
- **Files modified:** crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** 桌面会话实跑 `position_stable_on_filter` 通过（三阶段高度 256/640/704 物理 px 全部达成）
- **Committed in:** f481943（Task 3 提交）

---

**Total deviations:** 1 auto-fixed (1 bug — plan description omission)
**Impact on plan:** 修复为探针可运行的必要条件，无行为/范围变化。计划全部断言与覆盖声明保持原样。

## Issues Encountered

- STATE.md 计划计数器陈旧（03-03/03-04 gap-closure 执行后未推进，仍显示 "Plan: 1 of 8"）——本计划完成后补推进至 "Plan: 5 of 8"（实际完成数）。
- 无其他问题。

## Known Stubs

None — 所有修改文件均为完整实现，无占位符/TODO/空数据流。

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- GAP-3 已关闭：`position_stable_on_filter` 探针可在桌面会话重复运行（`cargo test -p mybox-palette --test integration -- --ignored`）；人工最终复验见 UAT 测试 6（唤出后位置固定、过滤改变命令项数量面板不位移）
- WR-01/WR-02 一并关闭（VERIFICATION.md Anti-Patterns 表中两行 warning 根因消除）；WR-03（创建失败路径 pending_close）仍 deferred Phase 4（03-VERIFICATION.md 已记录，不受本计划影响）
- 下一计划 03-06（GAP-4/5 行交互重写）无阻塞——本计划仅改帧循环触发与同步函数签名，未触碰 ui.rs 行绘制路径

---

*Phase: 03-命令面板*
*Completed: 2026-08-15*

## Self-Check: PASSED

- [x] 4 个修改文件均存在磁盘（session.rs / lib.rs / palette_checks.rs / integration.rs）
- [x] 3 个任务提交均存在 git 历史：`2235c5f`、`813e9ad`、`f481943`
- [x] 全部 acceptance_criteria 验证通过（源码断言 + 测试命令 + 桌面会话 7/7）
- [x] `cargo check --workspace` 零 warning
