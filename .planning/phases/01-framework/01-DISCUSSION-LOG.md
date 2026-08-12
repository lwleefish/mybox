# Phase 1: 框架核心 - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-08-11
**Phase:** 1-框架核心
**Areas discussed:** 渲染架构, 事件总线设计, 事件循环 + 多窗口, 配置系统

---

## 渲染架构

### Question 1: tiny-skia 和 egui 共存方式

| Option | Description | Selected |
|--------|-------------|----------|
| 每窗口独立渲染 | 截图覆盖层用 tiny-skia，命令面板用 egui，各自独立 | |
| 统一 tiny-skia + egui-skia | 所有窗口都用 tiny-skia，egui 渲染到 Pixmap 再合成 | |
| 统一 egui Painter | 所有窗口都用 egui，自定义绘制用 Painter API | |

**User's choice:** 提到了 Zed 的渲染方式，想了解 GPUI 方案。

### Question 2: 渲染方向选择

| Option | Description | Selected |
|--------|-------------|----------|
| GPUI 方案 | GPU 渲染，高性能，但增加 GPU 依赖 | |
| wgpu 自建渲染层 | 灵活但工作量大 | |
| tiny-skia 统一 + egui 集成 | 统一且无 GPU 依赖 | ✓ |

**User's choice:** tiny-skia 统一 + egui 集成
**Notes:** 用户了解到 Zed 用 GPUI (wgpu) 后，仍然选择了 tiny-skia 统一方案，保持与之前"避免 GPU 依赖"的决策一致。

### Question 3: Pixmap 合成方式

| Option | Description | Selected |
|--------|-------------|----------|
| 统一 Pixmap 合成 | 所有窗口共用一个 Pixmap，egui 叠加后 softbuffer 上屏 | ✓ |
| 每窗口独立 Pixmap | 更灵活但复杂 | |

**User's choice:** 统一 Pixmap 合成

### Question 4: 渲染抽象层级

| Option | Description | Selected |
|--------|-------------|----------|
| core 封装 Renderer | 模块只调 draw API，不接触底层 | ✓ |
| 模块直接用原生 API | 更自由但可能不一致 | |

**User's choice:** core 封装 Renderer

---

## 事件总线设计

### Question 1: 分发模型

| Option | Description | Selected |
|--------|-------------|----------|
| 同步分发 | 简单、确定性强，但慢处理者阻塞事件循环 | |
| 异步 channel | 不阻塞但复杂度高 | ✓ |
| 同步 + 可选 offload | 两全其美 | |

**User's choice:** 异步 channel

### Question 2: 分发策略

| Option | Description | Selected |
|--------|-------------|----------|
| 广播 + 过滤 | 所有订阅者都收到，自行过滤 | |
| 类型路由 | 高效但需维护路由表 | |
| 广播 + 通配符过滤 | 灵活且高效 | ✓ |

**User's choice:** 广播 + 通配符过滤

### Question 3: Payload 类型

| Option | Description | Selected |
|--------|-------------|----------|
| serde_json::Value | 灵活但无类型安全 | |
| 强类型枚举 | 类型安全但耦合 | |
| 混合方案 | 核心用枚举，模块自定义用 JSON | ✓ |

**User's choice:** 混合方案

---

## 事件循环 + 多窗口

### Question 1: 多窗口管理

| Option | Description | Selected |
|--------|-------------|----------|
| 集中管理 + ID 分发 | HashMap<WindowId, WindowState>，按 ID 分发 | ✓ |
| 类型路由 handler | 更清晰但类型系统复杂 | |

**User's choice:** 集中管理 + ID 分发

### Question 2: 热键/托盘事件集成

| Option | Description | Selected |
|--------|-------------|----------|
| UserEvent 集成 | 通过 winit Event::UserEvent 分发 | |
| 独立线程 + channel | 解耦但多一层间接 | ✓ |

**User's choice:** 独立线程 + channel

### Question 3: 多显示器覆盖窗口

| Option | Description | Selected |
|--------|-------------|----------|
| 每屏一窗 | 每个显示器独立窗口，手动定位 | ✓ |
| 单窗口跨屏 | 简单但可能有缝隙 | |
| v1 只做主屏 | 延后多屏 | |

**User's choice:** 每屏一窗

---

## 配置系统

### Question 1: 加载策略

| Option | Description | Selected |
|--------|-------------|----------|
| 全量内存缓存 | 启动加载，写入全量写回 | ✓ |
| 每次读文件 | 实时但慢 | |
| 内存 + watch 热重载 | 体验好但复杂 | |

**User's choice:** 全量内存缓存

### Question 2: 热键配置格式

| Option | Description | Selected |
|--------|-------------|----------|
| 字符串格式 | "F1" / "Cmd+Shift+S"，人类可读 | ✓ |
| 结构化格式 | [key, modifiers] 结构 | |

**User's choice:** 字符串格式

### Question 3: 默认配置生成

| Option | Description | Selected |
|--------|-------------|----------|
| 自动生成默认配置 | 首次启动生成完整配置文件 | ✓ |
| 代码默认值，不生成文件 | 只在用户手动创建后生效 | |

**User's choice:** 自动生成默认配置

### Question 4: 模块默认配置提供方式

| Option | Description | Selected |
|--------|-------------|----------|
| Module::default_config() | 模块通过 trait 方法提供默认值 | ✓ |
| 每模块 default.toml 文件 | 文件形式 | |
| core 全局默认 | 模块自己处理缺失配置 | |

**User's choice:** Module::default_config()

---

## Claude's Discretion

- Renderer API 的具体方法签名和绘制接口设计
- EventPayload 枚举的具体变体划分
- WindowState 的内部结构
- channel 的具体实现选择
- 配置文件的分节命名规则细节

## Deferred Ideas

None - discussion stayed within phase scope.
