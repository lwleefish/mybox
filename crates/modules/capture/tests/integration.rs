//! Display / OS integration tests for the capture module (plan 02-04-03).
//!
//! Every test here is `#[ignore]`: they need a real macOS GUI session (and, for
//! the clipboard check, Screen Recording permission). They are excluded from
//! `cargo nextest run`; run them explicitly with:
//!
//! ```text
//! cargo test -- --ignored -p mybox-capture
//! ```
//!
//! ## Why these tests spawn a helper binary
//!
//! winit on macOS requires the `EventLoop` to be created on the **real main
//! thread** (`MainThreadMarker` panics otherwise) and allows only one
//! `EventLoop` per process. Rust's `cargo test` harness runs each `#[test]` on
//! a spawned worker thread, so the checks cannot create an `EventLoop` inline.
//! Each test therefore spawns the `capture_checks` binary (see
//! `crates/modules/capture/src/bin/capture_checks.rs`) in its own process,
//! where the check runs on that process's main thread (mirrors
//! `crates/mybox-core/tests/integration.rs`).

use std::process::Command;

/// Path to the built `capture_checks` binary (cargo sets `CARGO_BIN_EXE_*` for
/// integration tests of the same package).
const CHECKS_BIN: &str = env!("CARGO_BIN_EXE_capture_checks");

/// Run one capture check in a fresh subprocess and assert it exited cleanly.
fn run_check(name: &str) {
    let status = Command::new(CHECKS_BIN)
        .arg(name)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn capture_checks '{name}': {e}"));
    assert!(
        status.success(),
        "capture_checks '{name}' exited with {status:?}"
    );
}

/// Test 1 — an Overlay window composites a capture + mask and presents (CAP-02).
#[test]
#[ignore]
fn overlay_window_composites_and_presents() {
    run_check("overlay_capture");
}

/// Test 2 — the drag-select state machine reaches Selected with a selection
/// (CAP-03).
#[test]
#[ignore]
fn drag_select_reaches_selected_state() {
    run_check("drag_selection");
}

/// Test 3 — the confirm flow crops + copies to the clipboard and reads back the
/// correct dimensions (CAP-04).
#[test]
#[ignore]
fn enter_confirm_copies_selection_to_clipboard() {
    run_check("enter_clipboard");
}

/// Test 4 — ESC/cancel drains the overlay ids exactly once (CAP-05).
#[test]
#[ignore]
fn esc_cancel_destroys_overlays_idempotently() {
    run_check("esc_destroy");
}
