---
phase: 03-命令面板
plan: 03
subsystem: core
tags: [rust, winit, global-hotkey, event-bus, window-lifecycle, gap-closure]
requires:
  - phase: 03-命令面板
    provides: 03-01/03-02 palette build-destroy lifecycle + palette_checks E2E harness (the substrate this gap closure repairs)
provides:
  - HotKeyState::Pressed-only forwarding in App::on_hotkey (GAP-1 root cause #1)
  - WindowSpec.on_created per-window main-thread creation callback (GAP-1 root cause #2)
  - PaletteSession::on_window_created pairing entry owned by the palette's own window
  - consecutive_summon_close E2E probe (desktop session, 5/5 --ignored green)
affects: [03-命令面板, capture module window-created broadcast usage]
tech-stack:
  added: []
  patterns:
    - "Per-window WindowSpec.on_created callback (main-thread synchronous, additive extension point) instead of broadcast bus subscriptions for build-destroy pairing"
    - "Pressed-only hotkey filtering at the App boundary (macOS kEventHotKeyPressed/Released double-report)"

key-files:
  created: []
  modified:
    - crates/mybox-core/src/app.rs
    - crates/mybox-core/src/window.rs
    - crates/modules/palette/src/session.rs
    - crates/modules/palette/src/lib.rs
    - crates/modules/palette/src/bin/palette_checks.rs
    - crates/modules/palette/tests/integration.rs
    - .planning/REQUIREMENTS.md

key-decisions:
  - "Released 热键事件在 App::on_hotkey 入口统一过滤（HotKeyState::Pressed 守卫），一次物理按键只产生一次 hotkey.triggered——同时消除 palette 与 capture 两个模块的 Released 双报"
  - "建销配对从广播 core/window-created 总线事件改为 WindowSpec.on_created 主线程同步回调：配对只作用于面板自己的窗口，pending_close 补销毁与创建在同一 about_to_wait drain pass 内完成，消除异步竞态"
  - "保留 set_window_id（harness/测试路径）与 consume_pending_close（five_summon_esc 残留断言），on_window_created 作为生产配对唯一入口"

requirements-completed: [PAL-01]

duration: 7min
completed: 2026-08-15
---

# Phase 3 Plan 03: GAP-1 热键重复唤出修复 Summary

**global-hotkey 0.8.0 macOS Pressed/Released 双报过滤 + 每窗口 on_created 同步配对替代广播订阅，consecutive_summon_close 探针 5/5 桌面会话通过**

## Performance

- **Duration:** 7 min
- **Started:** 2026-08-15T03:39:39Z
- **Completed:** 2026-08-15T03:47:07Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- **GAP-1 根因 #1（热键双报）消除**：`App::on_hotkey` 顶部加 `HotKeyState::Pressed` 守卫——global-hotkey 0.8.0 macOS 后端（kEventHotKeyPressed/Released）与 Windows 后端均双报，一次物理按键原先产生两次 toggle（按下=summon、松开=close）。守卫同时消除 capture 热键的 Released 双报。
- **GAP-1 根因 #2（建销配对跨模块污染 + 异步竞态）消除**：`WindowSpec` 新增 `on_created` 每窗口创建回调（加性、Default=None），`App::create_window` 在窗口注册后、`core/window-created` 总线事件前于主线程同步调用；palette 删除广播订阅，配对闭包只归属面板自己的窗口——capture overlay 的创建既不会覆盖 palette 的 window_id，也不会被 pending_close 误销毁。
- **E2E 回归探针**：新增 `consecutive_summon_close`（真实窗口 + 真实事件循环）：3 轮 summon→配对→观察 ≥2 帧无 Destroy（面板保持显示，GAP-1 闪退症状的直接回归）→ESC 配对销毁无残留，最终唤出观察 ≥3 帧后关闭。harness 的 `realize_window` 切到生产 `spec.on_created` 回调路径，既有 4 个 check 全部改走生产配对。
- **文档同步**：REQUIREMENTS.md PAL-01 标记 Complete（第 39 行 checkbox、第 112 行 traceability、末行时间戳）。

## Task Commits

Each task was committed atomically:

1. **Task 1: core 热键状态过滤** - `3b3bd1e` (fix)
2. **Task 2: on_created 配对归属修复** - `0b54850` (fix)
3. **Task 3: E2E 探针 + 集成接线 + PAL-01 同步** - `18beb99` (test)

**Plan metadata:** committed with this SUMMARY (docs)

## Files Created/Modified

- `crates/mybox-core/src/app.rs` - `on_hotkey` Pressed 守卫 + 新单测 `on_hotkey_released_event_is_ignored`；`create_window` 改 `mut spec`、take 并调用 `on_created`（register 之后、总线 emit 之前）
- `crates/mybox-core/src/window.rs` - `WindowSpec.on_created` 字段声明 + Default + default 断言
- `crates/modules/palette/src/session.rs` - `on_window_created(id) -> bool` 配对入口；set_window_id/close/consume_pending_close 文档措辞更新
- `crates/modules/palette/src/lib.rs` - init 删除广播 `core/window-created` 订阅；build_window_spec 字面量携带 `on_created: Some(...)` 配对闭包；`late_window_created_after_close_is_destroyed` 改走生产回调；新增 `summon_spec_carries_on_created_pairing`
- `crates/modules/palette/src/bin/palette_checks.rs` - `realize_window` 切生产 `spec.on_created` 配对；新增 `check_consecutive_summon_close`（3 轮 + 最终唤出状态机）；main 分发与 usage 更新
- `crates/modules/palette/tests/integration.rs` - `palette_consecutive_summon_close`（`#[ignore]`）
- `.planning/REQUIREMENTS.md` - PAL-01 → Complete

## Verification Evidence

- `cargo nextest run -p mybox-core app::tests::on_hotkey` → 3/3 PASS（known_id_emits + released_ignored + unknown_id_emits_nothing）
- `cargo nextest run --workspace` → **211/211 PASS**（无回归）
- `cargo check --workspace` → exit 0，无 warning
- 桌面会话 `cargo test -p mybox-palette --test integration -- --ignored` → **5/5 PASS**，其中 `consecutive_summon_close` 在真实窗口/事件循环上验证「连续唤出/关闭循环无残留、最终唤出保持显示」

## Decisions Made

- Released 过滤放在 `App::on_hotkey` 入口（而非模块层）——一处修复同时覆盖 palette 与 capture 两个热键消费者，且守住「一次物理按压=一次 toggle」的框架级不变量。
- 配对回调 `on_created` 在 `register` 之后、总线 `window-created` emit 之前调用——回调内入队的 Destroy 与本次创建属于同一 `about_to_wait` drain pass，pending_close 补销毁无竞态窗口。
- `consume_pending_close` 保留：five_summon_esc 探针仍用它做「无残留」断言，生产配对路径则统一走 `on_window_created`。

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None. 所有验收标准一次通过（含桌面会话 5/5 E2E）。

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- GAP-1（BLOCKER）两处根因均已消除并有单元/集成/E2E 三层回归锁定。
- 桌面会话 E2E 已由本 executor 复跑 5/5 全绿；真实物理热键的最终人工复验（按 Cmd+Shift+Space 唤出→关闭→再唤出保持显示，及「开始截图」后再次唤出正常）留给 verify-phase 的 HUMAN-UAT 测试 1 重跑。
- 03-04（GAP-2 文字渲染灰色块）为 Phase 3 剩余 gap closure 计划，与本计划无依赖冲突。

---

*Phase: 03-命令面板*
*Completed: 2026-08-15*

## Self-Check: PASSED

- All key-files exist on disk (app.rs, window.rs, session.rs, lib.rs, palette_checks.rs, integration.rs, SUMMARY.md)
- All 4 plan commits present in git history (3b3bd1e, 0b54850, 18beb99, 046ced1)
- Plan-level verification re-run: `cargo nextest run --workspace` 211/211 PASS; `cargo check --workspace` clean; desktop `--ignored` integration 5/5 PASS
