---
status: resolved
trigger: "鼠标在选区中时鼠标已经修改成移动图标，但是实际还是重新选取，并不是移动选取"
created: 2026-08-13
updated: 2026-08-13
---

# Debug Session: drag-inside-selection-restarts

## Trigger

鼠标在选区中时鼠标已经修改成移动图标，但是实际还是重新选取，并不是移动选取

## Symptoms

- Expected: 鼠标位于已有选区内部时按下拖动，应移动整个选区（光标已正确切换为移动图标）
- Actual: 光标已显示为移动图标（hover 检测正常），但按下并拖动却开始一次新的框选，选区被重新绘制
- Errors: 未报告错误信息
- Timeline: 选区移动功能实现后即存在（上一轮修复 mask 叠加后仍在）
- Repro: 截图 → 拖出选区 → 松开 → 将鼠标移入选区内部（光标变为移动图标）→ 按下并拖动 → 观察选区被重置为新框选而非移动

## Current Focus

hypothesis: 待建立（疑似 mouse-down 命中判定未走 move 分支，或状态机在 drag 开始时的 hit-test 路径丢失）
next_action: gather initial evidence

## Evidence

- timestamp: 2026-08-13T16:45:00Z
  observation: overlay.rs `cursor_for` (line 259-285) 在选区内部返回 `CursorIcon::Move` —— hover 路径有完整的 inside 命中判定；但 `handle_overlay_event` 的 `MouseInput Pressed` 分支（line 322-368）只做 toolbar hit-test → handle hit-test → 否则直接 `session.on_mouse_down(monitor_index, pos)`，没有 "inside selection → move" 分支
- timestamp: 2026-08-13T16:45:00Z
  observation: `session.on_mouse_down` 无条件 `phase = Phase::Selecting` 且 `selection = Some((monitor, selection::drag_start(pos)))` —— 按下点被当作新选区锚点，旧选区被覆盖。SessionState 没有 move 状态（无 Phase::Moving / move offset 字段），`on_mouse_move` 只有 Selecting/Selected/Idle 三分支
- timestamp: 2026-08-13T16:45:00Z
  observation: ac0fd30（re-entrancy guard）只影响 begin_capture/deactivate/finish/window_created，与鼠标状态机无交互；`Phase` 枚举新增变体对其他引用（session.rs match、capture_checks.rs `== Selected`）无破坏
- timestamp: 2026-08-13T16:45:00Z
  observation: 规划文档（02-02/02-03 PLAN）只定义了拖选与手柄 resize，移动选区仅以 cursor 提示形式出现（`CursorIcon::Move`），从未接入 press 路由

## Resolution

root_cause: 选区移动功能只实现了光标提示（cursor_for 返回 Move），mouse-down 路由从未实现 inside-selection 的移动分支，press 无条件走 on_mouse_down 开启新框选
fix: 状态机新增 Phase::Moving（move_anchor + move_rect 记录按下点与起始选区），overlay press 在 Tool::Select 且命中选区内部时走 on_move_start，on_mouse_move 按位移平移选区并 clamp 到所属显示器边界；cursor_for 与 press 路由共用 selection_contains 保持一致（非 Select 工具在选区内仍画标注，光标改回 Crosshair）
status: resolved
verification: cargo check --workspace 通过；cargo nextest run --workspace 147 passed, 8 skipped（新增 10 个单测：selection translate/clamped、session move 状态机、cursor_for 命中路由）
changed_files:
  - crates/modules/capture/src/selection.rs（translate / translate_clamped 纯函数）
  - crates/modules/capture/src/session.rs（Phase::Moving + move_anchor/move_rect + on_move_start/selection_contains）
  - crates/modules/capture/src/overlay.rs（press 路由 move 分支 + cursor_for 与路由一致）
