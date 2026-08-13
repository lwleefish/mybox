---
status: resolved
trigger: "1. 截图区域没有覆盖顶部状态栏和 dock栏。2.直接 enter不能接全屏，非得先选区"
created: 2026-08-13
updated: 2026-08-13
---

# Debug Session: overlay-not-fullscreen-enter

## Trigger

1. 截图区域没有覆盖顶部状态栏和 dock栏。2.直接 enter不能接全屏，非得先选区

## Symptoms

- Expected:
  1. 截图蒙版/覆盖层应覆盖整个屏幕，包括 macOS 顶部菜单栏和 Dock
  2. 触发截图后直接按 Enter 应接受全屏截图，无需先手动框选
- Actual:
  1. 覆盖窗口没有盖住顶部状态栏（菜单栏）和 Dock 区域，只覆盖中间区域
  2. 直接按 Enter 无响应，必须先拖出选区后才能确认
- Errors: 未报告错误信息
- Timeline: 一直如此（非回归）；上一轮 8186348 修复选区移动后仍存在
- Repro:
  1. 触发截图 → 观察遮罩未覆盖菜单栏/Dock
  2. 触发截图 → 直接按 Enter → 无反应；先拖出选区再 Enter → 正常确认

## Current Focus

hypothesis: 两个独立缺陷，同一交互链路上：
  (1) 覆盖窗口层级太低：Overlay 用 WindowLevel::AlwaysOnTop，在 winit 0.30.13 macOS 上映射为
      kCGFloatingWindowLevel=3，低于 Dock(20) 和菜单栏(24)，因此遮罩被两者遮挡，表现为"只盖中间"。
      窗口几何本身正确（xcap 全显示边界 + 物理像素）。
  (2) 直接 Enter 无效有两层原因：(a) 焦点缺失——app 是 Accessory 策略，全局快捷键触发时 app
      未激活，覆盖窗口非 key，未点击前键盘事件到不了窗口；(b) confirm() 无选区时返回 None，
      没有任何"全屏兜底"路径。
next_action: 修复（core: 层级提升 + focus；capture: confirm 全屏兜底）并验证

## Evidence

- timestamp: 2026-08-13T10:00:00
  hypothesis: 覆盖窗口创建于 capture::overlay::create_overlays，用 MonitorGeom 的物理像素
    (x,y,w,h) 作 inner_size/position —— 几何覆盖完整显示区域。
  test: WindowSpec { kind: Overlay, inner_size: Some((w,h)), position: Some((x,y)) }
  result: 几何正确，非几何问题。

- timestamp: 2026-08-13T10:01:00
  hypothesis: 层级不够导致菜单栏/Dock 遮挡遮罩。
  test: mybox-core::window::window_attributes (window.rs:86-91) Overlay → WindowLevel::AlwaysOnTop；
    winit 0.30.13 src/platform_impl/macos/window_delegate.rs:1531-1537 把 AlwaysOnTop 映射为
    kCGFloatingWindowLevel (3)；objc2-app-kit 0.2.2 NSWindow.rs 常量：NSMainMenuWindowLevel=24、
    NSStatusWindowLevel=25（Dock 在 kCGDockWindowLevel=20）。
  result: 确认。层级 3 < 20 < 24，遮罩画在 Dock 和菜单栏之下，两者区域始终露出。

- timestamp: 2026-08-13T10:02:00
  hypothesis: 直接 Enter 无效是键盘焦点问题：app 未激活、覆盖窗口非 key。
  test: app.rs:192-197 用 ActivationPolicy::Accessory；winit 0.30.13 window_delegate.rs:904-909
    set_visible(true) → makeKeyAndOrderFront（仅当 app 已激活时才会成为 key）；focus_window()
    (1573-1583) 才调用 activateIgnoringOtherApps + makeKeyAndOrderFront。点击拖选 = 点击使
    app 激活 + 窗口变 key，故拖选后 Enter 正常。
  result: 确认。叠加第二层缺陷：session::confirm() (session.rs:489-500) `state.selection?`，
    无选区直接返回 None，confirm_and_copy (overlay.rs:446-448) 静默返回 —— 即使焦点修好，
    无选区 Enter 也什么都不做。

## Eliminated

- hypothesis: 窗口内层尺寸/位置用错单位（逻辑 vs 物理）导致没盖满。
  evidence: capture.rs:36-42 将 xcap 点坐标乘以 scale_factor 转物理像素；window.rs:104-109
    用 PhysicalSize/PhysicalPosition 设置。单位一致。
- hypothesis: xcap 捕获本身不含菜单栏像素。
  evidence: CGDisplayBounds 返回完整显示边界（含菜单栏区域），capture_image 同样取全显示。
- hypothesis: 原生 fullscreen (Fullscreen::Borderless) 可解决层级。
  evidence: winit 0.30.13 macOS 用 toggleFullScreen（新 Space），不适合截图覆盖层；
    set_simple_fullscreen 不提升层级且改全局 presentation options，副作用大。改用 objc2 直接 setLevel。

## Resolution

root_cause: (1) Overlay 窗口在 macOS 层级为 kCGFloatingWindowLevel(3)，低于 Dock(20)/菜单栏(24)，遮罩不覆盖两者区域。
  (2) 直接 Enter 无效：Accessory 激活策略下覆盖窗口未激活成 key（无点击则键盘事件不达），且
  confirm() 在无选区时无全屏兜底。
fix: (1) macOS 上 Overlay 窗口创建后经 raw-window-handle 取 NSView→NSWindow，setLevel(NSStatusWindowLevel+1)；
  创建 Overlay 时调用 focus_window() 激活 app 并置 key（全平台）。(2) confirm() 无选区时兜底为
  光标所在显示器（无光标记录则第一台）全屏矩形。
verification: cargo check --workspace ✓；cargo nextest run -p mybox-capture -p mybox-core ✓
  （143 passed, 8 skipped — 新增 confirm_without_selection_falls_back_to_full_monitor、
  confirm_returns_none_without_any_shots）；窗口层级/焦点为 macOS GUI 行为，需用户手动验证。
files_changed: crates/mybox-core/Cargo.toml, crates/mybox-core/src/app.rs, crates/mybox-core/src/window.rs,
  crates/modules/capture/src/session.rs

## Specialist Review

specialist_hint: rust/general — 映射表中 rust→无对应 skill，general→engineering:debug 不可用，直接进行修复。
修复方向自查：(1) 层级用 NSStatusWindowLevel+1(26)：高于菜单栏(24)与 Dock(20)，低于屏保/屏蔽级，
  不会遮挡系统权限弹窗 —— 截图工具惯用做法（CleanShot X / Shottr 同级）。
  (2) focus_window() 在 winit 中即 activateIgnoringOtherApps + makeKeyAndOrderFront，
  与 Accessory 策略兼容（Accessory 应用可被 activateIgnoringOtherApps 激活）。
  (3) 多显示器时"全屏"定义为光标所在显示器，符合每显示器一个选区模型的现有设计；
  跨屏拼接（虚拟屏整图）作为后续功能，不在本次修复范围。
