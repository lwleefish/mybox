//! System tray + context menu (INFRA-02).
//!
//! Full implementation (menu assembly from module `menu_items()`, tray build)
//! lands in plan 01-03. This file currently holds the default-constructible
//! shell consumed by `ModuleContext`.

/// Owns the tray icon and the shared context menu.
#[derive(Default)]
pub struct TrayManager;
