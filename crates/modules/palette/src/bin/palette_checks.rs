//! Display / OS integration checks for the palette module (plan 03-02-03).
//!
//! This is a **binary**, not a `#[test]`: winit on macOS requires the
//! `EventLoop` to be created on the *real main thread* and allows only one
//! `EventLoop` per process, so the `#[ignore]` integration tests spawn this
//! binary (one subprocess per check) — the same harness as `capture_checks`
//! (02-04) and `display_checks` (01).
//!
//! Each check drives the PRODUCTION path — `summon_palette` →
//! `build_window_spec`'s `on_event_win` closure — with synthetic winit events
//! injected into a real window, and asserts on the session state + the
//! standalone `WindowManagerHandle` request queue. A 10s deadline watchdog
//! guards against hangs; the driver is re-entered on a 50ms poll (finalize
//! hops arrive via `AppEvent::Ui`, so the driver must never block the loop).
//!
//! Usage: `palette_checks <summon_render|fuzzy_navigation_execute|capture_hides_first|five_summon_esc_no_residue|consecutive_summon_close|glyph_shape|position_stable_on_filter|hover_click_alignment|ctrl_pn_navigation|ime_commit_updates_input|keyword_highlight|click_hide_before_capture>`
//! Exit code 0 on success, 1 on failure, 2 on bad usage.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use mybox_core::command::{Command, CommandRegistry, CommandRunner};
use mybox_core::renderer::Renderer;
use mybox_core::window::{
    window_attributes, WindowId, WindowKind, WindowManager, WindowManagerHandle, WindowRequest,
    WindowSpec,
};
use mybox_core::winit::application::ApplicationHandler;
use mybox_core::winit::event::WindowEvent;
use mybox_core::winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use mybox_core::winit::window::WindowId as WinitWindowId;
use mybox_core::{AppEvent, TinySkiaSoftbufferRenderer, UiThreadProxy};

use mybox_palette::execute;
use mybox_palette::session::{PaletteSession, PaletteState};
use mybox_palette::{on_palette_key, summon_palette};

/// A per-check script invoked for each window event (see the harness below).
type Driver =
    Box<dyn FnMut(&mut PaletteHarness, &ActiveEventLoop, WindowEvent) -> Result<(), String>>;

/// Shared harness for the window-based checks: creates the winit window for a
/// summoned spec, drives `spec.on_event_win` with synthetic events, runs the
/// real renderer draw/present on RedrawRequested, and polls the script every
/// 50ms until it passes, fails, or the 10s deadline watchdog fires.
struct PaletteHarness {
    session: Arc<PaletteSession>,
    handle: Arc<WindowManagerHandle>,
    wm: WindowManager,
    pending_spec: Option<WindowSpec>,
    window: Option<Arc<mybox_core::winit::window::Window>>,
    current_winit_id: Option<WinitWindowId>,
    created_id: Option<WindowId>,
    deadline: Option<Instant>,
    driver: Option<Driver>,
    result: Option<Result<(), String>>,
}

impl PaletteHarness {
    fn new(
        session: Arc<PaletteSession>,
        handle: Arc<WindowManagerHandle>,
        pending_spec: WindowSpec,
        driver: Driver,
    ) -> Self {
        Self {
            session,
            handle,
            wm: WindowManager::new(),
            pending_spec: Some(pending_spec),
            window: None,
            current_winit_id: None,
            created_id: None,
            deadline: None,
            driver: Some(driver),
            result: None,
        }
    }

    /// Create the winit window for the pending spec and register it with the
    /// self-managed WindowManager (OverlayHarness shape, 02-04). Replaces any
    /// previous window: destroy the previous round's window via the
    /// WindowManager before re-registering (IN-03) — `wm.register` retains an
    /// Arc, so merely dropping `self.window` would leave the old palette
    /// window alive (and visible) for the whole check duration. Also pairs
    /// the session window id through the spec's `on_created` callback, which
    /// is what production's `App::create_window` does via `spec.on_created`
    /// (GAP-1 fix).
    fn realize_window(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        // IN-03: destroy the previous round's window — `wm.register` retains
        // an Arc, so merely dropping `self.window` would leave the old palette
        // window alive (and visible) for the whole check duration.
        if let Some(prev) = self.created_id.take() {
            self.wm.destroy(prev);
        }
        let spec = self
            .pending_spec
            .as_ref()
            .ok_or("no pending spec to realize")?;
        let attrs = window_attributes(spec);
        let window =
            Arc::new(el.create_window(attrs).map_err(|e| format!("create window: {e}"))?);
        let winit_id = window.id();
        let id = self.wm.next_id();
        let mut renderer = TinySkiaSoftbufferRenderer::new(Arc::clone(&window))
            .map_err(|e| format!("renderer: {e}"))?;
        // Windows softbuffer requires an explicit surface resize before the
        // first `buffer_mut` (win32 backend panics "Must set size of surface");
        // macOS tolerates the missing resize. Mirrors production's
        // Resized -> resize contract so the first present is always sized.
        let size = window.inner_size();
        renderer.resize(size.width, size.height);
        let kind = spec.kind;
        self.wm.register(
            id,
            kind,
            winit_id,
            Some(Arc::clone(&window)),
            Box::new(renderer) as Box<dyn Renderer>,
            WindowSpec {
                kind,
                title: spec.title.clone(),
                transparent: spec.transparent,
                always_on_top: spec.always_on_top,
                ..Default::default()
            },
        );
        self.window = Some(window);
        self.current_winit_id = Some(winit_id);
        self.created_id = Some(id);
        // Production pairing path (GAP-1): the on_created closure captures
        // only Arc clones of the session/windows, so borrowing `spec` from
        // `pending_spec` never touches the harness itself.
        if let Some(cb) = &spec.on_created {
            cb(id);
        }
        Ok(())
    }

    /// Inject a synthetic event into the current spec's `on_event_win`.
    fn inject(&self, event: WindowEvent) -> Result<(), String> {
        let spec = self.pending_spec.as_ref().ok_or("no spec for injection")?;
        let window = self.window.as_ref().ok_or("window not realized")?;
        let cb = spec.on_event_win.as_ref().ok_or("spec has no on_event_win")?;
        cb(window, &event);
        Ok(())
    }

    /// Count framebuffer pixels that differ from the opaque #202020 card
    /// background (text/antialiasing evidence that a frame rendered).
    fn non_background_pixels(&self) -> usize {
        self.session.with_framebuffer(|fb| match fb {
            Some(fb) => fb
                .data()
                .chunks_exact(4)
                .filter(|p| p[3] > 0 && (p[0] != 32 || p[1] != 32 || p[2] != 32))
                .count(),
            None => 0,
        })
    }

    /// The live window's outer position in physical px (top-left origin).
    fn window_outer_position(&self) -> Result<(i32, i32), String> {
        let window = self.window.as_ref().ok_or("window not realized")?;
        window
            .outer_position()
            .map(|p| (p.x, p.y))
            .map_err(|e| format!("outer_position: {e}"))
    }

    /// The live window's inner size in physical px.
    fn window_inner_size(&self) -> Result<(u32, u32), String> {
        let window = self.window.as_ref().ok_or("window not realized")?;
        let size = window.inner_size();
        Ok((size.width, size.height))
    }

    /// Run the real renderer chain (draw + present) for the current window —
    /// what core's `handle_redraw` does with the palette's `on_draw` blit.
    fn render_present(&mut self) -> Result<(), String> {
        let id = self.created_id.ok_or("window not registered")?;
        let spec = self.pending_spec.as_ref().ok_or("no spec")?;
        let on_draw = spec.on_draw.as_ref().ok_or("spec has no on_draw")?;
        let state = self
            .wm
            .get_mut(id)
            .ok_or("created window must be registered with the WindowManager")?;
        state.renderer.draw(&mut |pixmap, w, h| on_draw(pixmap, w, h));
        state
            .renderer
            .present()
            .map_err(|e| format!("present: {e}"))
    }

    fn pass(&mut self) {
        self.result = Some(Ok(()));
    }
}

impl ApplicationHandler<AppEvent> for PaletteHarness {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.created_id.is_some() {
            return;
        }
        if let Err(e) = self.realize_window(el) {
            self.result = Some(Err(e));
            el.exit();
            return;
        }
        self.deadline = Some(Instant::now() + Duration::from_secs(10));
        el.set_control_flow(ControlFlow::WaitUntil(self.deadline.unwrap()));
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WinitWindowId, event: WindowEvent) {
        // Ignore stragglers from replaced windows.
        if Some(id) != self.current_winit_id {
            return;
        }
        let mut driver = self.driver.take();
        let result = match &mut driver {
            Some(d) => d(self, el, event),
            None => Ok(()),
        };
        self.driver = driver;
        if let Err(e) = result {
            self.result = Some(Err(e));
        }
        if self.result.is_some() {
            el.exit();
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, event: AppEvent) {
        match event {
            // The real UiThreadProxy hop: runner completions land here on the
            // main thread (finalize + destroy enqueue).
            AppEvent::Ui(f) => {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.result.is_some() {
            el.exit();
            return;
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.result = Some(Err("deadline exceeded (10s watchdog)".into()));
                el.exit();
                return;
            }
        }
        // Polling drive: re-enter the driver every 50ms until the script
        // completes. The driver must never block — completion hops arrive as
        // AppEvent::Ui between poll ticks.
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        // Windows stops dispatching RedrawRequested for hidden windows
        // (WM_PAINT is not delivered to hidden windows, so request_redraw →
        // RedrawWindow is a no-op there), which would stall poll stages that
        // wait on shared state — e.g. stage 4 of click_hide_before_capture,
        // whose gated runner only releases after the click already hid the
        // window (04-01 CI watchdog). macOS keeps dispatching redraws after
        // orderOut, so this drive path is Windows-only in practice. Drive the
        // script directly with a synthetic RedrawRequested so hidden-window
        // polling keeps advancing.
        let hidden = self
            .window
            .as_ref()
            .and_then(|w| w.is_visible())
            .map(|v| !v)
            .unwrap_or(false);
        if hidden {
            let mut driver = self.driver.take();
            let result = match &mut driver {
                Some(d) => d(self, el, WindowEvent::RedrawRequested),
                None => Ok(()),
            };
            self.driver = driver;
            if let Err(e) = result {
                self.result = Some(Err(e));
            }
            if self.result.is_some() {
                el.exit();
            }
        }
    }
}

// ─── Shared check plumbing ──────────────────────────────────────────────────

/// Press a key through the panel key router (the production on_event_win
/// path). winit 0.30's `KeyEvent` carries a `pub(crate)` platform field, so
/// synthetic `KeyboardInput` events are not constructible — the router is the
/// shared entry point both the closure and this harness drive. No modifiers
/// (the unmodified-key path — every existing probe).
fn press_key(
    session: &Arc<PaletteSession>,
    handle: &Arc<WindowManagerHandle>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
    key: mybox_core::winit::keyboard::Key,
) -> bool {
    on_palette_key(
        session,
        handle,
        ui_proxy,
        &key,
        mybox_core::winit::keyboard::ModifiersState::empty(),
    )
}

/// Press a key with explicit modifiers (the GAP-6 Ctrl+P/N probe path).
fn press_key_mods(
    session: &Arc<PaletteSession>,
    handle: &Arc<WindowManagerHandle>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
    key: mybox_core::winit::keyboard::Key,
    mods: mybox_core::winit::keyboard::ModifiersState,
) -> bool {
    on_palette_key(session, handle, ui_proxy, &key, mods)
}

fn fake_command(
    id: &'static str,
    name: &str,
    keywords: &[&'static str],
    runner: CommandRunner,
) -> Command {
    Command {
        id,
        name: name.to_string(),
        description: format!("{name} description"),
        keywords: keywords.to_vec(),
        hide_before_execute: false,
        runner,
    }
}

fn ok_runner() -> CommandRunner {
    Arc::new(|| Box::pin(async { Ok(()) }))
}

fn counting_runner(counter: Arc<AtomicUsize>) -> CommandRunner {
    Arc::new(move || {
        let c = Arc::clone(&counter);
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    })
}

/// A runner blocked on a channel gate — the counter only increments after the
/// gate releases (observable mid-execute state). The receiver is wrapped in a
/// Mutex BEFORE capture (`Receiver` is Send but not Sync, and the runner
/// closure must be Send + Sync).
fn gated_runner(counter: Arc<AtomicUsize>, rx: std::sync::mpsc::Receiver<()>) -> CommandRunner {
    let rx = Arc::new(Mutex::new(rx));
    Arc::new(move || {
        let c = Arc::clone(&counter);
        let rx = Arc::clone(&rx);
        Box::pin(async move {
            rx.lock().unwrap().recv().ok();
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    })
}

fn registry_with(commands: Vec<Command>) -> Arc<CommandRegistry> {
    let mut registry = CommandRegistry::new();
    for cmd in commands {
        registry
            .register(cmd)
            .expect("fake command ids are unique");
    }
    Arc::new(registry)
}

fn expect_create(handle: &WindowManagerHandle) -> Result<WindowSpec, String> {
    match handle.try_recv() {
        Some(WindowRequest::Create(spec)) => Ok(spec),
        other => Err(format!("expected Create, got {}", request_name(other.as_ref()))),
    }
}

fn request_name(req: Option<&WindowRequest>) -> &'static str {
    match req {
        Some(WindowRequest::Create(_)) => "Create",
        Some(WindowRequest::Destroy(_)) => "Destroy",
        Some(WindowRequest::Redraw(_)) => "Redraw",
        Some(WindowRequest::SetCursor(_, _)) => "SetCursor",
        None => "Nothing",
    }
}

/// Create the EventLoop (real main thread), wire the REAL UiThreadProxy hop,
/// run the harness to completion.
fn run_harness(
    harness: PaletteHarness,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
) -> Result<(), String> {
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .map_err(|e| format!("event loop: {e}"))?;
    let proxy = event_loop.create_proxy();
    let ui = UiThreadProxy::new();
    ui.set_proxy(proxy);
    let _ = ui_proxy.set(ui);
    let mut harness = harness;
    event_loop
        .run_app(&mut harness)
        .map_err(|e| format!("run app: {e}"))?;
    harness
        .result
        .unwrap_or_else(|| Err("check finished without a result".into()))
}

// ─── Check 1: summon renders + ESC closes ───────────────────────────────────

fn check_summon_render() -> Result<(), String> {
    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    let registry = registry_with(vec![
        fake_command("c0", "Alpha One", &[], ok_runner()),
        fake_command("c1", "Beta Two", &[], ok_runner()),
        fake_command("c2", "Gamma Three", &[], ok_runner()),
        fake_command("c3", "Delta Four", &[], ok_runner()),
        fake_command("c4", "Epsilon Five", &[], ok_runner()),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;
    assert!(spec.kind == WindowKind::Floating, "palette must be a Floating window");
    assert!(spec.inner_size.is_some(), "palette must have a fixed size");
    assert!(spec.on_event_win.is_some(), "frame loop wired");
    assert!(spec.on_draw.is_some(), "blit wired");

    let s = Arc::clone(&session);
    let ui_lock = Arc::clone(&ui_proxy);
    let mut stage = 0u8;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |h, _el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // Drive the frame loop → rasterize into the framebuffer.
                    h.inject(WindowEvent::RedrawRequested)?;
                    if h.non_background_pixels() == 0 {
                        return Err("frame must produce non-background pixels".into());
                    }
                    // The real core render chain: on_draw blit + present.
                    h.render_present()?;
                    stage = 1;
                    Ok(())
                }
                1 => {
                    // ESC closes: Destroy enqueued, session Hidden.
                    press_key(
                        &s,
                        &h.handle,
                        &ui_lock,
                        mybox_core::winit::keyboard::Key::Named(
                            mybox_core::winit::keyboard::NamedKey::Escape,
                        ),
                    );
                    match h.handle.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            if Some(id) != h.created_id {
                                return Err(format!(
                                    "ESC must destroy the created window ({id} != {:?})",
                                    h.created_id
                                ));
                            }
                        }
                        // Drain Redraw stragglers; keep polling.
                        _ => return Ok(()),
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("ESC must move the session to Hidden".into());
                    }
                    h.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 2: fuzzy filter reorders, ArrowDown moves, Enter executes the
//     MAPPED command (the Filtering-reorder regression, end to end) ──────────

fn check_fuzzy_navigation_execute() -> Result<(), String> {
    use mybox_core::winit::keyboard::{Key, NamedKey};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());

    // 5 commands; ≥2 hit "jietu" (otherwise ArrowDown wraps around and the
    // selection==Some(1) assertion is impossible). idx 1 matches by NAME
    // (tier 0), idx 2 (capture fake) matches by KEYWORD (tier 2) → filtered
    // must be [1, 2] — a real reorder, not the Idle identity mapping.
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let capture_ran = Arc::new(AtomicUsize::new(0));
    let jietu_ran = Arc::new(AtomicUsize::new(0));
    let registry = registry_with(vec![
        fake_command("c0", "Zero Command", &[], ok_runner()),
        fake_command("c1", "Jietu Editor", &[], counting_runner(Arc::clone(&jietu_ran))),
        fake_command(
            "capture.start",
            "Capture Fake",
            &["jietu"],
            gated_runner(Arc::clone(&capture_ran), release_rx),
        ),
        fake_command("c3", "Three Command", &[], ok_runner()),
        fake_command("c4", "Four Command", &[], ok_runner()),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let ui_lock = Arc::clone(&ui_proxy);
    let mut stage = 0u8;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |h, _el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // Initial Idle frame renders.
                    h.inject(WindowEvent::RedrawRequested)?;
                    if h.non_background_pixels() == 0 {
                        return Err("Idle frame must render".into());
                    }
                    stage = 1;
                    Ok(())
                }
                1 => {
                    // Filtering: "jietu" hits idx 1 (name tier) and idx 2
                    // (keyword tier), name tier first.
                    s.set_input("jietu");
                    h.inject(WindowEvent::RedrawRequested)?;
                    if s.filtered() != vec![1, 2] {
                        return Err(format!("expected filtered [1, 2], got {:?}", s.filtered()));
                    }
                    if s.selection() != Some(0) {
                        return Err("input change must reset selection to 0".into());
                    }
                    stage = 2;
                    Ok(())
                }
                2 => {
                    press_key(&s, &h.handle, &ui_lock, Key::Named(NamedKey::ArrowDown));
                    if s.selection() != Some(1) {
                        return Err(format!(
                            "ArrowDown must select filtered position 1, got {:?}",
                            s.selection()
                        ));
                    }
                    stage = 3;
                    Ok(())
                }
                3 => {
                    // Enter executes commands()[filtered[1]] == idx 2 (the
                    // capture fake). A buggy resolve (selection as command
                    // index) would execute commands()[1] == the Jietu fake.
                    press_key(&s, &h.handle, &ui_lock, Key::Named(NamedKey::Enter));
                    if s.state() != PaletteState::Executing {
                        return Err(format!("Enter must enter Executing, got {:?}", s.state()));
                    }
                    if capture_ran.load(Ordering::SeqCst) != 0 {
                        return Err("gated runner must not complete before release".into());
                    }
                    if jietu_ran.load(Ordering::SeqCst) != 0 {
                        return Err("the WRONG command executed (selection mapped incorrectly)".into());
                    }
                    let _ = release_tx.send(());
                    stage = 4;
                    Ok(())
                }
                4 => {
                    // The real finalize hop (AppEvent::Ui) destroys the window.
                    match h.handle.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            if Some(id) != h.created_id {
                                return Err(format!("finalize must destroy {id:?}"));
                            }
                        }
                        _ => return Ok(()), // drain Redraw stragglers / wait
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("finalize Ok must close the panel".into());
                    }
                    if capture_ran.load(Ordering::SeqCst) != 1 {
                        return Err("capture fake must run exactly once".into());
                    }
                    if jietu_ran.load(Ordering::SeqCst) != 0 {
                        return Err("Jietu fake must never run".into());
                    }
                    h.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 3: capture (hide_before_execute) destroys the panel BEFORE the
//     runner runs — the screenshot-order hard constraint (no window needed) ──

fn check_capture_hides_palette_first() -> Result<(), String> {
    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    // No event loop: the finalize hop is stashed (never runs) — the queue
    // must therefore stay clean after release (no second Destroy).
    let ui = UiThreadProxy::new();

    let gen = session.summon(vec![fake_command("a", "Alpha", &[], ok_runner())]);
    session.set_window_id(7);

    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let ran = Arc::new(AtomicUsize::new(0));
    let cmd = Command {
        id: "capture.start",
        name: "开始截图".to_string(),
        description: "capture fake".to_string(),
        keywords: vec!["jietu"],
        hide_before_execute: true,
        runner: gated_runner(Arc::clone(&ran), release_rx),
    };
    execute::execute(&session, &ui, &handle, cmd);

    // While the runner is gated: the FIRST queued request must be Destroy(7)
    // and the runner must not have completed.
    let mut destroyed = false;
    for _ in 0..200 {
        match handle.try_recv() {
            Some(WindowRequest::Destroy(7)) => {
                destroyed = true;
                break;
            }
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    if !destroyed {
        return Err("hide_before_execute must enqueue Destroy before the runner runs".into());
    }
    if ran.load(Ordering::SeqCst) != 0 {
        return Err("runner must not complete before the Destroy".into());
    }
    if session.state() != PaletteState::Hidden {
        return Err("panel must be closed before the runner runs".into());
    }

    // Release: the runner completes; finalize is generation/state-guarded and
    // no second Destroy may appear.
    let _ = release_tx.send(());
    for _ in 0..200 {
        if ran.load(Ordering::SeqCst) == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if ran.load(Ordering::SeqCst) != 1 {
        return Err("runner must complete after release".into());
    }
    std::thread::sleep(Duration::from_millis(20));
    if let Some(req) = handle.try_recv() {
        return Err(format!(
            "no second Destroy expected after a hidden-panel completion, got {}",
            request_name(Some(&req))
        ));
    }
    let _ = gen;
    Ok(())
}

// ─── Check 4: 5× summon-ESC leaves zero residue (re-entrancy acceptance) ────

fn check_five_summon_esc_no_residue() -> Result<(), String> {
    use mybox_core::winit::keyboard::{Key, NamedKey};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    let registry = registry_with(vec![
        fake_command("c0", "Alpha One", &[], ok_runner()),
        fake_command("c1", "Beta Two", &[], ok_runner()),
        fake_command("c2", "Gamma Three", &[], ok_runner()),
        fake_command("c3", "Delta Four", &[], ok_runner()),
        fake_command("c4", "Epsilon Five", &[], ok_runner()),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let h = Arc::clone(&handle);
    let ui_lock = Arc::clone(&ui_proxy);
    let registry_lock = Arc::clone(&registry);
    let mut round: usize = 0;
    let mut phase = 0u8; // 0 = press ESC; 1 = expect Destroy + next summon
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |harness, el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            if round >= 5 {
                // Final residue assertions live here: `round` is owned by this
                // closure, not shared with the outer check function.
                if s.state() != PaletteState::Hidden {
                    return Err("session must end Hidden after 5 rounds".into());
                }
                if s.has_live_window() {
                    return Err("no live window may survive the 5 rounds".into());
                }
                harness.pass();
                return Ok(());
            }
            match phase {
                0 => {
                    press_key(&s, &h, &ui_lock, Key::Named(NamedKey::Escape));
                    phase = 1;
                    Ok(())
                }
                1 => match harness.handle.try_recv() {
                    Some(WindowRequest::Destroy(id)) => {
                        let expected = harness.created_id.ok_or("window id missing")?;
                        if id != expected {
                            return Err(format!("round {round}: Destroy({id}) != {expected}"));
                        }
                        if s.state() != PaletteState::Hidden {
                            return Err(format!("round {round}: session must be Hidden"));
                        }
                        if s.generation() != (round as u64) + 1 {
                            return Err(format!(
                                "round {round}: generation mismatch ({})",
                                s.generation()
                            ));
                        }
                        if s.consume_pending_close() {
                            return Err(format!("round {round}: pending_close residue"));
                        }
                        round += 1;
                        if round < 5 {
                            // Next summon through the production path.
                            summon_palette(&s, &h, &registry_lock, &ui_lock)
                                .map_err(|e| format!("round {round}: summon: {e}"))?;
                            let spec = expect_create(&harness.handle)?;
                            harness.pending_spec = Some(spec);
                            harness.realize_window(el)?;
                            phase = 0;
                        }
                        Ok(())
                    }
                    // Drain Redraw stragglers; keep polling.
                    _ => Ok(()),
                },
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 5: consecutive summon/close rounds keep the panel visible ────────

/// Consecutive summon/close rounds with REAL windows and a REAL event loop:
/// 3× (summon → on_created pairing → observe ≥2 frames with NO Destroy in the
/// queue — the panel STAYS visible, the direct regression of GAP-1's
/// flash-close → ESC → paired Destroy with zero residue), then a final summon
/// observed for ≥3 frames before the last close.
///
/// Coverage statement (kept honest): the probe drives summoning through
/// `summon_palette` at the bus level, NOT through the OS physical-hotkey path
/// — truth #1's single-toggle-per-press behavior is locked by the Task 1 unit
/// test (`on_hotkey_released_event_is_ignored`) and re-verified by human UAT
/// test 1 on the desktop. This probe's coverage is limited to: consecutive
/// summon/close cycles pair builds and destroys correctly with no residue,
/// and the final summon stays visible.
fn check_consecutive_summon_close() -> Result<(), String> {
    use mybox_core::winit::keyboard::{Key, NamedKey};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    let registry = registry_with(vec![
        fake_command("c0", "Alpha One", &[], ok_runner()),
        fake_command("c1", "Beta Two", &[], ok_runner()),
        fake_command("c2", "Gamma Three", &[], ok_runner()),
        fake_command("c3", "Delta Four", &[], ok_runner()),
        fake_command("c4", "Epsilon Five", &[], ok_runner()),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let h = Arc::clone(&handle);
    let ui_lock = Arc::clone(&ui_proxy);
    let registry_lock = Arc::clone(&registry);
    let mut round: usize = 0; // 0..=2 loop rounds, 3 = final summon round
    let mut phase = 0u8; // 0 = observe frames (no Destroy); 1 = expect Destroy after ESC
    let mut frames = 0u32; // observed RedrawRequested frames in the current phase-0 stretch
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |harness, el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            if round >= 4 {
                // All rounds (3 loop + 1 final) closed; residue assertions ran
                // in the last Destroy arm.
                if s.state() != PaletteState::Hidden {
                    return Err("session must end Hidden after the final close".into());
                }
                if s.has_live_window() {
                    return Err("no live window may survive the final close".into());
                }
                harness.pass();
                return Ok(());
            }
            match phase {
                0 => {
                    // Observation phase: while frames elapse the panel must
                    // STAY — no Destroy may be enqueued and the pairing must
                    // point at the current window (GAP-1 flash-close).
                    while let Some(req) = h.try_recv() {
                        if matches!(req, WindowRequest::Destroy(_)) {
                            return Err(format!(
                                "round {round}: Destroy enqueued while the panel must stay visible"
                            ));
                        }
                    }
                    let created = harness.created_id.ok_or("window id missing")?;
                    if s.window_id() != Some(created) {
                        return Err(format!(
                            "round {round}: session.window_id() == {:?}, expected {created}",
                            s.window_id()
                        ));
                    }
                    if s.state() != PaletteState::Idle {
                        return Err(format!(
                            "round {round}: state must be Idle, got {:?}",
                            s.state()
                        ));
                    }
                    frames += 1;
                    let needed: u32 = if round == 3 { 3 } else { 2 };
                    if frames >= needed {
                        press_key(&s, &h, &ui_lock, Key::Named(NamedKey::Escape));
                        phase = 1;
                    }
                    Ok(())
                }
                1 => match h.try_recv() {
                    Some(WindowRequest::Destroy(id)) => {
                        let created = harness.created_id.ok_or("window id missing")?;
                        if id != created {
                            return Err(format!("round {round}: Destroy({id}) != {created}"));
                        }
                        if s.state() != PaletteState::Hidden {
                            return Err(format!("round {round}: session must be Hidden after ESC"));
                        }
                        if s.has_live_window() {
                            return Err(format!("round {round}: no live window may remain"));
                        }
                        if s.consume_pending_close() {
                            return Err(format!("round {round}: pending_close residue"));
                        }
                        round += 1;
                        frames = 0;
                        phase = 0;
                        if round < 4 {
                            // Next summon through the production path.
                            summon_palette(&s, &h, &registry_lock, &ui_lock)
                                .map_err(|e| format!("round {round}: summon: {e}"))?;
                            let spec = expect_create(&h)?;
                            harness.pending_spec = Some(spec);
                            harness.realize_window(el)?;
                        }
                        Ok(())
                    }
                    // Drain Redraw stragglers; keep polling.
                    _ => Ok(()),
                },
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 6: glyph structure + incremental atlas patches (GAP-2) ───────────

/// Glyph-structure statistics over a premultiplied RGBA8 framebuffer (the
/// GAP-2 probe). The framebuffer is a COMPOSITED image (opaque #202020 card +
/// solid chrome fills + text), so alpha-based metrics cannot discriminate
/// glyphs from solid blocks (everything is opaque). Measures:
///
/// - text-pixel count and bbox (non-chrome, non-card pixels);
/// - distinct RGBA values among text pixels (guards catastrophic texture
///   failures — a frame without any texture diversity stays in single digits);
/// - `aa_spread`: the count of MID-TONE (60..245) pixels inside the input
///   text region (the white-on-#2E2E2E input box) — antialiased glyph stroke
///   edges. Calibrated on this DPI/font (03-04 SUMMARY): real glyphs ≈242,
///   the old solid-block bug ≈40 (rectangle-boundary AA only) — the ≥120
///   threshold is the solid-block discriminator.
///
/// `scale` = physical/logical pixel ratio (framebuffer width / panel width).
#[allow(clippy::type_complexity)]
fn glyph_structure(
    pixels: &[u8],
    width: usize,
    height: usize,
    scale: f32,
) -> (usize, Option<(usize, usize, usize, usize)>, usize, usize) {
    // ui.rs color tokens: BG #202020 (card), ROW_HOVERED #2E2E2E (input box),
    // ROW_SELECTED #404040 (selected row).
    let is_chrome = |p: &[u8]| {
        (p[0] == 32 && p[1] == 32 && p[2] == 32)
            || (p[0] == 46 && p[1] == 46 && p[2] == 46)
            || (p[0] == 64 && p[1] == 64 && p[2] == 64)
    };
    let is_text = |p: &[u8]| p[3] > 0 && !is_chrome(p);
    let mut non_bg = 0usize;
    let mut min_x = usize::MAX;
    let mut min_y = usize::MAX;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) * 4;
            let p = &pixels[i..i + 4];
            if is_text(p) {
                non_bg += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x == usize::MAX {
        return (0, None, 0, 0);
    }
    let mut kinds = std::collections::HashSet::new();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let i = (y * width + x) * 4;
            let p = &pixels[i..i + 4];
            if is_text(p) {
                kinds.insert([p[0], p[1], p[2], p[3]]);
            }
        }
    }
    // Input text region (ui.rs: 12px margin + 48px input box, text inset 12px):
    // logical x 24..588, y 24..60 — mapped to physical pixels via `scale`.
    let ix0 = (24.0 * scale) as usize;
    let iy0 = (24.0 * scale) as usize;
    let ix1 = ((588.0 * scale) as usize).min(width);
    let iy1 = ((60.0 * scale) as usize).min(height);
    let mut aa_spread = 0usize;
    for y in iy0..iy1 {
        for x in ix0..ix1 {
            let i = (y * width + x) * 4;
            let p = &pixels[i..i + 4];
            let mx = p[0].max(p[1]).max(p[2]);
            if (60..=245).contains(&(mx as i32)) {
                aa_spread += 1;
            }
        }
    }
    (non_bg, Some((min_x, min_y, max_x, max_y)), kinds.len(), aa_spread)
}

/// Glyph AA discrimination threshold for `aa_spread` (mid-tone stroke-edge
/// pixels): solid text-color blocks ≈40, real glyphs ≈242 with macOS Hiragino
/// grayscale AA. Windows YaHei renders with ClearType subpixel AA — edges are
/// more saturated (one channel near max), so fewer pixels land in the mid-tone
/// band (measured ≈108); keep a 2x margin over the solid-block baseline there.
#[cfg(target_os = "windows")]
const AA_SPREAD_MIN: usize = 80;
#[cfg(not(target_os = "windows"))]
const AA_SPREAD_MIN: usize = 120;

/// Real-window glyph rendering probe (PAL-02 / GAP-2 regression, 03-04).
///
/// Drives three frames with `Ime::Commit` injections between them so NEW
/// glyphs (not present in the initial UI) are rasterized incrementally —
/// `TextureAtlas::take_delta` then emits a `partial` delta, exercising the
/// apply_textures in-place patch path (GAP-2's secondary root cause) in a real
/// window. Assertions (thresholds calibrated on the real framebuffer — the
/// composited output is fully opaque, so the discriminator is the mid-tone AA
/// spread of glyph stroke edges, see `glyph_structure` and the 03-04 SUMMARY):
///
/// 1. Frame-3 vs frame-1 pixel diff > 0 — the committed text actually rendered
///    (proves the partial atlas delta was produced and applied).
/// 2. Glyph structure on frame 3 (the primary root cause regression: the old
///    color-equality dispatch painted solid text-color blocks): bbox ≥ 8x8
///    physical px; ≥16 distinct text RGBA values; `aa_spread` ≥ 120
///    (real glyphs ≈242, solid blocks ≈40).
fn check_glyph_shape() -> Result<(), String> {
    use mybox_core::winit::event::Ime;
    use mybox_core::winit::keyboard::{Key, NamedKey};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    // CJK command names force the glyph path (PAL-02 visual truth).
    let registry = registry_with(vec![
        Command {
            id: "capture.start",
            name: "开始截图".to_string(),
            description: "截取屏幕区域".to_string(),
            keywords: vec!["jietu"],
            hide_before_execute: true,
            runner: ok_runner(),
        },
        fake_command("builtin.quit", "退出应用", &["quit"], ok_runner()),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let ui_lock = Arc::clone(&ui_proxy);
    let mut stage = 0u8;
    let mut frame1: Option<Vec<u8>> = None;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |h, _el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // Frame 1: baseline render — the initial CJK glyphs
                    // (placeholder + command names) enter the atlas via the
                    // full delta; snapshot the frame-1 pixels.
                    h.inject(WindowEvent::RedrawRequested)?;
                    if h.non_background_pixels() == 0 {
                        return Err("frame 1 must render glyph pixels".into());
                    }
                    frame1 = Some(h.session.with_framebuffer(|fb| {
                        fb.as_ref().map(|p| p.data().to_vec()).unwrap_or_default()
                    }));
                    // Introduce CJK glyphs absent from the initial UI — the
                    // next frame rasterizes them incrementally (partial atlas
                    // delta = the GAP-2 secondary path).
                    h.inject(WindowEvent::Ime(Ime::Commit("测试".to_string())))?;
                    stage = 1;
                    Ok(())
                }
                1 => {
                    // Frame 2: renders the committed "测试" glyphs.
                    h.inject(WindowEvent::RedrawRequested)?;
                    // Latin glyphs force a further atlas increment.
                    h.inject(WindowEvent::Ime(Ime::Commit("zz".to_string())))?;
                    stage = 2;
                    Ok(())
                }
                2 => {
                    // Frame 3: renders "测试zz" — read the framebuffer and
                    // assert the GAP-2 regressions.
                    h.inject(WindowEvent::RedrawRequested)?;
                    let frame3 = h.session.with_framebuffer(|fb| {
                        fb.as_ref().map(|p| p.data().to_vec()).unwrap_or_default()
                    });
                    let (width, height) = h.session.with_framebuffer(|fb| match fb {
                        Some(p) => (p.width() as usize, p.height() as usize),
                        None => (0, 0),
                    });
                    let f1 = frame1.as_ref().expect("frame 1 snapshot");
                    let diff = f1.iter().zip(&frame3).filter(|(a, b)| a != b).count();
                    // Physical/logical scale: the framebuffer is 600 logical
                    // points wide (ui::PANEL_WIDTH).
                    let scale = width as f32 / mybox_palette::ui::PANEL_WIDTH;
                    let (non_bg, bbox, kinds, aa_spread) =
                        glyph_structure(&frame3, width, height, scale);
                    if diff == 0 {
                        return Err(format!(
                            "Ime commits did not change the rendered text (frame diff=0) — \
                             egui-winit did not translate the injected Ime events; \
                             measured non_bg={non_bg} bbox={bbox:?} kinds={kinds} \
                             aa_spread={aa_spread}"
                        ));
                    }
                    let Some((bx0, by0, bx1, by1)) = bbox else {
                        return Err("frame 3 produced no non-background pixels".into());
                    };
                    let bw = bx1 - bx0 + 1;
                    let bh = by1 - by0 + 1;
                    let measured = format!(
                        "measured bbox={bw}x{bh}@({bx0},{by0}) non_bg={non_bg} \
                         diff={diff} kinds={kinds} aa_spread={aa_spread}"
                    );
                    if bw < 8 || bh < 8 {
                        return Err(format!(
                            "glyph bbox too small ({measured}; expected ≥8x8 physical px)"
                        ));
                    }
                    if kinds < 16 {
                        return Err(format!(
                            "bbox has too few distinct RGBA values ({measured}; \
                             a texture-less frame has single digits)"
                        ));
                    }
                    if aa_spread < AA_SPREAD_MIN {
                        return Err(format!(
                            "input text region shows no glyph stroke antialiasing \
                             ({measured}; solid blocks measured ≈40, real glyphs ≈242)"
                        ));
                    }
                    eprintln!("palette_checks glyph_shape: {measured} — glyph structure OK");
                    press_key(&s, &h.handle, &ui_lock, Key::Named(NamedKey::Escape));
                    stage = 3;
                    Ok(())
                }
                3 => {
                    match h.handle.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            if Some(id) != h.created_id {
                                return Err(format!(
                                    "ESC must destroy the created window ({id} != {:?})",
                                    h.created_id
                                ));
                            }
                        }
                        // Drain Redraw stragglers; keep polling.
                        _ => return Ok(()),
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("ESC must move the session to Hidden".into());
                    }
                    h.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 7: position stays fixed across filter shrink / restore / Executing
//     growth (GAP-3 + WR-01/WR-02 regression, 03-05) ─────────────────────────

/// Assert the session framebuffer covers the full window physical size
/// (WR-02 regression): width = `ui::PANEL_WIDTH`·scale, height = `logical_h`·scale.
fn assert_framebuffer_covers(h: &PaletteHarness, scale: f64, logical_h: f32) -> Result<(), String> {
    let (w, hgt) = h.session.with_framebuffer(|fb| match fb {
        Some(p) => (p.width(), p.height()),
        None => (0, 0),
    });
    let expected_w = (mybox_palette::ui::PANEL_WIDTH as f64 * scale).round() as u32;
    let expected_h = (f64::from(logical_h) * scale).round() as u32;
    if w != expected_w || hgt != expected_h {
        return Err(format!(
            "WR-02 regression: framebuffer {w}x{hgt} does not cover the window {expected_w}x{expected_h}"
        ));
    }
    Ok(())
}

/// Real-window position-stability probe (PAL-03 / GAP-3 + WR-01/WR-02, 03-05).
///
/// Drives the production `on_event_win` frame loop on a real window through
/// three geometry-affecting stages — (1) filter shrink: `set_input("alpha
/// one")` filters 5 commands down to 1 (height 320→128 logical); (2) input
/// restore: `set_input("")` grows back to 5 (128→320); (3) Executing growth:
/// the out-of-frame `set_executing` transition adds the 32px status line
/// (320→352). At every stage the probe asserts, against the WINDOW SERVER's
/// view: the outer position equals the summon origin EXACTLY (GAP-3: the old
/// re-centering pushed the panel down on shrink) and the session framebuffer
/// covers the full window physical size (WR-02: the old once-only allocation
/// left growth regions undrawn).
///
/// Coverage statement (kept honest): the probe drives the real window and the
/// real on_event_win closure — `request_inner_size`/`resize_framebuffer` are
/// exercised exactly as production calls them. The OS-level "visually does
/// not move" truth is re-verified by human UAT test 6 on the desktop.
fn check_position_stable_on_filter() -> Result<(), String> {
    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    // 5 fake commands whose names guarantee `set_input("alpha one")` hits ONLY
    // c0 ("Alpha One") — no other name/description contains the query.
    let registry = registry_with(vec![
        fake_command("c0", "Alpha One", &[], ok_runner()),
        fake_command("c1", "Beta Two", &[], ok_runner()),
        fake_command("c2", "Gamma Three", &[], ok_runner()),
        fake_command("c3", "Delta Four", &[], ok_runner()),
        fake_command("c4", "Epsilon Five", &[], ok_runner()),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let mut stage = 0u8;
    let mut polls = 0u32;
    let mut scale: f64 = 1.0;
    let mut origin: Option<(i32, i32)> = None;
    let mut gen: Option<u64> = None;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |h, _el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // Baseline frame: render once, then capture the origin
                    // position and scale before any geometry change.
                    h.inject(WindowEvent::RedrawRequested)?;
                    if h.non_background_pixels() == 0 {
                        return Err("frame must produce non-background pixels".into());
                    }
                    let window = h.window.as_ref().ok_or("window not realized")?;
                    scale = window.scale_factor();
                    origin = Some(h.window_outer_position()?);
                    s.set_input("alpha one"); // filtered=[0] → 128 logical height
                    stage = 1;
                    polls = 0;
                    Ok(())
                }
                1 => {
                    // Filter shrink: drive the frame loop (revision sync),
                    // then poll the window-server height.
                    h.inject(WindowEvent::RedrawRequested)?;
                    let target = (128.0 * scale).round() as u32;
                    match h.window_inner_size()? {
                        (_, ht) if ht == target => {}
                        (_, ht) => {
                            polls += 1;
                            if polls > 20 {
                                return Err(format!(
                                    "filter shrink never reached height {target} \
                                     (inner={ht}, outer={:?})",
                                    h.window_outer_position()
                                ));
                            }
                            return Ok(());
                        }
                    }
                    // GAP-3 direct regression: the outer position must equal
                    // the summon origin EXACTLY after the shrink.
                    let pos = h.window_outer_position()?;
                    if Some(pos) != origin {
                        return Err(format!(
                            "GAP-3 regression: position moved on filter shrink — {pos:?} != {origin:?}"
                        ));
                    }
                    assert_framebuffer_covers(h, scale, 128.0)?;
                    s.set_input(""); // restore 5 commands → 320 logical height
                    stage = 2;
                    polls = 0;
                    Ok(())
                }
                2 => {
                    // Restore growth: 128 → 320.
                    h.inject(WindowEvent::RedrawRequested)?;
                    let target = (320.0 * scale).round() as u32;
                    match h.window_inner_size()? {
                        (_, ht) if ht == target => {}
                        (_, ht) => {
                            polls += 1;
                            if polls > 20 {
                                return Err(format!(
                                    "restore growth never reached height {target} \
                                     (inner={ht}, outer={:?})",
                                    h.window_outer_position()
                                ));
                            }
                            return Ok(());
                        }
                    }
                    let pos = h.window_outer_position()?;
                    if Some(pos) != origin {
                        return Err(format!(
                            "position moved on restore growth — {pos:?} != {origin:?}"
                        ));
                    }
                    assert_framebuffer_covers(h, scale, 320.0)?;
                    // WR-01 regression: the Executing transition happens HERE,
                    // outside any injected frame (a driver tick, like the
                    // production KeyboardInput event) — the geometry revision
                    // counter must still drive the +32px growth.
                    gen = Some(s.generation());
                    assert!(s.set_executing(gen.unwrap(), "c0"), "Idle → Executing");
                    stage = 3;
                    polls = 0;
                    Ok(())
                }
                3 => {
                    // Executing growth: 320 → 352 (112 + 48·5).
                    h.inject(WindowEvent::RedrawRequested)?;
                    let target = (352.0 * scale).round() as u32;
                    match h.window_inner_size()? {
                        (_, ht) if ht == target => {}
                        (_, ht) => {
                            polls += 1;
                            if polls > 20 {
                                return Err(format!(
                                    "Executing growth never reached height {target} \
                                     (inner={ht}, outer={:?})",
                                    h.window_outer_position()
                                ));
                            }
                            return Ok(());
                        }
                    }
                    let pos = h.window_outer_position()?;
                    if Some(pos) != origin {
                        return Err(format!(
                            "position moved on Executing growth — {pos:?} != {origin:?}"
                        ));
                    }
                    assert_framebuffer_covers(h, scale, 352.0)?;
                    // Ok completion → Hidden. The probe has no runner:
                    // finalize Ok only lands the Hidden state and does NOT
                    // deliver the Destroy (production hops it via
                    // UiThreadProxy) — the probe simulates that hop by
                    // enqueueing the destroy here, on the same handle.
                    let created = h.created_id.ok_or("created window id missing")?;
                    assert_eq!(
                        s.finalize(gen.unwrap(), Ok(())),
                        Some(created),
                        "finalize Ok returns the window id to destroy"
                    );
                    h.handle.destroy(created);
                    stage = 4;
                    polls = 0;
                    Ok(())
                }
                4 => {
                    match h.handle.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            let created = h.created_id.ok_or("created window id missing")?;
                            if id != created {
                                return Err(format!(
                                    "finalize must destroy the created window ({id} != {created})"
                                ));
                            }
                        }
                        // Drain Redraw stragglers; keep polling.
                        _ => {
                            polls += 1;
                            if polls > 20 {
                                return Err("Destroy never enqueued after finalize Ok".into());
                            }
                            return Ok(());
                        }
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("finalize Ok must move the session to Hidden".into());
                    }
                    h.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 8: hover highlight + click alignment on a real window (GAP-4 /
//     GAP-5 regression, 03-06) ──────────────────────────────────────────────

/// Real-window hover/click alignment probe (PAL-04 / GAP-4 / GAP-5, 03-06).
///
/// Injects synthetic pointer events (`CursorMoved` / `MouseInput` — winit
/// exposes these structs for external construction, unlike `KeyEvent`) through
/// the real window + the production `on_event_win` closure, so the full chain
/// egui-winit translation → egui hit-testing → `Response::clicked()` →
/// `execute::execute` runs exactly as production drives it:
///
/// - stage 0: baseline Idle frame; capture the window scale factor.
/// - stage 1: inject `CursorMoved` at row 1's center (logical (300, 92) — the
///   row band is y 68..116 below the 12..60 input box + 8px gap), render the
///   hover frame, then measure the composited framebuffer (physical space,
///   scale-converted):
///   * `hover_px_in_band` ≥ 100 — the ROW_HOVERED fill (#2E2E2E) covers the
///     row band (GAP-4: interaction + painting now share the content
///     coordinate system, the highlight sits exactly on the row rect);
///   * `hover_px_above_band` == 0 — no highlight pixel in the 8px band above
///     the row (between the input box bottom and row 1's top): the direct
///     "highlight floats above the text area" regression;
///   * `text_px_in_band` > 0 — row text (non-chrome) pixels live inside the
///     same band, so highlight and text overlap.
///   Then inject `MouseInput` Pressed.
/// - stage 2: render one frame with the button down; inject Released.
/// - stage 3: render the frame that processes the release — this is the frame
///   where the click fires. Assert `Executing` and that the gated runner has
///   NOT completed (the click routed through execute with the re-entrancy
///   guard — GAP-5: the old hover-only sense could never produce a click).
///   Release the runner gate.
/// - stage 4: poll for the finalize Destroy (the real `UiThreadProxy` hop),
///   assert `Hidden` + the runner ran exactly once.
///
/// Coverage statement (kept honest): the probe drives the real window and the
/// real on_event_win closure with synthetic pointer events — everything from
/// the egui-winit translation through the clicked→execute chain is exercised.
/// OS-level physical mouse behavior (real cursor tracking, click targeting on
/// the desktop) is re-verified by human UAT tests 7/8.
fn check_hover_click_alignment() -> Result<(), String> {
    use mybox_core::winit::event::{DeviceId, ElementState, MouseButton};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    // c0 carries a gated runner so the click→execute transition is observable
    // deterministically (Executing + counter==0 before the gate release).
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let counter = Arc::new(AtomicUsize::new(0));
    let registry = registry_with(vec![
        fake_command(
            "c0",
            "Alpha One",
            &[],
            gated_runner(Arc::clone(&counter), release_rx),
        ),
        fake_command("c1", "Beta Two", &[], ok_runner()),
        fake_command("c2", "Gamma Three", &[], ok_runner()),
        fake_command("c3", "Delta Four", &[], ok_runner()),
        fake_command("c4", "Epsilon Five", &[], ok_runner()),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let mut stage = 0u8;
    let mut scale: f64 = 1.0;
    let mut polls = 0u32;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |h, _el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // Baseline Idle frame renders 5 rows (registers the row
                    // widgets for the next frame's hit-test).
                    h.inject(WindowEvent::RedrawRequested)?;
                    if h.non_background_pixels() == 0 {
                        return Err("Idle frame must produce non-background pixels".into());
                    }
                    let window = h.window.as_ref().ok_or("window not realized")?;
                    scale = window.scale_factor();
                    // Hover row 1's center: logical (300, 92) — row band
                    // y 68..116 (input box 12..60 + 8px gap, 48px rows).
                    h.inject(WindowEvent::CursorMoved {
                        device_id: DeviceId::dummy(),
                        position: mybox_core::winit::dpi::PhysicalPosition::new(
                            300.0 * scale,
                            92.0 * scale,
                        ),
                    })?;
                    stage = 1;
                    Ok(())
                }
                1 => {
                    // Render the hover frame, then measure the composited
                    // framebuffer (physical space, scale-converted).
                    h.inject(WindowEvent::RedrawRequested)?;
                    let (width, height, data) = h.session.with_framebuffer(|fb| match fb {
                        Some(p) => (p.width(), p.height(), p.data().to_vec()),
                        None => (0, 0, vec![]),
                    });
                    let y_band_top = (68.0 * scale).round() as usize;
                    let y_band_bottom = (116.0 * scale).round() as usize;
                    let y_above_top = (60.0 * scale).round() as usize;
                    // ui.rs chrome tokens: BG #202020, ROW_HOVERED #2E2E2E,
                    // ROW_SELECTED #404040 (composited framebuffer, opaque).
                    let is_hover_fill =
                        |p: &[u8]| p[3] > 0 && p[0] == 46 && p[1] == 46 && p[2] == 46;
                    let is_chrome = |p: &[u8]| {
                        (p[0] == 32 && p[1] == 32 && p[2] == 32)
                            || (p[0] == 46 && p[1] == 46 && p[2] == 46)
                            || (p[0] == 64 && p[1] == 64 && p[2] == 64)
                    };
                    let mut hover_px_in_band = 0usize;
                    let mut hover_px_above_band = 0usize;
                    let mut text_px_in_band = 0usize;
                    for y in 0..height as usize {
                        for x in 0..width as usize {
                            let i = (y * width as usize + x) * 4;
                            let p = &data[i..i + 4];
                            if is_hover_fill(p) {
                                if (y_band_top..y_band_bottom).contains(&y) {
                                    hover_px_in_band += 1;
                                } else if (y_above_top..y_band_top).contains(&y) {
                                    hover_px_above_band += 1;
                                }
                            }
                            if (y_band_top..y_band_bottom).contains(&y) && p[3] > 0 && !is_chrome(p)
                            {
                                text_px_in_band += 1;
                            }
                        }
                    }
                    let measured = format!(
                        "hover_px_in_band={hover_px_in_band} \
                         hover_px_above_band={hover_px_above_band} \
                         text_px_in_band={text_px_in_band} scale={scale}"
                    );
                    if hover_px_in_band < 100 {
                        return Err(format!(
                            "GAP-4 regression: hover fill does not cover the row band \
                             ({measured}; the row fill must produce ≥100 exact-#2E2E2E pixels)"
                        ));
                    }
                    if hover_px_above_band != 0 {
                        return Err(format!(
                            "GAP-4 regression: hover fill bleeds above the row band ({measured})"
                        ));
                    }
                    if text_px_in_band == 0 {
                        return Err(format!(
                            "GAP-4 regression: no row text inside the hovered band ({measured})"
                        ));
                    }
                    eprintln!(
                        "palette_checks hover_click_alignment: {measured} — \
                         hover aligned with the row band"
                    );
                    h.inject(WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                    })?;
                    stage = 2;
                    Ok(())
                }
                2 => {
                    // One frame while the button is down, then release.
                    h.inject(WindowEvent::RedrawRequested)?;
                    h.inject(WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: ElementState::Released,
                        button: MouseButton::Left,
                    })?;
                    stage = 3;
                    Ok(())
                }
                3 => {
                    // The release frame computes the click → execute. The
                    // gated runner blocks, so the session stays Executing and
                    // the counter is still 0 — the click DID route through
                    // execute (GAP-5: the old hover-only sense never could).
                    h.inject(WindowEvent::RedrawRequested)?;
                    if s.state() != PaletteState::Executing {
                        return Err(format!(
                            "GAP-5 regression: the click must enter Executing, got {:?}",
                            s.state()
                        ));
                    }
                    if counter.load(Ordering::SeqCst) != 0 {
                        return Err("the gated runner must not complete before release".into());
                    }
                    let _ = release_tx.send(());
                    stage = 4;
                    polls = 0;
                    Ok(())
                }
                4 => {
                    // The real finalize hop (AppEvent::Ui) destroys the window.
                    match h.handle.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            if Some(id) != h.created_id {
                                return Err(format!(
                                    "finalize must destroy the created window ({id} != {:?})",
                                    h.created_id
                                ));
                            }
                        }
                        // Drain Redraw stragglers; keep polling.
                        _ => {
                            polls += 1;
                            if polls > 20 {
                                return Err(
                                    "Destroy never enqueued after the click execution".into()
                                );
                            }
                            return Ok(());
                        }
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("finalize Ok must close the panel".into());
                    }
                    if counter.load(Ordering::SeqCst) != 1 {
                        return Err("the clicked runner must run exactly once".into());
                    }
                    h.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 9: Ctrl+P/N navigation equals ↑/↓ (GAP-6, 03-07) ────────────────

/// Real-window Ctrl+P/N navigation probe (PAL-04 / GAP-6, 03-07).
///
/// Injects a REAL `WindowEvent::ModifiersChanged` through the production
/// `on_event_win` closure — the probe's core coverage point is the event
/// stream → `session.set_modifiers` wiring that production relies on (winit
/// 0.30's KeyEvent has no modifiers field; the variant carries
/// `event::Modifiers`, converted via `Modifiers::from(ModifiersState)`).
/// Then drives the shared key router with Ctrl+P/N and asserts the
/// ↑/↓-equivalent wrap-around behavior:
///
/// - stage 0: baseline Idle frame; inject `ModifiersChanged(CONTROL)` through
///   the real closure and assert `session.modifiers().control_key()`.
/// - stage 1: `press_key_mods("p", CONTROL)` in Idle (no selection) wraps to
///   the LAST entry (Some(2), same as ↑); `press_key_mods("n", CONTROL)` then
///   wraps back to Some(0) (same as ↓).
/// - stage 2: inject `ModifiersChanged(empty)` through the real closure and
///   assert the session state cleared; `press_key(Escape)` (the unmodified
///   path — press_key regression) closes the panel.
/// - stage 3: poll for the paired Destroy + Hidden residue assertions.
///
/// Coverage statement (kept honest): the probe covers the real
/// ModifiersChanged → session wiring and the router's Ctrl behavior. The OS
/// physical Ctrl+P keypress → winit ModifiersChanged/KeyboardInput event
/// stream is re-verified by human UAT test 9 on the desktop.
fn check_ctrl_pn_navigation() -> Result<(), String> {
    use mybox_core::winit::event::Modifiers;
    use mybox_core::winit::keyboard::{Key, ModifiersState, NamedKey};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    // 3 fake commands: Idle with no selection → Ctrl+P wraps to the last (2).
    let registry = registry_with(vec![
        fake_command("c0", "Alpha One", &[], ok_runner()),
        fake_command("c1", "Beta Two", &[], ok_runner()),
        fake_command("c2", "Gamma Three", &[], ok_runner()),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let ui_lock = Arc::clone(&ui_proxy);
    let mut stage = 0u8;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |h, _el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // Baseline frame, then the REAL ModifiersChanged event
                    // flows through the production on_event_win closure into
                    // session.set_modifiers (the GAP-6 wiring under test).
                    h.inject(WindowEvent::RedrawRequested)?;
                    if h.non_background_pixels() == 0 {
                        return Err("Idle frame must produce non-background pixels".into());
                    }
                    h.inject(WindowEvent::ModifiersChanged(Modifiers::from(
                        ModifiersState::CONTROL,
                    )))?;
                    if !s.modifiers().control_key() {
                        return Err(format!(
                            "ModifiersChanged(CONTROL) must reach session.modifiers, got {:?}",
                            s.modifiers()
                        ));
                    }
                    stage = 1;
                    Ok(())
                }
                1 => {
                    // Ctrl+P in Idle (no selection) is equivalent to ↑: wrap
                    // to the LAST entry (index 2). Then Ctrl+N wraps forward
                    // to index 0.
                    if !press_key_mods(
                        &s,
                        &h.handle,
                        &ui_lock,
                        Key::Character("p".into()),
                        ModifiersState::CONTROL,
                    ) {
                        return Err("Ctrl+P must be consumed by the router".into());
                    }
                    if s.selection() != Some(2) {
                        return Err(format!(
                            "Ctrl+P in Idle must wrap to the last entry (↑-equivalent), got {:?}",
                            s.selection()
                        ));
                    }
                    if !press_key_mods(
                        &s,
                        &h.handle,
                        &ui_lock,
                        Key::Character("n".into()),
                        ModifiersState::CONTROL,
                    ) {
                        return Err("Ctrl+N must be consumed by the router".into());
                    }
                    if s.selection() != Some(0) {
                        return Err(format!(
                            "Ctrl+N at the last index must wrap to 0 (↓-equivalent), got {:?}",
                            s.selection()
                        ));
                    }
                    stage = 2;
                    Ok(())
                }
                2 => {
                    // Clear the modifiers through the REAL event stream, then
                    // run the unmodified press_key path (ESC — press_key
                    // regression: it must still pass ModifiersState::empty()).
                    h.inject(WindowEvent::ModifiersChanged(Modifiers::from(
                        ModifiersState::empty(),
                    )))?;
                    if s.modifiers() != ModifiersState::empty() {
                        return Err(format!(
                            "ModifiersChanged(empty) must clear session.modifiers, got {:?}",
                            s.modifiers()
                        ));
                    }
                    press_key(&s, &h.handle, &ui_lock, Key::Named(NamedKey::Escape));
                    stage = 3;
                    Ok(())
                }
                3 => {
                    match h.handle.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            if Some(id) != h.created_id {
                                return Err(format!(
                                    "ESC must destroy the created window ({id} != {:?})",
                                    h.created_id
                                ));
                            }
                        }
                        // Drain Redraw stragglers; keep polling.
                        _ => return Ok(()),
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("ESC must move the session to Hidden".into());
                    }
                    h.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 10: Ime Commit → input → filter + explicit IME flag (GAP-7,
//     03-08) ──────────────────────────────────────────────────────────────────

/// Real-window IME commit probe (PAL-03 / GAP-7, 03-08).
///
/// Injects synthetic `WindowEvent::Ime` events (`Ime::Preedit` / `Ime::Commit`
/// — winit exposes these for external construction, unlike `KeyEvent`)
/// through the real window + the production `on_event_win` closure, so the
/// full chain egui-winit translation → egui `Event::Ime` → TextEdit insert →
/// `input_resp.changed()` → `session.set_input` runs exactly as production
/// drives it:
///
/// - stage 0: baseline Idle frame (the first production-closure invocation
///   triggers `ensure_winit_state` — assert the GAP-7 explicit IME-enable
///   flag was set through the REAL closure, this probe's core coverage
///   point); inject `Ime::Preedit("测", None)` (the OS sends Preedit before
///   Commit during pinyin composition).
/// - stage 1: process the Preedit frame; inject `Ime::Commit("截图")`.
/// - stage 2: commit frame — assert the committed Chinese text reached
///   `session.input` ("截图"), moved the state to Filtering, and filtered to
///   [0] (capture.start's name tier). Then `set_input("tuichu")` asserts the
///   GAP-7 no-IME prefix-discovery path at the session level (the pinyin
///   keyword alias hits builtin.quit → filtered [1]). ESC closes.
/// - stage 3: poll for the paired Destroy + Hidden residue assertions, then
///   re-summon through the production `summon_palette` path — preparation
///   for the GAP-8 re-summon coverage below.
/// - stage 4 (GAP-8, 03-09): reset + re-set evidence — assert
///   `s.ime_allowed() == false` right before the second window's first
///   event (the summon() reset evidence), then `h.inject(RedrawRequested)`
///   to run the second window's first event through the real production
///   closure (which re-enters `ensure_winit_state`, rebuilds the
///   egui-winit State for the new winit Window, and re-issues
///   `window.set_ime_allowed(true)`), and assert
///   `s.ime_allowed() == true` (re-set path evidence — REVIEW WR-01's
///   exact defect path that 03-08's probe missed).
/// - stage 5: zero-regression — inject `Ime::Preedit("重新截图")` + Redraw
///   + `Ime::Commit("截图")` + Redraw on the second window's freshly-built
///   egui-winit State (the "重新截图" candidate buffer preedits via the
///   fresh State's set_ime_cursor_area path; the "截图" commit matches
///   `开始截图`'s name tier — the same chain stage 2 drives on the first
///   window, replayed on the second window's fresh State); assert
///   `s.input() == "截图"`, `s.state() == Filtering`, and
///   `s.filtered() == [0]`.
/// - stage 6: ESC closes the second window — poll for the second
///   Destroy (created_id is the SECOND window's id after stage 3's
///   realize_window), then Hidden + no-live-window + no-pending-close
///   residue assertions and `pass()`.
///
/// Coverage statement (kept honest, with the re-summon gap closed — GAP-8,
/// 03-09): the probe injects synthetic Ime events and covers the winit
/// event → egui-winit → TextEdit → session chain plus the ime_allowed
/// flag on BOTH the first window (stages 0-2) AND a re-summoned SECOND
/// window (stages 4-5) — the reset → re-set code path that 03-08's probe
/// left uncovered is now exercised through the real production closure
/// (the GAP-8 coverage-hole fix). The OS input-method composition window
/// (candidate window) appearance/interaction on BOTH the first and the
/// re-summoned second window can only be re-verified by human UAT test 10
/// on the desktop (the re-summon scenario) — this probe locks the
/// code-level IME re-enable path so the 03-08 "10/10 green with a real
/// defect" gap cannot recur.
fn check_ime_commit_updates_input() -> Result<(), String> {
    use mybox_core::winit::event::Ime;
    use mybox_core::winit::keyboard::{Key, NamedKey};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    // 2 commands mirroring the production inventory: capture.start (name
    // "开始截图", keywords incl. "jietu") + builtin.quit (name "退出应用",
    // keywords incl. the Task-1 GAP-7 pinyin alias "tuichu").
    let registry = registry_with(vec![
        fake_command(
            "capture.start",
            "开始截图",
            &["截图", "capture", "screen", "jietu"],
            ok_runner(),
        ),
        fake_command(
            "builtin.quit",
            "退出应用",
            &["退出", "quit", "exit", "tuichu"],
            ok_runner(),
        ),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    // GAP-8 re-summon (03-09): the closure re-invokes `summon_palette` on a
    // SECOND window — mirrors consecutive_summon_close (Arc clones of the
    // handle AND registry enter the closure scope so the re-summon call
    // sites match the production summon_palette(&s, &h, &registry_lock,
    // &ui_lock) signature). Captured `h` is the WindowManagerHandle Arc;
    // the closure param is renamed to `harness` (the PaletteHarness) so the
    // stage 3 re-summon can call harness.realize_window(el) and read
    // harness.created_id exactly like consecutive_summon_close stages 1.
    let h = Arc::clone(&handle);
    let ui_lock = Arc::clone(&ui_proxy);
    let registry_lock = Arc::clone(&registry);
    let mut stage = 0u8;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |harness, el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // First frame: the first production-closure invocation
                    // triggers ensure_winit_state — the GAP-7 explicit IME
                    // enable must have happened through the real closure.
                    harness.inject(WindowEvent::RedrawRequested)?;
                    if harness.non_background_pixels() == 0 {
                        return Err("Idle frame must produce non-background pixels".into());
                    }
                    if !s.ime_allowed() {
                        return Err(
                            "GAP-7: the first window event must set the ime_allowed flag".into(),
                        );
                    }
                    // Start an IME composition (the OS sends Preedit before
                    // Commit during pinyin composition).
                    harness.inject(WindowEvent::Ime(Ime::Preedit("测".to_string(), None)))?;
                    stage = 1;
                    Ok(())
                }
                1 => {
                    // Frame that processes the Preedit, then commit.
                    harness.inject(WindowEvent::RedrawRequested)?;
                    harness.inject(WindowEvent::Ime(Ime::Commit("截图".to_string())))?;
                    stage = 2;
                    Ok(())
                }
                2 => {
                    // Commit frame: egui-winit → egui Event::Ime → TextEdit
                    // insert → input_resp.changed → session.set_input. The
                    // committed Chinese text must have reached the session
                    // and triggered filtering.
                    harness.inject(WindowEvent::RedrawRequested)?;
                    if s.input() != "截图" {
                        return Err(format!(
                            "GAP-7: Ime Commit must reach session.input, got {:?}",
                            s.input()
                        ));
                    }
                    if s.state() != PaletteState::Filtering {
                        return Err(format!(
                            "committed text must transition to Filtering, got {:?}",
                            s.state()
                        ));
                    }
                    if s.filtered() != vec![0] {
                        return Err(format!(
                            "\"截图\" must filter to [0] (capture.start), got {:?}",
                            s.filtered()
                        ));
                    }
                    // GAP-7 prefix discovery at the session level: the pinyin
                    // keyword alias hits the Chinese builtin without an IME.
                    s.set_input("tuichu");
                    if s.filtered() != vec![1] {
                        return Err(format!(
                            "\"tuichu\" must hit builtin.quit via the pinyin keyword, got {:?}",
                            s.filtered()
                        ));
                    }
                    press_key(&s, &h, &ui_lock, Key::Named(NamedKey::Escape));
                    stage = 3;
                    Ok(())
                }
                // ─── Re-summon extension (03-09, GAP-8 / REVIEW WR-01) ─────
                3 => {
                    // ESC Destroy already enqueued by stage 2's press_key;
                    // drain the handle for the paired Destroy, assert Hidden
                    // + no-live-window + no-pending-close residue, then
                    // re-summon through the production summon_palette path
                    // (mirrors consecutive_summon_close). summon() resets
                    // ime_allowed=false + winit_state=None (Task 1) so the
                    // SECOND window's first event re-enters ensure_winit_state
                    // and re-issues window.set_ime_allowed(true).
                    match h.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            if Some(id) != harness.created_id {
                                return Err(format!(
                                    "ESC must destroy the first window ({id} != {:?})",
                                    harness.created_id
                                ));
                            }
                        }
                        // Drain Redraw stragglers; keep polling.
                        _ => return Ok(()),
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("ESC must move the first-window session to Hidden".into());
                    }
                    if s.has_live_window() {
                        return Err("no live window may survive the first close".into());
                    }
                    if s.consume_pending_close() {
                        return Err("pending_close residue after the first close".into());
                    }
                    // Re-summon through the production path on a SECOND window.
                    summon_palette(&s, &h, &registry_lock, &ui_lock)
                        .map_err(|e| format!("re-summon: {e}"))?;
                    let spec = expect_create(&h)?;
                    harness.pending_spec = Some(spec);
                    harness.realize_window(el)?;
                    stage = 4;
                    Ok(())
                }
                // ─── Core GAP-8 coverage: reset → re-set evidence ────────
                4 => {
                    // Before the second window's first event fires through
                    // the production closure, ime_allowed must be observable
                    // in its reset state — `false` (summon() reset evidence).
                    // GAP-8 reset assertion (03-09): s.ime_allowed() == false
                    // right before the second window's first event — the
                    // exact observable contract that 03-08's probe could
                    // not lock (a per-session ime_allowed stays true and
                    // the defect hides).
                    if s.ime_allowed() {
                        return Err(
                            "GAP-8: re-summon must reset ime_allowed=false before the second window's first event".into(),
                        );
                    }
                    // First event for the second window: ensure_winit_state
                    // re-enters the `if !inner.ime_allowed` guard (summon
                    // reset both ime_allowed and winit_state), builds a fresh
                    // egui-winit State for the new winit Window, and issues
                    // window.set_ime_allowed(true) on it.
                    harness.inject(WindowEvent::RedrawRequested)?;
                    // GAP-8 re-set assertion (03-09): s.ime_allowed() == true
                    // after the second window's first event re-entered
                    // ensure_winit_state through the real production closure
                    // — the re-set path REVIEW WR-01 said was missing.
                    if !s.ime_allowed() {
                        return Err(
                            "GAP-8: re-summon must re-set ime_allowed=true on the first event of the second window".into(),
                        );
                    }
                    stage = 5;
                    Ok(())
                }
                // ─── Zero-regression: second-window Chinese IME flow ──────
                5 => {
                    // The second window's freshly-built egui-winit State
                    // must accept Chinese IME events through the same winit
                    // → egui-winit → TextEdit → set_input chain (the 03-09
                    // VERIFICATION gaps[0].missing[1] suggestion). Use a
                    // "重新截图" ("re-screenshot") Preedit (the composition
                    // candidate buffer OS input methods display during pinyin
                    // jin) followed by a "截图" Commit — the committed text
                    // enters session.set_input and matches 开始截图's name
                    // tier (the same chain stage 2 exercises on the first
                    // window, now replayed on the second window's fresh
                    // egui-winit State).
                    harness.inject(WindowEvent::Ime(Ime::Preedit("重新截图".to_string(), None)))?;
                    harness.inject(WindowEvent::RedrawRequested)?;
                    harness.inject(WindowEvent::Ime(Ime::Commit("截图".to_string())))?;
                    harness.inject(WindowEvent::RedrawRequested)?;
                    if s.input() != "截图" {
                        return Err(format!(
                            "GAP-8: second-window IME Commit must reach session.input, got {:?}",
                            s.input()
                        ));
                    }
                    if s.state() != PaletteState::Filtering {
                        return Err(format!(
                            "GAP-8: second-window IME Commit must transition to Filtering, got {:?}",
                            s.state()
                        ));
                    }
                    if s.filtered() != vec![0] {
                        return Err(format!(
                            "GAP-8: second-window IME Commit must filter to [0] (capture.start), got {:?}",
                            s.filtered()
                        ));
                    }
                    stage = 6;
                    Ok(())
                }
                // ─── Convergence: close the second window ─────────────────
                6 => {
                    // ESC closes the second window through the production
                    // close path; created_id is the SECOND window's id
                    // (realize_window updated it at the end of stage 3).
                    press_key(&s, &h, &ui_lock, Key::Named(NamedKey::Escape));
                    match h.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            if Some(id) != harness.created_id {
                                return Err(format!(
                                    "GAP-8: ESC must destroy the second window ({id} != {:?})",
                                    harness.created_id
                                ));
                            }
                        }
                        // Drain Redraw stragglers; keep polling.
                        _ => return Ok(()),
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("GAP-8: ESC must move the second-window session to Hidden".into());
                    }
                    if s.has_live_window() {
                        return Err("GAP-8: no live window may survive the second close".into());
                    }
                    if s.consume_pending_close() {
                        return Err("GAP-8: pending_close residue after the second close".into());
                    }
                    harness.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 11: keyword-tier tag highlight (Gap 1 / UAT test 5, 03-10) ───────

/// IN-06: named layout geometry — band top = input box (SP_MD 12..60)
/// + 8px gap (SP_SM), band bottom = one 48px row later (SP_2XL).
/// Derived from ui.rs's SP_* so a layout change breaks the build here
/// instead of silently shifting the band (CR-01 lesson).
const ROW_BAND_TOP_LOGICAL: f64 =
    (mybox_palette::ui::SP_MD + mybox_palette::ui::SP_2XL + mybox_palette::ui::SP_SM) as f64; // 12+48+8 = 68
const ROW_BAND_BOTTOM_LOGICAL: f64 = ROW_BAND_TOP_LOGICAL + mybox_palette::ui::SP_2XL as f64; // 68+48 = 116

/// The exact ACCENT token (UI-SPEC #FF6000) — ui.rs's ACCENT constant.
const ACCENT_RGB: (u8, u8, u8) = (0xFF, 0x60, 0x00);

/// Count ACCENT-ish (#FF6000 ± tolerance) pixels and their y-extent.
///
/// The keyword tag's matched glyphs paint pure ACCENT in their interiors
/// (opaque coverage → premultiplied == straight); the tolerance absorbs
/// per-platform rasterization differences (exact #FF6000 matching was
/// macOS-calibrated — 04-01 CI: Windows AA spread differs, so the scan is
/// tolerant around ACCENT_RGB). Returns `(band_count, full_frame_count,
/// min_y, max_y)` — band = row 1 (logical y 68..116: input box 12..60 +
/// 8px gap, 48px rows); the full-frame diagnostics distinguish "tag not
/// painted at all" from "painted at a different y than macOS".
///
/// IN-06: presence-only assertion — it cannot detect an x/y offset inside
/// the band (CR-01 lesson); position blindness is a documented limitation,
/// not a fix target (REVIEW "at minimum" tier).
fn accent_pixels_in_frame(
    h: &PaletteHarness,
    scale: f64,
) -> (usize, usize, Option<usize>, Option<usize>) {
    let (width, height, data) = h.session.with_framebuffer(|fb| match fb {
        Some(p) => (p.width(), p.height(), p.data().to_vec()),
        None => (0, 0, vec![]),
    });
    let y_top = (ROW_BAND_TOP_LOGICAL * scale).round() as usize;
    let y_bottom = (ROW_BAND_BOTTOM_LOGICAL * scale).round() as usize;
    let mut band = 0usize;
    let mut full = 0usize;
    let mut min_y = None;
    let mut max_y = None;
    for y in 0..height as usize {
        for x in 0..width as usize {
            let i = (y * width as usize + x) * 4;
            // ACCENT (255, 96, 0) ± tolerance; excludes white text (G≈255),
            // the #202020 card (R<224) and any dim tag-tint blend (R<224).
            if data[i + 3] > 0
                && data[i] >= 0xE0
                && (0x40..=0x80).contains(&data[i + 1])
                && data[i + 2] <= 0x20
            {
                full += 1;
                min_y = Some(min_y.map_or(y, |m: usize| m.min(y)));
                max_y = Some(max_y.map_or(y, |m: usize| m.max(y)));
                if (y_top..y_bottom).contains(&y) {
                    band += 1;
                }
            }
        }
    }
    (band, full, min_y, max_y)
}

/// Real-window keyword-tier highlight probe (PAL-03 / Gap 1 / UAT test 5,
/// 03-10).
///
/// Drives the production `on_event_win` frame loop on a real window with a
/// registry mirroring the production inventory (capture.start FIRST, then the
/// four builtins with their pinyin keyword aliases). Two Filtering stages type
/// pinyin queries and assert the rendered keyword tag paints exact #FF6000
/// (ACCENT) pixels inside row 1's band:
///
/// - stage 0: baseline Idle frame (5 rows, the summon height — no resize);
///   capture the window scale factor.
/// - stage 1: `set_input("jt")` — "jt" is a subsequence of capture.start's
///   "jietu" pinyin keyword → filtered == [0] (capture.start first, the UAT 5
///   "命中排前" half). TWO Redraw frames settle the geometry sync: the first
///   Filtering frame's 320→128 shrink reallocates the framebuffer (wiping the
///   just-painted content), so the SECOND frame paints the " · jietu" tag at
///   the new size. The band scan then asserts ACCENT pixels > 0.
/// - stage 2: `set_input("tuichu")` — builtin.quit via its pinyin keyword →
///   filtered == [1]; the same band scan asserts ACCENT pixels again (the
///   WHOLE keyword tier renders the tag, not just capture.start —
///   UAT gaps[0].missing[2]).
/// - stage 3: ESC closes with the paired Destroy + Hidden residue assertions.
///
/// Coverage statement (kept honest): the probe locks the render path end to
/// end (real window + real frame loop + exact ACCENT pixels in the session
/// framebuffer); the filter-layer keyword index assertions are covered by the
/// Task-1 unit tests (all five pinyin keywords); the OS-level "the user's eye
/// sees the orange highlight" truth is re-verified by human UAT test 5.
fn check_keyword_highlight() -> Result<(), String> {
    use mybox_core::winit::keyboard::{Key, NamedKey};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    // Registry mirroring the production inventory (registration order keeps
    // capture.start FIRST — the same order as command.rs). All ok_runner: no
    // runner observability is needed — this probe locks the RENDER path.
    let registry = registry_with(vec![
        fake_command(
            "capture.start",
            "开始截图",
            &["截图", "capture", "screen", "jietu"],
            ok_runner(),
        ),
        fake_command(
            "builtin.quit",
            "退出应用",
            &["退出", "quit", "exit", "tuichu"],
            ok_runner(),
        ),
        fake_command(
            "builtin.open_config",
            "打开配置目录",
            &["配置", "config", "peizhi"],
            ok_runner(),
        ),
        fake_command(
            "builtin.restart",
            "重启应用",
            &["重启", "restart", "chongqi"],
            ok_runner(),
        ),
        fake_command(
            "builtin.open_log",
            "打开日志文件",
            &["日志", "log", "rizhi"],
            ok_runner(),
        ),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let ui_lock = Arc::clone(&ui_proxy);
    let mut stage = 0u8;
    let mut scale: f64 = 1.0;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |h, _el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // Baseline Idle frame (5 rows, 320 logical height — the
                    // summon size, so no resize/realloc wipe on this frame).
                    h.inject(WindowEvent::RedrawRequested)?;
                    if h.non_background_pixels() == 0 {
                        return Err("Idle frame must render".into());
                    }
                    let window = h.window.as_ref().ok_or("window not realized")?;
                    scale = window.scale_factor();
                    s.set_input("jt");
                    stage = 1;
                    Ok(())
                }
                1 => {
                    // Filtering "jt" → capture.start via the "jietu" pinyin
                    // keyword (filtered [0]). TWO Redraw frames: the first
                    // Filtering frame runs the geometry sync (320→128 logical
                    // height) which reallocates the framebuffer and wipes the
                    // just-painted content — the second frame paints the
                    // keyword tag at the new size.
                    h.inject(WindowEvent::RedrawRequested)?;
                    h.inject(WindowEvent::RedrawRequested)?;
                    if s.state() != PaletteState::Filtering {
                        return Err(format!(
                            "set_input must transition to Filtering, got {:?}",
                            s.state()
                        ));
                    }
                    if s.filtered() != vec![0] {
                        return Err(format!(
                            "jt must filter capture.start to position 0, got {:?}",
                            s.filtered()
                        ));
                    }
                    let (accent, full, min_y, max_y) = accent_pixels_in_frame(h, scale);
                    if accent == 0 {
                        return Err(format!(
                            "jt must render #FF6000 accent pixels in row 1's band \
                             (the \" · jietu\" keyword tag) — measured {accent} band / \
                             {full} frame px @scale {scale} y-range {min_y:?}..{max_y:?}"
                        ));
                    }
                    eprintln!(
                        "palette_checks keyword_highlight: jt stage measured \
                         {accent} ACCENT px in row 1's band ({full} frame px)"
                    );
                    s.set_input("tuichu");
                    stage = 2;
                    Ok(())
                }
                2 => {
                    // "tuichu" → builtin.quit via its pinyin keyword (filtered
                    // [1]) — the WHOLE keyword tier renders the tag, not just
                    // capture.start. Same height (1 row → 128 logical), so a
                    // single frame suffices (the second inject guards against
                    // any stray realloc — harmless repaint).
                    h.inject(WindowEvent::RedrawRequested)?;
                    h.inject(WindowEvent::RedrawRequested)?;
                    if s.filtered() != vec![1] {
                        return Err(format!(
                            "tuichu must filter builtin.quit to position 1, got {:?}",
                            s.filtered()
                        ));
                    }
                    let (accent, full, min_y, max_y) = accent_pixels_in_frame(h, scale);
                    if accent == 0 {
                        return Err(format!(
                            "tuichu must render #FF6000 accent pixels in row 1's band \
                             (the \" · tuichu\" keyword tag) — measured {accent} band / \
                             {full} frame px @scale {scale} y-range {min_y:?}..{max_y:?}"
                        ));
                    }
                    eprintln!(
                        "palette_checks keyword_highlight: tuichu stage measured \
                         {accent} ACCENT px in row 1's band ({full} frame px)"
                    );
                    press_key(&s, &h.handle, &ui_lock, Key::Named(NamedKey::Escape));
                    stage = 3;
                    Ok(())
                }
                3 => {
                    match h.handle.try_recv() {
                        Some(WindowRequest::Destroy(id)) => {
                            if Some(id) != h.created_id {
                                return Err(format!(
                                    "ESC must destroy the created window ({id} != {:?})",
                                    h.created_id
                                ));
                            }
                        }
                        // Drain Redraw stragglers; keep polling.
                        _ => return Ok(()),
                    }
                    if s.state() != PaletteState::Hidden {
                        return Err("ESC must move the session to Hidden".into());
                    }
                    if s.has_live_window() {
                        return Err("no live window may survive the close".into());
                    }
                    h.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Check 12: click path hides the window before the read-screen (Gap 2 /
//     UAT test 11, 03-10) ──────────────────────────────────────────────────

/// Real-window click-path hide-before-capture probe (PAL-04/PAL-05 / Gap 2 /
/// UAT test 11, 03-10).
///
/// Drives the full production click chain on a real window with synthetic
/// pointer events (CursorMoved / MouseInput — winit exposes these for external
/// construction): egui-winit translation → egui hit-testing →
/// `Response::clicked()` → `execute::execute` with `hide_before_execute` —
/// exactly the mouse path that photographed the panel into the screenshot
/// (UAT 11). capture.start carries a GATED runner (the read-screen side
/// simulated — the counter only increments after the gate releases):
///
/// - stage 0: baseline Idle frame (registers row 1's widget); capture the
///   window scale; assert the BASELINE window-server visibility
///   `is_visible() == Some(true)` (a window that starts hidden would make the
///   stage-3 hide assertion vacuous).
/// - stage 1: inject `CursorMoved` at row 1's center (logical (300, 92)) and
///   render the hover frame.
/// - stage 2: inject `MouseInput` Pressed, render one frame, inject Released.
/// - stage 3 (the core coverage point): the release frame computes the click →
///   execute → session.close (Hidden + Destroy enqueued) → the Task-3 Hidden
///   guard synchronously hides the window (`set_visible(false)` + early
///   return — no paint, no present, no request_redraw). Assert: session
///   Hidden; the WINDOW SERVER reports `is_visible() == Some(false)` (macOS
///   orderOut is immediate — the panel is off-screen before the Destroy
///   drains or the read-screen runs); the gated runner has NOT started
///   (counter == 0 — the read-screen never saw the panel); the Destroy for
///   the created window is already enqueued.
/// - stage 4: release the gate → the runner completes exactly once → after
///   the finalize hop settles, NO second Destroy (the hidden panel's
///   finalize is a generation/state-guarded no-op — capture_hides_first same
///   assertion).
///
/// Coverage statement (kept honest): the probe drives the real window + the
/// real on_event_win closure; `is_visible() == Some(false)` locks the
/// window-server view of the hide BEFORE the gated read-screen starts. The
/// OS compositor-level "the screenshot never contains the panel" truth is
/// re-verified by human UAT test 11 on the desktop.
fn check_click_hide_before_capture() -> Result<(), String> {
    use mybox_core::winit::event::{DeviceId, ElementState, MouseButton};

    let session = Arc::new(PaletteSession::new());
    let handle = Arc::new(WindowManagerHandle::new());
    let ui_proxy = Arc::new(OnceLock::new());
    // capture.start FIRST (row 1 in Idle): hide_before_execute + a GATED
    // runner — the "window hidden before the read-screen" ordering is
    // observable deterministically (counter == 0 at the hide, 1 after the
    // release).
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let counter = Arc::new(AtomicUsize::new(0));
    let registry = registry_with(vec![
        Command {
            id: "capture.start",
            name: "开始截图".to_string(),
            description: "capture fake".to_string(),
            keywords: vec!["jietu"],
            hide_before_execute: true,
            runner: gated_runner(Arc::clone(&counter), release_rx),
        },
        fake_command(
            "builtin.quit",
            "退出应用",
            &["退出", "quit", "exit", "tuichu"],
            ok_runner(),
        ),
        fake_command(
            "builtin.open_config",
            "打开配置目录",
            &["配置", "config", "peizhi"],
            ok_runner(),
        ),
        fake_command(
            "builtin.restart",
            "重启应用",
            &["重启", "restart", "chongqi"],
            ok_runner(),
        ),
        fake_command(
            "builtin.open_log",
            "打开日志文件",
            &["日志", "log", "rizhi"],
            ok_runner(),
        ),
    ]);
    summon_palette(&session, &handle, &registry, &ui_proxy).map_err(|e| format!("summon: {e}"))?;
    let spec = expect_create(&handle)?;

    let s = Arc::clone(&session);
    let mut stage = 0u8;
    let mut polls = 0u32;
    let harness = PaletteHarness::new(
        Arc::clone(&session),
        Arc::clone(&handle),
        spec,
        Box::new(move |h, _el, event| {
            let WindowEvent::RedrawRequested = event else { return Ok(()); };
            match stage {
                0 => {
                    // Baseline Idle frame renders 5 rows (registers the row
                    // widgets for the next frame's hit-test).
                    h.inject(WindowEvent::RedrawRequested)?;
                    if h.non_background_pixels() == 0 {
                        return Err("Idle frame must render".into());
                    }
                    let window = h.window.as_ref().ok_or("window not realized")?;
                    let scale = window.scale_factor();
                    // Baseline visibility: the window must START visible or the
                    // stage-3 Some(false) assertion is vacuous.
                    match window.is_visible() {
                        Some(true) => {}
                        other => {
                            return Err(format!(
                                "baseline: the window must start visible \
                                 (is_visible() == Some(true)), got {other:?}"
                            ));
                        }
                    }
                    // Hover row 1's center: logical (300, 92) — row band
                    // y 68..116 (input box 12..60 + 8px gap, 48px rows).
                    h.inject(WindowEvent::CursorMoved {
                        device_id: DeviceId::dummy(),
                        position: mybox_core::winit::dpi::PhysicalPosition::new(
                            300.0 * scale,
                            92.0 * scale,
                        ),
                    })?;
                    stage = 1;
                    Ok(())
                }
                1 => {
                    // Hover frame: egui hit-tests the PREVIOUS frame's widgets
                    // (row 1 registered in stage 0) — the hover state sticks.
                    h.inject(WindowEvent::RedrawRequested)?;
                    h.inject(WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                    })?;
                    stage = 2;
                    Ok(())
                }
                2 => {
                    // One frame while the button is down, then release.
                    h.inject(WindowEvent::RedrawRequested)?;
                    h.inject(WindowEvent::MouseInput {
                        device_id: DeviceId::dummy(),
                        state: ElementState::Released,
                        button: MouseButton::Left,
                    })?;
                    stage = 3;
                    Ok(())
                }
                3 => {
                    // The release frame computes the click → execute →
                    // hide_before_execute → session.close (Hidden + Destroy
                    // enqueued) → the Task-3 Hidden guard synchronously hides
                    // the window (set_visible(false) + early return — no paint,
                    // no present, no request_redraw). The capture chain's
                    // screen read must happen AFTER this frame: the window
                    // server already reports it hidden.
                    h.inject(WindowEvent::RedrawRequested)?;
                    if s.state() != PaletteState::Hidden {
                        return Err(format!(
                            "the click must close the panel (Hidden), got {:?}",
                            s.state()
                        ));
                    }
                    // Honest calibration: `is_visible() == Some(false)` on
                    // macOS (orderOut is immediate); a None/Some(true) result
                    // fails with the actual value printed.
                    let vis = h.window.as_ref().ok_or("window not realized")?.is_visible();
                    match vis {
                        Some(false) => {}
                        other => {
                            return Err(format!(
                                "window server must report the window hidden \
                                 (is_visible() == Some(false)) before the read-screen \
                                 runner starts — got {other:?} (honest calibration)"
                            ));
                        }
                    }
                    if counter.load(Ordering::SeqCst) != 0 {
                        return Err(
                            "the gated read-screen runner must not have started \
                             before the window hide"
                                .into(),
                        );
                    }
                    // Drain the request queue deterministically: close()
                    // enqueues the Destroy synchronously inside the click
                    // frame (execute → session.close → windows.destroy), so a
                    // bounded drain loop must reach it WITHOUT relying on
                    // further RedrawRequested dispatches. On Windows a hidden
                    // window stops dispatching WM_PAINT (request_redraw →
                    // RedrawWindow is a no-op for hidden windows), which
                    // stalled the old one-request-per-event poll forever (the
                    // 04-01 CI watchdog); macOS keeps dispatching redraws
                    // after orderOut, which is why the probe only failed on
                    // Windows. Redraw stragglers from earlier stages sit ahead
                    // of the Destroy in the FIFO queue and are dropped here.
                    let mut destroyed = None;
                    loop {
                        match h.handle.try_recv() {
                            Some(WindowRequest::Destroy(id)) => {
                                destroyed = Some(id);
                                break;
                            }
                            Some(req) => {
                                eprintln!(
                                    "palette_checks click_hide_before_capture: drained \
                                     straggler {}",
                                    request_name(Some(&req))
                                );
                            }
                            None => break,
                        }
                    }
                    match destroyed {
                        Some(id) if Some(id) == h.created_id => {}
                        other => {
                            return Err(format!(
                                "the click must destroy the created window \
                                 (queue drained to {other:?}, created {:?})",
                                h.created_id
                            ));
                        }
                    }
                    let _ = release_tx.send(());
                    stage = 4;
                    polls = 0;
                    Ok(())
                }
                4 => {
                    // Released in stage 3: the gated runner completes on its
                    // worker thread and the finalize hop (UiThreadProxy →
                    // AppEvent::Ui) lands on the main thread. Poll the
                    // counter, let the hop settle, then assert no second
                    // Destroy (the hidden panel's finalize is a
                    // generation/state-guarded no-op — capture_hides_first
                    // same assertion).
                    if counter.load(Ordering::SeqCst) != 1 {
                        polls += 1;
                        if polls > 20 {
                            return Err("the gated runner must complete after release".into());
                        }
                        return Ok(());
                    }
                    polls += 1;
                    if polls < 4 {
                        return Ok(()); // let the finalize hop settle
                    }
                    if let Some(req) = h.handle.try_recv() {
                        return Err(format!(
                            "no second Destroy expected after a hidden-panel completion, \
                             got {}",
                            request_name(Some(&req))
                        ));
                    }
                    h.pass();
                    Ok(())
                }
                _ => Ok(()),
            }
        }),
    );
    run_harness(harness, &ui_proxy)
}

// ─── Entry point (exit 0 ok / 1 fail / 2 usage — 02-04 discipline) ──────────

fn main() {
    let check = std::env::args().nth(1).unwrap_or_default();
    let result = match check.as_str() {
        "summon_render" => check_summon_render(),
        "fuzzy_navigation_execute" => check_fuzzy_navigation_execute(),
        "capture_hides_first" => check_capture_hides_palette_first(),
        "five_summon_esc_no_residue" => check_five_summon_esc_no_residue(),
        "consecutive_summon_close" => check_consecutive_summon_close(),
        "glyph_shape" => check_glyph_shape(),
        "position_stable_on_filter" => check_position_stable_on_filter(),
        "hover_click_alignment" => check_hover_click_alignment(),
        "ctrl_pn_navigation" => check_ctrl_pn_navigation(),
        "ime_commit_updates_input" => check_ime_commit_updates_input(),
        "keyword_highlight" => check_keyword_highlight(),
        "click_hide_before_capture" => check_click_hide_before_capture(),
        _ => {
            eprintln!(
                "usage: palette_checks <summon_render|fuzzy_navigation_execute|capture_hides_first|five_summon_esc_no_residue|consecutive_summon_close|glyph_shape|position_stable_on_filter|hover_click_alignment|ctrl_pn_navigation|ime_commit_updates_input|keyword_highlight|click_hide_before_capture>"
            );
            std::process::exit(2);
        }
    };
    match result {
        Ok(()) => println!("palette_checks '{check}': OK"),
        Err(e) => {
            eprintln!("palette_checks '{check}': FAILED: {e}");
            std::process::exit(1);
        }
    }
}
