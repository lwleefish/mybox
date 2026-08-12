//! mybox-core — framework core for the mybox modular desktop toolbox.
//!
//! Zero business logic. This crate defines the module extension contract, the
//! event model, window/hotkey/tray/config service types, the renderer
//! abstraction, and the unified error type. Feature modules depend only on the
//! public API re-exported here (FRMW-02).

pub mod app;
pub mod config;
pub mod context;
pub mod error;
pub mod event;
pub mod hotkey;
pub mod module;
pub mod renderer;
pub mod tray;
pub mod window;

pub use app::{App, AppBuilder, AppEvent};
pub use config::{config_dir, config_file_path, ConfigCenter};
pub use context::{ModuleContext, UiThreadProxy};
pub use error::{MyboxError, Result};
pub use event::{Event, EventBus, EventFilter, EventPayload, FrameworkEvent, SubscriptionId};
pub use hotkey::HotkeyManager;
pub use module::{Module, ModuleRegistry};
pub use renderer::{Renderer, TinySkiaSoftbufferRenderer};
pub use tray::{build_menu, generate_icon, TrayManager};

// Re-exported so feature-module crates can implement `Module` (whose trait
// signatures mention `toml::Table`, `tray_icon::menu::MenuItem`, and
// `anyhow::Result`) with mybox-core as their ONLY dependency — the module
// boundary (FRMW-02): mybox-test declares just `mybox-core`, no third-party
// crates.
pub use anyhow;
pub use log;
pub use toml;
pub use tray_icon;
pub use window::{
    window_attributes, WindowId, WindowKind, WindowManager, WindowManagerHandle, WindowRequest,
    WindowSpec, WindowState,
};
