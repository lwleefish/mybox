---
phase: 3
slug: palette
status: approved
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-14
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo-nextest 0.9.143 (unit/integration) + `cargo test -- --ignored` (display/OS, subprocess-per-check) |
| **Config file** | workspace `Cargo.toml` + per-crate `[dev-dependencies]` |
| **Quick run command** | `cargo nextest run -p mybox-core -p mybox-palette` |
| **Full suite command** | `cargo nextest run && cargo test -- --ignored` |
| **Estimated runtime** | ~15s quick; ~90s full |

---

## Sampling Rate

- **After every task commit:** Run `cargo nextest run -p mybox-core -p mybox-palette`
- **After every plan wave:** Run `cargo nextest run && cargo test -- --ignored`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** ~15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 03-01-03 | 01 | 1 | PAL-01 | T-3-01 / — | toggle state machine: summon creates (position computed), re-trigger closes, 5× summon/ESC no residue | unit (headless session + fake handles) | `cargo nextest run -p mybox-palette session::tests` | ❌ W0 | ⬜ pending |
| 03-01-01 | 01 | 1 | PAL-02 | T-3-02 / — | registry: ≥5 commands, module-first order, duplicate id rejected, non-empty name/description | unit | `cargo nextest run -p mybox-core command::tests` | ❌ W0 | ⬜ pending |
| 03-02-01 | 02 | 2 | PAL-03 | T-3-03 / — | filter: "截图" and "jt" hit capture.start first; no-match → Empty; empty input → all; highlight indices correct; tie-break stable | unit (pure) | `cargo nextest run -p mybox-palette filter::tests` | ❌ W0 | ⬜ pending |
| 03-02-02 | 02 | 2 | PAL-04 | T-3-04 / — | navigation: selection reset on input, ↑/↓ wrap, Enter executes first when none; execute: Executing state, runner runs off main thread, Ok→close, Err→Error state, generation guard | unit (fake runner + counting) | `cargo nextest run -p mybox-palette session::tests execute::tests` | ❌ W0 | ⬜ pending |
| 03-02-02 | 02 | 2 | PAL-05 | T-3-05 / — | ESC → destroy enqueued, no command executed | unit | `cargo nextest run -p mybox-palette session::tests::esc` | ❌ W0 | ⬜ pending |
| — | 02 | 2 | Capture exception | T-3-06 / — | hide-before-execute enqueues Destroy before runner invocation (queue order assertion) | unit | `cargo nextest run -p mybox-palette execute::tests` | ❌ W0 | ⬜ pending |
| — | 01 | 1 | Builtins | T-3-07 / — | quit emits app-exit; restart spawns current_exe then exits; open_config/open_log invoke platform opener with the right path (injectable spawner) | unit | `cargo nextest run -p mybox-core command::tests` | ❌ W0 | ⬜ pending |
| — | 01 | 1 | Rasterizer | T-3-08 / — | headless egui frame (Chinese label) → tessellate → framebuffer has non-background pixels; solid fast path == barycentric path on solid triangle | unit (no window — egui Context is headless-safe) | `cargo nextest run -p mybox-palette raster::tests` | ❌ W0 | ⬜ pending |
| — | 02 | 2 | E2E | T-3-09 / — | real window: summon/focus/type/enter/esc; capture.start hides palette before overlay appears | integration (#[ignore], subprocess-per-check) + manual checklist | `cargo test -- --ignored -p mybox-palette` | ❌ W0 | ⬜ pending |

> Task IDs mirror the revised 03-01 split (Task 3 = module skeleton + session + position + fonts + hotkey/lifecycle; Task 4 = raster + ui render chain). Wave 0 checklist items for `bin/palette_checks.rs`/`tests/integration.rs`/manual checklist are created in 03-02 Task 3 (wave 2) — listed here as forward references.

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/modules/palette/` — new crate (lib/session/filter/ui/raster/position/execute/fonts + tests + bin/palette_checks.rs)
- [ ] `crates/mybox-core/src/command.rs` — Command/CommandRegistry/BuiltinCommands/run_command + tests
- [ ] Core additive changes C1–C6 with tests (on_event_win routing, Floating profile assertions, AppEvent::Exit)
- [ ] `crates/modules/capture/src/lib.rs` — `commands()` impl (runner reusing start_capture) + keyword `"jietu"`
- [ ] `crates/mybox-app/src/main.rs` — dual-sink logger (D-12) + palette module registration
- [ ] `crates/modules/test/src/lib.rs` — fix pre-existing WindowRequest match arms (Pitfall 7)
- [ ] `cargo add egui egui-winit fuzzy-matcher pollster` (behind slopcheck — already [OK]-verified)
- [ ] `rustup target add x86_64-pc-windows-msvc` (or Phase 4 deferral note)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Global hotkey summons palette, re-trigger closes | PAL-01 | OS-level hotkey registration not unit-testable headless | Run app, press global hotkey twice, confirm toggle |
| Palette lists capture module commands ("开始截图" etc.) | PAL-02 | Requires real module registration + window | Summon palette, verify command list renders |
| Rounded corners on macOS (layer trick) | — | Runtime visual check; square-corner fallback documented | Summon palette on macOS, inspect corners |
| Windows behavior (summon/type/enter/esc) | PAL-01..05 | No Windows runner in this environment | On Windows VM/machine, run manual checklist of all 5 success criteria |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-08-14
