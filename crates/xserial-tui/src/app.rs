use std::{io, time::Duration};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use tokio::sync::broadcast;
use xserial_client::{
    DecoderConfig, FramerConfig, PipelineConfig, SessionConfig, SessionEvent, SessionId,
    SessionManager,
};
use xserial_core::protocol::DecodedData;
use xserial_core::transport::{
    TransportConfig, TransportType,
    serial::{SerialDataBits, SerialFlowControl, SerialParity, SerialStopBits, SerialTransport},
};

use crate::app_state::{self, PersistedTuiState};
use crate::buffers::{HexBuffer, TextBuffer};

#[derive(Clone)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    Error(String),
}

impl ConnectionStatus {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
            Self::Error(_) => "error",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Text,
    Hex,
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
    pub id: SessionId,
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

pub enum AppMode {
    Normal,
    SendInput,
    SessionForm(SessionForm),
}

pub enum SessionFormAction {
    Create,
    Edit(SessionId),
}

pub struct SessionForm {
    pub action: SessionFormAction,
    pub transport: TransportType,
    pub tcp_addr: String,
    pub udp_bind_addr: String,
    pub udp_remote_addr: String,
    pub serial_port: String,
    pub serial_baud_rate: String,
    pub serial_data_bits: SerialDataBits,
    pub serial_parity: SerialParity,
    pub serial_stop_bits: SerialStopBits,
    pub serial_flow_control: SerialFlowControl,
    pub pipeline_name: String,
    pub auto_reconnect: bool,
    pub focused_field: usize,
    pub available_serial_ports: Vec<String>,
}

impl SessionForm {
    fn new_create() -> Self {
        let available_serial_ports = available_serial_ports();
        let serial_port = available_serial_ports.first().cloned().unwrap_or_default();

        Self {
            action: SessionFormAction::Create,
            transport: TransportType::Tcp,
            tcp_addr: String::from("127.0.0.1:8080"),
            udp_bind_addr: String::from("0.0.0.0:8080"),
            udp_remote_addr: String::new(),
            serial_port,
            serial_baud_rate: String::from("115200"),
            serial_data_bits: SerialDataBits::Eight,
            serial_parity: SerialParity::None,
            serial_stop_bits: SerialStopBits::One,
            serial_flow_control: SerialFlowControl::None,
            pipeline_name: String::from("text"),
            auto_reconnect: false,
            focused_field: 0,
            available_serial_ports,
        }
    }

    fn from_config(id: SessionId, config: &SessionConfig) -> Self {
        let mut form = Self::new_create();
        form.action = SessionFormAction::Edit(id);
        form.pipeline_name = config
            .pipelines
            .first()
            .map(|pipeline| pipeline.name.clone())
            .unwrap_or_else(|| String::from("text"));
        form.auto_reconnect = config.auto_reconnect;

        match &config.transport {
            TransportConfig::Tcp { addr } => {
                form.transport = TransportType::Tcp;
                form.tcp_addr = addr.clone();
            }
            TransportConfig::Udp {
                bind_addr,
                remote_addr,
            } => {
                form.transport = TransportType::Udp;
                form.udp_bind_addr = bind_addr.clone();
                form.udp_remote_addr = remote_addr.clone().unwrap_or_default();
            }
            TransportConfig::Serial {
                port,
                baud_rate,
                data_bits,
                parity,
                stop_bits,
                flow_control,
                ..
            } => {
                form.transport = TransportType::Serial;
                form.serial_port = port.clone();
                form.serial_baud_rate = baud_rate.to_string();
                form.serial_data_bits = *data_bits;
                form.serial_parity = *parity;
                form.serial_stop_bits = *stop_bits;
                form.serial_flow_control = *flow_control;
            }
        }

        form
    }

    pub fn title(&self) -> &'static str {
        match self.action {
            SessionFormAction::Create => "New Session",
            SessionFormAction::Edit(_) => "Edit Session",
        }
    }

    pub fn rows(&self) -> Vec<String> {
        match self.transport {
            TransportType::Tcp => vec![
                format!("Transport: {}", transport_name(self.transport)),
                format!("Addr: {}", self.tcp_addr),
                format!("Pipeline: {}", self.pipeline_name),
                format!("Auto Reconnect: {}", self.auto_reconnect),
            ],
            TransportType::Udp => vec![
                format!("Transport: {}", transport_name(self.transport)),
                format!("Bind Addr: {}", self.udp_bind_addr),
                format!("Remote Addr: {}", self.udp_remote_addr),
                format!("Pipeline: {}", self.pipeline_name),
                format!("Auto Reconnect: {}", self.auto_reconnect),
            ],
            TransportType::Serial => vec![
                format!("Transport: {}", transport_name(self.transport)),
                format!("Port: {}", self.serial_port),
                format!("Baud Rate: {}", self.serial_baud_rate),
                format!(
                    "Data Bits: {}",
                    serial_data_bits_name(self.serial_data_bits)
                ),
                format!("Parity: {}", serial_parity_name(self.serial_parity)),
                format!(
                    "Stop Bits: {}",
                    serial_stop_bits_name(self.serial_stop_bits)
                ),
                format!(
                    "Flow Control: {}",
                    serial_flow_control_name(self.serial_flow_control)
                ),
                format!("Pipeline: {}", self.pipeline_name),
                format!("Auto Reconnect: {}", self.auto_reconnect),
            ],
        }
    }

    pub fn footer_lines(&self) -> Vec<String> {
        let mut lines = vec![
            String::from("Framer: Line, Decoder: Text, History: 1000"),
            String::from("Tab move, Left/Right change, Enter submit, Esc cancel"),
        ];
        if matches!(self.transport, TransportType::Serial) {
            if self.available_serial_ports.is_empty() {
                lines.push(String::from("Ports: no serial ports found"));
            } else {
                lines.push(format!("Ports: {}", self.available_serial_ports.join(", ")));
            }
        }
        lines
    }

    fn field_count(&self) -> usize {
        self.rows().len()
    }

    fn next_field(&mut self) {
        self.focused_field = (self.focused_field + 1) % self.field_count();
    }

    fn prev_field(&mut self) {
        self.focused_field = if self.focused_field == 0 {
            self.field_count() - 1
        } else {
            self.focused_field - 1
        };
    }

    fn cycle_transport(&mut self, forward: bool) {
        self.transport = match (self.transport, forward) {
            (TransportType::Serial, true) => TransportType::Tcp,
            (TransportType::Tcp, true) => TransportType::Udp,
            (TransportType::Udp, true) => TransportType::Serial,
            (TransportType::Serial, false) => TransportType::Udp,
            (TransportType::Tcp, false) => TransportType::Serial,
            (TransportType::Udp, false) => TransportType::Tcp,
        };
        self.focused_field = self.focused_field.min(self.field_count() - 1);
    }

    fn cycle_current_option(&mut self, forward: bool) {
        match self.transport {
            TransportType::Tcp => match self.focused_field {
                0 => self.cycle_transport(forward),
                3 => self.auto_reconnect = !self.auto_reconnect,
                _ => {}
            },
            TransportType::Udp => match self.focused_field {
                0 => self.cycle_transport(forward),
                4 => self.auto_reconnect = !self.auto_reconnect,
                _ => {}
            },
            TransportType::Serial => match self.focused_field {
                0 => self.cycle_transport(forward),
                1 => self.cycle_serial_port(forward),
                3 => self.serial_data_bits = next_serial_data_bits(self.serial_data_bits, forward),
                4 => self.serial_parity = next_serial_parity(self.serial_parity, forward),
                5 => self.serial_stop_bits = next_serial_stop_bits(self.serial_stop_bits, forward),
                6 => {
                    self.serial_flow_control =
                        next_serial_flow_control(self.serial_flow_control, forward)
                }
                8 => self.auto_reconnect = !self.auto_reconnect,
                _ => {}
            },
        }
    }

    fn cycle_serial_port(&mut self, forward: bool) {
        if self.available_serial_ports.is_empty() {
            return;
        }

        let ports = &self.available_serial_ports;
        let next_index = ports
            .iter()
            .position(|port| port == &self.serial_port)
            .map(|index| {
                if forward {
                    (index + 1) % ports.len()
                } else if index == 0 {
                    ports.len() - 1
                } else {
                    index - 1
                }
            })
            .unwrap_or(0);
        self.serial_port = ports[next_index].clone();
    }

    fn focused_text_field_mut(&mut self) -> Option<&mut String> {
        match self.transport {
            TransportType::Tcp => match self.focused_field {
                1 => Some(&mut self.tcp_addr),
                2 => Some(&mut self.pipeline_name),
                _ => None,
            },
            TransportType::Udp => match self.focused_field {
                1 => Some(&mut self.udp_bind_addr),
                2 => Some(&mut self.udp_remote_addr),
                3 => Some(&mut self.pipeline_name),
                _ => None,
            },
            TransportType::Serial => match self.focused_field {
                1 => Some(&mut self.serial_port),
                2 => Some(&mut self.serial_baud_rate),
                7 => Some(&mut self.pipeline_name),
                _ => None,
            },
        }
    }

    fn accept_char(&mut self, c: char) {
        let serial_baud_field =
            matches!(self.transport, TransportType::Serial) && self.focused_field == 2;
        if let Some(field) = self.focused_text_field_mut() {
            if serial_baud_field && !c.is_ascii_digit() {
                return;
            }
            field.push(c);
        } else if c == ' ' {
            self.cycle_current_option(true);
        }
    }

    fn backspace(&mut self) {
        if let Some(field) = self.focused_text_field_mut() {
            field.pop();
        }
    }
}

pub struct App {
    should_quit: bool,
    mode: AppMode,
    manager: SessionManager,
    tabs: Vec<SessionTab>,
    active: usize,
    display: DisplayOptions,
    session_rx: broadcast::Receiver<SessionEvent>,
    notice: Option<String>,
}

impl App {
    pub fn new(manager: SessionManager, session_rx: broadcast::Receiver<SessionEvent>) -> Self {
        let saved = app_state::load_tui_state();
        let mut app = Self {
            should_quit: false,
            mode: AppMode::Normal,
            manager,
            tabs: Vec::new(),
            active: 0,
            display: DisplayOptions {
                show_timestamp: saved.show_timestamp,
                show_direction: saved.show_direction,
                show_pipeline: saved.show_pipeline,
            },
            session_rx,
            notice: Some(String::from(
                "n new, e edit, i input, c connect, x delete, q quit",
            )),
        };
        app.restore_saved_sessions(saved);
        app
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn mode(&self) -> &AppMode {
        &self.mode
    }

    pub fn display(&self) -> DisplayOptions {
        self.display
    }

    pub fn tabs(&self) -> &[SessionTab] {
        &self.tabs
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active_tab(&self) -> Option<&SessionTab> {
        self.tabs.get(self.active)
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn help_text(&self) -> &'static str {
        match self.mode {
            AppMode::Normal => {
                "j/k select  n new  e edit  x delete  c connect  r reconnect  C clear  1 text  2 hex  i input  m mode  a newline  t/o/p display  q quit"
            }
            AppMode::SendInput => "type input  Enter send  Esc stop editing  Backspace delete",
            AppMode::SessionForm(_) => {
                "Tab/Shift+Tab move  Left/Right change  Enter submit  Esc cancel"
            }
        }
    }

    pub fn transport_summary(config: &SessionConfig) -> String {
        match &config.transport {
            TransportConfig::Serial { port, .. } => format!("Serial {port}"),
            TransportConfig::Tcp { addr } => format!("TCP {addr}"),
            TransportConfig::Udp {
                bind_addr,
                remote_addr,
            } => match remote_addr {
                Some(remote) => format!("UDP {bind_addr} -> {remote}"),
                None => format!("UDP {bind_addr}"),
            },
        }
    }

    fn restore_saved_sessions(&mut self, saved: PersistedTuiState) {
        for session_config in saved.sessions {
            self.add_session_tab(session_config);
        }
        if !self.tabs.is_empty() {
            self.active = saved.active.min(self.tabs.len() - 1);
        }
    }

    fn persist_state(&self) {
        app_state::save_tui_state(&PersistedTuiState {
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

    fn set_notice(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
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
            view: View::Text,
            auto_reconnect,
            send_input: String::new(),
            send_mode: SendMode::Text,
            append_newline: true,
            send_status: None,
        });
    }

    fn open_create_form(&mut self) {
        self.mode = AppMode::SessionForm(SessionForm::new_create());
    }

    fn open_edit_form(&mut self) {
        let Some(tab) = self.active_tab() else {
            self.set_notice("no session selected");
            return;
        };
        self.mode = AppMode::SessionForm(SessionForm::from_config(tab.id, &tab.session_config));
    }

    fn apply_session_config(&mut self, id: SessionId, session_config: SessionConfig) {
        let handle = self.manager.get(id);
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            let history_limit = session_config.history_limit;
            tab.session_config = session_config.clone();
            tab.auto_reconnect = session_config.auto_reconnect;
            tab.console.set_limit(history_limit);
            tab.hex.set_limit(history_limit);
            tab.status = ConnectionStatus::Connecting;
            tab.send_status = Some(String::from("Session reconfigured"));
        }
        self.persist_state();

        if let Some(handle) = handle {
            tokio::spawn(async move {
                let _ = handle.reconfigure(session_config).await;
            });
        } else if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.status = ConnectionStatus::Error(String::from("session not found"));
        }
    }

    fn delete_active_session(&mut self) {
        if self.tabs.is_empty() {
            self.set_notice("no session to delete");
            return;
        }

        let tab = self.tabs.remove(self.active);
        self.manager.remove(tab.id);
        if self.active >= self.tabs.len() && !self.tabs.is_empty() {
            self.active = self.tabs.len() - 1;
        }
        self.persist_state();
        self.set_notice(format!("deleted session {}", tab.id));
    }

    fn clear_active_buffers(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.console.clear();
            tab.hex.clear();
            tab.send_status = Some(String::from("Cleared"));
        } else {
            self.set_notice("no session selected");
        }
    }

    fn toggle_connection(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            self.set_notice("no session selected");
            return;
        };

        let connected = matches!(
            tab.status,
            ConnectionStatus::Connected | ConnectionStatus::Connecting
        );

        let Some(handle) = self.manager.get(tab.id) else {
            tab.status = ConnectionStatus::Error(String::from("session not found"));
            return;
        };

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
    }

    fn reconnect_active(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            self.set_notice("no session selected");
            return;
        };

        let Some(handle) = self.manager.get(tab.id) else {
            tab.status = ConnectionStatus::Error(String::from("session not found"));
            return;
        };

        tab.status = ConnectionStatus::Connecting;
        tokio::spawn(async move {
            let _ = handle.reconnect().await;
        });
    }

    fn toggle_auto_reconnect_active(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            self.set_notice("no session selected");
            return;
        };
        tab.auto_reconnect = !tab.auto_reconnect;
        tab.session_config.auto_reconnect = tab.auto_reconnect;
        let session_id = tab.id;
        let enabled = tab.auto_reconnect;
        let missing_handle_error = ConnectionStatus::Error(String::from("session not found"));
        let _ = tab;
        self.persist_state();

        let Some(handle) = self.manager.get(session_id) else {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.status = missing_handle_error;
            }
            return;
        };
        tokio::spawn(async move {
            let _ = handle.set_auto_reconnect(enabled).await;
        });
    }

    fn toggle_send_mode(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            self.set_notice("no session selected");
            return;
        };
        tab.send_mode = match tab.send_mode {
            SendMode::Text => SendMode::Hex,
            SendMode::Hex => SendMode::Text,
        };
    }

    fn enter_send_input(&mut self) {
        if self.tabs.is_empty() {
            self.set_notice("no session selected");
            return;
        }
        self.mode = AppMode::SendInput;
    }

    fn send_active_input(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            self.set_notice("no session selected");
            return;
        };

        let payload = match build_payload(tab) {
            Ok(Some(payload)) => payload,
            Ok(None) => {
                tab.send_status = Some(String::from("Nothing to send"));
                return;
            }
            Err(message) => {
                tab.send_status = Some(format!("Send failed: {message}"));
                return;
            }
        };

        let Some(handle) = self.manager.get(tab.id) else {
            tab.send_status = Some(String::from("Send failed: session not found"));
            return;
        };

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
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.session_rx.try_recv() {
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
                            DecodedData::Plot(_) | DecodedData::Binary(_) => {}
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::result_large_err)]
    fn submit_session_form(&mut self, form: SessionForm) -> Result<(), (SessionForm, String)> {
        let config = match build_session_config(&form) {
            Ok(config) => config,
            Err(err) => return Err((form, err)),
        };

        match form.action {
            SessionFormAction::Create => {
                self.add_session_tab(config);
                self.active = self.tabs.len() - 1;
                self.persist_state();
                self.set_notice("created session");
            }
            SessionFormAction::Edit(id) => {
                self.apply_session_config(id, config);
                self.set_notice(format!("updated session {id}"));
            }
        }
        self.mode = AppMode::Normal;
        Ok(())
    }

    fn on_normal_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') if self.active + 1 < self.tabs.len() => {
                self.active += 1;
                self.persist_state();
            }
            KeyCode::Char('k') if self.active > 0 => {
                self.active -= 1;
                self.persist_state();
            }
            KeyCode::Char('n') => self.open_create_form(),
            KeyCode::Char('e') => self.open_edit_form(),
            KeyCode::Char('x') => self.delete_active_session(),
            KeyCode::Char('c') => self.toggle_connection(),
            KeyCode::Char('r') => self.reconnect_active(),
            KeyCode::Char('C') => self.clear_active_buffers(),
            KeyCode::Char('i') => self.enter_send_input(),
            KeyCode::Char('m') => self.toggle_send_mode(),
            KeyCode::Char('a') => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.append_newline = !tab.append_newline;
                }
            }
            KeyCode::Char('u') => self.toggle_auto_reconnect_active(),
            KeyCode::Char('1') => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view = View::Text;
                }
            }
            KeyCode::Char('2') => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.view = View::Hex;
                }
            }
            KeyCode::Char('t') => {
                self.display.show_timestamp = !self.display.show_timestamp;
                self.persist_state();
            }
            KeyCode::Char('o') => {
                self.display.show_direction = !self.display.show_direction;
                self.persist_state();
            }
            KeyCode::Char('p') => {
                self.display.show_pipeline = !self.display.show_pipeline;
                self.persist_state();
            }
            KeyCode::Enter if modifiers.is_empty() => self.enter_send_input(),
            _ => {}
        }
    }

    fn on_send_input_key(&mut self, code: KeyCode) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            self.mode = AppMode::Normal;
            return;
        };

        match code {
            KeyCode::Esc => self.mode = AppMode::Normal,
            KeyCode::Backspace => {
                tab.send_input.pop();
            }
            KeyCode::Enter => {
                self.send_active_input();
                self.mode = AppMode::Normal;
            }
            KeyCode::Char(c) => {
                tab.send_input.push(c);
            }
            KeyCode::Tab => {
                tab.send_input.push('\t');
            }
            _ => {}
        }
    }

    fn on_form_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => self.mode = AppMode::Normal,
            KeyCode::Tab | KeyCode::Down => {
                if let AppMode::SessionForm(form) = &mut self.mode {
                    form.next_field();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let AppMode::SessionForm(form) = &mut self.mode {
                    form.prev_field();
                }
            }
            KeyCode::Left => {
                if let AppMode::SessionForm(form) = &mut self.mode {
                    form.cycle_current_option(false);
                }
            }
            KeyCode::Right => {
                if let AppMode::SessionForm(form) = &mut self.mode {
                    form.cycle_current_option(true);
                }
            }
            KeyCode::Backspace => {
                if let AppMode::SessionForm(form) = &mut self.mode {
                    form.backspace();
                }
            }
            KeyCode::Char(c) => {
                if let AppMode::SessionForm(form) = &mut self.mode {
                    form.accept_char(c);
                }
            }
            KeyCode::Enter => {
                let mode = std::mem::replace(&mut self.mode, AppMode::Normal);
                if let AppMode::SessionForm(form) = mode
                    && let Err((form, err)) = self.submit_session_form(form)
                {
                    self.mode = AppMode::SessionForm(form);
                    self.set_notice(err);
                }
            }
            _ => {}
        }
    }

    pub fn on_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match &self.mode {
            AppMode::Normal => self.on_normal_key(code, modifiers),
            AppMode::SendInput => self.on_send_input_key(code),
            AppMode::SessionForm(_) => self.on_form_key(code),
        }
    }
}

pub fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    loop {
        app.drain_events();
        terminal.draw(|frame| crate::ui::render(frame, &app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key.code, key.modifiers);
        }

        if app.should_quit() {
            break;
        }
    }
    Ok(())
}

fn build_session_config(form: &SessionForm) -> Result<SessionConfig, String> {
    let pipeline_name = form.pipeline_name.trim();
    if pipeline_name.is_empty() {
        return Err(String::from("pipeline name cannot be empty"));
    }

    let transport = match form.transport {
        TransportType::Tcp => {
            let addr = form.tcp_addr.trim();
            if addr.is_empty() {
                return Err(String::from("tcp addr cannot be empty"));
            }
            TransportConfig::Tcp {
                addr: addr.to_string(),
            }
        }
        TransportType::Udp => {
            let bind_addr = form.udp_bind_addr.trim();
            if bind_addr.is_empty() {
                return Err(String::from("udp bind addr cannot be empty"));
            }
            let remote_addr = {
                let remote = form.udp_remote_addr.trim();
                if remote.is_empty() {
                    None
                } else {
                    Some(remote.to_string())
                }
            };
            TransportConfig::Udp {
                bind_addr: bind_addr.to_string(),
                remote_addr,
            }
        }
        TransportType::Serial => {
            let port = form.serial_port.trim();
            if port.is_empty() {
                return Err(String::from("serial port cannot be empty"));
            }
            let baud_rate = form
                .serial_baud_rate
                .trim()
                .parse::<u32>()
                .map_err(|_| String::from("serial baud rate must be a positive integer"))?;
            TransportConfig::Serial {
                port: port.to_string(),
                baud_rate,
                data_bits: form.serial_data_bits,
                parity: form.serial_parity,
                stop_bits: form.serial_stop_bits,
                flow_control: form.serial_flow_control,
                dtr: false,
                rts: false,
            }
        }
    };

    Ok(SessionConfig {
        transport,
        pipelines: vec![PipelineConfig {
            name: pipeline_name.to_string(),
            framer: FramerConfig::Line {
                strip_cr: true,
                max_line_len: 4096,
            },
            decoder: DecoderConfig::Text {
                encoding: Default::default(),
            },
        }],
        history_limit: 1000,
        auto_reconnect: form.auto_reconnect,
    })
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

fn available_serial_ports() -> Vec<String> {
    let mut ports: Vec<_> = SerialTransport::list_ports()
        .into_iter()
        .map(|port| port.port_name)
        .collect();
    ports.sort();
    ports.dedup();
    ports
}

fn transport_name(transport: TransportType) -> &'static str {
    match transport {
        TransportType::Serial => "Serial",
        TransportType::Tcp => "TCP",
        TransportType::Udp => "UDP",
    }
}

fn serial_data_bits_name(value: SerialDataBits) -> &'static str {
    match value {
        SerialDataBits::Five => "5",
        SerialDataBits::Six => "6",
        SerialDataBits::Seven => "7",
        SerialDataBits::Eight => "8",
    }
}

fn serial_parity_name(value: SerialParity) -> &'static str {
    match value {
        SerialParity::None => "None",
        SerialParity::Odd => "Odd",
        SerialParity::Even => "Even",
    }
}

fn serial_stop_bits_name(value: SerialStopBits) -> &'static str {
    match value {
        SerialStopBits::One => "1",
        SerialStopBits::Two => "2",
    }
}

fn serial_flow_control_name(value: SerialFlowControl) -> &'static str {
    match value {
        SerialFlowControl::None => "None",
        SerialFlowControl::Software => "Software",
        SerialFlowControl::Hardware => "Hardware",
    }
}

fn next_serial_data_bits(value: SerialDataBits, forward: bool) -> SerialDataBits {
    match (value, forward) {
        (SerialDataBits::Five, true) => SerialDataBits::Six,
        (SerialDataBits::Six, true) => SerialDataBits::Seven,
        (SerialDataBits::Seven, true) => SerialDataBits::Eight,
        (SerialDataBits::Eight, true) => SerialDataBits::Five,
        (SerialDataBits::Five, false) => SerialDataBits::Eight,
        (SerialDataBits::Six, false) => SerialDataBits::Five,
        (SerialDataBits::Seven, false) => SerialDataBits::Six,
        (SerialDataBits::Eight, false) => SerialDataBits::Seven,
    }
}

fn next_serial_parity(value: SerialParity, forward: bool) -> SerialParity {
    match (value, forward) {
        (SerialParity::None, true) => SerialParity::Odd,
        (SerialParity::Odd, true) => SerialParity::Even,
        (SerialParity::Even, true) => SerialParity::None,
        (SerialParity::None, false) => SerialParity::Even,
        (SerialParity::Odd, false) => SerialParity::None,
        (SerialParity::Even, false) => SerialParity::Odd,
    }
}

fn next_serial_stop_bits(value: SerialStopBits, forward: bool) -> SerialStopBits {
    match (value, forward) {
        (SerialStopBits::One, true) => SerialStopBits::Two,
        (SerialStopBits::Two, true) => SerialStopBits::One,
        (SerialStopBits::One, false) => SerialStopBits::Two,
        (SerialStopBits::Two, false) => SerialStopBits::One,
    }
}

fn next_serial_flow_control(value: SerialFlowControl, forward: bool) -> SerialFlowControl {
    match (value, forward) {
        (SerialFlowControl::None, true) => SerialFlowControl::Software,
        (SerialFlowControl::Software, true) => SerialFlowControl::Hardware,
        (SerialFlowControl::Hardware, true) => SerialFlowControl::None,
        (SerialFlowControl::None, false) => SerialFlowControl::Hardware,
        (SerialFlowControl::Software, false) => SerialFlowControl::None,
        (SerialFlowControl::Hardware, false) => SerialFlowControl::Software,
    }
}
