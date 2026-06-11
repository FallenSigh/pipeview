use crate::app_state::{self, PersistedGuiState};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::buffers::{HexBuffer, PlotBuffer, TextBuffer};
use crate::panels::{config, console, hex_view, plot_view, sidebar};
use crate::perf::{DrainStats, GuiProfiler, GuiSnapshot};
use crate::shortcuts::{self, default_bindings};
use crate::ui_fonts::{self, FontCandidate, FontChoice, UiFontSettings};
use egui::{Color32, Layout, Panel, Pos2, Rect, TextEdit, UiBuilder};
use xserial_client::SessionManager;
use xserial_client::config::SessionConfig;
use xserial_client::session::SessionEvent;
use xserial_core::protocol::DecodedData;
use xserial_core::transport::TransportConfig;

const DATA_REPAINT_INTERVAL: Duration = Duration::from_millis(33);

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

#[derive(Clone)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<usize>,
    pub current_match: usize,
    pub case_sensitive: bool,
    pub active: bool,
    pub just_opened: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            current_match: 0,
            case_sensitive: false,
            active: false,
            just_opened: false,
        }
    }
}

impl SearchState {
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current_match = 0;
        self.active = false;
        self.just_opened = false;
    }

    pub fn next(&mut self) {
        if !self.matches.is_empty() {
            self.current_match = (self.current_match + 1) % self.matches.len();
        }
    }

    pub fn prev(&mut self) {
        if !self.matches.is_empty() {
            self.current_match = if self.current_match == 0 {
                self.matches.len() - 1
            } else {
                self.current_match - 1
            };
        }
    }

    // pub fn current_line_index(&self) -> Option<usize> {
        // self.matches.get(self.current_match).copied()
    // }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    pub fn current_display(&self) -> usize {
        if self.matches.is_empty() {
            0
        } else {
            self.current_match + 1
        }
    }
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
    pub dtr: bool,
    pub rts: bool,
    pub send_input: String,
    pub send_mode: SendMode,
    pub append_newline: bool,
    pub send_status: Option<String>,
    pub search: SearchState,
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
    profiler: GuiProfiler,
    shortcut_bindings: Vec<(shortcuts::Action, egui::KeyboardShortcut)>,
}

impl XserialApp {
    pub fn new(
        manager: SessionManager,
        mut rx: tokio::sync::broadcast::Receiver<SessionEvent>,
        ctx: egui::Context,
    ) -> Self {
        let profiler = GuiProfiler::from_env();
        let repaint_counter = profiler.repaint_counter();
        let (event_tx, event_rx) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        tokio::spawn(async move {
            let mut last_repaint = Instant::now()
                .checked_sub(DATA_REPAINT_INTERVAL)
                .unwrap_or_else(Instant::now);
            let mut delayed_repaint_pending = false;
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if event_tx.send(event).is_err() {
                            break;
                        }
                        let elapsed = last_repaint.elapsed();
                        if elapsed >= DATA_REPAINT_INTERVAL {
                            repaint_counter.increment();
                            repaint_ctx.request_repaint();
                            last_repaint = Instant::now();
                            delayed_repaint_pending = false;
                        } else if !delayed_repaint_pending {
                            repaint_counter.increment();
                            repaint_ctx.request_repaint_after(DATA_REPAINT_INTERVAL - elapsed);
                            delayed_repaint_pending = true;
                        }
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
            profiler,
            shortcut_bindings: default_bindings(),
        };
        app.restore_saved_sessions(saved_state);
        app
    }

    fn execute(&mut self, action: shortcuts::Action) {
        use shortcuts::Action::*;

        match action {
            NextTab => {
                if self.tabs.len() > 1 {
                    self.active = (self.active + 1) % self.tabs.len();
                    self.persist_gui_state();
                }
            }
            PrevTab => {
                if self.tabs.len() > 1 {
                    self.active = if self.active == 0 {
                        self.tabs.len() - 1
                    } else {
                        self.active - 1
                    };
                    self.persist_gui_state();
                }
            }
            ViewText => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view = View::Text;
                }
            }
            ViewHex => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view = View::Hex;
                }
            }
            ViewPlot => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view = View::Plot;
                }
            }
            NewSession => self.open_create_config(),
            EditSession => {
                if let Some(tab) = self.tabs.get(self.active) {
                    self.open_edit_config(tab.id);
                }
            }
            DeleteSession => {
                if !self.tabs.is_empty() {
                    let id = self.tabs[self.active].id;
                    self.manager.remove(id);
                    self.tabs.remove(self.active);
                    if self.active >= self.tabs.len() && !self.tabs.is_empty() {
                        self.active = self.tabs.len() - 1;
                    }
                    self.persist_gui_state();
                }
            }
            ToggleConnect => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    let connected = matches!(
                        tab.status,
                        ConnectionStatus::Connected | ConnectionStatus::Connecting
                    );
                    if let Some(handle) = self.manager.get(tab.id) {
                        if connected {
                            tab.status = ConnectionStatus::Disconnected;
                            let handle = handle.clone();
                            tokio::spawn(async move {
                                let _ = handle.disconnect().await;
                            });
                        } else {
                            tab.status = ConnectionStatus::Connecting;
                            let handle = handle.clone();
                            tokio::spawn(async move {
                                let _ = handle.connect().await;
                            });
                        }
                    } else {
                        tab.status = ConnectionStatus::Error(String::from("session not found"));
                    }
                }
            }
            Clear => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.console.clear();
                    tab.hex.clear();
                    tab.plot.clear();
                    tab.send_status = Some(String::from("Cleared"));
                }
            }
            UiSettings => self.font_settings_open = true,
            CloseOverlay => {
                self.config_open = false;
                self.font_settings_open = false;
            }
            Search => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.search.active = true;
                    tab.search.just_opened = true;
                }
            }
            SearchNext => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    if tab.search.active {
                        tab.search.next();
                    }
                }
            }
            SearchPrev => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    if tab.search.active {
                        tab.search.prev();
                    }
                }
            }
        }
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
        let (dtr, rts) = match &session_config.transport {
            TransportConfig::Serial { dtr, rts, .. } => (*dtr, *rts),
            _ => (false, false),
        };
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
            dtr,
            rts,
            send_input: String::new(),
            send_mode: SendMode::Text,
            append_newline: true,
            send_status: None,
            search: SearchState::default(),
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
        let started = Instant::now();
        let mut stats = DrainStats {
            drained_events: 0,
            drained_data_events: 0,
            drained_text_events: 0,
            drained_hex_events: 0,
            drained_plot_events: 0,
            pending_events: 0,
        };
        while let Ok(event) = self.event_rx.try_recv() {
            self.pending.push(event);
        }
        stats.pending_events = self.pending.len();

        for event in self.pending.drain(..) {
            stats.drained_events += 1;
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
                    stats.drained_data_events += 1;
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                        match &entry.data {
                            DecodedData::Text(_) => {
                                stats.drained_text_events += 1;
                                tab.console.push(&entry);
                            }
                            DecodedData::Hex(_) => {
                                stats.drained_hex_events += 1;
                                tab.hex.push(&entry);
                            }
                            DecodedData::Plot(_) => {
                                stats.drained_plot_events += 1;
                                tab.plot.push(&entry);
                            }
                            DecodedData::Binary(_) => {}
                        }
                    }
                }
            }
        }
        self.profiler.record_drain(started.elapsed(), stats);
    }

    fn wants_live_plot_repaint(&self) -> bool {
        self.tabs.iter().enumerate().any(|(index, tab)| {
            matches!(tab.status, ConnectionStatus::Connected)
                && (tab.plot_view.detached
                    || (index == self.active && matches!(tab.view, View::Plot)))
        })
    }

    fn profiler_snapshot(&self) -> GuiSnapshot {
        if let Some(tab) = self.tabs.get(self.active) {
            GuiSnapshot {
                tabs: self.tabs.len(),
                active_view: match tab.view {
                    View::Text => "text",
                    View::Hex => "hex",
                    View::Plot => "plot",
                },
                active_text_lines: tab.console.len(),
                active_hex_lines: tab.hex.len(),
                active_plot_series: tab.plot.series_len(),
                active_plot_points: tab.plot.total_points(),
            }
        } else {
            GuiSnapshot {
                tabs: self.tabs.len(),
                active_view: "none",
                active_text_lines: 0,
                active_hex_lines: 0,
                active_plot_series: 0,
                active_plot_points: 0,
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
        let mut on_edit = None;
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
                sidebar::render(
                    ui,
                    &sessions,
                    &mut self.active,
                    &mut on_new,
                    &mut on_edit,
                    &mut on_delete,
                );
            });

        if on_new {
            self.open_create_config();
        }

        if let Some(index) = on_edit
            && let Some(tab) = self.tabs.get(index)
        {
            self.open_edit_config(tab.id);
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
            let mut persist_state = false;
            let mut text_render = None;
            let mut hex_render = None;
            let mut plot_render = None;

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

                let auto_reconnect_changed = render_session_controls(ui, &manager, tab);
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

                render_search_bar(ui, tab);

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
                    |ui| {
                        let started = Instant::now();
                        match tab.view {
                            View::Text => {
                                let line_count = console::render(ui, &tab.console, *display, tab.search.active.then_some(&tab.search));
                                text_render = Some((started.elapsed(), line_count));
                            }
                            View::Hex => {
                                let line_count = hex_view::render(ui, &tab.hex, *display, tab.search.active.then_some(&tab.search));
                                hex_render = Some((started.elapsed(), line_count));
                            }
                            View::Plot => {
                                if tab.plot_view.detached {
                                    ui.heading(format!(
                                        "Plot window detached for Session {}",
                                        tab.id
                                    ));
                                    ui.label("The plot is currently shown in a floating window.");
                                    if ui.button("Dock Plot Back").clicked() {
                                        tab.plot_view.detached = false;
                                    }
                                } else {
                                    let output = plot_view::render(
                                        ui,
                                        &mut tab.plot,
                                        tab.id,
                                        &mut tab.plot_view,
                                    );
                                    if output.toggle_detached {
                                        tab.plot_view.detached = true;
                                    }
                                    plot_render = Some((started.elapsed(), output.stats));
                                }
                            }
                        }
                    },
                );

                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(send_rect)
                        .layout(Layout::top_down(egui::Align::Min).with_cross_justify(true)),
                    |ui| render_send_panel(ui, &manager, tab),
                );
            }

            if let Some((elapsed, line_count)) = text_render {
                self.profiler.record_text_render(elapsed, line_count);
            }
            if let Some((elapsed, line_count)) = hex_render {
                self.profiler.record_hex_render(elapsed, line_count);
            }
            if let Some((elapsed, stats)) = plot_render {
                self.profiler.record_plot_render(elapsed, stats);
            }
            if persist_state {
                self.persist_gui_state();
            }
        });
    }

    fn render_detached_plot_windows(&mut self, ctx: &egui::Context) {
        let mut plot_renders = Vec::new();

        for tab in &mut self.tabs {
            if !tab.plot_view.detached {
                continue;
            }

            let mut open = true;
            let mut output = None;
            egui::Window::new(format!("Plot - Session {}", tab.id))
                .open(&mut open)
                .default_width(720.0)
                .default_height(480.0)
                .resizable(true)
                .vscroll(false)
                .show(ctx, |ui| {
                    let started = Instant::now();
                    let render_output =
                        plot_view::render(ui, &mut tab.plot, tab.id, &mut tab.plot_view);
                    output = Some((started.elapsed(), render_output));
                });

            if let Some((elapsed, render_output)) = output {
                if render_output.toggle_detached {
                    tab.plot_view.detached = false;
                }
                plot_renders.push((elapsed, render_output.stats));
            }

            if !open {
                tab.plot_view.detached = false;
            }
        }

        for (elapsed, stats) in plot_renders {
            self.profiler.record_plot_render(elapsed, stats);
        }
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
        {
            let supress = ui.ctx().egui_wants_keyboard_input();
            let actions = shortcuts::process(ui.ctx(), supress, &self.shortcut_bindings);

            for action in actions {
                self.execute(action);
            }
        }

        let frame_started = Instant::now();
        self.drain_events();
        if self.wants_live_plot_repaint() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        self.render_top_bar(ui);
        self.render_font_settings_window(ui.ctx());
        self.render_config_window(ui.ctx());
        self.render_sidebar(ui);
        self.render_main_panel(ui);
        self.render_detached_plot_windows(ui.ctx());
        self.profiler
            .record_frame(frame_started.elapsed(), self.profiler_snapshot());
    }
}

fn render_search_bar(ui: &mut egui::Ui, tab: &mut SessionTab) {
    if !tab.search.active {
        return;
    }
    ui.horizontal(|ui| {
        let response = ui.add(
            TextEdit::singleline(&mut tab.search.query)
                .hint_text("Search...")
                .desired_width(200.0),
        );
        if tab.search.just_opened {
            response.request_focus();
            tab.search.just_opened = false;
        }
        if response.changed() {
            let matches = match tab.view {
                View::Text => tab.console.search(&tab.search.query, tab.search.case_sensitive),
                View::Hex => tab.hex.search(&tab.search.query, tab.search.case_sensitive),
                View::Plot => Vec::new(),
            };
            tab.search.matches = matches;
            tab.search.current_match = 0;
        }
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            tab.search.next();
        }

        ui.label(format!(
            "{}/{}",
            tab.search.current_display(),
            tab.search.match_count()
        ));

        if ui.button("▲").clicked() {
            tab.search.prev();
        }
        if ui.button("▼").clicked() {
            tab.search.next();
        }

        if ui.checkbox(&mut tab.search.case_sensitive, "Aa").changed() {
            let matches = match tab.view {
                View::Text => tab.console.search(&tab.search.query, tab.search.case_sensitive),
                View::Hex => tab.hex.search(&tab.search.query, tab.search.case_sensitive),
                View::Plot => Vec::new(),
            };
            tab.search.matches = matches;
            tab.search.current_match = 0;
        }

        if ui.button("✕").clicked() {
            tab.search.clear();
        }
    });
}

fn render_session_controls(
    ui: &mut egui::Ui,
    manager: &SessionManager,
    tab: &mut SessionTab,
) -> bool {
    let mut auto_reconnect_changed = false;
    ui.horizontal_wrapped(|ui| {
        let connected = matches!(
            tab.status,
            ConnectionStatus::Connected | ConnectionStatus::Connecting
        );
        let connect_label = if connected { "Disconnect" } else { "Connect" };
        if ui.button(connect_label).clicked() {
            if let Some(handle) = manager.get(tab.id) {
                if connected {
                    tab.status = ConnectionStatus::Disconnected;
                    tokio::spawn(async move {
                        let _ = handle.disconnect().await;
                    });
                } else {
                    tab.status = ConnectionStatus::Connecting;
                    tokio::spawn(async move {
                        let _ = handle.connect().await;
                    });
                }
            } else {
                tab.status = ConnectionStatus::Error(String::from("session not found"));
            }
        }

        // if ui.button("Reconnect").clicked() {
        //     if let Some(handle) = manager.get(tab.id) {
        //         tab.status = ConnectionStatus::Connecting;
        //         tokio::spawn(async move {
        //             let _ = handle.reconnect().await;
        //         });
        //     } else {
        //         tab.status = ConnectionStatus::Error(String::from("session not found"));
        //     }
        // }

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
        if matches!(tab.status, ConnectionStatus::Connected)
            && matches!(tab.session_config.transport, TransportConfig::Serial { .. })
        {
            let dtr_changed = ui.checkbox(&mut tab.dtr, "DTR").changed();
            let rts_changed = ui.checkbox(&mut tab.rts, "RTS").changed();
            if dtr_changed || rts_changed {
                if let Some(handle) = manager.get(tab.id) {
                    let dtr = tab.dtr;
                    let rts = tab.rts;
                    tokio::spawn(async move {
                        if dtr_changed {
                            let _ = handle.set_dtr(dtr).await;
                        }
                        if rts_changed {
                            let _ = handle.set_rts(rts).await;
                        }
                    });
                } else {
                    tab.status = ConnectionStatus::Error(String::from("session not found"));
                }
            }
        }
    });
    auto_reconnect_changed
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
                dtr: false,
                rts: false,
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
