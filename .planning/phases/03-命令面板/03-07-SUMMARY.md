---
phase: 03-命令面板
plan: 07
subsystem: ui
tags: [winit, modifiers, ctrl-p, ctrl-n, keyboard-navigation, gap-closure, palette, egui]

requires:
  - phase: 03-02
    provides: on_palette_key 共享路由 / KeyEvent 不可外部构造的约束（deviation #2）
  - phase: 03-05
    provides: geometry_revision 修订计数 / position_stable_on_filter 探针模式
  - phase: 03-06
    provides: ui::draw 签名与 hover_click_alignment 探针模式
provides:
  - session 修饰键状态（modifiers 字段 + set_modifiers/modifiers 访问器，summon 重置）
  - on_palette_key modifiers 参数 + Ctrl+P/N 守卫臂（等价 move_selection(∓1)，环绕）
  - on_event_win 闭包 ModifiersChanged → session.set_modifiers 接线
  - E2E 探针 ctrl_pn_navigation（真实 ModifiersChanged 注入断言）
affects: [03-08 IME 中文输入, UAT 测试 9 复验, Phase 4 跨平台, REQUIREMENTS PAL-04]

tech-stack:
  added: []
  patterns:
    - "修饰键状态经 ModifiersChanged 事件流跟踪：winit 0.30 KeyEvent 无 modifiers 字段（0.31 才加入），Ctrl+P/N 判定状态只能经独立事件流写入 session"
    - "路由守卫臂不做 or-pattern 绑定；winit 0.30 字母键恒为 Key::Character（NamedKey 无字母变体，source-verified keyboard.rs:755）"
    - "探针经 Modifiers::from(ModifiersState) 构造真实 ModifiersChanged 事件（Modifiers 字段 pub(crate)，From 转换是唯一公开构造路径）"

key-files:
  created: []
  modified:
    - crates/modules/palette/src/session.rs
    - crates/modules/palette/src/lib.rs
    - crates/modules/palette/src/bin/palette_checks.rs
    - crates/modules/palette/tests/integration.rs

key-decisions:
  - "修饰键状态经 WindowEvent::ModifiersChanged 事件流跟踪存入 session（GAP-6 修复）：winit 0.30 KeyEvent 无 modifiers 字段，Ctrl+P/N 判定所需状态只能经独立事件流获取；summon 重置防跨窗口残留"
  - "on_palette_key 路由增加 modifiers 参数、Ctrl+P/N 守卫臂等价 move_selection(∓1)：无 Ctrl 守卫不满足返回 false、字符透传 TextEdit；Error 态任意键关闭语义保持"
  - "winit 0.30 NamedKey 无字母变体（KeyP/KeyN 是 0.31 概念）——Ctrl+P/N 只匹配 Key::Character 臂（source-verified keyboard.rs:755）"

patterns-established:
  - "ModifiersChanged 接线不早退：事件继续流向 egui-winit（其内部也消费 ModifiersChanged）"
  - "探针覆盖声明纪律：真实 ModifiersChanged → session 接线由探针锁定，OS 物理 Ctrl+P 键击 → winit 事件流由 UAT 测试 9 人工复验"

requirements-completed: [PAL-04]

duration: 26 min
completed: 2026-08-15
---

# Phase 3 Plan 7: GAP-6 Ctrl+P/N 键盘导航 Summary

**修饰键状态经真实 ModifiersChanged 事件流跟踪存入 session，on_palette_key 增加 modifiers 参数与 Ctrl+P/N 守卫臂（等价 ↑/↓ 环绕导航），普通 P/N 无 Ctrl 时透传 egui 输入框，headless 路由单测 + 真实窗口 E2E 探针双层锁定，桌面会话 9/9 通过**

## Performance

- **Duration:** 26 min
- **Started:** 2026-08-15T08:25:45Z
- **Completed:** 2026-08-15T08:51:22Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- **GAP-6 根因消除**：winit 0.30.13 `KeyEvent` 无 modifiers 字段（源码查证 event.rs:523-588，0.31 才加入）——Ctrl+P/N 判定所需修饰键状态经独立的 `WindowEvent::ModifiersChanged` 事件流跟踪：on_event_win 闭包写入 `session.set_modifiers`（不早退，事件继续流向 egui-winit），`on_palette_key` 以 `modifiers` 参数读取（headless 可测，生产与 E2E 共用同一路由——KeyEvent 不可外部构造，03-02 deviation #2）
- **Ctrl+P/N 与 ↑/↓ 等价**：路由新增 Ctrl+P/N 守卫臂（`modifiers.control_key()` + 字符 p/n 匹配）等价 `move_selection(-1/+1)`（环绕）；无 Ctrl 时守卫不满足 → 返回 false → 事件透传 egui-winit，普通 P/N 照常进入 TextEdit（过滤语义不变）；Error 态首臂仍最先匹配——Ctrl+P/N 在 Error 态关闭面板（D-05「任意键」语义保持）
- **summon 重置修饰键状态**（T-03-15）：新窗口新状态——旧窗口的 Ctrl 残留不得使新面板误吞普通 P/N；单测 `summon_resets_modifiers` 锁定
- **E2E 探针 `ctrl_pn_navigation`**：真实窗口 + 真实 `ModifiersChanged(CONTROL)` 注入（经生产 on_event_win 闭包 → `session.set_modifiers`，本探针的核心覆盖点）→ `press_key_mods` 断言 Idle 无选中时 Ctrl+P 环绕到末位（Some(2)）、Ctrl+N 环绕回 Some(0) → 真实 `ModifiersChanged(empty)` 事件流清空断言 → 无修饰键 ESC 路径回归（press_key 内部传 `ModifiersState::empty()`）——桌面会话 9/9 通过

## Task Commits

Each task was committed atomically:

1. **Task 1: session 修饰键状态字段 + 访问器 + 单测** - `2631429` (feat)
2. **Task 2: on_palette_key modifiers 参数 + Ctrl+P/N 分支 + 闭包接线** - `d8c2458` (feat；含 Task 3 的 press_key 适配，见偏差 3)
3. **Task 3: press_key_mods + E2E 探针 ctrl_pn_navigation + 集成接线** - `be87f0b` (feat)

**Plan metadata:** 见最终 docs commit

## Files Created/Modified

- `crates/modules/palette/src/session.rs` - `modifiers` 字段/`set_modifiers`/`modifiers` 访问器 + summon 重置 + 2 个新单测（`modifiers_tracking_roundtrip`、`summon_resets_modifiers`）
- `crates/modules/palette/src/lib.rs` - `on_palette_key` modifiers 参数 + Ctrl+P/N 守卫臂（2 个 `Key::Character` 臂）+ on_event_win 闭包 `ModifiersChanged` 接线 + `session.modifiers()` 传参 + 5 个新路由单测
- `crates/modules/palette/src/bin/palette_checks.rs` - `press_key` 适配（保留签名，内部传 `ModifiersState::empty()`）+ `press_key_mods` + `check_ctrl_pn_navigation` 探针（四阶段 driver 状态机）+ main() 分发/usage 接线
- `crates/modules/palette/tests/integration.rs` - `palette_ctrl_pn_navigation` `#[ignore]` 测试接线（PAL-04/GAP-6 回归）

## Verification Evidence

- `cargo nextest run -p mybox-palette session` — 23/23 PASS（含 2 个新单测）
- `cargo nextest run -p mybox-palette` — 66/66 PASS（9 skipped，含 5 个新路由测试）
- `cargo check --workspace` — exit 0，零 warning
- 桌面会话 `cargo test -p mybox-palette --test integration -- --ignored` — **9/9 PASS**，其中 `ctrl_pn_navigation` 实测：真实 `ModifiersChanged(CONTROL)` 注入 → `session.modifiers().control_key()` 断言通过；Ctrl+P（Idle 无选中）→ `selection() == Some(2)`（环绕到末位，↑ 等价）；Ctrl+N → `Some(0)`（环绕，↓ 等价）；真实 `ModifiersChanged(empty)` → 状态清空断言；无修饰键 ESC → Destroy(created_id) + Hidden

## Decisions Made

- 修饰键状态经 `WindowEvent::ModifiersChanged` 事件流跟踪存入 session（GAP-6 根因：winit 0.30 `KeyEvent` 无 modifiers 字段——键盘事件自身无法提供 Ctrl 状态）
- `on_palette_key` 增加 modifiers 参数（headless 可测、生产与 E2E 共用同一路由），Ctrl+P/N 守卫臂等价 `move_selection(∓1)`
- summon 重置修饰键状态（T-03-15：跨窗口 Ctrl 残留不得影响新面板）

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1/3 - Plan API 缺陷] winit 0.30 `NamedKey` 无 KeyP/KeyN 变体——四守卫臂缩减为两守卫臂**
- **Found during:** Task 2（Ctrl+P/N 分支实现，编译期 E0599）
- **Issue:** 计划指定四个守卫臂（`Key::Named(NamedKey::KeyP)`、字符 p、`KeyN`、字符 n）——但 winit 0.30.13 的 `NamedKey` 枚举（pinned 源码 keyboard.rs:755）没有任何字母变体；字母键恒以 `Key::Character` 到达（KeyP/KeyN 是 winit 0.31 的概念）。计划所引「KeyP/KeyN 覆盖无字符布局」的前提在 0.30 不成立。
- **Fix:** 缩减为两个 `Key::Character` 守卫臂（p/n + `control_key()`），注释说明版本事实；5 个路由测试与 E2E 探针相应改用 `Key::Character("p"/"n".into())`。
- **Files modified:** crates/modules/palette/src/lib.rs
- **Verification:** `cargo nextest run -p mybox-palette` 66/66 通过；桌面探针 9/9 通过
- **Committed in:** d8c2458

**2. [Rule 1/3 - Plan API 缺陷] `ModifiersChanged` 载荷是 `event::Modifiers` 而非 `ModifiersState`**
- **Found during:** Task 2（闭包接线，编译期 E0308）
- **Issue:** 计划断言 `WindowEvent::ModifiersChanged(ModifiersState)` 可直接匹配/构造——实际载荷是 `winit::event::Modifiers`（event.rs:660），其字段为 `pub(crate)`（不可外部构造任意状态）；公开路径是 `Modifiers::from(ModifiersState)`（event.rs:724）与 `Modifiers::default()`。
- **Fix:** 闭包用 `session.set_modifiers(m.state())`；E2E 探针用 `Modifiers::from(ModifiersState::CONTROL/empty())` 构造真实事件注入。
- **Files modified:** crates/modules/palette/src/lib.rs, crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** 桌面探针实测真实注入 → `control_key()` 断言与清空断言均通过
- **Committed in:** d8c2458（lib.rs）、be87f0b（palette_checks.rs）

**3. [Rule 3 - Blocking] Task 3 的 press_key 适配并入 Task 2 提交**
- **Found during:** Task 2（路由签名变更后 crate 无法编译）
- **Issue:** 计划将 `press_key` 内部传 `ModifiersState::empty()` 的适配放在 Task 3——但 `on_palette_key` 签名在 Task 2 已变，palette_checks bin 在两次提交之间不可编译（03-06 偏差 1 同型）。
- **Fix:** Task 2 提交同时包含 press_key 适配（签名保留，既有调用点零改动）；Task 3 只新增 `press_key_mods` + 探针 + 集成接线。
- **Files modified:** crates/modules/palette/src/bin/palette_checks.rs
- **Verification:** 每任务提交后 `cargo nextest run -p mybox-palette` 均全绿
- **Committed in:** d8c2458

**4. [Rule 1 - Bug] 测试前置 `move_selection(1)` 落在 Some(0) 而非 Some(1)**
- **Found during:** Task 2（`ctrl_p_moves_selection_up` 首跑 FAIL）
- **Issue:** UI-SPEC 语义「无选中时首个 ↓ 选择索引 0」——Idle 无选中时一次 `move_selection(1)` 得 Some(0)，计划描述「`session.move_selection(1)`（Some(1)）」与状态机语义不符。
- **Fix:** 测试改为两次 `move_selection(1)`（None → Some(0) → Some(1)）后断言 Ctrl+P 上移到 Some(0)。
- **Files modified:** crates/modules/palette/src/lib.rs（tests 模块）
- **Verification:** 66/66 通过
- **Committed in:** d8c2458

---

**Total deviations:** 4 auto-fixed (2 API 缺陷、1 blocking 编排、1 测试前置 bug)
**Impact on plan:** 偏差 1/2 为计划所依据的 winit API 事实错误（已对 pinned 0.30.13 源码逐项查证并修正，行为等价——守卫臂合并后「无 Ctrl 透传」语义不变）；偏差 3 为任务边界编排修正（03-06 先例）；偏差 4 为测试前置修正。计划 must_have 真值与覆盖声明全部保持：Ctrl+P/N 与 ↑/↓ 等价、普通 P/N 透传、Error 态任意键关闭、真实 ModifiersChanged 跟踪。无范围蔓延。

## Issues Encountered

- 无。全部问题均为计划 API 假设错误，已在偏差记录中修复并验证。

## Known Stubs

None — 所有修改文件均为完整实现，无占位符/TODO/空数据流。

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- GAP-6 已关闭：`ctrl_pn_navigation` 探针可在桌面会话重复运行（`cargo test -p mybox-palette --test integration -- --ignored`，9/9）；人工最终复验见 UAT 测试 9（Ctrl+P/Ctrl+N 可选择命令、普通 P/N 正常输入）
- 03-08（GAP-7 中文输入）无阻塞——本计划未触碰 IME/TextEdit 输入路径；修饰键跟踪只新增 on_event_win 事件分支，不早退、不影响 egui-winit 翻译
- REQUIREMENTS.md 无改动（PAL-04 已 `[x]` Complete，本计划强化其导航键盘映射）

---

*Phase: 03-命令面板*
*Completed: 2026-08-15*

## Self-Check: PASSED

- [x] 4 个修改文件均存在磁盘（session.rs / lib.rs / palette_checks.rs / integration.rs）
- [x] 3 个任务提交均存在 git 历史：`2631429`、`d8c2458`、`be87f0b`
- [x] 全部 acceptance_criteria 验证通过（源码断言 + `cargo nextest run -p mybox-palette` 66/66 + 桌面会话 9/9）
- [x] `cargo check --workspace` 零 warning
- [x] E2E 探针 `ctrl_pn_navigation` 在桌面会话实跑通过（真实 ModifiersChanged 注入 + 环绕断言 + 无修饰键 ESC 回归）
