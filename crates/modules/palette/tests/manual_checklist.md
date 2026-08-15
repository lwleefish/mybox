# mybox 命令面板 — Manual Verification Checklist (Phase 3, plan 03-02)

These checks cover Phase 3 success criteria 1-5 plus the builtin commands, the
screenshot timing, and the corner radius. Run them after
`cargo run -p mybox-app` from a Terminal on the Mac desktop.

> Prerequisites: mybox already granted **Screen Recording** permission
> (Phase 2); the palette needs no additional permission.

## 1. Summon / close — centered panel, auto-focused input, 5× no residue (PAL-01)

- [ ] Launch `cargo run -p mybox-app` (keep it running in the terminal).
- [ ] Press `Cmd+Shift+Space` (default `[palette].hotkey`; override in
      `~/Library/Application Support/mybox/config.toml` under `[palette].hotkey`).
- [ ] A centered, dark, rounded panel appears on the monitor containing the
      cursor; the input box is auto-focused (type immediately — characters
      appear without clicking).
- [ ] Press the hotkey again — the panel closes.
- [ ] Summon and close 5 times in a row — no leftover/orphan windows, no
      crash (re-entrancy acceptance).

## 2. Full command list (PAL-02)

- [ ] With an empty input the panel lists at least 5 commands in registration
      order: 开始截图, then 退出应用 / 打开配置目录 / 重启应用 / 打开日志文件.
- [ ] Each row shows the name (white, 14px) with the description (grey,
      12px) below it.

## 3. Fuzzy filter + highlight (PAL-03, D-10)

- [ ] Type `截图` — 开始截图 ranks FIRST, and the matched characters
      截 / 图 render in orange `#FF6000`.
- [ ] Clear the input and type `jt` — 开始截图 ranks FIRST (pinyin keyword
      `jietu`).
- [ ] Type a nonsense string (e.g. `zzzz`) — the panel shows the empty state:
      「没有匹配的命令」/「换个关键词试试，清空输入可显示全部命令」.
- [ ] Clear the input — the full command list returns in registration order.

## 4. Keyboard navigation + execution (PAL-04)

- [ ] With the full list, press `↓` — the first row highlights; `↑` from the
      first row wraps to the last row.
- [ ] Type a filter, then press `↓` — the highlight moves through the
      FILTERED list; changing the input resets the highlight to the top.
- [ ] With no highlight (empty input), press `Enter` — the first command
      (开始截图) executes.
- [ ] Execute `退出应用` — the app exits cleanly.
- [ ] Execute `打开配置目录` — Finder opens `~/Library/Application Support/mybox/`.
- [ ] Execute `打开日志文件` — the log file opens (it exists from startup,
      D-12).
- [ ] Execute `重启应用` — a new mybox process spawns and the old one exits.

## 5. Screenshot timing (SPEC hard constraint) + execution status (D-04/D-05)

- [ ] Press `Enter` on 开始截图 (or `Cmd+Shift+S`): the palette DISAPPEARS
      FIRST, then the screenshot overlay appears — the panel is never visible
      in the captured image.
- [ ] Execute 打开配置目录 (a quick command): while it runs, the input box
      below shows 「正在执行：打开配置目录…」, the list dims (50%), and the
      input is disabled; the panel closes itself when done.
- [ ] Failure path (if you can provoke one, e.g. a bad config): the panel
      stays open showing 执行「…」失败 + the error text; pressing any key or
      ESC closes it.

## 6. ESC closes without executing (PAL-05)

- [ ] Type `截图`, then press `Esc` — the panel closes and NOTHING executes
      (no screenshot overlay appears).

## 7. Corner radius check (A2)

- [ ] With a light wallpaper, check the four corners of the panel — they
      should be rounded (12px). If the corners render square, record it as
      the accepted MVP fallback (A2 — Phase 4 polish), not a defect.

---

### Display integration suite (automated, still needs a desktop session)

Run from the workspace root:

```text
cargo test -- --ignored -p mybox-palette
```

This exercises the real summon → render → present chain, fuzzy-filtered
navigation with Enter execution (including the real UiThreadProxy finalize
hop), the capture hide-before-execute ordering, and 5× summon-ESC residue
freedom in a live desktop session (one subprocess per check).
