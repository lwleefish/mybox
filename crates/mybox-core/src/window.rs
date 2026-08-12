//! Window types (FRMW-03).
//!
//! Data models for the three window kinds the framework supports. The
//! `WindowSpec` → winit `WindowAttributes` builder and the `WindowManager`
//! implementation land in plan 01-02.

use crate::renderer::Renderer;

/// Three window kinds the framework supports (FRMW-03).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowKind {
    /// Fullscreen transparent overlay (screenshot selection, Phase 2).
    Overlay,
    /// Independent always-on-top floating window (pin-type, Phase 2).
    Floating,
    /// Regular panel window (command palette, Phase 3).
    Panel,
}

/// Abstract window creation spec (D-07 centralized + ID dispatch).
///
/// All fields are `pub` so modules in separate crates can construct specs with
/// struct-literal syntax (e.g. `WindowSpec { kind, title, inner_size, .. }`).
pub struct WindowSpec {
    pub kind: WindowKind,
    pub title: String,
    pub transparent: bool,
    pub always_on_top: bool,
    pub decorations: bool,
    pub visible: bool,
    /// Physical pixels.
    pub inner_size: Option<(u32, u32)>,
    /// Physical pixels.
    pub position: Option<(i32, i32)>,
    /// Per-window event callback (D-07 routing target).
    pub on_event: Option<Box<dyn Fn(&winit::event::WindowEvent) + Send + Sync>>,
}

impl Default for WindowSpec {
    fn default() -> Self {
        Self {
            kind: WindowKind::Panel,
            title: "mybox".to_string(),
            transparent: false,
            always_on_top: false,
            decorations: true,
            visible: true,
            inner_size: None,
            position: None,
            on_event: None,
        }
    }
}

/// Framework window id — a u64 incrementing counter (RESEARCH §11 #6).
pub type WindowId = u64;

/// Runtime state for a live window. Fields are declared here; construction is
/// completed in plan 01-02 (needs a real winit window + renderer).
#[allow(dead_code)]
pub struct WindowState {
    id: WindowId,
    kind: WindowKind,
    winit_id: winit::window::WindowId,
    window: winit::window::Window,
    renderer: Box<dyn Renderer>,
    spec: WindowSpec,
}

/// Main-thread-bound handle to the window manager. Modules enqueue window
/// creation requests through this; the winit loop executes them on the main
/// thread (RESEARCH §2.3). Implementation lands in plan 01-02.
#[derive(Default)]
pub struct WindowManagerHandle;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_kind_has_three_distinct_variants() {
        let kinds = [WindowKind::Overlay, WindowKind::Floating, WindowKind::Panel];
        assert_eq!(kinds.len(), 3);
        assert_ne!(WindowKind::Overlay, WindowKind::Floating);
        assert_ne!(WindowKind::Floating, WindowKind::Panel);
        assert_ne!(WindowKind::Overlay, WindowKind::Panel);
    }

    #[test]
    fn window_spec_default_is_panel_decorated_visible() {
        let spec = WindowSpec::default();
        assert_eq!(spec.kind, WindowKind::Panel);
        assert_eq!(spec.title, "mybox");
        assert!(spec.decorations);
        assert!(spec.visible);
        assert!(!spec.transparent);
        assert!(!spec.always_on_top);
        assert!(spec.inner_size.is_none());
        assert!(spec.position.is_none());
        assert!(spec.on_event.is_none());
    }

    #[test]
    fn window_spec_fields_are_public_for_struct_literal() {
        // Exercises the public-field contract: modules in separate crates build
        // WindowSpec with `..Default::default()` (mybox-test does this in 01-04).
        let spec = WindowSpec {
            kind: WindowKind::Overlay,
            title: "overlay".to_string(),
            inner_size: Some((1920, 1080)),
            ..Default::default()
        };
        assert_eq!(spec.kind, WindowKind::Overlay);
        assert_eq!(spec.inner_size, Some((1920, 1080)));
        assert!(spec.decorations); // inherited from Default
    }

    #[test]
    fn window_id_is_u64() {
        let id: WindowId = 42u64;
        assert_eq!(id, 42);
    }
}
