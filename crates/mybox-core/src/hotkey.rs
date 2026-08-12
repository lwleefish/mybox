//! Global hotkey manager (FRMW-04).
//!
//! Full implementation (config-driven registration, id->action map, event
//! emission) lands in plan 01-03. This file currently holds the
//! default-constructible shell consumed by `ModuleContext`.

/// Registers and tracks global hotkeys; emits bus events on trigger.
#[derive(Default)]
pub struct HotkeyManager;
