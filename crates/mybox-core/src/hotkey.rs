//! Global hotkey manager (FRMW-04, D-11).

use std::collections::HashMap;
use std::sync::Mutex as StdMutex;

use global_hotkey::hotkey::HotKey;
use global_hotkey::GlobalHotKeyManager;

use crate::error::{MyboxError, Result};

/// Registers and tracks global hotkeys; maps trigger ids back to action names.
///
/// D-11: hotkeys are configured as plain strings (`"Cmd+Shift+T"`) and parsed
/// with `HotKey`'s built-in `FromStr` — no hand-written parser (RESEARCH §2.4).
/// The OS-level manager lives behind `parking_lot::Mutex<Option<..>>` interior
/// mutability so every method takes `&self`: by the time `run()` is reached the
/// `Arc<HotkeyManager>` is already cloned into `ModuleContext` (refcount >= 2),
/// so `Arc::get_mut` would return `None` and a `&mut self` design is unusable.
///
/// Lifecycle (FRMW-04): `init()` on the main thread (macOS) → `register_str(..)`
/// for each configured hotkey → on a trigger, 01-04 maps the event's `HotKey::id`
/// back to an action via [`action_for_id`](Self::action_for_id).
pub struct HotkeyManager {
    /// OS-level manager; `None` until [`init`](Self::init) is called.
    manager: parking_lot::Mutex<Option<GlobalHotKeyManager>>,
    /// `hotkey id -> action name`, used to translate trigger events.
    map: StdMutex<HashMap<u32, String>>,
}

impl Default for HotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HotkeyManager {
    /// Create a manager with no OS backing yet ([`init`](Self::init) must run first).
    pub fn new() -> Self {
        Self {
            manager: parking_lot::Mutex::new(None),
            map: StdMutex::new(HashMap::new()),
        }
    }

    /// Create the real OS-level [`GlobalHotKeyManager`].
    ///
    /// macOS: MUST be called on the main thread (RESEARCH §2.4). The 01-04 App
    /// calls this from its main-thread startup before registering hotkeys.
    pub fn init(&self) -> Result<()> {
        let manager = GlobalHotKeyManager::new().map_err(|e| {
            MyboxError::HotkeyParse(format!("failed to init global hotkey manager: {e}"))
        })?;
        *self.manager.lock() = Some(manager);
        Ok(())
    }

    /// Parse `hotkey_str` (e.g. `"Cmd+Shift+T"`) and register it with the OS,
    /// recording the `hotkey id -> action` mapping.
    ///
    /// Returns the registered hotkey's id so callers can match trigger events.
    /// Returns `Err` if [`init`](Self::init) was not called yet or the string is
    /// not a valid hotkey (D-11, T-1-01: strict parse, never panic, never
    /// register garbage). Registration failures (T-1-07: conflicts with the
    /// system or other apps) are surfaced as errors; 01-04 logs and continues.
    pub fn register_str(&self, action: &str, hotkey_str: &str) -> Result<u32> {
        let hotkey: HotKey = hotkey_str.parse().map_err(|e| {
            MyboxError::HotkeyParse(format!("invalid hotkey string '{hotkey_str}': {e}"))
        })?;

        let manager = self.manager.lock();
        let manager = manager.as_ref().ok_or_else(|| {
            MyboxError::HotkeyParse(
                "hotkey manager not initialized (call init() on the main thread first)".to_string(),
            )
        })?;

        manager.register(hotkey).map_err(|e| {
            MyboxError::HotkeyParse(format!("failed to register hotkey '{action}' ({hotkey_str}): {e}"))
        })?;

        let id = hotkey.id();
        self.map.lock().unwrap().insert(id, action.to_string());
        Ok(id)
    }

    /// Look up the action name registered for a hotkey id (trigger translation).
    pub fn action_for_id(&self, id: u32) -> Option<String> {
        self.map.lock().unwrap().get(&id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::HotKey;

    #[test]
    fn parses_modifier_plus_key_string() {
        // D-11: config strings are parsed with global-hotkey's built-in FromStr.
        let hk: HotKey = "Cmd+Shift+T".parse().expect("valid hotkey string");
        let hk2: HotKey = "Cmd+Shift+T".parse().expect("valid hotkey string");
        assert_eq!(hk.id(), hk2.id(), "same combo must hash to the same id");
    }

    #[test]
    fn rejects_invalid_hotkey_string_without_panicking() {
        // T-1-01: a bad config value yields Err, never a panic.
        let err = "not-a-hotkey!!".parse::<HotKey>();
        assert!(err.is_err(), "garbage string must not parse");
    }

    #[test]
    fn action_for_id_round_trips_through_map() {
        let hm = HotkeyManager::new();
        // Inject a fake id directly into the map (headless-safe: no real OS
        // registration here; real registration is 01-04 integration scope).
        hm.map.lock().unwrap().insert(4242, "open_test_window".to_string());
        assert_eq!(hm.action_for_id(4242), Some("open_test_window".to_string()));
        assert_eq!(hm.action_for_id(9999), None);
    }

    #[test]
    fn register_str_errors_when_manager_not_initialized() {
        // Headless-safe: never call init() (needs the main thread / OS).
        let hm = HotkeyManager::new();
        let err = hm
            .register_str("open_test_window", "Cmd+Shift+T")
            .unwrap_err();
        assert!(matches!(err, MyboxError::HotkeyParse(_)));
    }
}
