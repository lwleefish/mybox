# Phase 3: 命令面板 - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-13
**Phase:** 3-命令面板
**Areas discussed:** egui 归属与集成方式, 执行期间面板表现, 面板视觉形态, 内置命令实现细节

---

## egui 归属与集成方式

| Option | Description | Selected |
|--------|-------------|----------|
| core 共享 | egui 依赖放 mybox-core，未来模块复用 UI 能力 | ✓ |
| palette 私有 | egui 只在 palette 内部，core 保持零 UI 依赖 | |

**User's choice:** core 共享（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| 0.30 | 最新稳定版，与 winit 0.30 集成最好 | ✓ |
| 0.29 | STACK.md 原始推荐版本，更保守 | |

**User's choice:** 0.30（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| 模块自持有 | palette 经 WindowSpec.on_event/on_draw 转发与渲染，核心框架零改动 | ✓ |
| core 内建 egui 钩子 | core 新增 egui 窗口能力，需改核心框架 | |

**User's choice:** 模块自持有（推荐）

---

## 执行期间面板表现

| Option | Description | Selected |
|--------|-------------|----------|
| 状态行+禁用 | 状态行显示「正在执行」，列表禁用防重入 | ✓ |
| 保持原样 | 无视觉反馈，完成前不响应输入 | |
| 立即关闭 | 后台执行，与 SPEC「面板保持到完成」冲突 | |

**User's choice:** 状态行+禁用（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| 面板内提示 | 列表区显示错误消息，任意键关闭 | ✓ |
| 系统通知 | macOS 通知中心 / Windows toast | |
| 面板内+日志 | 面板提示 + 写日志，不弹通知 | |

**User's choice:** 面板内提示（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| 建销模式 | 每次唤出新建 Floating，关闭销毁，与 Phase 2 一致 | ✓ |
| 单例复用 | 隐藏/显示复用，更快但需状态重置 | |

**User's choice:** 建销模式（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| 同步 runner | 同步闭包 + worker 线程，当前命令都很快 | |
| 异步 runner | Future + async 运行时，为未来 AI 命令预留 | ✓ |

**User's choice:** 异步 runner
**Notes:** 用户覆盖了推荐（同步），明确理由是 v2 AI 对话类慢命令需要异步，长期方向优先。

---

## 面板视觉形态

| Option | Description | Selected |
|--------|-------------|----------|
| Raycast 风 | 大圆角卡片、大号列表项、宽松间距 | ✓ |
| Alfred 风 | 紧凑列表、小号字体、信息密度高 | |
| 极简风 | 不模仿特定产品 | |

**User's choice:** Raycast 风（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| 深色固定 | 背景深灰、文字浅色，与截图遮罩一致 | ✓ |
| 跟随系统 | 按系统外观切换，多一层平台适配 | |
| 浅色固定 | — | |

**User's choice:** 深色固定（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| 名称+描述+高亮 | 名称 + 灰色描述 + 命中字符高亮 | ✓ |
| 名称+描述 | 无高亮 | |
| 仅名称 | 最紧凑 | |

**User's choice:** 名称+描述+高亮（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| 600px 宽 | 高度自适应上限约 10 行 | ✓ |
| 480px 紧凑 | 接近 Spotlight 尺寸 | |
| You decide | 细节由 planner 定 | |

**User's choice:** 600px 宽（推荐）

---

## 内置命令实现细节

| Option | Description | Selected |
|--------|-------------|----------|
| 配置目录 logs/ | logs/mybox.log，启动即落盘，命令必能找到文件 | ✓ |
| 配置目录根 | mybox.log 与配置混杂 | |
| 不落盘 | 命令降级，与 SPEC 锁定 4 命令不符 | |

**User's choice:** 配置目录 logs/（推荐）

---

| Option | Description | Selected |
|--------|-------------|----------|
| spawn+退出 | spawn 当前可执行文件 + 正常退出，跨平台标准 | ✓ |
| 框架内软重启 | 卸载模块重建状态，复杂且风险高 | |

**User's choice:** spawn+退出（推荐）

---

## the agent's Discretion

- async 运行时选型（tokio/smol 等）与 runner 执行线程模型
- egui CPU 软件渲染后端方案（egui-tiny-skia 或 0.30 softbuffer 集成）
- 截图命令与面板的衔接（capture 模块注册自己的 runner）
- fuzzy-matcher 评分参数
- 面板具体视觉参数（行高、内边距、圆角、颜色值）
- BuiltinCommands 实现形态、退出命令复用托盘退出路径、dev 模式 spawn 路径

## Deferred Ideas

None — discussion stayed within phase scope.
