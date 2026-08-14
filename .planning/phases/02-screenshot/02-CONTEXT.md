# Phase 2: 截图模块 - Context

**Gathered:** 2026-08-12
**Status:** Ready for planning

<domain>
## Phase Boundary

实现完整截图功能作为第一个真实业务模块，验证 Phase 1 框架可用性。交付内容：屏幕捕获（所有显示器）、全屏透明覆盖窗口、鼠标拖拽区域选择（含调整手柄）、标注工具（矩形、箭头、画笔、文字）、撤销、剪贴板复制、macOS 屏幕录制权限引导。

不在本阶段范围：Pin 浮窗（v2）、放大镜（v2）、马赛克标注（v2）、多显示器选区跨越（v2）、截图保存到文件（v2）、命令面板（Phase 3）。

</domain>

<decisions>
## Implementation Decisions

### 截图流程编排
- **D-01:** 整体流程为 Select -> Annotate -> Confirm。用户按热键触发截图 -> 捕获屏幕画面 -> 覆盖窗口出现 -> 拖拽选择区域 -> 标注工具栏出现 -> 用户可选标注 -> 按 Enter 或工具栏确认按钮 -> 当前图像（含标注）复制到剪贴板 -> 覆盖窗口关闭。不标注直接确认则复制原始选区图像。
- **D-02:** 选择区域后进入"可调整选择"阶段。拖拽完成后显示 8 个拖拽手柄（四角 + 四边中点），用户可在开始标注前或标注过程中随时调整选区位置和大小。不设显式模式切换。
- **D-03:** 工具栏采用统一模式（no modes）。拖拽完成后在选区附近显示工具栏，同时包含标注工具按钮和选择手柄。用户可拖拽手柄调整选区，或点击工具按钮开始标注——当前工具选择决定操作行为，无需显式模式切换。选区手柄和标注工具可同时使用。
- **D-04:** 确认方式为 Enter 键或工具栏确认按钮。确认后当前选区图像（含标注）复制到剪贴板，覆盖窗口立即关闭。ESC 键取消整个截图流程，覆盖窗口立即关闭，不复制任何内容。行为简单可预测——ESC 不是分步撤销，而是一键取消全部。

### Claude's Discretion
- 屏幕捕获库选择（xcap vs screenshots vs scrap）— 技术决策，由 researcher/planner 决定
- 标注工具的具体绘制实现（tiny-skia 路径/形状 API）
- 工具栏的 UI 布局和视觉设计（egui 集成方式）
- 撤销栈的内部数据结构
- 选区手柄的视觉样式
- 尺寸标签（WxH）的显示位置和格式
- 覆盖窗口的渲染管线集成方式（如何将捕获画面 + 遮罩 + 选区 + 标注通过 Renderer::draw 闭包合成）
- batch_create 的真实实现方式（D-09 每屏一窗策略落地）
- macOS 权限检测的具体 API 调用（CGPreflightScreenCaptureAccess 等）
- 剪贴板复制的具体实现（arboard 库集成）

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Context
- `.planning/PROJECT.md` - 项目定义、核心价值、关键决策表
- `.planning/REQUIREMENTS.md` - v1 需求列表，Phase 2 对应 CAP-01~CAP-08
- `.planning/ROADMAP.md` - Phase 2 目标、成功标准、计划分解（02-01/02-02/02-03）

### Phase 1 Decisions (Carried Forward)
- `.planning/phases/01-framework/01-CONTEXT.md` - Phase 1 实现决策，特别是：
  - D-01/D-02/D-03: tiny-skia 统一渲染架构，Renderer trait 抽象
  - D-07: 多窗口集中管理 + ID 分发
  - D-09: 截图覆盖窗口"每屏一窗"策略
  - D-08: 热键/托盘事件通过独立线程 + channel 转发到 winit 事件循环

### Research
- `.planning/research/PITFALLS.md` - 关键陷阱：
  - Pitfall 2: macOS Screen Recording 权限被拒（Phase 2 直接相关）
  - Pitfall 3: 全屏覆盖窗口在多显示器下不覆盖（Phase 2 直接相关）
  - Pitfall 4: 事件循环中执行耗时操作导致卡顿（截图捕获需在独立线程）
- `.planning/research/ARCHITECTURE.md` - 系统架构图、项目结构（crates/modules/capture/）、模块模式
- `.planning/research/STACK.md` - 推荐技术栈和版本兼容性

### Codebase
- `crates/mybox-core/src/window.rs` - WindowKind::Overlay, WindowSpec, WindowManager, WindowManagerHandle, batch_create (placeholder)
- `crates/mybox-core/src/renderer/mod.rs` - Renderer trait (draw closure, resize, present)
- `crates/mybox-core/src/event.rs` - EventBus, EventFilter, EventPayload (Framework + Module)
- `crates/mybox-core/src/context.rs` - ModuleContext (emit/on/windows/config/hotkeys/ui)
- `crates/mybox-core/src/module.rs` - Module trait (id/name/init/default_config/menu_items)
- `crates/modules/test/src/lib.rs` - TestModule 模式参考（订阅 hotkey.triggered -> 创建窗口）
- `crates/mybox-core/src/app.rs` - App 事件循环、create_window、window_event 路由

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `WindowKind::Overlay` - 透明、无装饰、置顶的覆盖窗口类型，已定义并测试
- `WindowSpec` - 公开字段结构体，模块可用结构体字面量构造（kind/title/inner_size/position/on_event）
- `WindowManagerHandle` - 线程安全的窗口创建/销毁请求队列，模块通过 `ctx.windows().create()` 发起
- `Renderer` trait - `draw` 闭包接收 `tiny_skia::PixmapMut`，模块通过此接口绘制自定义内容
- `EventBus` + `EventFilter` - 广播 + 通配符过滤，模块用 `ctx.on(filter, handler)` 订阅事件
- `ModuleContext` - 模块与框架交互的唯一入口（emit/on/windows/config/hotkeys/ui）
- `Module` trait - 扩展点，模块实现 id/name/init/default_config/menu_items
- `UiThreadProxy` - 将闭包转发到 winit 主线程执行（`ctx.ui().run(Box::new(...))`）
- `premul_rgba_to_u32` - 像素格式转换工具函数

### Established Patterns
- **TestModule 模式:** 模块在 `init()` 中订阅 `hotkey.triggered` 事件，检查 action 名称，通过 `ctx.windows().create()` 创建窗口。截图模块应遵循同样模式：注册热键 action（如 `start_screenshot`），在事件回调中触发截图流程。
- **窗口事件路由:** `WindowSpec.on_event` 闭包接收 `&WindowEvent`，由 App 的 `window_event` handler 按 winit_id 查找对应 WindowState 后调用。截图模块的鼠标/键盘交互通过此回调处理。
- **线程模型:** 事件总线在独立工作线程分发；winit 窗口和事件循环在主线程；耗时操作（屏幕捕获）必须在独立线程执行，结果通过 channel 或 `UiThreadProxy` 传回主线程。
- **Renderer 集成:** `Renderer::draw` 闭包接收 `PixmapMut`——截图模块在此闭包内绘制捕获画面、遮罩、选区边框、标注。但当前 App 的 `window_event` handler 在 `RedrawRequested` 时只调用 `renderer.present()`，不调用 `draw()`。Phase 2 需要补充 draw 调用链。

### Integration Points
- `ctx.windows().create(WindowSpec { kind: Overlay, ... })` - 创建截图覆盖窗口
- `WindowManager::batch_create` - Phase 1 placeholder，Phase 2 需实现真实的每显示器多窗口创建（D-09）
- `Renderer::draw` 闭包 - 截图画面的渲染入口
- `ctx.on(EventFilter::kind("core", "hotkey.triggered"), ...)` - 监听截图热键
- `ctx.config()` - 读取截图模块配置（热键、默认工具等）
- `ctx.hotkeys()` - 注册截图热键
- App 的 `about_to_wait` / `window_event` - 窗口事件处理和渲染循环
- 新建 crate: `crates/modules/capture/` - 截图模块独立 crate

### Known Gaps (Phase 2 Must Address)
- `batch_create` 仅返回占位 ID，未创建真实窗口
- App 的 `RedrawRequested` handler 只调 `present()`，未调 `draw()`——无内容渲染
- 无 `arboard` 依赖（剪贴板）
- 无 `xcap`/`screenshots` 依赖（屏幕捕获）
- 无 `egui` 依赖（工具栏 UI）
- `WindowSpec.on_event` 回调存在但 TestModule 未使用鼠标/键盘事件——Phase 2 是首个真实使用者

</code_context>

<specifics>
## Specific Ideas

- 用户选择了"统一工具栏（no modes）"模式——工具栏同时提供选区手柄和标注工具，当前工具选择决定操作行为。这是参考 Snipaste/Shottr 的交互模式。
- 用户偏好简单可预测的确认/取消行为：Enter 确认复制，ESC 一键取消全部。不需要分步 ESC。

</specifics>

<deferred>
## Deferred Ideas

None - discussion stayed within phase scope.

</deferred>

---

*Phase: 2-截图模块*
*Context gathered: 2026-08-12*
