//! macOS Screen Recording permission preflight (CAP-08).

use mybox_core::log;

/// Injectable access checker (function pointer) so tests can simulate denial.
pub type AccessChecker = fn() -> bool;

/// Real OS check: query Screen Recording permission. Non-macOS hosts have no
/// such gate, so they always report granted — the rest of the module still
/// compiles and runs cross-platform.
#[cfg(target_os = "macos")]
pub fn real_access_checker() -> bool {
    // Safe wrapper: objc2-core-graphics already performs the `unsafe` FFI call
    // internally (generated binding).
    objc2_core_graphics::CGPreflightScreenCaptureAccess()
}

#[cfg(not(target_os = "macos"))]
pub fn real_access_checker() -> bool {
    true
}

/// Gate capture on the injected checker; returns the verdict. Modules call this
/// with `real_access_checker` in production and a fake in tests.
pub fn check_access(access: AccessChecker) -> bool {
    access()
}

/// Trigger the macOS Screen Recording authorization prompt (CAP-08). Returns
/// whether access is granted; on some macOS versions this only opens System
/// Settings or requires a restart (A1). Non-macOS hosts have no gate.
#[cfg(target_os = "macos")]
pub fn request_access() -> bool {
    // Safe wrapper — the generated binding performs the `unsafe` FFI internally.
    objc2_core_graphics::CGRequestScreenCaptureAccess()
}

#[cfg(not(target_os = "macos"))]
pub fn request_access() -> bool {
    true
}

/// Open the System Settings Screen Recording pane via the deep link (A7):
/// `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`.
///
/// The command is a compile-time constant (no user input — T-2-14) and is
/// spawned through `Command::new("open")` without a shell (no injection
/// surface). Non-macOS hosts are a no-op.
#[cfg(target_os = "macos")]
pub fn open_system_settings() {
    const SCREEN_CAPTURE_PANE: &str =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
    if let Err(e) = std::process::Command::new("open")
        .arg(SCREEN_CAPTURE_PANE)
        .spawn()
    {
        log::error!("failed to open System Settings Screen Recording pane: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn open_system_settings() {
    // No gate on other platforms.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_access_delegates_to_the_injected_checker() {
        assert!(check_access(|| true));
        assert!(!check_access(|| false));
    }
}
