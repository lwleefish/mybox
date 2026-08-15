//! Command execution lifecycle (PAL-04 / D-04 / D-05): the Enter-to-runner
//! path.
//!
//! `execute` transitions the session to Executing (re-entrancy guard), enqueues
//! the window Destroy BEFORE the runner for `hide_before_execute` commands (the
//! capture exception — Pitfall 4: the single FIFO request channel guarantees
//! the Destroy drains before any later Create, so the panel can never appear in
//! a screenshot), then hands the command to core's `run_command` (named worker
//! thread + `pollster::block_on` + `UiThreadProxy` hop — RESEARCH Pattern 4).
//! The completion closure finalizes through the session's generation guard
//! (Pitfall 3) and destroys the window on success or requests a redraw so the
//! Error state renders (Pitfall 8: `ControlFlow::Wait` only redraws on
//! request).

use std::sync::Arc;

use mybox_core::command::{run_command, Command};
use mybox_core::log;
use mybox_core::window::WindowManagerHandle;
use mybox_core::UiThreadProxy;

use crate::session::{PaletteSession, PaletteState};

/// Execute a command from the palette.
///
/// `session` is the palette session; `windows` is the shared handle whose
/// request channel the App drains on the main thread (the standalone handle in
/// headless tests). The runner never touches winit — it runs on its own named
/// worker thread and only the finalize hop (through `ui`) returns to the main
/// thread (FRMW-05, T-3-04).
pub fn execute(
    session: &Arc<PaletteSession>,
    ui: &UiThreadProxy,
    windows: &Arc<WindowManagerHandle>,
    cmd: Command,
) {
    let cmd_id = cmd.id;
    let gen = session.generation();
    if !session.set_executing(gen, cmd_id) {
        // Anti-reentrancy (D-04): not Idle/Filtering or a stale generation.
        log::warn!("palette: command '{cmd_id}' ignored — not idle/filtering");
        return;
    }
    if cmd.hide_before_execute {
        // Destroy enqueues BEFORE the runner runs (Pitfall 4 — queue-order
        // guarantee, T-3-06). The capture runner itself also captures before
        // creating overlays, so the ordering is doubly safe.
        if let Some(id) = session.close() {
            windows.destroy(id);
        }
    }
    let session = Arc::clone(session);
    let windows = Arc::clone(windows);
    run_command(
        cmd,
        ui,
        Box::new(move |result| {
            if let Err(e) = &result {
                log::warn!("palette: command '{cmd_id}' failed: {e:#}");
            }
            match session.finalize(gen, result) {
                Some(id) => windows.destroy(id),
                None => {
                    if session.state() == PaletteState::Error {
                        // Err branch: the panel stays open and must repaint the
                        // error state (Pitfall 8 — no redraw without request).
                        if let Some(id) = session.window_id() {
                            windows.redraw(id);
                        }
                    }
                    // Otherwise: stale completion or a hide_before_execute
                    // panel that is already gone — nothing to touch.
                }
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
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

    /// A fake command whose runner increments `count` and then resolves.
    fn counting_command(id: &'static str, count: Arc<AtomicUsize>) -> Command {
        Command {
            id,
            name: format!("command {id}"),
            description: "test".to_string(),
            keywords: vec![],
            hide_before_execute: false,
            runner: Arc::new(move || {
                let c = Arc::clone(&count);
                Box::pin(async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        }
    }

    #[test]
    fn execute_enters_executing_and_finalizes_ok() {
        let session = Arc::new(PaletteSession::new());
        let gen = session.summon(vec![counting_command("a", Arc::new(AtomicUsize::new(0)))]);
        let windows = Arc::new(WindowManagerHandle::new());
        let ui = UiThreadProxy::new();

        let count = Arc::new(AtomicUsize::new(0));
        let cmd = counting_command("fake.cmd", Arc::clone(&count));
        execute(&session, &ui, &windows, cmd);

        assert!(wait_until(|| count.load(Ordering::SeqCst) == 1), "runner never ran");
        assert_eq!(session.state(), PaletteState::Executing, "Executing while the runner runs");
        // Headless: the finalize closure is stashed in the proxy (no loop yet).
        // Drive the completion directly (the production hop does exactly this).
        session.set_window_id(3);
        let id = session.finalize(gen, Ok(()));
        assert_eq!(id, Some(3), "finalize Ok returns the window to destroy");
        assert_eq!(session.state(), PaletteState::Hidden);
    }

    #[test]
    fn execute_failure_sets_error_state() {
        let session = Arc::new(PaletteSession::new());
        let gen = session.summon(vec![counting_command("a", Arc::new(AtomicUsize::new(0)))]);
        let windows = Arc::new(WindowManagerHandle::new());
        let ui = UiThreadProxy::new();

        let count = Arc::new(AtomicUsize::new(0));
        let cmd = Command {
            id: "failing.cmd",
            name: "failing".to_string(),
            description: "test".to_string(),
            keywords: vec![],
            hide_before_execute: false,
            runner: {
                let c = Arc::clone(&count);
                Arc::new(move || {
                    let c = Arc::clone(&c);
                    Box::pin(async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        mybox_core::anyhow::bail!("spawn failed: no such binary")
                    })
                })
            },
        };
        execute(&session, &ui, &windows, cmd);

        assert!(wait_until(|| count.load(Ordering::SeqCst) == 1), "runner never ran");
        session.set_window_id(3);
        assert_eq!(session.finalize(gen, Err(mybox_core::anyhow::anyhow!("spawn failed"))), None);
        assert_eq!(session.state(), PaletteState::Error, "failure keeps the panel open (D-05)");
        assert!(
            session.error().as_deref().unwrap_or_default().contains("spawn failed"),
            "error text surfaced in-panel"
        );
        assert_eq!(session.window_id(), Some(3), "window stays for the error display");
    }

    #[test]
    fn hide_before_execute_destroys_before_runner_runs() {
        // Pitfall 4 / T-3-06: the Destroy must be enqueued BEFORE the runner
        // starts — the capture command's panel can never be photographed.
        let session = Arc::new(PaletteSession::new());
        let gen = session.summon(vec![counting_command("a", Arc::new(AtomicUsize::new(0)))]);
        session.set_window_id(42);
        let windows = Arc::new(WindowManagerHandle::new());
        let ui = UiThreadProxy::new();

        // The runner blocks on a gate so we can observe the queue mid-execute;
        // the counter only increments AFTER the gate — before release it is
        // deterministically 0. (The receiver rides in a Mutex: `Receiver` is
        // Send but not Sync, and the runner closure must be Send + Sync.)
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let release_rx = std::sync::Mutex::new(release_rx);
        let runner_ran = Arc::new(AtomicUsize::new(0));
        let cmd = Command {
            id: "capture.start",
            name: "开始截图".to_string(),
            description: "test".to_string(),
            keywords: vec!["jietu"],
            hide_before_execute: true,
            runner: {
                let r = Arc::clone(&runner_ran);
                let rx = Arc::new(release_rx);
                Arc::new(move || {
                    let r = Arc::clone(&r);
                    let rx = Arc::clone(&rx);
                    Box::pin(async move {
                        rx.lock().unwrap().recv().ok(); // gate — blocks until released
                        r.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                })
            },
        };
        execute(&session, &ui, &windows, cmd);

        // While the runner is blocked: the FIRST queued request must be the
        // Destroy, and the runner must not have completed.
        assert!(
            wait_until(|| matches!(windows.try_recv(), Some(mybox_core::WindowRequest::Destroy(42)))),
            "Destroy must be enqueued before the runner runs"
        );
        assert_eq!(runner_ran.load(Ordering::SeqCst), 0, "runner must not complete before Destroy");
        assert_eq!(session.state(), PaletteState::Hidden, "panel closed before the runner");

        // Release: the runner completes, but the panel is already closed —
        // finalize is a no-op (wrong state) and no second Destroy is enqueued.
        let _ = release_tx.send(());
        assert!(
            wait_until(|| runner_ran.load(Ordering::SeqCst) == 1),
            "runner must eventually complete"
        );
        // Headless: the finalize hop is stashed (no event-loop proxy), so no
        // completion-side request can appear — assert the queue stays clean.
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            windows.try_recv().is_none(),
            "no second Destroy after a hide_before_execute completion"
        );
        // The generation guard itself is exercised in the session unit tests;
        // here the close-before-runner ordering is the contract under test.
        let _ = gen;
    }

    #[test]
    fn stale_finalize_does_not_touch_new_window() {
        // Pitfall 3 via the public API: finalize for a closed+re-summoned
        // palette is a no-op on the session (the completion closure then
        // enqueues nothing).
        let session = Arc::new(PaletteSession::new());
        let gen1 = session.summon(vec![counting_command("a", Arc::new(AtomicUsize::new(0)))]);
        session.set_executing(gen1, "a");
        let gen2 = session.summon(vec![counting_command("a", Arc::new(AtomicUsize::new(0)))]);
        session.set_window_id(9);
        assert_ne!(gen1, gen2);
        assert_eq!(session.finalize(gen1, Ok(())), None, "stale finalize returns no destroy target");
        assert_eq!(session.state(), PaletteState::Idle, "new palette untouched");
        assert_eq!(session.window_id(), Some(9), "new window survives");
    }

    #[test]
    fn execute_ignored_when_not_idle_or_filtering() {
        // D-04: a command cannot start from Executing/Empty/Hidden.
        let session = Arc::new(PaletteSession::new());
        let gen = session.summon(vec![counting_command("a", Arc::new(AtomicUsize::new(0)))]);
        session.set_executing(gen, "a");
        let windows = Arc::new(WindowManagerHandle::new());
        let ui = UiThreadProxy::new();
        let count = Arc::new(AtomicUsize::new(0));
        execute(&session, &ui, &windows, counting_command("b", Arc::clone(&count)));
        assert_eq!(count.load(Ordering::SeqCst), 0, "runner must not start while Executing");
    }
}
