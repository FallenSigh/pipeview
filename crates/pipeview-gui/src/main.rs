#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod app_state;
mod buffers;
mod logging;
mod panels;
mod perf;
mod shortcuts;
mod ui_fonts;

use pipeview_client::SessionManager;

/// On Linux, when launched from a file manager, a terminal emulator may be spawned
/// to run this binary. We re-spawn ourselves detached so the terminal can close
/// while the GUI window persists.
#[cfg(target_os = "linux")]
fn ensure_detached() {
    use std::os::unix::process::CommandExt;
    use std::process;

    if std::env::var("PIPEVIEW_DETACHED").is_ok() {
        return;
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };

    let args: Vec<String> = std::env::args().skip(1).collect();

    let _ = process::Command::new(&exe)
        .args(&args)
        .env("PIPEVIEW_DETACHED", "1")
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .process_group(0)
        .spawn();

    process::exit(0);
}

fn main() {
    #[cfg(target_os = "linux")]
    ensure_detached();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    let mgr = SessionManager::new();
    let rx = mgr.subscribe();

    let _ = eframe::run_native(
        "pipeview",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(app::XserialApp::new(mgr, rx, cc.egui_ctx.clone())))),
    );
}
