<!-- GSD:project-start source:PROJECT.md -->
## Project

**mybox**

mybox 是一个纯 Rust 原生的跨平台（macOS + Windows）桌面工具箱应用。它不是单一功能软件，而是一个可生长的模块化平台——每个功能（截图、颜色拾取、快捷启动器、定时任务、idea 记录器、AI 对话助手等）是一个独立模块，通过统一框架加载。面向个人使用，开源。第一个里程碑是搭好模块框架并用完整的截图功能验证。

**Core Value:** 一个统一入口、可按自己想法无限扩展的桌面工具箱——按一个快捷键唤起命令面板，所有工具触手可及。

### Constraints

- **Tech stack**: 纯 Rust 原生 - 不使用 Tauri/Electron 等 Web 混合方案
- **Platform**: macOS + Windows - 跨平台是硬需求，Linux 可延后
- **Rendering**: CPU 2D 渲染 (tiny-skia) - 避免 GPU 依赖，保证可移植性
- **Architecture**: 模块化 - 新增功能不能修改核心框架代码，只通过实现 Module trait 接入
- **Compatibility**: macOS 屏幕捕获需用户授权 Screen Recording 权限
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Recommended Stack
### Core Technologies
| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Rust | 1.75+ | Core language | Memory safety, zero-cost abstractions, cross-platform, mature ecosystem for system-level desktop apps |
| winit | 0.30+ | Window creation & event loop | De facto standard for Rust windowing; cross-platform; winit 0.30 introduced new event loop architecture |
| softbuffer | 0.4+ | Raw framebuffer compositor | Minimal dependency, pairs with winit for pixel-level window content; no GPU needed |
| tiny-skia | 0.11+ | CPU 2D rendering | Pure Rust Skia subset; no native dependencies; perfect for annotation/drawing; Pathfinder-quality paths |
| egui | 0.29+ | Immediate-mode UI | Toolbars, settings panels, text input; integrates with winit; no retained-mode complexity |
| global-hotkey | 0.6+ | System-wide hotkey registration | Cross-platform; maintained by tao team; clean API for registering/unregistering hotkeys |
| tray-icon | 0.19+ | System tray icon & menu | Cross-platform; same ecosystem as tao; supports menu items, icons, tooltips |
| arboard | 3.4+ | Clipboard access | Cross-platform clipboard; supports image data for screenshot copies; simple API |
| serde + serde_json | 1.0+ | Serialization | Config files, event payloads, state persistence; universal in Rust ecosystem |
| toml | 0.8+ | Config file format | Human-readable config; idiomatic in Rust; good for per-module config sections |
### Supporting Libraries
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| screenshots | 0.5+ | Screen capture | Simplified cross-platform screen capture; wraps platform APIs |
| scrap | 0.5+ | Low-level screen capture | Alternative to screenshots; more control; useful for multi-monitor fine-tuning |
| image | 0.25+ | Image encoding/decoding | PNG/JPEG save/load; pixel format conversion; screenshot output |
| xcap | 0.4+ | Screen capture (alternative) | Newer alternative; better multi-monitor support; active development |
| parking_lot | 0.12+ | Fast synchronization primitives | Mutexes/RwLocks for thread-safe shared state; lower overhead than std |
| anyhow | 1.0+ | Error handling | Application-level error handling; ergonomic context chaining |
| thiserror | 2.0+ | Library error types | For mybox-core error types; clean derive macros |
| uuid | 1.10+ | ID generation | Window IDs, event IDs, module instance IDs |
| log + env_logger | 0.4+ / 0.11+ | Logging | Lightweight; env-based filtering; sufficient for desktop app |
| directories | 5.0+ | Platform paths | Config dir, data dir, cache dir across platforms |
| objc2 | 0.6+ | macOS platform interop | NSWindow manipulation, screen recording permissions, NSPanel for pin windows |
### Development Tools
| Tool | Purpose | Notes |
|------|---------|-------|
| cargo | Build system & package manager | Workspace support for multi-crate structure |
| cargo-edit | Dependency management | `cargo add` for quick dependency addition |
| cargo-nextest | Test runner | Faster, better output than `cargo test` |
| rust-analyzer | IDE support | LSP for VS Code / IntelliJ |
## Installation
# Initialize workspace
# Core dependencies will be added per-crate via cargo add:
# mybox-core
# Platform-specific (conditional compilation)
# Capture module
# Dev
## Alternatives Considered
| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| winit | tao | If you need closer integration with system tray/global hotkey (tao is tauri's fork of winit). However, tao lags behind winit in updates and winit 0.30 closed many gaps. |
| tiny-skia | vello | If you need GPU-accelerated rendering (Velarro uses wgpu). Overkill for annotation; adds GPU dependency. |
| tiny-skia | femtovg | If you need GPU-accelerated 2D. Similar tradeoff as vello. |
| egui | iced | If you prefer retained-mode architecture. Iced is more traditional but heavier and harder to integrate with custom rendering. |
| egui | slint | If you want declarative UI with a DSL. Slint is good but has licensing considerations for commercial use. |
| screenshots | xcap | xcap has better multi-monitor support but is newer. Either works; screenshots is more battle-tested. |
| softbuffer | pixels | If you need GPU-accelerated compositing. softbuffer is simpler for CPU rendering. |
## What NOT to Use
| Avoid | Why | Use Instead |
|-------|-----|-------------|
| Tauri | We chose pure Rust native; Tauri adds WebView runtime and JS bridge overhead | winit + tiny-skia + egui |
| Electron | Massive binary size, high memory; defeats purpose of choosing Rust | Same as above |
| fltk-rs | Works but C++ FFI adds build complexity; less modern API; limited community | egui or iced |
| glium | Abandoned/maintenance mode; wgpu ecosystem is the future | wgpu (if GPU needed) or tiny-skia (CPU) |
| druid | Effectively abandoned; Xilem is the successor but not production-ready | egui for immediate-mode UI |
| core-graphics (direct) | macOS-only; doesn't solve cross-platform | tiny-skia (cross-platform CPU) |
| GDI+ (direct) | Windows-only; poor API | tiny-skia (cross-platform CPU) |
## Stack Patterns by Variant
- Use `objc2` for platform-specific needs (NSWindow level, screen recording permission prompt, NSPanel for always-on-top pin windows)
- Use `core-graphics` crate indirectly through `screenshots`/`xcap`
- Handle `NSApplicationDelegate` for proper activation policy
- Use `windows` crate for Win32 API calls (optional, for advanced window management)
- `global-hotkey` and `tray-icon` handle most platform specifics
- DPI scaling handled by winit's `PhysicalPosition`/`LogicalPosition` system
- Prefer `xcap` over `screenshots` for better multi-monitor enumeration
- Use winit's `monitor_iter()` for display geometry
- Virtual screen coordinates for spanning windows across monitors
## Version Compatibility
| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| winit 0.30 | egui 0.29+ | Need egui-winit 0.29+ for winit 0.30 compat; check egui changelog |
| winit 0.30 | softbuffer 0.4+ | softbuffer adapted to winit 0.30's new Window trait |
| global-hotkey 0.6 | winit 0.30 | Both use raw-window-handle 0.6; compatible |
| tray-icon 0.19 | winit 0.30 | Independent event source; listen on same event loop |
| tiny-skia 0.11 | - | No platform deps; works everywhere |
## Sources
- crates.io - version verification for all recommended packages
- winit 0.30 migration guide - breaking changes and new event loop
- egui integration docs - winit integration patterns
- Snipaste, Shottr, CleanShot X - feature analysis for screenshot tools
- Raycast, Alfred - command palette interaction patterns
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
