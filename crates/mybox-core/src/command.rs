//! Command model, registry, framework builtins, and the async runner dispatch
//! (Phase 3, PAL-02 / SPEC req 1).
//!
//! A [`Command`] is data (id/name/description/keywords) plus an async runner
//! closure (D-07). The [`CommandRegistry`] is assembled exactly once in
//! `AppBuilder::build` — module commands in registration order, then the four
//! framework builtins — and exposed to modules via `ModuleContext::commands()`.
//! There is no runtime registration path (T-3-02).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use crate::context::UiThreadProxy;
use crate::error::{MyboxError, Result};
use crate::event::{Event, EventBus, EventPayload, FrameworkEvent};

/// Async command runner (D-07): each invocation returns a boxed future driven
/// by `pollster::block_on` on a dedicated worker thread (see [`run_command`]).
/// `Arc`-wrapped so `Command` can be cloned (the palette snapshots the list at
/// summon time).
pub type CommandRunner =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> + Send + Sync>;

/// A command the palette can list and execute.
///
/// Every command has a non-empty name and description (SPEC req 1). The
/// runner must never panic (it runs on a worker thread; panics are caught
/// nowhere in the runner itself — completion hops back through UiThreadProxy
/// which core wraps in `catch_unwind`).
#[derive(Clone)]
pub struct Command {
    pub id: &'static str,
    pub name: String,
    pub description: String,
    pub keywords: Vec<&'static str>,
    pub runner: CommandRunner,
    /// Close the palette window BEFORE running (capture.start — the panel must
    /// never appear in screenshots, UI-SPEC lifecycle rule 1).
    pub hide_before_execute: bool,
}

/// Registry of all commands, assembled once at build time.
#[derive(Default)]
pub struct CommandRegistry {
    commands: Vec<Command>,
}

impl CommandRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Register a command. Returns `Err(MyboxError::Command)` on a duplicate
    /// id (mirrors `ModuleRegistry::register` — T-3-02).
    pub fn register(&mut self, cmd: Command) -> Result<()> {
        if self.commands.iter().any(|c| c.id == cmd.id) {
            return Err(MyboxError::Command(format!("duplicate command id '{}'", cmd.id)));
        }
        self.commands.push(cmd);
        Ok(())
    }

    /// All commands in registration order (cloned — callers snapshot the list).
    pub fn all(&self) -> Vec<Command> {
        self.commands.clone()
    }

    /// Number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// True when no commands are registered.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// The four framework builtin commands (UI-SPEC command inventory).
pub struct BuiltinCommands;

impl BuiltinCommands {
    /// Build the builtin command list with the production platform opener
    /// (`open` / `explorer`, no shell — T-3-07) and spawner.
    pub fn build(bus: Arc<EventBus>, config_dir: PathBuf, log_path: PathBuf) -> Vec<Command> {
        Self::build_with(bus, config_dir, log_path, platform_opener(), platform_spawner())
    }

    /// Build the builtin command list with injectable OS side effects
    /// (headless unit-test injection point — the `CaptureFn` discipline).
    pub fn build_with(
        bus: Arc<EventBus>,
        config_dir: PathBuf,
        log_path: PathBuf,
        opener: Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync>,
        spawner: Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync>,
    ) -> Vec<Command> {
        vec![
            Command {
                id: "builtin.quit",
                name: "退出应用".to_string(),
                description: "退出 mybox 应用".to_string(),
                // GAP-7: pinyin aliases let users hit the Chinese command name
                // without an IME (same keyword-tier mechanism as capture's
                // "jietu" — pure data, no code-path change).
                keywords: vec!["退出", "quit", "exit", "tuichu"],
                hide_before_execute: false,
                runner: {
                    // Clone before closure creation: the runner is `Fn` (may be
                    // invoked multiple times), so each invocation clones again.
                    let bus = Arc::clone(&bus);
                    Arc::new(move || {
                        let bus = Arc::clone(&bus);
                        Box::pin(async move {
                            bus.emit(Event {
                                from: "core",
                                kind: "app-exit",
                                payload: EventPayload::Framework(FrameworkEvent::AppExit),
                            });
                            Ok(())
                        })
                    })
                },
            },
            Command {
                id: "builtin.open_config",
                name: "打开配置目录".to_string(),
                description: "在文件管理器中打开 mybox 配置目录".to_string(),
                keywords: vec!["配置", "config", "peizhi"],
                hide_before_execute: false,
                runner: {
                    let opener = Arc::clone(&opener);
                    let config_dir = config_dir.clone();
                    Arc::new(move || {
                        let opener = Arc::clone(&opener);
                        let config_dir = config_dir.clone();
                        Box::pin(async move { opener(&config_dir) })
                    })
                },
            },
            Command {
                id: "builtin.restart",
                name: "重启应用".to_string(),
                description: "重启 mybox 应用".to_string(),
                keywords: vec!["重启", "restart", "chongqi"],
                hide_before_execute: false,
                runner: {
                    let bus = Arc::clone(&bus);
                    let spawner = Arc::clone(&spawner);
                    Arc::new(move || {
                        let bus = Arc::clone(&bus);
                        let spawner = Arc::clone(&spawner);
                        Box::pin(async move {
                            // D-13: `current_exe()` naturally resolves the cargo
                            // run artifact path in dev mode; the child is
                            // detached from the parent lifecycle by design
                            // (survives exit).
                            let exe = std::env::current_exe()?;
                            spawner(&exe)?;
                            bus.emit(Event {
                                from: "core",
                                kind: "app-exit",
                                payload: EventPayload::Framework(FrameworkEvent::AppExit),
                            });
                            Ok(())
                        })
                    })
                },
            },
            Command {
                id: "builtin.open_log",
                name: "打开日志文件".to_string(),
                description: "打开 mybox 运行日志".to_string(),
                keywords: vec!["日志", "log", "rizhi"],
                hide_before_execute: false,
                runner: {
                    let opener = Arc::clone(&opener);
                    let log_path = log_path.clone();
                    Arc::new(move || {
                        let opener = Arc::clone(&opener);
                        let log_path = log_path.clone();
                        Box::pin(async move { opener(&log_path) })
                    })
                },
            },
        ]
    }
}

/// Production file-manager opener: `open` (macOS) / `explorer` (Windows) with
/// the path as a single argument — never through a shell (T-3-07).
#[cfg(target_os = "macos")]
fn platform_opener() -> Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync> {
    Arc::new(|path: &Path| {
        std::process::Command::new("open").arg(path).spawn()?;
        Ok(())
    })
}

#[cfg(target_os = "windows")]
fn platform_opener() -> Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync> {
    Arc::new(|path: &Path| {
        std::process::Command::new("explorer").arg(path).spawn()?;
        Ok(())
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_opener() -> Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync> {
    Arc::new(|_path: &Path| Ok(()))
}

/// Production process spawner for restart (D-13): spawn the given executable
/// as a detached child; the current process then exits normally.
fn platform_spawner() -> Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync> {
    Arc::new(|exe: &Path| {
        std::process::Command::new(exe).spawn()?;
        Ok(())
    })
}

/// Run a command's async runner on a dedicated named worker thread (D-07).
///
/// The runner future is driven by `pollster::block_on` (no full async runtime
/// this phase — RESEARCH Pattern 4); the completion result hops back to the
/// winit main thread through `UiThreadProxy::run`.
pub fn run_command(
    cmd: Command,
    ui: &UiThreadProxy,
    on_done: Box<dyn FnOnce(anyhow::Result<()>) + Send>,
) {
    // Clone before spawning: the worker thread outlives this call.
    let ui = ui.clone();
    std::thread::Builder::new()
        .name(format!("mybox-cmd-{}", cmd.id))
        .spawn(move || {
            let result = pollster::block_on((cmd.runner)());
            ui.run(Box::new(move || on_done(result)));
        })
        .expect("spawn command runner thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventFilter;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
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

    fn sample_command(id: &'static str) -> Command {
        Command {
            id,
            name: format!("command {id}"),
            description: "test command".to_string(),
            keywords: vec![],
            hide_before_execute: false,
            runner: Arc::new(|| Box::pin(async { Ok(()) })),
        }
    }

    fn noop_opener() -> Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync> {
        Arc::new(|_| Ok(()))
    }

    fn noop_spawner() -> Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync> {
        Arc::new(|_| Ok(()))
    }

    fn builtins() -> Vec<Command> {
        BuiltinCommands::build_with(
            Arc::new(EventBus::new()),
            PathBuf::from("/tmp/mybox-test-config"),
            PathBuf::from("/tmp/mybox-test-config/logs/mybox.log"),
            noop_opener(),
            noop_spawner(),
        )
    }

    #[test]
    fn registry_rejects_duplicate_command_id() {
        let mut reg = CommandRegistry::new();
        reg.register(sample_command("capture.start")).expect("first ok");
        let err = reg
            .register(sample_command("capture.start"))
            .expect_err("duplicate id must fail");
        assert!(matches!(err, MyboxError::Command(_)));
        assert!(err.to_string().contains("capture.start"));
        assert_eq!(reg.len(), 1, "registry must be unchanged after rejection");
    }

    #[test]
    fn registry_preserves_registration_order() {
        let mut reg = CommandRegistry::new();
        reg.register(sample_command("first")).unwrap();
        reg.register(sample_command("second")).unwrap();
        reg.register(sample_command("third")).unwrap();
        let all = reg.all();
        let ids: Vec<&'static str> = all.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["first", "second", "third"]);
        assert_eq!(reg.len(), 3);
    }

    #[test]
    fn registry_default_is_empty() {
        let reg = CommandRegistry::default();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn builtin_quit_emits_app_exit() {
        let bus = Arc::new(EventBus::new());
        let seen: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        bus.on(EventFilter::all(), Box::new(move |e| s.lock().unwrap().push(e.clone())));

        let cmds = BuiltinCommands::build_with(
            Arc::clone(&bus),
            PathBuf::from("/tmp/cfg"),
            PathBuf::from("/tmp/cfg/logs/mybox.log"),
            noop_opener(),
            noop_spawner(),
        );
        let quit = cmds.iter().find(|c| c.id == "builtin.quit").expect("quit exists");
        pollster::block_on((quit.runner)()).expect("quit runner ok");

        assert!(wait_until(|| seen.lock().unwrap().len() == 1), "app-exit never emitted");
        let e = &seen.lock().unwrap()[0];
        assert_eq!(e.from, "core");
        assert_eq!(e.kind, "app-exit");
        assert!(matches!(e.payload, EventPayload::Framework(FrameworkEvent::AppExit)));
    }

    #[test]
    fn builtin_restart_spawns_current_exe_then_emits_exit() {
        let bus = Arc::new(EventBus::new());
        let seen: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        bus.on(EventFilter::all(), Box::new(move |e| s.lock().unwrap().push(e.clone())));

        let spawns = Arc::new(AtomicUsize::new(0));
        let sc = spawns.clone();
        let spawned_path: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
        let sp = spawned_path.clone();
        let spawner: Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync> =
            Arc::new(move |p: &Path| {
                sc.fetch_add(1, Ordering::SeqCst);
                *sp.lock().unwrap() = Some(p.to_path_buf());
                Ok(())
            });

        let cmds = BuiltinCommands::build_with(
            Arc::clone(&bus),
            PathBuf::from("/tmp/cfg"),
            PathBuf::from("/tmp/cfg/logs/mybox.log"),
            noop_opener(),
            spawner,
        );
        let restart = cmds.iter().find(|c| c.id == "builtin.restart").expect("restart exists");
        pollster::block_on((restart.runner)()).expect("restart runner ok");

        assert_eq!(spawns.load(Ordering::SeqCst), 1, "restart must spawn once");
        assert_eq!(
            *spawned_path.lock().unwrap(),
            std::env::current_exe().ok(),
            "restart must spawn the current executable (D-13)"
        );
        assert!(
            wait_until(|| seen.lock().unwrap().iter().any(|e| e.kind == "app-exit")),
            "app-exit never emitted after spawn"
        );
    }

    #[test]
    fn builtin_open_config_and_log_use_opener_with_correct_paths() {
        let opened: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        let o = opened.clone();
        let opener: Arc<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync> =
            Arc::new(move |p: &Path| {
                o.lock().unwrap().push(p.to_path_buf());
                Ok(())
            });

        let config_dir = PathBuf::from("/tmp/config-dir");
        let log_path = PathBuf::from("/tmp/config-dir/logs/mybox.log");
        let cmds = BuiltinCommands::build_with(
            Arc::new(EventBus::new()),
            config_dir.clone(),
            log_path.clone(),
            opener,
            noop_spawner(),
        );
        let open_config = cmds.iter().find(|c| c.id == "builtin.open_config").unwrap();
        let open_log = cmds.iter().find(|c| c.id == "builtin.open_log").unwrap();
        pollster::block_on((open_config.runner)()).expect("open_config runner ok");
        pollster::block_on((open_log.runner)()).expect("open_log runner ok");

        let got = opened.lock().unwrap();
        assert_eq!(*got, vec![config_dir, log_path], "opener paths must match exactly");
    }

    #[test]
    fn all_builtins_have_nonempty_name_and_description() {
        let cmds = builtins();
        assert_eq!(cmds.len(), 4);
        let mut ids: Vec<&'static str> = cmds.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            vec![
                "builtin.open_config",
                "builtin.open_log",
                "builtin.quit",
                "builtin.restart",
            ]
        );
        for cmd in &cmds {
            assert!(!cmd.name.is_empty(), "{}: name must be non-empty", cmd.id);
            assert!(!cmd.description.is_empty(), "{}: description must be non-empty", cmd.id);
        }
    }

    #[test]
    fn builtin_keywords_include_pinyin_aliases() {
        // GAP-7 prefix discovery: without an IME (or with the OS IME
        // disabled) users must still be able to hit the Chinese builtins via
        // pinyin — the same keyword-tier data mechanism as capture's "jietu".
        // This test locks the data so the aliases can never silently drop.
        let cmds = builtins();
        let quit = cmds.iter().find(|c| c.id == "builtin.quit").expect("quit exists");
        assert!(quit.keywords.contains(&"tuichu"), "quit must carry the tuichu alias");
        let open_config = cmds
            .iter()
            .find(|c| c.id == "builtin.open_config")
            .expect("open_config exists");
        assert!(
            open_config.keywords.contains(&"peizhi"),
            "open_config must carry the peizhi alias"
        );
        let restart = cmds
            .iter()
            .find(|c| c.id == "builtin.restart")
            .expect("restart exists");
        assert!(
            restart.keywords.contains(&"chongqi"),
            "restart must carry the chongqi alias"
        );
        let open_log = cmds
            .iter()
            .find(|c| c.id == "builtin.open_log")
            .expect("open_log exists");
        assert!(open_log.keywords.contains(&"rizhi"), "open_log must carry the rizhi alias");
    }

    #[test]
    fn run_command_runs_runner_on_named_thread() {
        let ui = UiThreadProxy::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let thread_name: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let tn = thread_name.clone();

        let cmd = Command {
            id: "test.cmd",
            name: "test".to_string(),
            description: "test".to_string(),
            keywords: vec![],
            hide_before_execute: false,
            runner: Arc::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
                *tn.lock().unwrap() = std::thread::current()
                    .name()
                    .unwrap_or_default()
                    .to_string();
                Box::pin(async { Ok(()) })
            }),
        };
        run_command(cmd, &ui, Box::new(|_| {}));

        assert!(wait_until(|| count.load(Ordering::SeqCst) == 1), "runner never ran");
        assert_eq!(
            *thread_name.lock().unwrap(),
            "mybox-cmd-test.cmd",
            "runner must run on the named worker thread"
        );
    }
}
