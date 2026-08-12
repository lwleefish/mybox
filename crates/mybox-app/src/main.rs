//! mybox application entry point.
//!
//! Phase 1 skeleton: initializes the logger and returns. The real bootstrap
//! (`App::builder().module(...).build()?.run()`) lands in plan 01-04 once the
//! event-loop integration exists.

fn main() -> anyhow::Result<()> {
    env_logger::init();
    Ok(())
}
