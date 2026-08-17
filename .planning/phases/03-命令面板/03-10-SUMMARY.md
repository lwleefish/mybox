---
phase: 03-命令面板
plan: 10
subsystem: ui
tags: [keyword-filter, tier-highlight, winit, e2e-probe, hidden-sync, pixel-assert]

# Dependency graph
requires:
  - phase: 03-命令面板
    provides: 03-09 palette 会话核心 (session.rs 帧循环/输入管线/session.close/窗口管理)
provides:
  - "Match.keyword_hit：命中的拼音 keyword 字符串 + fuzzy_indices 字符位置数据载体（filter.rs）"
  - "draw_command_row keyword tag：description 行尾「 · {keyword}」，命中字符 #FF6000 渲染（ui.rs）"
  - "RedrawRequested 帧循环 Hidden 守卫 + window.set_visible(false) 同步隐藏早退（lib.rs）"
  - "E2E 探针 keyword_highlight + click_hide_before_capture 及 integration.rs 测试 11/12"
affects: [03-命令面板后续计划, HUMAN-UAT 复核, VERIFICATION 复核]

# Tech tracking
tech-stack:
  added: [无新增依赖 —— 全部使用现有 tiny-skia/egui/winit 栈]
  patterns:
    - "E2E 探针双帧注入：首帧渲染建立布局后注入第二帧再扫描（framebuffer resize 会重建空白 Pixmap）"
    - "帧内 band 像素扫描：按 logical 坐标带 (y 68..116) 乘 scale 换算 physical，统计精确 #FF6000 像素数"
    - "gated_runner 模式：AtomicUsize 计数器 + mpsc 通道延迟执行副作用，供断言时序收敛"

key-files:
  created: []
  modified:
    - crates/modules/palette/src/filter.rs
    - crates/modules/palette/src/ui.rs
    - crates/modules/palette/src/lib.rs
    - crates/modules/palette/src/bin/palette_checks.rs
    - crates/modules/palette/tests/integration.rs

key-decisions:
  - "keyword_hit 携带命中 keyword 字符串 + fuzzy_indices 字符位置（非仅索引）——tag 渲染与命中字符高亮由 UI 层独立消费"
  - "全部拼音 keyword 命中路径同机制渲染（jt/tuichu/peizhi/chongqi/rizhi），不止 capture.start——梯队索引单测在 filter.rs，E2E 只验证渲染层"
  - "click 探针采用 gated_runner 验证时序：点击帧内 window.is_visible()==Some(false) 且 runner 计数器==0（读屏前面板已消失）"
  - "click 探针在 stage 0 增加基线可见性断言 is_visible()==Some(true)，保证 stage 3 的 Some(false) 断言非空洞"

patterns-established:
  - "Pattern 1: keyword_hit 数据流 filter → UI（Match 携带命中信息，渲染层无重复计算）"
  - "Pattern 2: 帧内关闭同步隐藏——lib.rs Hidden 守卫在 RedrawRequested 帧循环里 set_visible(false)，Destroy 排出前已离屏"
  - "Pattern 3: E2E 像素探针——物理坐标 = logical × scale，按行带扫描精确颜色像素断言渲染正确性"

requirements-completed: [PAL-03, PAL-04]

# Metrics
duration: 33min
completed: 2026-08-17
---

# Phase 03 Plan 10: Keyword-Tier Highlight + Click-Sync-Hide Summary

**拼音 keyword 梯队命中在 filter/ui/session 三层打通：#FF6000 关键词高亮渲染（全部 5 个 keyword 同机制）与鼠标点击执行时窗口同步隐藏（读屏前已离屏），E2E 像素探针 keyword_highlight + click_hide_before_capture 在真实窗口验证通过**

## Performance

- **Duration:** 33 min（Tasks 1-4 从 14:04Z 到 14:37Z）
- **Started:** 2026-08-17T06:04:08Z
- **Completed:** 2026-08-17T06:37:18Z
- **Tasks:** 4
- **Files modified:** 5

## Accomplishments
- `Match.keyword_hit` 数据载体：filter.rs 的 keyword 梯队分支取最高分 keyword，存入命中的 keyword 字符串 + fuzzy_indices 字符位置
- keyword tag 渲染：ui.rs `draw_command_row` 在 description 行尾渲染「 · {keyword}」，命中字符以 ACCENT #FF6000 绘制
- 帧内同步隐藏：lib.rs 在 RedrawRequested 帧循环中增加 Hidden 守卫——面板关闭时 `set_visible(false)` 早退，Destroy 排出前已从屏幕消失（截图绝不含面板）
- E2E 探针 `keyword_highlight`（真实窗口双帧 + 行带像素扫描，"jt" 实测 19 ACCENT px、"tuichu" 实测 39 ACCENT px）+ `click_hide_before_capture`（gated_runner 时序收敛）——12/12 ignored 集成测试全绿

## Task Commits

Each task was committed atomically:

1. **Task 1: Match.keyword_hit data channel** - `9c40064` (feat)
2. **Task 2: Keyword tag rendered in description line** - `2c5f2a3` (feat)
3. **Task 3: Sync-hide window on in-frame close** - `d6bdd4a` (fix)
4. **Task 4: Keyword-highlight + click-hide-before-capture E2E probes** - `20f11a0` (test)

**Plan metadata:** `c09317a` (docs: plan revision — KeywordHit derive + fix auto-propagation claim)

## Files Created/Modified
- `crates/modules/palette/src/filter.rs` - `Match.keyword_hit` 字段：最高分拼音 keyword 字符串 + fuzzy_indices 字符位置
- `crates/modules/palette/src/ui.rs` - `draw_command_row` 渲染 keyword tag（「 · {keyword}」行尾，命中字符 #FF6000）
- `crates/modules/palette/src/lib.rs` - RedrawRequested 帧循环 Hidden 守卫 + `set_visible(false)` 同步隐藏早退
- `crates/modules/palette/src/bin/palette_checks.rs` - 探针 `check_keyword_highlight` + `check_click_hide_before_capture`、`accent_pixels_in_row_band` 辅助、main() dispatch 与 usage 更新
- `crates/modules/palette/tests/integration.rs` - ignored 测试 11/12（palette_keyword_highlight / palette_click_hide_before_capture），带 UAT 回归注释

## Decisions Made
- keyword_hit 携带「keyword 字符串 + 字符位置」而非仅命中索引——UI 层可独立渲染 tag 与逐字符高亮，filter 层无需了解渲染
- 全部拼音 keyword（jietu/tuichu/peizhi/chongqi/rizhi）同机制渲染：梯队索引断言放 filter.rs 单测，E2E 只验证渲染层两条代表性路径（jt→capture.start、tuichu→builtin.quit）
- click 探针用 gated_runner（AtomicUsize + mpsc）验证「面板先消失再读屏」时序：stage 3 断言 Hidden + `is_visible()==Some(false)` + counter==0 + 配对 Destroy
- click 探针 stage 0 增加基线断言 `is_visible()==Some(true)`——使 stage 3 的 Some(false) 断言非空洞

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `unused_assignments` warning on captured `scale` in click probe**
- **Found during:** Task 4 (check_click_hide_before_capture)
- **Issue:** click 探针的 `scale` 只在本 stage 0 内读取（CursorMoved 物理坐标换算），却被声明为闭包捕获的 `mut` 变量——编译器警告 "value captured by `scale` is never read"（对比 hover_click/keyword_highlight 探针在后续 stage 复用 scale，无此警告）
- **Fix:** 改为 stage 0 内部局部变量 `let scale = window.scale_factor();`——语义完全一致（后续 stage 不依赖 scale），消除捕获与警告
- **Files modified:** crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** `cargo build -p mybox-palette --bin palette_checks` 零警告
- **Committed in:** 20f11a0 (Task 4 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** 局部变量化修复，无行为变化、无 scope creep——仅消除编译器警告。

## Issues Encountered
- **mybox-palette lib 测试 flaky（环境性，非本计划引入）：** 全量 `cargo nextest run -p mybox-palette -p mybox-core` 首轮 5 个 hotkey/window 测试失败（hotkey_toggle_summon_creates_floating_window 等），隔离重跑 5/5 通过，全量重跑 154/154 通过——global-hotkey 注册在 154 测试并行下竞争导致时序抖动。与 Task 4 改动无关（仅改 bin + ignored 测试）。
- **桌面会话集成测试运行确认：** `cargo test -p mybox-palette --test integration -- --ignored` 12/12 通过（含新测试 11/12）——探针在真实 macOS 窗口上运行成功。

## Scope Notes
- 计划的 must_haves 要求「在 debug 文件标记 fix 已落地」（palette-capture-click-path.md / palette-keyword-tier-highlight.md 属 orchestrator 所有）：本执行器不触碰这些文件，留待 orchestrator 更新。

## Known Stubs
None - 所有探针完整接线，无占位数据。

## User Setup Required
None - 无外部服务配置要求（E2E 探针运行仅需桌面会话与屏幕录制权限按既有机制）。

## Next Phase Readiness
- keyword 梯队命中 + 高亮渲染 + 点击同步隐藏全链路在真实窗口验证通过（PAL-03 / PAL-04 两个 UAT 回归项 12/12 集成测试覆盖）
- HUMAN-UAT 复核项：UAT 测试 5（橙色高亮肉眼可见）与 UAT 测试 11（点击截图不含面板）已具备探针回归，待人工复核确认
- 无阻塞项

---
*Phase: 03-命令面板*
*Completed: 2026-08-17*

## Self-Check: PASSED
- 5/5 source files exist (filter.rs, ui.rs, lib.rs, palette_checks.rs, integration.rs)
- SUMMARY.md exists
- 4/4 task commits found (9c40064, 2c5f2a3, d6bdd4a, 20f11a0)
- Verification: nextest 154/154, workspace check clean, ignored integration 12/12
