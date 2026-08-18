---
phase: 04-跨平台完善
verified: 2026-08-18T12:30:00Z
status: human_needed
score: 9/9 truths verified
overrides_applied: 0
human_verification:
  - test: "Windows 真机运行 mybox，确认系统托盘图标实际显示且菜单可用（ROADMAP SC1）"
    expected: "托盘图标出现，右键菜单展示模块菜单项和退出按钮"
    why_human: "CI runner 无真实 Windows 桌面会话；D-02 明确托盘/捕获/交互留待真机验收"
  - test: "Windows 真机触发截图，核对画面捕获、选区、标注、复制全链路（ROADMAP SC2）"
    expected: "热键触发截图，覆盖窗口显示屏幕画面，拖拽选区，确认后剪贴板可粘贴"
    why_human: "CI 仅 headless 探针（窗口+合成事件+内存 framebuffer），真实捕获/输入/剪贴板交互无法在 CI 验证"
  - test: "Windows 真机唤出命令面板并执行命令（ROADMAP SC3）"
    expected: "全局热键（Ctrl+Shift+Space）唤出面板，中文命令名有字形，键盘导航+回车执行"
    why_human: "需要真实键盘/鼠标输入；CI 只验证窗口创建与合成事件渲染"
  - test: "Windows 真机 150% 缩放下截图选区与实际捕获区域一致（ROADMAP SC4 真机确认）"
    expected: "高 DPI 显示器上选区边界与捕获画面像素对齐（point_to_physical 换算正确性）"
    why_human: "纯函数单测证明换算逻辑正确，但真实显示器上的视觉一致性需真机确认"
---

# Phase 4: 跨平台完善 Verification Report

**Phase Goal:** 跨平台完善 — Windows 验证基础设施（GitHub Actions CI）落地并全绿，Phase 3 遗留 9 项错误债（WR-01..03 + IN-01..06）全部修复并有测试锚定，DPI 换算一致性可验证（point_to_physical 纯函数 + 多 scale 用例）
**Verified:** 2026-08-18T12:30:00Z
**Status:** human_needed（全部自动化 must-haves VERIFIED；4 项真机验证项按 D-02 设计留待 Windows 真机）
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Windows CI 三步验证链（build / unit / headless 探针）落地且全绿 | ✓ VERIFIED | `.github/workflows/windows-ci.yml` 8 steps；末次代码推送 run 32097933874 全步骤 success（Build (Windows target) `cargo build --target x86_64-pc-windows-msvc --locked`、Unit tests 245/245、Capture 4 + Palette 12 probes 全部 OK）；run 32098172216 亦 green |
| 2 | Windows 单测全绿（CI job） | ✓ VERIFIED | run 32097933874 log: `245 tests run: 245 passed, 20 skipped`；含 fonts::tests::install_cjk_fonts_populates_font_data PASS（210/245） |
| 3 | 16 个 headless 探针按 D-03 执行，无白名单无失败即跳过 | ✓ VERIFIED | CI log 16 项全部 `'<check>': OK`（含 enter_clipboard 真实 OK）；能力门控代码存在（capture_checks.rs:173-176，Windows 无剪贴板时打印 SKIPPED 并 early Ok） |
| 4 | Windows CJK 中文字体加载（回退链） | ✓ VERIFIED | fonts.rs:52-75 `#[cfg(target_os = "windows")]` msyh.ttc → simhei.ttf → simsun.ttc 链；78-81 non-mac/windows no-op；116 `#[cfg(all(test, target_os = "windows"))]` 测试在 CI 通过 |
| 5 | CI 触发链就绪（仓库 + main 推送 + workflow） | ✓ VERIFIED | remote origin https://github.com/lwleefish/mybox.git；分支 main；`gh run list` 显示 push 触发连续 runs；repo PUBLIC（gh repo view） |
| 6 | D-08：9 项错误债（WR-01..03 + IN-01..06）全部修复并有测试锚定 | ✓ VERIFIED | 逐项见下方 Artifacts 表；9 项全部在代码层确认 + 对应测试本地与 CI 双绿 |
| 7 | D-09：9 项债归入 04-02（不新增 04-03） | ✓ VERIFIED | 全部 9 项修复落在 04-02 提交（2e12597/ca4946a/6e27c25/e71245a）；ROADMAP Phase 4 仅 04-01/04-02 两个计划 |
| 8 | DPI 换算一致可验证：point_to_physical 纯函数 + 4 scale 用例；compute_geometry @1.5 | ✓ VERIFIED | capture.rs:20-22 纯函数（capture_all_monitors L45-46 调用）；3 个测试（four_scales/rounding/negative）本地全绿；position.rs:158-161 @1.5 手算用例 (900,840)/(990,390) + 偏置光标用例 L170-177 全绿 |
| 9 | 04-02 改动经 Windows CI 回归（D-01 闭环） | ✓ VERIFIED | 04-02 提交（2e12597..e71245a）推送后 run 32097933874 全绿——9 项修复 + DPI 工作获得 Windows 编译级验证 |

**Score:** 9/9 truths verified

### Deferred Items

无 —— 未发现被后续 phase 明确承接的失败项（错误债已全部清零，04-02 即最后一计划）。

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `.github/workflows/windows-ci.yml` | FRMW-06 三步 CI job | ✓ VERIFIED | 存在；on push main + PR；concurrency；4 第三方 action 全部 SHA-pin + `# vX.Y.Z` 注释；`--locked` 全步骤；两探针循环名单与两个 main() match arm 逐一相等（4+12） |
| `.github/dependabot.yml` | github-actions + cargo 双条目 | ✓ VERIFIED | 存在；weekly；cargo group 合并更新 |
| `crates/modules/palette/src/fonts.rs` | Windows CJK 分支 + cfg 测试 | ✓ VERIFIED | 回退链三路径；not(any(macos, windows)) no-op；Windows 测试 CI 通过 |
| `crates/modules/capture/src/bin/capture_checks.rs` | enter_clipboard 能力门控 | ✓ VERIFIED | 173-176：能力探测先于断言；SKIPPED 打印 + early Ok；断言体未删改 |
| `crates/modules/capture/src/capture.rs` | point_to_physical + 多 scale 测试 | ✓ VERIFIED | L20-22 纯函数；L45-46 接入；测试 four_scales（1.0/1.25/1.5/2.0）、rounding（49.5→50）、negative（-1920/-150）全绿 |
| `crates/modules/palette/src/position.rs` | compute_geometry @1.5 用例 | ✓ VERIFIED | L158-161 手算断言 (900,840)/(990,390)；L170-177 偏置光标一致性 |
| `crates/mybox-core/src/window.rs` | WindowSpec.on_create_failed 槽位 | ✓ VERIFIED | L72 字段；L91 Default None |
| `crates/mybox-core/src/app.rs` | notify_create_failed + dispatch_window_event + IN-04 | ✓ VERIFIED | L379/L417 双失败路径触发；L469-473 take-once；L479-488 catch_unwind 双臂；L514 接线；L145-154 config_dir 显式 Option 传播（无 unwrap_or_default） |
| `crates/mybox-core/src/command.rs` | run_command spawn-Err hop + Option<PathBuf> | ✓ VERIFIED | L255 dispatch_completion；L291 worker 路径；L295-300 spawn-Err 路径（含 "failed to spawn command runner thread"）；无 `.expect("spawn command runner thread")` 残留；L98-99/108-109 Option<PathBuf>；L158/L210 显式 bail 消息 |
| `crates/mybox-core/src/context.rs` | pending_count 无副作用观察 | ✓ VERIFIED | L169 `#[cfg(test)]` pending_count |
| `crates/modules/palette/src/lib.rs` | WR-01 接线 + WR-03 + IN-02 注释 | ✓ VERIFIED | L421 on_create_failed 接线（failed_session clone）；L197 summon + L554 sync 双处 effective_window_height；L313 IN-02 锁序注释 |
| `crates/modules/palette/src/session.rs` | on_create_failed 复位 + 锁序文档 | ✓ VERIFIED | L211-219 复位 Hidden/window_id=None/pending_close=false/error=None；L510 IN-02 锁序不变量文档 |
| `crates/modules/palette/src/ui.rs` | TextEdit char_limit + effective_window_height | ✓ VERIFIED | L211 `.char_limit(filter::MAX_QUERY_LEN)`；L73-83 effective_window_height（零命令 → Empty 144） |
| `crates/modules/palette/src/bin/palette_checks.rs` | realize_window destroy + 具名常量 | ✓ VERIFIED | L94-96 register 前 wm.destroy(prev)；L81-89 注释与实现一致；L2101-2131 ROW_BAND_TOP/BOTTOM_LOGICAL（派生 ui::SP_*）+ ACCENT_RGB (0xFF,0x60,0x00) |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| create_window 两错误路径（app.rs L376-382, L414-420） | WindowSpec.on_create_failed | notify_create_failed（take + 只触发一次） | ✓ WIRED | 两个路径均先于返回 Err 调用；测试 create_failed_notifies_callback_once 验证 take 语义 |
| build_window_spec（lib.rs L421） | session.on_create_failed() | on_create_failed: Some(Box::new(move \|\| failed_session.on_create_failed())) | ✓ WIRED | 闭包捕获 failed_session clone（与 on_created 的 created_session 分持）；测试 create_failed_resets_session_and_unwedges 验证 summon→失败→再 summon |
| App::window_event（app.rs L514） | dispatch_window_event | 替换原两段 if-let | ✓ WIRED | on_event/on_event_win 均 catch_unwind；测试 panic_isolated_event_callbacks 用真实 headless winit 窗口驱动 on_event_win 臂（CR-01 路径覆盖，非 window:None 盲区） |
| run_command spawn Err / runner Err（command.rs L291, L295-300） | UiThreadProxy::run → finalize(Err) | dispatch_completion + Arc<parking_lot::Mutex<Option>> 共享 on_done | ✓ WIRED | 两分支各 .lock().take() 恰好一次；测试 spawn_failure_hops_error_to_main_thread 经 pending_count 轮询 + drain 验证 |
| summon_palette / sync_window_geometry（lib.rs L197, L554） | ui::effective_window_height | 命令数为 0 → Empty 144 分支 | ✓ WIRED | 测试 effective_height_zero_commands_uses_empty 验证 0 命令 144 / 非零命令既有表 |
| workflow 探针循环 | palette_checks/capture_checks main() match arm | cargo run --bin *_checks -- "$check" \|\| exit 1 | ✓ WIRED | 4+12 名单与 match arm 逐一相等（capture L237-240、palette L2625-2636）；CI 16/16 OK |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| capture.rs point_to_physical | monitor.x()/y() (points) × scale_factor() | xcap::Monitor（真实显示器几何） | ✓ 真实数据 | ✓ FLOWING |
| effective_window_height | command_count | session.commands()/filtered()（真实命令注册） | ✓ 真实数据 | ✓ FLOWING |
| on_create_failed → session 复位 | PaletteState | 失败路径真实触发（CI 编译级 + 单测路径） | ✓ 测试锚定 | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| point_to_physical 4 scale + 舍入 + 负坐标 | `cargo nextest run -p mybox-capture -E 'test(four_scales) or test(rounding_matches) or test(negative_coordinates)'` | 3/3 PASS | ✓ PASS |
| compute_geometry @1.5 手算 | `cargo nextest run -p mybox-palette -E 'test(scale_1_5)'` | PASS (900,840)/(990,390) | ✓ PASS |
| WR-01 take-once 回调 | `cargo nextest run -p mybox-core -E 'test(create_failed)'` | PASS（含 create_failed_notifies_callback_once） | ✓ PASS |
| WR-02 panic 隔离（真实窗口） | `cargo nextest run -p mybox-core -E 'test(panic_isolated)'` | PASS（双臂执行且不传播） | ✓ PASS |
| IN-01 spawn-Err hop | `cargo nextest run -p mybox-core -E 'test(spawn_failure)'` | PASS（pending_count 轮询 + drain 取回） | ✓ PASS |
| IN-04 显式 bail | `cargo nextest run -p mybox-core -E 'test(no_config_dir)'` | PASS（两运行器均 Err 带消息） | ✓ PASS |
| WR-03 零命令高度 | `cargo nextest run -p mybox-palette -E 'test(effective_height_zero)'` | PASS（0→144，非零→既有表） | ✓ PASS |
| IN-05 截断契约 | `cargo nextest run -p mybox-palette -E 'test(input_limit)'` | PASS | ✓ PASS |
| 本地全量（macOS） | `cargo nextest run -p mybox-core -p mybox-palette -p mybox-capture` | 235/240（5 个已知 flaky 窗口时序测试，见下） | ✓ PASS* |
| ignored 显示/OS 级测试 | `cargo test -- --ignored` | 20/20 全绿（4+4+12） | ✓ PASS |
| Windows CI 全步骤 | `gh run view 32097933874` | 8/8 steps success；245/245 unit；16/16 probes OK | ✓ PASS |

\* 5 个失败为已文档化的窗口时序 flaky（hotkey_toggle_during_executing_closes_and_ignores_stale_finalize、summon_spec_carries_on_created_pairing、hotkey_toggle_closes_after_window_created、hotkey_toggle_summon_creates_floating_window、late_window_created_after_close_is_destroyed）——并行 nextest 下窗口时序竞争；逐个隔离运行 5/5 PASS；Windows CI 245/245 稳定绿（本次验证已逐项隔离复跑确认，非本 phase 引入回归）。

### Probe Execution

本 phase 无 PLAN 声明的 probe 脚本（探针为 `cargo run --bin *_checks` 形式，已在 CI 内执行）：

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| 4 capture probes（overlay_capture/drag_selection/esc_destroy/enter_clipboard） | CI step "Capture headless probes"（run 32097933874） | 4/4 `'<check>': OK` | PASS |
| 12 palette probes（summon_render … click_hide_before_capture） | CI step "Palette headless probes"（run 32097933874） | 12/12 `'<check>': OK` | PASS |
| 本地抽查 summon_render / keyword_highlight | `cargo run -p mybox-palette --bin palette_checks -- <check>` | 两者 exit 0（04-02 SUMMARY 记录；CI 亦绿） | PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ----------- | ----------- | ------ | -------- |
| FRMW-06 | 04-01 + 04-02 | macOS Accessory 模式（原义）；Phase 4 扩展：Windows 验证三步 + 稳定性硬化 | ✓ SATISFIED | CI build/unit/probes 全绿（04-01）；WR-01/02 稳定性硬化 + DPI 验证（04-02）；两 SUMMARY 均声明 requirements-completed |
| INFRA-04 | 04-02 | 配置文件存储在用户配置目录；Windows 路径不可用时显式错误 | ✓ SATISFIED | config.rs:154-159 ProjectDirs mybox（macOS ~/Library/Application Support/mybox，Windows 同构）；IN-04 Option<PathBuf> 显式 bail（command.rs L158/L210）+ app.rs L145-154 显式 warn 传播 |
| D-01（CI 唯一 Windows 验证） | 04-01 | CI runner 验证 Windows | ✓ SATISFIED | 仓库创建 + workflow 落地 + 两次推送全绿（32097933874/32098172216） |
| D-06/D-07（DPI 验证导向） | 04-02 | 验证现有 scale_factor 换算 | ✓ SATISFIED | point_to_physical 纯函数 + 3 测试 + compute_geometry @1.5 用例；不改 awareness |
| D-08（9 项错误债全修） | 04-02 | WR-01..03 + IN-01..06 | ✓ SATISFIED | 见 Artifacts 表逐项证据 |
| D-09（债归 04-02） | 04-02 | 不新增 04-03 | ✓ SATISFIED | ROADMAP Phase 4 仅 04-01/04-02；全部修复在 04-02 commits |

**Orphaned requirements:** 无。REQUIREMENTS.md traceability 表 INFRA-04 仍标 "Phase 1 | Pending"（文档滞后——本 phase 已按 PLAN frontmatter 声明并完成 INFRA-04 的 Windows 路径部分；不影响实现事实，建议后续更新 traceability 表）。

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| （无） | - | TBD/FIXME/XXX 债务标记 | - | 无——本 phase 全部修改文件零债务标记 |
| （无） | - | 空实现/硬编码空数据 | - | 无——ui.rs PLACEHOLDER 为具名颜色常量（#6E6E6E 占位文字色），非 stub |
| （无） | - | 未接线孤儿代码 | - | 无——point_to_physical 被 capture_all_monitors 调用；effective_window_height 被 summon/sync 双处调用 |
| （无） | - | 遗留 .expect / unwrap_or_default | - | 无——`expect("spawn command runner thread")` 与 `config_dir().unwrap_or_default()` 均已移除 |

### Human Verification Required

按 D-02 设计（用户决策：CI runner 暂不用真机测试），4 项 Windows 真机验证为本 phase 的既定待验项，非失败项：

### 1. Windows 托盘图标实际显示（ROADMAP SC1）

**Test:** Windows 真机运行 mybox，确认系统托盘图标出现且右键菜单可用
**Expected:** 托盘图标显示，菜单展示模块项与退出按钮
**Why human:** CI runner 无真实桌面会话，托盘渲染无法在 headless 验证

### 2. Windows 截图全链路（ROADMAP SC2）

**Test:** Windows 真机按热键触发截图，核对捕获、选区、标注、复制
**Expected:** 覆盖窗口显示屏幕画面；拖拽选区实时显示尺寸；确认后剪贴板可粘贴
**Why human:** CI 探针仅窗口创建 + 合成事件 + 内存 framebuffer，真实捕获/剪贴板交互需真机

### 3. Windows 命令面板交互（ROADMAP SC3）

**Test:** Windows 真机唤出命令面板并执行命令
**Expected:** Ctrl+Shift+Space 唤出；中文命令名有字形（CJK 字体链生效）；键盘导航 + 回车执行
**Why human:** 需要真实键盘/鼠标输入与真实显示器渲染

### 4. Windows 高 DPI 选区一致性（ROADMAP SC4 真机确认）

**Test:** 150% 缩放下截图，核对选区与实际捕获区域边界对齐
**Expected:** 选区与捕获像素一致（point_to_physical 换算在真实显示器上的视觉确认）
**Why human:** 纯函数单测证明逻辑正确性；真实 DPI 显示环境的视觉一致性需真机确认

### Gaps Summary

**无自动化验证缺口。** 9/9 可编程验证的 must-haves 全部 VERIFIED：

- CI 基础设施（04-01）：workflow 8 步骤全绿（末次代码推送 run 32097933874 + 32098172216），245/245 单测、16/16 探针 OK、CJK 字体测试通过、4 个 action SHA-pin、dependabot 双条目。
- 9 项错误债（04-02）：WR-01（on_create_failed take-once + session 复位）、WR-02（dispatch_window_event 双臂 catch_unwind + 真实窗口测试）、WR-03（effective_window_height 零命令 144）、IN-01（spawn-Err hop 经 Arc<Mutex<Option>> 恰一次）、IN-02（锁序文档）、IN-03（realize_window destroy + 注释修正）、IN-04（Option<PathBuf> 显式 bail）、IN-05（TextEdit char_limit 64）、IN-06（ROW_BAND_*/ACCENT_RGB 具名常量）——逐项代码层确认 + 测试本地/CI 双绿。
- DPI（04-02）：point_to_physical 纯函数 3 测试 + compute_geometry @1.5 手算用例全绿。
- 本地 macOS：235/240（5 个已知 flaky 窗口时序测试隔离运行全 PASS，Windows CI 稳定绿）；ignored 20/20 绿。

唯一非实现类发现：REQUIREMENTS.md traceability 表 INFRA-04 行未更新（文档滞后，建议随后续 docs 更新修正）；04-VALIDATION.md 行仍为 ⬜ pending（本 phase 刚完成，验证行未回填——非实现缺口）。

---

_Verified: 2026-08-18T12:30:00Z_
_Verifier: the agent (gsd-verifier)_