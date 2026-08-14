---
phase: 2
slug: screenshot
status: approved
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-13
---

# Phase 2 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo-nextest 0.9.143 (unit/integration) + `cargo test -- --ignored` (display/OS tests, subprocess-per-check pattern from Phase 1) |
| **Config file** | workspace `Cargo.toml` + per-crate `[dev-dependencies]` |
| **Quick run command** | `cargo nextest run -p mybox-core -p mybox-capture` |
| **Full suite command** | `cargo nextest run && cargo test -- --ignored` |
| **Estimated runtime** | ~10 seconds quick; ~60 seconds full (includes display checks) |

---

## Sampling Rate

- **After every task commit:** Run `cargo nextest run -p mybox-core -p mybox-capture`
- **After every plan wave:** Run `cargo nextest run && cargo test -- --ignored`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 02-01-T2 | 01 | 1 | (prereq: draw chain + Ui panic) | T-2-03, T-2-16 | RedrawRequested calls draw then present (MockRenderer records calls); `AppEvent::Ui(f)` catch_unwind (WR-06) | unit + source assert | `cargo nextest run -p mybox-core app::redraw_draws_then_presents` | ❌ W0 | ⬜ pending |
| 02-01-T3 | 01 | 1 | CAP-01 | T-2-01, T-2-02, T-2-04 | Capture spawns on worker thread; `start_screenshot` hotkey auto-registered from `[capture].hotkey`; preflight blocks denied capture | unit (mockable capture fn) | `cargo nextest run -p mybox-capture capture::tests permission::tests` | ❌ W0 | ⬜ pending |
| 02-02-T1 | 02 | 2 | CAP-02 | T-2-05 | Draw closure composites image + mask correctly (per-monitor overlay) | unit (headless Pixmap pixel asserts) | `cargo nextest run -p mybox-capture overlay::tests` | ❌ W0 | ⬜ pending |
| 02-02-T2 | 02 | 2 | CAP-03 | - | Selection state machine: drag updates selection; 8 handles hit/resize | unit | `cargo nextest run -p mybox-capture selection::tests` | ❌ W0 | ⬜ pending |
| 02-02-T3 | 02 | 2 | CAP-03, CAP-05 | T-2-06, T-2-07 | Border + WxH rendered; ESC -> cancel -> Destroy requests for all overlays | unit | `cargo nextest run -p mybox-capture session::tests::cancel text::tests` | ❌ W0 | ⬜ pending |
| 02-03-T1 | 03 | 3 | CAP-06, CAP-07 | T-2-08, T-2-09, T-2-10 | Rect/arrow/pen/text drawn to Pixmap; undo stack pop semantics | unit (headless tiny-skia) | `cargo nextest run -p mybox-capture annotate::tests text::tests` | ❌ W0 | ⬜ pending |
| 02-03-T2 | 03 | 3 | CAP-06, CAP-07 | T-2-11 | Toolbar hit-test switches tool; Ctrl+Z triggers undo to empty == original | unit | `cargo nextest run -p mybox-capture annotate::tests::undo` | ❌ W0 | ⬜ pending |
| 02-04-T1 | 04 | 4 | CAP-04 | T-2-12, T-2-13, T-2-15 | Crop->ImageData (RGBA8 straight); bake annotations; confirm flow | unit + #[ignore] display | `cargo nextest run -p mybox-capture clipboard::tests session::tests::confirm` | ❌ W0 | ⬜ pending |
| 02-04-T2 | 04 | 4 | CAP-08 | T-2-02, T-2-14 | Preflight -> CGRequest -> deep-link guidance -> abort if denied | unit (injectable checker) | `cargo nextest run -p mybox-capture permission::tests` | ❌ W0 | ⬜ pending |
| 02-04-T3 | 04 | 4 | CAP-01~08 | - | Overlay shows capture, drag selects, Enter copies, ESC closes (E2E) | integration (#[ignore]) + manual | `cargo test -- --ignored -p mybox-capture` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/modules/capture/` - new module crate (lib.rs, session/selection/annotate/clipboard/permission modules)
- [ ] `crates/modules/capture/tests/` - unit test dirs per module; `#[ignore]` display integration (subprocess-per-check harness like `mybox-core/src/bin/display_checks.rs`)
- [ ] `crates/mybox-core/src/` - `WindowSpec.on_draw` field + `WindowRequest::Redraw` variant + App wiring; extend existing `window.rs`/`app.rs` unit tests
- [ ] Framework-installed: cargo-nextest already installed (0.9.143) - no action
- [ ] `cargo add xcap arboard ab_glyph` (+ macOS `objc2-core-graphics`) - behind `checkpoint:human-verify` per Package Legitimacy Audit

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Overlay window appears on screen showing captured image | CAP-01, CAP-02 | Requires live display + window system | 1. Run app 2. Press screenshot hotkey 3. Verify overlay appears with screen content |
| Drag selection shows real-time border and size label | CAP-03 | Requires live mouse input + rendering | 1. In overlay, drag to select 2. Verify border + WxH label update in real-time |
| Annotation tools draw visually correct shapes | CAP-06 | Visual correctness needs human eye | 1. Select region 2. Use each tool (rect/arrow/pen/text) 3. Verify shapes render correctly |
| Ctrl+Z undo works visually | CAP-07 | Visual state verification | 1. Draw annotations 2. Press Ctrl+Z 3. Verify last annotation removed |
| Enter copies image to clipboard | CAP-04 | Cross-app clipboard verification | 1. Select region 2. Press Enter 3. Paste in another app 4. Verify correct image |
| ESC cancels and closes overlay | CAP-05 | Window behavior verification | 1. Start screenshot 2. Press ESC 3. Verify overlay closes, no clipboard copy |
| macOS permission prompt appears on first use | CAP-08 | OS-level permission dialog | 1. Revoke Screen Recording permission 2. Trigger screenshot 3. Verify prompt appears |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (02-01-T1 is the package-legitimacy human gate — exempt as `checkpoint:human-verify`)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 10s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved (2026-08-13, revision pass — per-task map synced to final plan structure)
