---
status: resolved
trigger: "fix: 启动后第一次截图是正常的只添加了一次蒙版，后面就会有明显的多次蒙版，画面先暗一下，再暗一下。选区内部是 50% 变暗，比选区外亮，但比截图之前暗。然后多次截图后无法取消截图画面，整个页面都被灰色蒙版盖住，不能关闭，不能取消。也不能选区。"
created: 2026-08-13
updated: 2026-08-13
---

# Debug Session: screenshot-mask-accumulates

## Trigger

fix: 启动后第一次截图是正常的只添加了一次蒙版，后面就会有明显的多次蒙版，画面先暗一下，再暗一下。选区内部是 50% 变暗，比选区外亮，但比截图之前暗。然后多次截图后无法取消截图画面，整个页面都被灰色蒙版盖住，不能关闭，不能取消。也不能选区。

## Symptoms

- Expected: 每次截图只添加一次蒙版；选区内部保持截图原亮度（比外部亮）；能正常取消/关闭截图画面
- Actual: 第一次截图正常（一次蒙版）；后续截图蒙版叠加（画面先暗一下再暗一下）；选区内部变暗 50%（比选区外亮但比原图暗）；多次截图后无法取消/关闭/选区，整个页面被灰色蒙版盖住
- Errors: 未报告错误信息
- Timeline: 启动后第一次截图正常，第二次及以后开始异常，多次后完全卡死
- Repro: 启动应用 → 连续多次触发截图 → 观察蒙版叠加与取消失效

## Evidence

- timestamp: 2026-08-13T16:20:00
  finding: overlay.rs `draw_overlay` 为 immediate-mode，每次 redraw 全量 memcpy dimmed 层 + 恢复选区内部——单会话内不存在像素累加（composite_frame 测试通过）
- timestamp: 2026-08-13T16:21:00
  finding: `premultiply_dimmed_pixmap` 每张截图只构建一次 dimmed 层（MASK_ALPHA=0x80 ≈50%）。选区内部 50% 变暗 = 截图本身已含一层旧蒙版 + 新 overlay 再盖一层、选区内部恢复一层
- timestamp: 2026-08-13T16:22:00
  finding: committed 代码（HEAD 6e96ba8）中 `start_capture` 无任何 "session 进行中" 检查；hotkey 与 tray 两条触发路径都直接 spawn capture 线程。连续触发会叠加 capture/overlay
- timestamp: 2026-08-13T16:23:00
  finding: xcap 的 `capture_image`（CGWindowListCreateImage）会把屏幕上所有窗口（含 mybox 自己的 always-on-top overlay）拍进截图——旧 overlay 蒙版会被烤进下一张截图
- timestamp: 2026-08-13T16:24:00
  finding: working tree 已有未提交的重入 guard 草稿（session.rs `active`/`begin_capture`/`deactivate` + lib.rs 调用点，mtime 16:16，早于本会话）——未完成、未测试、未提交
- timestamp: 2026-08-13T16:25:00
  finding: 堆叠场景下共享 SessionState：`store_shots` 替换 shots 但旧 overlay 的 on_draw 闭包仍持有旧 frame/dimmed 缓存（孤儿窗口永远画旧灰色蒙版）；`overlay_ids` 混代、`pending_overlays` 被覆盖 → teardown 只能销毁部分窗口
- timestamp: 2026-08-13T16:26:00
  finding: 次要竞态：`finish()` 时若 `pending_overlays > 0`（window-created 事件还在异步 bus 上），ids 未配对 → 返回空 ids → 已创建窗口永不销毁。window-created 经异步 bus worker 投递（event.rs），可滞后
- timestamp: 2026-08-13T16:27:00
  finding: committed `finish()` 不重置 current_tool/ctrl_down——若上一会话停在 Pen 工具，下一会话拖动会画标注而非选区（与"不能选区"症状一致）；guard 草稿已含此重置
- timestamp: 2026-08-13T16:40:00
  finding: 测试证实：`start_capture_ignores_duplicate_trigger_while_active` 中第二次触发被拒绝，capture fn 只执行一次。修复后 `hotkey_and_menu_both_route_to_start_capture` 需要先 finish() 释放 guard（真实应用中用户会在两次截图之间确认/取消）

## Eliminated

- hypothesis: 单会话内 composite_frame 像素累加（每帧在旧帧上再 blend）
  evidence: draw_overlay 每帧从缓存 frame/dimmed 全量 memcpy 重建，无增量绘制；premultiply_dimmed 只构建一次；composite_frame 单元测试通过 → 排除
- hypothesis: dimmed 层数学错误导致双层变暗（一次 premultiply 里乘了两次 dim）
  evidence: premultiply_dimmed_pixmap 单次应用 (255-MASK_ALPHA)/255；首张截图蒙版正常 → 排除
- hypothesis: 每张截图创建两个 overlay 窗口（同帧双蒙版）
  evidence: create_overlays 每 shot 创建一个 spec；pending 计数与 window-created 配对测试通过 → 排除
- hypothesis: 剪贴板失败导致 overlay 常驻
  evidence: 无法在无 GUI 环境验证，但症状"第一次截图正常"与剪贴板路径失败不符（失败会每次都在）→ 低概率，非主因

## Resolution

root_cause: 截图流程缺少重入保护——上一轮 overlay 未销毁时再次触发截图，xcap 把屏幕上的旧蒙版烤进新截图，新 overlay 再叠加一层蒙版，多代窗口堆叠成孤儿（灰色蒙版常驻且拦截输入）
fix: session 增加 `active` 重入 guard（begin_capture/deactivate，重复触发直接忽略）；`finish()` 重置 current_tool/ctrl_down 并记录 torn_down_pending；`window_created` 对迟到配对事件返回"立即销毁"，lib.rs 监听器据此销毁孤儿 overlay；CaptureModule 持有 session 供测试释放 guard
verification: cargo check --workspace 通过；cargo nextest run --workspace 137 passed（新增 begin_capture 重入测试、torn_down 销毁测试、duplicate trigger 测试）；cargo clippy 无新增警告
files_changed: crates/modules/capture/src/session.rs, crates/modules/capture/src/lib.rs
specialist_hint: rust
commit: fix(capture): re-entrancy guard + orphan-overlay teardown (mask accumulation)
