//! mybox application entry point.
//!
//! Phase 1 walking skeleton: assemble the framework with the TestModule
//! (registered through `AppBuilder`, FRMW-01) and run the event loop. Startup
//! is main-thread throughout (macOS: hotkey manager + tray require it).

use mybox_core::App;

/// Writes every log line to all wrapped sinks (stderr + log file, D-12).
struct TeeWriter(Vec<Box<dyn std::io::Write + Send>>);

impl std::io::Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for sink in &mut self.0 {
            let _ = sink.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        for sink in &mut self.0 {
            let _ = sink.flush();
        }
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    // D-12: dual-sink logging — stderr + <config_dir>/logs/mybox.log, created
    // at startup so the builtin 「打开日志文件」 command always opens an
    // existing file.
    let log_dir = mybox_core::config_dir()?.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("mybox.log"))?;
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(TeeWriter(vec![
            Box::new(std::io::stderr()),
            Box::new(log_file),
        ]))))
        .init();

    let mut builder = App::builder();
    // module() returns Result — a duplicate module id bubbles up here (N1).
    builder.module(Box::new(mybox_test::TestModule))?;
    builder.module(Box::new(mybox_capture::CaptureModule::new()))?;
    builder.build()?.run()
}
