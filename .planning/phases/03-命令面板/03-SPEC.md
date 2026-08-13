# Phase 3: 命令面板 — Specification

**Created:** 2026-08-13
**Ambiguity score:** 0.09 (gate: ≤ 0.20)
**Requirements:** 6 locked

## Goal

命令面板从"不存在"变为可用的统一交互入口：用户按全局快捷键 `Cmd+Shift+Space`（Windows: `Ctrl+Shift+Space`）唤出屏幕中央浮窗，面板列出全部命令（模块命令 + 框架内置命令），输入关键词实时模糊过滤，方向键导航、回车执行（面板保持到命令完成），ESC 关闭。

## Background

代码库现状（2026-08-13 scouted）：

- `Module` trait（`crates/mybox-core/src/module.rs`）已有 `id/name/init/default_config/menu_items/shutdown`，**没有命令注册接口**——模块命令无法被面板发现
- `ModuleRegistry` 编译期注册，`AppBuilder` 在启动时装配模块
- `HotkeyManager`（`hotkey.rs`）支持 `register_str(action, hotkey_str)` + `action_for_id`，截图模块已注册 `Cmd+Shift+T` 并直接触发截图——Phase 3 后面板应成为统一入口
- `WindowManager` 已有 `WindowKind::Floating`，`WindowSpec` 支持 title/transparent/always_on_top/decorations/visible/inner_size/position/cursor_icon/on_event/on_draw；Overlay 层级提升与 focus 逻辑在 `window.rs`（b4fa248/75badf3 已修复）
- `EventBus` pub/sub 已就绪（FRMW-02），`ModuleContext` 提供 bus/windows/config/hotkeys/ui 访问器
- 渲染管线：tiny-skia + softbuffer CPU 渲染，**无 UI 框架**；STACK.md 已推荐 egui 0.29+ + egui-winit（与 winit 0.30 兼容）用于工具栏/面板类 UI
- 托盘菜单已存在（INFRA-02），"退出"目前走托盘菜单
- 截图模块是唯一功能模块；Phase 2 已完成并通过手动验证

Phase 3 的目标增量：命令注册机制（Module trait 扩展或独立注册）、命令面板模块（palette）、egui 渲染集成、全局唤出热键。

## Requirements

1. **命令注册接口（PAL-02 前置）**: 框架提供命令注册机制，模块和框架自身都能声明可被面板发现的命令（id、名称、关键词、执行回调）。
   - Current: `Module` trait 无命令概念；截图模块通过 `HotkeyManager::register_str` 直接注册热键动作
   - Target: `Module` trait 新增 `commands()` 方法（返回 `Vec<Command>`，`Command` 含 `id/name/description/keywords/runner`）；框架通过 `BuiltinCommands` 或等价机制注册 4 个内置命令（退出/打开配置目录/重启应用/打开日志文件）
   - Acceptance: 注册截图模块 + 框架内置命令后，命令面板数据源可枚举出 ≥5 个命令（≥1 模块命令 + 4 内置命令），每个命令有非空 name 和可调用的 runner

2. **全局唤出热键（PAL-01）**: 用户按全局快捷键唤出命令面板浮窗。
   - Current: 无命令面板热键；截图热键 `Cmd+Shift+T` 已注册
   - Target: 注册全局热键 `Cmd+Shift+Space`（Windows: `Ctrl+Shift+Space`）为默认唤出键，可从配置读取覆盖；触发时唤出（未显示→显示并聚焦，已显示→关闭）
   - Acceptance: 启动应用后按 `Cmd+Shift+Space` 面板出现并聚焦输入；再按一次面板关闭；配置文件中写入热键字符串覆盖后重启生效

3. **面板窗口（PAL-01/PAL-02）**: 屏幕中央浮窗显示命令列表。
   - Current: `WindowKind::Floating` 已存在但无使用方；渲染管线无 UI 框架
   - Target: 新增 palette 模块（`crates/modules/palette`）使用 `WindowKind::Floating` 创建屏幕中央（当前显示器居中）无边框浮窗，egui 渲染输入框 + 命令列表；空输入时显示全部命令（按注册顺序）；窗口不可调整大小、聚焦时接收键盘输入
   - Acceptance: 唤出后窗口出现在当前活动显示器中央，列出全部命令，无输入时列表非空；窗口尺寸固定不可 resize

4. **模糊过滤（PAL-03）**: 输入关键词实时模糊过滤命令列表。
   - Current: 无任何搜索/过滤逻辑
   - Target: 引入 `fuzzy-matcher` crate；对命令 name（及 description/keywords）做模糊匹配评分（子序列 + 连续匹配优先），按分数降序排列，无匹配时显示空态提示；输入变化即时重算
   - Acceptance: 输入"截图"或"jt"均能命中截图命令且排在首位；输入无匹配字符串时列表为空并显示空态；清空输入恢复全部命令

5. **键盘导航与执行（PAL-04）**: 方向键选择命令，回车执行；面板在执行期间保持，命令完成后关闭。
   - Current: 无键盘导航/执行逻辑
   - Target: ↑/↓ 移动高亮（含首尾环绕，可选），高亮跟随过滤结果；回车执行当前高亮命令（无高亮时执行首个匹配）；执行期间面板保持显示（可显示执行状态），runner 完成（或失败并提示）后面板关闭；错误在面板内或系统通知中可见
   - Acceptance: 过滤后 ↓ 两次回车执行的是第三个匹配命令；执行截图命令时面板关闭时机不遮挡截图（面板隐藏后触发截图）；执行失败时用户能看到错误提示，面板随后关闭

6. **ESC 关闭（PAL-05）**: 用户按 ESC 关闭命令面板。
   - Current: 无面板可关闭
   - Target: 面板获得键盘焦点时按 ESC 销毁/隐藏浮窗并释放焦点，不执行任何命令
   - Acceptance: 唤出后按 ESC 面板关闭；连续唤出-ESC 3 次无窗口残留（复用 Phase 2 的 re-entrancy 教训：无孤儿窗口、无重复热键副作用）

## Boundaries

**In scope:**
- `Module` trait 命令注册接口（`commands()`）与 `Command` 类型
- 4 个框架内置命令：退出应用、打开配置目录、重启应用、打开日志文件
- palette 模块（新 crate `crates/modules/palette`）：Floating 居中浮窗 + egui 集成
- egui + egui-winit 依赖引入（mybox-core 或 palette 内部）
- fuzzy-matcher 依赖引入，模糊过滤 + 排序
- 全局唤出热键（`Cmd+Shift+Space` / `Ctrl+Shift+Space`，可配置）
- 键盘导航（↑/↓/回车/ESC）与命令执行生命周期（面板保持到完成）
- 空态展示（无输入全列表、无匹配空态）

**Out of scope:**
- 命令历史 / 最近使用记录 — 留待 v2（避免 MVP 蔓延）
- 命令排序策略（频率/学习）— 仅按模糊评分排序
- 多显示器定位选择 — 默认当前活动显示器，多显示器打磨留 Phase 4
- 配置热重载 — 热键等配置改动需重启生效，热重载留 Phase 4/v2
- 插件市场 / 动态模块加载 — 已在项目级 Out of Scope
- AI 对话助手（EXT-05）等 v2 模块命令 — 仅支撑现有截图模块
- 中文 IME 组合输入特殊处理 — 不做特殊处理，如有问题记录为 Phase 4 打磨项

## Constraints

- 平台：macOS + Windows；egui 版本必须兼容 winit 0.30（egui-winit 0.29+）
- 面板窗口必须复用 WindowManager 的 Floating 类型与现有 focus/层级机制（b4fa248 的 `focus_window`、75badf3 的 non-resizable 模式）
- 不修改核心框架渲染管线（tiny-skia + softbuffer 保留给截图 overlay）；egui 仅用于 palette 浮窗
- 新增功能不能修改核心框架代码，只通过实现 `Module` trait 接入（palette 是模块；命令注册接口是框架扩展点）
- 执行命令面板保持期间的实现必须保证截图命令触发前面板已隐藏（Phase 2 截图会捕获整个屏幕，面板可见会被拍进去）

## Acceptance Criteria

- [ ] 按 `Cmd+Shift+Space` 唤出面板，再按一次关闭，连续 5 次无窗口残留/无重复副作用
- [ ] 面板列出 ≥5 个命令：≥1 个模块命令（截图）+ 4 个内置命令（退出/配置/重启/日志）
- [ ] 空输入显示全部命令；输入"截图"（或拼音前缀子序列）过滤出截图命令并排在首位
- [ ] ↑/↓ 导航高亮正确跟随过滤结果；回车执行当前高亮命令
- [ ] 执行"开始截图"时：面板先隐藏/关闭 → 截图 overlay 正常出现（不被面板遮挡）→ 截图流程可完成
- [ ] 执行"退出应用"后应用进程正常退出
- [ ] 执行"打开配置目录"后在文件管理器中打开配置目录（macOS 用 `open`，Windows 用 Explorer）
- [ ] 按 ESC 面板关闭且不执行任何命令
- [ ] 全部纯逻辑（过滤、排序、导航状态机、命令注册）有单元测试；`cargo check --workspace` 无错误
- [ ] Windows 构建不破坏（`cargo check --target x86_64-pc-windows-msvc` 或等价检查通过或记录为 Phase 4 事项）

## Ambiguity Report

| Dimension          | Score | Min  | Status | Notes                              |
|--------------------|-------|------|--------|------------------------------------|
| Goal Clarity       | 0.95  | 0.75 | ✓      |                                    |
| Boundary Clarity   | 0.90  | 0.70 | ✓      | 明确 out-of-scope 列表              |
| Constraint Clarity | 0.90  | 0.65 | ✓      | egui 版本、热键、窗口类型均确定      |
| Acceptance Criteria| 0.85  | 0.70 | ✓      | 10 条 pass/fail 标准                |
| **Ambiguity**      | 0.09  | ≤0.20| ✓      |                                    |

## Interview Log

| Round | Perspective     | Question summary                    | Decision locked                                |
|-------|-----------------|-------------------------------------|------------------------------------------------|
| 1     | Researcher      | 命令列表内容（模块命令 vs 内置）     | 模块命令 + 框架内置命令                        |
| 1     | Researcher      | UI 渲染方案（tiny-skia vs egui）     | 引入 egui（+ egui-winit，兼容 winit 0.30）      |
| 1     | Researcher      | 窗口形态                             | 屏幕中央浮窗（Floating）                       |
| 2     | Simplifier      | 模糊匹配强度                         | 引入 fuzzy-matcher crate                       |
| 2     | Simplifier      | 内置命令清单                         | 退出/打开配置目录/重启/打开日志（4 个）        |
| 2     | Simplifier      | 空输入行为                           | 显示全部命令（注册顺序）                       |
| 3     | Boundary Keeper | 默认唤出热键                         | Cmd+Shift+Space（Win: Ctrl+Shift+Space）        |
| 3     | Boundary Keeper | 回车执行后面板行为                   | 面板保持到命令完成，完成后关闭                |
| 3     | Boundary Keeper | Phase 3 明确不做什么                 | 不做历史/排序/多显示器定位/配置热重载/插件市场 |

---

*Phase: 03-palette*
*Spec created: 2026-08-13*
*Next step: /gsd:discuss-phase 3 — implementation decisions (egui 集成方式、命令执行生命周期、面板状态机等)*
