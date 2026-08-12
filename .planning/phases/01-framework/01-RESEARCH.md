# Phase 1: 框架核心 - Research

**Researched:** 2026-08-11
**Status:** Ready for planning
**Scope:** What is needed to PLAN Phase 1 well (framework core: Module trait, EventBus, WindowManager, HotkeyManager, TrayManager, ConfigCenter, Renderer)
**Method:** All crate APIs verified against downloaded crate sources (crates.io via cargo), not assumed from CLAUDE.md which is stale. Dependency matrix verified with `cargo metadata`.

---

## 0. Executive Summary (read this first)

1. **CLAUDE.md's stack table is stale.** Current verified versions (2026-08): winit **0.30.13**, egui/egui-winit **0.36.1**, tiny-skia **0.12.0**, softbuffer **0.4.8**, global-hotkey **0.8.0**, tray-icon **0.24.2** (+ muda 0.19.3), crossbeam-channel 0.5.16, toml 1.1.4, directories 6.0.0, objc2 0.6.4. All resolve together (verified). winit 0.31 is only a **beta** — do NOT use it.

2. **`egui-tiny-skia` does NOT exist on crates.io** (verified via `cargo search`). Decision D-01's mechanism is unavailable. `egui_skia` uses real Skia C++ bindings (violates the no-native-deps constraint); `egui_software_backend` 0.0.3 is a CPU software rasterizer (immature, egui 0.34, does not use tiny-skia). **Recommendation: Phase 1 Renderer is pure tiny-skia; egui integration is deferred to Phase 3 and must be hand-rolled** (tessellate egui meshes into the tiny-skia Pixmap). Full rationale in §5.

3. **winit 0.30.13 natively supports macOS Accessory mode** — `winit::platform::macos::EventLoopBuilderExtMacOS::with_activation_policy(ActivationPolicy::Accessory)`. **objc2 is NOT required for FRMW-06.** This removes the biggest "macOS platform adaptation" risk from the phase.

4. **global-hotkey 0.8 already parses human hotkey strings** (`"Cmd+Shift+S".parse::<HotKey>()` implements `FromStr`). D-11 (string hotkey config) needs no custom parser.

5. **softbuffer 0.4.8 has NO per-pixel alpha on macOS** (verified: backend uses `CGImageAlphaInfo::NoneSkipFirst`, pixel format `0x00RRGGBB`). Transparent overlay windows (FRMW-03's Overlay, Phase 2's screenshot overlay) cannot be done with softbuffer alone. Phase 1: create Overlay windows with transparent window attributes but render opaque test content; real alpha compositing for overlay windows lands in Phase 2 (needs objc2 CALayer with RGBA CGImage, or an alpha-capable path).

6. **Hotkey/tray/winit integration is solved and documented** by the crates themselves: `EventLoopBuilder::with_user_event::<AppEvent>()` + `EventLoopProxy::send_event()` + `GlobalHotKeyEvent::set_event_handler` / `TrayIconEvent::set_event_handler` / `MenuEvent::set_event_handler`. Keep `ControlFlow::Wait` (no polling). This satisfies D-08's intent (independent listener threads, decoupled from winit's own event sources).

7. **The event bus design (D-04/D-05) needs one reconciliation:** handlers that touch winit windows can only run on the main thread. Recommend the bus's worker thread dispatches logic handlers; UI-touching handlers forward to the winit loop via a proxy helper (`ctx.ui_proxy()`). See §2.2.

8. **Softbuffer + winit + tiny-skia compose correctly:** softbuffer `buffer_mut()` → `&mut [u32]` in `0x00RRGGBB`; tiny-skia `Pixmap::data()` is premultiplied RGBA bytes; conversion is a trivial per-pixel shuffle for opaque content (Phase 1 test windows).

---

## 1. Verified Version Matrix (2026-08)

Pinned versions for Phase 1 (all verified to resolve together via `cargo metadata`; the `crates.io-1949cf8c6b5b557f` registry sources were inspected directly):

| Crate | Version | Phase 1 use | Notes (verified from source) |
|-------|---------|-------------|------------------------------|
| winit | **0.30.13** | windows + event loop | `ApplicationHandler` trait; `EventLoopBuilder::with_user_event()`; `EventLoopProxy::send_event`; `ActiveEventLoop::create_window`; macOS `with_activation_policy`. **egui-winit 0.36.1 requires `winit = "0.30.13"`** — pin exactly. 0.31.0-beta.2 must be avoided. |
| egui / egui-winit / epaint | **0.36.1** | (Phase 3 for UI; not a Phase 1 dep) | `Context::run/begin_pass`, `FullOutput { platform_output, textures_delta, shapes, pixels_per_point, viewport_output }`, `Context::tessellate(shapes, ppi) -> Vec<ClippedPrimitive>`. egui-winit `State::new(ctx, ViewportId, &dyn HasDisplayHandle, native_ppi, theme, max_tex_side)`, `on_window_event() -> EventResponse`, `take_egui_input(&Window)`, `handle_platform_output(&Window, PlatformOutput)`. |
| tiny-skia | **0.12.0** | Renderer backend | `Pixmap::new(w,h)`; draw via `Pixmap::fill_rect/fill_path/stroke_path/draw_pixmap(rect, path, paint, transform, mask)`; premultiplied RGBA u8 in `Pixmap::data()`. |
| softbuffer | **0.4.8** | present framebuffer | `Context::new(display_handle)`, `Surface::new(&ctx, window_handle)`, `resize(NonZeroU32, NonZeroU32)`, `buffer_mut() -> Buffer` (derefs to `&mut [u32]`, format `0x00RRGGBB`), `present()`. macOS backend = CoreGraphics with `CGImageAlphaInfo::NoneSkipFirst` (NO alpha). |
| global-hotkey | **0.8.0** | global hotkeys | `GlobalHotKeyManager::new()`, `manager.register(HotKey)`, `HotKey::new(Option<Modifiers>, Code)`; **`HotKey: FromStr/Display/TryFrom<&str>`** parses `"Cmd+Shift+S"`, `"F1"`; `GlobalHotKeyEvent::receiver()` (crossbeam) + `set_event_handler(Some(closure))`; `serde` feature (off by default) for config round-trip. **Must create manager on main thread (macOS).** |
| tray-icon | **0.24.2** | tray + menu | `TrayIconBuilder::new().with_menu(Box<dyn ContextMenu>).with_icon(...).build()`; re-exports muda as `tray_icon::menu::*`; `TrayIconEvent::set_event_handler` + `menu::MenuEvent::set_event_handler` (docs example uses `EventLoopProxy`). |
| muda | **0.19.3** | menu items | `Menu::new()`, `menu.append(&item)`, `MenuItem::new(text, enabled, accelerator)`, `PredefinedMenuItem::separator()`, `PredefinedMenuItem::quit(Some("退出"))`. |
| crossbeam-channel | **0.5.16** | EventBus + bridging | Already a transitive dep of global-hotkey/tray-icon/muda → zero extra compile cost. MPMC + `select!`. |
| serde / serde_json | 1.x | config/events | |
| toml | **1.1.4** | config file | |
| directories | **6.0.0** | config path | `ProjectDirs::from("", "", "mybox")` → macOS `~/Library/Application Support/mybox/` (INFRA-04). |
| parking_lot | 0.12.5 | shared state | |
| anyhow / thiserror | 1.x / 2.x | errors (INFRA-03) | thiserror 2.0 note: variant attribute changes (`#[error]`/`#[from]` unchanged for common cases). |
| log + env_logger | 0.4 / 0.11 | logging (INFRA-03) | |
| objc2 | **0.6.4** | (Phase 2 for screenSaver level / Screen Recording; NOT required for Phase 1) | winit covers activation policy. |

Windows-only (`#[cfg(windows)]`, Phase 4): `windows` crate optional.

---

## 2. Component-by-Component Technical Approach

### 2.1 Module trait + ModuleContext (FRMW-01, FRMW-02)

**Pattern:** trait object + builder registration, per ARCHITECTURE.md Pattern 1. Modules are `Send + Sync + 'static`, typically zero-sized structs; any per-instance state lives inside the module (Arc/OnceLock) or in shared core services.

**Recommended trait surface** (satisfies FRMW-01, INFRA-02 via menu_items, D-13 via default_config):

```rust
// module.rs
pub trait Module: Send + Sync + 'static {
    /// Unique module id, used as event namespace ("capture", "palette") and config section name.
    fn id(&self) -> &'static str;
    fn name(&self) -> &str;

    /// Called once at startup after config is loaded. Register event handlers here (D-13 default config is
    /// queried by ConfigCenter BEFORE init so the default file can be generated first-run).
    fn init(&self, ctx: &ModuleContext) -> Result<()>;

    /// Default config section merged into config.toml on first run (D-13).
    fn default_config(&self) -> toml::Table { toml::Table::new() }

    /// Tray context-menu items contributed to the shared tray menu (INFRA-02).
    fn menu_items(&self) -> Vec<MenuItem> { vec![] }

    /// Optional ordered cleanup on exit.
    fn shutdown(&self, _ctx: &ModuleContext) {}
}
```

**ModuleContext** — the only thing modules see of the core. A lightweight facade over `Arc`-backed services:

```rust
pub struct ModuleContext {
    bus: Arc<EventBus>,
    windows: Arc<WindowManagerHandle>,   // main-thread-bound; see §2.3
    config: Arc<ConfigCenter>,
    hotkeys: Arc<HotkeyManager>,
    ui: UiThreadProxy,                   // forwards closures/events to winit loop (see §2.2)
}

impl ModuleContext {
    pub fn emit(&self, event: Event) -> Result<()>;                       // non-blocking publish
    pub fn on(&self, filter: EventFilter, handler: impl Fn(&Event) + Send + Sync + 'static) -> SubscriptionId;
    pub fn windows(&self) -> &Arc<WindowManagerHandle>;
    pub fn config(&self) -> &Arc<ConfigCenter>;
    pub fn ui(&self) -> &UiThreadProxy;                                    // "run this closure on the winit main thread"
}
```

**AppBuilder registration** (FRMW-01, "编译期通过 AppBuilder 注册"):

```rust
let app = App::builder()
    .module(TestModule::new())
    .build()?;   // registers modules, creates EventBus + ConfigCenter, generates/loads config
app.run()?;      // creates EventLoop + HotkeyManager + TrayManager, runs loop
```

**App::run() lifecycle (the "skeleton"):**
1. `EventLoopBuilder::with_user_event::<AppEvent>()`, apply macOS activation policy.
2. Create ConfigCenter (load or generate default file from all `Module::default_config()`).
3. Create HotkeyManager + register hotkeys from config (main thread — required on macOS).
4. Build tray menu from all `menu_items()` + separator + 退出; create TrayManager.
5. Install event forwarders (hotkey/tray/menu → `EventLoopProxy`).
6. `event_loop.run_app(&mut app)`; `App` implements `ApplicationHandler<AppEvent>`.
7. In `resumed()`: create any startup windows (none for Phase 1; modules create windows on demand).

### 2.2 EventBus (FRMW-02, FRMW-05, D-04/D-05/D-06)

**Data model (D-06 hybrid payload):**

```rust
// event.rs
#[derive(Clone)]
pub struct Event {
    pub from: &'static str,     // module id or "core"
    pub kind: &'static str,     // "screenshot-taken", "window-created", ...
    pub payload: EventPayload,
}

#[derive(Clone)]
pub enum EventPayload {
    /// Typed framework events (window lifecycle, hotkey, module) - D-06 "typed for core".
    Framework(FrameworkEvent),
    /// Module-defined events as freeform JSON - D-06 "JSON for modules".
    Module(serde_json::Value),
}

#[derive(Clone)]
pub enum FrameworkEvent {
    WindowCreated(u64), WindowDestroyed(u64),
    HotkeyTriggered(u32),              // global-hotkey id
    ModuleLoaded(&'static str),
    AppReady, AppExit,
}
```

**Filter (D-05 broadcast + wildcard):**

```rust
#[derive(Clone, PartialEq)]
pub struct EventFilter { pub from: &'static str, pub kind: &'static str }  // "*" = wildcard

impl EventFilter {
    pub fn all() -> Self { Self { from: "*", kind: "*" } }
    pub fn kind(from: &'static str, kind: &'static str) -> Self;
    pub fn matches(&self, e: &Event) -> bool;   // glob compare; "capture:*" matches any kind with from=="capture"
}
```

**Transport (D-04 async channel, non-blocking publish):**

- Backend: `crossbeam_channel::unbounded::<Event>()`. `emit()` = `sender.send(event)` — never blocks the event loop (FRMW-05).
- A dedicated **bus worker thread** `recv()`s events and dispatches to every registered handler whose filter matches (D-05 broadcast semantics: all subscribers receive; each handler's filter decides).
- **Reconciliation with winit (important):** winit windows are main-thread-bound. Options:
  - *(recommended)* Bus thread dispatches pure-logic handlers; UI-touching handlers forward to the winit loop via `ctx.ui()` (an `EventLoopProxy<AppEvent>`-backed helper `UiThreadProxy::run(Box<dyn FnOnce + Send>)`). This honors D-04's "background thread receives and dispatches" literally and keeps the event loop free.
  - *(simpler alternative)* Drain the bus channel on the main loop (in `about_to_wait()` or on a `UserEvent::BusTick`), dispatch handlers on the main thread. Less threading, but "background thread" from D-04 becomes just the publish side. Modules needing heavy work spawn their own threads.
- **Planner must lock one of these.** The first is recommended to keep D-04's letter; the second is simpler and still satisfies FRMW-05. If the second is chosen, publish stays channel-based and the loop drain is non-blocking.

**Handler storage:** `Arc<parking_lot::Mutex<Vec<(EventFilter, Box<dyn Fn(&Event) + Send + Sync>)>>>`; `on()` returns a `SubscriptionId` (u64) for future unsubscribe (v2).

### 2.3 WindowManager (FRMW-03, D-07, D-09)

**Ownership model:** WindowManager is main-thread-bound (it holds `winit::Window`s, which are not `Send`). It is stored inside `App` (the `ApplicationHandler`). Modules never touch it directly — they call `ctx.windows().create(spec)` which enqueues a request; `App` executes it on the main thread in `about_to_wait()`/next loop iteration, then emits `FrameworkEvent::WindowCreated(id)`.

**Types (D-07 centralized + ID dispatch):**

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WindowKind { Overlay, Floating, Panel }        // FRMW-03

pub struct WindowSpec {
    pub kind: WindowKind,
    pub title: String,
    pub transparent: bool,          // winit with_transparent (NOTE: alpha not blittable via softbuffer on macOS, §0.5)
    pub always_on_top: bool,        // winit with_window_level(WindowLevel::AlwaysOnTop)
    pub decorations: bool,
    pub visible: bool,
    pub inner_size: Option<(u32, u32)>,       // physical px
    pub position: Option<(i32, i32)>,         // physical px
    pub on_event: Option<Box<dyn Fn(&WindowEvent) + Send + Sync>>,  // per-window callback (D-07 routing target)
}

pub struct WindowState {
    pub id: WindowId,               // mybox u64 id (incrementing counter or uuid)
    pub kind: WindowKind,
    pub winit_id: winit::window::WindowId,
    pub window: winit::Window,
    pub renderer: Box<dyn Renderer>,  // per-window Pixmap + softbuffer surface (D-02 per-window composite)
    pub spec: WindowSpec,
}

pub struct WindowManager {
    states: HashMap<WindowId, WindowState>,
    next_id: u64,
    renderer_factory: Box<dyn Fn(&winit::Window) -> Box<dyn Renderer>>,  // injectable for tests
}
```

**Key APIs:**
- `create(spec) -> WindowId` — enqueue; executed on main thread via `ActiveEventLoop::create_window(WindowAttributes)` (build attrs via `Window::default_attributes().with_*`, §5 list verified).
- `destroy(id)` / `close_all()`.
- `batch_create(Vec<WindowSpec>) -> Vec<WindowId>` — reserved for D-09 (per-monitor overlay, Phase 2), but the signature is defined in Phase 1.
- `get_mut(id) -> Option<&mut WindowState>`.
- `window_event(event_loop, winit_id, event)` — **the routing hot path**: look up state by `winit_id`, invoke `state.spec.on_event(&event)` (module callback) and let the renderer react to `RedrawRequested`.

**Window attributes builder for each kind** (verified winit 0.30.13 methods):
- Overlay: `with_transparent(true)`, `with_decorations(false)`, `with_window_level(WindowLevel::AlwaysOnTop)`, `with_visible(true)`, fullscreen via per-monitor `with_position`+`with_inner_size` (D-09, Phase 2).
- Floating (pin-type, Phase 2): `with_decorations(false)`, `with_window_level(AlwaysOnTop)`.
- Panel: `with_decorations(true)`, `with_title`, normal level.

**Redraw loop:** window created with `visible`; module calls `window.request_redraw()`; `WindowEvent::RedrawRequested` → `renderer.paint()` → softbuffer `present()`.

### 2.4 HotkeyManager (FRMW-04, D-11)

**Verified global-hotkey 0.8 API:**

```rust
use global_hotkey::{GlobalHotKeyManager, hotkey::{HotKey, Modifiers, Code}};

let manager = GlobalHotKeyManager::new()?;          // macOS: MUST be on main thread
let hotkey: HotKey = "Cmd+Shift+S".parse()?;        // D-11: string config -> HotKey (FromStr built-in)
manager.register(hotkey)?;                          // returns Result; HotKey has .id() (u32)
manager.unregister(hotkey)?;
```

**Design:**
- `HotkeyManager { manager: GlobalHotKeyManager, map: HashMap<u32, String /* action name */>, config: Arc<ConfigCenter> }`.
- Reads hotkeys from config at startup (D-11 strings), parses via `HotKey::from_str`, registers, and records id→action.
- Hotkey trigger events flow: `GlobalHotKeyEvent::set_event_handler(move |e| { let _ = proxy.send_event(AppEvent::Hotkey(e)); })` (see §4).
- On `AppEvent::Hotkey(e)`, `App` maps `e.id` → action → emits `Event { from: "core", kind: "hotkey.triggered", payload: Framework(HotkeyTriggered(id)) }` into the bus. Modules subscribe with `EventFilter::kind("core", "hotkey.triggered")` and inspect the id (or the bus is keyed by action name — planner's discretion).
- `serde` feature on global-hotkey enables `HotKey` serde derive if we ever store parsed keys (recommend keeping config as plain strings per D-11 and only parsing on load).

### 2.5 TrayManager (INFRA-02)

**Verified tray-icon 0.24.2 API:**

```rust
use tray_icon::{TrayIconBuilder, TrayIconEvent, menu::{Menu, MenuItem, PredefinedMenuItem}};

let menu = Menu::new();
menu.append(&MenuItem::new("开始截图", true, None))?;      // from Module::menu_items()
menu.append(&PredefinedMenuItem::separator())?;
menu.append(&PredefinedMenuItem::quit(Some("退出")))?;

let _tray = TrayIconBuilder::new()
    .with_menu(Box::new(menu))
    .with_icon(icon)                      // Icon::from_rgba(rgba_data, w, h)
    .with_icon_as_template(true)          // macOS menu bar template (monochrome)
    .build()?;
```

**Design:**
- `TrayManager { _tray: TrayIcon, menu: Menu }` — one shared menu built at startup from all modules' `menu_items()` (INFRA-02: "右键菜单展示模块注册的菜单项和退出按钮").
- **Icon without an asset file:** generate a small monochrome bitmap at startup with tiny-skia → `tray_icon::Icon::from_rgba(data, w, h)`. Avoids bundling images. (Planner discretion: asset file is also fine.)
- Menu clicks: `menu::MenuEvent::set_event_handler(move |e| { let _ = proxy.send_event(AppEvent::Menu(e)); })`. `App` maps `MenuEvent.id` (a string menu id) → action → emits bus event. Set `MenuItem` ids (`MenuItem::with_id("capture.start", ...)`) so the id round-trips.
- `TrayIconEvent::set_event_handler` for click/enter/leave (optional in Phase 1).

### 2.6 ConfigCenter (INFRA-01, INFRA-04, D-10/D-12/D-13)

**Design (all decisions D-10/D-11/D-12/D-13 map 1:1):**

- **Location (INFRA-04):** `directories::ProjectDirs::from("", "", "mybox")` → macOS `~/Library/Application Support/mybox/config.toml` (verified semantics). Create dir if missing.
- **In-memory cache (D-10):** load once at startup into `Arc<parking_lot::RwLock<toml::Table>>`. Reads hit memory; `save()` serializes the whole table back to TOML. No watch/reload (v2 INFRA-EX-02).
- **First-run generation (D-12/D-13):** if the file does not exist, collect `module.default_config()` for every registered module, merge into a skeleton, write it, then load. Result: user gets a complete commented `config.toml` on first launch.
- **Module namespace isolation (INFRA-01):** each module's config lives under `[module_id]`. Hotkeys under a framework-owned `[hotkeys]` section.
- **API:**

```rust
impl ConfigCenter {
    pub fn load_or_create(modules: &[&dyn Module]) -> Result<Self>;
    pub fn get(&self, module: &str, key: &str) -> Option<toml::Value>;
    pub fn get_section(&self, module: &str) -> Option<&toml::Table>;
    pub fn set(&self, module: &str, key: &str, value: toml::Value);   // memory
    pub fn save(&self) -> Result<()>;                                 // full write-back (D-10)
    pub fn hotkey(&self, action: &str) -> Result<Option<global_hotkey::hotkey::HotKey>>; // D-11: string -> HotKey via FromStr
}
```

- **Error handling (INFRA-03):** `thiserror` for core errors (`MyboxError::ConfigNotFound`, `::ConfigParse`, `::HotkeyParse`, `::Window`), `anyhow` at the app boundary; `log` on every key operation (file create/load/save, hotkey register failure).

### 2.7 Renderer (D-01/D-02/D-03)

**Abstraction (D-03):** modules see a `Renderer` trait; core owns tiny-skia + softbuffer.

```rust
// renderer/mod.rs
pub trait Renderer: Send {
    fn resize(&mut self, width: u32, height: u32);
    /// Draw custom content: the closure receives the tiny-skia PixmapMut + size.
    fn draw(&mut self, f: &mut dyn FnMut(&mut tiny_skia::PixmapMut, u32, u32));
    /// Present the composited Pixmap to the window (softbuffer).
    fn present(&mut self) -> Result<()>;
}

// renderer/tiny_skia_softbuffer.rs  (Phase 1 backend)
pub struct TinySkiaSoftbufferRenderer {
    surface: softbuffer::Surface<D, W>, // W5: softbuffer 0.4 Context<D>/Surface<D,W> generics are raw-window-handle display/window handle types (inferred as winit::Window), NOT PhysicalSize<u32>; don't write the generic explicitly
    pixmap: tiny_skia::Pixmap,
    // (Phase 3) egui overlay state
}
```

**Per-window composite pipeline (D-02, per window):**
1. `RedrawRequested` → `pixmap.fill(clear_color)`.
2. Run the module's draw closure against `pixmap.as_mut()` (tiny-skia paths/shapes).
3. *(Phase 3)* Run egui layer: `egui_winit_state.take_egui_input(&window)` → `ctx.run(input, ui_fn)` → `ctx.tessellate(out.shapes, out.pixels_per_point)` → draw meshes onto the same pixmap (see §5).
4. Copy `pixmap.data()` (premultiplied RGBA bytes) into softbuffer's `&mut [u32]` as `0x00RRGGBB`.
5. `surface.present()`.

**Pixel conversion (pure function — unit-testable):** for opaque content (alpha=255), `u32 = (r<<16)|(g<<8)|b`. For general premultiplied RGBA → straight RGB: `r' = r*255/a` etc., then pack. NOTE: softbuffer drops alpha on macOS anyway (NoneSkipFirst); real alpha needs the Phase 2 overlay path (§0.5).

**Resize handling:** on `WindowEvent::Resized`, `renderer.resize(w, h)` → `softbuffer_surface.resize(NonZeroU32, NonZeroU32)` + `pixmap = Pixmap::new(w,h)`; then `request_redraw()`.

**Windows (Phase 4 note):** softbuffer on Win32 supports no-copy presentation and the same `0x00RRGGBB` format.

---

## 3. Cargo Workspace Structure

Per ARCHITECTURE.md structure, with verified crate layout for Phase 1:

```
mybox/
├── Cargo.toml                # [workspace]
│                             #   members = ["crates/mybox-core", "crates/mybox-app", "crates/modules/*"]
│                             #   resolver = "2"
├── crates/
│   ├── mybox-core/           # framework core; zero business logic
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs        # pub use: App, AppBuilder, Module, ModuleContext, Event*, Window*, Renderer...
│   │       ├── app.rs        # App/AppBuilder; ApplicationHandler<AppEvent> impl; run() lifecycle
│   │       ├── module.rs     # Module trait, ModuleContext
│   │       ├── event.rs      # Event, EventPayload, FrameworkEvent, EventFilter, EventBus
│   │       ├── window.rs     # WindowManager, WindowSpec, WindowKind, WindowId, WindowState
│   │       ├── hotkey.rs     # HotkeyManager
│   │       ├── tray.rs       # TrayManager + tray icon generation
│   │       ├── config.rs     # ConfigCenter
│   │       ├── renderer/
│   │       │   ├── mod.rs        # Renderer trait
│   │       │   └── tiny_skia_softbuffer.rs
│   │       ├── platform/     # #[cfg(target_os="macos")] activation policy helper; (Phase 2: screenSaver level)
│   │       └── error.rs      # MyboxError (thiserror)
│   ├── mybox-app/            # binary
│   │   └── src/main.rs       # env_logger + App::builder().module(TestModule).build().run()
│   └── modules/
│       └── test/             # TestModule: registers hotkey, creates test window on trigger, emits/on event
│           ├── Cargo.toml
│           └── src/lib.rs
```

**Workspace details:**
- `members = ["crates/mybox-core", "crates/mybox-app", "crates/modules/*"]` (explicit; avoids glob ambiguities until more modules exist).
- `[workspace.dependencies]` to centralize pinned versions (use the table in §1).
- `mybox-core` depends on: winit, softbuffer, tiny-skia, global-hotkey (serde), tray-icon, crossbeam-channel, serde, serde_json, toml, directories, parking_lot, anyhow, thiserror, log. (egui/egui-winit NOT in Phase 1 — deferred to Phase 3.)
- `mybox-app` depends on `mybox-core` + `mybox-test` (module) + `env_logger`.
- `mybox-test` (module) depends on `mybox-core` only — proves the module boundary (FRMW-02: modules don't depend on each other or on core internals).

---

## 4. winit 0.30 Event Loop + Hotkey/Tray Integration (the skeleton's heart)

**Verified winit 0.30.13 pattern (ApplicationHandler, not the old closure API — PITFALLS Pitfall 1):**

```rust
enum AppEvent {                       // mybox's own event namespace (NOT winit's native events)
    Hotkey(global_hotkey::GlobalHotKeyEvent),
    Tray(tray_icon::TrayIconEvent),
    Menu(tray_icon::menu::MenuEvent),
    Ui(Box<dyn FnOnce() + Send>),     // ui_proxy helper
}

struct App { /* ModuleRegistry, EventBus, WindowManager, ConfigCenter, HotkeyManager, TrayManager, egui state(Ph3) */ }

impl App {
    fn new() -> Result<Self> { /* build bus, config, modules */ }
    fn run(&mut self) -> Result<()> {
        let mut builder = winit::event_loop::EventLoopBuilder::with_user_event::<AppEvent>();
        #[cfg(target_os = "macos")]
        { use winit::platform::macos::EventLoopBuilderExtMacOS;
          builder.with_activation_policy(winit::platform::macos::ActivationPolicy::Accessory); } // FRMW-06 -> see §6

        let event_loop = builder.build()?;
        let proxy = event_loop.create_proxy();

        // HotkeyManager + TrayManager on MAIN thread (required on macOS), then forward:
        let proxy2 = proxy.clone();
        global_hotkey::GlobalHotKeyEvent::set_event_handler(Some(move |e| { let _ = proxy2.send_event(AppEvent::Hotkey(e)); }));
        let proxy3 = proxy.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |e| { let _ = proxy3.send_event(AppEvent::Tray(e)); }));
        let proxy4 = proxy.clone();
        tray_icon::menu::MenuEvent::set_event_handler(Some(move |e| { let _ = proxy4.send_event(AppEvent::Menu(e)); }));

        event_loop.run_app(self)?;
        Ok(())
    }
}

impl winit::application::ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
        // macOS app is now active. Safe place to create startup windows / lazy init.
    }

    fn window_event(&mut self, el: &winit::event_loop::ActiveEventLoop, id: winit::window::WindowId, event: winit::event::WindowEvent) {
        // D-07 routing: find WindowState by winit id -> dispatch to its on_event callback + renderer
        if let Some(state) = self.windows.get_mut_by_winit(id) {
            if let Some(cb) = &state.spec.on_event { cb(&event); }
            match event {
                winit::event::WindowEvent::RedrawRequested => { state.renderer.present().ok(); }
                winit::event::WindowEvent::Resized(size) => { state.renderer.resize(size.width, size.height); }
                winit::event::WindowEvent::CloseRequested => { self.windows.destroy(state.id); }
                _ => {}
            }
        }
    }

    fn user_event(&mut self, el: &winit::event_loop::ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Hotkey(e) => self.on_hotkey(e),   // map id -> action -> emit bus event
            AppEvent::Menu(e)   => self.on_menu(e),
            AppEvent::Ui(f)     => f(),                 // ui_proxy runs closures on main thread
            AppEvent::Tray(_)   => {}
        }
    }

    fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        // Optional: drain bus channel here if the "main-thread drain" bus option is chosen (§2.2).
        el.set_control_flow(winit::event_loop::ControlFlow::Wait);  // efficient; no polling
    }
}
```

**Key points:**
- `ControlFlow::Wait` + `EventLoopProxy` wakeups = idle app burns ~0 CPU; every hotkey/tray/menu event wakes the loop. This is exactly the tray-icon docs' recommended winit integration.
- **D-08 reconciliation:** the decision says "不使用 winit 的 UserEvent 集成，保持热键/托盘与窗口事件的解耦". The listener threads (global-hotkey/tray internals) ARE separate; `AppEvent` is mybox's enum, and `EventLoopProxy` is only the wake-up bridge into the loop. This is the only reliable way to wake a `Wait`-based loop from another thread and is what the crate authors document. Treat "不直接使用 UserEvent 集成" as "don't merge hotkey/tray into winit's native event sources" — the proxy delivery is standard and should be used. The planner should note this refinement explicitly so implementers don't try to poll channels with `ControlFlow::Poll` (wastes CPU).
- **macOS hotkey manager on main thread** (verified global-hotkey docs): create `GlobalHotKeyManager` in `main()` before `run_app`. Same for `TrayIconBuilder` (tray-icon on macOS is main-thread too).

---

## 5. tiny-skia + egui Integration (D-01/D-02) — IMPORTANT FINDING

**Verified: `egui-tiny-skia` does NOT exist on crates.io** (`cargo search egui-tiny-skia` → empty; `egui_tiny_skia` → unrelated crates). Options that DO exist:

| Option | What it is | Verdict for mybox |
|--------|-----------|-------------------|
| `egui_skia` 0.4.0 | egui rendered into **real Skia** via `skia-safe` (skia C++ bindings) | Rejected — native C++ dep, violates "no native dependencies" constraint. |
| `egui_software_backend` 0.0.3 | Pure-Rust CPU software rasterizer for egui → renders into a user buffer (`BufferMutRef` over `&mut [[u8;4]]`) or softbuffer; egui **0.34**, winit 0.30 | Interesting fallback, but: 0.0.x immature, egui 0.34 mismatch (we're on 0.36), single-maintainer, does NOT use tiny-skia (separate pipeline, can't share the D-02 Pixmap). Not recommended for the foundation. |
| **Hand-rolled (recommended for Phase 3)** | Implement an egui→tiny-skia painter in mybox-core | Keeps the unified tiny-skia pipeline (D-01/D-02), zero native deps, full control. ~300–500 lines. |

**Recommendation for the phase plan:**
- **Phase 1 Renderer = pure tiny-skia** (`TinySkiaSoftbufferRenderer`). Phase 1's own success criteria need no egui: the test window shows tiny-skia content (colored panel/shape). This also sidesteps PITFALLS Pitfall 5's egui/tiny-skia conflict entirely in Phase 1.
- **Defer egui to Phase 3** (command palette is the first UI-heavy window). Design the `Renderer` trait in Phase 1 so an egui overlay layer slots in later (the `draw()` closure already isolates content from compositing).
- **Document the Phase 3 manual integration now** (so the decision D-01 is not silently dropped):

```rust
// Phase 3 sketch — egui layer onto the same tiny-skia Pixmap (manual integration):
let raw_input = egui_winit_state.take_egui_input(&window);
let full_output = egui_ctx.run(raw_input, |ctx| { /* build UI */ });
egui_winit_state.handle_platform_output(&window, full_output.platform_output);
let primitives = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
// textures_delta: upload new textures to a tiny_skia::Pixmap texture cache (font atlas etc.)
for clipped in primitives {
    for tri in clipped.primitive.indices.chunks(3) {
        let [a, b, c] = tri; // three vertices: pos (Pos2), uv (Vec2), color (Color32)
        let mut pb = tiny_skia::PathBuilder::new();
        // move_to/line_to/close for the 3 points
        let mut paint = tiny_skia::Paint::default();
        if clipped.primitive.texture_id.is_default() {
            paint.set_color_rgba8(r, g, b, a);          // flat color from vertex color
        } else {
            paint.shader = tiny_skia::Pattern::new(tex_pixmap, SpreadMode::Pad, FilterQuality::Bilinear).into();
            // set Pattern transform so uv maps correctly
        }
        pixmap.fill_path(&pb.finish().unwrap(), &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
    }
    // clip_rect handling via Mask / clip path (the tricky part)
}
```

Notes for Phase 3 planning: per-triangle `fill_path` is fast enough at UI geometry scale; texture glyphs are drawn via `Pattern` shaders; `clip_rect` needs tiny-skia `Mask` (`PixmapMut::apply_mask`) or clip-path construction; reuse the Pixmap + texture cache across frames. If this proves too fiddly, `egui_software_backend` (or a re-evaluated decision) is the fallback — decided in Phase 3, NOT Phase 1.

---

## 6. macOS-Specific (FRMW-06 + future hooks)

1. **Accessory mode / no Dock icon (FRMW-06) — SOLVED by winit, no objc2:**
   ```rust
   use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
   builder.with_activation_policy(ActivationPolicy::Accessory);
   ```
   Verified in winit 0.30.13 (`ActivationPolicy::{Regular, Accessory, Prohibited}`). Accessory = no Dock icon, windows can still get focus.

2. **Overlay window keyboard focus (FRMW-06 second half):** after creating an overlay window, call `window.focus_window()` (verified winit method). If more aggressive activation is needed on macOS, `ActiveEventLoopExtMacOS` offers `hide_application()` etc.; full `NSApp.activate(ignoringOtherApps:)` via objc2 is a Phase 2 refinement if focus proves flaky.

3. **Window level:** winit `WindowLevel::{AlwaysOnBottom, Normal, AlwaysOnTop}` (verified). `AlwaysOnTop` maps to floating level — **NOT** the `screenSaver` level needed to cover fullscreen apps/menu bar. Phase 2 overlay must set `NSWindow.level = .screenSaver` via objc2 (PITFALLS Pitfall 3 / macOS note). Phase 1 test window only needs `AlwaysOnTop` — no objc2.

4. **Screen Recording permission:** NOT in Phase 1 (CAP-08 → Phase 2). For Phase 2: `objc2` + `CGPreflightScreenCaptureAccess()` (from `objc2-core-graphics`), guide user to System Settings via `x-apple.systempreferences:...?Privacy_ScreenCapture` deep link. PITFALLS Pitfall 2. Research exists; no Phase 1 action.

5. **Softbuffer alpha caveat:** macOS softbuffer uses `CGImageAlphaInfo::NoneSkipFirst` → no per-pixel alpha → transparent Overlay content via softbuffer is impossible. Phase 2 must present overlay content through an alpha-capable layer (objc2 CALayer with an RGBA CGImage, or render the dim mask into the (opaque) screenshot). Phase 1: use `with_transparent(true)` window attrs for the Overlay test window; rendered test content is opaque — acceptable for framework validation.

6. **Tray icon:** `.with_icon_as_template(true)` for proper monochrome menu-bar rendering. Generate icon programmatically with tiny-skia → `tray_icon::Icon::from_rgba(data, w, h)` (no asset bundling).

Windows notes (Phase 4, recorded for completeness): activation policy N/A; tray-only = hide from taskbar; DPI handled via winit Physical/Logical; softbuffer Win32 supports no-copy present, same 0x00RRGGBB format.

---

## 7. Channel Implementation Choice (D-04, "Claude's discretion")

**Recommendation: `crossbeam-channel` 0.5.16.**

Rationale:
- global-hotkey, tray-icon, and muda already use `crossbeam_channel` internally (verified in source) → adding it as a direct dep costs nothing extra in build time or binary size; consistent idioms.
- MPMC (many publishers, many consumers) + `select!` for multiplexing — the EventBus, and later a "drain all buses in one place" pattern, benefit.
- `std::sync::mpsc` is single-consumer (each receiver can only have one thread) and has no `select!`; fine for trivial 1:1 but a poor fit for broadcast/publish and for multiplexing hotkey+tray+bus.
- `tokio` is a full async runtime — no async I/O exists in mybox-core; adding it is pure overhead and complicates winit's sync event loop. Explicitly avoid. `async-std`/`smol` similarly unnecessary.
- `flume` is a viable lightweight alternative (MPMC) but adds no benefit over crossbeam here and isn't already in the tree.

---

## 8. Validation Architecture (Nyquist) — how to test each component

Guiding split: **pure logic → cheap unit tests (CI-safe); winit/display/OS-permission → integration tests gated behind `#[ignore]`** (or a `gui` cargo feature) so `cargo nextest` stays green in headless CI while a human/macOS run exercises the real stack.

| Component | Unit tests (headless, CI-safe) | Integration tests (display/permission; #[ignore]) | Mapping |
|-----------|--------------------------------|---------------------------------------------------|---------|
| Module trait + AppBuilder | Fake module: assert registration, `init` called once with a real `ModuleContext`, unique-id conflict error, `default_config()` merged into ConfigCenter. | — | FRMW-01 |
| EventBus / EventFilter | `emit` → matching handler fires (ordering), wildcard `"capture:*"` and `"*:*"`, non-matching filters skipped, `Framework` vs `Module(JSON)` payload round-trip, `emit` is non-blocking (send into full buffer doesn't block under `unbounded`), SubscriptionId tracking. | Cross-thread: emit from a spawned thread; worker-thread dispatch + `ui()` forwarding lands on main thread. | FRMW-02, FRMW-05 |
| WindowManager | ID→state map logic (create/destroy/lookup/next_id) in a mocked renderer; `WindowSpec` → winit `WindowAttributes` builder unit test. | Create each `WindowKind` on a real display; assert created, `RedrawRequested` → `present()` runs, `CloseRequested` → destroyed, batch_create returns N ids. | FRMW-03, D-07, D-09 |
| HotkeyManager | `"Cmd+Shift+S"`.parse::<HotKey>() (global-hotkey FromStr) round-trip via ConfigCenter::hotkey(); id→action map. | `GlobalHotKeyManager::register` on macOS (may need accessibility/perms) — `#[ignore]`; simulate trigger → assert `AppEvent::Hotkey` reaches the loop. | FRMW-04, D-11 |
| TrayManager | Menu assembly from `menu_items()` (pure) — items + separator + quit present, ids unique. | `TrayIconBuilder::build()` + `Icon::from_rgba` (display) `#[ignore]`; menu click → `MenuEvent`. | INFRA-02 |
| ConfigCenter | Temp-dir (or `dirs` override): first-run generates file with all module defaults; `get/set/save` round-trip; module namespace isolation (`[capture]` ≠ `[palette]`); malformed TOML → typed error, not panic; missing file → auto-create. | — | INFRA-01, INFRA-04, D-10/12/13 |
| Renderer | Pure pixel conversion function (premul RGBA → `0x00RRGGBB`); tiny-skia Pixmap draw ops (fill_rect/fill_path) produce expected pixels. | Full `RedrawRequested` → softbuffer present on real window `#[ignore]`. | D-01/02/03 |
| Errors/logging | `MyboxError` derives `thiserror`; app path wraps with `anyhow` context; key ops log (assert log lines via test logger). | — | INFRA-03 |

**Success-criteria → verification (from ROADMAP):**
1. Tray icon shows, no Dock → integration `#[ignore]` + manual checklist.
2. Hotkey triggers callback → integration + manual.
3. Multiple modules + event bus → unit tests (module A emits, module B receives).
4. Overlay + Panel windows → integration test creating both kinds.
5. Config created in user dir + read/write → ConfigCenter unit tests + manual path check.

**Tooling:** `cargo nextest` (dev tool per CLAUDE.md); `cargo test -- --ignored` for the display suite on a dev Mac. Keep module crates testable in isolation (they only depend on mybox-core).

---

## 9. Known Pitfalls from PITFALLS.md Affecting Phase 1

| Pitfall | Phase-1 impact | Mitigation (verified) |
|---------|----------------|-----------------------|
| **P1: winit 0.30 breaking changes** | Direct — the skeleton IS the winit loop. | Use `ApplicationHandler`; pin `winit = "0.30.13"` (also required by egui-winit 0.36.1); `WindowBuilder` → `Window::default_attributes().with_*` + `ActiveEventLoop::create_window`. All verified against source. |
| **P4: main-thread blocking** | Direct — event bus design. | Non-blocking `emit`; heavy work offloads to worker threads (modules spawn threads, emit completion events); loop only does UI + render (FRMW-05). See §2.2. |
| **P5: egui + tiny-skia conflict** | Resolved in Phase 1 by NOT using egui yet (pure tiny-skia). | Defer egui to Phase 3; D-01/D-02 mechanism (egui-tiny-skia) doesn't exist — flagged in §5. |
| **P7: macOS Activation Policy** | Direct (FRMW-06). | winit built-in `with_activation_policy(Accessory)` — no objc2 needed. §6. |
| Gotcha: hotkey/tray manager thread | Direct. | Create on main thread (macOS); forward via `EventLoopProxy` + `ControlFlow::Wait` (§4). |
| **NEW (this research): softbuffer no alpha on macOS** | Affects Overlay transparency. | Phase 1: opaque test content; Phase 2: alpha-capable overlay compositor. §0.5/§6.5. |
| Deferred to Phase 2 (NOT Phase 1): P2 Screen Recording perms, P3 multi-monitor overlay, P6 Windows DPI. | — | Recorded; no Phase 1 action. |

---

## 10. Critical Code Examples (consolidated)

### 10.1 Module trait + registration (FRMW-01)

```rust
// crates/mybox-core/src/module.rs
pub trait Module: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn name(&self) -> &str;
    fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()>;
    fn default_config(&self) -> toml::Table { toml::Table::new() }
    fn menu_items(&self) -> Vec<tray_icon::menu::MenuItem> { vec![] }
    fn shutdown(&self, _ctx: &ModuleContext) {}
}

// crates/modules/test/src/lib.rs
pub struct TestModule;
impl Module for TestModule {
    fn id(&self) -> &'static str { "test" }
    fn name(&self) -> &str { "测试模块" }
    fn default_config(&self) -> toml::Table {
        toml::Table::from_iter([("message".into(), toml::Value::String("hello from test".into()))])
    }
    fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()> {
        ctx.on(EventFilter::kind("core", "hotkey.triggered"), |e| {
            log::info!("test module got hotkey event: {e:?}");
        });
        Ok(())
    }
}

// crates/mybox-app/src/main.rs
fn main() -> anyhow::Result<()> {
    env_logger::init();
    App::builder().module(TestModule).build()?.run()
}
```

### 10.2 Event bus dispatch (FRMW-02, D-04/05/06)

```rust
// publish (non-blocking; any thread)
ctx.emit(Event { from: "capture", kind: "screenshot-taken", payload: EventPayload::Module(json!({"path": "/tmp/shot.png"})) })?;

// subscribe with wildcard filter
let sub = ctx.on(EventFilter::kind("capture", "*"), |e| { /* handles all capture events */ });

// framework event
ctx.emit(Event { from: "core", kind: "window-created", payload: EventPayload::Framework(FrameworkEvent::WindowCreated(id)) })?;
```

### 10.3 Window event routing (D-07)

```rust
// App::window_event — the routing hot path (§4). winit WindowId -> WindowState -> per-window callback.
let state = self.windows.get_mut_by_winit(id);       // HashMap<WindowId, WindowState> lookup by winit_id
if let Some(state) = state {
    if let Some(cb) = &state.spec.on_event { cb(&event); }
    match event {
        WindowEvent::RedrawRequested => state.renderer.present().ok(),
        WindowEvent::Resized(size)   => { state.renderer.resize(size.width, size.height); }
        WindowEvent::CloseRequested  => { let wid = state.id; self.windows.destroy(wid); }
        _ => {}
    }
}
```

### 10.4 Hotkey config → registration (FRMW-04, D-11)

```rust
// config.toml (first-run generated)
[hotkeys]
open_test_window = "Cmd+Shift+T"
exit = "Cmd+Shift+Q"

// HotkeyManager::init
for (action, hotkey) in self.config.hotkeys_section()? {          // strings
    let hk: global_hotkey::hotkey::HotKey = hotkey.parse()?;      // built-in FromStr
    self.manager.register(hk)?;                                   // macOS: main thread
    self.map.insert(hk.id(), action);
}
```

### 10.5 Renderer present (D-02/D-03)

```rust
// TinySkiaSoftbufferRenderer::present
let mut buffer = self.surface.buffer_mut()?;              // &mut [u32]
let (w, h) = (buffer.width().get() as usize, buffer.height().get() as usize);
let px = self.pixmap.data();                              // premultiplied RGBA bytes
for (i, px) in px.chunks_exact(4).take(w * h).enumerate() {
    let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
    let (r, g, b) = if a == 255 { (r, g, b) } else { /* un-premultiply: r*255/a ... */ };
    buffer[i] = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);   // softbuffer 0x00RRGGBB
}
buffer.present()?;
```

---

## 11. Open Decisions for the Planner (resolve before/while writing PLAN.md)

1. **egui in Phase 1?** Recommend: NO (pure tiny-skia Renderer). If the planner insists on egui visibility in Phase 1, the manual tessellation path (§5) must be scoped into a plan — significantly higher risk/time.
2. **Event bus dispatch location:** bus worker thread + `ui()` forwarding (recommended) vs main-thread drain in `about_to_wait`. Locks the `ModuleContext`/`UiThreadProxy` API shape.
3. **Hotkey dispatch granularity:** action-name keyed events (`hotkey.open_test_window`) vs raw `HotkeyTriggered(id)` + module-side map. Recommend action-name in the bus for decoupling.
4. **Tray icon source:** runtime-generated via tiny-skia (recommended, no assets) vs bundled PNG.
5. **Config section naming:** `[hotkeys]` top-level + `[module_id]` per module (recommended) — document exact first-run file layout in the plan so implementers match it.
6. **WindowId type:** u64 counter (recommended) vs uuid. uuid crate is in the stack but u64 is simpler for Phase 1; revisit if window ids leak cross-process.
7. **Overlay transparency in Phase 1:** accept opaque test content (recommended; softbuffer alpha limitation) vs build the alpha-capable overlay compositor now (Phase 2 scope creep).
8. **Winit `0.30.13` pinning:** lock it in `[workspace.dependencies]`; note egui-winit 0.36.1 forces `>=0.30.13, <0.31` once egui enters in Phase 3.

---

## 12. Sources

All API claims verified against downloaded crate sources (crates.io registry, `index.crates.io-1949cf8c6b5b557f`):
- winit 0.30.13: `src/application.rs` (ApplicationHandler), `src/event_loop.rs` (EventLoopBuilder/Proxy/ControlFlow/ActiveEventLoop), `src/window.rs` (WindowAttributes, WindowLevel), `src/platform/macos.rs` (ActivationPolicy, EventLoopBuilderExtMacOS, ActiveEventLoopExtMacOS), `src/event.rs`.
- egui 0.36.1 / egui-winit 0.36.1 / epaint 0.36.1: `src/context.rs` (tessellate, begin_pass), `src/data/output.rs` (FullOutput), egui-winit `src/lib.rs` (State, on_window_event→EventResponse, handle_platform_output), egui-winit Cargo.toml (winit = "0.30.13").
- tiny-skia 0.12.0: `src/pixmap.rs`, `src/painter.rs`.
- softbuffer 0.4.8: `src/lib.rs` (Context/Surface/Buffer, 0x00RRGGBB format), `src/backends/cg.rs` (CGImageAlphaInfo::NoneSkipFirst).
- global-hotkey 0.8.0: `src/lib.rs` (GlobalHotKeyEvent::receiver/set_event_handler, macOS main-thread note), `src/hotkey.rs` (HotKey FromStr/Display).
- tray-icon 0.24.2: `src/lib.rs` (TrayIconBuilder, event handler + EventLoopProxy docs example, `pub mod menu { pub use muda::*; }`), muda 0.19.3 (`src/menu.rs`, `src/items/normal.rs`, `src/items/predefined.rs`).
- Version resolution: `cargo metadata` on a throwaway manifest with the §1 matrix (all resolved; exit 0).

Project planning inputs (read): `01-CONTEXT.md` (decisions D-01..D-13), `REQUIREMENTS.md` (FRMW-01..06, INFRA-01..04), `ROADMAP.md` (Phase 1 plans 01-01..01-04 + success criteria), `STATE.md`, `PROJECT.md`, `research/STACK.md`, `research/ARCHITECTURE.md`, `research/PITFALLS.md`, `research/FEATURES.md`, `research/SUMMARY.md`.

---

*Phase: 1-框架核心*
*Research completed: 2026-08-11*
