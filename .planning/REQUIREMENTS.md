# Requirements: mybox

**Defined:** 2026-08-11
**Core Value:** 一个统一入口、可按自己想法无限扩展的桌面工具箱--按一个快捷键唤起命令面板，所有工具触手可及。

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Framework

- [ ] **FRMW-01**: 应用通过 Module trait 注册功能模块，模块在编译期通过 AppBuilder 注册
- [ ] **FRMW-02**: 模块间通过事件总线（pub/sub）通信，不直接依赖彼此 API
- [ ] **FRMW-03**: WindowManager 支持创建 Overlay（全屏透明覆盖）、Floating（独立浮窗）、Panel（常规面板）三种窗口类型
- [ ] **FRMW-04**: HotkeyManager 支持注册全局热键，热键触发时执行回调
- [ ] **FRMW-05**: 框架在独立线程执行耗时操作，事件循环线程只处理 UI 事件和渲染
- [ ] **FRMW-06**: macOS 上应用以 Accessory 模式运行（不显示在 Dock），覆盖窗口可获取键盘焦点

### Infrastructure

- [ ] **INFRA-01**: ConfigCenter 支持分节 TOML 配置读写，按模块 ID 命名空间隔离
- [ ] **INFRA-02**: 系统托盘驻留运行，右键菜单展示模块注册的菜单项和退出按钮
- [ ] **INFRA-03**: 统一错误处理（anyhow + thiserror），关键操作有日志输出
- [ ] **INFRA-04**: 应用配置文件存储在用户配置目录（macOS: ~/Library/Application Support/mybox/）

### Capture

- [ ] **CAP-01**: 用户按热键触发截图，捕获所有显示器画面到内存
- [ ] **CAP-02**: 全屏透明覆盖窗口显示，遮罩半透明黑色，选区内显示原始画面
- [ ] **CAP-03**: 用户通过鼠标拖拽选择截图区域，实时显示选区边框和尺寸（WxH 像素）
- [ ] **CAP-04**: 用户确认截图后，选区图像复制到系统剪贴板
- [ ] **CAP-05**: 用户按 ESC 取消截图，覆盖窗口销毁
- [ ] **CAP-06**: 截图标注工具：矩形框、箭头、画笔（自由路径）、文字
- [ ] **CAP-07**: 标注支持撤销（Ctrl+Z），可撤销到截图原始状态
- [ ] **CAP-08**: macOS 首次截图时检测 Screen Recording 权限并引导用户授权

### Palette

- [ ] **PAL-01**: 用户按全局快捷键唤出命令面板浮窗
- [ ] **PAL-02**: 命令面板列出所有模块注册的命令
- [ ] **PAL-03**: 用户输入关键词模糊过滤命令列表
- [ ] **PAL-04**: 用户通过方向键导航选择命令，回车执行
- [ ] **PAL-05**: 用户按 ESC 关闭命令面板

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Capture (Extended)

- **CAP-EX-01**: Pin 浮窗 - 截图后可钉在屏幕上，支持拖拽移动和滚轮缩放
- **CAP-EX-02**: 放大镜 - 鼠标附近像素放大显示
- **CAP-EX-03**: 马赛克标注工具
- **CAP-EX-04**: 多显示器选区跨越支持
- **CAP-EX-05**: 截图保存到文件（自定义路径、格式选择）

### Extension Modules

- **EXT-01**: 颜色拾取模块 - 从屏幕读取像素颜色值
- **EXT-02**: idea 记录器模块 - 快速记录灵感，本地存储 + 搜索
- **EXT-03**: 快捷启动器模块 - 快速启动应用/脚本/URL
- **EXT-04**: 定时任务模块 - cron 式调度执行模块任务
- **EXT-05**: AI 对话助手模块 - 命令面板内 LLM 对话交互

### Infrastructure (Extended)

- **INFRA-EX-01**: 开机自启（macOS LaunchAgent + Windows Registry）
- **INFRA-EX-02**: 配置热重载
- **INFRA-EX-03**: State Store - 跨模块共享运行时状态
- **INFRA-EX-04**: 热键冲突检测与提示

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Linux 支持 | 先做 macOS + Windows，Linux 可后期加 |
| 移动端 | 桌面工具箱，不涉及移动平台 |
| Web 版 | 纯原生应用，不做 Web |
| Tauri/Electron 方案 | 明确选择纯 Rust 原生 |
| 模块动态加载 (dylib) | Rust ABI 不稳定；首期编译期注册即可 |
| 屏幕录制 / GIF | 巨大复杂度，偏离工具箱定位 |
| 云同步 | 需要后端服务，偏离纯本地工具定位 |
| 内嵌图片编辑器 | 功能蔓延，标注工具足够 |
| 插件市场 | 过早优化，开源项目用 fork + PR |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FRMW-01 | Phase 1 | Pending |
| FRMW-02 | Phase 1 | Pending |
| FRMW-03 | Phase 1 | Pending |
| FRMW-04 | Phase 1 | Pending |
| FRMW-05 | Phase 1 | Pending |
| FRMW-06 | Phase 1 | Pending |
| INFRA-01 | Phase 1 | Pending |
| INFRA-02 | Phase 1 | Pending |
| INFRA-03 | Phase 1 | Pending |
| INFRA-04 | Phase 1 | Pending |
| CAP-01 | Phase 2 | Pending |
| CAP-02 | Phase 2 | Pending |
| CAP-03 | Phase 2 | Pending |
| CAP-04 | Phase 2 | Pending |
| CAP-05 | Phase 2 | Pending |
| CAP-06 | Phase 2 | Pending |
| CAP-07 | Phase 2 | Pending |
| CAP-08 | Phase 2 | Pending |
| PAL-01 | Phase 3 | Pending |
| PAL-02 | Phase 3 | Pending |
| PAL-03 | Phase 3 | Pending |
| PAL-04 | Phase 3 | Pending |
| PAL-05 | Phase 3 | Pending |

**Coverage:**
- v1 requirements: 23 total
- Mapped to phases: 23
- Unmapped: 0 ✓

---
*Requirements defined: 2026-08-11*
*Last updated: 2026-08-11 after roadmap creation*
