use crate::app_state::{self, PersistedGuiState};
use std::sync::mpsc;
use std::time::Duration;

use crate::buffers::{HexBuffer, PlotBuffer, TextBuffer};
use crate::panels::{config, console, hex_view, plot_view, sidebar};
use crate::ui_fonts::{self, FontCandidate, FontChoice, UiFontSettings};
use egui::{Color32, Layout, Panel, Pos2, Rect, TextEdit, UiBuilder};
use xserial_client::SessionManager;
use xserial_client::config::SessionConfig;
use xserial_client::session::SessionEvent;
use xserial_core::protocol::DecodedData;
use xserial_core::transport::TransportConfig;

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
    Plot,
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
    pub plot: PlotBuffer,
    pub plot_view: plot_view::PlotViewState,
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
    font_settings_open: bool,
    font_candidates: Vec<FontCandidate>,
    font_settings: UiFontSettings,
    primary_font_search: String,
    fallback_font_search: String,
    primary_filtered_fonts: Vec<usize>,
    fallback_filtered_fonts: Vec<usize>,
    primary_filter_cache_key: String,
    fallback_filter_cache_key: String,
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
        let repaint_ctx = ctx.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if event_tx.send(event).is_err() {
                            break;
                        }
                        repaint_ctx.request_repaint();
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        let font_candidates = ui_fonts::discover_font_candidates();
        let font_settings = ui_fonts::load_font_settings();
        ui_fonts::apply_font_settings(&ctx, &font_settings, &font_candidates);
        let saved_state = app_state::load_gui_state();

        let mut app = Self {
            manager,
            tabs: Vec::new(),
            active: 0,
            display: DisplayOptions {
                show_timestamp: saved_state.show_timestamp,
                show_direction: saved_state.show_direction,
                show_pipeline: saved_state.show_pipeline,
            },
            font_settings_open: false,
            font_candidates,
            font_settings,
            primary_font_search: String::new(),
            fallback_font_search: String::new(),
            primary_filtered_fonts: Vec::new(),
            fallback_filtered_fonts: Vec::new(),
            primary_filter_cache_key: String::from("\0"),
            fallback_filter_cache_key: String::from("\0"),
            config_open: false,
            config_target: None,
            config_form: config::ConfigForm::default(),
            event_rx,
            pending: Vec::new(),
        };
        app.restore_saved_sessions(saved_state);
        app
    }

    fn open_create_config(&mut self) {
        self.config_target = None;
        self.config_form = config::ConfigForm::default();
        self.config_open = true;
    }

    fn restore_saved_sessions(&mut self, saved_state: PersistedGuiState) {
        for session_config in saved_state.sessions {
            self.add_session_tab(session_config);
        }
        if !self.tabs.is_empty() {
            self.active = saved_state.active.min(self.tabs.len() - 1);
        }
    }

    fn add_session_tab(&mut self, session_config: SessionConfig) {
        let history_limit = session_config.history_limit;
        let auto_reconnect = session_config.auto_reconnect;
        let handle = self.manager.create(session_config.clone());
        self.tabs.push(SessionTab {
            id: handle.id(),
            session_config,
            status: ConnectionStatus::Connecting,
            console: TextBuffer::new(history_limit),
            hex: HexBuffer::new(history_limit),
            plot: PlotBuffer::new(history_limit),
            plot_view: plot_view::PlotViewState::default(),
            view: View::Text,
            auto_reconnect,
            send_input: String::new(),
            send_mode: SendMode::Text,
            append_newline: true,
            send_status: None,
        });
    }

    fn persist_gui_state(&self) {
        app_state::save_gui_state(&PersistedGuiState {
            sessions: self
                .tabs
                .iter()
                .map(|tab| tab.session_config.clone())
                .collect(),
            active: if self.tabs.is_empty() {
                0
            } else {
                self.active.min(self.tabs.len() - 1)
            },
            show_timestamp: self.display.show_timestamp,
            show_direction: self.display.show_direction,
            show_pipeline: self.display.show_pipeline,
        });
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
            tab.console.set_limit(history_limit);
            tab.hex.set_limit(history_limit);
            tab.plot.set_limit(history_limit);
            tab.status = ConnectionStatus::Connecting;
            tab.send_status = Some(String::from("Session reconfigured"));
        }
        self.persist_gui_state();

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
                            DecodedData::Plot(_) => tab.plot.push(&entry),
                            DecodedData::Binary(_) => {}
                        }
                    }
                }
            }
        }
    }

    fn wants_live_plot_repaint(&self) -> bool {
        self.tabs
            .get(self.active)
            .map(|tab| {
                matches!(tab.view, View::Plot) && matches!(tab.status, ConnectionStatus::Connected)
            })
            .unwrap_or(false)
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
                self.add_session_tab(session_config);
                self.active = self.tabs.len() - 1;
                self.persist_gui_state();
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
        let previous_active = self.active;

        Panel::left("sidebar")
            .resizable(false)
            .show_inside(ui, |ui| {
                let sessions: Vec<_> = self
                    .tabs
                    .iter()
                    .map(|tab| sidebar::SessionListItem {
                        id: tab.id,
                        status: tab.status.clone(),
                        transport_summary: transport_summary(&tab.session_config.transport),
                    })
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
            self.persist_gui_state();
        } else if self.active != previous_active {
            self.persist_gui_state();
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
            let mut persist_state = false;

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
                    if ui
                        .selectable_label(tab.view == View::Plot, "Plot")
                        .clicked()
                    {
                        tab.view = View::Plot;
                    }
                });

                let (configure_clicked, auto_reconnect_changed) =
                    render_session_controls(ui, &manager, tab);
                if configure_clicked {
                    edit_session = Some(tab.id);
                }
                persist_state |= auto_reconnect_changed;

                let mut display_changed = false;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Show:");
                    display_changed |= ui
                        .checkbox(&mut display.show_timestamp, "Timestamp")
                        .changed();
                    display_changed |= ui
                        .checkbox(&mut display.show_direction, "Direction")
                        .changed();
                    display_changed |= ui.checkbox(&mut display.show_pipeline, "Pipe").changed();
                });
                persist_state |= display_changed;

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
                        View::Plot => plot_view::render(ui, &tab.plot, tab.id, &mut tab.plot_view),
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
            if persist_state {
                self.persist_gui_state();
            }
        });
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        Panel::top("top_bar").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("xserial");
                ui.separator();
                // ui.label(format!(
                //     "Fonts: {} + {}  {:.1} pt",
                //     ui_fonts::font_choice_label(
                //         &self.font_settings.primary_choice,
                //         &self.font_candidates
                //     ),
                //     ui_fonts::font_choice_label(
                //         &self.font_settings.fallback_choice,
                //         &self.font_candidates
                //     ),
                //     self.font_settings.ui_font_size
                // ));
                if ui.button("UI Settings").clicked() {
                    self.font_settings_open = true;
                }
            });
        });
    }

    fn render_font_settings_window(&mut self, ctx: &egui::Context) {
        if !self.font_settings_open {
            return;
        }

        let mut open = self.font_settings_open;
        let mut changed = false;
        egui::Window::new("UI Settings")
            .open(&mut open)
            .default_width(520.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Fonts");
                ui.horizontal(|ui| {
                    ui.label("Primary:");
                    ui.monospace(ui_fonts::font_choice_label(
                        &self.font_settings.primary_choice,
                        &self.font_candidates,
                    ));
                    ui.label("Fallback:");
                    ui.monospace(ui_fonts::font_choice_label(
                        &self.font_settings.fallback_choice,
                        &self.font_candidates,
                    ));
                    if ui.button("Refresh").clicked() {
                        self.font_candidates = ui_fonts::discover_font_candidates();
                        self.invalidate_font_filters();
                    }
                });
                ui.small("Primary font is tried first. Fallback font is used when the primary font lacks a glyph.");
                ui.add_space(6.0);
                render_font_selector(
                    ui,
                    "Primary font",
                    "primary_font_choice",
                    &mut self.font_settings.primary_choice,
                    &mut self.primary_font_search,
                    &self.font_candidates,
                    &mut self.primary_filtered_fonts,
                    &mut self.primary_filter_cache_key,
                    true,
                    true,
                    180.0,
                    &mut changed,
                );
                ui.separator();
                render_font_selector(
                    ui,
                    "Fallback font",
                    "fallback_font_choice",
                    &mut self.font_settings.fallback_choice,
                    &mut self.fallback_font_search,
                    &self.font_candidates,
                    &mut self.fallback_filtered_fonts,
                    &mut self.fallback_filter_cache_key,
                    true,
                    true,
                    140.0,
                    &mut changed,
                );
                ui.separator();
                ui.heading("Sizes");
                ui.label("UI font size");
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.font_settings.ui_font_size, 10.0..=28.0)
                            .suffix(" pt"),
                    )
                    .changed();
                ui.label("Monospace font size");
                changed |= ui
                    .add(
                        egui::Slider::new(
                            &mut self.font_settings.monospace_font_size,
                            10.0..=28.0,
                        )
                        .suffix(" pt"),
                    )
                    .changed();
                ui.label("Heading size");
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.font_settings.heading_font_size, 14.0..=40.0)
                            .suffix(" pt"),
                    )
                    .changed();
                ui.separator();
                ui.heading("Preview");
                ui.label("The quick brown fox jumps over the lazy dog.");
                ui.label("中文预览：串口、网络、绘图、十六进制、会话管理。");
                ui.monospace("Monospace preview: 0123456789 ABCDEF deadbeef");
            });

        if changed {
            ui_fonts::apply_font_settings(ctx, &self.font_settings, &self.font_candidates);
            ui_fonts::save_font_settings(&self.font_settings);
        }
        self.font_settings_open = open;
    }

    fn invalidate_font_filters(&mut self) {
        self.primary_filtered_fonts.clear();
        self.fallback_filtered_fonts.clear();
        self.primary_filter_cache_key = String::from("\0");
        self.fallback_filter_cache_key = String::from("\0");
    }
}

fn transport_summary(transport: &TransportConfig) -> String {
    match transport {
        TransportConfig::Serial { port, .. } => format!("Serial {}", port),
        TransportConfig::Tcp { addr } => format!("TCP {}", addr),
        TransportConfig::Udp {
            bind_addr,
            remote_addr,
        } => match remote_addr {
            Some(remote_addr) => format!("UDP {} -> {}", bind_addr, remote_addr),
            None => format!("UDP {}", bind_addr),
        },
    }
}

fn render_font_selector(
    ui: &mut egui::Ui,
    title: &str,
    id_prefix: &str,
    choice: &mut FontChoice,
    search: &mut String,
    candidates: &[FontCandidate],
    filtered: &mut Vec<usize>,
    cache_key: &mut String,
    allow_auto: bool,
    allow_default: bool,
    max_height: f32,
    changed: &mut bool,
) {
    ui.label(title);
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(search);
    });
    ui.horizontal_wrapped(|ui| {
        if allow_auto {
            *changed |= ui
                .selectable_value(choice, FontChoice::Auto, "Auto")
                .changed();
        }
        if allow_default {
            *changed |= ui
                .selectable_value(choice, FontChoice::Default, "Default")
                .changed();
        }
    });
    ui.add_space(4.0);
    refresh_font_filter(search, candidates, filtered, cache_key);
    ui.small(format!("{} fonts", filtered.len()));
    let row_height = ui.spacing().interact_size.y;
    egui::ScrollArea::vertical()
        .id_salt(format!("{id_prefix}_scroll"))
        .max_height(max_height)
        .auto_shrink([false, false])
        .show_rows(ui, row_height, filtered.len(), |ui, row_range| {
            for row in row_range {
                if let Some(candidate) = filtered.get(row).and_then(|index| candidates.get(*index))
                {
                    let response = ui.selectable_value(
                        choice,
                        FontChoice::System(candidate.id.clone()),
                        candidate.display_label.as_str(),
                    );
                    *changed |= response.changed();
                    response.on_hover_text(&candidate.path);
                }
            }
        });
}

fn refresh_font_filter(
    search: &str,
    candidates: &[FontCandidate],
    filtered: &mut Vec<usize>,
    cache_key: &mut String,
) {
    let needle = search.trim().to_lowercase();
    if *cache_key == needle {
        return;
    }

    filtered.clear();
    if needle.is_empty() {
        filtered.extend(0..candidates.len());
    } else {
        filtered.extend(
            candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| candidate.search_key.contains(&needle))
                .map(|(index, _)| index),
        );
    }
    *cache_key = needle;
}

impl eframe::App for XserialApp {
    fn update(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_events();
        if self.wants_live_plot_repaint() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        self.render_top_bar(ui);
        self.render_font_settings_window(ui.ctx());
        self.render_config_window(ui.ctx());
        self.render_sidebar(ui);
        self.render_main_panel(ui);
    }
}

fn render_session_controls(
    ui: &mut egui::Ui,
    manager: &SessionManager,
    tab: &mut SessionTab,
) -> (bool, bool) {
    let mut configure_clicked = false;
    let mut auto_reconnect_changed = false;
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

        if ui.button("Clear").clicked() {
            tab.console.clear();
            tab.hex.clear();
            tab.plot.clear();
            tab.send_status = Some(String::from("Cleared"));
        }

        let response = ui.checkbox(&mut tab.auto_reconnect, "Auto reconnect");
        if response.changed() {
            tab.session_config.auto_reconnect = tab.auto_reconnect;
            auto_reconnect_changed = true;
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
    (configure_clicked, auto_reconnect_changed)
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

#[cfg(test)]
mod tests {
    use super::transport_summary;
    use xserial_core::transport::TransportConfig;
    use xserial_core::transport::serial::{
        SerialDataBits, SerialFlowControl, SerialParity, SerialStopBits,
    };

    #[test]
    fn transport_summary_formats_tcp_and_serial() {
        assert_eq!(
            transport_summary(&TransportConfig::Tcp {
                addr: String::from("127.0.0.1:8080"),
            }),
            "TCP 127.0.0.1:8080"
        );

        assert_eq!(
            transport_summary(&TransportConfig::Serial {
                port: String::from("COM7"),
                baud_rate: 115200,
                data_bits: SerialDataBits::Eight,
                parity: SerialParity::None,
                stop_bits: SerialStopBits::One,
                flow_control: SerialFlowControl::None,
            }),
            "Serial COM7"
        );
    }

    #[test]
    fn transport_summary_formats_udp() {
        assert_eq!(
            transport_summary(&TransportConfig::Udp {
                bind_addr: String::from("0.0.0.0:9000"),
                remote_addr: Some(String::from("127.0.0.1:9001")),
            }),
            "UDP 0.0.0.0:9000 -> 127.0.0.1:9001"
        );

        assert_eq!(
            transport_summary(&TransportConfig::Udp {
                bind_addr: String::from("127.0.0.1:0"),
                remote_addr: None,
            }),
            "UDP 127.0.0.1:0"
        );
    }
}
