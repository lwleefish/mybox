# Phase 1: 框架核心 - Context

**Gathered:** 2026-08-11
**Status:** Ready for planning

<domain>
## Phase Boundary

搭建 mybox 的模块化框架基础设施：Module trait、事件总线、窗口管理器、热键管理器、系统托盘、配置中心。Phase 1 交付一个可运行的空壳应用（托盘驻留 + 全局热键 + 窗口创建 + 配置读写），用测试模块验证框架可用性。不包含任何业务功能模块（截图、命令面板等在后续 Phase）。

</domain>

<decisions>
## Implementation Decisions

### 渲染架构
- **D-01:** 统一使用 tiny-skia 作为所有窗口的底层渲染目标。egui 通过 egui-tiny-skia 集成，渲染到同一个 tiny-skia Pixmap，再通过 softbuffer 上屏。不使用 GPU 渲染（如 Zed 的 GPUI/wgpu 方案）。
- **D-02:** 每个窗口共用一个 tiny-skia Pixmap 进行合成。先画自定义内容（tiny-skia 路径），再叠加 egui UI 层，最后 softbuffer 上屏。
- **D-03:** mybox-core 封装 Renderer 抽象，内部管理 Pixmap + egui 集成 + softbuffer。模块通过 Renderer API 绘制自定义内容，egui 状态由 core 管理。模块开发者看到的是 `Renderer` trait，不直接接触 tiny-skia/egui/softbuffer。

### 事件总线设计
- **D-04:** 事件总线使用异步 channel 模型。发布者通过 channel 发送事件，后台线程接收并分发。不阻塞事件循环线程。
- **D-05:** 事件分发采用广播 + 通配符过滤。订阅者用 `EventFilter` 注册过滤器（如 `capture:*` 匹配所有 capture 模块事件，`*:*` 匹配全部）。每个事件都会广播到所有订阅者，由过滤器决定是否处理。
- **D-06:** 事件 payload 采用混合方案：核心框架事件（如窗口创建/销毁、热键触发、模块加载）使用强类型枚举；模块自定义事件使用 `serde_json::Value`。Event 结构体包含 `from: &'static str`（模块 ID）、`kind: &'static str`（事件类型）、`payload: EventPayload`（枚举， either typed or JSON）。

### 事件循环 + 多窗口
- **D-07:** 多窗口采用集中管理 + ID 分发。WindowManager 内部维护 `HashMap<WindowId, WindowState>`，winit 事件循环收到窗口事件后根据 `window_id` 查找对应的 WindowState，分发给该窗口的回调函数。
- **D-08:** global-hotkey 和 tray-icon 事件通过独立线程监听，再通过 channel 发送到 winit 事件循环处理。不使用 winit 的 UserEvent 集成，保持热键/托盘与窗口事件的解耦。
- **D-09:** 截图覆盖窗口在多显示器下采用"每屏一窗"策略：为每个显示器创建独立的无边框透明置顶窗口，手动定位。WindowManager 需要支持批量创建一组关联窗口并统一管理其生命周期。

### 配置系统
- **D-10:** 配置采用全量内存缓存策略。启动时一次性加载 TOML 文件到内存，读取时直接访问内存。写入时全量写回文件。不做文件 watch 热重载（v2 考虑）。
- **D-11:** 热键配置使用字符串格式，如 `hotkey = "F1"` 或 `hotkey = "Cmd+Shift+S"`。ConfigCenter 负责解析字符串为 `HotKey` 类型。
- **D-12:** 首次启动时自动生成默认配置文件。文件包含所有已注册模块的默认配置节，用户可参考修改。
- **D-13:** 每个模块通过 `Module::default_config()` 方法提供自己的默认配置。ConfigCenter 在首次启动时收集所有模块的默认配置，合并生成完整的 `config.toml`。

### Claude's Discretion
- Renderer API 的具体方法签名和绘制接口设计
- EventPayload 枚举的具体变体划分
- WindowState 的内部结构
- channel 的具体实现选择（crossbeam vs tokio vs std）
- 配置文件的分节命名规则细节

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project Context
- `.planning/PROJECT.md` - 项目定义、核心价值、关键决策
- `.planning/REQUIREMENTS.md` - v1 需求列表，Phase 1 对应 FRMW-01~06, INFRA-01~04
- `.planning/ROADMAP.md` - Phase 1 目标和成功标准

### Research
- `.planning/research/STACK.md` - 推荐技术栈和版本兼容性
- `.planning/research/ARCHITECTURE.md` - 系统架构图、项目结构、设计模式
- `.planning/research/PITFALLS.md` - winit 0.30 breaking changes、macOS 权限、渲染冲突等

No external specs - requirements fully captured in decisions above.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- None - greenfield project, no existing code.

### Established Patterns
- None - first phase, establishing patterns for future phases.

### Integration Points
- This phase creates the foundation that all future modules will build on.
- Module trait, EventBus, WindowManager, Renderer are the primary APIs future modules use.
- The workspace structure (mybox-core, mybox-app, modules/) is established here.

</code_context>

<specifics>
## Specific Ideas

- 用户提到了 Zed 的渲染方式作为参考，最终选择了 tiny-skia 统一渲染（而非 GPU），但关注渲染统一性。
- 事件总线选择异步 channel 体现了对事件循环不阻塞的重视（与 FRMW-05 一致）。
- 配置系统的 `Module::default_config()` 模式确保模块自包含，符合"加模块不改核心"的设计理念。

</specifics>

<deferred>
## Deferred Ideas

None - discussion stayed within phase scope.

</deferred>

---

*Phase: 1-框架核心*
*Context gathered: 2026-08-11*
