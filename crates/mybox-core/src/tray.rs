//! System tray + context menu (INFRA-02).

use tray_icon::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};

use crate::error::{MyboxError, Result};

/// Compute the ordered tray menu content (module items → separator → quit) as
/// concrete [`MenuItemKind`]s.
///
/// Pure and headless-testable: on macOS a native [`Menu`] can only be created
/// on the main thread (muda constraint), so `build_menu` (which owns that
/// [`Menu`]) is not unit-testable off the main thread. This function is the
/// single source of truth for what `build_menu` appends, and the unit tests
/// verify its exact content and order.
fn assemble_menu_items(module_items: Vec<MenuItem>) -> Vec<MenuItemKind> {
    let mut items: Vec<MenuItemKind> = module_items
        .into_iter()
        .map(MenuItemKind::MenuItem)
        .collect();
    items.push(MenuItemKind::Predefined(PredefinedMenuItem::separator()));
    items.push(MenuItemKind::Predefined(PredefinedMenuItem::quit(Some("退出"))));
    items
}

/// Assemble the shared tray context menu from module items (INFRA-02).
///
/// Order: every module menu item (ids preserved so a `MenuEvent` can round-trip
/// the original id) → a separator → the built-in quit item (`退出`). Module
/// menu item ids must be unique across modules (documented contract, T-1-08).
///
/// macOS: must be called on the main thread (muda `Menu` construction
/// requirement); the 01-04 App builds the tray in its main-thread startup.
/// The `append` calls cannot fail for our item kinds on the supported
/// platforms, so a failure indicates an invariant break (T-1-08) and panicking
/// is appropriate.
pub fn build_menu(module_items: Vec<MenuItem>) -> Menu {
    let menu = Menu::new();
    for kind in assemble_menu_items(module_items) {
        match kind {
            MenuItemKind::MenuItem(item) => menu.append(&item).expect("append module item"),
            MenuItemKind::Predefined(item) => {
                menu.append(&item).expect("append predefined item")
            }
            _ => unreachable!("only MenuItem/Predefined are assembled into the tray menu"),
        }
    }
    menu
}

/// Render a monochrome tray icon into RGBA bytes via tiny-skia (headless-testable
/// pure core, RESEARCH §2.5 / §11 #4: no bundled PNG asset).
///
/// A filled circle on a transparent background gives macOS enough silhouette
/// for a monochrome template image (the alpha channel defines the shape).
pub fn generate_icon_rgba(size: u32) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("pixmap allocation");
    pixmap.fill(tiny_skia::Color::TRANSPARENT);

    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    let mut path_builder = tiny_skia::PathBuilder::new();
    path_builder.push_circle(
        size as f32 / 2.0,
        size as f32 / 2.0,
        size as f32 * 0.32,
    );
    let path = path_builder.finish().expect("valid circle path");
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );

    pixmap.data().to_vec()
}

/// Wrap the runtime-rendered bytes into a `tray_icon::Icon` (no asset file).
pub fn generate_icon(size: u32) -> tray_icon::Icon {
    let data = generate_icon_rgba(size);
    tray_icon::Icon::from_rgba(data, size, size).expect("valid icon dimensions")
}

/// Owns the tray icon and the shared context menu.
#[derive(Default)]
pub struct TrayManager {
    _tray: Option<tray_icon::TrayIcon>,
    menu: Menu,
}

impl TrayManager {
    /// Build the tray icon with the shared menu assembled from module items.
    ///
    /// Requires a live desktop session — this is 01-04 integration/manual
    /// verification, not exercised in headless unit tests. `size` is the icon
    /// side length in pixels (e.g. 32).
    pub fn build(&mut self, module_items: Vec<MenuItem>, size: u32) -> Result<()> {
        let menu = build_menu(module_items);
        let icon = generate_icon(size);
        let tray = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_icon(icon)
            .with_icon_as_template(true) // macOS monochrome template (RESEARCH §2.5)
            .build()
            .map_err(|e| MyboxError::Tray(e.to_string()))?;
        self._tray = Some(tray);
        self.menu = menu;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tray_icon::menu::{MenuItem, MenuItemKind};

    #[test]
    fn menu_assembly_orders_module_items_then_separator_then_quit() {
        let start = MenuItem::with_id("capture.start", "开始截图", true, None);
        let pin = MenuItem::with_id("capture.pin", "Pin", true, None);
        // build_menu appends exactly these items; assemble_menu_items is the
        // headless-testable projection of that assembly (muda Menu is
        // main-thread-only on macOS).
        let items = assemble_menu_items(vec![start, pin]);
        assert_eq!(items.len(), 4, "2 module items + separator + quit");

        // Module items first, ids preserved so MenuEvent can round-trip them.
        assert!(
            matches!(&items[0], MenuItemKind::MenuItem(_)),
            "items[0] should be a module MenuItem"
        );
        assert_eq!(items[0].id(), "capture.start");
        assert!(
            matches!(&items[1], MenuItemKind::MenuItem(_)),
            "items[1] should be a module MenuItem"
        );
        assert_eq!(items[1].id(), "capture.pin");

        // Then the separator (no text), then the quit item (退出).
        match &items[2] {
            MenuItemKind::Predefined(p) => assert!(p.text().is_empty(), "separator has no text"),
            _ => panic!("items[2] should be the separator"),
        }
        match &items[3] {
            MenuItemKind::Predefined(p) => assert_eq!(p.text(), "退出"),
            _ => panic!("items[3] should be the quit item"),
        }
    }

    #[test]
    fn menu_assembly_handles_no_module_items() {
        let items = assemble_menu_items(vec![]);
        assert_eq!(items.len(), 2, "separator + quit only");
        assert!(matches!(&items[0], MenuItemKind::Predefined(_)));
        assert!(matches!(&items[1], MenuItemKind::Predefined(_)));
    }

    #[test]
    fn generate_icon_rgba_has_correct_size_and_opaque_pixels() {
        let size = 32u32;
        let data = generate_icon_rgba(size);
        assert_eq!(data.len(), (size * size * 4) as usize);
        let has_opaque_pixel = data.chunks_exact(4).any(|px| px[3] > 0);
        assert!(
            has_opaque_pixel,
            "icon must have at least one pixel with alpha > 0"
        );
    }

    #[test]
    fn generate_icon_wraps_into_tray_icon() {
        // Icon::from_rgba validates width*height*4 == data len; a success here
        // proves the rendered bytes form a well-formed tray Icon.
        let _icon = generate_icon(32);
    }
}
