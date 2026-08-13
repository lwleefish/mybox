//! macOS Screen Recording permission preflight (CAP-08).

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
