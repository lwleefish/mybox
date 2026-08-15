---
phase: 03-命令面板
plan: 08
subsystem: ui
tags: [winit, egui-winit, IME, 中文输入, pinyin, keywords, fuzzy-matcher, palette, gap-closure]

requires:
  - phase: 03-02
    provides: palette session 状态机 / set_input 过滤语义 / MAX_QUERY_LEN 截断
  - phase: 03-04
    provides: glyph_shape 探针的 Ime 注入脚手架（Commit 经 egui-winit 翻译的先例）
  - phase: 03-07
    provides: palette_checks 探针模式 / press_key / 覆盖声明纪律
provides:
  - 四个内置命令拼音 keywords（tuichu/peizhi/chongqi/rizhi）——无 IME 前缀发现路径（GAP-7）
  - session ime_allowed 标志 + ensure_winit_state 首次事件显式 window.set_ime_allowed(true)（GAP-7 输入子问题）
  - E2E 探针 ime_commit_updates_input（Ime 注入 → session.input 中文断言 + 拼音过滤断言 + ime_allowed 标志断言）
affects: [UAT 测试 10 复验, Phase 4 跨平台 IME 验证, REQUIREMENTS PAL-03 输入可达性]

tech-stack:
  added: []
  patterns:
    - "IME 显式开启纪律：winit 调用移出 state 锁段，ime_allowed 标志一次生效守卫，锁内带出布尔值锁外执行"
    - "拼音前缀发现路径：keywords 纯数据扩展（与 capture 'jietu' 同机制），fuzzy-matcher 关键词梯队天然支持，无代码路径变化"
    - "探针覆盖声明纪律：合成 Ime 事件覆盖 winit→egui-winit→TextEdit→session 全链路 + 标志断言；OS 候选窗出现由 UAT 测试 10 人工复验"

key-files:
  created: []
  modified:
    - crates/mybox-core/src/command.rs
    - crates/modules/palette/src/session.rs
    - crates/modules/palette/src/bin/palette_checks.rs
    - crates/modules/palette/tests/integration.rs

key-decisions:
  - "IME 显式开启（GAP-7 输入子问题）：面板窗口首次事件即 window.set_ime_allowed(true)（ime_allowed 标志一次生效），消除 egui-winit 依赖 TextEdit 聚焦后帧 PlatformOutput.ime 的多帧时序——真实桌面首帧竞态下 OS 候选窗不出现；egui-winit 后续按焦点变化的 set_ime_allowed(false/true) 行为保留"
  - "拼音 keywords 覆盖全部内置命令（GAP-7 前缀发现子问题）：tuichu/peizhi/chongqi/rizhi 与 capture 既有 jietu 同机制（关键词梯队），无 IME 场景下用户可用拼音命中中文命令"
  - "SPEC 边界未扩大：无自研 IME 组合输入特殊处理、无拼音转换引擎；显式开启系统 IME 与 keywords 纯数据均为既有机制，03-SPEC/03-CONTEXT 不改动"

patterns-established:
  - "ensure_winit_state 锁外 winit 调用模式：锁段内只做状态判定（winit_state 创建 + ime_allowed 守卫），winit 调用在锁外用局部布尔值带出后执行"
  - "探针注册表镜像生产数据：ime_commit_updates_input 的 fake builtin.quit 镜像 Task 1 core 新 keywords，断言生产数据路径而非硬编码假设"

requirements-completed: [PAL-03]

duration: 5 min
completed: 2026-08-15
---

# Phase 3 Plan 8: GAP-7 中文输入修复 Summary

**面板窗口首次事件即显式 `window.set_ime_allowed(true)`（session `ime_allowed` 标志锁定，消除 egui-winit 焦点多帧时序竞态）+ 四个内置命令拼音 keywords（tuichu/peizhi/chongqi/rizhi，无 IME 前缀发现路径），E2E 探针 `ime_commit_updates_input` 经真实闭包断言标志设置、中文 Commit 进入 session.input 并过滤、拼音关键词命中中文命令——桌面会话 10/10 通过**

## Performance

- **Duration:** 5 min
- **Started:** 2026-08-15T08:36:24Z
- **Completed:** 2026-08-15T08:41:42Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- **GAP-7 输入子问题消除（主修复）**：`ensure_winit_state` 在创建 egui-winit State 的同一首次调用中显式 `window.set_ime_allowed(true)`（锁段内 `ime_allowed` 标志一次生效守卫，winit 调用在锁外执行）。根因：egui-winit 自身的 `set_ime_allowed` 依赖"TextEdit 聚焦 → 下一帧 PlatformOutput.ime → handle_platform_output"多帧时序（egui-winit lib.rs:851），真实桌面首帧竞态下 OS 候选窗不出现。面板唯一用途是文本输入，首次事件即显式开启消除时序依赖；egui-winit 后续按焦点变化关闭/开启的行为保留（Executing 输入禁用时关闭 IME 正确）
- **GAP-7 前缀发现子问题消除（辅修复）**：四个内置命令 keywords 增加拼音别名——`builtin.quit`→`tuichu`、`builtin.open_config`→`peizhi`、`builtin.restart`→`chongqi`、`builtin.open_log`→`rizhi`——与 capture 既有 `jietu` 同机制（fuzzy-matcher 关键词梯队，纯数据零代码路径变化）；单测 `builtin_keywords_include_pinyin_aliases` 锁定数据不丢失。无 IME 场景（或 OS 禁用 IME）下用户可用拼音命中中文命令
- **E2E 探针 `ime_commit_updates_input`**：真实窗口 + 真实生产 on_event_win 闭包注入合成 `Ime::Preedit("测")`/`Ime::Commit("截图")`，断言全链路：首次事件经真实闭包设置 `ime_allowed == true`（本探针核心覆盖点）→ Commit 帧 `session.input == "截图"`、状态 Filtering、`filtered == [0]`（capture.start name 梯队）→ `set_input("tuichu")` 断言 `filtered == [1]`（拼音关键词命中 退出应用）→ ESC 配对 Destroy + Hidden——桌面会话 10/10 通过
- **SPEC 边界声明落实**：无自研 IME 组合输入特殊处理、无拼音转换引擎——只显式开启系统 IME（RESEARCH Anti-Patterns 明示标准路径）与使用既有 keywords 数据字段，03-SPEC/03-CONTEXT 未改动

## Task Commits

Each task was committed atomically:

1. **Task 1: core 内置命令拼音 keywords + 单测** - `cf03c0e` (feat)
2. **Task 2: session 显式开启 IME（ime_allowed 标志 + set_ime_allowed）** - `bcd4bef` (feat)
3. **Task 3: E2E 探针 ime_commit_updates_input + 集成测试接线** - `e9b4fd7` (feat)

**Plan metadata:** 见最终 docs commit

## Files Created/Modified

- `crates/mybox-core/src/command.rs` - 四个内置命令 keywords 各追加拼音别名（GAP-7 注释说明）+ `builtin_keywords_include_pinyin_aliases` 单测（`builtins()` helper 取 4 命令逐一断言）
- `crates/modules/palette/src/session.rs` - `SessionInner.ime_allowed: bool` 字段（构造 false）+ `pub fn ime_allowed()` 访问器 + `ensure_winit_state` 重写（doc 注释记录 GAP-7 根因；锁段内状态判定/标志守卫，锁外 `window.set_ime_allowed(true)` + debug 日志）
- `crates/modules/palette/src/bin/palette_checks.rs` - `check_ime_commit_updates_input` 探针（四阶段 driver 状态机 + 覆盖声明）+ main() 分发/usage 接线（含模块 doc usage 更新）
- `crates/modules/palette/tests/integration.rs` - `palette_ime_commit_updates_input` `#[ignore]` 测试接线（PAL-03/GAP-7 回归）

## Verification Evidence

- `cargo nextest run -p mybox-core command` — 11/11 PASS（含新测试 `builtin_keywords_include_pinyin_aliases`）
- `cargo nextest run -p mybox-palette session` — 23/23 PASS（ime_allowed 标志无回归）
- `cargo nextest run -p mybox-palette -p mybox-core` — 150/150 PASS（14 skipped）
- `cargo nextest run --workspace` — 228/228 PASS（18 skipped）
- `cargo check --workspace` — exit 0，零 warning（core module.rs 既有 2 个 test-only warning 为 03-08 前已存在，超出本计划范围——`cargo check` 不含 test 代码故不出现）
- 桌面会话 `cargo test -p mybox-palette --test integration -- --ignored` — **10/10 PASS**，其中 `ime_commit_updates_input` 实测：首次事件 `ime_allowed() == true` 经真实闭包设置 ✓；`Ime::Preedit("测")` → `Ime::Commit("截图")` → Commit 帧 `session.input == "截图"`、Filtering、`filtered == [0]` ✓；`set_input("tuichu")` → `filtered == [1]` ✓；ESC → Destroy(created_id) + Hidden ✓

## Decisions Made

- IME 显式开启落点选在 `ensure_winit_state`（面板窗口首次事件），而非依赖 egui-winit 的焦点时序——一次生效、主线程调用、不持锁调用 winit
- 拼音关键词直接进 keywords 数据（不新增拼音转换引擎）——GAP-7 两个子问题各自用既有机制闭环，SPEC 边界零扩大
- 探针覆盖声明保持诚实：合成 Ime 事件覆盖 winit→egui-winit→TextEdit→session 全链路；OS 候选窗出现/交互由 UAT 测试 10 人工复验

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- 无。所有验收命令一次通过，探针首跑即绿（03-04 glyph_shape 探针已验证 Ime 注入链路，本探针在其脚手架之上叠加 session 级断言）。

## Known Stubs

None — 所有修改文件均为完整实现，无占位符/TODO/空数据流。

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- GAP-7 已关闭（代码层）：`ime_commit_updates_input` 探针可在桌面会话重复运行（`cargo test -p mybox-palette --test integration -- --ignored`，10/10）；人工最终复验见 UAT 测试 10（真实输入法下输入框可输入中文、候选窗出现；或拼音 tuichu 命中退出应用）
- Phase 03（命令面板）8/8 计划全部完成——REQUIREMENTS PAL-01..PAL-05 全部 Complete，GAP-1..GAP-7 全部关闭
- Phase 4（跨平台）无阻塞：IME 显式开启为 winit 跨平台 API（Windows 默认支持、调用无害）；拼音 keywords 为纯数据
- REQUIREMENTS.md 无改动（PAL-03 已 `[x]` Complete；本计划强化其输入可达性）

---

*Phase: 03-命令面板*
*Completed: 2026-08-15*

## Self-Check: PASSED

- [x] 4 个修改文件均存在磁盘（command.rs / session.rs / palette_checks.rs / integration.rs）
- [x] 3 个任务提交均存在 git 历史：`cf03c0e`、`bcd4bef`、`e9b4fd7`
- [x] 全部 acceptance_criteria 验证通过（源码断言 + `cargo nextest run -p mybox-core command` 11/11 + `cargo nextest run -p mybox-palette session` 23/23 + headless 150/150 + 桌面会话 10/10）
- [x] `cargo check --workspace` exit 0 零 warning
- [x] E2E 探针 `ime_commit_updates_input` 桌面会话实跑通过（ime_allowed 标志 + 中文 Commit 断言 + 拼音过滤断言 + ESC-Destroy 收尾）
