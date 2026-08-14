# Phase 3: 命令面板 - Context

**Gathered:** 2026-08-13
**Status:** Ready for planning

<domain>
## Phase Boundary

命令面板从"不存在"变为统一交互入口：全局热键 `Cmd+Shift+Space`（Windows: `Ctrl+Shift+Space`）唤出屏幕中央浮窗，列出全部命令（模块命令 + 框架内置命令），输入关键词实时模糊过滤，方向键导航、回车执行（面板保持到命令完成），ESC 关闭。

本阶段交付：命令注册机制（`Module` trait 扩展 + `BuiltinCommands`）、palette 模块（新 crate，egui 渲染）、全局唤出热键、模糊过滤与排序、键盘导航与执行生命周期。

不在本阶段范围：命令历史/最近使用、命令排序学习、多显示器定位选择、配置热重载、IME 特殊处理、v2 模块命令（AI 对话助手等）。

</domain>

<spec_lock>
## Requirements (locked via SPEC.md)

**6 requirements are locked.** See `03-SPEC.md` for full requirements, boundaries, and acceptance criteria.

Downstream agents MUST read `03-SPEC.md` before planning or implementing. Requirements are not duplicated here.

**In scope (from SPEC.md):** `Module` trait 命令注册接口（`commands()`）与 `Command` 类型；4 个框架内置命令（退出应用/打开配置目录/重启应用/打开日志文件）；palette 模块（新 crate `crates/modules/palette`）：Floating 居中浮窗 + egui 集成；egui + egui-winit 依赖引入；fuzzy-matcher 依赖，模糊过滤 + 排序；全局唤出热键（可配置）；键盘导航（↑/↓/回车/ESC）与命令执行生命周期（面板保持到完成）；空态展示。

**Out of scope (from SPEC.md):** 命令历史 / 最近使用记录；命令排序策略（频率/学习）；多显示器定位选择；配置热重载；插件市场 / 动态模块加载；AI 对话助手等 v2 模块命令；中文 IME 组合输入特殊处理。

</spec_lock>

<decisions>
## Implementation Decisions

### egui 归属与集成方式
- **D-01:** egui/egui-winit 依赖引入 mybox-core（可 re-export 供所有模块复用）。未来模块（如 v2 AI 对话助手）依赖 core 的 egui re-export，不各自引入。
- **D-02:** egui 版本锁定 0.30（researcher 需验证与 winit 0.30 / softbuffer 的兼容组合及 CPU 软件渲染方案）。
- **D-03:** palette 模块自持有 egui 集成：通过 `WindowSpec.on_event` 把 winit 事件转发给 egui-winit，通过 `on_draw` 软件渲染到 Pixmap。核心框架代码零改动，符合"加模块不改核心"约束。
  - **D-03 细化（用户已批准，2026-08-14）：** 实测 `egui_winit::State::on_window_event`/`take_egui_input` 需要 `&Window`（egui-winit 0.30.0 源码验证），`WindowSpec.on_event` 签名提供不了 → 字面"零改动"不可行。授权 6 个**加性**核心扩展点（全部非破坏性，同类于 Phase 2 已加的 `on_draw` 字段）：
    - C1: `Module` trait 增加 `commands()` 方法（默认空实现）
    - C2: 新增 `CommandRegistry`（核心命令注册表）
    - C3: `WindowSpec` 增加 `on_event_win` 字段（补 `on_event` 缺的 `&Window` 引用）
    - C4: Floating 窗口聚焦 + 不可缩放 profile
    - C5: `AppEvent::Exit` 退出通道
    - C6: macOS 圆角窗口支持（可选，layer 技巧）
  - 意图保留：palette 仍自持有 egui 集成，模块与核心职责边界不变（架构责任映射见 RESEARCH）。

### 执行期间面板表现
- **D-04:** 命令执行期间：输入框下方显示状态行（如「正在执行：开始截图…」），列表和输入禁用（防重入），runner 完成后面板关闭。截图命令例外：SPEC 要求触发截图前先隐藏/关闭面板（避免面板被拍进截图）。
- **D-05:** 执行失败的错误提示在面板内：列表区显示错误消息，用户按任意键或 ESC 关闭面板。不使用系统通知 API。
- **D-06:** 面板窗口生命周期采用建销模式：每次唤出创建 Floating 窗口，关闭/执行完成后销毁。与 Phase 2 截图 overlay 模式一致，无残留状态。
- **D-07:** `Command.runner` 为**异步**签名（返回 Future），框架接入 async 运行时。用户明确选择异步（覆盖同步推荐），理由：为 v2 AI 对话类慢命令预留。运行时选型（tokio/smol 等）与执行线程模型由 researcher/planner 决定。

### 面板视觉形态
- **D-08:** 视觉风格参考 Raycast：大圆角卡片、大号列表项、宽松间距。
- **D-09:** 深色固定主题（背景深灰、文字浅色），与截图遮罩暗色风格一致，不跟随系统。
- **D-10:** 列表项显示：命令名称 + 灰色描述，命中关键词的字符高亮。
- **D-11:** 面板约 600px 宽固定，高度按列表条数自适应（上限约 10 行）。居中、不可 resize（SPEC 已锁）。

### 内置命令实现细节
- **D-12:** 「打开日志文件」依赖日志落盘：日志写到配置目录 `logs/mybox.log`（macOS: `~/Library/Application Support/mybox/logs/mybox.log`），应用启动即开始写文件（当前 env_logger 仅 stderr，需加文件 sink）。命令直接打开该文件，必然存在。
- **D-13:** 「重启应用」机制：spawn 当前可执行文件为新进程 + 当前进程正常退出。dev 模式（cargo run）下需处理 spawn 编译产物路径。

### the agent's Discretion
- async 运行时选型（tokio vs smol vs 其他）及 runner 执行线程模型
- egui CPU 软件渲染后端的具体方案（egui-tiny-skia 或 egui 0.30 的 softbuffer 集成等），researcher 验证
- 截图命令与面板的衔接：capture 模块注册自己的 runner（含先隐藏面板再触发截图的时序落地）
- fuzzy-matcher 的具体评分参数与关键词权重
- 面板具体视觉参数（行高、内边距、圆角、颜色值、高亮色）
- `BuiltinCommands` 的具体实现形态与 4 个内置命令的注册位置
- 「退出应用」命令复用托盘退出路径（INFRA-02 已有退出菜单）
- 热键配置键名与读取方式（沿用 D-11 字符串格式 + ConfigCenter 解析）

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 3 Spec (locked requirements)
- `.planning/phases/03-命令面板/03-SPEC.md` — Locked requirements — MUST read before planning。6 条需求 + 10 条验收标准 + 目标/背景/边界/约束。

### Project Context
- `.planning/PROJECT.md` — 项目定义、核心价值（命令面板式交互是 Core Value 载体）、约束（CPU 渲染、纯 Rust 原生）
- `.planning/REQUIREMENTS.md` — PAL-01~PAL-05 需求定义
- `.planning/ROADMAP.md` — Phase 3 目标、成功标准、计划分解（03-01 命令面板窗口+命令注册、03-02 模糊搜索+键盘导航+命令执行）

### Prior Phase Decisions (Carried Forward)
- `.planning/phases/01-framework/01-CONTEXT.md` — Phase 1 决策，特别是：
  - D-07: 多窗口集中管理 + ID 分发（palette Floating 窗口复用此机制）
  - D-08: 热键/托盘事件通过独立线程 + channel 转发到 winit 事件循环
  - D-11: 热键配置字符串格式（`"Cmd+Shift+Space"`）+ ConfigCenter 解析
- `.planning/phases/02-screenshot/02-CONTEXT.md` — Phase 2 决策与教训：
  - TestModule/capture 模式：init 中订阅 `hotkey.triggered` → 创建窗口
  - re-entrancy 教训（连续唤出无孤儿窗口、无重复热键副作用 — SPEC 验收项已引用）

### Research
- `.planning/research/STACK.md` — 技术栈与版本兼容（egui 0.29+/egui-winit 与 winit 0.30 兼容性；Phase 3 需 researcher 更新到 0.30 的验证结论）
- `.planning/research/ARCHITECTURE.md` — 系统架构、crate 结构、模块模式
- `.planning/research/PITFALLS.md` — 已知陷阱（winit 事件处理、焦点管理、macOS accessory 模式）

### Codebase
- `crates/mybox-core/src/module.rs` — Module trait（id/name/init/default_config/menu_items），需新增 `commands()` 接口
- `crates/mybox-core/src/window.rs` — WindowKind::Floating（无边框+置顶）、WindowSpec、WindowManager、focus_window（b4fa248）、non-resizable 模式（75badf3）
- `crates/mybox-core/src/hotkey.rs` — HotkeyManager::register_str / action_for_id
- `crates/mybox-core/src/event.rs` — EventBus、EventFilter（命令执行完成/失败的事件通道）
- `crates/mybox-core/src/context.rs` — ModuleContext（emit/on/windows/config/hotkeys/ui）
- `crates/mybox-core/src/app.rs` — App 事件循环、window_event 路由、focus_window 调用点
- `crates/mybox-core/src/renderer/mod.rs` — Renderer trait（draw 闭包 + present）
- `crates/mybox-core/src/config.rs` — ConfigCenter（热键字符串解析）
- `crates/modules/capture/src/lib.rs` — 截图模块（「开始截图」命令的注册方与 runner 提供方）
- `crates/mybox-app/src/main.rs` — env_logger::init()（D-12 需改为文件 sink）

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `WindowKind::Floating` — 无边框 + 置顶窗口类型，已定义并测试，palette 直接使用
- `WindowSpec` — 公开字段结构体（kind/title/inner_size/position/on_event/on_draw/cursor_icon/visible/always_on_top/decorations），palette 用结构体字面量构造
- `WindowManagerHandle` + `focus_window`（b4fa248）— 窗口创建/销毁请求队列与聚焦逻辑，palette 唤出后聚焦输入
- `EventBus` + `EventFilter` — 广播 + 通配符过滤；命令执行完成/失败结果可用事件回传面板
- `ModuleContext` — 模块与框架交互唯一入口；palette 通过 `ctx.windows().create()` 建窗
- `HotkeyManager::register_str(action, "Cmd+Shift+Space")` — 唤出热键注册（沿用 Phase 2 `Cmd+Shift+T` 模式）
- `ModuleRegistry` / `AppBuilder` — 编译期注册；palette 模块注册入口，`BuiltinCommands` 装配点
- `UiThreadProxy` — 闭包转发到 winit 主线程（runner 完成后关闭面板需用）

### Established Patterns
- **capture 模块模式（Phase 2）:** init() 中注册热键 action + 订阅 `hotkey.triggered` 事件；palette 模式：注册唤出热键 → toggle 逻辑（未显示→显示并聚焦，已显示→关闭）
- **窗口事件路由:** `WindowSpec.on_event` 闭包接收 `&WindowEvent`，App 的 window_event handler 按 winit_id 分发——egui-winit 事件转发在此回调内完成（D-03）
- **线程模型:** 事件总线在独立工作线程分发；winit 事件循环在主线程；D-07 的 async runner 执行需沿用"耗时操作不在事件循环线程"原则
- **建销窗口模式:** Phase 2 overlay 每屏一窗建销；palette 每次唤出新建、关闭销毁（D-06）

### Integration Points
- `Module` trait 新增 `commands() -> Vec<Command>` 方法 — 框架扩展点（SPEC requirement 1）
- `Command { id, name, description, keywords, runner }` 新类型，runner 为 async Future（D-07）
- `BuiltinCommands` — 4 个内置命令（退出/打开配置目录/重启/打开日志）的注册机制
- 新 crate `crates/modules/palette/` — egui 依赖经 mybox-core re-export（D-01）
- capture 模块注册「开始截图」命令，runner 内先请求隐藏面板再触发截图（SPEC 约束：面板可见会被拍进截图）
- AppBuilder 启动流程：注册 palette 模块 + BuiltinCommands + 唤出热键
- ConfigCenter：palette 热键从配置读取（默认 `Cmd+Shift+Space`，覆盖后重启生效）

### Known Gaps (Phase 3 Must Address)
- `Module` trait 无命令概念 — 需扩展接口（改 trait 是允许的：SPEC 明确命令注册是框架扩展点）
- 无 egui/egui-winit/egui-tiny-skia 依赖 — 需引入并验证与 winit 0.30 + CPU 渲染约束的兼容
- 无 async 运行时 — D-07 需要引入（tokio/smol 等，由 researcher 选型）
- `env_logger::init()` 仅 stderr — D-12 需加文件 sink（logs/mybox.log）
- 无 `fuzzy-matcher` 依赖 — 需引入
- 无"当前活动显示器"查询封装 — 面板居中定位需 monitor 信息（winit `monitor_iter` / 光标位置）

</code_context>

<specifics>
## Specific Ideas

- 视觉风格明确参考 Raycast：大圆角卡片、大号列表项、宽松间距、深色主题（PROJECT.md 亦提及 Raycast/Alfred 为交互参考）。
- 用户选择异步 runner 覆盖了推荐的同步方案——明确理由是为 v2 AI 对话命令预留，异步是长期方向，不是当前需要。
- 截图命令执行时序是用户最关注的边界：面板必须先隐藏再触发截图，否则面板会被拍进截图（SPEC 约束 + 验收项）。
- Phase 2 的 re-entrancy 教训被 SPEC 验收项显式引用（连续唤出 5 次无窗口残留）——建销模式（D-06）是为此选择的。

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 3-命令面板*
*Context gathered: 2026-08-13*
