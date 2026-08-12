//! Window types (FRMW-03).
//!
//! Window abstraction data models are defined in plan 01-01-05. This file
//! currently holds the main-thread-bound handle shell consumed by ModuleContext.

/// Main-thread-bound handle to the window manager. Modules enqueue window
/// creation requests through this; the winit loop executes them on the main
/// thread (RESEARCH §2.3). Implementation lands in plan 01-02.
#[derive(Default)]
pub struct WindowManagerHandle;
