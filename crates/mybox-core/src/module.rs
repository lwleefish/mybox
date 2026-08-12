//! Module extension contract (FRMW-01) and the compile-time registry.
//!
//! Every feature module implements [`Module`] and is registered through the
//! registry (via AppBuilder in plan 01-04). Modules see the framework only
//! through [`ModuleContext`] (FRMW-02: no dependencies on core internals).

use crate::context::ModuleContext;
use crate::error::{MyboxError, Result};

/// Extension point for feature modules. Compiled into the binary at build time;
/// no dynamic loading in the first milestone.
pub trait Module: Send + Sync + 'static {
    /// Unique module id — used as the event `from` namespace and the config
    /// section name.
    fn id(&self) -> &'static str;

    /// Human-readable module name.
    fn name(&self) -> &str;

    /// Called once at startup after config is loaded. Register event handlers
    /// and do any one-time setup here.
    fn init(&self, ctx: &ModuleContext) -> anyhow::Result<()>;

    /// Default config section merged into config.toml on first run (D-13).
    fn default_config(&self) -> toml::Table {
        toml::Table::new()
    }

    /// Tray context-menu items contributed to the shared tray menu (INFRA-02).
    fn menu_items(&self) -> Vec<tray_icon::menu::MenuItem> {
        vec![]
    }

    /// Optional ordered cleanup on exit.
    fn shutdown(&self, _ctx: &ModuleContext) {}
}

/// Compile-time registry of registered modules. Rejects duplicate ids.
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { modules: Vec::new() }
    }

    /// Register a module. Returns `Err(MyboxError::Module)` if a module with the
    /// same `id()` is already registered (FRMW-01: unique module ids enforced).
    pub fn register(&mut self, module: Box<dyn Module>) -> Result<()> {
        let id = module.id();
        if self.modules.iter().any(|m| m.id() == id) {
            return Err(MyboxError::Module(format!("duplicate module id '{id}'")));
        }
        self.modules.push(module);
        Ok(())
    }

    /// Iterate over all registered modules.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Module> {
        self.modules.iter().map(|m| m.as_ref())
    }

    /// Number of registered modules.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Look up a module by id.
    pub fn get_by_id(&self, id: &str) -> Option<&dyn Module> {
        self.modules.iter().find(|m| m.id() == id).map(|m| m.as_ref())
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeModule {
        id: &'static str,
        name: &'static str,
    }

    impl FakeModule {
        fn new(id: &'static str, name: &'static str) -> Self {
            Self { id, name }
        }
    }

    impl Module for FakeModule {
        fn id(&self) -> &'static str {
            self.id
        }
        fn name(&self) -> &str {
            self.name
        }
        fn init(&self, _ctx: &ModuleContext) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn registers_two_modules_successfully() {
        let mut reg = ModuleRegistry::new();
        reg.register(Box::new(FakeModule::new("capture", "截图")))
            .expect("first register ok");
        reg.register(Box::new(FakeModule::new("palette", "命令面板")))
            .expect("second register ok");
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.iter().count(), 2);
    }

    #[test]
    fn duplicate_id_is_rejected_with_id_in_message() {
        let mut reg = ModuleRegistry::new();
        reg.register(Box::new(FakeModule::new("capture", "截图")))
            .expect("first register ok");
        let err = reg.register(Box::new(FakeModule::new("capture", "另一个截图")));
        let err = err.expect_err("duplicate id must fail");
        assert!(matches!(err, MyboxError::Module(_)));
        assert!(err.to_string().contains("capture"));
        // Registry unchanged.
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn get_by_id_returns_correct_module() {
        let mut reg = ModuleRegistry::new();
        reg.register(Box::new(FakeModule::new("capture", "截图")));
        reg.register(Box::new(FakeModule::new("palette", "命令面板")));
        let m = reg.get_by_id("palette").expect("palette registered");
        assert_eq!(m.id(), "palette");
        assert_eq!(m.name(), "命令面板");
        assert!(reg.get_by_id("missing").is_none());
    }
}
