---
phase: 03-命令面板
plan: 02
subsystem: ui
tags: [egui, fuzzy-matcher, skim, palette, keyboard-navigation, command-execution, e2e, layoutjob]

# Dependency graph
requires:
  - phase: 03-命令面板
    provides: [PaletteSession 六态骨架 + summon/close/generation, build_window_spec 渲染链, CommandRegistry, on_event_win]
  - phase: 02-screenshot
    provides: [capture_checks subprocess-per-check harness 模板, re-entrancy 纪律, UiThreadProxy worker 模式]
provides:
  - 模糊过滤 filter.rs（SkimMatcherV2 三层梯队 + 高亮字符索引 + 64 字符 cap）
  - 导航状态机（move_selection 环绕 / resolve_execution_target 经 filtered 映射）
  - 命令执行生命周期 execute.rs（hide_before_execute 先 Destroy + generation 守卫 finalize）
  - egui 状态渲染（LayoutJob 高亮/Empty/Executing 状态行/Error 三行块/自适应高度）
  - palette_checks 子进程 E2E harness（4 检查）+ #[ignore] 集成测试 + 手动清单
affects: [Phase 4 跨平台完善（Windows 热键 Send 修复、字体发现、圆角 A2 复检）, Phase 4 verifier UAT]

# Tech tracking
tech-stack:
  added: []
  patterns: [on_palette_key 可注入按键路由（winit KeyEvent 不可外部构造）, e2e 驱动 production 路径 + 真实 UiThreadProxy hop, raster 三角 bbox 迭代]

key-files:
  created: [crates/modules/palette/src/filter.rs, crates/modules/palette/src/execute.rs, crates/modules/palette/src/bin/palette_checks.rs, crates/modules/palette/tests/integration.rs, crates/modules/palette/tests/manual_checklist.md]
  modified: [crates/modules/palette/src/session.rs, crates/modules/palette/src/ui.rs, crates/modules/palette/src/lib.rs, crates/modules/palette/src/raster.rs]

key-decisions:
  - "键盘路由抽取为 pub on_palette_key：winit 0.30 KeyEvent 的 platform_specific 字段为 pub(crate)，外部无法合成 KeyboardInput 事件——e2e 与生产闭包共用同一路由函数"
  - "Executing 态输入框改为静态渲染（非 interactive(false) TextEdit）：painter 50% alpha 降暗对自定义绘制统一生效，输入禁用由构造保证"
  - "filter 高亮索引在 ui::draw 内按需重算（filter 确定性且廉价），session 只存 ranked cmd_index 序列（filtered: Vec<usize> 保持 03-01 API）"
  - "Windows 交叉检查受阻项（HotkeyManager 非 Send 的延迟注册模式）记录为 Phase 4 事项（SPEC 验收 10 允许路径）"

patterns-established:
  - "e2e harness 驱动 production 路径：summon_palette → expect_create → 自管 WindowManager + 真实 renderer → inject 事件/on_palette_key → 断言 session + handle 队列；50ms 轮询 + 10s watchdog，driver 永不阻塞事件循环"
  - "raster::paint 纹理三角仅迭代自身 bbox（此前整 clip 像素循环，debug 下单帧数十秒）"
  - "session 状态机全方法化 + generation 守卫：stale 完成/重入全部 headless 可测"

requirements-completed: [PAL-03, PAL-04, PAL-05]

# Metrics
duration: ~55min
completed: 2026-08-15
---

# Phase 03-02: 模糊搜索 + 键盘导航 + 命令执行 Summary

**命令面板交互闭环：SkimMatcherV2 三层梯队模糊过滤（#FF6000 字符高亮）、filtered 空间环绕导航、generation 守卫的命令执行生命周期（截图命令先销毁面板）、六态 egui 渲染与自适应高度——209 单测 + 4 项子进程 E2E 全绿**

## Performance

- **Duration:** ~55 min
- **Started:** 2026-08-15T00:50Z
- **Completed:** 2026-08-15T01:47Z
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments

- 模糊过滤：name > description > keywords 三层梯队 + 稳定 tie-break + `MAX_QUERY_LEN=64` 输入上限（Security V5）；「截图」「jt」均命中「开始截图」排首位（jt 经 pinyin keyword `jietu`，Pitfall 6 数据路径）
- 导航状态机：`move_selection` filtered 空间环绕（Idle 首按 ↓ 选 0）、`resolve_execution_target` 经 filtered 映射返回命令下标（Filtering 重排回归被单测 + E2E 双重锁定）
- 执行生命周期：`execute()` 防重入 → `hide_before_execute` 先入队 Destroy（Pitfall 4 队列序）→ 命名线程 runner → generation 守卫 finalize（Ok→Hidden+销毁 / Err→Error 面板内提示 + redraw）
- 六态 egui 渲染：LayoutJob 命中字符 `#FF6000` 高亮（字符位置→UTF-8 字节区间转换）、Empty/Error/零命令三行块逐字对照 UI-SPEC copy、Executing 状态行「正在执行：{name}…」+ 50% 降暗 + 输入禁用、选中行自动滚动
- 键盘接线：on_event_win 在 egui-winit 之前拦截 ↑/↓/Enter/ESC（Error 任意键关闭守卫臂置最前）、输入/状态快照驱动 `sync_window_geometry`（request_inner_size + 显示器重居中）+ request_redraw（Pitfall 8）
- E2E：palette_checks harness（真实窗口 + 真实 UiThreadProxy finalize hop）4 检查全绿，含截图时序队列序断言与 5× 唤出-ESC 无残留

## Task Commits

Each task was committed atomically:

1. **Task 1: 模糊过滤 + 导航状态机** - `10039a6` (feat)
2. **Task 2: 命令执行生命周期 + 键盘接线 + 状态渲染 + 自适应高度** - `3d13433` (feat)
3. **Task 3: E2E 子进程检查 + 集成测试 + 手动清单 + workspace 健康** - `2e94bac` (test)

## Files Created/Modified

- `crates/modules/palette/src/filter.rs` - SkimMatcherV2 三层梯队过滤 + Match 高亮索引（纯函数，7 单测）
- `crates/modules/palette/src/execute.rs` - execute() 生命周期：防重入、hide_before_execute 先 Destroy、generation 守卫 finalize（5 单测含队列序断言）
- `crates/modules/palette/src/session.rs` - set_input（三态转换+截断）、move_selection、resolve_execution_target、set_executing/finalize/error/executing_id（+16 单测）
- `crates/modules/palette/src/ui.rs` - 六态分派渲染、highlight_job、状态块、自动滚动、set_input 回写接线
- `crates/modules/palette/src/lib.rs` - on_palette_key 按键路由、帧循环输入/状态快照、sync_window_geometry、summon_palette pub
- `crates/modules/palette/src/raster.rs` - 纹理三角 bbox 迭代优化（Rule 1 perf 修复）
- `crates/modules/palette/src/bin/palette_checks.rs` - E2E harness + 4 check_*（watchdog + exit 0/1/2）
- `crates/modules/palette/tests/integration.rs` - 4 个 #[ignore] 子进程-per-check 测试
- `crates/modules/palette/tests/manual_checklist.md` - 8 步手动验收清单

## Verification Evidence

- `cargo nextest run`（全 workspace）：209 passed / 12 skipped（#[ignore] E2E）
- `cargo check --workspace`：exit 0，无 warning
- `cargo test -p mybox-palette --test integration -- --ignored`（真实桌面会话）：4/4 passed（summon_render / fuzzy_navigation_execute / capture_hides_first / five_summon_esc_no_residue）
- mybox-test Pitfall 7 复检：WindowRequest match 已含全部 4 分支，无需修改（条件性修改未触发）
- Windows 交叉检查：`rustup target add x86_64-pc-windows-msvc` 成功；`cargo check --target` 仅剩 2 处**既有**错误（capture 02-01 / palette 03-01 继承的延迟热键注册 Send 模式）→ 记录 Phase 4

## Decisions Made

- 键盘路由抽取为 `pub on_palette_key`（见 Deviations #2——winit API 硬约束下的最小侵入方案）
- Executing 输入框静态渲染而非 `interactive(false)` TextEdit（降暗统一 + 输入禁用由构造保证）
- 高亮索引在 ui::draw 按需重算，session 保持 `filtered: Vec<usize>` API 不变

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] 首帧死锁：with_framebuffer 内调用 session.textures()**
- **Found during:** Task 3（palette_checks summon_render 挂起定位）
- **Issue:** 03-01 帧循环在 `with_framebuffer`（持有 state 锁）闭包内调用 `session.textures()`（重锁同一 std Mutex）——面板第一帧即死锁
- **Fix:** 纹理表快照移到 `with_framebuffer` 之前
- **Files modified:** crates/modules/palette/src/lib.rs
- **Verification:** summon_render / fuzzy E2E 均通过；`cargo nextest run -p mybox-palette` 49 passed
- **Committed in:** 2e94bac

**2. [Rule 3 - Blocking] winit 0.30 KeyEvent 无法外部构造——键盘路由抽取为 pub on_palette_key**
- **Found during:** Task 3（合成 KeyboardInput 事件编译失败）
- **Issue:** KeyEvent 的 `platform_specific` 字段为 `pub(crate)`，plan 的 `DeviceId::dummy()` 合成方案无法绕过；winit 0.30 亦无公开构造器
- **Fix:** 键盘路由从 on_event_win 闭包抽取为 `pub fn on_palette_key(session, windows, ui_proxy, key)`，生产闭包与 e2e harness 共用同一入口；RedrawRequested（可构造）仍经真实闭包注入
- **Files modified:** crates/modules/palette/src/lib.rs, crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** fuzzy_navigation_execute / five_summon_esc E2E 经该路由驱动并通过
- **Committed in:** 2e94bac

**3. [Rule 1 - Perf] raster::paint 纹理三角全 clip 像素循环——debug 下单帧数十秒**
- **Found during:** Task 3（fuzzy E2E 10s watchdog 超时，sample 栈定位在 paint_textured_triangle/barycentric）
- **Issue:** 每纹理三角对整 clip pixmap（1200×640 物理像素）逐像素 barycentric 测试，Filtering 帧数百三角 → O(w·h·N)，debug 构建分钟级
- **Fix:** 按三角自身 bbox 截断像素迭代（bbox 外像素必然不通过 barycentric 测试，行为等价）
- **Files modified:** crates/modules/palette/src/raster.rs
- **Verification:** `cargo nextest run -p mybox-palette` 全绿（含 raster 正确性测试）；fuzzy E2E 从挂起变 1.5s 内通过
- **Committed in:** 2e94bac

**4. [Rule 1 - Bug] five_summon_esc_no_residue 断言读闭包外的 round 副本**
- **Found during:** Task 3（检查失败 "completed 0 rounds"）
- **Issue:** driver 闭包 move 捕获 `round`（usize Copy），外层检查函数读到的永远是初始值 0
- **Fix:** 最终残留断言移入 driver 的 `round >= 5` 分支内部
- **Files modified:** crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** five_summon_esc_no_residue E2E 通过（5 轮 Create/Destroy 配对 + generation==5）
- **Committed in:** 2e94bac

**5. [Rule 3 - Blocking] EventLoop 用户事件构造 API**
- **Found during:** Task 3（编译错误）
- **Issue:** winit 0.30 `EventLoop::<T>::new()` 仅限单元类型；自定义 UserEvent 需 `EventLoop::with_user_event().build()`
- **Fix:** harness 改用 `EventLoop::<AppEvent>::with_user_event().build()`，user_event 处理 `AppEvent::Ui(f)`（真实 finalize hop）
- **Files modified:** crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** fuzzy E2E 的 finalize→Destroy 经真实 UiThreadProxy hop 断言通过
- **Committed in:** 2e94bac

---

**Total deviations:** 5 auto-fixed (2 阻塞 API 约束、2 缺陷、1 性能缺陷)
**Impact on plan:** 全部为 E2E 交付所必需的修复；其中 #1/#3 修复了 03-01 遗留的隐藏缺陷（首帧死锁、光栅化 O(w·h·N)）。无范围蔓延。

## Issues Encountered

- Windows 交叉检查：目标安装成功，但 `*mut c_void cannot be sent` 阻断 capture（02-01 遗留）与 palette（03-01 继承）的延迟热键注册模式——HotkeyManager 的 Windows Send 化属核心框架改动（Rule 4 级），按 SPEC 验收 10 允许路径记录为 Phase 4 事项，不阻塞本 plan。
- 检查 harness 的 winit 事件循环需要 50ms 轮询驱动（finalize 经 AppEvent::Ui 回跳，driver 不能阻塞主循环）——已内建于 harness 设计。

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- PAL-03/04/05 全部验收闭环：单测 + E2E 证据齐备，手动清单（8 步）待 verifier 按 Phase 3 成功标准走查
- Phase 4 事项：Windows HotkeyManager Send 化（延迟热键注册模式）、Windows 字体发现、圆角 A2 视觉复检
- 03-02 就绪供 `/gsd-verify-work` UAT 与 Phase 3 收尾（phase 最后 plan）

---

*Phase: 03-命令面板*
*Completed: 2026-08-15*

## Self-Check: PASSED

- key-files.created: 5/5 存在于磁盘（filter.rs, execute.rs, palette_checks.rs, integration.rs, manual_checklist.md）
- key-files.modified: 4/4 存在（session.rs, ui.rs, lib.rs, raster.rs）
- Task commits: 10039a6 / 3d13433 / 2e94bac 均在 git log
- 验证命令全部重跑通过：cargo nextest run（209 passed）、cargo check --workspace（exit 0）、cargo test -p mybox-palette --test integration -- --ignored（4/4）
