//! Test module used to validate the mybox module framework (01-04-04).
//!
//! `TestModule` proves the module boundary end-to-end (FRMW-01): it declares a
//! config section, contributes a tray menu item, subscribes to the core
//! `hotkey.triggered` event, and — when the configured `open_test_window`
//! action fires — enqueues a window-creation request.
//!
//! This crate depends ONLY on `mybox-core` (no third-party crates): every type
//! it needs (`Module`, `ModuleContext`, events, `WindowSpec`, and the
//! `toml`/`tray_icon`/`anyhow` re-exports) comes from the framework's public
//! API (FRMW-02 — modules never depend on core internals or on each other).

use mybox_core::anyhow;
use mybox_core::event::{EventFilter, EventPayload, FrameworkEvent};
use mybox_core::log;
use mybox_core::module::Module;
use mybox_core::toml;
use mybox_core::tray_icon;
use mybox_core::window::{WindowKind, WindowSpec};
use mybox_core::ModuleContext;

/// The walking-skeleton demo module: hotkey `open_test_window` → test window.
pub struct TestModule;

impl Module for TestModule {
    fn id(&self) -> &'static str {
        "test"
    }

    fn name(&self) -> &str {
        "测试模块"
    }

    fn default_config(&self) -> toml::Table {
        // Matches the 01-03 first-run generation contract: [test].message.
        let mut table = toml::Table::new();
        table.insert(
            "message".to_string(),
            toml::Value::String("hello from test".to_string()),
        );
        table
    }

    fn menu_items(&self) -> Vec<tray_icon::menu::MenuItem> {
        // The id round-trips through `MenuEvent` → `menu.triggered` (INFRA-02).
        vec![tray_icon::menu::MenuItem::with_id(
            "test.open_window",
            "打开测试窗口",
            true,
            None,
        )]
    }

    fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()> {
        // The handler runs on the bus worker thread, so it needs its own clone
        // of the window handle to enqueue into (W2): `create()` is thread-safe,
        // non-blocking, and wakes the main loop (W3). No main-thread hop needed.
        let windows = ctx.windows().clone();
        ctx.on(
            EventFilter::kind("core", "hotkey.triggered"),
            Box::new(move |e| {
                if let EventPayload::Framework(FrameworkEvent::HotkeyTriggered { action, .. }) =
                    &e.payload
                {
                    if action == "open_test_window" {
                        log::info!("test module: hotkey 'open_test_window' triggered");
                        windows.create(WindowSpec {
                            kind: WindowKind::Panel,
                            title: "mybox test".to_string(),
                            inner_size: Some((400, 300)),
                            ..Default::default()
                        });
                    }
                }
            }),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mybox_core::{ConfigCenter, Event, EventBus, HotkeyManager, UiThreadProxy};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn wait_until(cond: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// A context over a fresh bus + window handle (headless: no real config
    /// dir, no OS hotkey/tray).
    fn sample_context() -> (Arc<EventBus>, Arc<mybox_core::WindowManagerHandle>, ModuleContext) {
        let bus = Arc::new(EventBus::new());
        let handle = Arc::new(mybox_core::WindowManagerHandle::new());
        let ctx = ModuleContext::new(
            Arc::clone(&bus),
            Arc::clone(&handle),
            Arc::new(ConfigCenter::default()),
            Arc::new(HotkeyManager::new()),
            UiThreadProxy::new(),
        );
        (bus, handle, ctx)
    }

    #[test]
    fn id_and_name() {
        let module = TestModule;
        assert_eq!(module.id(), "test");
        assert_eq!(module.name(), "测试模块");
    }

    #[test]
    fn default_config_has_message_key() {
        let table = TestModule.default_config();
        assert_eq!(
            table.get("message"),
            Some(&toml::Value::String("hello from test".to_string()))
        );
    }

    #[test]
    fn menu_items_contains_open_window_item() {
        let items = TestModule.menu_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id(), "test.open_window");
        assert_eq!(items[0].text(), "打开测试窗口");
    }

    #[test]
    fn hotkey_trigger_enqueues_test_window_and_wakes_once() {
        let (bus, handle, ctx) = sample_context();
        // W3: the wake hook must fire exactly once per enqueue (create()).
        let wake = Arc::new(AtomicUsize::new(0));
        let w = wake.clone();
        handle.set_wakeup(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));

        TestModule.init(&ctx).expect("init registers handler");

        bus.emit(Event {
            from: "core",
            kind: "hotkey.triggered",
            payload: EventPayload::Framework(FrameworkEvent::HotkeyTriggered {
                id: 1,
                action: "open_test_window".to_string(),
            }),
        });

        // The handler runs on the bus worker thread; poll until the Create
        // request is enqueued, capturing it in the same poll (try_recv consumes).
        let received: Arc<std::sync::Mutex<Option<mybox_core::WindowRequest>>> =
            Arc::new(std::sync::Mutex::new(None));
        let got = received.clone();
        assert!(
            wait_until(|| {
                if let Some(req) = handle.try_recv() {
                    *got.lock().unwrap() = Some(req);
                    true
                } else {
                    false
                }
            }),
            "hotkey trigger never enqueued a WindowRequest"
        );
        match received.lock().unwrap().as_ref() {
            Some(mybox_core::WindowRequest::Create(spec)) => {
                assert_eq!(spec.title, "mybox test");
                assert_eq!(spec.kind, WindowKind::Panel);
                assert_eq!(spec.inner_size, Some((400, 300)));
            }
            Some(mybox_core::WindowRequest::Destroy(_)) => panic!("expected Create, got Destroy"),
            Some(mybox_core::WindowRequest::Redraw(_)) => panic!("expected Create, got Redraw"),
            Some(mybox_core::WindowRequest::SetCursor(_, _)) => panic!("expected Create, got SetCursor"),
            None => panic!("no request queued"),
        }
        assert_eq!(
            wake.load(Ordering::SeqCst),
            1,
            "create() must trigger the wake hook once (W3)"
        );
    }

    #[test]
    fn other_hotkey_actions_do_not_open_window() {
        let (bus, handle, ctx) = sample_context();
        TestModule.init(&ctx).expect("init registers handler");

        bus.emit(Event {
            from: "core",
            kind: "hotkey.triggered",
            payload: EventPayload::Framework(FrameworkEvent::HotkeyTriggered {
                id: 2,
                action: "some_other_action".to_string(),
            }),
        });
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            handle.try_recv().is_none(),
            "a non-matching action must not enqueue a window request"
        );
    }
}
