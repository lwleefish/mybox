//! Unified error type for the mybox framework (INFRA-03).

use std::io;

/// Framework-wide error type. Modules may return this through the `Result` alias;
/// application-level code wraps it with `anyhow` context at the binary boundary.
#[derive(Debug, thiserror::Error)]
pub enum MyboxError {
    /// Config file could not be found where the platform path contract expects it.
    #[error("config file not found")]
    ConfigNotFound,
    /// Config file existed but could not be parsed as TOML.
    #[error("config parse error: {0}")]
    ConfigParse(String),
    /// A hotkey string in config could not be parsed.
    #[error("hotkey parse error: {0}")]
    HotkeyParse(String),
    /// Window creation or window-manager operation failed.
    #[error("window error: {0}")]
    Window(String),
    /// Event bus publish or dispatch failure.
    #[error("event bus error: {0}")]
    EventBus(String),
    /// I/O error (config read/write etc.).
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    /// softbuffer context/surface error, mapped to a string message.
    #[error("softbuffer error: {0}")]
    Softbuffer(String),
    /// Tray icon/menu error.
    #[error("tray error: {0}")]
    Tray(String),
}

/// Convenience alias used throughout the framework.
pub type Result<T> = std::result::Result<T, MyboxError>;

impl From<toml::de::Error> for MyboxError {
    fn from(e: toml::de::Error) -> Self {
        MyboxError::ConfigParse(e.to_string())
    }
}

impl From<toml::ser::Error> for MyboxError {
    fn from(e: toml::ser::Error) -> Self {
        MyboxError::ConfigParse(e.to_string())
    }
}

// softbuffer 0.4.8 exposes a single unified error enum (`SoftBufferError`), not
// the per-operation `ContextError`/`SurfaceError` types. Mapping it to the
// `Softbuffer` variant keeps W6 satisfied: neither `Surface::new`/`Context::new`
// nor `buffer_mut()`/`present()` errors ever fall into an unmapped variant.
impl From<softbuffer::SoftBufferError> for MyboxError {
    fn from(e: softbuffer::SoftBufferError) -> Self {
        MyboxError::Softbuffer(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_derives_thiserror() {
        let err = MyboxError::ConfigNotFound;
        // Display messages exist and are non-empty for the core variants.
        assert!(!err.to_string().is_empty());
        assert!(format!("{err:?}").contains("ConfigNotFound"));
    }

    #[test]
    fn io_error_converts_from_std() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let mybox_err: MyboxError = io_err.into();
        assert!(matches!(mybox_err, MyboxError::Io(_)));
    }

    #[test]
    fn toml_de_error_bridges_to_config_parse() {
        let bad: std::result::Result<toml::Value, toml::de::Error> = toml::from_str("key = = broken");
        let err = bad.unwrap_err();
        let mybox_err: MyboxError = err.into();
        assert!(matches!(mybox_err, MyboxError::ConfigParse(_)));
    }

    #[test]
    fn toml_ser_error_from_exists() {
        // A toml::ser::Error can be produced at runtime only from types that
        // fail TOML serialization; those require a serde Serialize impl. We
        // verify the From bridge exists and is callable (W6: no unmapped errors).
        fn assert_from(e: toml::ser::Error) -> MyboxError {
            MyboxError::from(e)
        }
        let _ = assert_from;
    }

    #[test]
    fn softbuffer_error_converts() {
        // SoftBufferError::Unimplemented is a public unit variant we can
        // construct to exercise the From bridge at runtime.
        let sb_err = softbuffer::SoftBufferError::Unimplemented;
        let mybox_err: MyboxError = sb_err.into();
        assert!(matches!(mybox_err, MyboxError::Softbuffer(_)));
    }

    #[test]
    fn result_alias_exists() {
        let r: Result<u32> = Ok(42);
        assert_eq!(r.unwrap(), 42);
    }
}
