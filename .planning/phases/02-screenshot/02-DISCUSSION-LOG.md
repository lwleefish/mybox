# Phase 2: 截图模块 - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-08-12
**Phase:** 2-截图模块
**Areas discussed:** Screenshot flow staging

---

## Gray Area Selection

| Option | Description | Selected |
|--------|-------------|----------|
| Screenshot flow staging | Select -> annotate -> copy? Or select -> confirm -> clipboard + annotate? The requirements list both selection and annotation but the order/flow is ambiguous. | ✓ |
| Annotation tool UX | How does the user switch tools? Floating egui toolbar? Keyboard shortcuts? Both? | |
| Selection confirmation | How does the user confirm? Enter, double-click, button? Can they re-adjust? | |
| Visual style of overlay | How does the selection look during drag? Dimmed background, crosshair, size label position? | |

**User's choice:** Screenshot flow staging only.

---

## Screenshot Flow Staging

### Question 1: Overall flow

| Option | Description | Selected |
|--------|-------------|----------|
| Select -> Annotate -> Confirm | Drag to select -> annotation tools appear -> annotate (optional) -> Enter/click to confirm -> annotated image copied -> overlay closes. Most tools (Snipaste, Shottr) work this way. | ✓ |
| Select -> Confirm -> Annotate | Drag to select -> confirm -> raw image immediately in clipboard -> overlay stays for annotation -> if annotated, clipboard updates. | |
| Select -> Copy immediately | Drag to select -> raw image copied -> overlay closes. Annotation is a separate step. Simplest flow. | |

**User's choice:** Select -> Annotate -> Confirm
**Notes:** Follows the Snipaste/Shottr pattern. Annotation is optional before confirmation.

---

### Question 2: Selection adjustability

| Option | Description | Selected |
|--------|-------------|----------|
| Lock selection, then annotate | After drag-release, selection is locked. Annotation toolbar appears. ESC cancels to re-trigger. | |
| Adjustable selection, then annotate | After drag-release, selection handles appear (8 drag points). User can resize/move, then switch to annotation. | ✓ |
| Both simultaneously | After drag-release, toolbar appears AND selection stays adjustable. Most flexible, most complex. | |

**User's choice:** Adjustable selection, then annotate
**Notes:** 8 drag handles (4 corners + 4 edge midpoints) for resize and move before annotation.

---

### Question 3: Mode switching (selection <-> annotation)

| Option | Description | Selected |
|--------|-------------|----------|
| Unified toolbar (no modes) | Toolbar appears with both selection handles and annotation tools. Tool choice determines action. No explicit mode switch. | ✓ |
| Toolbar with explicit switch | Toolbar appears with tool buttons. Clicking a tool enters annotation mode (handles disappear). 'Back' button returns to selection. | |
| Keyboard-driven switch | Selection handles active by default. Tool shortcut keys (R/A/B/T) switch to annotation. Esc returns to selection, Esc again cancels. | |

**User's choice:** Unified toolbar (no modes)
**Notes:** No explicit mode switching. The current tool choice determines what the user's input does. Selection handles and annotation tools coexist.

---

### Question 4: Confirmation and ESC behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Enter/confirm + ESC cancel | Enter or toolbar confirm button copies to clipboard, closes overlay. ESC cancels everything immediately. Simple and predictable. | ✓ |
| Double-click + staged ESC | Double-click inside selection confirms. ESC: first press exits annotation, second press cancels screenshot. More nuanced. | |
| Button + toast feedback | Toolbar copy button + Enter. After copy, brief toast notification. ESC cancels. Extra feedback but more UI. | |

**User's choice:** Enter/confirm + ESC cancel
**Notes:** Simple and predictable. ESC is a full cancel, not staged. No toast notification.

---

## Claude's Discretion

- Screen capture library selection (xcap vs screenshots vs scrap)
- Annotation drawing implementation (tiny-skia path/shape API)
- Toolbar UI layout and visual design (egui integration)
- Undo stack internal data structure
- Selection handle visual style
- Size label (WxH) display position and format
- Overlay rendering pipeline integration (how to composite captured screen + mask + selection + annotations through Renderer::draw)
- batch_create real implementation (D-09 per-monitor window strategy)
- macOS permission detection API calls
- Clipboard copy implementation (arboard integration)

## Deferred Ideas

None - discussion stayed within phase scope.
