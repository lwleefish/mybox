---
phase: 04-跨平台完善
plan: 02
subsystem: [core, ui, testing]
tags: [error-debt, dpi, panic-isolation, windows-ci]

requires:
  - phase: 03-命令面板
    provides: 9 项错误债（WR-01..03 + IN-01..06）来源（03-REVIEW.md）
provides:
  - Phase 3 遗留 9 项错误债全部修复并有测试锚定（D-08）
  - point_to_physical DPI 纯函数 + 4 scale 用例；compute_geometry @1.5 用例（D-06/D-07）
  - 核心层 panic 隔离（WR-02 catch_unwind）+ 窗口创建失败回调（WR-01 on_create_failed）
affects: [04-03, 后续所有涉及窗口生命周期/回调/几何的 phase]

tech-stack:
  added: []
  patterns:
    - "WindowSpec.on_create_failed take-once 回调（与 on_created 对称的失败配对路径）"
    - "dispatch_window_event 自由函数包 catch_unwind（模块回调 panic 不杀事件循环）"
    - "run_command 完成/失败统一经 dispatch_completion → UiThreadProxy hop（Arc<Mutex<Option>> 恰好一次）"
    - "effective_window_height 单一高度入口（summon 与帧循环共用，杜绝 diverging）"

key-files:
  created: []
  modified:
    - crates/modules/capture/src/capture.rs
    - crates/modules/palette/src/position.rs
    - crates/mybox-core/src/window.rs
    - crates/mybox-core/src/app.rs
    - crates/mybox-core/src/command.rs
    - crates/mybox-core/src/context.rs
    - crates/modules/palette/src/lib.rs
    - crates/modules/palette/src/session.rs
    - crates/modules/palette/src/ui.rs
    - crates/modules/palette/src/bin/palette_checks.rs

key-decisions:
  - "WR-01 失败回调采用无参 on_create_failed + take-once（失败时无 WindowId 可传；take 防止重试双触发）"
  - "IN-04 的 config_dir 失败以 Option<PathBuf> 显式传播，运行器返回带消息 Err——绝不静默打开空路径/CWD 相对路径"
  - "WR-02 真实窗口测试在 Windows 用 EventLoopBuilderExtWindows::with_any_thread（winit 全平台主线程限制；nextest 测试线程非主线程）"

patterns-established:
  - "失败配对路径：create_window/renderer 失败 → on_create_failed → session 复位 Hidden（has_live_window()==false）→ toggle 可再次 summon"
  - "事件循环存活保证：模块 on_event/on_event_win 回调 catch_unwind 隔离（CR-01 类 panic 不可再杀循环）"

requirements-completed: [INFRA-04, FRMW-06]

duration: ~2h
completed: 2026-08-18
---

# Phase 04: 跨平台完善 — Plan 02 Summary

**Phase 3 遗留 9 项错误债（WR-01..03 + IN-01..06）全部修复并测试锚定，DPI 换算抽为纯函数，Windows CI 回归全绿**

## Performance

- **Duration:** ~2h（含 2 轮 CI 迭代）
- **Started:** 2026-08-18
- **Completed:** 2026-08-18
- **Tasks:** 4
- **Files modified:** 10

## Accomplishments
- DPI 换算纯函数 `point_to_physical`（capture.rs）+ 1.0/1.25/1.5/2.0 四 scale 用例 + 负坐标（虚拟屏幕）用例；`compute_geometry` @1.5 手算用例（position.rs）
- WR-01: `WindowSpec.on_create_failed` take-once 回调，create_window/renderer 双失败路径触发，session 复位 Hidden 不再永久卡死
- WR-02: `dispatch_window_event` 自由函数，on_event/on_event_win 均 catch_unwind 隔离（真实窗口驱动 on_event_win 臂——CR-01 路径全覆盖）
- IN-01: run_command spawn 失败不 panic 主线程，Err 经 UiThreadProxy hop 到 finalize(Err)
- IN-04: config_dir 不可用时 builtin 运行器显式 bail，不静默打开空路径
- WR-03: effective_window_height 零命令 → Empty 144px（summon/sync 单一入口）
- IN-02/03/05/06: 锁序不变量文档化、realize_window 旧窗口 destroy、TextEdit char_limit(64)、探针具名常量
- Windows CI 回归全绿：build（--target x86_64-pc-windows-msvc --locked）/ unit tests（245 tests）/ 16 探针

## Task Commits

1. **Task 1: DPI 纯函数 + 多 scale 测试** - `2e12597` (feat)
2. **Task 2: 核心层错误债（WR-01/02 + IN-01/04）** - `ca4946a` (fix)
3. **Task 3: 面板层错误债（WR-01 接线 + WR-03 + IN-02/03/05/06）** - `6e27c25` (fix)
4. **Task 4: CI 回归修复（WR-02 测试 Windows 主线程限制）** - `e71245a` (fix)

## Files Created/Modified
- `crates/modules/capture/src/capture.rs` - point_to_physical 纯函数 + 4 scale 测试
- `crates/modules/palette/src/position.rs` - compute_geometry @1.5 用例
- `crates/mybox-core/src/window.rs` - WindowSpec.on_create_failed 字段
- `crates/mybox-core/src/app.rs` - notify_create_failed + dispatch_window_event + 测试
- `crates/mybox-core/src/command.rs` - run_command spawn-Err hop + Option<PathBuf> builtins
- `crates/mybox-core/src/context.rs` - pending_count（测试轮询）
- `crates/modules/palette/src/lib.rs` - on_create_failed 接线 + effective_window_height 调用 + IN-02 注释
- `crates/modules/palette/src/session.rs` - on_create_failed 复位 + 锁序注释 + 测试
- `crates/modules/palette/src/ui.rs` - effective_window_height + TextEdit char_limit + 测试
- `crates/modules/palette/src/bin/palette_checks.rs` - IN-03 旧窗口 destroy + IN-06 具名常量

## Decisions Made
- 失败回调无参 + take-once（失败时无 WindowId；防双触发）
- config_dir 失败显式 Option 传播而非静默降级
- WR-02 测试用 with_any_thread 绕开 winit 全平台主线程限制

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] WR-02 真实窗口测试在 Windows CI panic（主线程限制）**
- **Found during:** Task 4（CI run 32097496749）
- **Issue:** executor 假定 Windows 无主线程限制（非 macOS 分支用 `EventLoop::new()`），winit 0.30.13 实际全平台检查主线程（event_loop.rs:190 panic），nextest 测试运行在 harness 线程
- **Fix:** `EventLoopBuilder::new().with_any_thread(true)`（`EventLoopBuilderExtWindows`），非 macOS 分支 cfg import
- **Files modified:** crates/mybox-core/src/app.rs
- **Verification:** Windows CI unit tests 全绿（run 32097933874）
- **Committed in:** e71245a

**2. [Rule 3 - Minor] IN-06 精确 ACCENT 匹配与 04-01 容差修复冲突**
- **Found during:** Task 3
- **Issue:** 计划要求像素比较改为精确 `== ACCENT_RGB`，但 04-01（90b44c8）已把 keyword_highlight 从精确匹配放宽为容差扫描（Windows AA spread 差异）——精确匹配会打红已绿的 Windows CI
- **Fix:** 保留容差比较逻辑，仅把几何魔法数字（68/116）抽为 ROW_BAND_*（派生 ui::SP_*）、参考色抽为 ACCENT_RGB（容差中心）
- **Files modified:** crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** 本地探针 keyword_highlight OK + Windows CI Palette probes 绿
- **Committed in:** 6e27c25

**3. [Rule 3 - Minor] on_create_failed 闭包捕获冲突**
- **Found during:** Task 3
- **Issue:** on_created 闭包 move 了 created_session，on_create_failed 无法再捕获同一 Arc
- **Fix:** 在 build_window_spec 前额外 clone `failed_session`，两闭包各持一份
- **Files modified:** crates/modules/palette/src/lib.rs
- **Verification:** 编译通过 + session 测试绿
- **Committed in:** 6e27c25

---

**Total deviations:** 3 项（1 blocking + 2 minor），均为实现层必要修正，无范围蔓延。
**Impact on plan:** 全部 must_haves 达成；CI 全绿闭环。

## Issues Encountered
- 本地 macOS 窗口时序单测（hotkey_toggle_*/late_window_*/summon_spec_* 共 5 个）已知 flaky（重跑即绿），CI Windows 稳定——与 04-01 记录一致，非本 plan 引入
- 推送时 GitHub 网络两次超时（Recv failure / connect timeout），重试成功，CI 触发正常

## User Setup Required
None - 无外部服务配置。

## Next Phase Readiness
- 错误债清零：后续 phase 无需再携带 WR/IN 项
- 高 DPI 验证基础就绪：point_to_physical/compute_geometry 用例可直接支撑 04-03 或后续 DPI 工作
- Windows CI 回归链保持全绿，新改动 push 即验证

---
*Phase: 04-跨平台完善*
*Completed: 2026-08-18*