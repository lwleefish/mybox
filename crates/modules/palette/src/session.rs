//! Palette session state — the shared, lock-guarded state machine the palette
//! module drives (six states per the UI-SPEC interaction contract; 03-01 only
//! drives Hidden/Idle — Filtering/Empty/Executing/Error are filled in by
//! 03-02).
//!
//! Discipline mirrors `capture::session`: `std::sync::Mutex` (parking_lot
//! never crosses the FRMW-02 module boundary), all state transitions are
//! methods so they are headless-testable.

use std::collections::HashMap;
use std::sync::Arc;

use mybox_core::command::Command;
use mybox_core::egui;
use mybox_core::egui_winit;
use mybox_core::log;
use mybox_core::tiny_skia;
use mybox_core::winit;
use mybox_core::WindowId;

use crate::filter;

/// Palette interaction states (UI-SPEC interaction contract).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteState {
    /// No window.
    Hidden,
    /// Summoned with empty input: full command list, no selection.
    Idle,
    /// Non-empty input: fuzzy-filtered list, selection at index 0.
    Filtering,
    /// No command matches the input.
    Empty,
    /// A command is executing (input + list disabled, D-04).
    Executing,
    /// Execution failed (any key closes, D-05).
    Error,
}

/// The lock-guarded session payload. `egui_winit::State` is `Send` (all its
/// fields are) but not `Sync`; the surrounding `Mutex` provides the `Sync`
/// bound the `WindowSpec` closures require — the lock is only ever taken on
/// the main thread, so there is zero contention.
struct SessionInner {
    state: PaletteState,
    input: String,
    selection: Option<usize>,
    /// Indices into `commands` (03-01: always the full list — 03-02 swaps in
    /// the fuzzy filter).
    filtered: Vec<usize>,
    commands: Vec<Command>,
    window_id: Option<WindowId>,
    /// Set when `close()` ran before the window was created (build-destroy
    /// pairing — the capture `torn_down_pending` shape generalized; consumed
    /// by the `on_created` callback path, not the broadcast bus event).
    pending_close: bool,
    /// Incremented per summon; guards stale runner completions (Pitfall 3).
    generation: u64,
    /// Geometry revision: incremented by every state-machine transition that
    /// changes the target window height (summon / set_input / set_executing
    /// success / finalize-Err). The frame loop compares this counter against
    /// its last-seen value — a deterministic trigger that catches transitions
    /// occurring OUTSIDE a frame (Enter→Executing arrives in a KeyboardInput
    /// event, finalize-Err hops in via UiThreadProxy), which the old in-frame
    /// prev/next snapshot comparison could never observe (WR-01 root cause).
    geometry_revision: u64,
    executing_id: Option<&'static str>,
    error: Option<String>,
    framebuffer: Option<tiny_skia::Pixmap>,
    textures: HashMap<egui::TextureId, egui::epaint::ImageData>,
    winit_state: Option<egui_winit::State>,
    /// True when the input widget should request focus on the next frame.
    focus_requested: bool,
    /// Tracked modifier-key state (GAP-6). winit 0.30.13's `KeyEvent` has NO
    /// modifiers field (source-verified — the field only landed in winit
    /// 0.31), so the Ctrl+P/N decision cannot read the Ctrl state from the
    /// KeyboardInput event itself. The state comes from the separate
    /// `WindowEvent::ModifiersChanged` event stream: the on_event_win closure
    /// writes it via `set_modifiers` (main thread only) and the key router
    /// reads it via `modifiers` (any thread).
    modifiers: winit::keyboard::ModifiersState,
    /// True after the palette window explicitly requested IME input
    /// (GAP-7). Set once in `ensure_winit_state` — the panel's only purpose
    /// is text input, so the first event enables the OS IME channel
    /// immediately instead of waiting for egui-winit's multi-frame
    /// focus→PlatformOutput.ime sequence.
    ime_allowed: bool,
}

/// Cloneable handle to the shared palette session.
#[derive(Clone)]
pub struct PaletteSession {
    state: Arc<std::sync::Mutex<SessionInner>>,
    /// egui::Context is `Send` but NOT `Sync` — held in its own mutex so the
    /// session itself stays `Send + Sync` (RESEARCH Anti-Patterns). Locked on
    /// the main thread only.
    egui_ctx: Arc<std::sync::Mutex<egui::Context>>,
}

impl PaletteSession {
    pub fn new() -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(SessionInner {
                state: PaletteState::Hidden,
                input: String::new(),
                selection: None,
                filtered: Vec::new(),
                commands: Vec::new(),
                window_id: None,
                pending_close: false,
                generation: 0,
                geometry_revision: 0,
                executing_id: None,
                error: None,
                framebuffer: None,
                textures: HashMap::new(),
                winit_state: None,
                focus_requested: false,
                modifiers: winit::keyboard::ModifiersState::empty(),
                ime_allowed: false,
            })),
            egui_ctx: Arc::new(std::sync::Mutex::new(egui::Context::default())),
        }
    }

    /// Summon the palette: snapshot the command list, reset input/selection,
    /// bump the generation counter, move to Idle. Returns the new generation.
    /// Also bumps the geometry revision (a fresh window gets fresh geometry —
    /// the frame loop must re-sync the height for the new command count).
    /// GAP-8 (03-09): also resets `ime_allowed=false` and `winit_state=None`
    /// so every fresh window re-runs `ensure_winit_state` — the explicit
    /// `window.set_ime_allowed(true)` hits the new winit Window and a fresh
    /// egui-winit State is built for it. egui-winit 0.30's `set_ime_allowed`
    /// debounce (vendored lib.rs:848-852) only fires on an `allow_ime` FLIP,
    /// so reusing the prior State (`allow_ime` stays true) never re-opens IME
    /// on the new window — winit macOS defaults to IME disabled per window.
    pub fn summon(&self, commands: Vec<Command>) -> u64 {
        let mut inner = self.state.lock().unwrap();
        inner.generation += 1;
        inner.geometry_revision += 1;
        // T-03-15: a fresh window starts with a fresh modifier state — a stale
        // Ctrl from a previous window must never make the new panel swallow
        // plain P/N input.
        inner.modifiers = winit::keyboard::ModifiersState::empty();
        // GAP-8 reset (03-09, REVIEW WR-01 fix): forces `ensure_winit_state`
        // to re-enter the `if !inner.ime_allowed` guard on the next event for
        // the new window → re-issue `window.set_ime_allowed(true)` on the new
        // winit Window and build a fresh egui-winit State for it. Without
        // these resets the second and later summon windows never re-receive
        // `set_ime_allowed(true)` (the debounce sees no `allow_ime` flip).
        inner.ime_allowed = false;
        inner.winit_state = None;
        inner.state = PaletteState::Idle;
        inner.input.clear();
        inner.selection = None;
        inner.error = None;
        inner.filtered = (0..commands.len()).collect();
        inner.commands = commands;
        inner.window_id = None;
        inner.pending_close = false;
        inner.executing_id = None;
        inner.focus_requested = true;
        inner.generation
    }

    /// Record the framework window id (harness/test path; production pairs via
    /// `on_window_created`).
    pub fn set_window_id(&self, id: WindowId) {
        self.state.lock().unwrap().window_id = Some(id);
    }

    /// Build-destroy pairing entry point for the palette's OWN window. Must be
    /// called only from the `WindowSpec.on_created` callback — never from a
    /// subscription to the broadcast `core/window-created` bus event (that
    /// event also fires for every OTHER module's windows, which would let a
    /// capture overlay overwrite the palette's window id or let a stray
    /// pending close destroy someone else's window — GAP-1, T-03-02).
    ///
    /// Returns `true` when a close arrived before the window was created
    /// (`pending_close`): the caller must destroy the incoming window
    /// immediately. Otherwise records the id as the live palette window and
    /// returns `false`.
    pub fn on_window_created(&self, id: WindowId) -> bool {
        let mut inner = self.state.lock().unwrap();
        if inner.pending_close {
            inner.pending_close = false;
            return true;
        }
        inner.window_id = Some(id);
        false
    }

    /// True while the palette is summonable-closed: a window exists, a create
    /// request is in flight (`pending_close`), or the session was summoned but
    /// the on_created pairing has not happened yet. The last case matters for
    /// re-entrancy: a toggle during the in-flight window must CLOSE (pairing
    /// up), never summon a second window (the Phase 2 re-entrancy lesson).
    pub fn has_live_window(&self) -> bool {
        let inner = self.state.lock().unwrap();
        inner.state != PaletteState::Hidden
            || inner.window_id.is_some()
            || inner.pending_close
    }

    /// WR-01: creation-failure reset — a summon whose window could not be
    /// created must leave the session summonable again. `summon()` moved
    /// the state to Idle with `window_id=None`; the failure never pairs an
    /// id, so `has_live_window()` would stay true forever (pending_close
    /// never clears). Reset to Hidden and drop the in-flight state; the
    /// next toggle summons fresh.
    pub fn on_create_failed(&self) {
        let mut inner = self.state.lock().unwrap();
        inner.state = PaletteState::Hidden;
        inner.window_id = None;
        inner.pending_close = false;
        inner.error = None;
        // geometry_revision intentionally untouched: no window exists to
        // sync, and the next summon bumps it anyway.
    }

    /// Close the palette: move to Hidden and return the window id to destroy
    /// (the caller enqueues `WindowRequest::Destroy`). If no window id was
    /// recorded yet but a summon is in flight, `pending_close` is set so the
    /// late on_created callback destroys the window immediately — the
    /// build-destroy pairing (capture `torn_down_pending` shape).
    pub fn close(&self) -> Option<WindowId> {
        let mut inner = self.state.lock().unwrap();
        let was_visible = inner.state != PaletteState::Hidden;
        inner.state = PaletteState::Hidden;
        if let Some(id) = inner.window_id.take() {
            inner.pending_close = false;
            Some(id)
        } else if was_visible {
            inner.pending_close = true;
            None
        } else {
            None
        }
    }

    /// Consume a pending close (on_created pairing path: if true, the incoming
    /// window must be destroyed immediately). Also asserted by the
    /// `five_summon_esc` check for residue detection.
    pub fn consume_pending_close(&self) -> bool {
        let mut inner = self.state.lock().unwrap();
        let was = inner.pending_close;
        inner.pending_close = false;
        was
    }

    pub fn state(&self) -> PaletteState {
        self.state.lock().unwrap().state
    }

    pub fn generation(&self) -> u64 {
        self.state.lock().unwrap().generation
    }

    /// Record the modifier-key state (GAP-6). The on_event_win closure feeds
    /// this from `WindowEvent::ModifiersChanged` — the ONLY source of modifier
    /// state (winit 0.30's `KeyEvent` has no modifiers field). Written only on
    /// the main thread; the key router reads it from any thread.
    pub fn set_modifiers(&self, m: winit::keyboard::ModifiersState) {
        self.state.lock().unwrap().modifiers = m;
    }

    /// The tracked modifier-key state (GAP-6) — read by `on_palette_key` to
    /// guard the Ctrl+P/N navigation arms. `ModifiersState` is `Copy`, so it
    /// is returned by value.
    pub fn modifiers(&self) -> winit::keyboard::ModifiersState {
        self.state.lock().unwrap().modifiers
    }

    /// Whether the palette window has explicitly requested IME input
    /// (GAP-7). Set once by `ensure_winit_state` (the first window event);
    /// asserted by the `ime_commit_updates_input` E2E probe through the real
    /// production closure.
    pub fn ime_allowed(&self) -> bool {
        self.state.lock().unwrap().ime_allowed
    }

    /// The geometry revision counter — the deterministic height-sync trigger
    /// (WR-01 fix). Incremented by every geometry-affecting state-machine
    /// transition (summon / set_input / set_executing success /
    /// finalize-Err); the frame loop compares it against its last-seen value
    /// and calls `sync_window_geometry` on any change.
    pub fn geometry_revision(&self) -> u64 {
        self.state.lock().unwrap().geometry_revision
    }

    pub fn input(&self) -> String {
        self.state.lock().unwrap().input.clone()
    }

    /// 03-01: raw text storage without filtering (03-02 replaces this with the
    /// filtering `set_input`).
    pub fn set_input_raw(&self, s: impl Into<String>) {
        self.state.lock().unwrap().input = s.into();
    }

    /// Set the input with filtering (PAL-03, the 03-02 entry point): truncate
    /// to `filter::MAX_QUERY_LEN` chars (Security V5), then transition —
    /// trim-empty → Idle (full list, no selection); no matches → Empty;
    /// matches → Filtering with the selection reset to index 0 (UI-SPEC:
    /// every input change resets the highlight).
    pub fn set_input(&self, s: &str) {
        let mut inner = self.state.lock().unwrap();
        // Geometry revision bump FIRST: the input change re-ranks the filtered
        // list and changes the target window height. Production only calls
        // this when the TextEdit actually changed (ui.rs `input_resp.changed()`
        // guard); headless direct calls are safe — the frame-loop sync is
        // deduped by the `last_height` gate.
        inner.geometry_revision += 1;
        let truncated: String = s.chars().take(filter::MAX_QUERY_LEN).collect();
        inner.input = truncated.clone();
        if truncated.trim().is_empty() {
            inner.state = PaletteState::Idle;
            inner.selection = None;
            inner.filtered = (0..inner.commands.len()).collect();
            return;
        }
        let matches = filter::filter_commands(&inner.commands, &truncated);
        if matches.is_empty() {
            inner.state = PaletteState::Empty;
            inner.selection = None;
            inner.filtered.clear();
        } else {
            inner.state = PaletteState::Filtering;
            inner.filtered = matches.iter().map(|m| m.cmd_index).collect();
            inner.selection = Some(0);
        }
    }

    /// Move the selection by `delta` in **filtered space** with wrap-around
    /// (PAL-04). Only Idle/Filtering respond; an empty list and every other
    /// state are no-ops. With no selection, `↓` (positive delta) selects
    /// index 0 and `↑` (negative) selects the last entry (UI-SPEC: first ↓ in
    /// Idle selects index 0).
    pub fn move_selection(&self, delta: i32) {
        let mut inner = self.state.lock().unwrap();
        if !matches!(inner.state, PaletteState::Idle | PaletteState::Filtering) {
            return;
        }
        let len = inner.filtered.len();
        if len == 0 {
            return;
        }
        inner.selection = Some(match inner.selection {
            None => {
                if delta >= 0 {
                    0
                } else {
                    len - 1
                }
            }
            Some(i) => (i as i32 + delta).rem_euclid(len as i32) as usize,
        });
    }

    /// The **command index** to execute on Enter: the selected filtered entry,
    /// else the first filtered entry when nothing is selected (SPEC req 5 —
    /// Enter with no highlight executes the first command). `None` outside
    /// Idle/Filtering (never executes from Empty/Executing/Error).
    ///
    /// The selected entry is a position in *filtered space* (move_selection
    /// wraps around it, ui.rs renders rows in filtered order) — it MUST be
    /// mapped through `filtered` to the commands() index. Using the selection
    /// directly would execute the wrong command whenever Filtering reorders
    /// the list (e.g. query "退出" → filtered=[1], selection 0 → must execute
    /// commands()[1], not commands()[0]).
    pub fn resolve_execution_target(&self) -> Option<usize> {
        let inner = self.state.lock().unwrap();
        match inner.state {
            PaletteState::Idle | PaletteState::Filtering => inner
                .selection
                .and_then(|s| inner.filtered.get(s).copied())
                .or_else(|| inner.filtered.first().copied()),
            _ => None,
        }
    }

    pub fn commands(&self) -> Vec<Command> {
        self.state.lock().unwrap().commands.clone()
    }

    /// The current selection index (None in Idle — no highlight, Enter runs
    /// the first command; UI-SPEC selection semantics).
    pub fn selection(&self) -> Option<usize> {
        self.state.lock().unwrap().selection
    }

    /// The filtered index list (03-01: always the full command list).
    pub fn filtered(&self) -> Vec<usize> {
        self.state.lock().unwrap().filtered.clone()
    }

    pub fn window_id(&self) -> Option<WindowId> {
        self.state.lock().unwrap().window_id
    }

    /// Allocate (or replace) the render framebuffer at summon (the initial
    /// allocation path). Runtime resizes go through `resize_framebuffer` —
    /// every height sync keeps the framebuffer matching the window's physical
    /// size (WR-02 fix).
    pub fn install_framebuffer(&self, w: u32, h: u32) {
        self.state.lock().unwrap().framebuffer = tiny_skia::Pixmap::new(w, h);
    }

    /// Resize (or allocate) the render framebuffer to `w`×`h` physical pixels
    /// — the runtime companion to `install_framebuffer` (WR-02 fix).
    ///
    /// WR-02 root cause: the framebuffer used to be allocated exactly once at
    /// summon, so after the window GREW (Executing adds the 32px status line:
    /// `112+48·n > 80+48·n`) the new region had nothing to draw. Every height
    /// sync calls this so the framebuffer always covers the window's physical
    /// size.
    ///
    /// Same-size calls keep the existing `Pixmap` instance (no per-frame
    /// allocation churn). A failed allocation warns and KEEPS the old buffer
    /// (never panics — the old buffer still displays safely, clipped by the
    /// tiny-skia blit).
    pub fn resize_framebuffer(&self, w: u32, h: u32) {
        let mut inner = self.state.lock().unwrap();
        let needs_resize = match &inner.framebuffer {
            None => true,
            Some(fb) => fb.width() != w || fb.height() != h,
        };
        if !needs_resize {
            return;
        }
        match tiny_skia::Pixmap::new(w, h) {
            Some(pixmap) => inner.framebuffer = Some(pixmap),
            None => log::warn!(
                "palette: framebuffer allocation failed for {w}x{h} — keeping the previous buffer"
            ),
        }
    }

    /// Lock the session and hand `f` the framebuffer (the on_draw blit path).
    pub fn with_framebuffer<R>(
        &self,
        f: impl FnOnce(&mut Option<tiny_skia::Pixmap>) -> R,
    ) -> R {
        let mut inner = self.state.lock().unwrap();
        f(&mut inner.framebuffer)
    }

    /// Merge an egui `TexturesDelta` (font atlas updates) into the texture table.
    ///
    /// GAP-2 secondary root cause (was: whole-table replace): epaint's
    /// `TextureAtlas::take_delta` (texture_atlas.rs) emits
    /// `ImageDelta::partial(pos, patch, ..)` whenever glyphs are rasterized
    /// incrementally into the atlas (no resize). Replacing the whole texture
    /// with the patch corrupted every already-rasterized glyph — UV sampling
    /// read garbage outside the patch. `Some(pos)` patches are now written in
    /// place at `pos`; `None` still replaces the whole texture (full delta).
    pub fn apply_textures(&self, delta: egui::TexturesDelta) {
        let mut inner = self.state.lock().unwrap();
        for (id, change) in delta.set {
            match change.pos {
                None => {
                    inner.textures.insert(id, change.image);
                }
                Some(pos) => match inner.textures.get_mut(&id) {
                    Some(existing) => patch_texture_image(existing, &change.image, pos),
                    // No texture at this id yet — defensive: store the patch as
                    // the whole image (deterministic, never panic).
                    None => {
                        log::warn!(
                            "palette: partial texture patch for unknown id {id:?} — inserting as full image"
                        );
                        inner.textures.insert(id, change.image);
                    }
                },
            }
        }
        for id in delta.free {
            inner.textures.remove(&id);
        }
    }

    /// Snapshot the texture table (the rasterizer reads it).
    pub fn textures(&self) -> HashMap<egui::TextureId, egui::epaint::ImageData> {
        self.state.lock().unwrap().textures.clone()
    }

    /// Lock the egui context (main thread only — zero contention).
    pub fn egui_ctx(&self) -> std::sync::MutexGuard<'_, egui::Context> {
        self.egui_ctx.lock().unwrap()
    }

    /// Lazily construct the egui-winit `State` for the given window (created
    /// on the first event — Pattern 2) and explicitly enable the OS IME
    /// channel on that same first call (GAP-7).
    ///
    /// GAP-7 root cause: egui-winit's own `set_ime_allowed` depends on a
    /// multi-frame sequence — "TextEdit gains focus → the NEXT frame's
    /// `PlatformOutput.ime` → handle_platform_output → set_ime_allowed"
    /// (egui-winit lib.rs:851). Under the real desktop first-frame focus race
    /// the OS candidate window never appears. The palette window's only
    /// purpose is text input, so this method requests IME input explicitly
    /// the first time the window ever receives an event — the timing
    /// dependency is gone. egui-winit's later focus-driven
    /// `set_ime_allowed(false/true)` calls are preserved (disabling IME while
    /// the Executing state disables input is correct).
    ///
    /// The winit call happens OUTSIDE the state lock (no lock held across a
    /// winit call), gated once by the `ime_allowed` flag.
    ///
    /// **Lock-order invariant (IN-02):** `state` → `egui_ctx` order here. The
    /// frame loop (`lib.rs` RedrawRequested arm) takes `egui_ctx` first then
    /// `state` via `ui::draw`. **Never call `ensure_winit_state` /
    /// `with_winit_state_mut` while holding `egui_ctx()`** — both orders are
    /// main-thread-only and never nest today; this comment documents the
    /// invariant so future edits cannot nest them into a deadlock.
    pub fn ensure_winit_state(&self, window: &Arc<winit::window::Window>) {
        let enable_ime = {
            let mut inner = self.state.lock().unwrap();
            if inner.winit_state.is_none() {
                let ctx = self.egui_ctx.lock().unwrap().clone();
                inner.winit_state = Some(egui_winit::State::new(
                    ctx,
                    egui::ViewportId::ROOT,
                    window.as_ref(),
                    None,
                    None,
                    None,
                ));
            }
            if !inner.ime_allowed {
                inner.ime_allowed = true;
                true
            } else {
                false
            }
        };
        if enable_ime {
            window.set_ime_allowed(true);
            log::debug!("palette: IME allowed on the palette window (GAP-7)");
        }
    }

    /// Lock the session and hand `f` exclusive access to the egui-winit state
    /// (main thread only; the on_event_win frame loop uses this).
    pub fn with_winit_state_mut<R>(
        &self,
        f: impl FnOnce(&mut Option<egui_winit::State>) -> R,
    ) -> R {
        let mut inner = self.state.lock().unwrap();
        f(&mut inner.winit_state)
    }

    /// Consume the focus request (the input widget calls this each frame).
    pub fn take_focus_request(&self) -> bool {
        let mut inner = self.state.lock().unwrap();
        let was = inner.focus_requested;
        inner.focus_requested = false;
        was
    }

    /// Enter Executing with the given command id. Only Idle/Filtering may
    /// execute, and only when `gen` matches the current generation (a stale
    /// execution attempt from a previous palette instance is a no-op) — the
    /// D-04 anti-reentrancy guard. Returns true when the transition happened.
    pub fn set_executing(&self, gen: u64, id: &'static str) -> bool {
        let mut inner = self.state.lock().unwrap();
        if !matches!(inner.state, PaletteState::Idle | PaletteState::Filtering) {
            return false;
        }
        if gen != inner.generation {
            return false;
        }
        // Successful transition only: the Executing status line adds 32px
        // (112+48·n > 80+48·n) — bump the geometry revision so the frame loop
        // grows the window. Rejected duplicate/stale calls return above and
        // never bump (no geometry change, no sync needed).
        inner.geometry_revision += 1;
        inner.state = PaletteState::Executing;
        inner.executing_id = Some(id);
        true
    }

    /// Runner completion. Guarded by `gen == generation && state == Executing`
    /// (Pitfall 3 — a stale completion must not touch a re-summoned window and
    /// must not write errors into a fresh panel).
    ///
    /// `Ok` → Hidden; returns the window id to destroy (`None` + pending_close
    /// when no id was recorded yet — the same pairing semantics as `close()`).
    /// `Err` → Error state with the formatted message; the window stays (D-05)
    /// and the executing id is kept (the error block names the command).
    pub fn finalize(&self, gen: u64, result: mybox_core::anyhow::Result<()>) -> Option<WindowId> {
        let mut inner = self.state.lock().unwrap();
        if gen != inner.generation || inner.state != PaletteState::Executing {
            return None; // stale completion or wrong state — no-op
        }
        match result {
            Ok(()) => {
                inner.state = PaletteState::Hidden;
                inner.executing_id = None;
                inner.error = None;
                if let Some(id) = inner.window_id.take() {
                    inner.pending_close = false;
                    Some(id)
                } else {
                    inner.pending_close = true;
                    None
                }
            }
            Err(e) => {
                inner.state = PaletteState::Error;
                inner.error = Some(format!("{e:#}"));
                // Error shrinks the window to the fixed 144 — bump the
                // revision so the frame loop syncs the shrink (WR-01: this
                // transition arrives via a UiThreadProxy hop, invisible to
                // in-frame snapshots). The Ok branch needs no bump: the
                // window is destroyed, nothing to resize.
                inner.geometry_revision += 1;
                None
            }
        }
    }

    /// The id of the command currently executing / that failed (None outside
    /// Executing/Error — the UI names the command in the status/error lines).
    pub fn executing_id(&self) -> Option<&'static str> {
        self.state.lock().unwrap().executing_id
    }

    /// The formatted error text (Some only in the Error state).
    pub fn error(&self) -> Option<String> {
        self.state.lock().unwrap().error.clone()
    }
}

impl Default for PaletteSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply an `ImageDelta` patch in place into an existing texture at `[x, y]`
/// (T-03-05 mitigation: bounds-checked, never panics, never corrupts the
/// atlas). Same-variant row copies preserve `existing.size`; an out-of-bounds
/// patch warns and skips (or clips) instead of writing out of bounds; a
/// variant-mismatched patch (Font↔Color) warns and degrades to a whole-image
/// replace (deterministic behavior).
fn patch_texture_image(
    existing: &mut egui::epaint::ImageData,
    patch: &egui::epaint::ImageData,
    pos: [usize; 2],
) {
    let [x, y] = pos;
    match (existing, patch) {
        (egui::epaint::ImageData::Font(dst), egui::epaint::ImageData::Font(src)) => {
            // FontImage: single-channel coverage (1 f32 per pixel).
            let (dst_w, dst_h) = (dst.size[0], dst.size[1]);
            let (src_w, src_h) = (src.size[0], src.size[1]);
            let rows = src_h.min(dst_h.saturating_sub(y));
            let cols = src_w.min(dst_w.saturating_sub(x));
            if rows == 0 || cols == 0 {
                log::warn!(
                    "palette: partial font patch at {pos:?} ({src_w}x{src_h}) \
                     lies outside the {dst_w}x{dst_h} atlas — skipping patch"
                );
                return;
            }
            if rows < src_h || cols < src_w {
                log::warn!(
                    "palette: partial font patch at {pos:?} clipped \
                     ({src_w}x{src_h} into {dst_w}x{dst_h} at {x},{y})"
                );
            }
            for row in 0..rows {
                let dst_off = (y + row) * dst_w + x;
                let src_off = row * src_w;
                dst.pixels[dst_off..dst_off + cols]
                    .copy_from_slice(&src.pixels[src_off..src_off + cols]);
            }
        }
        (egui::epaint::ImageData::Color(dst), egui::epaint::ImageData::Color(src)) => {
            // ColorImage: straight RGBA8 — same row copy with a 4-byte stride.
            // `ImageData::Color` holds `Arc<ColorImage>` — `make_mut` clones
            // the image only when the Arc is shared (the texture table owns
            // the only reference here).
            let dst = std::sync::Arc::make_mut(dst);
            let (dst_w, dst_h) = (dst.size[0], dst.size[1]);
            let (src_w, src_h) = (src.size[0], src.size[1]);
            let rows = src_h.min(dst_h.saturating_sub(y));
            let cols = src_w.min(dst_w.saturating_sub(x));
            if rows == 0 || cols == 0 {
                log::warn!(
                    "palette: partial color patch at {pos:?} ({src_w}x{src_h}) \
                     lies outside the {dst_w}x{dst_h} image — skipping patch"
                );
                return;
            }
            if rows < src_h || cols < src_w {
                log::warn!(
                    "palette: partial color patch at {pos:?} clipped \
                     ({src_w}x{src_h} into {dst_w}x{dst_h} at {x},{y})"
                );
            }
            for row in 0..rows {
                let dst_off = ((y + row) * dst_w + x) * 4;
                let src_off = row * src_w * 4;
                let len = cols * 4;
                dst.pixels[dst_off..dst_off + len]
                    .copy_from_slice(&src.pixels[src_off..src_off + len]);
            }
        }
        (dst_variant, _src_variant) => {
            log::warn!(
                "palette: partial texture patch variant mismatch — \
                 degrading to whole-image replace"
            );
            *dst_variant = patch.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_command(id: &'static str) -> Command {
        Command {
            id,
            name: format!("command {id}"),
            description: "test command".to_string(),
            keywords: vec![],
            hide_before_execute: false,
            runner: Arc::new(|| Box::pin(async { Ok(()) })),
        }
    }

    /// A command with a specific name/description/keywords for filter tests.
    fn named_command(id: &'static str, name: &str, keywords: &[&'static str]) -> Command {
        Command {
            id,
            name: name.to_string(),
            description: format!("description of {name}"),
            keywords: keywords.to_vec(),
            hide_before_execute: false,
            runner: Arc::new(|| Box::pin(async { Ok(()) })),
        }
    }

    #[test]
    fn summon_close_cycle_returns_window_id() {
        let s = PaletteSession::new();
        assert_eq!(s.state(), PaletteState::Hidden);
        let gen = s.summon(vec![sample_command("a"), sample_command("b")]);
        assert_eq!(gen, 1);
        assert_eq!(s.state(), PaletteState::Idle);
        s.set_window_id(7);
        assert!(s.has_live_window());
        let id = s.close();
        assert_eq!(id, Some(7), "close must return the window id to destroy");
        assert_eq!(s.state(), PaletteState::Hidden);
        assert!(!s.has_live_window());
        assert!(!s.consume_pending_close(), "no pending close after a paired close");
    }

    #[test]
    fn five_summon_close_cycles_no_residue() {
        // PAL-01 5x re-entrancy acceptance: consecutive summon/close cycles
        // leave no residual state (window id, pending close, input).
        let s = PaletteSession::new();
        for round in 1..=5u64 {
            let gen = s.summon(vec![sample_command("a")]);
            assert_eq!(gen, round, "generation must increment per summon");
            assert_eq!(s.state(), PaletteState::Idle);
            s.set_window_id(round);
            let id = s.close();
            assert_eq!(id, Some(round));
            assert_eq!(s.state(), PaletteState::Hidden);
            assert!(!s.has_live_window(), "round {round}: no residual window");
        }
        assert!(s.input().is_empty());
    }

    #[test]
    fn close_before_window_created_sets_pending_close() {
        let s = PaletteSession::new();
        s.summon(vec![sample_command("a")]);
        // No window-created yet — close must pair up via pending_close.
        let id = s.close();
        assert_eq!(id, None, "no window id recorded yet");
        assert!(s.consume_pending_close(), "pending close must be set");
        assert!(!s.consume_pending_close(), "consumed exactly once");
        assert!(!s.has_live_window(), "pairing consumed — nothing live");
    }

    #[test]
    fn create_failed_resets_session_and_unwedges() {
        // WR-01: a summon whose window could not be created must leave the
        // session summonable again (toggle no longer wedges).
        let s = PaletteSession::new();
        let gen1 = s.summon(vec![sample_command("a")]);
        assert_eq!(s.state(), PaletteState::Idle);
        assert!(s.has_live_window(), "Idle with no id yet counts as in-flight");
        s.on_create_failed();
        assert_eq!(s.state(), PaletteState::Hidden);
        assert!(!s.has_live_window(), "creation failure must unwedge the session");
        assert!(s.window_id().is_none());
        // The next summon succeeds (generation increments) — toggle recovers.
        let gen2 = s.summon(vec![sample_command("a")]);
        assert_eq!(gen2, gen1 + 1, "generation must increment on re-summon");
        assert_eq!(s.state(), PaletteState::Idle);
    }

    #[test]
    fn input_limit_truncates_paste() {
        // IN-05: the 64-char cap enforced at session.set_input (the headless
        // backstop; the TextEdit char_limit covers the interactive path).
        let s = PaletteSession::new();
        s.summon(vec![sample_command("a")]);
        s.set_input(&"a".repeat(100));
        assert_eq!(s.input().chars().count(), crate::filter::MAX_QUERY_LEN);
        assert_eq!(s.input(), "a".repeat(crate::filter::MAX_QUERY_LEN));
    }

    #[test]
    fn summon_snapshots_commands_and_resets_input() {
        let s = PaletteSession::new();
        let cmds = vec![sample_command("a"), sample_command("b"), sample_command("c")];
        s.summon(cmds.clone());
        assert_eq!(s.commands().len(), 3, "commands snapshot");
        assert_eq!(s.filtered(), vec![0, 1, 2], "full list at summon");
        assert!(s.input().is_empty());
        assert!(s.take_focus_request(), "focus requested on summon");
        assert!(!s.take_focus_request(), "focus request consumed once");
        // Input survives across close/summon boundaries? No — summon resets it.
        s.set_input_raw("jietu");
        assert_eq!(s.input(), "jietu");
        s.summon(cmds);
        assert!(s.input().is_empty(), "summon must reset input");
    }

    #[test]
    fn input_transitions_idle_filtering_empty() {
        let s = PaletteSession::new();
        s.summon(vec![
            named_command("capture.start", "开始截图", &["jietu"]),
            named_command("builtin.quit", "退出应用", &["quit"]),
        ]);
        assert_eq!(s.state(), PaletteState::Idle);

        // Matching input → Filtering with the ranked filtered list.
        s.set_input("截图");
        assert_eq!(s.state(), PaletteState::Filtering);
        assert_eq!(s.filtered(), vec![0], "截图 hits 开始截图 only");
        assert_eq!(s.selection(), Some(0), "selection resets to 0");

        // Non-matching input → Empty with no selection.
        s.set_input("zzzz");
        assert_eq!(s.state(), PaletteState::Empty);
        assert!(s.filtered().is_empty());
        assert_eq!(s.selection(), None);

        // Cleared (trim-empty) input → Idle with the full list restored.
        s.set_input("   ");
        assert_eq!(s.state(), PaletteState::Idle);
        assert_eq!(s.filtered(), vec![0, 1], "full list restored");
        assert_eq!(s.selection(), None, "Idle has no highlight");
    }

    #[test]
    fn selection_resets_to_zero_on_input_change() {
        let s = PaletteSession::new();
        s.summon(vec![
            named_command("a", "alpha one", &[]),
            named_command("b", "alpha two", &[]),
        ]);
        s.set_input("alpha");
        assert_eq!(s.selection(), Some(0));
        s.move_selection(1);
        assert_eq!(s.selection(), Some(1));
        // Any input change resets the highlight to index 0.
        s.set_input("alpha t");
        assert_eq!(s.selection(), Some(0));
    }

    #[test]
    fn move_selection_wraps_both_ends() {
        let s = PaletteSession::new();
        s.summon(vec![sample_command("a"), sample_command("b"), sample_command("c")]);
        // Idle: no selection yet; first ↓ selects index 0 (UI-SPEC).
        s.move_selection(1);
        assert_eq!(s.selection(), Some(0), "first ↓ selects index 0");
        s.move_selection(1);
        assert_eq!(s.selection(), Some(1));
        s.move_selection(1);
        assert_eq!(s.selection(), Some(2));
        s.move_selection(1);
        assert_eq!(s.selection(), Some(0), "↓ wraps around at the end");
        s.move_selection(-1);
        assert_eq!(s.selection(), Some(2), "↑ wraps around at the start");
        // Fresh summon: first ↑ selects the last entry.
        s.summon(vec![sample_command("a"), sample_command("b"), sample_command("c")]);
        s.move_selection(-1);
        assert_eq!(s.selection(), Some(2), "first ↑ selects the last entry");
    }

    #[test]
    fn resolve_target_maps_through_filtered_indices() {
        // The Filtering-reorder regression: selection lives in filtered space
        // and MUST map through `filtered` — executing selection directly would
        // run the wrong command. filtered == [3, 1] after querying "alpha"
        // (cmd 3 "alpha" scores above cmd 1 "xxalpha" — earlier match start).
        let s = PaletteSession::new();
        s.summon(vec![
            named_command("zero", "zero", &[]),
            named_command("xx", "xxalpha", &[]),
            named_command("beta", "beta", &[]),
            named_command("alpha", "alpha", &[]),
        ]);
        s.set_input("alpha");
        assert_eq!(s.filtered(), vec![3, 1], "name tier reorders the list");
        // Selection Some(0) (reset by set_input) → commands()[3].
        assert_eq!(s.resolve_execution_target(), Some(3));
        // Selection Some(1) → commands()[1] (NOT commands()[1] by position).
        s.move_selection(1);
        assert_eq!(s.selection(), Some(1));
        assert_eq!(
            s.resolve_execution_target(),
            Some(1),
            "selection must map through filtered"
        );
    }

    #[test]
    fn resolve_target_first_when_no_selection() {
        // Idle identity case: no highlight → Enter executes the first command
        // (SPEC req 5). filtered == [0, 1, 2] is the identity mapping, so the
        // mapping is transparent here — the behavior is the same either way.
        let s = PaletteSession::new();
        s.summon(vec![sample_command("a"), sample_command("b"), sample_command("c")]);
        assert_eq!(s.selection(), None);
        assert_eq!(s.resolve_execution_target(), Some(0), "no highlight → first command");
    }

    #[test]
    fn resolve_target_none_in_empty_and_executing() {
        let s = PaletteSession::new();
        s.summon(vec![sample_command("a")]);
        s.set_input("zzzz");
        assert_eq!(s.state(), PaletteState::Empty);
        assert_eq!(s.resolve_execution_target(), None, "Empty never executes");
        // Executing also resolves to nothing (re-entrancy).
        s.set_input("");
        assert!(s.set_executing(s.generation(), "a"), "Idle may execute");
        assert_eq!(s.resolve_execution_target(), None, "Executing never executes");
        s.finalize(s.generation(), Ok(()));
        // Hidden resolves to nothing either.
        assert_eq!(s.resolve_execution_target(), None);
    }

    #[test]
    fn set_executing_only_from_idle_filtering() {
        let s = PaletteSession::new();
        let gen = s.summon(vec![sample_command("a")]);
        assert!(s.set_executing(gen, "a"), "Idle → Executing");
        assert_eq!(s.state(), PaletteState::Executing);
        // Re-entrancy: a second set_executing while Executing is rejected (D-04).
        assert!(!s.set_executing(gen, "a"), "Executing must not re-execute");
        s.finalize(gen, Ok(()));
        // Hidden cannot execute either.
        assert!(!s.set_executing(gen, "a"), "Hidden must not execute");
        // A stale generation cannot execute (previous palette instance).
        let gen2 = s.summon(vec![sample_command("a")]);
        assert!(!s.set_executing(gen, "a"), "stale generation must not execute");
        assert_eq!(s.state(), PaletteState::Idle);
        assert!(s.set_executing(gen2, "a"), "current generation executes");
    }

    #[test]
    fn finalize_ok_closes_and_returns_window_id() {
        let s = PaletteSession::new();
        let gen = s.summon(vec![sample_command("a")]);
        s.set_executing(gen, "a");
        s.set_window_id(7);
        let id = s.finalize(gen, Ok(()));
        assert_eq!(id, Some(7), "finalize Ok returns the window id to destroy");
        assert_eq!(s.state(), PaletteState::Hidden);
        assert!(!s.has_live_window(), "no residual window after finalize Ok");
        assert!(!s.consume_pending_close(), "no pending close after a paired finalize");
    }

    #[test]
    fn finalize_err_sets_error_state() {
        let s = PaletteSession::new();
        let gen = s.summon(vec![sample_command("a")]);
        s.set_executing(gen, "a");
        s.set_window_id(7);
        let err = mybox_core::anyhow::anyhow!("screenshot permission denied");
        assert_eq!(s.finalize(gen, Err(err)), None, "error keeps the window open (D-05)");
        assert_eq!(s.state(), PaletteState::Error);
        assert_eq!(s.error().as_deref(), Some("screenshot permission denied"));
        assert_eq!(s.executing_id(), Some("a"), "error block names the command");
        assert_eq!(s.window_id(), Some(7), "the window stays alive for the error display");
    }

    #[test]
    fn finalize_stale_generation_is_noop() {
        // Pitfall 3: the old runner's completion must not destroy the
        // re-summoned window nor write errors into the fresh panel.
        let s = PaletteSession::new();
        let gen1 = s.summon(vec![sample_command("a")]);
        s.set_executing(gen1, "a");
        // The user closes mid-execution and re-summons (new generation).
        let gen2 = s.summon(vec![sample_command("a")]);
        assert_ne!(gen1, gen2);
        s.set_window_id(9);
        assert_eq!(s.finalize(gen1, Ok(())), None, "stale Ok is a no-op");
        assert_eq!(s.state(), PaletteState::Idle, "fresh panel untouched");
        assert_eq!(s.finalize(gen1, Err(mybox_core::anyhow::anyhow!("boom"))), None);
        assert_eq!(s.state(), PaletteState::Idle, "stale Err is a no-op");
        assert_eq!(s.window_id(), Some(9), "new window survives stale completions");
    }

    #[test]
    fn finalize_wrong_state_is_noop() {
        // A completion arriving outside Executing (e.g. the panel was closed
        // via ESC/hotkey while the runner continued) must not resurrect state.
        let s = PaletteSession::new();
        let gen = s.summon(vec![sample_command("a")]);
        s.set_executing(gen, "a");
        s.set_window_id(5);
        s.close(); // ESC/hotkey mid-execution — runner keeps going
        assert_eq!(s.state(), PaletteState::Hidden);
        assert_eq!(s.finalize(gen, Ok(())), None, "wrong state → no-op");
        assert_eq!(s.state(), PaletteState::Hidden);
        assert!(!s.has_live_window());
    }

    #[test]
    fn modifiers_tracking_roundtrip() {
        // GAP-6: the modifier state is tracked via the ModifiersChanged event
        // stream (winit 0.30 KeyEvent has no modifiers field). set / read /
        // clear must round-trip deterministically.
        let s = PaletteSession::new();
        assert_eq!(
            s.modifiers(),
            winit::keyboard::ModifiersState::empty(),
            "a fresh session tracks no modifiers"
        );
        s.set_modifiers(winit::keyboard::ModifiersState::CONTROL);
        assert_eq!(s.modifiers(), winit::keyboard::ModifiersState::CONTROL);
        assert!(s.modifiers().control_key(), "CONTROL must report control_key()");
        s.set_modifiers(winit::keyboard::ModifiersState::empty());
        assert_eq!(
            s.modifiers(),
            winit::keyboard::ModifiersState::empty(),
            "an explicit clear resets the tracked state"
        );
    }

    #[test]
    fn summon_resets_modifiers() {
        // T-03-15: a stale Ctrl state from a previous window must never make
        // a fresh panel swallow plain P/N input — summon resets the modifiers.
        let s = PaletteSession::new();
        s.summon(vec![sample_command("a")]);
        s.set_modifiers(winit::keyboard::ModifiersState::CONTROL);
        assert!(s.modifiers().control_key());
        s.summon(vec![sample_command("a")]);
        assert_eq!(
            s.modifiers(),
            winit::keyboard::ModifiersState::empty(),
            "summon must reset the modifier state"
        );
    }

    #[test]
    fn geometry_revision_bumps_on_geometry_affecting_transitions() {
        // The revision counter is the deterministic height-sync trigger
        // (WR-01): it must advance ONLY on the four geometry-affecting
        // transitions — summon, set_input, set_executing success,
        // finalize-Err — and stay put for everything else.
        let s = PaletteSession::new();
        assert_eq!(s.geometry_revision(), 0, "fresh session starts at zero");
        let gen = s.summon(vec![sample_command("a")]);
        assert_eq!(s.geometry_revision(), 1, "summon bumps the revision");
        s.set_input("a");
        assert_eq!(s.geometry_revision(), 2, "set_input bumps the revision");
        assert!(s.set_executing(gen, "a"));
        assert_eq!(s.geometry_revision(), 3, "set_executing success bumps the revision");
        assert_eq!(s.finalize(gen, Err(mybox_core::anyhow::anyhow!("x"))), None);
        assert_eq!(s.geometry_revision(), 4, "finalize Err bumps the revision");
        // Ok path: re-summon to reset the generation, execute, complete Ok —
        // the window is about to be destroyed, so no height sync (no bump).
        let gen2 = s.summon(vec![sample_command("a")]);
        assert!(s.set_executing(gen2, "a"));
        let revision_before_ok = s.geometry_revision();
        assert_eq!(s.finalize(gen2, Ok(())), None, "no window id recorded — pending close");
        assert_eq!(
            s.geometry_revision(),
            revision_before_ok,
            "finalize Ok does not bump the revision"
        );
        // Navigation and close are not geometry-affecting.
        s.summon(vec![sample_command("a"), sample_command("b")]);
        let revision_before_move = s.geometry_revision();
        s.move_selection(1);
        assert_eq!(
            s.geometry_revision(),
            revision_before_move,
            "move_selection does not bump the revision"
        );
        let revision_before_close = s.geometry_revision();
        let _ = s.close();
        assert_eq!(
            s.geometry_revision(),
            revision_before_close,
            "close does not bump the revision"
        );
    }

    #[test]
    fn set_executing_rejected_does_not_bump_revision() {
        let s = PaletteSession::new();
        let gen = s.summon(vec![sample_command("a")]);
        let revision = s.geometry_revision();
        assert!(s.set_executing(gen, "a"));
        assert_eq!(s.geometry_revision(), revision + 1, "successful transition bumps once");
        // Re-entrancy: a second set_executing while Executing is rejected (D-04)
        // and must not bump the revision (no geometry change).
        assert!(!s.set_executing(gen, "a"), "Executing must not re-execute");
        assert_eq!(
            s.geometry_revision(),
            revision + 1,
            "rejected duplicate set_executing does not bump"
        );
        // A stale generation is rejected without a bump too.
        let gen2 = s.summon(vec![sample_command("a")]);
        let revision2 = s.geometry_revision();
        assert!(!s.set_executing(gen, "a"), "stale generation must not execute");
        assert_eq!(
            s.geometry_revision(),
            revision2,
            "stale set_executing does not bump"
        );
        let _ = gen2;
    }

    #[test]
    fn resize_framebuffer_grows_shrinks_and_keeps_same_size() {
        let s = PaletteSession::new();
        s.install_framebuffer(100, 100);
        s.resize_framebuffer(200, 200);
        s.with_framebuffer(|fb| {
            let fb = fb.as_ref().expect("framebuffer installed");
            assert_eq!(fb.width(), 200, "grow to 200 wide");
            assert_eq!(fb.height(), 200, "grow to 200 tall");
        });
        s.resize_framebuffer(50, 50);
        s.with_framebuffer(|fb| {
            let fb = fb.as_ref().expect("framebuffer installed");
            assert_eq!(fb.width(), 50, "shrink to 50 wide");
            assert_eq!(fb.height(), 50, "shrink to 50 tall");
        });
        // Same size: the Pixmap instance must be preserved (zero allocation).
        let ptr_before = s.with_framebuffer(|fb| fb.as_ref().map(|p| p.data().as_ptr()));
        s.resize_framebuffer(50, 50);
        let ptr_after = s.with_framebuffer(|fb| fb.as_ref().map(|p| p.data().as_ptr()));
        assert_eq!(
            ptr_before, ptr_after,
            "same-size resize must keep the existing Pixmap instance"
        );
        // Allocating with no prior framebuffer installs a fresh one (the
        // defensive path — summon normally installs first).
        let s2 = PaletteSession::new();
        s2.resize_framebuffer(30, 40);
        s2.with_framebuffer(|fb| {
            let fb = fb.as_ref().expect("allocation on first call");
            assert_eq!((fb.width(), fb.height()), (30, 40));
        });
    }

    /// Build a full FontImage delta for `id`.
    fn full_font_delta(id: egui::TextureId, size: [usize; 2], value: f32) -> (egui::TextureId, egui::epaint::ImageDelta) {
        (
            id,
            egui::epaint::ImageDelta::full(
                egui::epaint::ImageData::Font(egui::epaint::FontImage {
                    size,
                    pixels: vec![value; size[0] * size[1]],
                }),
                egui::TextureOptions::LINEAR,
            ),
        )
    }

    #[test]
    fn apply_textures_patches_partial_font_delta_in_place() {
        // GAP-2 secondary root cause regression (RED before the fix): a
        // partial atlas patch must be written in place at `pos` — the old
        // whole-table replace shrank the stored texture to the patch,
        // corrupting every already-rasterized glyph. Seed a 4x4 atlas, patch
        // a 2x2 region at [1,1], and assert untouched texels survive.
        let s = PaletteSession::new();
        s.apply_textures(egui::TexturesDelta {
            set: vec![full_font_delta(egui::TextureId::Managed(0), [4, 4], 0.25)],
            free: vec![],
        });
        s.apply_textures(egui::TexturesDelta {
            set: vec![(
                egui::TextureId::Managed(0),
                egui::epaint::ImageDelta::partial(
                    [1, 1],
                    egui::epaint::ImageData::Font(egui::epaint::FontImage {
                        size: [2, 2],
                        pixels: vec![0.9; 4],
                    }),
                    egui::TextureOptions::LINEAR,
                ),
            )],
            free: vec![],
        });

        let textures = s.textures();
        let img = textures
            .get(&egui::TextureId::Managed(0))
            .expect("seeded texture must exist");
        assert!(matches!(img, egui::epaint::ImageData::Font(_)), "Font image");
        let egui::epaint::ImageData::Font(f) = img else { unreachable!() };
        assert_eq!(f.size, [4, 4], "patch must preserve the atlas size");
        let px = |x: usize, y: usize| f.pixels[y * 4 + x];
        assert_eq!(px(1, 1), 0.9, "patch top-left lands at [1,1]");
        assert_eq!(px(2, 2), 0.9, "patch bottom-right lands at [2,2]");
        assert_eq!(px(0, 0), 0.25, "texel outside the patch untouched");
        assert_eq!(px(3, 3), 0.25, "texel outside the patch untouched");
        assert_eq!(px(0, 1), 0.25, "same-row texel before the patch untouched");
        assert_eq!(px(1, 0), 0.25, "same-column texel above the patch untouched");
    }

    #[test]
    fn apply_textures_full_delta_replaces_whole_image() {
        // A full delta (atlas resize) still replaces the whole texture —
        // size and content become exactly the new image.
        let s = PaletteSession::new();
        s.apply_textures(egui::TexturesDelta {
            set: vec![full_font_delta(egui::TextureId::Managed(0), [4, 4], 0.25)],
            free: vec![],
        });
        s.apply_textures(egui::TexturesDelta {
            set: vec![full_font_delta(egui::TextureId::Managed(0), [2, 2], 0.5)],
            free: vec![],
        });

        let textures = s.textures();
        let img = textures.get(&egui::TextureId::Managed(0)).expect("texture");
        assert!(matches!(img, egui::epaint::ImageData::Font(_)), "Font image");
        let egui::epaint::ImageData::Font(f) = img else { unreachable!() };
        assert_eq!(f.size, [2, 2], "full delta replaces the whole image");
        assert!(f.pixels.iter().all(|&p| p == 0.5), "content is the new image");
    }

    #[test]
    fn apply_textures_partial_out_of_bounds_patch_clips_and_skips() {
        // T-03-05: a patch must never write out of bounds. A partially
        // out-of-bounds patch is clipped (the in-bounds part lands); a wholly
        // out-of-bounds patch is skipped — the atlas stays intact either way.
        let s = PaletteSession::new();
        s.apply_textures(egui::TexturesDelta {
            set: vec![full_font_delta(egui::TextureId::Managed(0), [4, 4], 0.25)],
            free: vec![],
        });
        // Partially out of bounds: 4x4 patch at [2,2] → only [2..4)x[2..4) fits.
        s.apply_textures(egui::TexturesDelta {
            set: vec![(
                egui::TextureId::Managed(0),
                egui::epaint::ImageDelta::partial(
                    [2, 2],
                    egui::epaint::ImageData::Font(egui::epaint::FontImage {
                        size: [4, 4],
                        pixels: vec![0.9; 16],
                    }),
                    egui::TextureOptions::LINEAR,
                ),
            )],
            free: vec![],
        });
        let textures = s.textures();
        let egui::epaint::ImageData::Font(f) =
            textures.get(&egui::TextureId::Managed(0)).expect("texture")
        else {
            panic!("Font image expected")
        };
        assert_eq!(f.size, [4, 4], "size unchanged after a clipped patch");
        let px = |x: usize, y: usize| f.pixels[y * 4 + x];
        assert_eq!(px(2, 2), 0.9, "in-bounds part of the patch lands");
        assert_eq!(px(3, 3), 0.9, "in-bounds corner lands");
        assert_eq!(px(1, 1), 0.25, "out-of-patch texel untouched");
        assert_eq!(px(0, 0), 0.25, "far texel untouched");

        // Wholly out of bounds: patch at [5,5] sized 4x4 → nothing fits.
        s.apply_textures(egui::TexturesDelta {
            set: vec![(
                egui::TextureId::Managed(0),
                egui::epaint::ImageDelta::partial(
                    [5, 5],
                    egui::epaint::ImageData::Font(egui::epaint::FontImage {
                        size: [4, 4],
                        pixels: vec![0.7; 16],
                    }),
                    egui::TextureOptions::LINEAR,
                ),
            )],
            free: vec![],
        });
        let textures = s.textures();
        let egui::epaint::ImageData::Font(f) =
            textures.get(&egui::TextureId::Managed(0)).expect("texture")
        else {
            panic!("Font image expected")
        };
        assert_eq!(f.pixels[3 * 4 + 3], 0.9, "wholly OOB patch skipped — texel intact");
        assert_eq!(f.size, [4, 4], "size intact after a skipped patch");
    }
}
