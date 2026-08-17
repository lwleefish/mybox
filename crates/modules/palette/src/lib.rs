//! Command palette module (id `"palette"`) — Phase 3.
//!
//! Global hotkey (`toggle_palette`, default `Cmd+Shift+Space`) → a Floating
//! window centered on the active monitor → egui renders the command list
//! (render closures land in Task 4 of plan 03-01). Lifecycle is build-on-
//! summon / destroy-on-close (D-06) with per-window `WindowSpec.on_created`
//! pairing so no orphan windows survive a fast toggle (the Phase 2
//! re-entrancy lesson generalized). The pairing is guaranteed by the
//! on_created callback — the palette never subscribes to the broadcast
//! `core/window-created` event, so other modules' windows can neither
//! overwrite the palette's window id nor consume its pending_close (GAP-1).

pub mod execute;
pub mod filter;
pub mod fonts;
pub mod position;
pub mod raster;
pub mod session;
pub mod ui;

use std::sync::{Arc, OnceLock};

use mybox_core::anyhow;
use mybox_core::command::CommandRegistry;
use mybox_core::event::{EventFilter, EventPayload, FrameworkEvent};
use mybox_core::log;
use mybox_core::module::Module;
use mybox_core::tiny_skia;
use mybox_core::toml;
use mybox_core::window::{WindowKind, WindowManagerHandle, WindowSpec};
use mybox_core::winit;
use mybox_core::{ConfigCenter, ModuleContext, UiThreadProxy};

use position::PanelGeometry;
use session::{PaletteSession, PaletteState};

/// Command palette module: hotkey toggle + build-destroy window lifecycle.
pub struct PaletteModule {
    session: Arc<PaletteSession>,
    /// Injected at `init` — Task 4's render closures and 03-02's execute path
    /// consume it via `ui.get()`; the injection wiring is completed here so
    /// 03-02 never touches the PaletteModule fields again.
    ui: Arc<OnceLock<UiThreadProxy>>,
}

impl PaletteModule {
    pub fn new() -> Self {
        let session = Arc::new(PaletteSession::new());
        // CJK fonts must be installed before the first frame (Pitfall 5).
        // Failure is warn-and-continue: ASCII fallback.
        if let Err(e) = fonts::install_cjk_fonts(&session.egui_ctx()) {
            log::warn!("palette: CJK font install failed ({e:#}) — ASCII fallback");
        }
        // Dark fixed theme + card overrides, once, before the first frame.
        ui::configure_egui_ctx(&session.egui_ctx());
        Self {
            session,
            ui: Arc::new(OnceLock::new()),
        }
    }
}

impl Module for PaletteModule {
    fn id(&self) -> &'static str {
        "palette"
    }

    fn name(&self) -> &str {
        "命令面板"
    }

    fn default_config(&self) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert(
            "hotkey".to_string(),
            toml::Value::String(default_hotkey()),
        );
        table
    }

    fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()> {
        // Inject the main-thread proxy for the Task 4 / 03-02 closures.
        let _ = self.ui.set(ctx.ui().clone());

        // Hotkey toggle: summon or close (D-06 build-destroy + re-entrancy).
        // The handler runs on the bus worker thread; window operations are
        // enqueued through the shared handle (W2).
        let session = Arc::clone(&self.session);
        let windows = ctx.windows().clone();
        let commands = ctx.commands().clone();
        let ui_proxy = Arc::clone(&self.ui);
        ctx.on(
            EventFilter::kind("core", "hotkey.triggered"),
            Box::new(move |e| {
                if let EventPayload::Framework(FrameworkEvent::HotkeyTriggered { action, .. }) =
                    &e.payload
                {
                    if action == "toggle_palette" {
                        log::info!("palette: hotkey 'toggle_palette' triggered");
                        toggle_palette(&session, &windows, &commands, &ui_proxy);
                    }
                }
            }),
        );

        // (GAP-1, T-03-02) No `core/window-created` subscription here: the
        // build-destroy pairing runs through the per-window
        // `WindowSpec.on_created` callback (see `build_window_spec`), so only
        // the palette's own window ever touches this session. The broadcast
        // event still exists for the capture module and must never be
        // consumed by the palette.

        // Deferred hotkey registration (capture template, CAP-01 shape): module
        // init runs inside AppBuilder::build BEFORE hotkeys.init(), so a
        // direct register_str would fail — ctx.ui().run stashes the closure
        // until the loop is live.
        let hotkeys = ctx.hotkeys().clone();
        let hotkey_str = hotkey_from_config(ctx.config());
        ctx.ui().run(Box::new(move || {
            if let Err(e) = hotkeys.register_str("toggle_palette", &hotkey_str) {
                log::warn!("failed to register toggle_palette hotkey: {e}");
            }
        }));

        Ok(())
    }
}

/// The platform default palette hotkey (config `[palette].hotkey` overrides).
fn default_hotkey() -> String {
    #[cfg(target_os = "windows")]
    {
        "Ctrl+Shift+Space".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "Cmd+Shift+Space".to_string()
    }
}

/// Read `[palette].hotkey`, defaulting when absent or not a string
/// (capture's `hotkey_from_config` shape).
fn hotkey_from_config(config: &ConfigCenter) -> String {
    config
        .get("palette", "hotkey")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(default_hotkey)
}

/// Toggle: visible → close; hidden → summon (D-06 + re-entrancy-safe).
fn toggle_palette(
    session: &Arc<PaletteSession>,
    windows: &Arc<WindowManagerHandle>,
    commands: &Arc<CommandRegistry>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
) {
    if session.has_live_window() {
        close_palette(session, windows);
    } else if let Err(e) = summon_palette(session, windows, commands, ui_proxy) {
        log::warn!("palette: summon failed: {e:#}");
    }
}

/// Summon: compute active-monitor geometry → snapshot the command list →
/// allocate the framebuffer → enqueue Create. `pub` so 03-02's
/// `palette_checks` harness can reuse the production path (fake commands are
/// registered in a `CommandRegistry` by the harness).
pub fn summon_palette(
    session: &Arc<PaletteSession>,
    windows: &Arc<WindowManagerHandle>,
    commands: &Arc<CommandRegistry>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
) -> anyhow::Result<()> {
    let all = commands.all();
    // UI-SPEC geometry table: height adapts to the visible row count.
    // WR-02 (03-09): `all.len().max(1)` unifies with the frame-loop rule in
    // `sync_window_geometry` (which uses `session.filtered().len().max(1)`)
    // — the zero-command case is unreachable in production (≥4 builtins
    // always exist) but the two call sites now agree on 80 instead of
    // diverging into 80 vs 128.
    let height = ui::window_height(PaletteState::Idle, all.len().max(1));
    let geometry = position::summon_geometry((ui::PANEL_WIDTH, height))?;
    session.summon(all);
    session.install_framebuffer(geometry.inner_size.0, geometry.inner_size.1);
    windows.create(build_window_spec(session, windows, ui_proxy, geometry));
    Ok(())
}

/// Build the Floating window spec with the full render chain:
/// `on_event_win` runs the egui-winit frame loop (event translation → egui run
/// → tessellate → `raster::paint` into the session framebuffer), intercepts
/// the panel keys (↑/↓/Enter/ESC/Error-any-key) BEFORE the egui-winit
/// translation, and syncs the window height on geometry-revision changes
/// (position is never touched — GAP-3); `on_draw`
/// blits the framebuffer into the core Pixmap before `present()`; `on_created`
/// pairs the window id with the session on the main thread (GAP-1 fix — the
/// build-destroy pairing runs here instead of the broadcast `core/window-created`
/// bus event, so only the palette's own window can touch the session: a
/// pending close destroys the late window immediately, otherwise the id is
/// recorded). `pub` so 03-02's `palette_checks` harness can reuse it.
///
/// The closures capture the injected `ui_proxy` (an `Arc<OnceLock<UiThreadProxy>>`):
/// the Enter arm reads it via `ui_proxy.get()` — unset (headless) skips
/// execution.
pub fn build_window_spec(
    session: &Arc<PaletteSession>,
    windows: &Arc<WindowManagerHandle>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
    geometry: PanelGeometry,
) -> WindowSpec {
    let session = Arc::clone(session);
    let windows = Arc::clone(windows);
    let ui_proxy = Arc::clone(ui_proxy);
    // One clone per closure (on_event_win and on_draw below).
    let session_draw = Arc::clone(&session);
    // GAP-1 pairing closure: session/windows clones for `on_created`.
    let created_session = Arc::clone(&session);
    let created_windows = Arc::clone(&windows);
    // Last physical height applied to the window (the sync gate).
    let last_height = Arc::new(std::sync::Mutex::new(geometry.inner_size.1));
    // Last geometry revision consumed by the frame loop (WR-01 fix). Starts
    // at 0: the first frame's revision (≥1 after summon) always triggers one
    // sync, which the `last_height` gate dedupes into a no-op.
    let last_revision = Arc::new(std::sync::Mutex::new(0u64));

    let on_event_win: Option<
        Box<dyn Fn(&Arc<winit::window::Window>, &winit::event::WindowEvent) + Send + Sync>,
    > = Some(Box::new(move |window, event| {
        // RESEARCH Architecture Diagram left column, node by node. All
        // egui-winit calls happen here on the main thread only
        // (Anti-Patterns: never touch the winit State off the main thread).
        use winit::event::{ElementState, KeyEvent, WindowEvent};

        session.ensure_winit_state(window);

        // GAP-6 modifier tracking: winit 0.30's KeyEvent has no modifiers
        // field, so the Ctrl+P/N decision reads the state captured from this
        // separate event stream (the variant carries `event::Modifiers`, whose
        // `state()` yields the `ModifiersState` we store). NO early return —
        // the event continues to the egui-winit translation below (it consumes
        // ModifiersChanged too).
        if let WindowEvent::ModifiersChanged(m) = event {
            session.set_modifiers(m.state());
        }

        // 0. Panel key routing — intercepted BEFORE the egui-winit translation
        // (deterministic ownership: egui may consume TextEdit arrow keys, but
        // ↑/↓/Enter/ESC belong to the panel). Consumed events never reach egui.
        if let WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    logical_key,
                    state: ElementState::Pressed,
                    repeat: false,
                    ..
                },
            ..
        } = event
        {
            if on_palette_key(
                &session,
                &windows,
                &ui_proxy,
                logical_key,
                session.modifiers(),
            ) {
                return;
            }
        }

        // 1. Translate the remaining winit events into egui input.
        let resp = session.with_winit_state_mut(|state| {
            state
                .as_mut()
                .expect("ensure_winit_state ran")
                .on_window_event(window, event)
        });
        // Pitfall 8: ControlFlow::Wait does not redraw on its own — request a
        // redraw whenever egui asks for one.
        if resp.repaint {
            repaint(&session, &windows);
        }

        // 2. Frame loop: run egui, rasterize into the session framebuffer,
        // then sync the window geometry on a geometry-revision change.
        if let WindowEvent::RedrawRequested = event {
            let raw = session.with_winit_state_mut(|state| {
                state
                    .as_mut()
                    .expect("ensure_winit_state ran")
                    .take_egui_input(window)
            });
            let egui_ctx = session.egui_ctx();
            // 03-06 (GAP-5): the click-execute chain — ui::draw threads
            // windows/ui_proxy into the command rows, where a row click routes
            // through execute::execute with the same semantics as Enter.
            let full_output = egui_ctx.run(raw, |ctx| ui::draw(ctx, &session, &windows, &ui_proxy));

            // Gap 2 (03-10, UAT test 11): 鼠标点击路径的 execute()（含 hide_before_execute
            // 的 Destroy 入队）发生在本帧 egui_ctx.run 闭包内（ui.rs draw_command_row
            // resp.clicked() → execute → session.close() → Hidden）。Destroy 只在
            // about_to_wait 排出（本帧剩余 paint+present 阻塞主线程 2-10ms+），与
            // capture 链屏幕读取（3-20ms）竞态重叠——面板仍可见时被拍进截图。此处
            // 同步隐藏窗口（macOS orderOut 即刻生效，winit 0.30.13 window_delegate
            // is_visible 确定性 Some）并跳过本帧剩余全部工作——不 paint、不 present、
            // 不 request_redraw（下方几何同步块内的 request_redraw 同样被跳过）。Enter
            // 路径 on_palette_key 返回 true 时上方已早退、不跑帧循环；此处 Hidden 态只
            // 可能来自帧内执行（点击路径）或关闭后残留帧——两者都该立即隐藏。
            if session.state() == PaletteState::Hidden {
                window.set_visible(false);
                return;
            }

            session.apply_textures(full_output.textures_delta);
            let primitives = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
            // Snapshot the texture table FIRST: `with_framebuffer` holds the
            // session state lock for the duration of the closure, and
            // `textures()` relocks the same mutex — calling it inside would
            // deadlock the first frame (found by palette_checks e2e).
            let textures = session.textures();
            session.with_framebuffer(|framebuffer| {
                if let Some(framebuffer) = framebuffer {
                    // Each frame repaints the full card, but the card only
                    // covers the CURRENT egui screen rect — after a geometry
                    // change (Filtering→Empty shrinks the window) rows outside
                    // the new rect would otherwise keep stale glyphs from the
                    // previous layout. Clear to the card background so the
                    // framebuffer always represents exactly this frame (the
                    // opaque #202020 full-bleed window contract, 03-PATTERNS).
                    framebuffer.fill(tiny_skia::Color::from_rgba8(0x20, 0x20, 0x20, 0xFF));
                    raster::paint(
                        framebuffer,
                        &primitives,
                        &textures,
                        full_output.pixels_per_point,
                    );
                }
            });
            session.with_winit_state_mut(|state| {
                state
                    .as_mut()
                    .expect("ensure_winit_state ran")
                    .handle_platform_output(window, full_output.platform_output);
            });
            // WR-01 fix: the sync trigger is the session's geometry revision
            // counter, NOT an in-frame prev/next snapshot comparison. Enter→
            // Executing happens in a KeyboardInput event and finalize-Err
            // hops in via UiThreadProxy — both OUTSIDE a frame, so a snapshot
            // taken at the next frame always saw prev==current and the sync
            // never fired. The counter is advanced by the state-machine
            // methods themselves and deterministically captures every
            // geometry-affecting transition.
            if session.geometry_revision() != *last_revision.lock().unwrap() {
                *last_revision.lock().unwrap() = session.geometry_revision();
                sync_window_geometry(window, &session, &last_height);
                // Pitfall 8: a geometry change must repaint (ControlFlow::Wait).
                window.request_redraw();
            }
        }
    }));

    let on_draw: Option<Box<dyn Fn(&mut tiny_skia::PixmapMut, u32, u32) + Send + Sync>> =
        Some(Box::new(move |pixmap, _w, _h| {
            // Single-line blit of the palette framebuffer; handle_redraw calls
            // this before present() (the Phase 2 chain).
            session_draw.with_framebuffer(|framebuffer| {
                if let Some(framebuffer) = framebuffer {
                    pixmap.draw_pixmap(
                        0,
                        0,
                        framebuffer.as_ref(),
                        &tiny_skia::PixmapPaint::default(),
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
            });
        }));

    WindowSpec {
        kind: WindowKind::Floating,
        title: "mybox-palette".to_string(),
        inner_size: Some(geometry.inner_size),
        position: Some(geometry.position),
        on_event_win,
        on_draw,
        // GAP-1: the per-window pairing callback — App::create_window invokes
        // it on the main thread after registering the window. A pending close
        // destroys the late window immediately; otherwise the id is recorded.
        on_created: Some(Box::new(move |id| {
            if created_session.on_window_created(id) {
                log::debug!("palette: late window {id} destroyed (close arrived first)");
                created_windows.destroy(id);
            }
        })),
        ..Default::default()
    }
}

/// Enqueue a redraw for the live palette window (no-op when the window id is
/// unknown yet).
fn repaint(session: &PaletteSession, windows: &Arc<WindowManagerHandle>) {
    if let Some(id) = session.window_id() {
        windows.redraw(id);
    }
}

/// Panel key router (PAL-04/PAL-05/D-05) — extracted from the `on_event_win`
/// closure so the `palette_checks` harness can drive keys without synthesizing
/// winit `KeyboardInput` events (winit 0.30's `KeyEvent.platform_specific`
/// field is `pub(crate)` — the struct is not constructible outside winit; the
/// plan's DeviceId::dummy() fallback cannot work around that).
///
/// GAP-6: winit 0.30.13's `KeyEvent` has no modifiers field (source-verified —
/// it only landed in winit 0.31), so the Ctrl state for the Ctrl+P/N arms
/// cannot come from the KeyboardInput event itself. The caller tracks it from
/// the separate `WindowEvent::ModifiersChanged` event stream
/// (`session.set_modifiers`) and passes it here — E2E probes and production
/// share this same router (KeyEvent is not externally constructible,
/// 03-02 deviation #2).
///
/// Returns true when the panel consumed the key (egui never sees it).
/// The Error-state arm sits FIRST — match arms try in order, and a failing
/// guard falls through — otherwise ↑/↓/Enter in Error state would hit the
/// navigation arms and get swallowed by their state guards, violating
/// "any key closes" (D-05).
pub fn on_palette_key(
    session: &Arc<PaletteSession>,
    windows: &Arc<WindowManagerHandle>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
    logical_key: &winit::keyboard::Key,
    modifiers: winit::keyboard::ModifiersState,
) -> bool {
    use winit::keyboard::{Key, NamedKey};
    match logical_key {
        _ if session.state() == PaletteState::Error => {
            close_palette(session, windows);
            true
        }
        Key::Named(NamedKey::Escape) => {
            // PAL-05: close without executing — Idle/Filtering/Empty. Executing
            // ignores ESC (only the global hotkey toggle may close mid-run; the
            // runner continues, finalize is generation-guarded).
            if session.state() != PaletteState::Executing {
                close_palette(session, windows);
                true
            } else {
                false
            }
        }
        // GAP-6: Ctrl+P / Ctrl+N are equivalent to ↑ / ↓ (wrap-around in
        // filtered space). winit 0.30 delivers letters ALWAYS as
        // `Key::Character` — `NamedKey` (keyboard.rs:755) has no letter
        // variants, so no KeyP/KeyN arm exists in this winit version (that
        // mapping is a 0.31 concept). Without Ctrl the guards fail and plain
        // P/N fall through to `_ => false` — the event passes on to egui-winit
        // and the character enters the TextEdit normally (filter semantics
        // unchanged).
        Key::Character(s) if modifiers.control_key() && s.eq_ignore_ascii_case("p") => {
            session.move_selection(-1);
            repaint(session, windows);
            true
        }
        Key::Character(s) if modifiers.control_key() && s.eq_ignore_ascii_case("n") => {
            session.move_selection(1);
            repaint(session, windows);
            true
        }
        Key::Named(NamedKey::ArrowDown) => {
            session.move_selection(1);
            repaint(session, windows);
            true
        }
        Key::Named(NamedKey::ArrowUp) => {
            session.move_selection(-1);
            repaint(session, windows);
            true
        }
        Key::Named(NamedKey::Enter) => {
            // resolve_execution_target already maps the selection through
            // `filtered` — the returned value is the commands() index of the
            // highlighted command (or the first entry when nothing is
            // selected, SPEC req 5). An unset proxy (headless) skips execution.
            if let Some(idx) = session.resolve_execution_target() {
                if let Some(cmd) = session.commands().get(idx).cloned() {
                    if let Some(ui_proxy) = ui_proxy.get() {
                        execute::execute(session, ui_proxy, windows, cmd);
                    }
                }
            }
            repaint(session, windows);
            true
        }
        _ => false,
    }
}

/// Resize the window to the state-dependent height (UI-SPEC geometry table,
/// physical space). **Only the size ever changes — never the position.**
///
/// The window position was decided by `position::summon_geometry` at summon
/// time and is held until the window is destroyed. GAP-3 root cause: the old
/// version re-centered the window on its monitor after every height change —
/// a filter shrink then pushed the top edge DOWN (re-centering moved the
/// smaller window lower), which is the observed "panel falls" drift. The
/// `last_height` gate ensures the same physical height is requested only once.
///
/// WR-04 (03-09): a Hidden-state window early-returns at the top of this
/// function. The capture.start click-execute path bumps `geometry_revision`
/// on the same frame and the window is about to be Destroyed — feeding
/// `window_height(Hidden, ..)` (which is `0` → `max(1)` `1`) into
/// `request_inner_size` and `resize_framebuffer` does a 1px size request +
/// 1px framebuffer reallocation per screenshot. The early return is placed
/// BEFORE the `last_height` lock so the gate is NOT poisoned with a 1px
/// value — the next summon's first Idle sync is not short-circuited by a
/// stale `*last == physical_h` 1px match. After Task 1's summon reset, the
/// next `summon_palette` calls `install_framebuffer` on a fresh buffer
/// anyway — Hidden leaving `last_height` untouched keeps the gate aligned
/// with the framebuffer lifecycle.
fn sync_window_geometry(
    window: &winit::window::Window,
    session: &PaletteSession,
    last_height: &std::sync::Mutex<u32>,
) {
    // WR-04 (03-09): Hidden early-return — see the doc comment above.
    if session.state() == PaletteState::Hidden { return; }
    let logical_h = ui::window_height(session.state(), session.filtered().len().max(1));
    let scale = window.scale_factor();
    let physical_h = (logical_h as f64 * scale).round() as u32;
    let mut last = last_height.lock().unwrap();
    if *last == physical_h {
        return;
    }
    *last = physical_h;
    let current = window.inner_size();
    let new_size = winit::dpi::PhysicalSize::new(current.width.max(1), physical_h.max(1));
    let _ = window.request_inner_size(new_size);
    // WR-02 fix: keep the framebuffer covering the new physical size — the
    // region revealed by a window GROWTH would otherwise have nothing to draw.
    session.resize_framebuffer(new_size.width, new_size.height);
}

/// Close: enqueue Destroy for the recorded window id (a late window creation
/// is destroyed by the `on_created` pairing callback in `build_window_spec`).
fn close_palette(session: &PaletteSession, windows: &Arc<WindowManagerHandle>) {
    if let Some(id) = session.close() {
        windows.destroy(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mybox_core::{Event, EventBus, HotkeyManager, WindowRequest};
    use std::time::Duration;

    fn wait_until(cond: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// A headless context over a fresh bus (no real config dir / OS hotkey).
    fn sample_context() -> (Arc<EventBus>, Arc<WindowManagerHandle>, ModuleContext) {
        let bus = Arc::new(EventBus::new());
        let handle = Arc::new(WindowManagerHandle::new());
        let ctx = ModuleContext::new(
            Arc::clone(&bus),
            Arc::clone(&handle),
            Arc::new(ConfigCenter::default()),
            Arc::new(HotkeyManager::new()),
            Arc::new(mybox_core::CommandRegistry::default()),
            UiThreadProxy::new(),
        );
        (bus, handle, ctx)
    }

    fn emit_toggle(bus: &Arc<EventBus>) {
        bus.emit(Event {
            from: "core",
            kind: "hotkey.triggered",
            payload: EventPayload::Framework(FrameworkEvent::HotkeyTriggered {
                id: 1,
                action: "toggle_palette".to_string(),
            }),
        });
    }

    #[test]
    fn id_name_and_default_config() {
        let module = PaletteModule::new();
        assert_eq!(module.id(), "palette");
        assert_eq!(module.name(), "命令面板");
        let table = module.default_config();
        assert_eq!(
            table.get("hotkey"),
            Some(&toml::Value::String(default_hotkey()))
        );
    }

    #[test]
    fn hotkey_toggle_summon_creates_floating_window() {
        let (bus, handle, ctx) = sample_context();
        PaletteModule::new()
            .init(&ctx)
            .expect("init registers handlers");

        emit_toggle(&bus);

        // The handler runs on the bus worker thread; poll for the Create
        // request (try_recv consumes — capture it in the poll).
        let received: Arc<std::sync::Mutex<Option<WindowRequest>>> =
            Arc::new(std::sync::Mutex::new(None));
        let got = received.clone();
        assert!(
            wait_until(|| {
                if let Some(req) = handle.try_recv() {
                    *got.lock().unwrap() = Some(req);
                    true
                } else {
                    false
                }
            }),
            "toggle hotkey never enqueued a WindowRequest"
        );
        let guard = received.lock().unwrap();
        match guard.as_ref() {
            Some(WindowRequest::Create(spec)) => {
                assert_eq!(spec.kind, WindowKind::Floating);
                assert_eq!(spec.title, "mybox-palette");
                assert!(spec.inner_size.is_some(), "palette must have a fixed size");
                assert!(spec.position.is_some(), "palette must be positioned at the monitor center");
                // Task 4: the full render chain is wired (egui frame loop +
                // framebuffer blit).
                assert!(spec.on_event_win.is_some(), "on_event_win frame loop must be wired");
                assert!(spec.on_draw.is_some(), "on_draw blit must be wired");
            }
            other => panic!(
                "expected Create, got {}",
                request_name(other.expect("request captured"))
            ),
        }
    }

    /// WindowRequest has no Debug derive — name the variant for panic messages.
    fn request_name(req: &WindowRequest) -> &'static str {
        match req {
            WindowRequest::Create(_) => "Create",
            WindowRequest::Destroy(_) => "Destroy",
            WindowRequest::Redraw(_) => "Redraw",
            WindowRequest::SetCursor(_, _) => "SetCursor",
        }
    }

    #[test]
    fn hotkey_toggle_closes_after_window_created() {
        let (bus, handle, ctx) = sample_context();
        let module = PaletteModule::new();
        module.init(&ctx).expect("init registers handlers");

        // Summon.
        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Create(_)))),
            "first toggle must summon"
        );
        // Simulate the core's window-created pairing.
        module.session.set_window_id(7);

        // Toggle again → close.
        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Destroy(7)))),
            "second toggle must enqueue Destroy(7)"
        );
        assert_eq!(module.session.state(), session::PaletteState::Hidden);
    }

    #[test]
    fn late_window_created_after_close_is_destroyed() {
        // Build-destroy pairing via the production on_created callback: close
        // before the window is created → the late creation is destroyed
        // immediately (no orphan, the 02-04 re-entrancy lesson; the GAP-1
        // per-window pairing path).
        let (bus, handle, ctx) = sample_context();
        let module = PaletteModule::new();
        module.init(&ctx).expect("init registers handlers");

        emit_toggle(&bus);
        // Capture the spec (not just matches!) so the on_created callback can
        // be driven directly — what production's App::create_window does.
        let spec_slot: Arc<std::sync::Mutex<Option<WindowSpec>>> =
            Arc::new(std::sync::Mutex::new(None));
        let slot = Arc::clone(&spec_slot);
        assert!(
            wait_until(|| match handle.try_recv() {
                Some(WindowRequest::Create(spec)) => {
                    *slot.lock().unwrap() = Some(spec);
                    true
                }
                _ => false,
            }),
            "first toggle must summon"
        );

        // Close before the window is created → pending_close set.
        emit_toggle(&bus);
        assert!(
            wait_until(|| module.session.has_live_window()),
            "close-before-create must leave the pairing pending"
        );

        // Simulate the late creation: production calls spec.on_created(id) on
        // the main thread inside create_window.
        let cb = spec_slot
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|spec| spec.on_created.take())
            .expect("spec must carry the on_created pairing callback");
        cb(9);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Destroy(9)))),
            "late window must be destroyed immediately"
        );
        assert!(!module.session.has_live_window(), "pairing consumed — nothing live");
    }

    #[test]
    fn summon_spec_carries_on_created_pairing() {
        // The production pairing lives in the spec: the on_created callback
        // records the palette's OWN window id, and a later toggle destroys
        // exactly that window (GAP-1 — no broadcast window-created involved,
        // so another module's window can never be paired or destroyed here).
        let (bus, handle, ctx) = sample_context();
        let module = PaletteModule::new();
        module.init(&ctx).expect("init registers handlers");

        emit_toggle(&bus);
        let spec_slot: Arc<std::sync::Mutex<Option<WindowSpec>>> =
            Arc::new(std::sync::Mutex::new(None));
        let slot = Arc::clone(&spec_slot);
        assert!(
            wait_until(|| match handle.try_recv() {
                Some(WindowRequest::Create(spec)) => {
                    *slot.lock().unwrap() = Some(spec);
                    true
                }
                _ => false,
            }),
            "first toggle must summon"
        );
        let mut guard = spec_slot.lock().unwrap();
        let spec = guard.as_mut().expect("spec captured");
        assert!(spec.on_created.is_some(), "spec must carry the pairing callback");

        // Simulate the main-thread on_created call with id 42.
        let cb = spec.on_created.take().expect("pairing callback");
        cb(42);
        assert_eq!(
            module.session.window_id(),
            Some(42),
            "on_created must record the palette window id"
        );
        drop(guard);

        // Toggle again → the paired window is destroyed.
        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Destroy(42)))),
            "second toggle must enqueue Destroy(42)"
        );
        assert_eq!(module.session.state(), session::PaletteState::Hidden);
    }

    #[test]
    fn init_ok_headless() {
        let (_bus, _handle, ctx) = sample_context();
        let module = PaletteModule::new();
        // HotkeyManager is not initialized headlessly — registration is
        // deferred via the ui proxy stash and must not panic (capture shape).
        module.init(&ctx).expect("init must not panic headlessly");
        assert!(module.ui.get().is_some(), "ui proxy injected during init");
    }

    #[test]
    fn hotkey_from_config_reads_palette_section_or_defaults() {
        let config = ConfigCenter::default();
        assert_eq!(hotkey_from_config(&config), default_hotkey());
        config.set(
            "palette",
            "hotkey",
            toml::Value::String("Ctrl+Alt+Space".to_string()),
        );
        assert_eq!(hotkey_from_config(&config), "Ctrl+Alt+Space");
    }

    #[test]
    fn enter_executes_selected_command() {
        // The Enter path: summon a counting command, run execute directly
        // (the on_event_win Enter arm calls exactly this), assert Executing +
        // runner ran, then drive finalize headlessly (the UiThreadProxy has no
        // loop yet — the completion closure is stashed, so finalize is called
        // on the session directly, which is what the stashed closure would do).
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (_bus, handle, _ctx) = sample_context();
        let module = PaletteModule::new();

        let count = Arc::new(AtomicUsize::new(0));
        let cmd = mybox_core::Command {
            id: "test.count",
            name: "counting command".to_string(),
            description: "test".to_string(),
            keywords: vec![],
            hide_before_execute: false,
            runner: {
                let c = Arc::clone(&count);
                Arc::new(move || {
                    let c = Arc::clone(&c);
                    Box::pin(async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                })
            },
        };
        let gen = module.session.summon(vec![cmd.clone()]);
        let ui = UiThreadProxy::new();
        execute::execute(&module.session, &ui, &handle, cmd);

        assert!(wait_until(|| count.load(Ordering::SeqCst) == 1), "runner never ran");
        assert_eq!(module.session.state(), PaletteState::Executing);
        module.session.set_window_id(7);
        let id = module.session.finalize(gen, Ok(()));
        assert_eq!(id, Some(7), "finalize Ok returns the window to destroy");
        assert_eq!(module.session.state(), PaletteState::Hidden);
    }

    /// A headless key-router rig: session + window handle + an UNSET ui proxy.
    /// The Ctrl+P/N tests never reach the Enter/execute arm, so no proxy
    /// injection is needed (same discipline as the existing hotkey tests).
    fn router_rig() -> (
        Arc<PaletteSession>,
        Arc<WindowManagerHandle>,
        Arc<OnceLock<UiThreadProxy>>,
    ) {
        (
            Arc::new(PaletteSession::new()),
            Arc::new(WindowManagerHandle::new()),
            Arc::new(OnceLock::new()),
        )
    }

    /// A fake command literal for the router tests.
    fn fake_cmd(id: &'static str) -> mybox_core::Command {
        mybox_core::Command {
            id,
            name: format!("command {id}"),
            description: "test".to_string(),
            keywords: vec![],
            hide_before_execute: false,
            runner: Arc::new(|| Box::pin(async { Ok(()) })),
        }
    }

    #[test]
    fn ctrl_p_moves_selection_up() {
        // GAP-6: Ctrl+P is equivalent to ↑ — consumed by the router and moves
        // the selection up.
        use winit::keyboard::{Key, ModifiersState};
        let (session, windows, ui_proxy) = router_rig();
        session.summon(vec![fake_cmd("a"), fake_cmd("b"), fake_cmd("c")]);
        // First ↓ selects index 0 (UI-SPEC); a second ↓ lands on index 1.
        session.move_selection(1);
        session.move_selection(1);
        assert_eq!(session.selection(), Some(1));
        assert!(
            on_palette_key(
                &session,
                &windows,
                &ui_proxy,
                &Key::Character("p".into()),
                ModifiersState::CONTROL,
            ),
            "Ctrl+P must be consumed by the router"
        );
        assert_eq!(session.selection(), Some(0), "Ctrl+P moves the selection up");
    }

    #[test]
    fn ctrl_n_moves_selection_down() {
        // GAP-6: Ctrl+N is equivalent to ↓.
        use winit::keyboard::{Key, ModifiersState};
        let (session, windows, ui_proxy) = router_rig();
        session.summon(vec![fake_cmd("a"), fake_cmd("b"), fake_cmd("c")]);
        session.move_selection(1);
        assert_eq!(session.selection(), Some(0));
        assert!(
            on_palette_key(
                &session,
                &windows,
                &ui_proxy,
                &Key::Character("n".into()),
                ModifiersState::CONTROL,
            ),
            "Ctrl+N must be consumed by the router"
        );
        assert_eq!(session.selection(), Some(1), "Ctrl+N moves the selection down");
    }

    #[test]
    fn ctrl_pn_wraps_like_arrows() {
        // GAP-6: Ctrl+P/N wrap around exactly like ↑/↓.
        use winit::keyboard::{Key, ModifiersState};
        let (session, windows, ui_proxy) = router_rig();
        session.summon(vec![fake_cmd("a"), fake_cmd("b"), fake_cmd("c")]);
        session.move_selection(1);
        assert_eq!(session.selection(), Some(0));
        assert!(
            on_palette_key(
                &session,
                &windows,
                &ui_proxy,
                &Key::Character("p".into()),
                ModifiersState::CONTROL,
            )
        );
        assert_eq!(
            session.selection(),
            Some(2),
            "Ctrl+P at index 0 wraps around to the last entry"
        );
        assert!(
            on_palette_key(
                &session,
                &windows,
                &ui_proxy,
                &Key::Character("n".into()),
                ModifiersState::CONTROL,
            )
        );
        assert_eq!(
            session.selection(),
            Some(0),
            "Ctrl+N at the last index wraps around to index 0"
        );
    }

    #[test]
    fn plain_p_without_ctrl_is_not_consumed() {
        // GAP-6 must-have truth: without Ctrl the guards fail and the router
        // returns false — plain P/N fall through to egui and enter the
        // TextEdit as normal characters.
        use winit::keyboard::{Key, ModifiersState};
        let (session, windows, ui_proxy) = router_rig();
        session.summon(vec![fake_cmd("a"), fake_cmd("b"), fake_cmd("c")]);
        session.move_selection(1);
        let before = session.selection();
        assert!(
            !on_palette_key(
                &session,
                &windows,
                &ui_proxy,
                &Key::Character("p".into()),
                ModifiersState::empty(),
            ),
            "plain P without Ctrl must fall through to egui"
        );
        assert_eq!(session.selection(), before, "plain P must not navigate");
        assert!(
            !on_palette_key(
                &session,
                &windows,
                &ui_proxy,
                &Key::Character("n".into()),
                ModifiersState::empty(),
            ),
            "plain N without Ctrl must fall through to egui"
        );
        assert_eq!(session.selection(), before, "plain N must not navigate");
    }

    #[test]
    fn ctrl_pn_in_error_state_closes_panel() {
        // D-05: the Error arm sits FIRST — any key, including Ctrl+P/N, closes
        // the panel instead of navigating.
        use winit::keyboard::{Key, ModifiersState};
        let (session, windows, ui_proxy) = router_rig();
        let gen = session.summon(vec![fake_cmd("a")]);
        assert!(session.set_executing(gen, "a"));
        session.set_window_id(7);
        assert_eq!(
            session.finalize(gen, Err(anyhow::anyhow!("x"))),
            None,
            "error keeps the window open (D-05)"
        );
        assert_eq!(session.state(), PaletteState::Error);
        assert!(
            on_palette_key(
                &session,
                &windows,
                &ui_proxy,
                &Key::Character("p".into()),
                ModifiersState::CONTROL,
            ),
            "any key in Error state is consumed"
        );
        assert!(
            wait_until(|| matches!(windows.try_recv(), Some(WindowRequest::Destroy(7)))),
            "Ctrl+P in Error state must enqueue Destroy(7)"
        );
        assert_eq!(
            session.state(),
            PaletteState::Hidden,
            "Error-state any-key closes the panel"
        );
    }

    #[test]
    fn hotkey_toggle_during_executing_closes_and_ignores_stale_finalize() {
        // Executing + hotkey → close (runner continues); re-summon bumps the
        // generation; the old runner's completion is ignored (Pitfall 3).
        let (bus, handle, ctx) = sample_context();
        let module = PaletteModule::new();
        module.init(&ctx).expect("init registers handlers");

        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Create(_)))),
            "first toggle must summon"
        );
        module.session.set_window_id(7);

        // Simulate Enter: transition to Executing with the current generation.
        let gen1 = module.session.generation();
        assert!(module.session.set_executing(gen1, "slow.cmd"));

        // Hotkey toggle mid-execution closes the panel (the runner keeps going).
        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Destroy(7)))),
            "toggle during Executing must close the panel"
        );

        // Re-summon: a fresh palette with a new generation.
        emit_toggle(&bus);
        assert!(
            wait_until(|| matches!(handle.try_recv(), Some(WindowRequest::Create(_)))),
            "re-toggle must summon a fresh palette"
        );
        let gen2 = module.session.generation();
        assert!(gen2 > gen1, "generation must advance per summon");

        // The old runner's completion must not touch the new palette.
        assert_eq!(module.session.finalize(gen1, Ok(())), None, "stale finalize is a no-op");
        assert_eq!(module.session.state(), PaletteState::Idle, "fresh panel untouched");
    }
}
