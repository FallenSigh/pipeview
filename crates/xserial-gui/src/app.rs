use crate::panels::{config, sidebar};
use tracing::debug;
use xserial_client::SessionManager;
use xserial_client::session::SessionEvent;

#[derive(Clone)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    Error(String),
}

pub struct SessionTab {
    pub id: u64,
    pub status: ConnectionStatus,
}

pub struct XserialApp {
    manager: SessionManager,
    tabs: Vec<SessionTab>,
    active: usize,
    config_open: bool,
    config_form: config::ConfigForm,
    event_rx: Option<tokio::sync::broadcast::Receiver<SessionEvent>>,
    pending: Vec<SessionEvent>,
}

impl XserialApp {
    pub fn new(m: SessionManager, rx: tokio::sync::broadcast::Receiver<SessionEvent>) -> Self {
        Self {
            manager: m,
            tabs: vec![],
            active: 0,
            config_open: false,
            config_form: config::ConfigForm::default(),
            event_rx: Some(rx),
            pending: vec![],
        }
    }
}

impl eframe::App for XserialApp {
    fn update(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 拉取 SessionManager 事件
        if let Some(ref mut rx) = self.event_rx {
            while let Ok(e) = rx.try_recv() {
                debug!(event = ?e, "GUI received event");
                self.pending.push(e);
            }
        }
        for e in self.pending.drain(..) {
            match e {
                SessionEvent::Connected(id) => {
                    debug!(session_id = id, "GUI: Connected");
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                        t.status = ConnectionStatus::Connected;
                    }
                }
                SessionEvent::Disconnected(id) | SessionEvent::Closed(id) => {
                    debug!(session_id = id, "GUI: Disconnected/Closed");
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                        t.status = ConnectionStatus::Disconnected;
                    }
                }
                SessionEvent::Error(id, msg) => {
                    debug!(session_id = id, error = %msg, "GUI: Error");
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                        t.status = ConnectionStatus::Error(msg);
                    }
                }
                SessionEvent::Data(id, entry) => {
                    debug!(session_id = id, pipeline = %entry.pipeline_name, "GUI: Data");
                }
            }
        }

        // Config 弹窗
        if self.config_open {
            egui::Window::new("Session Config").show(ui.ctx(), |ui| {
                if let Some(cfg) = config::render(ui, &mut self.config_form) {
                    let h = self.manager.create(cfg);
                    self.tabs.push(SessionTab {
                        id: h.id,
                        status: ConnectionStatus::Connecting,
                    });
                    self.active = self.tabs.len() - 1;
                    self.config_open = false;
                }
            });
        }

        // Sidebar
        let mut on_new = false;
        let mut on_delete: Option<usize> = None;
        egui::Panel::left("sidebar")
            .resizable(false)
            .show_inside(ui, |ui| {
                let ss: Vec<(u64, ConnectionStatus)> =
                    self.tabs.iter().map(|t| (t.id, t.status.clone())).collect();
                sidebar::render(ui, &ss, &mut self.active, &mut on_new, &mut on_delete);
            });
        if on_new {
            self.config_open = true;
        }
        if let Some(i) = on_delete {
            if let Some(tab) = self.tabs.get(i) {
                self.manager.remove(tab.id);
            }
            self.tabs.remove(i);
            if self.active >= self.tabs.len() && !self.tabs.is_empty() {
                self.active = self.tabs.len() - 1;
            }
        }

        // Central
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.tabs.is_empty() {
                ui.heading("No sessions.");
                return;
            }
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len() - 1;
            }
            let tab = &self.tabs[self.active];
            let txt = match &tab.status {
                ConnectionStatus::Connected => "🟢 Connected",
                ConnectionStatus::Disconnected => "⚫ Disconnected",
                ConnectionStatus::Connecting => "🟡 Connecting...",
                ConnectionStatus::Error(e) => &format!("🔴 {}", e),
            };
            ui.heading(format!("Session {} — {}", tab.id, txt));
        });
    }
}
