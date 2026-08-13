---
phase: 02-screenshot
plan: 04
subsystem: screenshot
tags: [clipboard, arboard, confirm, enter, crop, annotation-bake, permission, cgrequest, screen-recording, settings-deeplink, integration-test, manual-checklist]

# Dependency graph
requires:
  - phase: 02-screenshot
    plan: 03
    provides: retained AnnotationList + Annotation::draw (rect/arrow/pen/text), unified toolbar (Confirm/Cancel/Undo/tools), SessionState with annotations/current_tool/phase, Ctrl+Z undo
  - phase: 02-screenshot
    plan: 01
    provides: SessionState.shots (xcap RgbaImage, physical-pixel MonitorGeom), check_access(AccessChecker), start_capture worker-thread flow, overlay_ids teardown
provides:
  - clipboard.rs: crop_image (axis-aligned RGBA8 sub-rect) + bake_annotations (premultiply→draw→unpremultiply, annotation-origin translation) + copy_to_clipboard (confined-scope arboard set_image, macOS exclude_from_history)
  - session.rs: confirm() pure snapshot + finish() full teardown returning overlay_ids (drop-before-close); cancel() now drops shots too
  - overlay.rs: Enter/Confirm → confirm_and_copy (crop/bake/copy/destroy/finish/emit capture/screenshot-taken); Cancel/ESC → cancel_overlays
  - permission.rs: request_access (CGRequestScreenCaptureAccess) + open_system_settings (x-apple.systempreferences deep link)
  - lib.rs: start_capture denied → request → recheck → open settings + guidance → abort (CAP-08); request/open injectable
  - capture_checks bin + tests/integration.rs + manual_checklist.md: E2E #[ignore] checks + manual steps
affects: Phase 3 (command palette — consumes capture/screenshot-taken), Phase 4 (Windows port: clipboard thread-affinity already confined-scope; permission stubs no-op)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "confirm flow runs entirely on the main thread from on_event: session.confirm() (pure snapshot) → crop → bake → copy_to_clipboard (confined arboard scope) → finish() drains overlay_ids for destroy → emit bus event"
    - "annotation baking translates each Annotation by -crop_origin before drawing, because annotations are stored in MONITOR pixels while the crop pixmap is selection-local"
    - "premultiply→Pixmap→draw→unpremultiply round-trip: straight RGBA8 (xcap/arboard) ⇄ premultiplied (tiny-skia Pixmap)"
    - "injectable request/open as Arc<dyn Fn> so the denied-permission path is headless-testable with counting fakes (CaptureFn/AccessChecker precedent)"
key-files:
  created:
    - crates/modules/capture/src/clipboard.rs
    - crates/modules/capture/src/bin/capture_checks.rs
    - crates/modules/capture/tests/integration.rs
    - crates/modules/capture/tests/manual_checklist.md
  modified:
    - crates/modules/capture/src/session.rs
    - crates/modules/capture/src/overlay.rs
    - crates/modules/capture/src/permission.rs
    - crates/modules/capture/src/lib.rs
    - crates/modules/capture/Cargo.toml
    - crates/mybox-core/src/context.rs

key-decisions:
  - "confirm() is a pure snapshot (monitor_index, rect, shot, annotations) with no state mutation; finish() performs the full teardown and returns the drained overlay_ids — a failed clipboard copy can be retried without leaking overlay_ids (the plan's 'confirm drains overlay_ids' would leak ids on retry)"
  - "clipboard copy is excluded from history on macOS via arboard SetExtApple::exclude_from_history (org.nspasteboard.ConcealedType, T-2-13)"
  - "the screenshot-taken bus event carries an empty Module payload ({}); the image itself lives in the clipboard, not on the bus — Phase 3 consumers needing metadata can extend it"
  - "CGRequestScreenCaptureAccess is a safe generated wrapper (no outer unsafe), matching 02-01's CGPreflightScreenCaptureAccess finding"

patterns-established:
  - "ModuleContext::bus() accessor — the framework now lets modules emit events from their own 'static callbacks (mirrors the 02-02 winit re-export rationale)"
  - "capture_checks subprocess-per-check harness mirrors mybox-core's display_checks: one check per process on the real main thread, exit 0/1/2"

requirements-completed: [CAP-04, CAP-08]

# Metrics
duration: single-session (~35 min)
completed: 2026-08-13
---

# Phase 2 Plan 4: 剪贴板确认 + 权限引导 + 端到端验证 Summary

**Enter/工具栏确认 → 选区（含标注）裁剪烘焙 → arboard 复制到剪贴板 → 覆盖窗口关闭；macOS 无屏幕录制权限时请求授权并深链引导到系统设置——打通 Select → Annotate → Confirm 全流程（CAP-04, CAP-08, D-01, D-04），并交付端到端 `#[ignore]` 集成测试与人工清单，完成 Phase 成功标准 1-6 验证路径**

## Performance

- **Duration:** single-session (~35 min)
- **Completed:** 2026-08-13
- **Tasks:** 3
- **Files modified:** 10（4 created + 6 modified）

## Accomplishments

- `clipboard.rs`：`crop_image`（RGBA8 straight 逐行轴对齐子矩形拷贝，越界 clamp T-2-15）+ `bake_annotations`（空列表返回原裁剪字节 D-01；非空 premultiply → `Pixmap::from_vec` → 逐标注 `draw`（翻译 -origin）→ 反 premultiply 回 straight）+ `copy_to_clipboard`（受限作用域 `Clipboard::new → set_image → drop`，macOS `exclude_from_history` 隐藏系统剪贴板历史 T-2-13）
- `session.rs`：`confirm()` 纯快照（monitor_index/rect/shot/annotations，无副作用、幂等，可重试）+ `finish()` 全量清空并返回 overlay_ids（drop-before-close T-2-01）；`cancel()` 改为完整清空（含 shots），关闭 02-03「shots 清空留待 02-04」的取消路径缺口
- `overlay.rs`：Enter / 工具栏确认 → `confirm_and_copy`（主线程 crop→bake→copy→销毁全部 overlay→finish→emit `capture/screenshot-taken`；失败仅 log 不关窗）；工具栏取消 / ESC → `cancel_overlays`（完整 teardown）
- `permission.rs`：`request_access()`（macOS `CGRequestScreenCaptureAccess` 安全包装）+ `open_system_settings()`（`x-apple.systempreferences:...Privacy_ScreenCapture` 深链，编译期常量、不经 shell，T-2-14）
- `lib.rs`：`start_capture` denied 路径 → request → 复检仍 denied → open settings + 引导日志 → abort（CAP-08 完整链路，绝不复制黑图 T-2-02）；request/open 以 `Arc<dyn Fn>` 注入
- `capture_checks` bin + `tests/integration.rs` + `manual_checklist.md`：4 个 `#[ignore]` 子进程端到端检查（overlay 合成/present、拖拽状态机、确认复制回读、ESC 幂等销毁）+ 8 步人工清单（Phase 成功标准 1-6）

## Task Commits

Each task was committed atomically:

1. **Task 1: 剪贴板复制 + 确认流程（CAP-04, D-01, D-04）** - `7ccd471` (feat)
2. **Task 2: macOS 屏幕录制权限请求 + 引导（CAP-08）** - `d695e1b` (feat)
3. **Task 3: 端到端集成测试 + 手动清单（Phase 成功标准闭环）** - `5d281a0` (test)

**Plan metadata:** 本 SUMMARY 由 executor 独立提交（orchestrator 负责 STATE.md/ROADMAP.md/REQUIREMENTS.md 写回，见 working-tree 约定）。

## Files Created/Modified

- `crates/modules/capture/src/clipboard.rs` - `crop_image`/`bake_annotations`/`copy_to_clipboard` + `translate_annotation`/`unpremultiply_rgba8` + 6 个单测（子矩形精确字节、越界 clamp、空列表原样返回、橙色烘焙、origin 翻译、反预乘）
- `crates/modules/capture/src/session.rs` - `ConfirmSnapshot` + `confirm`/`finish`/`emit`/`set_bus`（OnceLock bus）+ `cancel` 委托 finish + 3 个新单测（confirm 幂等、空选区 None、finish 清空）
- `crates/modules/capture/src/overlay.rs` - Enter/Confirm/Cancel 接线 + `confirm_and_copy`/`cancel_overlays` helper + log/Event/EventPayload import
- `crates/modules/capture/src/permission.rs` - `request_access`/`open_system_settings` + `check_access` 委托单测
- `crates/modules/capture/src/lib.rs` - `AccessRequester`/`SettingsOpener` 类型 + `CaptureModule` request/open 字段 + `start_capture` 权限流程 + 重写 denied 测试（断言 request/open 各调用一次）
- `crates/modules/capture/Cargo.toml` - `serde_json` 提升为运行时依赖（bus payload）
- `crates/mybox-core/src/context.rs` - `ModuleContext::bus()` accessor（模块可发事件）
- `crates/modules/capture/src/bin/capture_checks.rs` - `OverlayHarness` + 4 个 check_*（overlay_capture/drag_selection/enter_clipboard/esc_destroy）
- `crates/modules/capture/tests/integration.rs` - `CARGO_BIN_EXE_capture_checks` + `run_check` + 4 个 `#[ignore]` 测试
- `crates/modules/capture/tests/manual_checklist.md` - Phase 成功标准 1-6 人工步骤（含首次权限引导 + AlwaysOnTop 已知限制）

## Decisions Made

- **confirm 纯快照 vs 计划「confirm 清 overlay_ids」：** confirm 返回 `ConfirmSnapshot` 且不改状态，finish 清空并返回 overlay_ids——修复「首次复制失败后 overlay_ids 已被排空、重试无法销毁窗口」的缺陷
- **标注烘焙坐标：** 标注存的是显示器像素坐标，烘焙进裁剪图需先翻译 `-origin`（计划签名遗漏偏移量，否则标注落错位置）
- **剪贴板历史：** macOS 上 `exclude_from_history`（ConcealedType）隐藏截图历史（Claude 裁定：截图不进历史更安全）
- **`screenshot-taken` 事件：** 携带空 Module payload（图像在剪贴板而非 bus），留作 Phase 3 命令面板等消费信号
- **权限注入：** request/open 用 `Arc<dyn Fn>`（非 fn 指针）以便无头测试用计数闭包断言 denied 路径

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] 新增 `ModuleContext::bus()` accessor（mybox-core）**
- **Found during:** Task 1（overlay 确认流程 emit `capture/screenshot-taken`）
- **Issue:** `ModuleContext.bus` 是 `pub(crate)`，模块 crate 无法取得 `Arc<EventBus>` 以从 `'static` on_event 闭包发事件；`ctx.emit` 是 `&self` 方法不可移入闭包。
- **Fix:** 在 `ModuleContext` 增加 `pub fn bus(&self) -> &Arc<EventBus>`（与 02-02 重导出 winit 同类的框架扩展）。
- **Files modified:** crates/mybox-core/src/context.rs
- **Committed in:** 7ccd471

**2. [Rule 3 - Blocking] `serde_json` 提升为运行时依赖**
- **Found during:** Task 1（overlay 构造 `EventPayload::Module`）
- **Issue:** `EventPayload::Module` 需 `serde_json::Value`，但 serde_json 仅在 dev-dependencies（02-01 加入），生产代码无法构造。
- **Fix:** 将 `serde_json.workspace = true` 移入 `[dependencies]`（版本已锁定于 workspace，无新版本）。
- **Files modified:** crates/modules/capture/Cargo.toml
- **Committed in:** 7ccd471

**3. [Rule 1 - Bug] confirm 清 overlay_ids 导致复制失败重试时窗口泄漏**
- **Found during:** Task 1（confirm/finish 设计）
- **Issue:** 计划让 confirm 排空 overlay_ids；若首次复制失败（不关窗），重试 Enter 时 overlay_ids 已空，成功销毁步骤无 id 可销毁。
- **Fix:** confirm 改为纯快照（无副作用）；finish 排空 overlay_ids 并返回。失败可重试，成功统一走 finish。
- **Files modified:** crates/modules/capture/src/session.rs
- **Committed in:** 7ccd471

**4. [Rule 1 - Bug] bake_annotations 缺少标注坐标偏移**
- **Found during:** Task 1（clipboard.rs bake）
- **Issue:** 计划签名 `bake_annotations(cropped, w, h, &AnnotationList)` 无偏移；但标注存的是显示器像素坐标，直接 `Annotation::draw` 到 w×h 裁剪图会落在错误位置（选区原点非 0 时标注全部越界）。
- **Fix:** 签名改为 `bake_annotations(cropped, w, h, &[Annotation], origin: Point)`，逐标注 `translate(-origin)` 后绘制；ConfirmSnapshot.annotations 用 `Vec<Annotation>` 避免给 AnnotationList 加 Clone。
- **Files modified:** crates/modules/capture/src/clipboard.rs, session.rs, overlay.rs
- **Committed in:** 7ccd471

**5. [Rule 2 - Missing] 工具栏 Cancel 按钮接线 + cancel 清空 shots**
- **Found during:** Task 1（overlay 工具路由 / 02-03 Known Stubs）
- **Issue:** 02-03 遗留 `tool_action` 的 Confirm/Cancel 均为日志 no-op，计划只显式要求 Confirm；Cancel 按钮若不接线会与 ESC（D-04）行为不一致；且 ESC 的 `cancel()` 未清 shots（02-03「留待 02-04」的 T-2-01 缺口）。
- **Fix:** `ToolAction::Cancel` 走 `cancel_overlays`（完整 teardown）；`cancel()` 委托 `finish()`（同时清 shots/annotations）。
- **Files modified:** crates/modules/capture/src/overlay.rs, session.rs
- **Committed in:** 7ccd471

**6. [Rule 3 - Blocking] `CGRequestScreenCaptureAccess` 外层 unsafe 冗余**
- **Found during:** Task 2（permission.rs `request_access`）
- **Issue:** 计划写 `unsafe { CGRequestScreenCaptureAccess() }`，但 objc2-core-graphics 0.3.2 生成的是安全包装（内部已完成 unsafe），外层 `unsafe` 触发 `unused_unsafe`（02-01 偏差 #4 同因）。
- **Fix:** 直接安全调用。
- **Files modified:** crates/modules/capture/src/permission.rs
- **Committed in:** d695e1b

---

**Total deviations:** 6 auto-fixed（2 framework/deps boundary + 4 correctness/API-shape）
**Impact on plan:** 全部为实现正确性与模块边界必需；无范围蔓延，依赖清单保持在 02-01 锁定的 xcap/arboard/ab_glyph/objc2-core-graphics 内（serde_json 仅提升为运行时依赖，版本未变）。

### Out-of-scope discovery (NOT fixed — pre-existing)

- `crates/modules/test/src/lib.rs`（mybox-test）的 `hotkey_trigger_enqueues_test_window_and_wakes_once` 测试对 `WindowRequest` 的 match 缺 `Redraw(_)` 分支，`cargo nextest run`（全量，含 mybox-test）会编译失败。此问题自 02-01 引入 `WindowRequest::Redraw` 起即存在，属本 plan 范围外（不修改 mybox-test），按 scope-boundary 规则记录于此。快速套件（`-p mybox-core -p mybox-capture`）不受影响。

## Known Stubs

- `crates/modules/capture/src/overlay.rs` — `confirm_and_copy` 发 `capture/screenshot-taken` 时携带空 `EventPayload::Module(json!({}))`：图像本身在系统剪贴板（无法也不应序列化进 bus），事件仅作「截图已复制」信号；Phase 3 命令面板若需元数据可在后续计划扩展 payload。此为有意设计，不阻塞 CAP-04/08 目标。

## Issues Encountered

- **模块发事件的边界：** `ModuleContext` 此前只暴露 `emit`/`on`，模块无法从 `'static` 回调发事件；新增 `bus()` accessor 后，`CaptureSession` 以 `OnceLock<Arc<EventBus>>` 承载（`set_bus` 在 init 注入），无头测试中 `emit` 为 no-op。
- **标注坐标空间：** overlay 绘制与裁剪烘焙共享同一显示器像素坐标系，但裁剪图为选区局部坐标——烘焙必须翻译 `-origin`，否则含偏移选区时标注错位。

## User Setup Required

None - no external service configuration required.

macOS Screen Recording 权限为一次性系统授权（用户操作，非工具）：`start_capture` 在 denied 时请求授权弹窗、被拒后深链打开系统设置屏幕录制面板并提示「授权后可能需要重启 mybox」（A1）。人工清单第 7 步覆盖首次引导验证；AlwaysOnTop-only 覆盖窗口层级（A3）为已接受的 MVP 限制，Phase 4 重评。

## Next Phase Readiness

- Phase 2 垂直切片闭环：热键/托盘 → 捕获 → 每屏覆盖窗 → 拖拽选区（8 手柄 + WxH）→ 四类标注 + Ctrl+Z → Enter/确认复制含标注图像（不标注复制原图）→ 覆盖窗关闭 / ESC 一键取消；无权限时请求 + 深链引导
- `capture/screenshot-taken` 事件可供 Phase 3 命令面板或后续模块消费；`ModuleContext::bus()` 为通用「模块发事件」能力
- 威胁缓解落实：T-2-01（finish/cancel 清 shots）、T-2-02（CAP-08 完整链路）、T-2-12（受限作用域 + 失败不关窗）、T-2-13（exclude_from_history）、T-2-14（深链编译期常量不经 shell）、T-2-15（confirm 空选区/越界 clamp）
- 已知限制：覆盖窗口 AlwaysOnTop-only（A3）、文字工具固定 "Text"（A6，02-03）——均 MVP 接受，Phase 4 重评

## Self-Check: PASSED

- Files verified: `crates/modules/capture/src/{clipboard,session,overlay,permission,lib}.rs`, `crates/modules/capture/src/bin/capture_checks.rs`, `crates/modules/capture/tests/{integration.rs,manual_checklist.md}`, `crates/modules/capture/Cargo.toml`, `crates/mybox-core/src/context.rs`
- Commits verified: `7ccd471`（Task 1）, `d695e1b`（Task 2）, `5d281a0`（Task 3）
- `cargo nextest run -p mybox-capture`: 56 passed, 4 skipped (exit 0)
- `cargo nextest run -p mybox-core -p mybox-capture`: 128 passed, 8 skipped (exit 0)
- `cargo build --bin capture_checks`: exit 0
- `cargo check --workspace`: exit 0, 无 warning

---

*Phase: 02-screenshot*
*Completed: 2026-08-13*
