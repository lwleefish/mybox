---
status: partial
phase: 02-screenshot
source: [02-VERIFICATION.md]
started: 2026-08-13T05:53:17Z
updated: 2026-08-13T05:53:17Z
---

## Current Test

[awaiting human testing]

## Tests

### 1. 真实截图触发与覆盖窗口显示
expected: 每屏出现全屏覆盖窗口，显示捕获画面，选区外为半透明黑色遮罩（成功标准 1）
result: [pending]

### 2. 剪贴板粘贴验证含标注 / 原图
expected: 粘贴出的图像尺寸 = 选区、包含已绘制标注；无标注时粘贴原始选区图像（成功标准 3）
result: [pending]

### 3. macOS 首次权限弹窗 + 设置深链引导
expected: 出现系统授权弹窗（或自动打开 系统设置→隐私与安全性→屏幕录制），终端输出引导日志（成功标准 6）
result: [pending]

### 4. 标注 / 选区交互手感
expected: 与 manual_checklist.md 第 2-6 步一致（拖拽实时边框与 WxH、8 手柄调整、四类标注、Ctrl+Z 逐步撤销、ESC 一键取消）；AlwaysOnTop-only 覆盖限制（A3）为已知 MVP 限制（成功标准 2/4/5）
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
