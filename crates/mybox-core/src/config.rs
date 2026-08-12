//! Configuration center (INFRA-01 / INFRA-04).
//!
//! Full `load_or_create`/`get`/`set`/`save` implementation lands in plan 01-03.
//! This file currently holds the default-constructible shell consumed by
//! `ModuleContext`; the `config_dir()`/`config_file_path()` path contract is
//! defined in plan 01-01-05.

/// Section-based TOML configuration center, namespaced per module id.
#[derive(Default)]
pub struct ConfigCenter;
