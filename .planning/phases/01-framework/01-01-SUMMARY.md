---
phase: 01-framework
plan: 01-01
subsystem: infra
tags: [rust, winit, tiny-skia, softbuffer, workspace, module, event-bus]

# Dependency graph
requires:
  - phase: 00-init
    provides: PROJECT.md core value + stack constraints, roadmap, RESEARCH version matrix
provides:
  - Cargo workspace (mybox-core / mybox-app / modules/test) with versions pinned to RESEARCH §1
  - Module trait + ModuleContext + ModuleRegistry with duplicate-id rejection
  - Event/EventPayload/FrameworkEvent/EventFilter with wildcard matching
  - WindowKind/WindowSpec/WindowId/WindowState + config path contract (INFRA-04)
  - Renderer trait + premul-RGBA→0x00RRGGBB pixel conversion
  - Unified MyboxError (thiserror) bridging toml/softbuffer/io errors
affects: [01-02 (EventBus impl, WindowManager, Renderer backend), 01-03 (hotkey/tray/config services), 01-04 (App event-loop integration, TestModule)]

# Tech tracking
tech-stack:
  added: [winit 0.30.13, tiny-skia 0.12.0, softbuffer 0.4.8, global-hotkey 0.8.0, tray-icon 0.24.2, crossbeam-channel 0.5.16, toml 1.1.4, directories 6.0.0, parking_lot 0.12.5, anyhow 1, thiserror 2, env_logger 0.11]
  patterns:
    - "Cargo workspace with [workspace.dependencies] pinning (resolver = 2, three member crates)"
    - "Module trait + compile-time ModuleRegistry (builder registration, FRMW-01)"
    - "ModuleContext facade: the only core surface modules see (FRMW-02)"
    - "Event model: hybrid payload (typed FrameworkEvent + JSON module payload), wildcard EventFilter"
    - "Renderer trait with draw() closure isolating content from compositing (egui slot for Phase 3)"

key-files:
  created:
    - Cargo.toml
    - .gitignore
    - crates/mybox-core/src/{lib.rs,error.rs,event.rs,module.rs,context.rs,window.rs,hotkey.rs,config.rs,tray.rs}
    - crates/mybox-core/src/renderer/mod.rs
    - crates/mybox-app/{Cargo.toml,src/main.rs}
    - crates/modules/test/{Cargo.toml,src/lib.rs}
  modified: []

key-decisions:
  - "softbuffer 0.4.8 exposes a single SoftBufferError enum (not ContextError/SurfaceError as the plan assumed); mapped it to MyboxError::Softbuffer"
  - "Added MyboxError::Module(String) variant so ModuleRegistry duplicate-id rejection returns a typed, message-carrying error"
  - "Renderer trait pulled forward into plan 01-01-05 because WindowState::renderer: Box<dyn Renderer> requires it at compile time; the pixel function stayed in 01-01-06"
  - "Committed Cargo.lock in the bootstrap commit for reproducible builds of the app workspace"

patterns-established:
  - "Conventional commit messages scoped by plan id: {type}({phase}-{plan}): ..."
  - "Shell-first service types (WindowManagerHandle/ConfigCenter/HotkeyManager/TrayManager/UiThreadProxy) declared as #[derive(Default)] unit structs, filled by later plans"
  - "Headless-safe unit tests: all core logic (filter matching, pixel math, config path, registry) is pure and CI-runnable; no #[ignore] tests introduced"

requirements-completed: [FRMW-01, FRMW-02, FRMW-03, INFRA-03, INFRA-04]

# Metrics
duration: 16min
completed: 2026-08-12
---

# Phase 1: Framework Plan 1: Workspace Skeleton + Core Types — Summary

**Cargo workspace with pinned deps; Module/Event/Window/Config/Renderer core types and a unified MyboxError, all compiling headless-green across mybox-core, mybox-app, and modules/test**

## Performance

- **Duration:** ~16 min
- **Started:** 2026-08-12T09:04:06+08:00
- **Completed:** 2026-08-12T01:20:33Z
- **Tasks:** 7
- **Files modified:** 14 (8 created in mybox-core, 2 mybox-app, 2 test module, workspace root Cargo.toml/.gitignore)

## Accomplishments
- Root workspace with three crates and `[workspace.dependencies]` pinned exactly to RESEARCH §1 (winit 0.30.13, tiny-skia 0.12.0, softbuffer 0.4.8, global-hotkey 0.8.0, tray-icon 0.24.2, etc.); `cargo check --workspace` green
- `Module` trait (`init`/`default_config`/`menu_items`/`shutdown`), `ModuleContext` facade, and `ModuleRegistry` that rejects duplicate module ids (FRMW-01)
- Event data model: `Event`/`EventPayload` (typed Framework + JSON module hybrid) /`FrameworkEvent`/`EventFilter` with `*` wildcard matching verified by unit tests (FRMW-02 types)
- Window abstraction types `WindowKind::{Overlay,Floating,Panel}`, all-pub `WindowSpec` (Default: Panel/decorated/visible), `WindowId = u64`, `WindowState` shell (FRMW-03 types)
- `config_dir()`/`config_file_path()` returning `<platform user config>/mybox/config.toml` via `directories::ProjectDirs` (INFRA-04, macOS assertion passes)
- `Renderer` trait + `premul_rgba_to_u32` pixel conversion (opaque direct-pack, alpha un-premultiply with 255-clamp) (D-01/D-02/D-03)
- Unified `MyboxError` (thiserror) bridging toml de/ser, io, and softbuffer errors; `Result<T>` alias (INFRA-03)

## Task Commits

Each task was committed atomically:

1. **Task 01-01-01: Cargo workspace skeleton + version locking** - `e18eaea` (chore)
2. **Task 01-01-02: MyboxError unified error type** - `e6f87c3` (feat)
3. **Task 01-01-03: Event/EventPayload/FrameworkEvent/EventFilter** - `854f5ea` (feat)
4. **Task 01-01-04: Module trait + ModuleContext + ModuleRegistry** - `05aedb7` (feat)
5. **Task 01-01-05: Window types + config path contract** - `de70688` (feat)
6. **Task 01-01-06: Renderer trait + pixel conversion** - `d446036` (feat)
7. **Task 01-01-07: Public API export + green workspace** - `a1a0467` (feat)

**Plan metadata:** `docs(01-01): complete workspace skeleton and core types plan` (this SUMMARY commit)

## Files Created/Modified
- `Cargo.toml` - workspace members + `[workspace.dependencies]` version-locked per RESEARCH §1
- `.gitignore` - `/target`, `*.log`
- `Cargo.lock` - committed for reproducible app builds
- `crates/mybox-core/src/lib.rs` - module declarations + `pub use` public API surface
- `crates/mybox-core/src/error.rs` - `MyboxError` enum (9 variants), `Result<T>` alias, toml/io/softbuffer `From` bridges
- `crates/mybox-core/src/event.rs` - `Event`/`EventPayload`/`FrameworkEvent`/`EventFilter` (+`matches`), `EventBus` shell, `SubscriptionId`
- `crates/mybox-core/src/module.rs` - `Module` trait, `ModuleRegistry` (register/iter/len/get_by_id)
- `crates/mybox-core/src/context.rs` - `ModuleContext` (+`pub(crate)::new`), `UiThreadProxy` shell
- `crates/mybox-core/src/window.rs` - `WindowKind`, `WindowSpec` (Default), `WindowId`, `WindowState` shell, `WindowManagerHandle` shell
- `crates/mybox-core/src/hotkey.rs` - `HotkeyManager` shell
- `crates/mybox-core/src/config.rs` - `ConfigCenter` shell + `config_dir`/`config_file_path`
- `crates/mybox-core/src/tray.rs` - `TrayManager` shell
- `crates/mybox-core/src/renderer/mod.rs` - `Renderer` trait + `premul_rgba_to_u32`
- `crates/mybox-app/{Cargo.toml,src/main.rs}` - bin entry (env_logger init placeholder)
- `crates/modules/test/{Cargo.toml,src/lib.rs}` - module-boundary placeholder crate

## Decisions Made
- softbuffer error handling maps the crate's single `SoftBufferError` (0.4.8 has no `ContextError`/`SurfaceError`) to `MyboxError::Softbuffer`, preserving the W6 "no unmapped error" invariant
- ModuleRegistry duplicate-id rejection uses a new `MyboxError::Module(String)` variant (message carries the offending id)
- `WindowSpec` keeps all fields `pub` so cross-crate modules (e.g. mybox-test) can build it with struct-literal syntax, per the plan
- `Cargo.lock` committed for a reproducible app workspace

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - API assumption] softbuffer error type is a single enum**
- **Found during:** Task 01-01-02 (MyboxError)
- **Issue:** Plan specified `From<softbuffer::ContextError>` and `From<softbuffer::SurfaceError>`; neither type exists in softbuffer 0.4.8 (verified in crate source — it exposes one `SoftBufferError` enum).
- **Fix:** Replaced both impls with a single `From<softbuffer::SoftBufferError>` mapping to `MyboxError::Softbuffer`. Test constructs `SoftBufferError::Unimplemented` to exercise the bridge.
- **Files modified:** crates/mybox-core/src/error.rs
- **Verification:** `cargo nextest run -p mybox-core error::` green (6 tests)
- **Committed in:** e6f87c3

**2. [Rule 1 - Missing variant] Added MyboxError::Module**
- **Found during:** Task 01-01-04 (ModuleRegistry)
- **Issue:** Plan required duplicate-id registration to return "Err(MyboxError 变体)" but no existing variant semantically fits module registration.
- **Fix:** Added `MyboxError::Module(String)` with `#[error("module error: {0}")]`; duplicate-id test asserts the message contains the id.
- **Files modified:** crates/mybox-core/src/error.rs
- **Verification:** `cargo nextest run -p mybox-core module::` green
- **Committed in:** 05aedb7

**3. [Rule 1 - Forward dependency] Renderer trait pulled forward**
- **Found during:** Task 01-01-05 (window types)
- **Issue:** Plan's `WindowState` includes `renderer: Box<dyn Renderer>` but the `Renderer` trait was scheduled for 01-01-06; without it window.rs cannot compile.
- **Fix:** Defined the `Renderer` trait in `renderer/mod.rs` during 01-01-05; `premul_rgba_to_u32` + its tests remained in 01-01-06.
- **Files modified:** crates/mybox-core/src/renderer/mod.rs
- **Verification:** `cargo nextest run -p mybox-core renderer::` green
- **Committed in:** de70688, d446036

**4. [Rule 1 - Missing dep] anyhow added to mybox-app**
- **Found during:** Task 01-01-01 (workspace bootstrap)
- **Issue:** Plan's main.rs is `fn main() -> anyhow::Result<()>` but mybox-app's dependency list omitted anyhow.
- **Fix:** Added `anyhow = { workspace = true }` to crates/mybox-app/Cargo.toml.
- **Files modified:** crates/mybox-app/Cargo.toml
- **Verification:** `cargo check --workspace` green
- **Committed in:** e18eaea

**5. [Rule 1 - API path] winit 0.30 window type path**
- **Found during:** Task 01-01-05 (WindowState)
- **Issue:** `winit::Window` does not exist in winit 0.30.13 (types live under `winit::window`).
- **Fix:** `WindowState.window` uses `winit::window::Window`.
- **Files modified:** crates/mybox-core/src/window.rs
- **Verification:** `cargo nextest run -p mybox-core window::` green
- **Committed in:** de70688

**6. [Rule 1 - Commit convention] Plan-id scoped commit messages**
- **Found during:** All tasks
- **Issue:** Plan `commit_message` fields use a `(core)` scope (e.g. `feat(core): ...`); the executor tooling and orchestrator convention require `{type}({phase}-{plan}):` prefixes (execute-plan.md update_codebase_map greps `feat(01-01):`).
- **Fix:** Applied the plan's descriptive text under a `{phase}-{plan}` scope (e.g. `feat(01-01): add MyboxError with thiserror derive`).
- **Files modified:** none (commit metadata only)
- **Verification:** `git log --oneline f84534b..HEAD` shows 7 conventional commits
- **Committed in:** all 7 task commits

---

**Total deviations:** 6 auto-fixed (5 plan/API corrections + 1 commit-convention alignment)
**Impact on plan:** All auto-fixes were necessary for correctness/compilation; no scope creep beyond the plan's own types and one error variant.

## Issues Encountered
- softbuffer 0.4.8's unified error enum was discovered by compiler feedback (E0425), then confirmed by reading the vendored crate source — resolved by mapping `SoftBufferError`.
- `cargo metadata --no-deps` initially failed because mybox-core had no `src/lib.rs`; the plan's bootstrap task expects an (empty) lib stub, which was created in task 01-01-01.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All core type contracts for plan 01-02 are in place: `EventBus` shell (channel + handler lock fields declared), `WindowSpec`/`WindowKind`/`WindowId`/`WindowState`, `Renderer` trait, `ModuleContext` with service handles.
- Plan 01-02 can now implement the EventBus worker-thread dispatch, WindowManager (spec→winit attributes builder, id→state routing), and the `TinySkiaSoftbufferRenderer` backend without touching public type shapes.
- No blockers. Full workspace is headless-green (24/24 tests, `cargo check --workspace` exit 0); no `#[ignore]` tests introduced.

---
*Phase: 01-framework*
*Completed: 2026-08-12*
