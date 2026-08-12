---
phase: 1
slug: framework
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-11
---

# Phase 1 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo-nextest (unit/integration) + `cargo test -- --ignored` (display/OS tests) |
| **Config file** | `Cargo.toml` workspace + per-crate `[dev-dependencies]` |
| **Quick run command** | `cargo nextest run` |
| **Full suite command** | `cargo nextest run && cargo test -- --ignored` |
| **Estimated runtime** | ~10 seconds (quick), ~30 seconds (full with display tests) |

---

## Sampling Rate

- **After every task commit:** Run `cargo nextest run`
- **After every plan wave:** Run `cargo nextest run && cargo test -- --ignored`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | - | - | Workspace compiles, deps pinned to RESEARCH §1 | build | `cargo check --workspace && cargo metadata --no-deps` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 1 | INFRA-03 | T-1-01 / - | Typed errors; bad toml returns Err not panic; softbuffer errors mapped | unit | `cargo nextest run -p mybox-core error::tests` | ❌ W0 | ⬜ pending |
| 01-01-03 | 01 | 1 | FRMW-02 | - | EventFilter wildcard matching | unit | `cargo nextest run -p mybox-core event::tests` | ❌ W0 | ⬜ pending |
| 01-01-04 | 01 | 1 | FRMW-01 | - | Module trait registration: unique ID conflict rejected | unit | `cargo nextest run -p mybox-core module::tests` | ❌ W0 | ⬜ pending |
| 01-01-05 | 01 | 1 | FRMW-03 / INFRA-04 | - | WindowSpec default; config path uses platform dir, not hardcoded | unit | `cargo nextest run -p mybox-core window::tests` + `... config::tests` | ❌ W0 | ⬜ pending |
| 01-01-06 | 01 | 1 | - | - | premul_rgba_to_u32 pixel math | unit | `cargo nextest run -p mybox-core renderer::tests` | ❌ W0 | ⬜ pending |
| 01-01-07 | 01 | 1 | - | - | Public API exports compile | build | `cargo check --workspace` | ❌ W0 | ⬜ pending |
| 01-02-01 | 02 | 2 | FRMW-02 / FRMW-05 | - | Wildcard dispatch + non-blocking emit (FRMW-05) | unit | `cargo nextest run -p mybox-core event_bus::tests` | ❌ W0 | ⬜ pending |
| 01-02-02 | 02 | 2 | FRMW-03 | - | WindowManager id→state routing + attrs builder | unit | `cargo nextest run -p mybox-core window_manager::tests` | ❌ W0 | ⬜ pending |
| 01-02-03 | 02 | 2 | - | - | Renderer draw/present pixel correctness | unit | `cargo nextest run -p mybox-core renderer::tests` | ❌ W0 | ⬜ pending |
| 01-02-04 | 02 | 2 | - | - | Services wired into ModuleContext | unit | `cargo nextest run` | ❌ W0 | ⬜ pending |
| 01-03-01 | 03 | 2 | FRMW-04 | - | HotKey FromStr parse + id→action map | unit | `cargo nextest run -p mybox-core hotkey::tests` | ❌ W0 | ⬜ pending |
| 01-03-02 | 03 | 2 | INFRA-01 | - | Config namespace isolation enforced | unit | `cargo nextest run -p mybox-core config::tests::namespace` | ❌ W0 | ⬜ pending |
| 01-03-03 | 03 | 2 | INFRA-02 | - | Tray menu assembly: items + quit | unit | `cargo nextest run -p mybox-core tray::tests` | ❌ W0 | ⬜ pending |
| 01-03-04 | 03 | 2 | - | - | config()/hotkeys() accessors compile | unit | `cargo nextest run` | ❌ W0 | ⬜ pending |
| 01-04-01 | 04 | 3 | FRMW-06 | T-1-02 / - | App runs as Accessory (no Dock entry); [hotkeys] config registered at startup | unit + integration | `cargo nextest run -p mybox-core app::tests` + `cargo test -- --ignored event_loop` | ❌ W0 | ⬜ pending |
| 01-04-02 | 04 | 3 | FRMW-04 | - | Hotkey trigger reaches event loop | unit + integration | `cargo nextest run -p mybox-core app::tests` + `cargo test -- --ignored hotkey` | ❌ W0 | ⬜ pending |
| 01-04-03 | 04 | 3 | FRMW-03 | - | Overlay + Panel windows created; WindowRequest drained on main thread | integration | `cargo test -- --ignored window` | ❌ W0 | ⬜ pending |
| 01-04-04 | 04 | 3 | FRMW-01 | - | TestModule subscribes + enqueues WindowRequest::Create | unit | `cargo nextest run -p mybox-test` | ❌ W0 | ⬜ pending |
| 01-04-05 | 04 | 3 | INFRA-02 | - | Full suite + ignored display tests (panel/overlay/hotkey register/tray build) | integration | `cargo nextest run && cargo test -- --ignored` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/mybox-core/tests/` - test directory with module, event_bus, window, config, hotkey, tray test stubs
- [ ] `crates/mybox-app/tests/` - integration test directory with `#[ignore]` display tests
- [ ] `cargo-nextest` installed as dev tool

*Existing infrastructure: None - greenfield project. Wave 0 establishes test harness.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| App shows tray icon, no Dock entry | FRMW-06 | Requires visible desktop session | Launch app, verify tray icon appears, check Dock has no mybox entry |
| Global hotkey triggers test window | FRMW-04 | Requires real OS hotkey registration + user input | Press Cmd+Shift+T, verify test window appears |
| Config file created at correct path | INFRA-04 | Requires real user directory | After first launch, check `~/Library/Application Support/mybox/config.toml` exists with module defaults |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
