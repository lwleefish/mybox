# Pitfalls Research

**Domain:** Rust native desktop toolbox (modular plugin architecture)
**Researched:** 2026-08-11
**Confidence:** HIGH

## Critical Pitfalls

### Pitfall 1: winit 0.30 事件循环 Breaking Changes

**What goes wrong:**
winit 0.30 引入了全新的 `ApplicationHandler` trait 架构，彻底改变了事件处理方式。大量教程和示例代码基于 0.29 旧 API，直接照搬会编译失败。同时 `WindowBuilder` 的方法签名也有变化。

**Why it happens:**
winit 在 0.29 -> 0.30 进行了重大重构，将事件循环从回调式改为 trait impl 式。社区文档和博客尚未完全跟上。

**How to avoid:**
- 严格参考 winit 0.30 官方文档和 CHANGELOG
- 使用 `ApplicationHandler` trait 而非 `EventLoop::run` 回调
- 在项目初始化时锁定 winit 版本，避免隐式升级

**Warning signs:**
- 编译报错提到 `EventLoop::run` 签名不匹配
- `WindowEvent` 无法在闭包中获取
- 找不到 `window_builder` 相关方法

**Phase to address:**
Phase 1 (框架搭建阶段，建立事件循环基础设施)

---

### Pitfall 2: macOS Screen Recording 权限被拒

**What goes wrong:**
应用首次运行截图功能时静默失败或返回黑屏，因为 macOS 要求用户手动授予 Screen Recording 权限。如果没有正确处理权限流程，用户会以为工具坏了。

**Why it happens:**
macOS 10.15+ 强制要求屏幕录制权限。`screenshots`/`xcap` crate 调用 CGDisplayStream API 时需要权限。权限请求不会自动弹窗--用户需要去系统设置中手动开启。

**How to avoid:**
- 首次启动时检测权限状态，引导用户去 系统设置 > 隐私与安全 > 屏幕录制
- 使用 `objc2` 调用 `CGPreflightScreenCaptureAccess()` 检测权限
- 提供 "打开系统设置" 按钮（`x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`）
- 截图失败时给出明确的错误提示而非静默失败

**Warning signs:**
- 截图返回全黑图像
- CGDisplayStream 返回空帧
- 用户报告 "截图不工作" 但没有错误信息

**Phase to address:**
Phase 2 (截图模块实现阶段)

---

### Pitfall 3: 全屏覆盖窗口在多显示器下不覆盖

**What goes wrong:**
`Fullscreen::Borderless(None)` 只覆盖主显示器。用户在副屏上无法截图，或者覆盖窗口只出现在一个屏幕上。

**Why it happens:**
winit 的 `Fullscreen` API 默认只操作一个显示器。多显示器场景下，需要为每个显示器创建单独的覆盖窗口，或者手动设置窗口位置和大小来覆盖整个虚拟屏幕。

**How to avoid:**
- 枚举所有显示器 (`event_loop.available_monitors()`)
- 为每个显示器创建一个无边框透明置顶窗口
- 使用 `set_outer_position` + `set_inner_size` 手动定位
- 捕获所有显示器画面，按虚拟坐标拼接

**Warning signs:**
- 副屏截图不可用
- 覆盖窗口只在主屏出现
- 选区坐标在不同显示器间偏移

**Phase to address:**
Phase 2 (截图模块实现阶段)

---

### Pitfall 4: 事件循环中执行耗时操作导致卡顿

**What goes wrong:**
截图选区拖动卡顿、命令面板输入有延迟、UI 响应不流畅。

**Why it happens:**
屏幕捕获（尤其多显示器）是耗时操作。如果在 winit 事件循环的主线程中同步执行，会阻塞所有窗口的事件处理和渲染。

**How to avoid:**
- 屏幕捕获在独立线程执行，结果通过 channel 传回
- 标注绘制保持轻量（tiny-skia CPU 渲染足够快）
- 避免在 `window_event` 处理中做任何 I/O
- 使用 `std::thread::spawn` + `crossbeam::channel` 或 `tokio::spawn`

**Warning signs:**
- 拖动选区时帧率明显下降
- 输入文字时字符显示有延迟
- 多窗口同时存在时整体变卡

**Phase to address:**
Phase 1 (框架设计阶段，确立线程模型) + Phase 2 (截图实现)

---

### Pitfall 5: egui + tiny-skia 渲染上下文冲突

**What goes wrong:**
egui 和 tiny-skia 同时渲染到同一个窗口时，内容互相覆盖或闪烁。

**Why it happens:**
egui 默认使用自己的渲染后端（wgpu 或 glow）。如果同时用 tiny-skia 画自定义内容到同一个窗口的 framebuffer，两者的渲染时序和缓冲区管理可能冲突。

**How to avoid:**
- 方案 A：截图覆盖窗口只用 tiny-skia（不用 egui），命令面板只用 egui
- 方案 B：用 `egui-skia` 集成，让 egui 渲染到 tiny-skia 的 Pixmap
- 方案 C：用 softbuffer 统一管理 framebuffer，先画 tiny-skia 再画 egui
- 推荐 A--不同窗口类型用不同渲染方案，避免集成复杂度

**Warning signs:**
- 窗口内容闪烁
- 标注绘制被 egui 覆盖
- 命令面板 UI 出现在截图覆盖层上

**Phase to address:**
Phase 1 (框架设计阶段，确定渲染策略)

---

### Pitfall 6: Windows DPI 缩放导致截图坐标错位

**What goes wrong:**
在高 DPI 显示器（如 150% 缩放）上，截图选区与实际捕获的图像区域不一致。选区看起来在 A 位置，但截出来的图是 B 位置的内容。

**Why it happens:**
Windows 的 DPI 缩放导致逻辑坐标和物理坐标不一致。winit 提供逻辑/物理坐标转换，但如果不一致地使用两种坐标，就会出现错位。

**How to avoid:**
- 内部统一使用物理像素坐标
- 窗口大小、鼠标位置都转换为 PhysicalPosition
- 截图捕获的图像尺寸也是物理像素
- 仅在 UI 显示尺寸时使用逻辑坐标

**Warning signs:**
- 截图区域与选区不符
- Pin 浮窗位置偏移
- 高 DPI 显示器上一切都不对齐

**Phase to address:**
Phase 2 (截图模块) + 全程持续注意

---

### Pitfall 7: macOS App Activation Policy 导致窗口不显示

**What goes wrong:**
应用窗口无法获取焦点，或者应用出现在 Dock 中而用户希望它只在托盘运行。

**Why it happens:**
macOS 的 `NSApplicationActivationPolicy` 控制应用如何展示。默认是 `Regular`（显示在 Dock），但工具箱类应用通常需要 `Accessory`（不显示在 Dock，但窗口可以获取焦点）。

**How to avoid:**
- 使用 `objc2` 设置 `NSApp.setActivationPolicy(.accessory)`
- 覆盖窗口需要 `NSWindow.setLevel(.screenSaver)` 级别
- Pin 浮窗需要 `NSPanel` 或设置 `canBecomeKey = true`
- 创建窗口时调用 `NSApp.activate(ignoringOtherApps: true)`

**Warning signs:**
- 覆盖窗口不接收键盘事件
- 应用图标出现在 Dock 中
- 点击窗口外区域后窗口消失且无法恢复

**Phase to address:**
Phase 1 (框架搭建，平台适配层)

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| 单显示器假设 | MVP 开发更快 | 多显示器支持需重写窗口逻辑 | MVP 阶段可接受，Phase 2 必须解决 |
| 配置硬编码 | 不用写配置系统 | 任何调整都需重新编译 | 仅 Phase 1 调试阶段 |
| 事件 payload 用 JSON Value | 灵活、快速实现 | 运行时错误、无类型安全 | 可长期接受；插件架构需要灵活性 |
| 跳过错误处理 | 快速出原型 | 静默失败难以调试 | 永远不可接受；至少 log 错误 |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| global-hotkey + winit | 忘记在事件循环中 poll hotkey events | 在 ApplicationHandler 中处理 GlobalHotKeyEvent |
| tray-icon + winit | 托盘菜单点击事件丢失 | tray-icon 有独立事件流，需在事件循环中显式处理 |
| arboard (clipboard) | 在非主线程操作剪贴板 | macOS/Windows 要求剪贴板操作在主线程 |
| screenshots (macOS) | 捕获时窗口正在创建，截到自己的覆盖窗口 | 先捕获屏幕，再创建覆盖窗口 |
| egui + winit 0.30 | 使用旧版 egui-winit 不兼容 winit 0.30 | 确认 egui-winit 版本支持 winit 0.30 |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| 每帧重新捕获屏幕 | CPU 飙高、风扇狂转 | 捕获一次缓存；只在需要时刷新 | 任何实时场景 |
| Pin 浮窗持有原始大图 | 内存占用高（4K 截图 ~32MB/张） | 缩放后存储；按需释放 | 5+ 个 Pin 同时存在 |
| 事件总线全量广播 | 每个事件所有模块都处理 | 事件过滤 + 精确订阅 | 模块 >10 个时 |
| tiny-skia 每帧重建 Pixmap | 渲染延迟 | 复用 Pixmap；只重绘脏区域 | 4K 显示器全屏覆盖 |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| AI 助手 API key 明文存储 | key 泄露 | 使用系统 keychain (macOS) / Credential Manager (Windows) |
| idea 记录器存储敏感信息无加密 | 本地数据泄露 | 可选加密；至少提示用户不要存敏感信息 |
| 全局热键冲突不检测 | 与其他应用冲突导致功能异常 | 启动时检测；冲突时提示用户修改 |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| 覆盖窗口出现有延迟（先黑屏再显示截图） | 感觉卡顿、不专业 | 预捕获屏幕；覆盖窗口出现即显示 |
| 选区无尺寸提示 | 不知道截了多大 | 选区右下角显示 WxH 像素 |
| ESC 关闭后无反馈 | 不确定是取消了还是完成了 | 取消时短暂显示提示文字 |
| Pin 浮窗无阴影 | 与背景融为一体，看不清边界 | 添加细微阴影/边框 |
| 命令面板搜索不模糊匹配 | 必须精确输入才能找到 | 实现模糊匹配（如 fzf 算法） |

## "Looks Done But Isn't" Checklist

- [ ] **截图区域选择:** 常缺少多显示器支持 - 验证：在双屏环境下测试选区跨越
- [ ] **Pin 浮窗:** 常缺少键盘交互 - 验证：ESC 关闭、方向键微移位置
- [ ] **标注工具:** 常缺少撤销/重做 - 验证：Ctrl+Z 撤销上一笔
- [ ] **全局热键:** 常缺少冲突检测 - 验证：与 Snipaste 同时运行测试 F1
- [ ] **命令面板:** 常缺少键盘导航 - 验证：方向键选择、回车执行
- [ ] **配置系统:** 常缺少热重载 - 验证：修改配置文件后无需重启
- [ ] **macOS 权限:** 常缺少首次引导 - 验证：全新安装后截图功能流程

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| winit 0.30 API 不兼容 | LOW | 查阅 0.30 migration guide；更新代码 |
| macOS 权限被拒 | LOW | 引导用户去系统设置；提供 deep link |
| 多显示器覆盖不全 | MEDIUM | 重构为多窗口方案；调整坐标系统 |
| 主线程阻塞 | MEDIUM | 重构耗时操作到独立线程；可能需要改事件流 |
| DPI 坐标错位 | HIGH | 需要全链路统一坐标系统；涉及多处代码 |
| 渲染冲突 | HIGH | 需要重新设计渲染架构；可能涉及多窗口重写 |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| winit 0.30 API | Phase 1 | 编译通过 + 基础窗口创建成功 |
| macOS 权限 | Phase 2 | 首次运行截图时权限引导出现 |
| 多显示器覆盖 | Phase 2 | 双屏环境下选区可跨越显示器 |
| 主线程阻塞 | Phase 1 | 选区拖动 60fps 流畅 |
| 渲染冲突 | Phase 1 | 截图覆盖层和命令面板分别正常 |
| DPI 坐标错位 | Phase 2 | 高 DPI 显示器上选区与截图一致 |
| macOS Activation Policy | Phase 1 | 覆盖窗口可获取键盘焦点；不显示在 Dock |

## Sources

- winit 0.30 CHANGELOG and migration guide
- macOS Screen Recording API documentation (Apple Developer)
- Snipaste 开发者博客 (多显示器截图经验)
- Rust desktop GUI ecosystem community discussions
- Windows DPI awareness documentation (Microsoft Docs)
- objc2 crate documentation (macOS platform interop)

---
*Pitfalls research for: Rust native desktop toolbox*
*Researched: 2026-08-11*
