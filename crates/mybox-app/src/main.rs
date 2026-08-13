//! mybox application entry point.
//!
//! Phase 1 walking skeleton: assemble the framework with the TestModule
//! (registered through `AppBuilder`, FRMW-01) and run the event loop. Startup
//! is main-thread throughout (macOS: hotkey manager + tray require it).

use mybox_core::App;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut builder = App::builder();
    // module() returns Result — a duplicate module id bubbles up here (N1).
    builder.module(Box::new(mybox_test::TestModule))?;
    builder.module(Box::new(mybox_capture::CaptureModule::new()))?;
    builder.build()?.run()
}
