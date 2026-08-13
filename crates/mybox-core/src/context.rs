//! ModuleContext — the only facade modules see of the core (FRMW-02).
//!
//! Holds `Arc`-backed references to core services, exposed through accessors:
//! `emit`/`on`/`ui`/`windows` landed in plan 01-02; `config`/`hotkeys` land in
//! plan 01-03. This file provides the fields, the `pub(crate)` constructor, and
//! the accessors.

use std::sync::Arc;

use crate::app::AppEvent;
use crate::config::ConfigCenter;
use crate::event::{Event, EventBus, EventFilter, SubscriptionId};
use crate::hotkey::HotkeyManager;
use crate::window::WindowManagerHandle;

/// The object handed to `Module::init`. Modules interact with the framework
/// exclusively through this context.
pub struct ModuleContext {
    pub(crate) bus: Arc<EventBus>,
    pub(crate) windows: Arc<WindowManagerHandle>,
    pub(crate) config: Arc<ConfigCenter>,
    pub(crate) hotkeys: Arc<HotkeyManager>,
    pub(crate) ui: UiThreadProxy,
}

impl ModuleContext {
    /// Construct a context from the core services.
    ///
    /// Public so feature-module crates can assemble a context in their own unit
    /// tests (the 01-04 `AppBuilder` is the production caller). Every parameter
    /// type is already public API.
    pub fn new(
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

    /// Access the shared event bus (added so feature modules can emit events
    /// from their own `'static` callbacks — e.g. the capture module's overlay
    /// `on_event` emits `capture/screenshot-taken` at confirm time).
    pub fn bus(&self) -> &Arc<EventBus> {
        &self.bus
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
    ///
    /// Returns the shared `Arc` so modules can clone the handle into 'static
    /// event-handler closures (the 01-04 TestModule does exactly this).
    pub fn windows(&self) -> &Arc<WindowManagerHandle> {
        &self.windows
    }

    /// Access the shared configuration center (INFRA-01/INFRA-04). Modules read
    /// and mutate their own `[module_id]` section here.
    pub fn config(&self) -> &Arc<ConfigCenter> {
        &self.config
    }

    /// Access the shared global hotkey manager (FRMW-04). Registration happens
    /// in the 01-04 App via `init()` + `register_str()`.
    pub fn hotkeys(&self) -> &Arc<HotkeyManager> {
        &self.hotkeys
    }
}

/// Forwards closures to the winit main thread (D-04 reconciliation with
/// winit's main-thread-bound windows). Backed by `EventLoopProxy<AppEvent>`:
/// [`run`](Self::run) forwards the closure as `AppEvent::Ui(f)`, which the App
/// executes in `ApplicationHandler::user_event` on the main thread (01-04).
///
/// Cloneable (shares the proxy behind an `Arc`) so both the App and the
/// `ModuleContext` handed to modules reference the same forwarder.
#[derive(Clone)]
pub struct UiThreadProxy {
    inner: Arc<parking_lot::Mutex<UiThreadProxyInner>>,
}

#[derive(Default)]
struct UiThreadProxyInner {
    proxy: Option<winit::event_loop::EventLoopProxy<AppEvent>>,
    pending: Vec<Box<dyn FnOnce() + Send>>,
}

impl UiThreadProxy {
    /// Create a proxy with no backing event loop yet.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(parking_lot::Mutex::new(UiThreadProxyInner::default())),
        }
    }

    /// Attach the winit `EventLoopProxy` (called by `App::run` once the loop
    /// exists). Any closures enqueued before the loop existed (e.g. during
    /// module `init`) are flushed through the loop at this point.
    pub fn set_proxy(&self, proxy: winit::event_loop::EventLoopProxy<AppEvent>) {
        let mut inner = self.inner.lock();
        let pending = std::mem::take(&mut inner.pending);
        for f in pending {
            let _ = proxy.send_event(AppEvent::Ui(f));
        }
        inner.proxy = Some(proxy);
    }

    /// Run `f` on the winit main thread.
    ///
    /// Forwarded as `AppEvent::Ui(f)` through the loop proxy. If the loop does
    /// not exist yet (module `init` runs before `run()`), the closure is stashed
    /// until `set_proxy` flushes it.
    pub fn run(&self, f: Box<dyn FnOnce() + Send>) {
        let mut inner = self.inner.lock();
        if let Some(proxy) = &inner.proxy {
            let _ = proxy.send_event(AppEvent::Ui(f));
        } else {
            inner.pending.push(f);
        }
    }

    /// Take any stashed closures so the main thread can execute them (tests).
    #[cfg(test)]
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
    fn config_and_hotkeys_accessors_return_shared_services() {
        let config = Arc::new(ConfigCenter::default());
        let hotkeys = Arc::new(HotkeyManager::new());
        let ctx = ModuleContext::new(
            Arc::new(EventBus::new()),
            Arc::new(WindowManagerHandle::new()),
            Arc::clone(&config),
            Arc::clone(&hotkeys),
            UiThreadProxy::new(),
        );
        // The accessors return the exact Arc instances the context was built
        // with — modules see the same ConfigCenter/HotkeyManager as the App.
        assert!(
            Arc::ptr_eq(ctx.config(), &config),
            "config() must return the shared ConfigCenter"
        );
        assert!(
            Arc::ptr_eq(ctx.hotkeys(), &hotkeys),
            "hotkeys() must return the shared HotkeyManager"
        );
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
