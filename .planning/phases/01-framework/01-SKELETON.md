# Phase 1 Walking Skeleton — mybox 框架核心

**Status:** Planning (execution pending)
**Mode:** MVP / Walking Skeleton
**Phase:** 1-框架核心

## Capability Proven End-to-End

用户启动 mybox，系统托盘出现 mybox 图标（Dock 无应用项）；按下注册的全局热键 Cmd+Shift+T，一个标题为 "mybox test" 的 400×300 测试窗口出现——证明整个框架栈（Module trait → AppBuilder → EventBus → HotkeyManager → UiThreadProxy → WindowManager → Renderer → softbuffer）端到端打通；同时配置文件在用户目录自动生成并正确读写。

## Architectural Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Framework | winit 0.30.13 (pinned) | ApplicationHandler trait；`with_user_event::<AppEvent>()` + EventLoopProxy；macOS Accessory 内建（无需 objc2）。0.31 仅为 beta，禁用。 |
| Rendering | tiny-skia 0.12.0 + softbuffer 0.4.8 | CPU 2D，无 GPU/原生依赖；Pixmap premul RGBA → softbuffer 0x00RRGGBB。egui 推迟到 Phase 3（egui-tiny-skia 不存在）。 |
| Event bus | crossbeam-channel 0.5.16 (unbounded) + 工作线程分发 | 已随 global-hotkey/tray-icon 在依赖树；MPMC；emit 非阻塞（FRMW-05）。 |
| Windows | WindowSpec 抽象 + WindowManager 状态表 | Overlay/Floating/Panel 三种类型统一；u64 自增 WindowId；winit 窗口主线程绑定。 |
| Hotkeys | global-hotkey 0.8.0 + `HotKey: FromStr` | 配置字符串 `"Cmd+Shift+T"` 直接解析（D-11 无需自写解析器）；manager 主线程创建。 |
| Tray | tray-icon 0.24.2 + muda 0.19.3 + 运行时图标 | tiny-skia 生成单色图标，零资源文件；菜单由模块 menu_items 动态组装。 |
| Config | toml 1.1.4 + directories 6.0.0 | `ProjectDirs::from("", "", "mybox")` → `~/Library/Application Support/mybox/config.toml`；内存缓存 + 全量写回；首次生成。 |
| Module system | Module trait + ModuleContext + 编译期 AppBuilder 注册 | 模块只依赖 mybox-core 公共 API；模块间零直接依赖，仅经事件总线。 |
| Threading | bus 工作线程分发 + UiThreadProxy 转发 | 纯逻辑 handler 在 bus 线程；触碰窗口的 handler 经 UiThreadProxy 到主线程。 |
| Errors | thiserror (core) + anyhow (app 边界) + log | INFRA-03：类型化错误 + 关键操作日志。 |
| Workspace | Cargo workspace：mybox-core / mybox-app / modules/test | 每模块独立 crate，可独立编译测试。 |

## Stack Touched in Phase 1

- [x] Rust workspace（mybox-core / mybox-app / modules/test）+ `[workspace.dependencies]` 版本锁定
- [x] winit 0.30.13：事件循环（ApplicationHandler + with_user_event）、Accessory 激活策略、窗口创建（Overlay/Floating/Panel）
- [x] tiny-skia 0.12.0：Renderer trait 后端、像素转换、测试窗口内容绘制、托盘图标生成
- [x] softbuffer 0.4.8：framebuffer present（0x00RRGGBB）
- [x] global-hotkey 0.8.0：热键注册 + 事件转发
- [x] tray-icon 0.24.2 / muda 0.19.3：托盘 + 菜单（模块 menu_items + 退出）
- [x] crossbeam-channel 0.5.16：EventBus + WindowRequest 队列
- [x] toml 1.1.4 / directories 6.0.0：ConfigCenter
- [x] serde / serde_json：事件 payload（Module JSON）+ 配置
- [x] thiserror 2.x / anyhow 1.x / log 0.4 + env_logger 0.11：错误与日志
- [x] parking_lot 0.12.x：共享状态（handler 列表、配置缓存）

## Out of Scope (deferred to later phases)

| Item | Deferred To | Why |
|------|-------------|-----|
| egui / 任何 UI 框架 | Phase 3 | egui-tiny-skia 不存在；Phase 1 成功标准无需 egui；手动 tessellation 集成 Phase 3 决策。 |
| 透明覆盖窗口真实 alpha | Phase 2 | softbuffer macOS 无逐像素 alpha（NoneSkipFirst）；需 objc2 CALayer 路径。 |
| Overlay 窗口 screenSaver 级别（覆盖全屏应用/菜单栏） | Phase 2 | winit AlwaysOnTop ≠ screenSaver；objc2。 |
| 多显示器批量覆盖（batch_create 实装） | Phase 2 | D-09 签名已在 Phase 1 定义。 |
| Screen Recording 权限引导 | Phase 2 | CAP-08。 |
| 热键冲突检测 | v2 | INFRA-EX-04。 |
| 配置热重载 | v2 | INFRA-EX-02。 |
| Windows 平台适配 | Phase 4 | 本 Phase 只做 macOS。 |
| 网络代码 / 动态模块加载 / 插件市场 | Out of Scope (PROJECT.md) | 无远程攻击面；编译期注册。 |

## Subsequent Slice Plan (Phase 2-4)

- **Phase 2 截图模块**：屏幕捕获（xcap/screenshots）→ 内存 → 覆盖窗口（透明合成 + screenSaver 级别，objc2）→ 区域选择交互 → 标注（tiny-skia）→ 剪贴板（arboard）→ macOS 权限引导。使用 Phase 1 的 WindowManager（Overlay + batch_create）、EventBus、HotkeyManager。
- **Phase 3 命令面板**：egui 手动 tessellation 进 tiny-skia Pixmap（RESEARCH §5 预案）；Panel 浮窗 + 模糊搜索 + 键盘导航；模块注册命令聚合。使用 Phase 1 的 Renderer trait 扩展点（draw 闭包插 egui 层）。
- **Phase 4 跨平台完善**：Windows 托盘/热键/截图适配（windows crate）；DPI 坐标统一（物理像素）；错误处理打磨。使用 Phase 1 的 platform 模块约定（cfg 隔离）。
