# Project Research Summary

**Project:** mybox
**Domain:** Rust native desktop toolbox (modular plugin architecture)
**Researched:** 2026-08-11
**Confidence:** HIGH

## Executive Summary

mybox 是一个纯 Rust 原生的跨平台桌面工具箱，采用模块化插件架构。核心技术栈以 winit 0.30 做窗口管理、tiny-skia 做 CPU 2D 渲染、egui 做即时模式 UI，通过 global-hotkey 和 tray-icon 实现全局热键和系统托盘。架构上采用宿主进程 + 编译期注册的 Module trait 模式，模块间通过事件总线解耦通信。

推荐方案成熟可行：Rust 桌面 GUI 生态在 2025-2026 年已足够成熟，winit + tiny-skia + egui 的组合在截图标注场景性能充分。主要风险集中在 macOS 平台适配（屏幕录制权限、NSWindow 激活策略）和 winit 0.30 的 breaking changes 适配。通过正确的线程模型（耗时操作 offload 到独立线程）和统一的物理坐标系统，可以避免性能和 DPI 问题。

首个里程碑应先搭建模块框架核心（Module trait、事件总线、窗口管理、热键、托盘），再用完整截图功能验证框架可用性。命令面板作为统一交互入口与框架同步建设。

## Key Findings

### Recommended Stack

纯 Rust 原生方案，无 WebView/GPU 依赖。核心 7 个 crate 构建整个应用，平台特定代码通过 conditional compilation 隔离在 core 层。

**Core technologies:**
- **winit 0.30**: 窗口创建与事件循环 - Rust 窗口管理事实标准；0.30 新架构需注意迁移
- **tiny-skia**: CPU 2D 渲染 - 纯 Rust，无平台依赖；截图标注性能充分
- **egui**: 即时模式 UI - 工具栏、命令面板、设置界面
- **global-hotkey**: 全局热键 - 跨平台，tao 生态组件
- **tray-icon**: 系统托盘 - 跨平台，与 global-hotkey 同生态
- **arboard**: 剪贴板 - 支持图片格式，截图复制必需
- **screenshots/xcap**: 屏幕捕获 - 跨平台截图核心

### Expected Features

**Must have (table stakes):**
- 系统托盘驻留
- 全局热键
- 截图区域选择 + 剪贴板复制
- ESC 取消操作
- 配置持久化

**Should have (competitive):**
- 模块化插件架构（核心差异化）
- 命令面板统一入口（核心差异化）
- Pin 浮窗
- 截图标注工具
- 放大镜

**Defer (v2+):**
- AI 对话助手
- 模块动态加载 (WASM)
- 屏幕录制

### Architecture Approach

三层架构：Application 层（入口 + 事件循环）-> Framework 层（mybox-core: Module Registry, Event Bus, Window Manager, Hotkey Manager, Config, Tray）-> Module 层（capture, palette 等独立 crate）。模块间零直接依赖，全部通过事件总线通信。窗口类型通过 WindowSpec 抽象统一管理（Overlay/Floating/Panel/Tray）。

**Major components:**
1. Module Registry - 模块注册、依赖解析、生命周期管理
2. Event Bus - pub/sub 解耦通信，JSON payload
3. Window Manager - 多窗口创建/销毁，WindowSpec 抽象
4. Hotkey Manager - 全局热键注册与事件分发
5. Config Center - 分节 TOML 配置，按模块 ID 命名空间

### Critical Pitfalls

1. **winit 0.30 breaking changes** - 必须使用 ApplicationHandler trait，不能照搬旧教程
2. **macOS Screen Recording 权限** - 首次运行需引导用户授权，否则截图静默失败
3. **多显示器覆盖窗口** - Fullscreen::Borderless 只覆盖主屏，需多窗口方案
4. **主线程阻塞** - 屏幕捕获等耗时操作必须放到独立线程
5. **渲染冲突** - 不同窗口类型用不同渲染方案（截图用 tiny-skia，面板用 egui）

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: 框架核心
**Rationale:** 一切模块的基础，必须先建。winit 事件循环、窗口管理、热键、托盘、配置这些基础设施不建好，任何模块都无法运行。
**Delivers:** 可运行的空壳应用（托盘驻留 + 全局热键 + 窗口创建能力 + 配置系统）
**Addresses:** Module trait, EventBus, WindowManager, HotkeyManager, Tray, Config
**Avoids:** winit 0.30 API 问题（Phase 1 解决）、渲染冲突（Phase 1 确定策略）、macOS Activation Policy（Phase 1 处理）

### Phase 2: 截图模块
**Rationale:** 第一个功能模块，用于验证框架可用性。依赖 Phase 1 的窗口管理和热键系统。
**Delivers:** 完整截图功能（捕获 + 区域选择 + 标注 + Pin + 剪贴板）
**Uses:** screenshots/xcap, tiny-skia, arboard
**Implements:** Overlay 窗口, 区域选择交互, 标注工具, Pin 浮窗
**Avoids:** macOS 权限问题、多显示器覆盖、DPI 坐标错位

### Phase 3: 命令面板
**Rationale:** 统一交互入口，提升工具箱体验。依赖 Phase 1 框架 + Phase 2 模块注册的命令。
**Delivers:** 全局快捷键唤出的命令面板，展示并执行所有模块命令
**Uses:** egui (命令面板 UI), 模糊匹配算法
**Implements:** Panel 窗口类型, 命令注册与搜索

### Phase 4: 跨平台完善 + 打磨
**Rationale:** 首轮功能完成后，补齐 Windows 平台测试和边缘场景。
**Delivers:** macOS + Windows 双平台稳定运行
**Addresses:** Windows DPI, 多显示器完善, 性能优化, 错误处理

### Phase 5+: 扩展模块
**Rationale:** 框架验证通过后，按需添加颜色拾取、idea 记录器、快捷启动器等模块。
**Delivers:** 新功能模块（每个模块 1-2 个 phase）

### Phase Ordering Rationale

- 框架核心先行：所有模块依赖框架，无法跳过
- 截图第二个：用户明确要求截图作为第一个功能验证框架
- 命令面板第三个：依赖模块注册的命令才有内容展示，截图模块完成后命令面板才有实际命令可执行
- 跨平台完善第四：功能开发时以 macOS 为主，Windows 适配在功能稳定后集中处理
- 扩展模块最后：框架验证通过后，新模块成本低

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1:** winit 0.30 新 API 架构，egui + winit 0.30 集成方式
- **Phase 2:** macOS 屏幕录制权限流程，多显示器截图方案
- **Phase 3:** 模糊匹配算法选择，egui 浮窗交互模式

Phases with standard patterns (skip research-phase):
- **Phase 4:** Windows 适配是标准 winit 跨平台流程
- **Phase 5+:** 新模块遵循已建立的 Module trait 模式

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | 所有推荐 crate 均为成熟、活跃维护的 crates.io 包 |
| Features | HIGH | 竞品分析充分（Snipaste, Raycast, CleanShot X），功能定位清晰 |
| Architecture | HIGH | Module trait + EventBus 是成熟的插件架构模式，Rust workspace 多 crate 结构是标准实践 |
| Pitfalls | HIGH | winit 0.30 和 macOS 权限问题有官方文档佐证 |

**Overall confidence:** HIGH

### Gaps to Address

- **winit 0.30 + egui 集成:** egui-winit 对 winit 0.30 的兼容性需要在 Phase 1 规划时确认具体版本
- **macOS NSPanel for Pin:** Pin 浮窗可能需要 NSPanel 而非 NSWindow，具体实现在 Phase 2 规划时研究
- **Windows Graphics Capture:** Windows 平台截图 API 细节在 Phase 4 适配时研究

## Sources

### Primary (HIGH confidence)
- crates.io - 所有推荐包的版本和活跃度验证
- winit 0.30 official documentation - 事件循环架构
- Apple Developer docs - macOS Screen Recording permission
- Microsoft Docs - Windows DPI awareness

### Secondary (MEDIUM confidence)
- Snipaste 官方文档 - 截图功能设计参考
- Raycast 文档 - 命令面板 + 插件架构参考
- Rust 社区讨论 - winit/egui 集成经验

### Tertiary (LOW confidence)
- 各 crate GitHub issues - 已知问题和兼容性信息

---
*Research completed: 2026-08-11*
*Ready for roadmap: yes*
