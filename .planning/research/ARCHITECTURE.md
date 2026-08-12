# Architecture Research

**Domain:** Rust native desktop toolbox (modular plugin architecture)
**Researched:** 2026-08-11
**Confidence:** HIGH

## Standard Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     Application Layer                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │   mybox-app   │  │  Tray + Menu │  │  Event Loop   │       │
│  │  (binary)     │  │  (托盘驻留)   │  │  (winit)      │       │
│  └──────┬───────┘  └──────────────┘  └──────────────┘       │
│         │                                                    │
├─────────┴────────────────────────────────────────────────────┤
│                    Framework Layer (mybox-core)               │
│  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐    │
│  │  Module    │ │  Event    │ │  Window   │ │  Hotkey   │    │
│  │  Registry  │ │  Bus      │ │  Manager  │ │  Manager  │    │
│  └───────────┘ └───────────┘ └───────────┘ └───────────┘    │
│  ┌───────────┐ ┌───────────┐ ┌───────────┐                  │
│  │  Config    │ │  State    │ │  Renderer │                  │
│  │  Center    │ │  Store    │ │  (shared) │                  │
│  └───────────┘ └───────────┘ └───────────┘                  │
├─────────────────────────────────────────────────────────────┤
│                       Module Layer                           │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐        │
│  │ capture  │ │ palette  │ │ color    │ │  ...     │        │
│  │ (截图)   │ │ (命令面板)│ │ (取色器) │ │ (future) │        │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘        │
│  Each module: implements Module trait, compiled into binary  │
├─────────────────────────────────────────────────────────────┤
│                    Platform Abstraction                      │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐        │
│  │  winit   │ │tiny-skia │ │  egui    │ │ arboard  │        │
│  │ (window) │ │ (render) │ │  (UI)    │ │(clipboard)│       │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘        │
└─────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| Module Registry | 注册/枚举/依赖解析模块 | Builder pattern + Vec<Box<dyn Module>> |
| Event Bus | 模块间异步解耦通信 | Channel-based pub/sub; serde_json payloads |
| Window Manager | 创建/销毁/管理多窗口 | winit WindowBuilder 封装; WindowSpec 抽象 |
| Hotkey Manager | 注册/监听全局热键 | global-hotkey crate; 事件循环集成 |
| Config Center | 分节配置读写 | TOML 文件 + serde; 按模块 ID 命名空间 |
| State Store | 跨模块共享运行时状态 | Arc<RwLock<HashMap<String, Value>>> |
| Renderer | 共享 2D 渲染上下文 | tiny-skia Pixmap + 绘制 API 封装 |
| Tray | 系统托盘 + 右键菜单 | tray-icon crate; 动态菜单项 |

## Recommended Project Structure

```
mybox/
├── Cargo.toml              # workspace 根
├── .planning/              # GSD 规划文件
├── crates/
│   ├── mybox-core/         # 框架核心
│   │   └── src/
│   │       ├── lib.rs      # 公开 API: App, AppBuilder, Module trait
│   │       ├── module.rs   # Module trait 定义
│   │       ├── context.rs  # ModuleContext - 模块与框架交互入口
│   │       ├── event.rs    # EventBus + Event + EventFilter
│   │       ├── window.rs   # WindowManager + WindowSpec + WindowKind
│   │       ├── hotkey.rs   # HotkeyManager + HotKey 类型
│   │       ├── config.rs   # ConfigCenter + 分节读写
│   │       ├── state.rs    # StateStore - 共享运行时状态
│   │       ├── tray.rs     # TrayManager + 菜单构建
│   │       └── error.rs    # 统一错误类型
│   ├── mybox-app/          # 二进制入口
│   │   └── src/
│   │       └── main.rs     # App::builder().module(...).build().run()
│   └── modules/            # 功能模块
│       ├── capture/        # 截图模块
│       │   └── src/
│       │       ├── lib.rs      # CaptureModule + Module impl
│       │       ├── overlay.rs  # 全屏覆盖窗口 + 区域选择
│       │       ├── annotate.rs # 标注工具
│       │       ├── pin.rs      # Pin 浮窗
│       │       └── magnifier.rs# 放大镜
│       ├── palette/        # 命令面板模块 (v1)
│       │   └── src/
│       │       └── lib.rs
│       ├── color/          # 颜色拾取 (v1.x)
│       │   └── src/
│       │       └── lib.rs
│       └── ...             # 未来模块
```

### Structure Rationale

- **crates/mybox-core:** 框架核心，零业务逻辑；模块只依赖 core 的公开 API
- **crates/mybox-app:** 极薄入口；只负责组装框架 + 注册模块 + 启动事件循环
- **crates/modules/:** 每个模块独立 crate；可独立编译测试；加模块 = 新建 crate + 一行注册
- **模块间无直接依赖:** 模块不互相 `cargo add`，只通过事件总线通信

## Architectural Patterns

### Pattern 1: Module Trait + Builder Registration

**What:** 所有功能模块实现统一的 `Module` trait，通过 `AppBuilder::module()` 注册
**When to use:** 总是--这是框架的核心扩展点
**Trade-offs:** 编译期注册（不能热加载）换取类型安全和零运行时开销

**Example:**
```rust
pub trait Module: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn name(&self) -> &str;
    fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()>;
    fn menu_items(&self) -> Vec<MenuItem> { vec![] }
    fn depends_on(&self) -> &[&str] { &[] }
}

// main.rs
let app = App::builder()
    .module(CaptureModule::new())
    .module(PaletteModule::new())
    .build()?;
app.run()?;
```

### Pattern 2: Event Bus Pub/Sub

**What:** 模块间通过事件总线通信，不直接调用彼此 API
**When to use:** 任何跨模块交互
**Trade-offs:** 运行时类型安全较弱（JSON payload）换取完全解耦

**Example:**
```rust
// 截图完成后发送事件
ctx.emit(Event {
    from: "capture",
    kind: "screenshot-taken",
    payload: json!({ "path": "/tmp/shot.png" }),
});

// 未来某模块监听
ctx.on(EventFilter::kind("capture", "screenshot-taken"), |e| {
    // 上传、索引、或其他处理
});
```

### Pattern 3: Window Spec Abstraction

**What:** 统一窗口创建接口，不同窗口类型（覆盖层/浮窗/面板）通过 WindowSpec 参数区分
**When to use:** 任何模块需要创建窗口时
**Trade-offs:** 抽象层可能限制某些平台特殊窗口特性的使用

**Example:**
```rust
let win_id = ctx.windows().create(WindowSpec {
    kind: WindowKind::Overlay,
    transparent: true,
    always_on_top: true,
    decorations: false,
    ..Default::default()
})?;
```

### Pattern 4: Shared Event Loop

**What:** winit 事件循环是唯一的，所有窗口、热键、托盘事件都在同一个循环中处理
**When to use:** 总是--winit 的设计约束
**Trade-offs:** 单线程事件处理；耗时操作必须 offload 到独立线程

**Example:**
```rust
// 事件循环中分发不同来源的事件
match event {
    WindowEvent { window_id, event } => { /* 窗口事件 */ }
    GlobalHotKeyEvent(hotkey_id) => { /* 热键触发 */ }
    TrayIconEvent { event } => { /* 托盘事件 */ }
}
```

## Data Flow

### 截图流程

```
用户按 F1
    ↓
HotkeyManager -> EventBus(hotkey.triggered)
    ↓
CaptureModule 接收 -> 启动截图流程
    ↓
屏幕捕获 (screenshots/xcap) -> 存储到内存
    ↓
WindowManager 创建 Overlay 窗口 (全屏透明置顶)
    ↓
用户拖拽选区 -> tiny-skia 实时绘制遮罩 + 选框
    ↓
用户松开鼠标 -> 进入标注模式 (可选)
    ↓
用户确认 (回车/双击)
    ↓
裁剪选区图像 -> arboard 复制到剪贴板
    ↓
EventBus 发送 screenshot-taken 事件
    ↓
WindowManager 销毁 Overlay 窗口
```

### 命令面板流程

```
用户按全局快捷键 (如 Cmd+Space)
    ↓
HotkeyManager -> PaletteModule
    ↓
WindowManager 创建 Panel 窗口 (居中浮窗)
    ↓
从 ModuleRegistry 收集所有模块的 menu_items()
    ↓
用户输入 -> 模糊过滤命令列表
    ↓
用户选择 -> EventBus 发送 command.executed
    ↓
对应模块接收事件 -> 执行命令
    ↓
WindowManager 销毁 Panel 窗口
```

### Key Data Flows

1. **热键触发:** HotkeyManager -> EventBus -> Module callback -> Window creation
2. **模块间通信:** Module A -> EventBus -> Module B (完全解耦)
3. **窗口渲染:** WindowManager -> Pixmap (tiny-skia) -> softbuffer -> Screen
4. **配置读取:** Module -> ConfigCenter -> TOML file (启动时加载到内存)

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| 1-5 模块 | 单进程，编译期注册，所有模块在同一 binary |
| 5-15 模块 | 考虑模块懒加载（init 延迟到首次使用） |
| 15+ 模块 | 考虑 WASM 插件系统；进程隔离关键模块 |

### Scaling Priorities

1. **First bottleneck:** 事件总线消息量--模块多了事件可能拥堵。解决：事件过滤 + 有针对性的订阅
2. **Second bottleneck:** 内存--每个 Pin 浮窗持有一张截图。解决：大图压缩；限制 Pin 数量

## Anti-Patterns

### Anti-Pattern 1: 模块直接依赖

**What people do:** `capture` crate 直接 `cargo add` `palette` crate 并调用其 API
**Why it's wrong:** 破坏解耦；删除/替换一个模块需要改其他模块
**Do this instead:** 通过事件总线通信；模块间零直接依赖

### Anti-Pattern 2: 主线程渲染阻塞

**What people do:** 在 winit 事件循环中执行耗时的屏幕捕获
**Why it's wrong:** 阻塞 UI 渲染；选区拖动卡顿
**Do this instead:** 在独立线程捕获屏幕，通过 channel 传回结果；事件循环只负责渲染

### Anti-Pattern 3: 每个模块引一套渲染

**What people do:** capture 用 tiny-skia，palette 用 egui，color 用 vello
**Why it's wrong:** 二进制体积膨胀；渲染上下文无法共享
**Do this instead:** mybox-core 提供共享渲染抽象；模块按需使用 tiny-skia 或 egui

### Anti-Pattern 4: 平台代码散落各处

**What people do:** `#[cfg(target_os = "macos")]` 散布在模块代码中
**Why it's wrong:** 难以维护；平台行为不一致
**Do this instead:** 平台特定代码集中在 core 的 platform 模块；模块只调用抽象 API

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| LLM API (OpenAI/Claude) | HTTP client (reqwest) | AI 助手模块直接调用；API key 存配置 |
| macOS Screen Recording | objc2 + CGDisplayStream | 需用户授权；首次使用弹权限请求 |
| Windows Graphics Capture | windows crate | Windows 10+ Graphics Capture API |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| core ↔ module | Module trait + ModuleContext | 模块只依赖 core 的公开 API |
| module ↔ module | EventBus (pub/sub) | 零直接依赖；事件 JSON payload |
| core ↔ platform | cfg + 平台 abstraction crate | 平台差异隔离在 core 内部 |
| event loop ↔ worker threads | crossbeam/tokio channels | 耗时操作 offload；结果回传事件循环 |

## Sources

- winit 0.30 architecture documentation
- Raycast plugin architecture (命令面板 + 模块交互参考)
- tao/tauri architecture (窗口管理 + 托盘集成参考)
- Snipaste 技术博客 (截图覆盖窗口实现思路)
- Rust workspace patterns (crates.io 官方文档)

---
*Architecture research for: Rust native desktop toolbox*
*Researched: 2026-08-11*
