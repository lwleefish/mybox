//! Event data model (D-06 hybrid payload), wildcard filter matching (D-05),
//! and the [`EventBus`] (FRMW-02 / FRMW-05).
//!
//! The bus uses an unbounded `crossbeam_channel` transport with a dedicated
//! worker thread: [`EventBus::emit`] only `send()`s (never blocks — FRMW-05),
//! and the worker thread `recv()`s and broadcasts each event to every handler
//! whose filter matches (D-05 broadcast semantics; D-04 keeps the "background
//! thread receives and dispatches" letter). Handlers that touch winit windows
//! forward to the main thread via `ModuleContext::ui()` (D-04 reconciliation).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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
/// unsubscribe API; reserved for a later plan).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubscriptionId(pub u64);

/// A registered subscriber: a filter plus the handler to invoke on a match.
/// Stored behind `Arc` so dispatch can snapshot the list without holding the
/// handler lock while user code runs (a handler that calls `on()` must not
/// deadlock on the same lock).
type Handler = (EventFilter, Arc<dyn Fn(&Event) + Send + Sync>);

/// Broadcast event bus (FRMW-02) with non-blocking publish (FRMW-05).
///
/// Cloneable — all clones share one channel + one worker thread. The worker
/// exits automatically when the last `EventBus` clone is dropped (the channel
/// sender drops, `recv()` returns `Err(Disconnected)`, the loop ends).
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<EventBusInner>,
}

struct EventBusInner {
    sender: crossbeam_channel::Sender<Event>,
    handlers: Arc<Mutex<Vec<Handler>>>,
    next_sub_id: AtomicU64,
    /// Keeps the worker thread handle alive as long as the bus lives. The
    /// thread itself terminates via channel disconnect when the bus drops.
    _worker: std::thread::JoinHandle<()>,
}

impl EventBus {
    /// Create a bus: unbounded channel (T-1-05: unbounded by design, Phase 1
    /// event volume is tiny; revisit bounded + backpressure if modules grow)
    /// and a worker thread that broadcasts each received event to matching
    /// handlers.
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded::<Event>();
        let handlers: Arc<Mutex<Vec<Handler>>> = Arc::new(Mutex::new(Vec::new()));
        let worker_handlers = Arc::clone(&handlers);
        let worker = std::thread::Builder::new()
            .name("mybox-event-bus".to_string())
            .spawn(move || {
                while let Ok(event) = receiver.recv() {
                    // Snapshot so dispatch never blocks on the handler lock and
                    // a handler subscribing mid-dispatch is picked up next event.
                    let snapshot: Vec<Handler> = worker_handlers.lock().clone();
                    for (filter, handler) in &snapshot {
                        if filter.matches(&event) {
                            // T-1-04: a panicking handler must not kill the
                            // dispatch loop / worker thread.
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                handler(&event);
                            }));
                        }
                    }
                }
            })
            .expect("spawn event-bus worker thread");
        Self {
            inner: Arc::new(EventBusInner {
                sender,
                handlers,
                next_sub_id: AtomicU64::new(1),
                _worker: worker,
            }),
        }
    }

    /// Publish an event. Non-blocking: only sends into the unbounded channel
    /// (FRMW-05). Returns immediately regardless of how busy the worker is.
    pub fn emit(&self, event: Event) {
        if let Err(err) = self.inner.sender.send(event) {
            // Can only happen after the worker receiver is gone (bus fully
            // dropped); log rather than panic on a broadcast path.
            log::warn!("event bus send failed: {err:?}");
        }
    }

    /// Subscribe with a filter. Returns an incrementing, unique [`SubscriptionId`].
    pub fn on(&self, filter: EventFilter, handler: Box<dyn Fn(&Event) + Send + Sync>) -> SubscriptionId {
        let id = self.inner.next_sub_id.fetch_add(1, Ordering::SeqCst);
        self.inner
            .handlers
            .lock()
            .push((filter, Arc::from(handler)));
        SubscriptionId(id)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
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

/// EventBus implementation tests (FRMW-02 / FRMW-05). Separate module so the
/// `event_bus::` nextest filter selects exactly these tests.
#[cfg(test)]
mod event_bus {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    fn event(from: &'static str, kind: &'static str) -> Event {
        Event {
            from,
            kind,
            payload: EventPayload::Module(serde_json::Value::Null),
        }
    }

    /// Poll `cond` for up to ~2s (dispatch is async on the worker thread).
    fn wait_until(cond: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn emit_dispatches_to_matching_handler_once_in_order() {
        let bus = EventBus::new();
        let seen: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        bus.on(EventFilter::all(), Box::new(move |e| s.lock().push(e.kind)));
        bus.emit(event("core", "first"));
        bus.emit(event("core", "second"));
        assert!(wait_until(|| seen.lock().len() == 2), "handler never fired");
        assert_eq!(*seen.lock(), vec!["first", "second"], "order not preserved");
    }

    #[test]
    fn wildcard_from_filter_receives_all_kinds_for_from() {
        let bus = EventBus::new();
        let seen: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        bus.on(EventFilter::kind("capture", "*"), Box::new(move |e| s.lock().push(e.kind)));
        bus.emit(event("capture", "screenshot-taken"));
        bus.emit(event("capture", "region-selected"));
        bus.emit(event("palette", "screenshot-taken")); // different from: must NOT fire
        assert!(
            wait_until(|| seen.lock().len() == 2),
            "expected exactly the two capture events"
        );
        assert_eq!(*seen.lock(), vec!["screenshot-taken", "region-selected"]);
    }

    #[test]
    fn all_filter_receives_every_event() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        bus.on(EventFilter::all(), Box::new(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        bus.emit(event("capture", "a"));
        bus.emit(event("core", "b"));
        bus.emit(event("any", "c"));
        assert!(wait_until(|| count.load(Ordering::SeqCst) == 3));
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn non_matching_filter_handler_is_not_called() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        bus.on(
            EventFilter::kind("capture", "screenshot-taken"),
            Box::new(move |_| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
        bus.emit(event("capture", "region-selected")); // does not match
        bus.emit(event("capture", "screenshot-taken")); // matches
        // Channel ordering guarantees: if region-selected had matched, the
        // count would reach 2. Waiting until exactly 1 proves it was skipped.
        assert!(wait_until(|| count.load(Ordering::SeqCst) == 1));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn framework_and_module_payloads_round_trip() {
        let bus = EventBus::new();
        let got: Arc<Mutex<Option<EventPayload>>> = Arc::new(Mutex::new(None));
        let g = got.clone();
        bus.on(EventFilter::all(), Box::new(move |e| *g.lock() = Some(e.payload.clone())));

        bus.emit(Event {
            from: "core",
            kind: "window-created",
            payload: EventPayload::Framework(FrameworkEvent::WindowCreated(42)),
        });
        assert!(wait_until(|| got.lock().is_some()));
        {
            let guard = got.lock();
            match &*guard {
                Some(EventPayload::Framework(FrameworkEvent::WindowCreated(id))) => assert_eq!(*id, 42),
                other => panic!("expected Framework(WindowCreated), got {other:?}"),
            }
        }

        *got.lock() = None;
        bus.emit(Event {
            from: "capture",
            kind: "screenshot-taken",
            payload: EventPayload::Module(serde_json::json!({ "path": "/tmp/shot.png" })),
        });
        assert!(wait_until(|| got.lock().is_some()));
        {
            let guard = got.lock();
            match &*guard {
                Some(EventPayload::Module(v)) => assert_eq!(v["path"], "/tmp/shot.png"),
                other => panic!("expected Module payload, got {other:?}"),
            }
        }
    }

    #[test]
    fn two_subscriptions_get_distinct_ids() {
        let bus = EventBus::new();
        let a = bus.on(EventFilter::all(), Box::new(|_| {}));
        let b = bus.on(EventFilter::all(), Box::new(|_| {}));
        assert_ne!(a, b, "on() must return a distinct SubscriptionId per call");
    }

    #[test]
    fn emit_is_non_blocking_while_handler_sleeps() {
        // FRMW-05: publish into an unbounded channel never waits on handlers.
        let bus = EventBus::new();
        let entered = Arc::new(AtomicBool::new(false));
        let e = entered.clone();
        bus.on(EventFilter::all(), Box::new(move |_| {
            e.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(100));
        }));
        bus.emit(event("core", "slow"));
        assert!(wait_until(|| entered.load(Ordering::SeqCst)), "worker never ran handler");
        let t0 = Instant::now();
        bus.emit(event("core", "while-busy"));
        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_millis(50),
            "emit blocked for {elapsed:?} while the handler was sleeping"
        );
    }

    #[test]
    fn cross_thread_emit_reaches_handler_within_timeout() {
        let bus = EventBus::new();
        let got = Arc::new(AtomicUsize::new(0));
        let g = got.clone();
        bus.on(EventFilter::all(), Box::new(move |_| {
            g.fetch_add(1, Ordering::SeqCst);
        }));
        let worker_bus = bus.clone();
        let thread = std::thread::spawn(move || worker_bus.emit(event("capture", "from-thread")));
        thread.join().expect("emitter thread panicked");
        assert!(wait_until(|| got.load(Ordering::SeqCst) == 1), "handler did not fire within ~2s");
    }
}
