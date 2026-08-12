//! mybox-core — framework core for the mybox modular desktop toolbox.
//!
//! Zero business logic. This crate defines the module extension contract, the
//! event model, window/hotkey/tray/config service types, the renderer
//! abstraction, and the unified error type. Feature modules depend only on the
//! public API re-exported here (FRMW-02).

pub mod config;
pub mod context;
pub mod error;
pub mod event;
pub mod hotkey;
pub mod module;
pub mod renderer;
pub mod tray;
pub mod window;

pub use config::{config_dir, config_file_path, ConfigCenter};
pub use context::{ModuleContext, UiThreadProxy};
pub use error::{MyboxError, Result};
pub use event::{Event, EventBus, EventFilter, EventPayload, FrameworkEvent, SubscriptionId};
pub use hotkey::HotkeyManager;
pub use module::{Module, ModuleRegistry};
pub use renderer::Renderer;
pub use tray::TrayManager;
pub use window::{
    window_attributes, WindowId, WindowKind, WindowManager, WindowManagerHandle, WindowRequest,
    WindowSpec, WindowState,
};
