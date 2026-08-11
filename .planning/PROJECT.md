# mybox

## What This Is

mybox 是一个纯 Rust 原生的跨平台（macOS + Windows）桌面工具箱应用。它不是单一功能软件，而是一个可生长的模块化平台——每个功能（截图、颜色拾取、快捷启动器、定时任务、idea 记录器、AI 对话助手等）是一个独立模块，通过统一框架加载。面向个人使用，开源。第一个里程碑是搭好模块框架并用完整的截图功能验证。

## Core Value

一个统一入口、可按自己想法无限扩展的桌面工具箱——按一个快捷键唤起命令面板，所有工具触手可及。

## Requirements

### Validated

(None yet - ship to validate)

### Active

- [ ] 模块化框架：Module trait、事件总线、窗口管理、热键系统、配置中心、系统托盘
- [ ] 截图模块：屏幕捕获、区域选择、标注工具、Pin 浮窗、剪贴板复制
- [ ] 命令面板：全局快捷键唤出，作为所有模块的统一交互入口
- [ ] 跨平台支持：macOS 和 Windows

### Out of Scope

- Linux 支持 - 先做 macOS + Windows，Linux 可后期加
- 移动端 - 桌面工具箱，不涉及移动平台
- Web 版 - 纯原生应用，不做 Web
- 模块动态加载 - 首期模块编译期注册即可，动态加载留待未来
- Tauri/Electron 方案 - 明确选择纯 Rust 原生

## Context

- **技术栈**：纯 Rust 原生，使用 winit/tao 做窗口管理，tiny-skia 做 CPU 2D 渲染，egui 做即时模式 UI
- **架构**：宿主进程 + 插件式模块。每个模块实现 Module trait，通过 ModuleContext 与框架交互。模块间通过事件总线解耦通信。
- **交互模式**：命令面板式（类似 Spotlight/Alfred），全局快捷键唤出。部分模块有自己的独立窗口（如截图覆盖层、Pin 浮窗）。
- **规划模块**：截图（第一个）、颜色拾取、快捷启动器、定时任务、idea 记录器、AI 对话助手（命令行式交互）
- **AI 助手形态**：命令行式，通过命令面板唤出后直接输入对话，不单独开窗口。框架需预留 LLM SDK 集成口子。
- **定位**：开源项目
- **开发者环境**：macOS (Darwin 24.6.0)

## Constraints

- **Tech stack**: 纯 Rust 原生 - 不使用 Tauri/Electron 等 Web 混合方案
- **Platform**: macOS + Windows - 跨平台是硬需求，Linux 可延后
- **Rendering**: CPU 2D 渲染 (tiny-skia) - 避免 GPU 依赖，保证可移植性
- **Architecture**: 模块化 - 新增功能不能修改核心框架代码，只通过实现 Module trait 接入
- **Compatibility**: macOS 屏幕捕获需用户授权 Screen Recording 权限

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| 纯 Rust 原生（非 Tauri） | 性能、内存占用、跨平台一致性；避免 Web 运行时开销 | - Pending |
| 命令面板式交互 | 统一入口，所有模块通过一个快捷键唤起；类似 Spotlight 体验 | - Pending |
| 编译期模块注册（非动态加载） | 首期简化复杂度，后续可迁移到动态加载 | - Pending |
| 事件总线解耦模块间通信 | 新模块不依赖旧模块 API，可独立增删 | - Pending |
| tiny-skia CPU 渲染 | 无 GPU 依赖，跨平台一致，截图标注场景性能足够 | - Pending |
| Cargo workspace + 每模块独立 crate | 模块可独立编译测试，未来可选动态加载 | - Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? -> Move to Out of Scope with reason
2. Requirements validated? -> Move to Validated with phase reference
3. New requirements emerged? -> Add to Active
4. Decisions to log? -> Add to Key Decisions
5. "What This Is" still accurate? -> Update if drifted

**After each milestone** (via `/gsd:complete-milestone`):
1. Full review of all sections
2. Core Value check - still the right priority?
3. Audit Out of Scope - reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-11 after initialization*
