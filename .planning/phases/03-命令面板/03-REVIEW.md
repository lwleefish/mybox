---
phase: 03-命令面板
reviewed: 2026-08-15T05:35:40Z
depth: standard
files_reviewed: 7
files_reviewed_list:
  - crates/mybox-core/src/app.rs
  - crates/mybox-core/src/window.rs
  - crates/modules/palette/src/lib.rs
  - crates/modules/palette/src/session.rs
  - crates/modules/palette/src/raster.rs
  - crates/modules/palette/src/bin/palette_checks.rs
  - crates/modules/palette/tests/integration.rs
findings:
  critical: 0
  warning: 4
  info: 4
  total: 8
status: issues_found
---

# Phase 03: Code Review Report

**Reviewed:** 2026-08-15T05:35:40Z
**Depth:** standard
**Files Reviewed:** 7
**Status:** issues_found

## Summary

Reviewed the gap-closure work for plans 03-03 (GAP-1) and 03-04 (GAP-2) across the core (`app.rs`, `window.rs`) and the palette module (`lib.rs`, `session.rs`, `raster.rs`, `palette_checks.rs`, `tests/integration.rs`).

**The two gap fixes themselves are correct and well-tested:**

- **GAP-1** — `App::on_hotkey` now drops non-`Pressed` `GlobalHotKeyEvent`s before dispatch (regression-locked by `on_hotkey_released_event_is_ignored`), and the build-destroy pairing moved from the broadcast `core/window-created` bus event to the per-window `WindowSpec.on_created` callback, which `App::create_window` invokes on the main thread after `register` but before the broadcast — so a pending close destroys the late window in the same `about_to_wait` drain pass and other modules' windows can never touch the palette session. The state machine's `pending_close`/generation guards make the fast-toggle, close-before-create, and mid-execution re-summon paths sound (verified by tracing `toggle_palette` → `close`/`summon` → `on_window_created` and the FIFO drain order).
- **GAP-2** — the textured-path dispatch now follows the epaint contract (any non-`WHITE_UV` vertex or non-default `TextureId` ⇒ textured), `apply_textures` patches partial `ImageDelta`s in place with bounds-checked row copies (verified against the epaint 0.30 `TextureAtlas::take_delta` semantics), the per-frame card-background fill prevents stale glyph residue after geometry shrinks, and the E2E `glyph_shape` probe exercises the incremental atlas-patch path with real `Ime::Commit` injections. The rasterizer's premultiplied blend math was hand-verified: the over-blend cannot overflow u8 (the premultiplied invariant `rgb ≤ a` holds through the straight→premultiplied texture multiply), and the −0.5 half-texel UV offset matches GL_LINEAR's texel-center convention.

No critical issues were found. Four warnings and four info items follow — three warnings live in code that pre-dates the gap-closure commits (03-01/03-02) but sits in the reviewed files; one warning (WR-03) is a genuine robustness gap in the new GAP-1 pairing design.

## Warnings

### WR-01: Executing/Error state transitions never trigger the window-height sync (clipped UI)

**File:** `crates/modules/palette/src/lib.rs:260-311` (frame loop), `session.rs`/`execute.rs` transitions
**Issue:** `sync_window_geometry` only fires when `prev_input != session.input() || prev_state != session.state()` — i.e. when the state changes *during* `egui_ctx.run` (TextEdit writeback → Filtering/Empty). The `Executing` transition happens in the Enter key handler (outside a frame) and the `Error` transition happens in the async finalize hop (also outside a frame), so on the next `RedrawRequested` the before/after snapshot is already `Executing`/`Error` and the comparison is false — the window never resizes. Per the UI-SPEC height table, `Executing` needs `112 + 48·n` vs `Idle/Filtering` `80 + 48·n` (+32 logical px), and `Error` needs 144 vs 128 for a 1-row Idle list (+16 px). Result: during Executing the bottom of the dimmed list is clipped off the card, and the Error block is clipped for small lists. (Pre-existing 03-02 code; surfaced while reviewing the in-scope files.)
**Fix:** Make geometry sync event-driven rather than frame-diff-driven: call `sync_window_geometry` (or set a "geometry dirty" flag consumed by the next frame) from the Enter arm after `execute::execute`, and from the finalize hop in `execute.rs` when the session enters `Error`. Alternatively, keep the snapshot comparison but persist the *last rendered* state across frames and compare against that instead of a per-frame snapshot.

### WR-02: Framebuffer is allocated once at summon and never resized — any window growth leaves an unpainted strip

**File:** `crates/modules/palette/src/lib.rs:179` (`install_framebuffer` at summon only), `lib.rs:314-330` (`on_draw` blits only the framebuffer), `session.rs:301-303`
**Issue:** `install_framebuffer` is called exactly once, in `summon_palette`, at the Idle height. The `on_draw` closure blits only that framebuffer onto the renderer pixmap, and `TinySkiaSoftbufferRenderer::resize` allocates a fresh *zeroed* pixmap on every `Resized` event. The moment WR-01 is fixed (or any future state makes the window taller than the summon height), the region below the framebuffer is transparent — and softbuffer's macOS backend drops per-pixel alpha, so the strip renders black, not card-background `#202020`. (The `install_framebuffer`-at-summon design is 03-01 code; the per-frame clear added in 03-04 fills only the fixed-size framebuffer, not the whole window.)
**Fix:** In `on_draw`, fill the entire pixmap with the card color before blitting (`pixmap.fill(tiny_skia::Color::from_rgba8(0x20, 0x20, 0x20, 0xFF))` then `draw_pixmap`), and/or reallocate the framebuffer in `sync_window_geometry` when the physical height changes. Filling the pixmap first is the robust minimal fix — it also covers the transparent-region case on Windows.

### WR-03: A failed window creation wedges `pending_close` and permanently disables the palette toggle

**File:** `crates/modules/palette/src/session.rs:151-176` (`has_live_window`/`close`), `crates/mybox-core/src/app.rs:519-521` (Create failure path)
**Issue:** The GAP-1 pairing design relies on `on_created` running exactly when the window materializes. If `App::create_window` *fails* (e.g. `el.create_window` error or the renderer factory — softbuffer surface creation can fail — returns `Err`; the App logs and drops the spec at `app.rs:519-521`), `on_created` never runs, so `pending_close` stays `true` forever. `has_live_window()` then returns `true` on every subsequent hotkey press (it includes `pending_close`), `close()` returns `None` (state is already `Hidden`, no `window_id`), and the toggle can never summon again — the palette is dead until the app restarts. There is no timeout or failure notification clearing the flag.
**Fix:** Give the framework a failure path and wire it into the pairing: e.g. add an optional `on_create_failed: Option<Box<dyn Fn() + Send + Sync>>` (or a `WindowSpec`-level failure callback) invoked in the `create_window` error arm, and have the palette's callback clear `pending_close` (via a small `clear_pending_close()` session method). A cheaper mitigation: `summon()` already resets `pending_close = false`, so any path that lets a summon through unwedges it — but `has_live_window()` currently blocks that path, so the flag must be cleared explicitly on the failure path.

### WR-04: `ImageData::Color` textures are double-premultiplied in the textured path

**File:** `crates/modules/palette/src/raster.rs:31-48` (`texture_pixels`), `raster.rs:299-304` (premultiply in `paint_textured_triangle`)
**Issue:** `TexturePixels.data` is documented as "Straight RGBA8", and `paint_textured_triangle` treats it as straight (multiplying by `tex[3]/255`). But the `ImageData::Color` arm copies `p.r()/g()/b()/a()` — and `ecolor::Color32` (egui 0.30, verified in the vendored source) stores **premultiplied** sRGBA; `ColorImage.pixels` is a `Vec<Color32>` of premultiplied values. Semi-transparent color textures therefore get multiplied by alpha twice and render too dark. This is latent today (the palette emits only `ImageData::Font` atlas textures, and the gray font path is exact because premultiplied == straight for `r=g=b=a`), but the GAP-2 UV dispatch is what routes *all* non-WHITE_UV meshes through this path — the first image/icon a module tessellates will hit it.
**Fix:** Convert with `to_srgba_unmultiplied()` in the Color arm:
```rust
egui::epaint::ImageData::Color(c) => {
    let mut data = Vec::with_capacity(c.pixels.len() * 4);
    for p in &c.pixels {
        data.extend_from_slice(&p.to_srgba_unmultiplied());
    }
    TexturePixels { size: c.size, data }
}
```
(The Font arm is correct as-is.)

## Info

### IN-01: Session doc comment contradicts the actual threading model

**File:** `crates/modules/palette/src/session.rs:42-44`
**Issue:** The `SessionInner` doc claims "the lock is only ever taken on the main thread, so there is zero contention" — false. `toggle_palette` (bus worker thread) calls `has_live_window`/`close`/`summon`/`install_framebuffer`, which all lock the same state mutex concurrently with the main-thread frame loop. No deadlock exists (the bus thread never acquires `egui_ctx`, so the `state → egui_ctx` vs `egui_ctx → state` ordering inversion never forms a cycle), but the comment misleads future maintainers about cross-thread contention and lock-ordering hazards.
**Fix:** Update the comment to state the actual discipline: the state lock is shared between the main thread (frame loop) and the bus worker thread (toggle/close/summon); `egui_ctx` remains main-thread-only, and lock nesting `state → egui_ctx` (`ensure_winit_state`) must never meet a holder of `egui_ctx` that wants `state`.

### IN-02: `WindowSpec.transparent` and `WindowSpec.decorations` are silently ignored

**File:** `crates/mybox-core/src/window.rs:198-236` (`window_attributes`), `window.rs:29-64` (pub fields)
**Issue:** `WindowSpec` exposes public `transparent` and `decorations` fields, but `window_attributes` never reads them — the `kind` profile owns those attributes and overrides them (the function's doc acknowledges this). A module constructing `WindowSpec { kind: Panel, decorations: false, .. }` silently gets decorations. The API invites misuse; only `always_on_top`, `inner_size`, `position`, `cursor_icon`, and `visible` are honored.
**Fix:** Either remove the two fields (breaking the harness's field-copy code) or assert/log when a spec's `transparent`/`decorations` disagree with its `kind` profile, so the mismatch is visible instead of silent.

### IN-03: `palette_checks` harness leaks prior-round windows; "dropping it closes it" comment is false

**File:** `crates/modules/palette/src/bin/palette_checks.rs:87-124` (`realize_window`)
**Issue:** `realize_window` comments that replacing `self.window` drops the previous window and closes it — but `self.wm.register(...)` stored `Some(Arc::clone(&window))` for every round, and `WindowManager` states are never destroyed, so each prior round's window stays alive (and visible) until process exit. In `five_summon_esc_no_residue` and `consecutive_summon_close`, up to 5 real windows accumulate on screen during the run. This does not affect the assertions (they read the session state and the request queue, and stale windows' events are filtered by `current_winit_id`), but it undermines the "no residue" narrative and could confuse a human watching the test run.
**Fix:** In `realize_window`, before replacing `self.window`, drain `self.wm` (e.g. `self.wm.close_all()`) so the old window's Arc refcount drops and it actually closes; correct the comment.

### IN-04: Stale assertion criteria in the Test 6 doc comment

**File:** `crates/modules/palette/tests/integration.rs:87-93`
**Issue:** The doc comment for `palette_glyph_shape` describes thresholds the check no longer uses: "sparse coverage <0.7, ≥1 fully-covered pixel". The actual `check_glyph_shape` probe asserts bbox ≥ 8x8 physical px, ≥16 distinct text RGBA values, and `aa_spread` ≥ 120 (the 03-04 SUMMARY criteria — the composited card is fully opaque, so coverage/alpha metrics were replaced).
**Fix:** Update the comment to match the implemented criteria (bbox ≥8x8, ≥16 distinct values, aa_spread ≥120, frame diff > 0), and remove the obsolete coverage sentence.

---

_Reviewed: 2026-08-15T05:35:40Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
