# Feature Research

**Domain:** Rust native desktop toolbox (modular plugin architecture)
**Researched:** 2026-08-11
**Confidence:** HIGH

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist. Missing these = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| 系统托盘驻留 | 桌面工具必须后台运行，不能一直占 dock/taskbar | LOW | tray-icon crate；右键菜单含退出/设置 |
| 全局热键 | 工具箱核心交互--从任何应用中唤起 | LOW | global-hotkey crate；用户可自定义绑定 |
| 截图区域选择 | 截图工具的基本功能 | MEDIUM | 全屏透明覆盖窗口 + 鼠标拖拽选区 |
| 截图复制到剪贴板 | 截图后最常见操作 | LOW | arboard crate；支持图片格式 |
| ESC 取消操作 | 任何覆盖窗口都应可取消 | LOW | 事件处理；销毁覆盖窗口 |
| 配置持久化 | 热键、保存路径等设置需保存 | LOW | TOML 文件 + serde；按模块分节 |
| 开机自启 | 工具箱应随系统启动 | LOW | macOS: LaunchAgent；Windows: Registry |

### Differentiators (Competitive Advantage)

Features that set the product apart. Not required, but valuable.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| 模块化插件架构 | 用户/开发者可按需扩展，不像 Snipaste 只有截图 | HIGH | Module trait + 事件总线 + 窗口管理 |
| 命令面板统一入口 | 类似 Spotlight/Raycast，一个快捷键访问所有工具 | HIGH | 全局浮窗 + 模糊搜索 + 模块注册的命令 |
| Pin 浮窗 | 截图后可钉在屏幕上，Snipaste 的标志性功能 | MEDIUM | 无边框置顶窗口 + 拖拽缩放 |
| 截图标注 | 矩形、箭头、文字、画笔、马赛克 | MEDIUM | tiny-skia 路径渲染 + 标注数据结构 |
| 放大镜 | 鼠标附近像素放大显示，精确选择 | MEDIUM | 裁剪缩放选区附近像素 |
| 多显示器支持 | 跨屏截图、多屏 Pin | MEDIUM | 虚拟屏幕坐标 + 多窗口 |
| 颜色拾取 | 从屏幕读取像素颜色值 | LOW | 截图时读取鼠标位置像素 RGB |
| idea 记录器 | 快速记录灵感，不切换应用 | MEDIUM | 命令面板输入 + 本地存储 + 搜索 |
| AI 对话助手 | 命令行式 AI 交互，不单独开窗口 | HIGH | LLM SDK 集成 + 命令面板内对话 |
| 定时任务 | 定时执行截图、脚本等 | MEDIUM | cron 式调度 + 模块注册任务 |
| 快捷启动器 | 快速启动应用/脚本/URL | MEDIUM | 命令面板子功能 + 索引构建 |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| 模块动态加载 (dylib) | "运行时安装新模块" | Rust ABI 不稳定；跨平台 dylib 痛苦；安全风险 | 编译期注册；未来可用 WASM 插件 |
| 屏幕录制 + GIF | "完整的录屏工具" | 巨大复杂度（编码、性能、存储）；偏离工具箱定位 | 截图足够；录屏推荐 OBS |
| 云同步 | "多设备同步配置" | 需要后端服务；偏离纯本地工具定位 | 本地配置导入/导出 |
| 内嵌图片编辑器 | "截图后高级修图" | 功能蔓延；与专业工具竞争 | 标注工具足够；导出到专业编辑器 |
| 插件市场 | "社区分享模块" | 需要后端、审核机制；过早优化 | 开源项目直接 fork + PR |

## Feature Dependencies

```
[模块框架 (Module trait + EventBus + WindowMgr)]
    └──requires──> [系统托盘]
    └──requires──> [全局热键系统]
    └──requires──> [配置系统]

[命令面板]
    └──requires──> [模块框架]
    └──requires──> [全局热键系统]

[截图模块]
    └──requires──> [模块框架]
    └──requires──> [窗口管理 (Overlay)]
    └──enhances──> [Pin 浮窗]
    └──enhances──> [标注工具]
    └──enhances──> [放大镜]

[Pin 浮窗]
    └──requires──> [窗口管理 (Floating)]
    └──requires──> [截图模块]

[颜色拾取]
    └──requires──> [模块框架]
    └──requires──> [屏幕捕获 (复用截图)]

[idea 记录器]
    └──requires──> [命令面板]
    └──requires──> [配置系统 (存储)]

[AI 对话助手]
    └──requires──> [命令面板]
    └──requires──> [网络层 (LLM API)]

[定时任务]
    └──requires──> [模块框架]
    └──requires──> [事件总线]

[快捷启动器]
    └──requires──> [命令面板]
```

### Dependency Notes

- **所有模块 requires 模块框架:** 框架是基础，必须先建
- **命令面板 requires 模块框架:** 命令面板需要读取已注册模块的命令列表
- **截图模块 enhances Pin 浮窗:** Pin 是截图的下游功能，但需要独立窗口管理
- **idea 记录器 requires 命令面板:** idea 记录通过命令面板输入
- **颜色拾取 requires 截图(屏幕捕获):** 复用屏幕捕获能力

## MVP Definition

### Launch With (v1)

Minimum viable product - what's needed to validate the concept.

- [ ] 模块框架核心 - Module trait, 事件总线, 窗口管理, 热键系统, 配置中心, 系统托盘
- [ ] 截图完整功能 - 屏幕捕获, 区域选择, 标注工具, Pin 浮窗, 剪贴板复制
- [ ] 命令面板 - 全局快捷键唤出, 展示已注册命令, 执行模块命令
- [ ] 跨平台基础 - macOS + Windows 基本可用

### Add After Validation (v1.x)

Features to add once core is working.

- [ ] 颜色拾取 - 截图框架已就绪，添加取色功能
- [ ] 快捷启动器 - 命令面板已就绪，添加应用索引
- [ ] idea 记录器 - 命令面板已就绪，添加本地存储
- [ ] 定时任务 - 事件总线已就绪，添加调度器

### Future Consideration (v2+)

Features to defer until product-market fit is established.

- [ ] AI 对话助手 - 需要选择 LLM SDK，设计 API 集成
- [ ] 模块动态加载 - 考虑 WASM 方案
- [ ] 多显示器完善支持 - 需要大量测试
- [ ] 开机自启 - 各平台实现差异大

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| 模块框架 | HIGH | HIGH | P1 |
| 系统托盘 | MEDIUM | LOW | P1 |
| 全局热键 | HIGH | LOW | P1 |
| 截图区域选择 | HIGH | MEDIUM | P1 |
| 截图复制到剪贴板 | HIGH | LOW | P1 |
| 截图标注 | MEDIUM | MEDIUM | P1 |
| Pin 浮窗 | HIGH | MEDIUM | P1 |
| 命令面板 | HIGH | HIGH | P1 |
| 配置持久化 | MEDIUM | LOW | P1 |
| 放大镜 | MEDIUM | MEDIUM | P2 |
| 颜色拾取 | MEDIUM | LOW | P2 |
| idea 记录器 | MEDIUM | MEDIUM | P2 |
| 快捷启动器 | MEDIUM | MEDIUM | P2 |
| 定时任务 | LOW | MEDIUM | P3 |
| AI 对话助手 | HIGH | HIGH | P3 |
| 多显示器完善 | MEDIUM | HIGH | P3 |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

## Competitor Feature Analysis

| Feature | Snipaste | Shottr | CleanShot X | Raycast | Our Approach |
|---------|----------|--------|-------------|---------|--------------|
| 截图区域选择 | ✓ | ✓ | ✓ | ✗ | winit 覆盖窗口 + 鼠标拖拽 |
| 标注工具 | ✓ (基础) | ✓ (丰富) | ✓ (丰富) | ✗ | tiny-skia 路径渲染 |
| Pin 浮窗 | ✓ | ✓ | ✓ | ✗ | 无边框置顶窗口 |
| 放大镜 | ✓ | ✓ | ✓ | ✗ | 像素裁剪缩放 |
| 命令面板 | ✗ | ✗ | ✗ | ✓ | 全局浮窗 + 模糊搜索 |
| 模块扩展 | ✗ | ✗ | ✗ | ✓ (插件) | Module trait 编译期注册 |
| 颜色拾取 | ✗ | ✓ | ✓ | ✓ (插件) | 复用屏幕捕获 |
| AI 助手 | ✗ | ✗ | ✗ | ✓ (AI) | 命令面板内对话 |
| 开源 | ✗ | ✗ | ✗ | ✗ | 完全开源 |

## Sources

- Snipaste - screenshot + pin 功能标杆
- Shottr - macOS 截图工具，标注丰富
- CleanShot X - macOS 截图工具，功能全面
- Raycast - 命令面板 + 插件架构标杆
- Alfred - macOS 快捷启动器，命令面板参考
- Espanso - Rust 文本扩展工具，Rust 桌面工具参考实现

---
*Feature research for: Rust native desktop toolbox*
*Researched: 2026-08-11*
