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
    executing_id: Option<&'static str>,
    error: Option<String>,
    framebuffer: Option<tiny_skia::Pixmap>,
    textures: HashMap<egui::TextureId, egui::epaint::ImageData>,
    winit_state: Option<egui_winit::State>,
    /// True when the input widget should request focus on the next frame.
    focus_requested: bool,
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
                executing_id: None,
                error: None,
                framebuffer: None,
                textures: HashMap::new(),
                winit_state: None,
                focus_requested: false,
            })),
            egui_ctx: Arc::new(std::sync::Mutex::new(egui::Context::default())),
        }
    }

    /// Summon the palette: snapshot the command list, reset input/selection,
    /// bump the generation counter, move to Idle. Returns the new generation.
    pub fn summon(&self, commands: Vec<Command>) -> u64 {
        let mut inner = self.state.lock().unwrap();
        inner.generation += 1;
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

    /// Allocate (or replace) the render framebuffer.
    pub fn install_framebuffer(&self, w: u32, h: u32) {
        self.state.lock().unwrap().framebuffer = tiny_skia::Pixmap::new(w, h);
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
    pub fn apply_textures(&self, delta: egui::TexturesDelta) {
        let mut inner = self.state.lock().unwrap();
        for (id, change) in delta.set {
            inner.textures.insert(id, change.image);
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
    /// on the first event — Pattern 2).
    pub fn ensure_winit_state(&self, window: &Arc<winit::window::Window>) {
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
}
