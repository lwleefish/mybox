---
phase: 03-命令面板
plan: 01
subsystem: ui
tags: [egui, egui-winit, tiny-skia, command-registry, hotkey, palette]

# Dependency graph
requires:
  - phase: 01-framework
    provides: [Module trait, EventBus, WindowManager, ModuleContext, renderer]
  - phase: 02-screenshot
    provides: [capture start_capture 流程、sample_context 测试模板、re-entrancy 纪律]
provides:
  - CommandRegistry + BuiltinCommands（4 内置命令）+ Module::commands() 接口
  - WindowSpec.on_event_win / Floating non-resizable+focus / round_floating_corners
  - mybox-palette crate：热键唤出、建销生命周期、egui→tiny-skia 软件光栅化、CJK 字体、活动显示器居中
affects: [03-02 模糊搜索/键盘导航/命令执行, Phase 4 跨平台完善]

# Tech tracking
tech-stack:
  added: [egui 0.30.0, egui-winit 0.30.0, fuzzy-matcher 0.3.7, pollster 1.0.1, objc2-app-kit 0.3.2, objc2-quartz-core 0.2.2]
  patterns: [CommandRunner = Arc<dyn Fn() -> Pin<Box<dyn Future>>>, UiThreadProxy 回跳, OnceLock 注入 ui/windows, egui 经 core re-export]

key-files:
  created: [crates/mybox-core/src/command.rs, crates/modules/palette/src/lib.rs, crates/modules/palette/src/session.rs, crates/modules/palette/src/position.rs, crates/modules/palette/src/fonts.rs, crates/modules/palette/src/raster.rs, crates/modules/palette/src/ui.rs]
  modified: [crates/mybox-core/src/module.rs, crates/mybox-core/src/context.rs, crates/mybox-core/src/app.rs, crates/mybox-core/src/window.rs, crates/mybox-core/src/error.rs, crates/mybox-core/src/lib.rs, crates/modules/capture/src/lib.rs, crates/mybox-app/src/main.rs]

key-decisions:
  - "egui/egui-winit/fuzzy-matcher/pollster 由 mybox-core 引入并 re-export，palette 模块零直接依赖（FRMW-02 边界）"
  - "objc2-quartz-core 锁定 0.2.2 行（与 objc2-app-kit 0.2.2 同版，避免跨 0.2/0.3 混用崩溃）"
  - "内置命令 runner 为 Box::pin(async) 无真实 await，run_command 每调用一线程 pollster::block_on 驱动"
  - "capture.start 命令 hide_before_execute=true，面板绝不被拍进截图"

patterns-established:
  - "OnceLock 注入 ui/windows：commands() 只有 &self，init 时 set，runner 闭包 clone Arc"
  - "模块热键注册经 ctx.ui().run 延迟（HotkeyManager 需 init 后）"
  - "window-created 配对 + pending_close 即时销毁（capture torn_down_pending 同形）"
  - "session 状态机全方法化便于 headless 测试（summon/close/set_window_id 等）"

requirements-completed: [PAL-01, PAL-02]

# Metrics
duration: ~60min
completed: 2026-08-15
---

# Phase 03-01: 命令面板窗口 + 命令注册系统 Summary

**全局热键 Cmd+Shift+Space 唤出屏幕中央 Floating 浮窗（egui 渲染、CJK 字体、大圆角）并列出全部已注册命令（5+ 个），配套 CommandRegistry 命令注册系统与 4 个框架内置命令**

## Performance

- **Duration:** ~60 min
- **Started:** 2026-08-15
- **Completed:** 2026-08-15
- **Tasks:** 4
- **Files modified:** 21

## Accomplishments

- 核心命令注册系统：`Command`/`CommandRegistry`/`BuiltinCommands`（退出/打开配置目录/重启/打开日志）+ `Module::commands()` trait 接口 + AppBuilder 装配（模块命令在前、内置在后，≥5 命令）
- 三个加性框架窗口扩展：`WindowSpec.on_event_win`（egui-winit 需要 &Window）、Floating 聚焦 + 不可 resize、macOS NSWindow 12px 圆角（layer cornerRadius）
- 新 crate `crates/modules/palette`：六态会话状态机（generation/pending_close 配对）、活动显示器居中定位（NSPoint 原点翻转）、Hiragino CJK 字体、egui→tiny-skia 软件光栅化（重心坐标 + 双线性 UV）
- capture 模块注册「开始截图」命令（capture.start，keywords 含 jietu，hide_before_execute=true）
- mybox-app 双路日志（TeeWriter：stderr + <config>/logs/mybox.log），启动即写文件

## Task Commits

Each task was committed atomically:

1. **Task 1: 依赖引入 + 核心命令注册系统（C1/C2/C5）** - `8eace3e` (feat)
2. **Task 2: 框架窗口扩展（C3/C4/C6）+ capture 命令注册 + 双路日志（D-12）** - `734638f` (feat)
3. **Task 3: palette 模块骨架 + 会话状态机 + 定位 + 字体 + 热键/建销生命周期** - `8024544` (feat)
4. **Task 4: egui 渲染全链路（raster/ui/on_event_win 接线）** - `087b5cc` (feat)

## Files Created/Modified

- `crates/mybox-core/src/command.rs` - Command/CommandRegistry/BuiltinCommands/run_command（命名线程 + pollster::block_on + UiThreadProxy 回跳）
- `crates/mybox-core/src/module.rs` - Module trait `commands()` 默认方法
- `crates/mybox-core/src/context.rs` - ModuleContext 增加 commands 注册表
- `crates/mybox-core/src/app.rs` - AppEvent::Exit + app-exit 转发 + Floating focus + 注册表装配
- `crates/mybox-core/src/window.rs` - on_event_win 字段 + Floating non-resizable + round_floating_corners
- `crates/modules/palette/src/{lib,session,position,fonts,raster,ui}.rs` - palette 模块全链路
- `crates/modules/capture/src/lib.rs` - capture.start 命令注册
- `crates/mybox-app/src/main.rs` - TeeWriter 双路日志 + palette 模块注册

## Decisions Made

- egui 归属 mybox-core 并 re-export（D-01），palette 模块经 `mybox_core::egui` 使用——模块边界纪律（FRMW-02）
- 内置命令 runner 采用 `Box::pin(async)` 无真实 await + 命名线程 pollster::block_on（D-07）
- restart 命令先 spawn current_exe 再发 app-exit（D-13，current_exe 天然解析 cargo run 产物路径）

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- 执行代理中途被取消，4 个任务提交已完成且测试全绿（177 passed / 8 skipped）；由编排器按 safe-resume 路径补齐 SUMMARY.md 与追踪更新。

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 03-02 就绪：PaletteSession 六态（Idle/Filtering/Empty/Executing/Error）、filtered 下标、`ui.get()` 注入接线已在 03-01 完成
- 面板渲染链路（on_event_win 帧循环 → raster::paint → on_draw blit）已验证，03-02 只需叠加过滤/导航/执行逻辑

---
*Phase: 03-命令面板*
*Completed: 2026-08-15*
