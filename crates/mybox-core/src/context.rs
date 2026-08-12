//! ModuleContext — the only facade modules see of the core (FRMW-02).
//!
//! Holds `Arc`-backed references to core services. Accessors
//! `emit`/`on`/`ui`/`windows` land in plan 01-02; `config`/`hotkeys` in
//! plan 01-03. This file provides the fields, the `pub(crate)` constructor,
//! and the plan-01-02 accessors.

use std::sync::Arc;

use crate::config::ConfigCenter;
use crate::event::{Event, EventBus, EventFilter, SubscriptionId};
use crate::hotkey::HotkeyManager;
use crate::window::WindowManagerHandle;

/// The object handed to `Module::init`. Modules interact with the framework
/// exclusively through this context.
///
/// `#[allow(dead_code)]`: `config`/`hotkeys` are consumed by 01-03's accessors;
/// `new` (below) is called by the 01-04 App.
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
    #[allow(dead_code)] // called by the App (01-04)
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

    /// Publish an event onto the shared bus (non-blocking, FRMW-05).
    pub fn emit(&self, event: Event) {
        self.bus.emit(event);
    }

    /// Subscribe with a filter; returns a unique [`SubscriptionId`].
    pub fn on(
        &self,
        filter: EventFilter,
        handler: Box<dyn Fn(&Event) + Send + Sync>,
    ) -> SubscriptionId {
        self.bus.on(filter, handler)
    }

    /// Forward closures to the winit main thread (D-04 reconciliation).
    pub fn ui(&self) -> &UiThreadProxy {
        &self.ui
    }

    /// Enqueue window create/destroy requests (executed on the main thread).
    pub fn windows(&self) -> &WindowManagerHandle {
        &self.windows
    }
}

/// Forwards closures to the winit main thread (D-04 reconciliation with
/// winit's main-thread-bound windows). Backed by `EventLoopProxy`.
///
/// Phase-1 note: `AppEvent` (01-04) does not exist yet, so the proxy is
/// temporarily typed `EventLoopProxy<()>` and closures are stashed in
/// `pending`. 01-04 switches the type to `EventLoopProxy<AppEvent>` and drains
/// `pending` inside `user_event`/`resumed` on the main thread.
pub struct UiThreadProxy {
    inner: parking_lot::Mutex<UiThreadProxyInner>,
}

#[derive(Default)]
struct UiThreadProxyInner {
    proxy: Option<winit::event_loop::EventLoopProxy<()>>,
    pending: Vec<Box<dyn FnOnce() + Send>>,
}

impl UiThreadProxy {
    /// Create a proxy with no backing event loop yet.
    pub fn new() -> Self {
        Self {
            inner: parking_lot::Mutex::new(UiThreadProxyInner::default()),
        }
    }

    /// Attach the winit `EventLoopProxy` (called by App in 01-04 once the loop
    /// exists).
    pub fn set_proxy(&self, proxy: winit::event_loop::EventLoopProxy<()>) {
        self.inner.lock().proxy = Some(proxy);
    }

    /// Run `f` on the main thread.
    ///
    /// Phase 1 placeholder: `EventLoopProxy<()>` cannot carry a closure, so the
    /// closure is stashed in `pending`. 01-04 swaps the proxy type to
    /// `EventLoopProxy<AppEvent>` (with an `AppEvent::Ui(Box<dyn FnOnce>)`
    /// variant) and this method forwards via `send_event`.
    pub fn run(&self, f: Box<dyn FnOnce() + Send>) {
        self.inner.lock().pending.push(f);
    }

    /// Take any stashed closures so the main thread can execute them (01-04).
    #[allow(dead_code)] // consumed by 01-04 when the proxy becomes AppEvent-typed
    pub(crate) fn drain_pending(&self) -> Vec<Box<dyn FnOnce() + Send>> {
        std::mem::take(&mut self.inner.lock().pending)
    }
}

impl Default for UiThreadProxy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigCenter;
    use crate::event::{Event, EventFilter, EventPayload};
    use crate::hotkey::HotkeyManager;
    use crate::window::WindowManagerHandle;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Assemble a context with all core services (the same assembly the 01-04
    /// App will do at runtime).
    fn sample_context() -> ModuleContext {
        ModuleContext::new(
            Arc::new(EventBus::new()),
            Arc::new(WindowManagerHandle::new()),
            Arc::new(ConfigCenter::default()),
            Arc::new(HotkeyManager::default()),
            UiThreadProxy::new(),
        )
    }

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
    fn emit_and_on_round_trip_through_context() {
        let ctx = sample_context();
        let got = Arc::new(AtomicUsize::new(0));
        let g = got.clone();
        ctx.on(
            EventFilter::kind("capture", "*"),
            Box::new(move |_| {
                g.fetch_add(1, Ordering::SeqCst);
            }),
        );
        ctx.emit(Event {
            from: "capture",
            kind: "screenshot-taken",
            payload: EventPayload::Module(serde_json::Value::Null),
        });
        assert!(wait_until(|| got.load(Ordering::SeqCst) == 1), "context emit never dispatched");
    }

    #[test]
    fn ui_and_windows_accessors_return_services() {
        let ctx = sample_context();
        // ui() returns the UiThreadProxy; windows() the WindowManagerHandle.
        // Both are used through their own APIs (run/drain, create/try_recv).
        ctx.ui().run(Box::new(|| {}));
        ctx.windows().create(crate::window::WindowSpec::default());
        let req = ctx.windows().try_recv();
        assert!(matches!(req, Some(crate::window::WindowRequest::Create(_))));
    }

    #[test]
    fn ui_proxy_stashes_closure_until_drained() {
        let ui = UiThreadProxy::new();
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let r = ran.clone();
        ui.run(Box::new(move || {
            r.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        let drained = ui.drain_pending();
        assert_eq!(drained.len(), 1);
        assert_eq!(
            ran.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "closure must not run until drained"
        );
        for f in drained {
            f();
        }
        assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
