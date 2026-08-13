---
status: resolved
trigger: "还有个问题，鼠标移动到屏幕边缘，灰色蒙版能被移动"
created: 2026-08-13
updated: 2026-08-13
---

# Debug Session: overlay-window-movable-at-edge

## Trigger

还有个问题，鼠标移动到屏幕边缘，灰色蒙版能被移动

## Symptoms

- Expected: 截图遮罩窗口固定在屏幕上不可移动；鼠标在屏幕边缘拖动时仍正常框选/移动选区，遮罩本身纹丝不动
- Actual: 鼠标移动到屏幕边缘时，灰色蒙版窗口本身可以被拖动（窗口跟随鼠标移动，露出未遮罩区域）
- Errors: 未报告错误信息
- Timeline: 在 b4fa248（提升窗口层级 + focus）之后仍然存在
- Repro: 触发截图 → 将鼠标移到屏幕边缘（可能是 Dock/菜单栏交界或显示器边界）→ 拖动 → 观察蒙版窗口整体移动
- Related context: overlay 是无边框 winit 窗口，macOS 上被提升到 NSStatusWindowLevel+1；疑似 NSWindow 仍可拖动（isMovable / 边缘 resize / 标题栏拖动区域），或 raw-window-handle 设置窗口层级后遗漏了 movable 相关配置

## Current Focus

hypothesis: 已确认
next_action: n/a

## Root Cause

overlay 的 `WindowSpec` 走 `window_attributes` 的 Overlay 分支：`with_decorations(false)`（无边框）但**未设置 `with_resizable(false)`**，而 winit 0.30 的 `resizable` 默认值为 `true`。

在 macOS 上，winit 对无边框窗口应用 `NSWindowStyleMask::Borderless | Resizable | Miniaturizable`（winit-0.30.13 `window_delegate.rs:535-545`）。`Resizable` 让无边框 NSWindow 拥有**不可见的边缘缩放把手**。由于 overlay 铺满整个显示器，其窗口边缘与屏幕边缘重合——鼠标移到屏幕边缘并拖动时，AppKit 将其识别为窗口 resize/move 手势，于是整块灰色蒙版跟着移动并露出未遮罩区域。

此外，无边框 NSWindow 的 `isMovable` 默认仍为 `true`（即便没有标题栏），而 `elevate_overlay_window`（b4fa248）只 `setLevel`，没有关闭可移动/可缩放属性。

## Fix

1. `crates/mybox-core/src/window.rs` `window_attributes` Overlay 分支增加 `.with_resizable(false)` —— 跨平台：macOS 去掉 `Resizable` styleMask（消除边缘 resize 把手），Windows 去掉 `WS_THICKFRAME`（同样不可缩放、不可拖动）。
2. `elevate_overlay_window`（macOS）在 `setLevel` 之后追加 `setMovable(false)` + `setMovableByWindowBackground(false)`，把无边框窗口的拖动能力彻底关掉（双保险）。
3. 补测试断言：`window_attributes_overlay_is_transparent_always_on_top` 校验 `!attrs.resizable`。

## Verification

- `cargo check -p mybox-core` 通过
- `cargo nextest run -p mybox-core window` 18 passed
- `cargo nextest run -p mybox-capture` 71 passed

## Files Changed

- crates/mybox-core/src/window.rs
