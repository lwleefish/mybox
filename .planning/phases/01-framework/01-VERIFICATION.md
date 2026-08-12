---
phase: 01-framework
verified: 2026-08-12T00:00:00Z
status: human_needed
score: 6/6 must-haves verified
overrides_applied: 0
overrides: []
gaps: []
human_verification:
  - test: "Launch `cargo run -p mybox-app` on a macOS desktop session; confirm a mybox tray icon appears in the menu bar and the Dock shows no mybox entry (ROADMAP success criterion 1, FRMW-06/INFRA-02)."
    expected: "Tray icon visible (monochrome circle template), no Dock item; app runs in Accessory mode."
    why_human: "Actual tray/Dock display requires a live GUI session; headless code checks cannot observe the macOS menu bar or Dock."
  - test: "Press Cmd+Shift+T after launch; confirm a window titled 'mybox test' (~400x300) appears (ROADMAP success criterion 2, FRMW-04+FRMW-03)."
    expected: "A 'mybox test' Panel window appears with opaque content; pressing again opens another."
    why_human: "Global hotkey firing and real window display require a live desktop session and a free Cmd+Shift+T."
  - test: "After first launch, verify `~/Library/Application Support/mybox/config.toml` exists with a [test] section (message = 'hello from test') and a [hotkeys] section (open_test_window = 'Cmd+Shift+T') (INFRA-04)."
    expected: "config.toml generated in the real user config dir on first run; sections present."
    why_human: "Unit tests use temp dirs by design (never the real user dir); real-dir generation needs one actual launch."
  - test: "Right-click the tray icon: confirm the menu contains '打开测试窗口' and '退出' (INFRA-02); choosing 退出 terminates the app (N2)."
    expected: "Menu shows module item + separator + quit; quit exits the process natively."
    why_human: "Tray menu and native quit behavior need a live menu-bar session."
  - test: "Resolve MVP goal-format discrepancy: ROADMAP Phase 1 mode is `mvp` but the goal is not in User Story format (gsd-sdk query user-story.validate returned valid=false). Run /gsd mvp-phase 1 to set a proper User Story goal, or explicitly accept the deviation."
    expected: "Phase 1 goal either rewritten as a User Story or the deviation accepted by the developer."
    why_human: "Format enforcement is a process decision, not a code check; the underlying capability is implemented."
  - test: "Resolve REQUIREMENTS.md staleness: FRMW-02, FRMW-03, INFRA-01, INFRA-04 are implemented in code but still marked `[ ]`/Pending in REQUIREMENTS.md."
    expected: "Requirement status updated to Complete, or a deliberate decision to defer the bookkeeping."
    why_human: "Status bookkeeping in REQUIREMENTS.md is a process decision; code evidence of implementation is present."
---

# Phase 1: 框架核心 Verification Report

**Phase Goal:** 搭建可运行的模块化框架：Module trait、事件总线、窗口管理、热键、托盘、配置系统。应用能以托盘驻留，通过热键触发一个测试窗口，验证框架可用。
**Verified:** 2026-08-12
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

The phase goal is achieved at the code level. All six must-have truths are verified against the actual codebase (not SUMMARY claims). The walking-skeleton chain (launch → tray/Accessory → Cmd+Shift+T → bus hotkey.triggered → TestModule → WindowRequest → main-thread window creation → renderer present) is fully wired, compiles, and is covered by passing unit tests. Items requiring a live macOS desktop session (actual tray display, actual hotkey firing, real user-dir config generation) are routed to human verification.

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | 应用启动后显示系统托盘图标，不显示在 Dock 中 (SC1) | ✓ VERIFIED | `app.rs run()` uses `winit::platform::macos::ActivationPolicy::Accessory` under `#[cfg(target_os="macos")]` (FRMW-06); tray built from module `menu_items()` via `TrayManager::build` with `TrayIconBuilder` + runtime tiny-skia icon (INFRA-02). `cargo build -p mybox-app` exit 0. Actual display = human item #1. |
| 2 | 按注册的全局热键能触发回调（弹出测试窗口）(SC2) | ✓ VERIFIED | Full chain: `register_config_hotkeys` parses `[hotkeys]` config → `HotkeyManager::register_str` (W1); `GlobalHotKeyEvent::set_event_handler` → `AppEvent::Hotkey` → `on_hotkey` → bus `hotkey.triggered` → TestModule handler (action `open_test_window`) → `ctx.windows().create` → `AppEvent::WindowRequested` wake (W3) → `about_to_wait` drains `window_rx` → `create_window` (W2). Unit-tested in `mybox-test` (hotkey_trigger_enqueues_test_window_and_wakes_once) and `app::tests`. Actual firing = human item #2. |
| 3 | Module trait 可注册多个模块，模块通过事件总线收发消息 (SC3) | ✓ VERIFIED | `Module` trait + `ModuleRegistry` (duplicate-id rejection, tests) in module.rs; `AppBuilder::module` + `build` calls each `init` once (tested). `EventBus` pub/sub broadcast + wildcard filter (event.rs tests). TestModule registers via `AppBuilder` in main.rs and receives events through `ctx.on` — only dependency is mybox-core (module boundary). |
| 4 | WindowManager 能创建 Overlay 与 Panel 两种窗口 (SC4) | ✓ VERIFIED | `window_attributes` maps Overlay → transparent+undecorated+AlwaysOnTop, Panel → decorated (unit tests); `WindowManager` register/destroy/get_mut_by_winit routing; `create_window` in app.rs; `display_checks.rs` panel/overlay integration harness + 2 `#[ignore]` integration tests. Real creation = human item via ignored suite. |
| 5 | ConfigCenter 能在用户目录创建并读写 config.toml (SC5) | ✓ VERIFIED | `ConfigCenter::load_or_create` → `config_file_path()` via `directories::ProjectDirs` (macOS `~/Library/Application Support/mybox/config.toml`, asserted in test); first-run `generate_file` writes `[test]`+`[hotkeys]`; `get/set/save` full write-back round-trip tested; malformed file → `MyboxError::ConfigParse` (no panic). Real-dir generation = human item #3. |
| 6 | 统一错误类型 + 日志贯穿 (SC6 / INFRA-03) | ✓ VERIFIED | `MyboxError` (thiserror, 9 variants) + `Result<T>` alias + toml/io/softbuffer `From` bridges in error.rs; `log::info/warn/debug` throughout app.rs/event.rs/tray.rs; `env_logger::init()` in main.rs. Tests green. |

**Score:** 6/6 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | --------- | ------ | ------- |
| `crates/mybox-core/src/module.rs` | Module trait + ModuleRegistry | ✓ VERIFIED | `Module: Send+Sync+'static` with init/default_config/menu_items/shutdown; registry rejects dup ids; 3 tests |
| `crates/mybox-core/src/event.rs` | Event data model + EventBus | ✓ VERIFIED | Hybrid payload, wildcard `matches`, worker-thread broadcast dispatch, non-blocking emit; 12+ tests |
| `crates/mybox-core/src/window.rs` | WindowKind/Spec/Manager/Handle | ✓ VERIFIED | 3 kinds, all-pub WindowSpec, attributes builder, id→state routing, wake hook; 15 tests |
| `crates/mybox-core/src/config.rs` | ConfigCenter | ✓ VERIFIED | load_or_create, namespace isolation, full write-back, hotkey() parse; 8 tests |
| `crates/mybox-core/src/hotkey.rs` | HotkeyManager | ✓ VERIFIED | register_str (FromStr), id→action map, interior mutability; 4 tests |
| `crates/mybox-core/src/tray.rs` | TrayManager | ✓ VERIFIED | menu assembly (module items→sep→quit), runtime tiny-skia icon; 4 tests |
| `crates/mybox-core/src/context.rs` | ModuleContext facade | ✓ VERIFIED | bus/windows/config/hotkeys/ui accessors + UiThreadProxy; 4 tests |
| `crates/mybox-core/src/app.rs` | App/AppBuilder/AppEvent | ✓ VERIFIED | Accessory, W1 hotkey reg, 3 forwarders, W2/W3 window path; 7 tests |
| `crates/mybox-core/src/error.rs` | MyboxError | ✓ VERIFIED | 9 variants, Result alias, toml/io/softbuffer bridges; 6 tests |
| `crates/mybox-core/src/renderer/*` | Renderer trait + backend | ✓ VERIFIED | premul_rgba_to_u32 + TinySkiaSoftbufferRenderer present pipeline; 9 tests |
| `crates/modules/test/src/lib.rs` | TestModule | ✓ VERIFIED | id "test", default_config, menu item, hotkey sub, window request; 4 tests |
| `crates/mybox-app/src/main.rs` | Entry point | ✓ VERIFIED | `App::builder().module(TestModule)?.build()?.run()`; env_logger init |
| `crates/mybox-core/tests/integration.rs` + `src/bin/display_checks.rs` | Display integration suite | ✓ VERIFIED | 4 `#[ignore]` tests spawning per-process binary (panel/overlay/hotkey/tray) |
| `crates/mybox-app/tests/manual_checklist.md` | Manual checklist | ✓ VERIFIED | Covers SC1/SC2 + config + menu + quit |
| Root `Cargo.toml` + 3 crate manifests | Workspace pinned deps | ✓ VERIFIED | winit 0.30.13 / tiny-skia 0.12.0 / softbuffer 0.4.8 / global-hotkey 0.8.0 / tray-icon 0.24.2 pinned; mybox-test depends ONLY on mybox-core (boundary) |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| App run() | HotkeyManager | `register_config_hotkeys` → `register_str` on shared Arc (W1) | WIRED | Source: app.rs `register_config_hotkeys`, `self.hotkeys.register_str(...)`, no `Arc::get_mut` |
| GlobalHotKeyEvent | App | `set_event_handler` → `EventLoopProxy::send_event(AppEvent::Hotkey)` | WIRED | app.rs `install_event_forwarders` (3 forwarders) |
| App user_event | EventBus | `on_hotkey` → `bus.emit` `hotkey.triggered` (id→action) | WIRED | Unit-tested (on_hotkey_known_id_emits_hotkey_triggered) |
| TestModule | WindowManagerHandle | `ctx.on` handler → `windows.create(WindowSpec{Panel,"mybox test",400x300})` | WIRED | TestModule init; unit-tested (enqueues + wake once) |
| WindowManagerHandle | App window_rx | crossbeam `WindowRequest` channel; `create`/`destroy` fire wake hook (W3) | WIRED | app.rs `set_wakeup` → `AppEvent::WindowRequested`; unit-tested |
| App window_rx | create_window | `about_to_wait` `try_recv` drain → `create_window(el, spec)` (W2) | WIRED | app.rs `about_to_wait` loop |
| ConfigCenter | config.toml | `load_or_create`/`generate_file`/`save` real file I/O | WIRED | Real path via `config_file_path()`; round-trip tested in temp dirs |
| WindowManager | Renderer | `window_event` RedrawRequested → `renderer.present()` | WIRED | app.rs window_event routing; display_checks present pipeline |
| MenuEvent | EventBus | `on_menu` → `bus.emit` `menu.triggered` (menu_id JSON) | WIRED | Unit-tested (on_menu_emits_menu_triggered_with_menu_id) |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| TestModule window handler | action string | config `[hotkeys]` → `register_str` → `GlobalHotKeyEvent` → `on_hotkey` → bus | Yes — real config string flows through | ✓ FLOWING (unit-tested end-to-end) |
| Tray menu | module `menu_items()` | TestModule::menu_items → `App::run` collection → `TrayManager::build` | Yes | ✓ FLOWING |
| ConfigCenter table | `toml::Table` | real file read / first-run generation | Yes — real file I/O | ✓ FLOWING |
| WindowManager state | `WindowState` | `create_window` register → window_event routing | Yes | ✓ FLOWING |
| Renderer present | pixmap pixels | `draw` closure → `premul_rgba_to_u32` → softbuffer | Yes (opaque fill) | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Workspace compiles | `cargo check --workspace` | Finished, exit 0 | ✓ PASS |
| Full headless test suite | `cargo nextest run` | 77 passed, 4 skipped, exit 0 | ✓ PASS |
| App binary builds | `cargo build -p mybox-app` | Finished, exit 0 | ✓ PASS |
| Integration test binary compiles | `cargo build -p mybox-core --tests --bins` | Finished (2 test-only unused-Result warnings) | ✓ PASS |
| Display/hotkey/tray integration | `cargo test -p mybox-core -- --ignored` | Requires live macOS GUI session | ? SKIP → human/display verification |

### Probe Execution

No phase-declared or conventional `scripts/*/tests/probe-*.sh` probes exist for this phase. The phase's runnable verification is the nextest suite (run above, 77/77 passing, exit 0) and the `#[ignore]` display checks routed to a live session. N/A.

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| FRMW-01 | 01-01, 01-04 | Module trait + AppBuilder registration | ✓ SATISFIED | module.rs, app.rs, main.rs; tests green |
| FRMW-02 | 01-01, 01-02 | Event bus pub/sub, no direct deps | ✓ SATISFIED | event.rs EventBus, module boundary (mybox-test depends only on mybox-core) |
| FRMW-03 | 01-01, 01-02 | Overlay/Floating/Panel windows | ✓ SATISFIED | window.rs attributes builder + WindowManager + integration harness |
| FRMW-04 | 01-03, 01-04 | Global hotkey registration + callback | ✓ SATISFIED | hotkey.rs + app.rs on_hotkey chain; unit tests |
| FRMW-05 | 01-02, 01-04 | Worker thread for heavy work | ✓ SATISFIED | EventBus worker-thread dispatch; non-blocking emit test |
| FRMW-06 | 01-04 | macOS Accessory mode (no Dock) | ✓ SATISFIED | app.rs `ActivationPolicy::Accessory` |
| INFRA-01 | 01-03 | Sectioned TOML + namespace isolation | ✓ SATISFIED | config.rs namespace_isolation test |
| INFRA-02 | 01-03, 01-04 | Tray residency + module menu + quit | ✓ SATISFIED | tray.rs + app.rs build path |
| INFRA-03 | 01-01, 01-04 | Unified errors + logging | ✓ SATISFIED | error.rs + log throughout |
| INFRA-04 | 01-01, 01-03 | Config in user config dir | ✓ SATISFIED | config.rs `ProjectDirs` path contract test |

All 10 phase requirement IDs appear in at least one PLAN frontmatter (union of 01-01/02/03/04 = all 10). No orphaned requirements.

**NOTE (WARNING):** REQUIREMENTS.md still marks FRMW-02, FRMW-03, INFRA-01, INFRA-04 as `[ ]`/Pending in both the requirement list and the traceability table, although the code implements all four. This is a state-sync discrepancy in REQUIREMENTS.md (code evidence of implementation is complete) — routed to human decision (item #6).

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| crates/mybox-core/src/window.rs | 298, 527 | `batch_create` placeholder returning speculative ids | ℹ️ Info | Documented Phase-2 scope (D-09) in PLAN 01-02 and SKELETON Out-of-Scope; not a phase-goal stub |

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK` markers found in any modified source file. No `return null`/empty-implementation stubs. No disconnected props or hardcoded-empty data paths. The two `cargo build --tests` warnings are unused `Result` values in test code (module.rs tests), not functional.

### Human Verification Required

1. **Tray icon + no Dock entry (SC1)** — Launch `cargo run -p mybox-app` on macOS desktop; confirm mybox tray icon appears in the menu bar and the Dock shows no mybox entry.
2. **Cmd+Shift+T opens test window (SC2)** — Press Cmd+Shift+T; confirm a "mybox test" ~400x300 window appears; press again for a second window.
3. **First-run config generation (INFRA-04)** — After first launch, confirm `~/Library/Application Support/mybox/config.toml` exists with `[test]` and `[hotkeys]` sections.
4. **Tray menu + quit (INFRA-02, N2)** — Confirm tray menu contains "打开测试窗口" and "退出"; choosing 退出 terminates the app.
5. **MVP goal-format decision** — ROADMAP Phase 1 mode is `mvp` but the goal is not a User Story (`gsd-sdk query user-story.validate` returned `valid: false`). Decide: rewrite the goal as a User Story via `/gsd mvp-phase 1`, or accept the deviation.
6. **REQUIREMENTS.md state sync** — FRMW-02/03, INFRA-01/04 are implemented but marked Pending; update status or accept the bookkeeping delay.

These are documented in the phase's own `crates/mybox-app/tests/manual_checklist.md` (items 1-4) and this report (items 5-6).

### Gaps Summary

No code-level gaps found. All six must-have truths are verified against the actual codebase (source files read, wiring traced, tests executed: `cargo check --workspace` exit 0, `cargo nextest run` 77 passed / 4 skipped exit 0, `cargo build -p mybox-app` exit 0). The phase goal is achieved at the code level. Status is `human_needed` because success criteria 1-2 (and config/tray behavior) require a live macOS desktop session to observe, and two process-level decisions (MVP goal format, REQUIREMENTS.md status sync) need a human.

---

_Verified: 2026-08-12_
_Verifier: Claude (gsd-verifier)_
