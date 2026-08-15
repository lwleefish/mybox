# Roadmap: mybox

## Overview

从零搭建一个纯 Rust 原生的跨平台桌面工具箱。先建立模块化框架核心（窗口管理、事件总线、热键、托盘、配置），再用完整的截图功能（捕获、区域选择、标注）验证框架可用性，最后加入命令面板作为统一交互入口。采用 Vertical MVP 模式，每个阶段交付端到端的用户能力。

## Phases

**Phase Numbering:**

- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: 框架核心** - 搭建模块化框架基础设施 (completed 2026-08-12)
- [x] **Phase 2: 截图模块** - 完整截图功能验证框架 (completed 2026-08-13)
- [x] **Phase 3: 命令面板** - 统一交互入口 (completed 2026-08-15)
- [ ] **Phase 4: 跨平台完善** - Windows 适配与打磨

## Phase Details

### Phase 1: 框架核心

**Goal**: 搭建可运行的模块化框架：Module trait、事件总线、窗口管理、热键、托盘、配置系统。应用能以托盘驻留，通过热键触发一个测试窗口，验证框架可用。
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: FRMW-01, FRMW-02, FRMW-03, FRMW-04, FRMW-05, FRMW-06, INFRA-01, INFRA-02, INFRA-03, INFRA-04
**Success Criteria** (what must be TRUE):

  1. 应用启动后显示系统托盘图标，不显示在 Dock 中
  2. 按注册的全局热键能触发回调（如打印日志或弹出一个测试窗口）
  3. Module trait 可注册多个模块，模块通过事件总线收发消息
  4. WindowManager 能创建透明覆盖窗口和常规面板窗口
  5. 配置文件在用户目录创建并正确读写

**Plans**: TBD

Plans:
**Wave 1**

- [x] 01-01: Workspace 骨架 + 核心类型定义（Module trait, Event, WindowSpec, errors）

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 01-02: 事件总线 + 窗口管理器实现
- [x] 01-03: 热键管理器 + 系统托盘 + 配置系统

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 01-04: 事件循环集成 + macOS 平台适配 + 测试模块验证

### Phase 2: 截图模块

**Goal**: 实现完整截图功能：屏幕捕获、区域选择、标注工具、剪贴板复制。用真实功能验证 Phase 1 的框架可用性。
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: CAP-01, CAP-02, CAP-03, CAP-04, CAP-05, CAP-06, CAP-07, CAP-08
**Success Criteria** (what must be TRUE):

  1. 用户按热键触发截图，覆盖窗口出现并显示屏幕画面
  2. 用户拖拽选择区域，实时显示选区边框和尺寸
  3. 用户确认截图后，选区图像在剪贴板中可用
  4. 用户可在截图上绘制矩形、箭头、画笔、文字标注
  5. 用户按 Ctrl+Z 撤销标注，按 ESC 取消截图
  6. macOS 首次截图时提示用户授予屏幕录制权限

**Plans**: 4 plans

Plans:
**Wave 1** *(deps gate + core render chain + capture backend)*

- [x] 02-01: 依赖验证（xcap/arboard/ab_glyph 门禁）+ 框架渲染链路（on_draw/Redraw/batch_create 处置）+ 捕获后端（xcap 全屏捕获 + 权限预检 + SessionState）

**Wave 2** *(blocked on Wave 1)*

- [x] 02-02: 覆盖窗口显示（每屏一窗 + 画面/遮罩合成）+ 区域选择交互（拖拽/8 手柄/WxH 标签/ESC 取消）

**Wave 3** *(blocked on Wave 2)*

- [x] 02-03: 标注工具（矩形/箭头/画笔/文字）+ 统一工具栏 + Ctrl+Z 撤销

**Wave 4** *(blocked on Wave 3)*

- [x] 02-04: 剪贴板复制（含标注）+ macOS 权限引导 + 确认流程串联 + 端到端测试

### Phase 3: 命令面板

**Goal**: 实现命令面板作为所有模块的统一交互入口。全局快捷键唤出，展示已注册命令，模糊搜索，键盘导航执行。
**Mode:** mvp
**Depends on**: Phase 2
**Requirements**: PAL-01, PAL-02, PAL-03, PAL-04, PAL-05
**Success Criteria** (what must be TRUE):

  1. 用户按全局快捷键唤出命令面板浮窗
  2. 命令面板列出截图模块注册的命令（如"开始截图"）
  3. 用户输入关键词可模糊过滤命令列表
  4. 用户通过方向键选择命令，回车执行对应功能
  5. 用户按 ESC 关闭命令面板

**Plans**: 4 plans

Plans:
**Wave 1**

- [x] 03-01: 命令面板窗口 + 命令注册系统（核心命令系统 C1/C2/C5 + 框架窗口扩展 C3/C4/C6 + palette crate 唤出热键/建销/egui 渲染 + capture 命令注册 + 双路日志）

**Wave 2** *(blocked on Wave 1)*

- [x] 03-02: 模糊搜索 + 键盘导航 + 命令执行（fuzzy-matcher 过滤高亮 + 导航状态机 + 执行生命周期 + E2E 集成测试）

**Wave 3** *(gap closure)*

- [x] 03-03: GAP-1 修复——热键重复唤出失败（core 热键 Pressed-only 过滤 + WindowSpec.on_created 建销配对归属 + E2E consecutive_summon_close 探针 + PAL-01 文档同步）

**Wave 4** *(blocked on Wave 3 — shared files)*

- [x] 03-04: GAP-2 修复——文字灰色块渲染（raster UV 路径判别 + 图集 partial 补丁 + E2E glyph_shape 探针 + PAL-02 文档同步）

### Phase 4: 跨平台完善

**Goal**: 在 Windows 上完成适配，确保 macOS 和 Windows 双平台稳定运行。处理 DPI 缩放、多显示器、错误处理打磨。
**Mode:** mvp
**Depends on**: Phase 3
**Requirements**: FRMW-06 (Windows 验证), INFRA-04 (Windows 路径)
**Success Criteria** (what must be TRUE):

  1. 应用在 Windows 上启动后显示系统托盘图标
  2. 截图功能在 Windows 上完整可用（捕获、选区、标注、复制）
  3. 命令面板在 Windows 上可唤出并执行命令
  4. 高 DPI 显示器上截图选区与实际捕获区域一致

**Plans**: TBD

Plans:

- [ ] 04-01: Windows 平台适配与测试
- [ ] 04-02: DPI 缩放修复 + 错误处理打磨

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. 框架核心 | 4/4 | Complete   | 2026-08-12 |
| 2. 截图模块 | 4/4 | Complete   | 2026-08-13 |
| 3. 命令面板 | 4/4 | Complete   | 2026-08-15 |
| 4. 跨平台完善 | 0/2 | Not started | - |
