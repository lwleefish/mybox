---
phase: 02-screenshot
plan: 01
subsystem: screenshot
tags: [xcap, screen-capture, worker-thread, cgpreflightscreencaptureaccess, ui-thread-proxy, on-draw, render-chain, redraw-request, hotkey]

# Dependency graph
requires:
  - phase: 01-framework
    provides: Module/ModuleContext/EventBus (01-02), WindowManager/Renderer (01-02), HotkeyManager/ConfigCenter/UiThreadProxy (01-03), App event loop (01-04)
provides:
  - on_draw render chain: RedrawRequested -> renderer.draw(spec.on_draw) -> present (closes WR-05)
  - WindowRequest::Redraw + WindowManagerHandle::redraw(id) -> about_to_wait -> request_redraw (closes Pattern 2)
  - batch_create placeholder + renderer_factory dead field removed (fixes WR-09)
  - AppEvent::Ui(f) wrapped in catch_unwind (WR-06 fix)
  - mybox-capture crate: capture_all_monitors (xcap, physical-pixel geometry), permission preflight (CGPreflightScreenCaptureAccess), CaptureSession/SessionState, CaptureModule (hotkey/menu -> preflight -> worker capture -> session)
  - out-of-the-box start_screenshot hotkey registration from [capture].hotkey (default Cmd+Shift+S)
affects: Phase 2 (02-02 overlay, 02-03 annotation, 02-04 clipboard/manual), Phase 3 (command palette), Phase 4 (Windows port)

# Tech tracking
tech-stack:
  added: [xcap 0.9.8, arboard 3.6.1, ab_glyph 0.2.32, objc2-core-graphics 0.3.2 (macOS target)]
  patterns:
    - "worker-thread xcap capture + UiThreadProxy::run handoff back to the main thread (RESEARCH Pattern 4)"
    - "injectable CaptureFn (Arc<dyn Fn>) + AccessChecker (fn pointer) so headless tests substitute fakes"
    - "deferred hotkey registration via ctx.ui().run (module init runs before hotkeys.init())"
    - "physical-pixel geometry conversion: xcap points x scale_factor (the only logical->physical point)"
    - "std::sync::Mutex shared session state in a module crate (FRMW-02 boundary: parking_lot is not re-exported)"
    - "on_draw closure wrapped in catch_unwind (T-2-03) so a panicking module draw cannot kill the loop"

key-files:
  created:
    - crates/modules/capture/Cargo.toml
    - crates/modules/capture/src/lib.rs
    - crates/modules/capture/src/capture.rs
    - crates/modules/capture/src/permission.rs
    - crates/modules/capture/src/session.rs
  modified:
    - Cargo.toml
    - crates/mybox-core/src/window.rs
    - crates/mybox-core/src/app.rs
    - crates/mybox-core/src/lib.rs
    - crates/mybox-core/src/bin/display_checks.rs
    - crates/mybox-app/src/main.rs
    - crates/mybox-app/Cargo.toml

key-decisions:
  - "Capture runs on a named worker thread (mybox-capture); results flow back through UiThreadProxy (AppEvent::Ui), never on the event loop or in the draw closure (Pitfall 4)"
  - "The module depends only on mybox-core + xcap/arboard/ab_glyph; it uses mybox_core::anyhow and std::sync::Mutex (not parking_lot, which is not re-exported across the FRMW-02 boundary)"
  - "start_screenshot hotkey is registered lazily via ctx.ui().run so it lands after hotkeys.init() on the main thread — a direct register_str in init() would fail 'not initialized'"
  - "Permission preflight (CGPreflightScreenCaptureAccess) gates the worker-thread spawn; denied access aborts with a clear log, never a silent black capture (CAP-08)"

patterns-established:
  - "Inject every platform/OS side effect (capture fn, access checker) behind an injectable field so the module is headless-unit-testable"
  - "Session state shared as Arc<std::sync::Mutex<SessionState>> across bus handler + draw closure + on_event"
  - "Module id 'capture' = event from namespace + config section [capture] + menu item id prefix"

requirements-completed: [CAP-01, CAP-08]

# Metrics
duration: multi-session (spans two executor sessions: Tasks 1-2 by prior agent, Task 3 completion by continuation agent)
completed: 2026-08-13
---

# Phase 2 Plan 1: 捕获后端 + on_draw 渲染链路 Summary

**关闭三个 Phase 1 框架缺口（on_draw 调用链、WindowRequest::Redraw 重绘路径、batch_create 占位处置）并交付 mybox-capture 模块 crate：热键/托盘 → macOS Screen Recording 预检 → xcap 工作线程全屏捕获 → SessionState**

## Performance

- **Duration:** multi-session（跨两个 executor 会话：Task 1-2 由前置 agent 完成，Task 3 由续接 agent 完成）
- **Started:** 2026-08-13（前置会话）
- **Completed:** 2026-08-13T04:38:51Z
- **Tasks:** 3
- **Files modified:** 13（5 created + 8 modified）

## Accomplishments

- 关闭 WR-05：`handle_redraw` 在 `RedrawRequested` 分支先 `renderer.draw(spec.on_draw)`（on_draw 闭包以 `catch_unwind` 包裹，T-2-03）再 `present()`；单测 `redraw_draws_then_presents` 断言 draw 先于 present
- 关闭 WR-06：`AppEvent::Ui(f)` 以 `std::panic::catch_unwind(AssertUnwindSafe(f))` 包裹，模块经 `ctx.ui().run` 转发到主线程的闭包 panic 不再终止事件循环
- 新增 `WindowRequest::Redraw(WindowId)` + `WindowManagerHandle::redraw(id)`（send + trigger_wake），`about_to_wait` 内调用 `window.request_redraw()`——模块从 bus 线程安全请求重绘
- 删除 `batch_create` 占位符与 `renderer_factory` 死字段（修复 WR-09 id 冲突风险），`WindowManager::new()` 改为无参（含 display_checks.rs:42 调用点同步）
- 新建 `crates/modules/capture` crate：`capture_all_monitors()`（xcap `Monitor::all()` + `capture_image()`，几何转物理像素 = points × scale_factor）、`permission::check_access`（macOS `CGPreflightScreenCaptureAccess`）、`CaptureSession` + `SessionState`（`Arc<std::sync::Mutex>`）+ `store_shots`
- `CaptureModule`（id "capture"）实现热键（`start_screenshot`）与托盘菜单（`capture.start`）双入口 → 预检 → 命名线程捕获 → `UiThreadProxy` 回写 SessionState；启动时经 ui proxy 延迟注册 `[capture].hotkey`（默认 Cmd+Shift+S），开箱即用无需手改 [hotkeys]

## Task Commits

Each task was committed atomically:

1. **Task 1: xcap/arboard/ab_glyph 包合法性门禁（checkpoint:human-verify）** - 人工批准（无代码产物；`gate="blocking-human"`）
2. **Task 2: 核心渲染链路（on_draw + Redraw + batch_create 处置）** - `e096be9` (feat)
3. **Task 3: mybox-capture 模块 crate + xcap 捕获后端 + 权限预检 + SessionState** - `e6a7285` (feat)

**Plan metadata:** 摘要提交见 `e6a7285` 之后的独立 docs 提交（由 orchestrator 完成后写回）。

## Files Created/Modified

- `crates/modules/capture/src/capture.rs` - `MonitorGeom` + `CaptureFn`（`Arc<dyn Fn>` 可注入）+ `capture_all_monitors()`（xcap 全屏捕获，物理像素几何）
- `crates/modules/capture/src/permission.rs` - `AccessChecker`（fn 指针可注入）+ `real_access_checker()`（macOS `CGPreflightScreenCaptureAccess`，非 macOS 返回 true）+ `check_access`
- `crates/modules/capture/src/session.rs` - `SessionState`（shots/selection/current_tool/annotations/overlay_ids/pending_overlays）+ `SelectionRect`/`Tool`/`Annotation` 枚举 + `CaptureSession::store_shots`
- `crates/modules/capture/src/lib.rs` - `CaptureModule`（注入字段 capture/access）+ `start_capture` + `hotkey_from_config` + 8 个单测
- `crates/modules/capture/Cargo.toml` - mybox-core + xcap/arboard/ab_glyph（workspace 引用）+ macOS objc2-core-graphics 0.3.2 + serde_json（dev）
- `crates/mybox-core/src/window.rs` - `WindowSpec.on_draw` 字段、`WindowRequest::Redraw`、`WindowManagerHandle::redraw`、移除 batch_create/renderer_factory（Task 2）
- `crates/mybox-core/src/app.rs` - `handle_redraw`（draw→present）、`AppEvent::Ui` catch_unwind、`about_to_wait` Redraw 分支、`redraw_draws_then_presents` 测试（Task 2）
- `crates/mybox-core/src/lib.rs` - `pub use tiny_skia;` 重导出（Task 2）
- `crates/mybox-core/src/bin/display_checks.rs` - `WindowManager::new()` 无参（Task 2）
- `crates/mybox-app/src/main.rs` - 注册 `mybox_capture::CaptureModule::new()`
- `crates/mybox-app/Cargo.toml` - 增加 `mybox-capture` 依赖（Rule 3 修正，见 Deviations）
- `Cargo.toml`（workspace）- members 增 capture；workspace.dependencies 增 xcap/arboard/ab_glyph
- `Cargo.lock` - 锁定新增依赖（xcap/arboard/ab_glyph 及 objc2-* 传递依赖）

## Decisions Made

- **捕获线程模型：** xcap 捕获跑在命名 worker 线程（`mybox-capture`），结果经 `UiThreadProxy::run`（`AppEvent::Ui`）回主线程写入 SessionState——绝不在事件循环或 draw 闭包内捕获（Pitfall 4）
- **FRMW-02 边界：** 模块 crate 只依赖 mybox-core + xcap/arboard/ab_glyph；使用 `mybox_core::anyhow`（非裸 `anyhow`）与 `std::sync::Mutex`（非 `parking_lot`，后者未跨边界重导出）
- **热键延迟注册：** `start_screenshot` 经 `ctx.ui().run` 延迟到 `hotkeys.init()` 之后主线程注册；若在 `init()` 内直调 `register_str` 会返回 "not initialized" 静默失败
- **权限预检门禁：** `CGPreflightScreenCaptureAccess` 在 spawn 前判定，未授权则中止 + 明确日志，绝不静默产出黑屏（CAP-08 检测半）

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `parking_lot::Mutex` 替换为 `std::sync::Mutex`**
- **Found during:** Task 3（session.rs 编译）
- **Issue:** 计划的 `Arc<parking_lot::Mutex<SessionState>>` 无法编译——`parking_lot` 既非 mybox-capture 依赖，也未由 mybox-core 重导出；且 Task 3 依赖清单（验收标准）只列 mybox-core + xcap + arboard + ab_glyph + objc2-core-graphics，不含 parking_lot。
- **Fix:** 改用 `std::sync::Mutex`（模块 crate 既有的 `crates/modules/test` 先例同样用 `std::sync::Mutex` 共享状态），`lock().unwrap()` 加 poison 处理入口。符合 FRMW-02（模块只依赖 mybox-core）。
- **Files modified:** crates/modules/capture/src/session.rs
- **Verification:** `cargo nextest run -p mybox-capture` 9 passed
- **Committed in:** e6a7285

**2. [Rule 3 - Blocking] `anyhow::Result` 改为 `mybox_core::anyhow::Result`**
- **Found during:** Task 3（capture.rs 编译）
- **Issue:** 计划 capture.rs 写 `anyhow::Result`，但 `anyhow` 不是模块 crate 的直接依赖（FRMW-02 要求经 mybox-core 重导出使用）。
- **Fix:** 加 `use mybox_core::anyhow;`，`anyhow::Result` 指向重导出（与 test 模块 / PATTERNS 导入段一致）。
- **Files modified:** crates/modules/capture/src/capture.rs
- **Verification:** `cargo check -p mybox-capture` 退出 0
- **Committed in:** e6a7285

**3. [Rule 3 - Blocking] `RgbaImage::new(2,2).unwrap()` 改为 `RgbaImage::new(2,2)`**
- **Found during:** Task 3（单测构造）
- **Issue:** 计划写 `xcap::image::RgbaImage::new(2, 2).unwrap()`，但 image 0.25 的 `ImageBuffer::new(width, height)` 是不可失败的（直接返回 `ImageBuffer`，非 `Result`），`.unwrap()` 不编译。
- **Fix:** 去掉 `.unwrap()`。
- **Files modified:** crates/modules/capture/src/lib.rs（测试）、crates/modules/capture/src/session.rs（测试）
- **Verification:** `cargo nextest run -p mybox-capture` 9 passed
- **Committed in:** e6a7285

**4. [Rule 3 - Blocking] `unsafe { CGPreflightScreenCaptureAccess() }` 改为安全调用**
- **Found during:** Task 3（permission.rs 编写）
- **Issue:** 计划写 `unsafe { objc2_core_graphics::CGPreflightScreenCaptureAccess() }`，但 objc2-core-graphics 0.3.2 生成的绑定是**安全**包装函数（`pub extern "C-unwind" fn ... { unsafe { ... } }`），外层 `unsafe` 会触发 `unused_unsafe`。
- **Fix:** 直接安全调用 `objc2_core_graphics::CGPreflightScreenCaptureAccess()`。
- **Files modified:** crates/modules/capture/src/permission.rs
- **Verification:** `cargo check -p mybox-capture` 无警告退出 0
- **Committed in:** e6a7285

**5. [Rule 3 - Blocking] mybox-app 增加 `mybox-capture` 依赖**
- **Found during:** Task 3（`cargo check --workspace`）
- **Issue:** 计划 Task 3 的 `<files>` 未列 `crates/mybox-app/Cargo.toml`，但 `main.rs` 引用 `mybox_capture::CaptureModule::new()` 必须声明该依赖，否则 `cargo check --workspace` 报 `cannot find module or crate mybox_capture`。
- **Fix:** 在 `crates/mybox-app/Cargo.toml` 的 `[dependencies]` 增加 `mybox-capture = { path = "../modules/capture" }`。
- **Files modified:** crates/mybox-app/Cargo.toml
- **Verification:** `cargo check --workspace` 退出 0
- **Committed in:** e6a7285

**6. [Rule 3 - Blocking] 增加 `serde_json` 为 dev-dependency**
- **Found during:** Task 3（handler 路由单测）
- **Issue:** 计划的 test 3 需构造 `core/menu.triggered` 事件（payload `EventPayload::Module(serde_json::json!({"menu_id": ...}))`），但 `serde_json` 未被 mybox-core 重导出、也不是模块依赖；无法构造 Module payload。
- **Fix:** 在 `crates/modules/capture/Cargo.toml` 增加 `[dev-dependencies] serde_json.workspace = true`（版本已锁定于 workspace.dependencies，无新版本）。
- **Files modified:** crates/modules/capture/Cargo.toml
- **Verification:** `cargo nextest run -p mybox-capture` 9 passed
- **Committed in:** e6a7285

---

**Total deviations:** 6 auto-fixed (6 blocking build/API correctness)
**Impact on plan:** 全部为编译正确性必需的修正（计划在 parking_lot 可用性、RgbaImage::new 返回类型、objc2 绑定安全性与 mybox-app 依赖上有小误差）。无范围蔓延；依赖清单保持在计划验收标准的枚举内（serde_json 仅 dev）。

## Issues Encountered

- **续接状态：** Task 1（包合法性门禁）与 Task 2（核心渲染链路，`e096be9`）由前置 agent 完成；本会话完成 Task 3 剩余工作（capture/src 四文件 + main.rs 注册）并修正 6 处编译阻塞。
- **`cargo check --workspace` 首轮失败：** mybox-app 未声明 `mybox-capture` 依赖（计划 `<files>` 遗漏），补充依赖后通过（见 Deviation 5）。

## User Setup Required

None - no external service configuration required.

macOS Screen Recording 权限为一次性系统授权（用户操作，非工具）。权限预检（CAP-08）在捕获前拦截未授权场景并输出明确日志；授权引导 UI 与手动核对清单属于 02-04，届时在真实桌面会话验证。

## Next Phase Readiness

- mybox-capture crate 编译通过，热键/托盘触发后真实捕获所有显示器到 SessionState（日志可见 `captured N monitors`）；无权限时日志可见权限拒绝提示（CAP-01 + CAP-08 检测半）
- 02-02（覆盖窗口）可直接消费 `SessionState.shots` + `MonitorGeom`（物理像素）+ `WindowSpec.on_draw`/`WindowRequest::Redraw` 渲染链路
- `SelectionRect`/`Tool`/`Annotation` 已声明，供 02-02/02-03/02-04 编译；覆盖窗口显示、选区、标注、剪贴板、授权引导仍待后续计划
- 已知限制：覆盖窗口 AlwaysOnTop-only（非 screenSaver 层级，A3），对全屏应用/菜单栏可能不覆盖——MVP 接受，Phase 4 重评

## Self-Check: PASSED

- Files verified: `crates/modules/capture/src/{lib,capture,permission,session}.rs`, `crates/modules/capture/Cargo.toml`, `crates/mybox-app/src/main.rs`, `crates/mybox-app/Cargo.toml`, `Cargo.toml`, `Cargo.lock`
- Commits verified: `e096be9`（Task 2，前置 agent）, `e6a7285`（Task 3，本会话）
- `cargo nextest run -p mybox-capture`: 9 passed, 0 failed (exit 0)
- `cargo nextest run -p mybox-core -p mybox-capture`: 81 passed, 4 skipped (exit 0)
- `cargo check --workspace`: exit 0

---
*Phase: 02-screenshot*
*Completed: 2026-08-13*
