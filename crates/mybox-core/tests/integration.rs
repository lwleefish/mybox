//! Display / OS integration tests for the mybox framework (plan 01-04-05).
//!
//! Every test here is `#[ignore]`: they need a real macOS GUI session and, for
//! the hotkey test, the user's cooperation (a free Cmd+Shift+T). They are
//! excluded from `cargo nextest run`; run them explicitly with:
//!
//! ```text
//! cargo test -p mybox-core -- --ignored
//! ```
//!
//! ## Why these tests spawn a helper binary
//!
//! winit on macOS requires the `EventLoop` to be created on the **real main
//! thread** (`MainThreadMarker` panics otherwise — see `winit`'s
//! `platform_impl/macos/event_loop.rs`) and allows only one `EventLoop` per
//! process. Rust's `cargo test` harness runs each `#[test]` on a spawned
//! worker thread, so the checks cannot create an `EventLoop` inline. Each test
//! therefore spawns the `display_checks` binary (see
//! `crates/mybox-core/src/bin/display_checks.rs`) in its own process, where the
//! check runs on that process's main thread (W2 / RESEARCH §2.4/§2.5).

use std::process::Command;

/// Path to the built `display_checks` binary (cargo sets `CARGO_BIN_EXE_*` for
/// integration tests of the same package).
const CHECKS_BIN: &str = env!("CARGO_BIN_EXE_display_checks");

/// Run one display check in a fresh subprocess and assert it exited cleanly.
fn run_check(name: &str) {
    let status = Command::new(CHECKS_BIN)
        .arg(name)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn display_checks '{name}': {e}"));
    assert!(
        status.success(),
        "display_checks '{name}' exited with {status:?}"
    );
}

/// Test 1 — a Panel window is created and its first `RedrawRequested` presents
/// without panicking (FRMW-03, D-02 present pipeline).
#[test]
#[ignore]
fn panel_window_creates_and_presents() {
    run_check("panel");
}

/// Test 2 — an Overlay window (transparent + undecorated + always-on-top) is
/// created and registered (FRMW-03 profile; real alpha is Phase 2, RESEARCH §0.5).
#[test]
#[ignore]
fn overlay_window_creates() {
    run_check("overlay");
}

/// Test 3 — the global hotkey manager initializes and a config string
/// registers successfully, returning a positive id (FRMW-04, D-11).
#[test]
#[ignore]
fn hotkey_init_and_register_succeeds() {
    run_check("hotkey");
}

/// Test 4 — the tray builds with the runtime-generated icon and the menu with
/// module items (INFRA-02; needs a live macOS menu-bar session).
#[test]
#[ignore]
fn tray_build_succeeds() {
    run_check("tray");
}
