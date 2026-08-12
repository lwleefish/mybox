//! mybox-core — framework core for the mybox modular desktop toolbox.
//!
//! Zero business logic. Public API surface is assembled in plan 01-01-07;
//! modules are added here as they land.

pub mod config;
pub mod context;
pub mod error;
pub mod event;
pub mod hotkey;
pub mod module;
pub mod tray;
pub mod window;
