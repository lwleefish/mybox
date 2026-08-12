# mybox Walking Skeleton — Manual Verification Checklist (Phase 1, plan 01-04)

These checks need a real macOS GUI session and cannot be automated headlessly
(they are the plan's success criteria 1 and 2). Run them after `cargo run -p
mybox-app` from a Terminal on the Mac desktop.

> Prerequisite: macOS Screen Recording permission is **not** required in Phase 1
> (that is Phase 2, CAP-08). Hotkey registration may prompt for Accessibility
> permission the first time; grant it in System Settings → Privacy & Security →
> Accessibility if the hotkey does not fire.

## 1. Tray icon shows, no Dock entry (FRMW-06) — success criterion 1

- [ ] Launch: `cargo run -p mybox-app` (keep it running in the terminal).
- [ ] A mybox tray icon appears in the macOS menu bar (top-right, monochrome
      circle template icon).
- [ ] The Dock shows **no** mybox entry (the app runs in Accessory mode).

## 2. Global hotkey opens the test window (FRMW-04 + FRMW-03) — success criterion 2

- [ ] Press `Cmd+Shift+T`.
- [ ] A window titled **"mybox test"**, roughly 400×300, appears with opaque
      content.
- [ ] (Optional) press it again — a second test window appears.

## 3. First-run config generation (INFRA-04)

- [ ] After first launch, `~/Library/Application Support/mybox/config.toml`
      exists and contains both a `[test]` section (`message = "hello from
      test"`) and a `[hotkeys]` section (`open_test_window = "Cmd+Shift+T"`).

## 4. Tray menu items (INFRA-02)

- [ ] Right-click / long-press the tray icon: the menu contains
      **"打开测试窗口"** and **"退出"** (separated by a separator).

## 5. Quit behavior (N2)

- [ ] Choosing **"退出"** terminates the app (macOS native `PredefinedMenuItem
      ::quit` → `terminate:`); the terminal process exits.

---

### Display integration suite (automated, still needs a session)

Run from the worktree root:

```text
cargo test -- --ignored -p mybox-core
```

This exercises real window creation (Panel + Overlay), hotkey manager
init + registration, and tray build — all in a live desktop session.
