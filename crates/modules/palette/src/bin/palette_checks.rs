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
//! Usage: `palette_checks <summon_render|fuzzy_navigation_execute|capture_hides_first|five_summon_esc_no_residue|consecutive_summon_close|glyph_shape|position_stable_on_filter>`
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
    /// previous window (dropping it closes it — its Destroy was already
    /// asserted by the check). Also pairs the session window id through the
    /// spec's `on_created` callback, which is what production's
    /// `App::create_window` does via `spec.on_created` (GAP-1 fix).
    fn realize_window(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let spec = self
            .pending_spec
            .as_ref()
            .ok_or("no pending spec to realize")?;
        let attrs = window_attributes(spec);
        let window =
            Arc::new(el.create_window(attrs).map_err(|e| format!("create window: {e}"))?);
        let winit_id = window.id();
        let id = self.wm.next_id();
        let renderer = TinySkiaSoftbufferRenderer::new(Arc::clone(&window))
            .map_err(|e| format!("renderer: {e}"))?;
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
                    if aa_spread < 120 {
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
        _ => {
            eprintln!(
                "usage: palette_checks <summon_render|fuzzy_navigation_execute|capture_hides_first|five_summon_esc_no_residue|consecutive_summon_close|glyph_shape|position_stable_on_filter|hover_click_alignment>"
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
