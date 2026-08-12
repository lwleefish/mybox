//! Event data model (D-06 hybrid payload) and wildcard filter matching (D-05).
//!
//! This module defines only pure data + pure logic. The `EventBus` channel and
//! worker-thread dispatch are implemented in plan 01-02; here it exists as a
//! type skeleton so `ModuleContext` can hold a reference to it.

use parking_lot::Mutex;

/// A single event flowing through the bus.
///
/// `from` is the emitting module id (or `"core"` for framework events); `kind`
/// is the event name. Payload is either a typed framework event or freeform
/// JSON for module-defined events (D-06).
#[derive(Clone, Debug)]
pub struct Event {
    pub from: &'static str,
    pub kind: &'static str,
    pub payload: EventPayload,
}

/// Hybrid event payload (D-06): typed for the framework, JSON for modules.
#[derive(Clone, Debug)]
pub enum EventPayload {
    /// Typed framework events (window lifecycle, hotkey, module lifecycle).
    Framework(FrameworkEvent),
    /// Module-defined events as freeform JSON.
    Module(serde_json::Value),
}

/// Typed framework events emitted by the core.
#[derive(Clone, Debug)]
pub enum FrameworkEvent {
    WindowCreated(u64),
    WindowDestroyed(u64),
    /// A global hotkey fired. `id` is the global-hotkey id; `action` is the
    /// configured action name for decoupled dispatch (RESEARCH §11 #3).
    HotkeyTriggered { id: u32, action: String },
    ModuleLoaded(&'static str),
    AppReady,
    AppExit,
}

/// Subscription filter with `"*"` wildcard support (D-05 broadcast + wildcard).
///
/// `matches` compares each field: a filter field of `"*"` matches any value;
/// otherwise the filter value must equal the event value exactly. `"capture:*"`
/// therefore matches every event whose `from == "capture"`, regardless of kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventFilter {
    pub from: &'static str,
    pub kind: &'static str,
}

impl EventFilter {
    /// Matches every event.
    pub fn all() -> Self {
        Self { from: "*", kind: "*" }
    }

    /// Matches events with the given `from` (may be `"*"`) and `kind` (may be `"*"`).
    pub fn kind(from: &'static str, kind: &'static str) -> Self {
        Self { from, kind }
    }

    /// True when this filter matches `event`.
    pub fn matches(&self, e: &Event) -> bool {
        (self.from == "*" || self.from == e.from) && (self.kind == "*" || self.kind == e.kind)
    }
}

/// Opaque subscription handle returned by `on(...)` (useful for a future
/// unsubscribe API; reserved in this plan).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubscriptionId(pub u64);

/// Event bus skeleton. Fields are declared here so `ModuleContext` can hold an
/// `Arc<EventBus>`; the channel worker + dispatch logic land in plan 01-02.
#[allow(dead_code)]
pub struct EventBus {
    sender: crossbeam_channel::Sender<Event>,
    handler_lock: Mutex<Vec<(EventFilter, Box<dyn Fn(&Event) + Send + Sync>)>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(from: &'static str, kind: &'static str) -> Event {
        Event {
            from,
            kind,
            payload: EventPayload::Module(serde_json::Value::Null),
        }
    }

    #[test]
    fn wildcard_kind_matches_any_kind_for_from() {
        let f = EventFilter::kind("capture", "*");
        assert!(f.matches(&event("capture", "screenshot-taken")));
        assert!(f.matches(&event("capture", "region-selected")));
        // A different `from` never matches.
        assert!(!f.matches(&event("palette", "screenshot-taken")));
    }

    #[test]
    fn all_matches_everything() {
        let f = EventFilter::all();
        assert!(f.matches(&event("capture", "screenshot-taken")));
        assert!(f.matches(&event("core", "window-created")));
        assert!(f.matches(&event("anything", "at-all")));
    }

    #[test]
    fn non_matching_filter_returns_false() {
        let f = EventFilter::kind("capture", "screenshot-taken");
        assert!(!f.matches(&event("capture", "region-selected")));
        assert!(!f.matches(&event("palette", "screenshot-taken")));
    }

    #[test]
    fn exact_from_and_kind_match() {
        let f = EventFilter::kind("capture", "screenshot-taken");
        assert!(f.matches(&event("capture", "screenshot-taken")));
    }

    #[test]
    fn hotkey_triggered_has_id_and_action() {
        let payload = EventPayload::Framework(FrameworkEvent::HotkeyTriggered {
            id: 7,
            action: "open-test-window".to_string(),
        });
        let e = Event {
            from: "core",
            kind: "hotkey.triggered",
            payload,
        };
        if let EventPayload::Framework(FrameworkEvent::HotkeyTriggered { id, action }) = &e.payload {
            assert_eq!(*id, 7);
            assert_eq!(action, "open-test-window");
        } else {
            panic!("expected HotkeyTriggered");
        }
    }
}
