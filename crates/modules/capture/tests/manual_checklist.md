# mybox Screenshot — Manual Verification Checklist (Phase 2, plan 02-04)

These checks cover Phase 2 success criteria 1-6 and need a real macOS GUI
session. Run them after `cargo run -p mybox-app` from a Terminal on the Mac
desktop.

> Prerequisite: grant mybox **Screen Recording** permission first (System
> Settings → Privacy & Security → Screen Recording) or accept it when prompted.

## 1. Trigger capture — overlay appears on every monitor (CAP-01/02)

- [ ] Launch `cargo run -p mybox-app` (keep it running in the terminal).
- [ ] Press `Cmd+Shift+S` (default `[capture].hotkey`; change it in
      `~/Library/Application Support/mybox/config.toml` under `[capture].hotkey`),
      or click the tray menu **"开始截图"**.
- [ ] Each monitor shows a full-screen overlay with the captured screen image
      dimmed by a semi-transparent black mask.

## 2. Drag selection — live border + WxH label (CAP-03)

- [ ] Drag a rectangle on the primary monitor.
- [ ] A white border and a `{w} × {h}` pixel label follow the drag in real time.

## 3. Resize with the 8 handles (CAP-03, D-02)

- [ ] After release, 8 handles (corners + edge midpoints) appear on the border.
- [ ] Drag a corner/edge handle to resize the selection; the label updates.

## 4. Annotate with all four tools + undo (CAP-06/07)

- [ ] Click the toolbar's rect / arrow / pen / text buttons and draw each shape
      in annotation orange (`0xFF6000`).
- [ ] Click **undo** (or press `Ctrl+Z` / `Cmd+Z`) repeatedly — annotations are
      removed one at a time back to the original image.

## 5. Enter copies the selection to the clipboard (CAP-04, D-01, D-04)

- [ ] With a selection made (and annotations drawn), press `Enter` (or click the
      toolbar **confirm** checkmark button).
- [ ] The overlay windows close immediately.
- [ ] In another app (e.g. Preview → File → New from Clipboard, or TextEdit
      paste), paste and verify the image **includes your annotations**.
- [ ] Repeat without any annotations and confirm the pasted image is the raw
      selection (D-01).

## 6. ESC cancels without copying (CAP-05, D-04)

- [ ] Start a new capture, make a selection, press `Esc`.
- [ ] The overlay closes and the clipboard still holds the previous content
      (nothing new was copied).

## 7. First-run permission guidance (CAP-08)

- [ ] Remove mybox from System Settings → Privacy & Security → Screen Recording.
- [ ] Trigger a screenshot again.
- [ ] The system authorization prompt appears (or System Settings opens to the
      Screen Recording pane via the deep link), and the terminal logs guidance
      text (授权 mybox；授权后可能需要重启 mybox).
- [ ] Grant permission (restart mybox if macOS asks) and confirm capture works.

## 8. Always-on-top limitation (A3 — accepted MVP limit, not a bug)

- [ ] Open a full-screen app (or menu bar) and trigger a screenshot over it.
- [ ] Observe whether the overlay appears above it. If not, record it as the
      known AlwaysOnTop-only limitation (Phase 4 re-evaluates screenSaver
      window level). This is **not** a defect of this plan.

---

### Display integration suite (automated, still needs a session)

Run from the worktree root:

```text
cargo test -- --ignored -p mybox-capture
```

This exercises real overlay window creation + composite/present, the drag
state machine, the confirm → clipboard copy (with read-back), and ESC teardown
in a live desktop session.
