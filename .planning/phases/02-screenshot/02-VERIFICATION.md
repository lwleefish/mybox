---
phase: 02-screenshot
verified: 2026-08-13T05:53:17Z
status: human_needed
score: 6/6 must-haves verified (code-level; observable outcomes pending human confirmation)
overrides_applied: 0
human_verification:
  - test: "实际触发截图：在真实 macOS 桌面会话运行 `cargo run -p mybox-app`，授予 Screen Recording 权限后按 Cmd+Shift+S（或托盘「开始截图」），观察每块显示器是否出现覆盖窗口并显示捕获画面 + 半透明遮罩"
    expected: "每屏出现全屏覆盖窗口，显示捕获画面，选区外为半透明黑色遮罩"
    why_human: "覆盖窗口在真实显示器上的实际显示需要 Screen Recording 权限 + 物理显示，headless 无法验证；代码级接线与合成逻辑已通过单测与 capture_checks/overlay_capture（#[ignore]）覆盖"
  - test: "选区 → Enter/确认 → 在其它应用（Preview/TextEdit）粘贴，验证剪贴板图像含标注；不标注直接确认验证粘贴原图"
    expected: "粘贴出的图像尺寸=选区、包含已绘制标注；无标注时粘贴原始选区图像"
    why_human: "arboard set_image 写入系统剪贴板需要真实前台应用会话与剪贴板连接；crop/bake 纯逻辑已单测，但实际系统剪贴板往返需人工粘贴确认"
  - test: "macOS 首次截图权限引导：从系统设置移除 mybox 的屏幕录制授权后触发截图，观察系统授权弹窗与设置深链引导"
    expected: "出现系统授权弹窗（或自动打开 系统设置→隐私与安全性→屏幕录制），终端输出引导日志"
    why_human: "CGRequestScreenCaptureAccess 系统弹窗与 open 深链为 macOS 系统 UI，无法 headless 断言；request/open 调用链路已通过注入单测（lib.rs denied 路径）验证"
  - test: "标注/选区交互手感：拖拽选区实时边框与 WxH、8 手柄调整、四类标注绘制、Ctrl+Z 逐步撤销、ESC 一键取消"
    expected: "与 manual_checklist.md 第 2-6 步一致；AlwaysOnTop-only 覆盖限制（A3）为已知 MVP 限制"
    why_human: "实时渲染与输入手感属于视觉/交互质量，逻辑已通过 selection/session/annotate/toolbar 单测 + capture_checks/drag_selection、esc_destroy headless 检查"
---

# Phase 2: 截图模块 Verification Report

**Phase Goal:** 实现完整截图功能：屏幕捕获、区域选择、标注工具、剪贴板复制。用真实功能验证 Phase 1 的框架可用性。
**Verified:** 2026-08-13T05:53:17Z
**Status:** human_needed（代码级实现与接线全部核实通过；6 项成功标准的可观察结果需真实显示器/剪贴板/系统权限弹窗的人工确认）
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths（ROADMAP 成功标准 1-6）

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | 用户按热键触发截图，覆盖窗口出现并显示屏幕画面 | ✓ VERIFIED (code) | `lib.rs` init 订阅 `core/hotkey.triggered`(action=`start_screenshot`)→`start_capture`→worker 线程 `capture_all_monitors`(xcap)→`UiThreadProxy` 回写 `store_shots`→`overlay::create_overlays`；`overlay.rs` `composite_frame` blit 捕获 + 遮罩；`window_attributes` Overlay=transparent+undecorated+AlwaysOnTop |
| 2 | 用户拖拽选择区域，实时显示选区边框和尺寸 | ✓ VERIFIED (code) | `selection.rs`(normalize/hit_test_handle/apply_handle_drag/drag_start/drag_update) + `session.on_mouse_down/move/up` + `overlay.draw_selection_overlay`(边框/8 手柄/WxH 标签)；`capture_checks drag_selection` headless 通过 |
| 3 | 用户确认截图后，选区图像在剪贴板中可用 | ✓ VERIFIED (code) | `overlay.confirm_and_copy`(Enter/工具栏确认)→`clipboard.crop_image`→`bake_annotations`→`copy_to_clipboard`(arboard set_image + macOS exclude_from_history)→`session.finish` 销毁 overlay；`session.confirm` 纯快照幂等 |
| 4 | 用户可在截图上绘制矩形、箭头、画笔、文字标注 | ✓ VERIFIED (code) | `annotate.rs` `Annotation::draw`(Rect/Arrow/Pen/Text) + `toolbar.rs`(7 按钮 layout/hit_test/draw) + `session.on_annotation_*` + `overlay.handle_overlay_event` 按 current_tool 分流 |
| 5 | 用户按 Ctrl+Z 撤销标注，按 ESC 取消截图 | ✓ VERIFIED (code) | `session.undo()`(AnnotationList pop) + `overlay` ModifiersChanged(CONTROL\|SUPER)+'z'→undo；Escape→`cancel_overlays`→`session.cancel()`(finish 清空并返回 overlay_ids)→destroy；`capture_checks esc_destroy` 幂等通过 |
| 6 | macOS 首次截图时提示用户授予屏幕录制权限 | ✓ VERIFIED (code) | `permission.rs` `real_access_checker`(CGPreflightScreenCaptureAccess)/`request_access`(CGRequestScreenCaptureAccess)/`open_system_settings`(深链 x-apple.systempreferences…Privacy_ScreenCapture) + `lib.rs start_capture` denied→request→复检→open+引导→abort |

**Score:** 6/6 成功标准在代码层面实现并接线。1/3/6（及 2/4/5 的交互手感）的可观察结果需人工在真实显示会话确认（见 human_verification 与 manual_checklist.md）。

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/modules/capture/src/capture.rs` | xcap 全屏捕获 + 物理像素几何 | ✓ VERIFIED | `capture_all_monitors`(Monitor::all+capture_image，x/y×scale_factor)、`CaptureFn` Arc 可注入 |
| `crates/modules/capture/src/permission.rs` | 权限预检/请求/深链 | ✓ VERIFIED | `check_access`/`real_access_checker`(Preflight)/`request_access`(Request)/`open_system_settings`，cfg(macos) 分离 |
| `crates/modules/capture/src/session.rs` | 共享状态 + 选区/标注状态机 + confirm/finish | ✓ VERIFIED | SessionState 全字段 + `on_mouse_*`/`tool_action`/`undo`/`push_annotation`/`confirm`(纯快照)/`finish`/`cancel`(委托 finish) |
| `crates/modules/capture/src/overlay.rs` | 每屏覆盖窗 + 合成 + 输入路由 | ✓ VERIFIED | `premultiply_rgba8`/`create_overlays`/`composite_frame`/`draw_selection_overlay`/`handle_overlay_event`/`confirm_and_copy`/`cancel_overlays` |
| `crates/modules/capture/src/selection.rs` | 8 手柄纯几何 | ✓ VERIFIED | Handle(8)/normalize/handle_rect/hit_test_handle/apply_handle_drag(MIN 4px)/drag_start/drag_update |
| `crates/modules/capture/src/annotate.rs` | 标注模型 + 绘制 + 撤销栈 | ✓ VERIFIED | `Annotation::draw` 四变体(橙 0xFF6000/3px round) + `AnnotationList`(push/undo/is_empty/iter) |
| `crates/modules/capture/src/toolbar.rs` | 工具栏布局/命中/绘制 | ✓ VERIFIED | ToolAction(Confirm/Cancel/Undo/Tool)+7 按钮 layout_buttons/hit_test/draw_toolbar |
| `crates/modules/capture/src/text.rs` | ab_glyph 文本光栅化 | ✓ VERIFIED | load_font(Arial.ttf+OnceLock)/draw_text(color 参数) |
| `crates/modules/capture/src/clipboard.rs` | 裁剪+烘焙+arboard | ✓ VERIFIED | crop_image(clamp)/bake_annotations(翻译-origin+反预乘)/copy_to_clipboard(受限作用域+exclude_from_history) |
| `crates/modules/capture/src/bin/capture_checks.rs` | 子进程端到端检查 | ✓ VERIFIED | 4 check_*(overlay_capture/drag_selection/enter_clipboard/esc_destroy) |
| `crates/modules/capture/tests/integration.rs` | #[ignore] 集成测试 | ✓ VERIFIED | 4 #[ignore] 测试 run_check 子进程 |
| `crates/modules/capture/tests/manual_checklist.md` | 人工清单(成功标准 1-6) | ✓ VERIFIED | 8 步 + 权限首次引导 + A3 限制 |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| 热键/托盘 | capture worker | `lib.rs init` 订阅 hotkey.triggered/menu.triggered → `start_capture` | WIRED | 两个 handler 各 clone 依赖，`action=="start_screenshot"`/`menu_id=="capture.start"` 白名单 |
| capture worker | SessionState | `UiThreadProxy::run` 回写 `store_shots` | WIRED | worker 线程 `"mybox-capture"` 捕获后 `ui.run(store_shots + create_overlays)` |
| `create_overlays` | 覆盖窗口 | `WindowManagerHandle::create` 每屏一 WindowSpec | WIRED | Overlay + physical geometry + on_event/on_draw 均 Some |
| 窗口创建 | overlay_ids | `core/window-created` → `session.window_created` | WIRED | pending_overlays 配对，供 ESC/确认销毁 |
| 捕获图像 | 渲染 | `on_draw` → `premultiply_rgba8` → `draw_pixmap` | WIRED | straight→premultiplied (Pitfall 2) |
| 鼠标/键盘 | 选区/标注状态 | `on_event` → `session.on_mouse_*`/`on_annotation_*` | WIRED | 每次变更 `redraw_all_overlays` |
| Enter/确认 | 剪贴板 | `confirm_and_copy` → crop→bake→`copy_to_clipboard`→finish→destroy | WIRED | 主线程执行(on_event)，失败不关窗可重试 |
| 权限预检失败 | 引导 | `start_capture` denied → request → 复检 → open_system_settings → abort | WIRED | 绝不静默黑图 (T-2-02) |
| 框架 Redraw | winit | `WindowManagerHandle::redraw` → `about_to_wait` Redraw 分支 → `request_redraw` | WIRED | window.rs + app.rs |
| 渲染管线 | on_draw | `handle_redraw` draw(spec.on_draw, catch_unwind)→present | WIRED | app.rs，单测 redraw_draws_then_presents |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| --- | --- | --- | --- | --- |
| overlay on_draw | `state.shots[monitor_index]` | xcap `capture_image()`（真实 OS 捕获） | Yes (需权限+显示) | ✓ FLOWING |
| overlay 选区 | `state.selection` | 鼠标拖拽事件 → `on_mouse_down/move` | Yes | ✓ FLOWING |
| overlay 标注 | `state.annotations` | `on_annotation_start/update/finish` push | Yes | ✓ FLOWING |
| clipboard 确认 | `confirm()` 快照 shot | `state.shots` 克隆（真实捕获图） | Yes | ✓ FLOWING |
| clipboard set_image | `baked` bytes | crop + bake(注释) 或原始 crop | Yes (需真实剪贴板会话) | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| 拖拽选区状态机达 Selected | `./target/debug/capture_checks drag_selection` | `OK` exit 0 | ✓ PASS |
| ESC 取消幂等销毁 | `./target/debug/capture_checks esc_destroy` | `OK` exit 0 | ✓ PASS |
| 覆盖窗合成/present | `capture_checks overlay_capture`（#[ignore]） | 需真实显示会话 | ? SKIP → human |
| 确认复制剪贴板回读 | `capture_checks enter_clipboard`（#[ignore]） | 需真实剪贴板会话 + 会写系统剪贴板 | ? SKIP → human |
| 全量快速套件 | `cargo test -p mybox-capture` | 56 passed, 4 ignored | ✓ PASS |
| 核心套件 | `cargo test -p mybox-core` | 72 passed, 4 ignored | ✓ PASS |
| 测试模块(Redraw 修复) | `cargo test -p mybox-test` | 5 passed | ✓ PASS |
| workspace 编译 | `cargo check --workspace` | exit 0, 无 warning | ✓ PASS |

### Probe Execution

Phase 02 未声明任何 `scripts/*/tests/probe-*.sh` 探针。端到端验证采用 `capture_checks` 子进程 harness + `#[ignore]` 集成测试（`crates/modules/capture/tests/integration.rs`）形式。已直接执行 headless 安全的两个 check（drag_selection/esc_destroy，均 exit 0）；overlay_capture/enter_clipboard 需真实显示/剪贴板会话，属 human_verification 范畴。无 MISSING_PROBE。

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| --- | --- | --- | --- | --- |
| CAP-01 | 02-01 | 热键触发，捕获所有显示器到内存 | ✓ SATISFIED | capture.rs `capture_all_monitors` + lib.rs hotkey/menu 双入口 + worker 线程 |
| CAP-02 | 02-02 | 透明覆盖窗 + 遮罩 + 选区内原图 | ✓ SATISFIED | overlay.rs create_overlays + composite_frame(blit+mask_outside) |
| CAP-03 | 02-02 | 拖拽选区 + 实时边框 WxH | ✓ SATISFIED | selection.rs + session 状态机 + draw_selection_overlay |
| CAP-04 | 02-04 | 确认后选区复制到剪贴板 | ✓ SATISFIED | clipboard.rs + overlay confirm_and_copy |
| CAP-05 | 02-02 | ESC 取消，覆盖窗销毁 | ✓ SATISFIED | overlay Escape → cancel_overlays → destroy |
| CAP-06 | 02-03 | 矩形/箭头/画笔/文字标注 | ✓ SATISFIED | annotate.rs + toolbar.rs + session on_annotation_* |
| CAP-07 | 02-03 | Ctrl+Z 撤销到原始状态 | ✓ SATISFIED | AnnotationList undo pop + overlay Ctrl+Z 检测 |
| CAP-08 | 02-01/02-04 | macOS 权限检测 + 引导授权 | ✓ SATISFIED | permission.rs preflight/request/settings 深链 + lib.rs denied 流程 |

**8/8 requirement IDs 在代码中有覆盖，无 ORPHANED。**

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| --- | --- | --- | --- | --- |
| `session.rs` | 292/295 | `tool_action` 的 Confirm/Cancel 分支为 log no-op | ℹ️ Info | 死代码——overlay.rs 工具栏命中路径直接调用 `confirm_and_copy`/`cancel_overlays`，不经过此分支；无功能影响 |
| `app.rs` | 30 | 注释 "not implemented in Phase 1"（quit 菜单） | ℹ️ Info | Phase 1 遗留说明，非 Phase 2 范围 |

无 BLOCKER / WARNING。无 TBD/FIXME/XXX/TODO 未引用 debt 标记。

### Human Verification Required

（详见 frontmatter `human_verification`，与 `crates/modules/capture/tests/manual_checklist.md` 8 步清单一致）

1. **真实截图触发与覆盖窗口显示**（成功标准 1）—— 需 Screen Recording 权限 + 物理显示
2. **剪贴板粘贴验证含标注/原图**（成功标准 3）—— 需真实前台应用会话 + 系统剪贴板
3. **macOS 首次权限弹窗 + 设置深链引导**（成功标准 6）—— 系统 UI 无法 headless 断言
4. **标注/选区交互手感**（成功标准 2/4/5 的可观察质量）—— 逻辑已 headless 验证，视觉/交互需人工

### Gaps Summary

无功能性 gap。Phase 2 的 8 项需求（CAP-01..CAP-08）与 6 项 ROADMAP 成功标准全部在代码中实现并接线：

- **框架链路**：`on_draw` 渲染链（draw→present + catch_unwind）、`WindowRequest::Redraw`、`ModuleContext::bus()`、`AppEvent::Ui` catch_unwind 全部落地，关闭 Phase 1 遗留 WR-05/WR-06/WR-09。
- **端到端垂直切片**：热键/托盘 → 权限预检 → xcap 工作线程捕获 → 每屏覆盖窗（画面+遮罩）→ 拖拽选区（8 手柄+WxH）→ 四类标注+统一工具栏 → Ctrl+Z 撤销 → Enter/确认复制（含标注烘焙）→ 覆盖窗关闭 / ESC 取消 → 无权限时请求+深链引导。
- **测试**：mybox-capture 56 passed、mybox-core 72 passed、mybox-test 5 passed（含 Redraw match arm 修复 e231d6c）、`cargo check --workspace` 无 warning；headless 行为检查 drag_selection/esc_destroy 通过。

唯一剩余的验证动作是**人工确认**：覆盖窗口在真实屏幕的显示、剪贴板实际粘贴、macOS 权限弹窗——这些均无法在 headless 环境断言，代码已正确实现，交由 human_verification 与 manual_checklist.md 闭环。`status: human_needed`（非 gaps_found）。

已知 MVP 限制（非缺陷，Phase 4 重评）：覆盖窗 AlwaysOnTop-only（A3，可能不覆盖全屏应用/菜单栏）、文字工具放置固定 "Text"（A6，无文本编辑）。

---

_Verified: 2026-08-13T05:53:17Z_
_Verifier: the agent (gsd-verifier)_
