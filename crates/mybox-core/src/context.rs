//! ModuleContext — the only facade modules see of the core (FRMW-02).
//!
//! Holds `Arc`-backed references to core services. Accessor methods
//! (`emit`/`on`/`ui`/`windows` land in plan 01-02; `config`/`hotkeys` in
//! plan 01-03). This plan provides the fields and the `pub(crate)` constructor.

use std::sync::Arc;

use crate::config::ConfigCenter;
use crate::event::EventBus;
use crate::hotkey::HotkeyManager;
use crate::window::WindowManagerHandle;

/// The object handed to `Module::init`. Modules interact with the framework
/// exclusively through this context.
#[allow(dead_code)]
pub struct ModuleContext {
    pub(crate) bus: Arc<EventBus>,
    pub(crate) windows: Arc<WindowManagerHandle>,
    pub(crate) config: Arc<ConfigCenter>,
    pub(crate) hotkeys: Arc<HotkeyManager>,
    pub(crate) ui: UiThreadProxy,
}

impl ModuleContext {
    /// Construct a context from the core services (core-internal).
    pub(crate) fn new(
        bus: Arc<EventBus>,
        windows: Arc<WindowManagerHandle>,
        config: Arc<ConfigCenter>,
        hotkeys: Arc<HotkeyManager>,
        ui: UiThreadProxy,
    ) -> Self {
        Self {
            bus,
            windows,
            config,
            hotkeys,
            ui,
        }
    }
}

/// Forwards closures to the winit main thread (D-04 reconciliation with winit's
/// main-thread-bound windows). Backed by `EventLoopProxy`; implemented in
/// plan 01-02. For now it is an empty, default-constructible shell.
#[derive(Default)]
pub struct UiThreadProxy;
