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
//! Usage: `palette_checks <summon_render|fuzzy_navigation_execute|capture_hides_first|five_summon_esc_no_residue>`
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
    /// asserted by the check). Also pairs the session window id, which is what
    /// production's `window-created` handler does.
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
        self.session.set_window_id(id);
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
/// shared entry point both the closure and this harness drive.
fn press_key(
    session: &Arc<PaletteSession>,
    handle: &Arc<WindowManagerHandle>,
    ui_proxy: &Arc<OnceLock<UiThreadProxy>>,
    key: mybox_core::winit::keyboard::Key,
) -> bool {
    on_palette_key(session, handle, ui_proxy, &key)
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

// ─── Entry point (exit 0 ok / 1 fail / 2 usage — 02-04 discipline) ──────────

fn main() {
    let check = std::env::args().nth(1).unwrap_or_default();
    let result = match check.as_str() {
        "summon_render" => check_summon_render(),
        "fuzzy_navigation_execute" => check_fuzzy_navigation_execute(),
        "capture_hides_first" => check_capture_hides_palette_first(),
        "five_summon_esc_no_residue" => check_five_summon_esc_no_residue(),
        _ => {
            eprintln!(
                "usage: palette_checks <summon_render|fuzzy_navigation_execute|capture_hides_first|five_summon_esc_no_residue>"
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
