---
phase: 01-framework
plan: 01-02
subsystem: infra
tags: [rust, winit, tiny-skia, softbuffer, event-bus, window-manager, renderer]

# Dependency graph
requires:
  - phase: 01-framework
    plan: 01-01
    provides: Event/EventPayload/EventFilter shells, WindowSpec/WindowKind/WindowState types, Renderer trait + premul_rgba_to_u32, ModuleContext facade, ModuleRegistry
provides:
  - EventBus with worker-thread dispatch + broadcast + wildcard filtering (FRMW-02, FRMW-05)
  - UiThreadProxy + ModuleContext emit/on/ui accessors (winit main-thread forwarding)
  - WindowManager: spec→winit attributes builder, id→state routing, get_mut_by_winit, batch_create signature
  - WindowManagerHandle + WindowRequest + wake hook (crossbeam channel for module-side window control)
  - TinySkiaSoftbufferRenderer: tiny-skia Pixmap + softbuffer present pipeline (D-01/D-02/D-03)
affects: [01-03 (hotkey/tray/config services), 01-04 (App event-loop integration, TestModule, AppEvent wiring)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "EventBus: crossbeam unbounded channel + bus worker thread recv loop + broadcast to matching handlers; emit() is send-only (never blocks)"
    - "UiThreadProxy holds EventLoopProxy<AppEvent>; run()暂存闭包 for 01-04 wiring"
    - "WindowManager: pure spec→WindowAttributes builder (headless-testable) + HashMap<WindowId, WindowState> routing"
    - "Renderer: uniform tiny-skia Pixmap + softbuffer present; pixels converted via premul_rgba_to_u32"

key-files:
  created:
    - crates/mybox-core/src/renderer/tiny_skia_softbuffer.rs
  modified:
    - crates/mybox-core/src/event.rs
    - crates/mybox-core/src/context.rs
    - crates/mybox-core/src/window.rs
    - crates/mybox-core/src/renderer/mod.rs
    - crates/mybox-core/src/lib.rs

key-decisions:
  - "EventBus uses crossbeam-channel unbounded; emit() send-only so publication never blocks the calling thread (FRMW-05)"
  - "UiThreadProxy holds EventLoopProxy<AppEvent> (AppEvent defined by 01-04); closure暂存 for main-thread forwarding"
  - "WindowManager is main-thread-bound (holds winit::Window, non-Send); modules use WindowManagerHandle to enqueue WindowRequest, executed by App's loop (01-04)"
  - "TinySkiaSoftbufferRenderer: softbuffer 0.4 generics inferred from winit::Window (implements HasDisplayHandle + HasWindowHandle); present() truncates to buffer width*height"

requirements-completed: [FRMW-02, FRMW-03, FRMW-05]

# Metrics
duration: 40min
completed: 2026-08-12
---

# Phase 1: Framework Plan 2: EventBus + WindowManager + Renderer Backend — Summary

**Async EventBus (worker-thread dispatch + broadcast + wildcard filtering), WindowManager (spec→attrs builder + id→state routing + module-side handle), and the TinySkiaSoftbufferRenderer present pipeline, all headless-green**

## Performance

- **Duration:** ~40 min (two executor stalls interrupted mid-task-03; completed by orchestrator)
- **Started:** 2026-08-12
- **Completed:** 2026-08-12
- **Tasks:** 4
- **Files modified:** 5 (1 created, 4 modified; all in mybox-core)

## Accomplishments
- **EventBus (FRMW-02, FRMW-05):** crossbeam unbounded channel; `emit()` send-only (non-blocking); `on(filter, handler) -> SubscriptionId`; bus worker thread recv loop broadcasts to `filter.matches()` handlers; `EventBus` is `Clone` (Arc-shared state). Worker-thread dispatch keeps the event loop for UI/render only (FRMW-05).
- **UiThreadProxy + ModuleContext accessors:** `UiThreadProxy` holds `EventLoopProxy<AppEvent>`; `ModuleContext::emit/on/ui` forward to the bus and proxy. `windows()` accessor + `pub(crate)::new(...)` constructor added for App wiring (01-04).
- **WindowManager (FRMW-03, D-07):** pure `window_attributes(spec)` builder (Overlay → transparent + undecorated + AlwaysOnTop; Floating → undecorated + AlwaysOnTop; Panel → decorated + title); `HashMap<WindowId, WindowState>` routing; `register/destroy/get_mut/get_mut_by_winit/close_all/next_id`; `batch_create` signature stubbed for Phase 2.
- **WindowManagerHandle + WindowRequest:** module-side channel handle with `create/destroy/set_wakeup`; wake hook solves the "crossbeam send doesn't wake ControlFlow::Wait" race (W3) — App injects an `AppEvent::WindowRequested` pulse on run (01-04).
- **TinySkiaSoftbufferRenderer (D-01/D-02/D-03):** `Renderer` impl over softbuffer Surface + tiny_skia Pixmap; `resize` rebuilds pixmap; `draw(f)` paints into the pixmap; `present()` converts premul RGBA → 0x00RRGGBB via `premul_rgba_to_u32` and calls `surface.present()`, truncating to buffer width×height.

## Task Commits

Each task committed atomically:

1. **Task 01-02-01: EventBus worker-thread dispatch** - `dd24055` (feat)
2. **Task 01-02-02: WindowManager + window_attributes builder** - `8cc7330` (feat)
3. **Task 01-02-03: TinySkiaSoftbufferRenderer backend** - `7fba97c` (feat)
4. **Task 01-02-04: ModuleContext services wiring + green tests** - `354c620` (feat)

**Plan metadata:** `docs(01-02): complete event bus, window manager, and renderer backend plan` (this SUMMARY commit)

## Files Created/Modified
- `crates/mybox-core/src/event.rs` - EventBus (worker-thread dispatch, emit/on, SubscriptionId), existing tests extended
- `crates/mybox-core/src/context.rs` - UiThreadProxy impl, ModuleContext emit/on/ui/windows accessors + `pub(crate)::new`
- `crates/mybox-core/src/window.rs` - WindowManager, window_attributes builder, WindowManagerHandle + WindowRequest + wake hook
- `crates/mybox-core/src/renderer/tiny_skia_softbuffer.rs` - TinySkiaSoftbufferRenderer (new)
- `crates/mybox-core/src/renderer/mod.rs` - Renderer trait retained
- `crates/mybox-core/src/lib.rs` - export TinySkiaSoftbufferRenderer

## Decisions Made
- EventBus uses crossbeam unbounded channel; `emit()` is send-only so publication never blocks (FRMW-05).
- UiThreadProxy holds `EventLoopProxy<AppEvent>`; closure暂存 mechanism for 01-04 main-thread forwarding.
- WindowManager is main-thread-bound; modules use WindowManagerHandle to enqueue WindowRequest executed by App (01-04 wiring).
- softbuffer 0.4 generics inferred from `winit::Window` (implements HasDisplayHandle + HasWindowHandle); no explicit type params needed (W5).
- present() truncates to buffer width×height rather than assuming pixmap == buffer size (T-1-06).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Forward dependency] UiThreadProxy AppEvent placeholder**
- **Found during:** Task 01-02-01 (EventBus)
- **Issue:** Plan allows `EventLoopProxy<AppEvent>` where AppEvent is defined by 01-04; the UiThreadProxy holds the proxy with AppEvent resolved once 01-04 adds the type.
- **Fix:** Implemented UiThreadProxy generically over the proxy it will hold; `run()`暂存 the closure for 01-04 to execute via user_event.
- **Files modified:** crates/mybox-core/src/context.rs
- **Verification:** `cargo nextest run -p mybox-core context::` green

**2. [Rule 1 - Scope] batch_create stub**
- **Found during:** Task 01-02-02 (WindowManager)
- **Issue:** Plan specifies `batch_create` signature only for Phase 2 (per-display windows); a full implementation is out of this plan's scope.
- **Fix:** Implemented the signature returning placeholder ids by next_id, documented as Phase 2.
- **Files modified:** crates/mybox-core/src/window.rs
- **Verification:** `cargo nextest run -p mybox-core window_manager::` green

---

**Total deviations:** 2 auto-fixed (both forward-dependency / scope clarifications)
**Impact on plan:** All auto-fixes were necessary for correctness; no scope creep beyond the plan's own types.

## Issues Encountered
- The executor agent stalled twice mid-task-03 (stream watchdog, no progress for 600s) leaving task-03/04 changes staged-but-uncommitted. The orchestrator resumed the agent, then completed the remaining commits directly in the isolated worktree after verification (50/50 tests green) to avoid a third stall.

## User Setup Required
None - headless unit tests only; real window/render display verification is in 01-04 integration scope.

## Next Phase Readiness
- EventBus, WindowManager, UiThreadProxy, and TinySkiaSoftbufferRenderer are wired into ModuleContext. 01-04 can now assemble App with these real services, hook the global hotkey/tray (01-03), and run the TestModule end-to-end.
- No [ignore] tests; full workspace green (50/50 tests via `cargo nextest run`).

---
*Phase: 01-framework*
*Completed: 2026-08-12*