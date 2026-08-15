//! Display / OS integration tests for the palette module (plan 03-02-03).
//!
//! Every test here is `#[ignore]`: they need a real macOS GUI session. They are
//! excluded from `cargo nextest run`; run them explicitly with:
//!
//! ```text
//! cargo test -- --ignored -p mybox-palette
//! ```
//!
//! ## Why these tests spawn a helper binary
//!
//! winit on macOS requires the `EventLoop` to be created on the **real main
//! thread** (`MainThreadMarker` panics otherwise) and allows only one
//! `EventLoop` per process. Rust's `cargo test` harness runs each `#[test]` on
//! a spawned worker thread, so the checks cannot create an `EventLoop` inline.
//! Each test therefore spawns the `palette_checks` binary (see
//! `crates/modules/palette/src/bin/palette_checks.rs`) in its own process,
//! where the check runs on that process's main thread (mirrors
//! `crates/modules/capture/tests/integration.rs`, 02-04).

use std::process::Command;

/// Path to the built `palette_checks` binary (cargo sets `CARGO_BIN_EXE_*` for
/// integration tests of the same package).
const CHECKS_BIN: &str = env!("CARGO_BIN_EXE_palette_checks");

/// Run one palette check in a fresh subprocess and assert it exited cleanly.
fn run_check(name: &str) {
    let status = Command::new(CHECKS_BIN)
        .arg(name)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn palette_checks '{name}': {e}"));
    assert!(
        status.success(),
        "palette_checks '{name}' exited with {status:?}"
    );
}

/// Test 1 — summon creates a Floating window, the frame loop rasterizes real
/// content into the framebuffer, the render chain presents, and ESC closes
/// with a paired Destroy (PAL-01/05 render path).
#[test]
#[ignore]
fn palette_summon_render() {
    run_check("summon_render");
}

/// Test 2 — fuzzy filter reorders the list (name tier before keyword tier),
/// ArrowDown selects filtered position 1, Enter executes the command mapped
/// through `filtered` (idx 2, not idx 1 — the Filtering-reorder regression),
/// and the real finalize hop destroys the window (PAL-03/04).
#[test]
#[ignore]
fn palette_fuzzy_navigation_execute() {
    run_check("fuzzy_navigation_execute");
}

/// Test 3 — a `hide_before_execute` command enqueues the Destroy BEFORE the
/// runner runs, and produces no second Destroy after completion (the capture
/// screenshot-order hard constraint, Pitfall 4).
#[test]
#[ignore]
fn palette_capture_hides_first() {
    run_check("capture_hides_first");
}

/// Test 4 — five consecutive summon/ESC rounds pair 5 Creates with 5 Destroys,
/// advance the generation 5 times, and leave zero residue (the Phase 2
/// re-entrancy lesson generalized, PAL-05).
#[test]
#[ignore]
fn palette_five_summon_esc_no_residue() {
    run_check("five_summon_esc_no_residue");
}
