//! Configuration center (INFRA-01 / INFRA-04).
//!
//! Full `load_or_create`/`get`/`set`/`save` implementation lands in plan 01-03.
//! This file holds the default-constructible shell consumed by `ModuleContext`
//! plus the platform path contract (INFRA-04) defined in plan 01-01-05.

use std::path::PathBuf;

/// Section-based TOML configuration center, namespaced per module id.
#[derive(Default)]
pub struct ConfigCenter;

/// Platform user-config directory for mybox, resolved via
/// `directories::ProjectDirs::from("", "", "mybox")` (INFRA-04):
/// macOS `~/Library/Application Support/mybox/`.
///
/// Returns `Err` (never panics) when the platform has no config directory.
pub fn config_dir() -> anyhow::Result<PathBuf> {
    directories::ProjectDirs::from("", "", "mybox")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("could not resolve the platform config directory"))
}

/// Full path to the config file: `config_dir()/config.toml` (INFRA-04 contract).
pub fn config_file_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_resolves_to_mybox_dir() {
        let dir = config_dir().expect("config dir must resolve");
        assert_eq!(
            dir.file_name().and_then(|n| n.to_str()),
            Some("mybox"),
            "config dir basename should be 'mybox', got {:?}",
            dir
        );
    }

    #[test]
    fn config_file_path_contract() {
        // INFRA-04: the config file lives at <platform user config dir>/mybox/config.toml.
        // Asserted via path components, not a hardcoded absolute path.
        let p = config_file_path().expect("config file path must resolve");
        assert_eq!(p.file_name().and_then(|n| n.to_str()), Some("config.toml"));
        assert_eq!(
            p.parent().and_then(|d| d.file_name()).and_then(|n| n.to_str()),
            Some("mybox")
        );
        // macOS-specific tail, verified only when running on macOS.
        #[cfg(target_os = "macos")]
        {
            let s = p.to_string_lossy();
            assert!(
                s.contains("Library/Application Support/mybox/config.toml"),
                "unexpected macOS config path: {s}"
            );
        }
    }
}
