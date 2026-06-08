use std::sync::mpsc;

use crate::buffers::{HexBuffer, TextBuffer};
use crate::panels::{config, console, hex_view, sidebar};
use egui::{Color32, Layout, Panel, Pos2, Rect, TextEdit, UiBuilder};
use xserial_client::SessionManager;
use xserial_client::config::SessionConfig;
use xserial_client::session::SessionEvent;
use xserial_core::protocol::DecodedData;

#[derive(Clone)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    Error(String),
}

impl ConnectionStatus {
    fn badge(&self) -> (&'static str, &'static str) {
        match self {
            Self::Connected => ("[connected]", "Connected"),
            Self::Disconnected => ("[disconnected]", "Disconnected"),
            Self::Connecting => ("[connecting]", "Connecting"),
            Self::Error(_) => ("[error]", "Error"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    Text,
    Hex,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SendMode {
    Text,
    Hex,
}

#[derive(Clone, Copy)]
pub struct DisplayOptions {
    pub show_timestamp: bool,
    pub show_direction: bool,
    pub show_pipeline: bool,
}

pub struct SessionTab {
    pub id: u64,
    pub session_config: SessionConfig,
    pub status: ConnectionStatus,
    pub console: TextBuffer,
    pub hex: HexBuffer,
    pub view: View,
    pub auto_reconnect: bool,
    pub send_input: String,
    pub send_mode: SendMode,
    pub append_newline: bool,
    pub send_status: Option<String>,
}

pub struct XserialApp {
    manager: SessionManager,
    tabs: Vec<SessionTab>,
    active: usize,
    display: DisplayOptions,
    config_open: bool,
    config_target: Option<u64>,
    config_form: config::ConfigForm,
    event_rx: mpsc::Receiver<SessionEvent>,
    pending: Vec<SessionEvent>,
}

impl XserialApp {
    pub fn new(
        manager: SessionManager,
        mut rx: tokio::sync::broadcast::Receiver<SessionEvent>,
        ctx: egui::Context,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if event_tx.send(event).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Self {
            manager,
            tabs: Vec::new(),
            active: 0,
            display: DisplayOptions {
                show_timestamp: true,
                show_direction: true,
                show_pipeline: true,
            },
            config_open: false,
            config_target: None,
            config_form: config::ConfigForm::default(),
            event_rx,
            pending: Vec::new(),
        }
    }

    fn open_create_config(&mut self) {
        self.config_target = None;
        self.config_form = config::ConfigForm::default();
        self.config_open = true;
    }

    fn open_edit_config(&mut self, id: u64) {
        if let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) {
            self.config_target = Some(id);
            self.config_form = config::ConfigForm::from_session_config(&tab.session_config);
            self.config_open = true;
        }
    }

    fn apply_session_config(&mut self, id: u64, session_config: SessionConfig) {
        let handle = self.manager.get(id);
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            let history_limit = session_config.history_limit;
            tab.session_config = session_config.clone();
            tab.auto_reconnect = session_config.auto_reconnect;
            tab.console = TextBuffer::new(history_limit);
            tab.hex = HexBuffer::new(history_limit);
            tab.status = ConnectionStatus::Connecting;
            tab.send_status = Some(String::from("Session reconfigured"));
        }

        if let Some(handle) = handle {
            tokio::spawn(async move {
                let _ = handle.reconfigure(session_config).await;
            });
        } else if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.status = ConnectionStatus::Error(String::from("session not found"));
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.pending.push(event);
        }

        for event in self.pending.drain(..) {
            match event {
                SessionEvent::Connected(id) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                        tab.status = ConnectionStatus::Connected;
                    }
                }
                SessionEvent::Disconnected(id) | SessionEvent::Closed(id) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                        tab.status = ConnectionStatus::Disconnected;
                    }
                }
                SessionEvent::Error(id, message) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                        tab.status = ConnectionStatus::Error(message);
                    }
                }
                SessionEvent::Data(id, entry) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                        match &entry.data {
                            DecodedData::Text(_) => tab.console.push(&entry),
                            DecodedData::Hex(_) => tab.hex.push(&entry),
                            DecodedData::Binary(_) | DecodedData::Plot(_) => {}
                        }
                    }
                }
            }
        }
    }

    fn render_config_window(&mut self, ctx: &egui::Context) {
        if !self.config_open {
            return;
        }

        let mut open = self.config_open;
        let title = if self.config_target.is_some() {
            "Edit Session Config"
        } else {
            "Session Config"
        };
        let submit_label = if self.config_target.is_some() {
            "Save Session Config"
        } else {
            "Create Session"
        };
        let mut submitted = None;
        egui::Window::new(title).open(&mut open).show(ctx, |ui| {
            submitted = config::render(ui, &mut self.config_form, submit_label);
        });

        if let Some(session_config) = submitted {
            if let Some(id) = self.config_target {
                self.apply_session_config(id, session_config);
            } else {
                let history_limit = session_config.history_limit;
                let auto_reconnect = session_config.auto_reconnect;
                let handle = self.manager.create(session_config.clone());
                self.tabs.push(SessionTab {
                    id: handle.id(),
                    session_config,
                    status: ConnectionStatus::Connecting,
                    console: TextBuffer::new(history_limit),
                    hex: HexBuffer::new(history_limit),
                    view: View::Text,
                    auto_reconnect,
                    send_input: String::new(),
                    send_mode: SendMode::Text,
                    append_newline: true,
                    send_status: None,
                });
                self.active = self.tabs.len() - 1;
            }
            open = false;
        }
        if !open {
            self.config_target = None;
        }
        self.config_open = open;
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let mut on_new = false;
        let mut on_delete = None;

        Panel::left("sidebar")
            .resizable(false)
            .show_inside(ui, |ui| {
                let sessions: Vec<_> = self
                    .tabs
                    .iter()
                    .map(|tab| (tab.id, tab.status.clone()))
                    .collect();
                sidebar::render(ui, &sessions, &mut self.active, &mut on_new, &mut on_delete);
            });

        if on_new {
            self.open_create_config();
        }

        if let Some(index) = on_delete {
            if let Some(tab) = self.tabs.get(index) {
                self.manager.remove(tab.id);
            }
            self.tabs.remove(index);
            if self.active >= self.tabs.len() && !self.tabs.is_empty() {
                self.active = self.tabs.len() - 1;
            }
        }
    }

    fn render_main_panel(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.tabs.is_empty() {
                ui.heading("No sessions.");
                return;
            }

            if self.active >= self.tabs.len() {
                self.active = self.tabs.len() - 1;
            }

            let manager = self.manager.clone();
            let display = &mut self.display;
            let mut edit_session = None;

            {
                let tab = &mut self.tabs[self.active];
                let (badge, status_text) = tab.status.badge();

                ui.horizontal(|ui| {
                    ui.heading(format!("Session {}", tab.id));
                    ui.label(badge);
                    ui.separator();
                    ui.label(status_text);
                    ui.separator();
                    if ui
                        .selectable_label(tab.view == View::Text, "Text")
                        .clicked()
                    {
                        tab.view = View::Text;
                    }
                    if ui.selectable_label(tab.view == View::Hex, "Hex").clicked() {
                        tab.view = View::Hex;
                    }
                });

                if render_session_controls(ui, &manager, tab) {
                    edit_session = Some(tab.id);
                }

                ui.horizontal_wrapped(|ui| {
                    ui.label("Show:");
                    ui.checkbox(&mut display.show_timestamp, "Timestamp");
                    ui.checkbox(&mut display.show_direction, "Direction");
                    ui.checkbox(&mut display.show_pipeline, "Pipe");
                });

                if let ConnectionStatus::Error(message) = &tab.status {
                    ui.label(egui::RichText::new(message).color(Color32::RED));
                }

                let full = ui.available_rect_before_wrap();
                let send_height = 240.0;
                let separator = 8.0;
                let receive_height = (full.height() - send_height - separator).max(0.0);
                let receive_rect = Rect::from_min_max(
                    full.min,
                    Pos2::new(full.max.x, full.min.y + receive_height),
                );
                let send_rect =
                    Rect::from_min_max(Pos2::new(full.min.x, full.max.y - send_height), full.max);

                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(receive_rect)
                        .layout(Layout::top_down(egui::Align::Min).with_cross_justify(true)),
                    |ui| match tab.view {
                        View::Text => console::render(ui, &tab.console, *display),
                        View::Hex => hex_view::render(ui, &tab.hex, *display),
                    },
                );

                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(send_rect)
                        .layout(Layout::top_down(egui::Align::Min).with_cross_justify(true)),
                    |ui| render_send_panel(ui, &manager, tab),
                );
            }

            if let Some(id) = edit_session {
                self.open_edit_config(id);
            }
        });
    }
}

impl eframe::App for XserialApp {
    fn update(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.render_config_window(ui.ctx());
        self.render_sidebar(ui);
        self.render_main_panel(ui);
    }
}

fn render_session_controls(
    ui: &mut egui::Ui,
    manager: &SessionManager,
    tab: &mut SessionTab,
) -> bool {
    let mut configure_clicked = false;
    ui.horizontal_wrapped(|ui| {
        if ui.button("Connect").clicked() {
            if let Some(handle) = manager.get(tab.id) {
                tab.status = ConnectionStatus::Connecting;
                tokio::spawn(async move {
                    let _ = handle.connect().await;
                });
            } else {
                tab.status = ConnectionStatus::Error(String::from("session not found"));
            }
        }

        if ui.button("Disconnect").clicked() {
            if let Some(handle) = manager.get(tab.id) {
                tab.status = ConnectionStatus::Disconnected;
                tokio::spawn(async move {
                    let _ = handle.disconnect().await;
                });
            } else {
                tab.status = ConnectionStatus::Error(String::from("session not found"));
            }
        }

        if ui.button("Reconnect").clicked() {
            if let Some(handle) = manager.get(tab.id) {
                tab.status = ConnectionStatus::Connecting;
                tokio::spawn(async move {
                    let _ = handle.reconnect().await;
                });
            } else {
                tab.status = ConnectionStatus::Error(String::from("session not found"));
            }
        }

        if ui.button("Configure").clicked() {
            configure_clicked = true;
        }

        let response = ui.checkbox(&mut tab.auto_reconnect, "Auto reconnect");
        if response.changed() {
            if let Some(handle) = manager.get(tab.id) {
                let enabled = tab.auto_reconnect;
                tokio::spawn(async move {
                    let _ = handle.set_auto_reconnect(enabled).await;
                });
            } else {
                tab.status = ConnectionStatus::Error(String::from("session not found"));
            }
        }
    });
    configure_clicked
}

fn render_send_panel(ui: &mut egui::Ui, manager: &SessionManager, tab: &mut SessionTab) {
    ui.set_width(ui.available_width());
    ui.heading("Send");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut tab.send_mode, SendMode::Text, "Text");
        ui.selectable_value(&mut tab.send_mode, SendMode::Hex, "Hex");
        if tab.send_mode == SendMode::Text {
            ui.checkbox(&mut tab.append_newline, "Append newline");
        }
    });
    ui.add_space(6.0);

    let hint = match tab.send_mode {
        SendMode::Text => "Enter text to send",
        SendMode::Hex => "Enter hex bytes, e.g. 48 65 6C 6C 6F",
    };
    let response = ui.add(
        TextEdit::multiline(&mut tab.send_input)
            .desired_rows(6)
            .desired_width(f32::INFINITY)
            .hint_text(hint),
    );

    let wants_submit = response.has_focus()
        && ui.input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.command_only());

    let mut send_clicked = false;
    ui.horizontal(|ui| {
        send_clicked = ui.button("Send").clicked();
        if let Some(status) = &tab.send_status {
            ui.label(
                egui::RichText::new(status).color(if status.starts_with("Send failed") {
                    Color32::RED
                } else {
                    Color32::GRAY
                }),
            );
        }
    });

    if !(send_clicked || wants_submit) {
        return;
    }

    match build_payload(tab) {
        Ok(Some(payload)) => {
            if let Some(handle) = manager.get(tab.id) {
                match tab.send_mode {
                    SendMode::Text => {
                        let text = if tab.append_newline {
                            tab.send_input.trim_end_matches('\n').to_string()
                        } else {
                            tab.send_input.clone()
                        };
                        if !text.is_empty() {
                            tab.console.push_outbound(text);
                        }
                    }
                    SendMode::Hex => {
                        let hex = tab
                            .send_input
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        if !hex.is_empty() {
                            tab.hex.push_outbound(hex);
                        }
                    }
                }
                tokio::spawn(async move {
                    let _ = handle.send(payload).await;
                });
                tab.send_input.clear();
                tab.send_status = Some(String::from("Sent"));
            } else {
                tab.send_status = Some(String::from("Send failed: session not found"));
            }
        }
        Ok(None) => {
            tab.send_status = Some(String::from("Nothing to send"));
        }
        Err(message) => {
            tab.send_status = Some(format!("Send failed: {message}"));
        }
    }
}

fn build_payload(tab: &SessionTab) -> Result<Option<Vec<u8>>, String> {
    let trimmed = tab.send_input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    match tab.send_mode {
        SendMode::Text => {
            let mut text = tab.send_input.clone();
            if tab.append_newline && !text.ends_with('\n') {
                text.push('\n');
            }
            Ok(Some(text.into_bytes()))
        }
        SendMode::Hex => {
            let compact: String = trimmed
                .chars()
                .filter(|ch| !ch.is_ascii_whitespace())
                .collect();
            hex::decode(compact)
                .map(Some)
                .map_err(|err| format!("invalid hex input ({err})"))
        }
    }
}
