---
status: partial
phase: 01-framework
source: [01-VERIFICATION.md]
started: 2026-08-12T09:20:00Z
updated: 2026-08-12T09:20:00Z
---

## Current Test

[awaiting human testing]

## Tests

### 1. Tray icon + no Dock entry (SC1, FRMW-06/INFRA-02)
expected: Launch `cargo run -p mybox-app` on a macOS desktop session; confirm a mybox tray icon appears in the menu bar and the Dock shows no mybox entry. App runs in Accessory mode.
result: [pending]

### 2. Cmd+Shift+T opens test window (SC2, FRMW-04+FRMW-03)
expected: Press Cmd+Shift+T after launch; confirm a window titled 'mybox test' (~400x300) appears. Pressing again opens another.
result: [pending]

### 3. First-run config generation (INFRA-04)
expected: After first launch, verify `~/Library/Application Support/mybox/config.toml` exists with a [test] section (message = 'hello from test') and a [hotkeys] section (open_test_window = 'Cmd+Shift+T').
result: [pending]

### 4. Tray menu + quit (INFRA-02, N2)
expected: Right-click the tray icon: confirm the menu contains '打开测试窗口' and '退出'; choosing 退出 terminates the app.
result: [pending]

### 5. MVP goal-format decision
expected: ROADMAP Phase 1 mode is `mvp` but the goal is not in User Story format. Run /gsd mvp-phase 1 to set a proper User Story goal, or explicitly accept the deviation.
result: [pending]

### 6. REQUIREMENTS.md state sync
expected: FRMW-02, FRMW-03, INFRA-01, INFRA-04 are implemented in code but still marked `[ ]`/Pending in REQUIREMENTS.md. Update requirement status to Complete, or accept the bookkeeping delay.
result: [pending]

## Summary

total: 6
passed: 0
issues: 0
pending: 6
skipped: 0
blocked: 0

## Gaps
