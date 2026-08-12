---
phase: 01-framework
plan: 01-04
subsystem: infra
tags: [winit, event-loop, macos, accessory, tray, hotkey, walking-skeleton, integration-testing]

# Dependency graph
requires:
  - phase: 01-framework
    provides: EventBus/WindowManager/Renderer (01-02), HotkeyManager/ConfigCenter/TrayManager (01-03)
provides:
  - App/AppBuilder + AppEvent with winit ApplicationHandler event loop
  - macOS Accessory activation policy (no Dock icon, FRMW-06)
  - hotkey/tray/menu event forwarding into event loop via EventLoopProxy (FRMW-04)
  - main-thread window creation via crossbeam WindowRequest channel drained in about_to_wait (W2)
  - TestModule proving module boundary end-to-end (hotkey -> bus -> window, FRMW-01)
  - #[ignore] display integration suite + manual checklist + SKELETON lock-in
affects: Phase 2 (screenshot module uses WindowManager/EventBus/HotkeyManager), Phase 3 (command palette), Phase 4 (Windows port)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "ApplicationHandler<AppEvent> + with_user_event for typed app events"
    - "EventLoopProxy::send_event as wake bridge for crossbeam-queued work (D-08 reconciliation)"
    - "crossbeam WindowRequest channel + about_to_wait drain for main-thread window creation (W2)"
    - "set_wakeup hook emitting AppEvent::WindowRequested pulse to wake ControlFlow::Wait (W3)"
    - "subprocess-per-check #[ignore] integration tests (winit macOS main-thread EventLoop constraint)"

key-files:
  created:
    - crates/mybox-core/src/app.rs
    - crates/mybox-core/src/bin/display_checks.rs
    - crates/mybox-core/tests/integration.rs
    - crates/mybox-app/tests/manual_checklist.md
  modified:
    - crates/mybox-core/src/lib.rs
    - crates/mybox-core/src/window.rs
    - crates/mybox-core/src/hotkey.rs
    - crates/mybox-core/src/context.rs
    - crates/modules/test/src/lib.rs
    - crates/mybox-app/src/main.rs
    - .planning/phases/01-framework/01-SKELETON.md

key-decisions:
  - "D-08 reconciliation: global-hotkey/tray/menu events wake the Wait loop via EventLoopProxy::send_event(AppEvent), not merged into winit native sources; no ControlFlow::Poll"
  - "W1: run() registers [hotkeys] config entries via register_str (&self on shared Arc); invalid entries log::warn and continue, not abort startup"
  - "W2: window creation via crossbeam WindowRequest channel drained in about_to_wait (ActiveEventLoop is non-Send and not storable)"
  - "W3: WindowManagerHandle::set_wakeup injects AppEvent::WindowRequested pulse so queued window requests always wake the Wait loop"
  - "N2: PredefinedMenuItem::quit exits natively on macOS (terminate:); no explicit event_loop.exit wiring in Phase 1 (v2 item if Windows needs it)"
  - "Integration tests spawn a helper bin (subprocess-per-check) because winit macOS EventLoop must be created on the real main thread, one per process"

patterns-established:
  - "Typed AppEvent enum + EventLoopProxy wake-bridge keeps ControlFlow::Wait idle-zero-cost"
  - "Module hotkey -> bus event -> WindowRequest enqueue -> about_to_wait drain -> create_window"

requirements-completed: [FRMW-01, FRMW-04, FRMW-05, FRMW-06, INFRA-02, INFRA-03]

# Metrics
duration: 3h10m
completed: 2026-08-12
---

# Phase 1 Plan 4: 事件循环集成 + macOS 平台适配 + 测试模块（Walking Skeleton）Summary

**winit ApplicationHandler event loop with macOS Accessory mode, hotkey/tray/menu forwarding via EventLoopProxy, main-thread window creation through a crossbeam WindowRequest channel, and a TestModule proving the module boundary end-to-end (hotkey → bus → window)**

## Performance

- **Duration:** 3h 10m (spans two executor sessions: tasks 1-4 by the prior worktree agent, task 5 completion + integration-suite fix by the continuation agent)
- **Started:** 2026-08-12T06:02:16Z
- **Completed:** 2026-08-12T09:12:23Z
- **Tasks:** 5
- **Files modified:** 12

## Accomplishments

- `App`/`AppBuilder` with `ApplicationHandler<AppEvent>`: `EventLoopBuilder::with_user_event::<AppEvent>()`, macOS `ActivationPolicy::Accessory` (no Dock item, FRMW-06), full `run()` lifecycle (builder → Accessory → ConfigCenter load → HotkeyManager/TrayManager main-thread init → `[hotkeys]` config registration (W1) → event-forwarder install → `run_app`)
- Three event forwarders wired: `GlobalHotKeyEvent`/`TrayIconEvent`/`MenuEvent::set_event_handler` each `EventLoopProxy::send_event(AppEvent::…)` — bus dispatch for `hotkey.triggered` (id→action→emit) and `menu.triggered` (D-08 reconciliation, FRMW-04/INFRA-03)
- Main-thread window routing: `about_to_wait` drains the crossbeam `window_rx` (`WindowRequest::Create/Destroy`), `window_event` routes RedrawRequested/Resized/CloseRequested; `create_window(el, spec)` builds attrs → registers state → emits `WindowCreated` (W2); `set_wakeup` injects the `AppEvent::WindowRequested` pulse so queued requests always wake `ControlFlow::Wait` (W3)
- `TestModule` (id `"test"`) subscribes to `core`/`hotkey.triggered`, opens a "mybox test" 400×300 Panel window on `open_test_window` via `ctx.windows().create` — proving the module boundary end-to-end with mybox-core as its only dependency (FRMW-01)
- `#[ignore]` display integration suite (Panel + Overlay windows, hotkey init+register, tray build) green on a real macOS session; manual verification checklist for success criteria 1/2; `01-SKELETON.md` locked to Implemented

## Task Commits

Each task was committed atomically:

1. **Task 1: App 结构 + macOS Accessory + run() 生命周期** - `6560bfe` (feat)
2. **Task 2: hotkey/tray/menu 事件转发到事件循环 + 热键→bus 事件** - `b82493d` (feat)
3. **Task 3: window_event 路由 + WindowRequest 主线程消费（窗口创建）** - `f4b2440` (feat)
4. **Task 4: TestModule 实现 + 托盘/菜单接线（FRMW-01 端到端）** - `4f5d0de` (feat)
5. **Task 5: #[ignore] 集成测试 + 手动验证清单 + SKELETON 固化** - `da3b62b` (test)

## Files Created/Modified

- `crates/mybox-core/src/app.rs` - App/AppBuilder/AppEvent; ApplicationHandler impl; event forwarders; on_hotkey/on_menu dispatch; create_window; run() lifecycle
- `crates/mybox-core/src/window.rs` - WindowManagerHandle wake hook (set_wakeup), window request handling support
- `crates/mybox-core/src/hotkey.rs` - shared-ref register_str / action mapping support for W1
- `crates/mybox-core/src/context.rs` - ModuleContext wiring for hotkey + windows accessors / UiThreadProxy
- `crates/mybox-core/src/lib.rs` - public API re-exports (App/AppBuilder/AppEvent etc.)
- `crates/modules/test/src/lib.rs` - TestModule (id "test", default_config, menu item, hotkey subscription, window request)
- `crates/mybox-app/src/main.rs` - real entry point: `App::builder().module(TestModule)?.build()?.run()`
- `crates/mybox-core/tests/integration.rs` - 4 `#[ignore]` display integration tests spawning the helper bin
- `crates/mybox-core/src/bin/display_checks.rs` - subprocess-per-check harness (panel/overlay/hotkey/tray)
- `crates/mybox-app/tests/manual_checklist.md` - manual verification for success criteria 1/2
- `crates/mybox-core/Cargo.toml` - dropped redundant `[dev-dependencies]` (winit/tiny-skia already regular deps)
- `.planning/phases/01-framework/01-SKELETON.md` - status Implemented, checklists aligned with delivered dependency versions

## Decisions Made

- **D-08 reconciliation (locked in):** hotkey/tray/menu events wake `ControlFlow::Wait` via `EventLoopProxy::send_event(AppEvent)`, not merged into winit native sources; no `Poll` — keeps idle at zero cost (T-1-11)
- **W1:** startup registers `[hotkeys]` config entries one by one via shared-`Arc` `register_str`; a bad entry logs `warn` and startup continues
- **W2:** `ActiveEventLoop` is non-`Send` and only available in callbacks, so window creation goes through a crossbeam `WindowRequest` channel drained in `about_to_wait`
- **W3:** `WindowManagerHandle::create/destroy` trigger an injected `set_wakeup` hook (`AppEvent::WindowRequested` pulse) because crossbeam `send` does not wake a `Wait` loop
- **N2:** quit is handled natively by macOS `PredefinedMenuItem::quit` (`terminate:`); explicit `event_loop.exit()` wiring deferred to v2 (Windows)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Display integration tests never compiled/passed under `cargo test`**
- **Found during:** Task 5 (integration suite verification)
- **Issue:** The initial `tests/integration.rs` created `winit::EventLoop::new()` inline inside `#[test]` fns. winit on macOS panics unless the `EventLoop` is created on the **real main thread** (`MainThreadMarker`), and allows only **one** `EventLoop` per process. Rust's `cargo test` harness runs each `#[test]` on a spawned worker thread, so all 4 tests panicked ("must be created on the main thread" / `EventLoopError::RecreationAttempt`) — the acceptance criterion `cargo test -- --ignored -p mybox-core` exit 0 was unreachable as written.
- **Fix:** Moved the checks into a helper binary `crates/mybox-core/src/bin/display_checks.rs` (one check per arg: panel/overlay/hotkey/tray), each running on its own process's main thread; rewrote `tests/integration.rs` so each `#[ignore]` test spawns the binary via `CARGO_BIN_EXE_display_checks` and asserts exit status 0. Removed the redundant `[dev-dependencies]` (winit/tiny-skia were already regular `[dependencies]`).
- **Files modified:** crates/mybox-core/tests/integration.rs, crates/mybox-core/src/bin/display_checks.rs (new), crates/mybox-core/Cargo.toml
- **Verification:** `cargo test -p mybox-core --test integration -- --ignored` → 4 passed on the macOS session; `cargo nextest run` → 77 passed, 4 skipped
- **Committed in:** `da3b62b` (part of task 5 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 bug)
**Impact on plan:** Fix required for the task-5 success criterion to be satisfiable. No scope creep.

## Issues Encountered

- **Continuation state:** Tasks 1-4 were committed by the prior worktree agent (`6560bfe`..`4f5d0de`); task 5 was staged but uncommitted when this continuation agent took over, and its integration tests were not compiling. Completed task 5, fixed the display-test design, verified the full suite, and committed (`da3b62b`).
- **cwd drift (#3097):** an early `gsd-sdk query requirements.mark-complete` ran from the main-checkout cwd and wrote to the main repo's `REQUIREMENTS.md`; reverted that specific file and re-ran the update from the worktree so it lands in the worktree's `REQUIREMENTS.md`.

## User Setup Required

None - no external service configuration required. The manual checklist (`crates/mybox-app/tests/manual_checklist.md`) covers the display-verification steps (tray/Dock, Cmd+Shift+T, config generation, quit) that need a human on the macOS desktop.

## Next Phase Readiness

- Walking Skeleton end-to-end capability proven: launch → tray (no Dock) → Cmd+Shift+T → "mybox test" window (automated suite + manual checklist in place)
- Phase 2 (screenshot module) can build on `WindowManager` (Overlay + batch_create), `EventBus`, `HotkeyManager`, and the `Renderer` draw-closure extension point
- `batch_create` (D-09) is a documented Phase-2 placeholder; transparent-overlay real alpha and screenSaver level remain deferred to Phase 2 (SKELETON Out of Scope)

## Self-Check: PASSED

- Files verified: app.rs, display_checks.rs, integration.rs, manual_checklist.md, modules/test/src/lib.rs, mybox-app/src/main.rs, 01-SKELETON.md, 01-04-SUMMARY.md
- Commits verified: `6560bfe`, `b82493d`, `f4b2440`, `4f5d0de`, `da3b62b`
- `cargo nextest run`: 77 passed, 4 skipped (exit 0)
- `cargo test -p mybox-core --test integration -- --ignored`: 4 passed (exit 0)
- `cargo check --workspace`: exit 0; `cargo build -p mybox-app`: exit 0

---
*Phase: 01-framework*
*Completed: 2026-08-12*
