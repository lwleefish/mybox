---
status: partial
phase: 04-跨平台完善
source: [04-VERIFICATION.md]
started: 2026-08-18
updated: 2026-08-18
---

## Current Test

[awaiting human testing]

## Tests

### 1. Windows 真机运行 mybox，确认系统托盘图标实际显示且菜单可用（ROADMAP SC1）
expected: 托盘图标出现，右键菜单展示模块菜单项和退出按钮

### 2. Windows 真机触发截图，核对画面捕获、选区、标注、复制全链路（ROADMAP SC2）
expected: 热键触发截图，覆盖窗口显示屏幕画面，拖拽选区，确认后剪贴板可粘贴

### 3. Windows 真机唤出命令面板并执行命令（ROADMAP SC3）
expected: 全局热键（Ctrl+Shift+Space）唤出面板，中文命令名有字形，键盘导航+回车执行

### 4. Windows 真机 150% 缩放下截图选区与实际捕获区域一致（ROADMAP SC4 真机确认）
expected: 高 DPI 显示器上选区边界与捕获画面像素对齐（point_to_physical 换算正确性）

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps