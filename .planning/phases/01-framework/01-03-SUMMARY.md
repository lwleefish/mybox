---
phase: 01-framework
plan: 01-03
subsystem: infra
tags: [rust, global-hotkey, tray-icon, tiny-skia, toml, hotkey, config, tray]

# Dependency graph
requires:
  - phase: 01-framework
    plan: 01-01
    provides: workspace + Module trait + unified MyboxError + INFRA-04 config path contract
  - phase: 01-framework
    plan: 01-02
    provides: ModuleContext facade with emit/on/ui/windows accessors, EventBus, WindowManagerHandle
provides:
  - HotkeyManager: global-hotkey registration from config strings + id→action map (FRMW-04, D-11)
  - ConfigCenter: first-run generation, per-module namespace isolation, in-memory cache + full write-back, hotkey() parsing (INFRA-01/INFRA-04, D-10/D-12/D-13)
  - TrayManager: menu assembly from module menu_items + runtime-generated monochrome icon (INFRA-02)
affects: [01-04 (App event-loop integration, TestModule, AppEvent wiring, real hotkey register/tray build)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Interior-mutability service singletons: HotkeyManager exposes &self register_str via parking_lot::Mutex<Option<GlobalHotKeyManager>> (Arc refcount >= 2 at runtime rules out Arc::get_mut)"
    - "First-run config generation (D-12/D-13): merge each Module::default_config() into [module_id] sections + a framework [hotkeys] section into a commented config.toml"
    - "Runtime tray icon generation (RESEARCH §11 #4): tiny-skia renders monochrome RGBA, no bundled PNG asset"
    - "Headless-testable pure cores split from main-thread-bound native wrappers (assemble_menu_items / generate_icon_rgba)"

key-files:
  created: []
  modified:
    - crates/mybox-core/src/hotkey.rs
    - crates/mybox-core/src/config.rs
    - crates/mybox-core/src/tray.rs
    - crates/mybox-core/src/context.rs
    - crates/mybox-core/src/lib.rs

key-decisions:
  - "HotkeyManager wraps GlobalHotKeyManager in parking_lot::Mutex<Option<..>> so every method takes &self (shared Arc rules out &mut/Arc::get_mut)"
  - "register_str maps parse/init/register failures to MyboxError::HotkeyParse with descriptive messages (plan left the error variant open)"
  - "ConfigCenter load_or_create delegates to an internal load_or_create_at(path, modules) so tests run against temp dirs, never the real user config dir"
  - "Tray menu assembly extracted to a pure assemble_menu_items because muda's Menu is main-thread-only on macOS (not unit-testable off the main thread); build_menu appends that exact content"

patterns-established:
  - "Conventional commits scoped {type}(01-03): per plan"
  - "Headless-safe unit tests: 16 new tests, all CI-runnable, no #[ignore] introduced; real hotkey registration / tray build deferred to 01-04 integration"

requirements-completed: [FRMW-04, INFRA-01, INFRA-02, INFRA-04]

# Metrics
duration: 20min
completed: 2026-08-12
---

# Phase 1 Plan 3: 热键管理器 + 系统托盘 + 配置中心 — Summary

**HotkeyManager (global-hotkey FromStr registration + id→action map), ConfigCenter (first-run TOML generation, per-module namespace isolation, full write-back), and TrayManager (module menu assembly + runtime tiny-skia icon), all headless-green and wired into ModuleContext**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-08-12T04:46:49Z
- **Completed:** 2026-08-12T05:06:58Z
- **Tasks:** 4
- **Files modified:** 5 (all in mybox-core)

## Accomplishments

- **HotkeyManager (FRMW-04, D-11):** `register_str` parses config strings via global-hotkey's built-in `FromStr` (no hand-written parser), registers through `GlobalHotKeyManager` behind interior mutability (`&self` methods), and records `hotkey id → action`; `action_for_id` translates trigger ids; `init()` is main-thread-only (macOS) for the 01-04 App.
- **ConfigCenter (INFRA-01/INFRA-04, D-10/D-12/D-13):** first-run generation merges `Module::default_config()` into `[module_id]` sections plus a framework `[hotkeys]` section (`open_test_window`/`exit`) with a comment header; namespace isolation keeps module A's keys out of module B; `set`/`get` hit the in-memory cache and `save()` writes the full table back; `hotkey()` parses `[hotkeys]` strings via `FromStr`.
- **TrayManager (INFRA-02):** `build_menu` assembles module `menu_items` → separator → quit (`退出`) preserving item ids; `generate_icon` renders a monochrome circle at runtime with tiny-skia (no asset); `TrayManager::build` wires `TrayIconBuilder` with the icon as a macOS template.
- **ModuleContext `config()`/`hotkeys()` accessors** expose the shared services to modules (with an Arc-pointer-equality test); `lib.rs` re-exports `build_menu`/`generate_icon`.

## Task Commits

Each task was committed atomically:

1. **Task 01-03-01: HotkeyManager implementation** - `64c787a` (feat)
2. **Task 01-03-02: ConfigCenter implementation** - `eb3abce` (feat)
3. **Task 01-03-03: TrayManager implementation** - `2dd5e2d` (feat)
4. **Task 01-03-04: ModuleContext config/hotkeys accessors + green workspace** - `bdbe5dd` (feat)

**Plan metadata:** `docs(01-03): complete hotkey, tray, and config center plan` (this SUMMARY commit)

## Files Created/Modified

- `crates/mybox-core/src/hotkey.rs` - `HotkeyManager` (`init`/`register_str`/`action_for_id`) + FromStr/map/not-initialized tests
- `crates/mybox-core/src/config.rs` - `ConfigCenter` (`load_or_create`/`get`/`get_section`/`set`/`save`/`hotkey`) + first-run generation + namespace/round-trip/robustness/hotkey tests; existing INFRA-04 path contract tests retained
- `crates/mybox-core/src/tray.rs` - `build_menu`/`assemble_menu_items`/`generate_icon`/`generate_icon_rgba`/`TrayManager::build` + assembly/icon tests
- `crates/mybox-core/src/context.rs` - `ModuleContext::config()`/`hotkeys()` accessors + shared-instance test
- `crates/mybox-core/src/lib.rs` - re-export `build_menu`/`generate_icon` alongside `TrayManager`

## Decisions Made

- HotkeyManager uses interior mutability (`parking_lot::Mutex<Option<GlobalHotKeyManager>>`) so all methods take `&self`: the shared `Arc<HotkeyManager>` has refcount >= 2 by run time, making `Arc::get_mut` unusable.
- `register_str` reports parse/init/register failures as `MyboxError::HotkeyParse` with descriptive messages (plan left the init-error variant open; no new error variant added so `error.rs` stays untouched).
- ConfigCenter keeps the public `load_or_create(modules)` signature while delegating to an internal `load_or_create_at(path, modules)` so tests use `temp_dir()` + unique subdirs (never the real user config dir).
- Tray menu assembly is expressed as a pure `Vec<MenuItemKind>` (`assemble_menu_items`) because muda's `Menu` is main-thread-only on macOS; `build_menu` appends exactly that content, and unit tests verify it headlessly.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Platform constraint] Tray menu test extracted to a pure `assemble_menu_items`**
- **Found during:** Task 01-03-03 (TrayManager)
- **Issue:** The plan's `build_menu` unit test calls `Menu::new()`, which panics on macOS with "muda::Menu can only be created on the main thread"; nextest runs tests on worker threads, so that headless test cannot pass. Separately, `tray_icon::Icon` hides its raw RGBA (platform-specific `pub(crate)` inner), so asserting `generate_icon(32)` data length is not directly possible.
- **Fix:** Extracted a pure `assemble_menu_items(module_items) -> Vec<MenuItemKind>` that `build_menu` appends (single source of truth) and a pure `generate_icon_rgba(size) -> Vec<u8>` that `generate_icon` wraps. Unit tests verify the exact ordered content `build_menu` appends (module items + separator + quit, ids preserved) and the RGBA bytes (`len == size²·4`, at least one opaque pixel).
- **Files modified:** crates/mybox-core/src/tray.rs
- **Verification:** `cargo nextest run -p mybox-core tray::` green (4 tests)
- **Committed in:** 2dd5e2d

**2. [Rule 1 - Test isolation] `load_or_create_at` internal path override**
- **Found during:** Task 01-03-02 (ConfigCenter)
- **Issue:** `load_or_create` must target the platform user dir (INFRA-04), but tests must never write there; the plan required temp-dir tests without changing the public signature.
- **Fix:** Public `load_or_create(modules)` delegates to an internal `load_or_create_at(path, modules)`; tests use `temp_dir()` + a per-test unique subdir.
- **Files modified:** crates/mybox-core/src/config.rs
- **Verification:** `cargo nextest run -p mybox-core config::` green (7 tests, incl. `config::tests::namespace`)
- **Committed in:** eb3abce

**3. [Rule 1 - Error mapping latitude] `register_str` error variant choice**
- **Found during:** Task 01-03-01 (HotkeyManager)
- **Issue:** The plan left the "manager not initialized" error open ("MyboxError 变体") and global-hotkey's `register()` returns its own `Error` type that needs mapping.
- **Fix:** Mapped parse/init/register failures to `MyboxError::HotkeyParse` with descriptive messages; no new error variant added (keeps `error.rs` untouched, per the plan's `files_modified`).
- **Files modified:** crates/mybox-core/src/hotkey.rs
- **Verification:** `cargo nextest run -p mybox-core hotkey::` green (4 tests)
- **Committed in:** 64c787a

**4. [Rule 1 - Commit convention] `{phase}-{plan}` commit scope**
- **Found during:** all tasks
- **Issue:** Plan `commit_message` fields use a `(core)` scope; the orchestrator convention requires `{type}({phase}-{plan}):` prefixes (same finding as plan 01-01).
- **Fix:** Applied the plan's descriptive text under a `feat(01-03):` scope.
- **Files modified:** none (commit metadata only)
- **Verification:** `git log --oneline fcb1381..HEAD` shows 4 conventional commits
- **Committed in:** all 4 task commits

---

**Total deviations:** 4 auto-fixed (1 platform constraint, 1 test isolation, 1 error-mapping latitude, 1 commit convention)
**Impact on plan:** All auto-fixes were necessary for compilation, correctness, or headless testability. No scope creep beyond the plan's own types.

## Issues Encountered

- muda's `Menu` is main-thread-only on macOS — discovered when the tray `build_menu` test panicked under nextest; resolved by extracting pure `assemble_menu_items` (deviation #1).
- `toml::Table` FromStr inference needed the parse target annotated (E0282) — resolved by relying on the existing `From<toml::de::Error>` bridge (`content.parse()?`).
- Temporary lifetime (E0716) when borrowing `&FakeModule::new(...)` in config tests — resolved by binding fakes to locals.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- HotkeyManager, ConfigCenter, and TrayManager are implemented and wired into ModuleContext via `config()`/`hotkeys()`. Plan 01-04 can now: call `hotkeys.init()` + `register_str()` from the `[hotkeys]` config on the main thread, build the tray from module `menu_items` with `generate_icon`, and run the TestModule end-to-end.
- No `#[ignore]` tests; full workspace green (66/66 via `cargo nextest run`).
- Real OS registration (`GlobalHotKeyManager::register`), tray build (`TrayIconBuilder::build`), and real user-dir first-run generation remain 01-04 integration/manual verification.

---
*Phase: 01-framework*
*Completed: 2026-08-12*
