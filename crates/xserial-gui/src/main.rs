mod app;
mod panels;

use xserial_client::SessionManager;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    let mgr = SessionManager::new();
    let rx = mgr.subscribe();

    let _ = eframe::run_native(
        "xserial",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(app::XserialApp::new(mgr, rx)))),
    );
}
