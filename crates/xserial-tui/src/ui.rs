use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, AppMode, ConnectionStatus, DisplayOptions, SendMode, SessionForm, View};
use crate::buffers::{ConsoleLine, HexLine, LineDirection};

const MAX_RENDER_LINES: usize = 400;

pub fn render(frame: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(20)])
        .split(frame.area());

    render_sidebar(frame, app, root[0]);
    render_main(frame, app, root[1]);

    if let AppMode::SessionForm(form) = app.mode() {
        render_session_form(frame, form);
    }
}

fn render_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .tabs()
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let prefix = if index == app.active_index() {
                "> "
            } else {
                "  "
            };
            let transport = App::transport_summary(&tab.session_config);
            ListItem::new(format!(
                "{prefix}[{}] {}\n   {}",
                tab.id,
                tab.status.badge(),
                transport
            ))
        })
        .collect();

    let sidebar = if items.is_empty() {
        Paragraph::new("No sessions.\nPress n to create one.")
            .block(Block::default().borders(Borders::ALL).title("Sessions"))
            .wrap(Wrap { trim: false })
    } else {
        Paragraph::new("").block(Block::default().borders(Borders::ALL).title("Sessions"))
    };

    frame.render_widget(sidebar, area);
    if !items.is_empty() {
        frame.render_widget(
            List::new(items).block(Block::default().borders(Borders::ALL).title("Sessions")),
            area,
        );
    }
}

fn render_main(frame: &mut Frame, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(7),
            Constraint::Length(3),
        ])
        .split(area);

    if let Some(tab) = app.active_tab() {
        render_header(frame, app, sections[0]);
        render_receive(frame, app, sections[1]);
        render_send(frame, app, sections[2]);
        let footer = format!(
            "{}{}",
            app.help_text(),
            app.notice()
                .map(|notice| format!(" | {notice}"))
                .unwrap_or_default()
        );
        frame.render_widget(
            Paragraph::new(footer)
                .block(Block::default().borders(Borders::ALL).title("Help"))
                .wrap(Wrap { trim: false }),
            sections[3],
        );

        if matches!(tab.status, ConnectionStatus::Error(_)) {
            if let ConnectionStatus::Error(message) = &tab.status {
                let _ = message;
            }
        }
    } else {
        frame.render_widget(
            Paragraph::new(format!(
                "No active session.\n{}\n{}",
                app.help_text(),
                app.notice().unwrap_or("")
            ))
            .block(Block::default().borders(Borders::ALL).title("xserial-tui"))
            .wrap(Wrap { trim: false }),
            area,
        );
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let tab = app.active_tab().expect("active tab");
    let display = app.display();
    let status_line = match &tab.status {
        ConnectionStatus::Error(message) => {
            format!("Session {} [{}]  {}", tab.id, tab.status.badge(), message)
        }
        _ => format!("Session {} [{}]", tab.id, tab.status.badge()),
    };
    let line2 = format!(
        "{}  |  View: {}  |  Send: {}  |  AutoReconnect: {}  |  AppendNewline: {}",
        App::transport_summary(&tab.session_config),
        match tab.view {
            View::Text => "Text",
            View::Hex => "Hex",
        },
        match tab.send_mode {
            SendMode::Text => "Text",
            SendMode::Hex => "Hex",
        },
        tab.auto_reconnect,
        tab.append_newline
    );
    let line3 = format!(
        "Display: ts={} dir={} pipe={}  |  Text lines={} Hex lines={}",
        display.show_timestamp,
        display.show_direction,
        display.show_pipeline,
        tab.console.len(),
        tab.hex.len()
    );
    frame.render_widget(
        Paragraph::new(format!("{status_line}\n{line2}\n{line3}"))
            .block(Block::default().borders(Borders::ALL).title("Session"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_receive(frame: &mut Frame, app: &App, area: Rect) {
    let tab = app.active_tab().expect("active tab");
    let display = app.display();
    let title = match tab.view {
        View::Text => "Receive Text",
        View::Hex => "Receive Hex",
    };

    let lines: Vec<String> = match tab.view {
        View::Text => tab
            .console
            .recent(MAX_RENDER_LINES)
            .into_iter()
            .map(|line| format_console_line(&line, display))
            .collect(),
        View::Hex => tab
            .hex
            .recent(MAX_RENDER_LINES)
            .into_iter()
            .map(|line| format_hex_line(&line, display))
            .collect(),
    };

    let body = if lines.is_empty() {
        String::from("no data")
    } else {
        lines.join("\n")
    };

    frame.render_widget(
        Paragraph::new(body)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_send(frame: &mut Frame, app: &App, area: Rect) {
    let tab = app.active_tab().expect("active tab");
    let editing = matches!(app.mode(), AppMode::SendInput);
    let title = if editing { "Send [editing]" } else { "Send" };
    let status = tab.send_status.as_deref().unwrap_or("idle");
    let hint = match tab.send_mode {
        SendMode::Text => "Press i to edit input, Enter to send while editing",
        SendMode::Hex => "Hex bytes, e.g. 48 65 6C 6C 6F",
    };
    let body = format!(
        "Mode: {}  |  Append newline: {}\nStatus: {}\nHint: {}\n\n{}",
        match tab.send_mode {
            SendMode::Text => "Text",
            SendMode::Hex => "Hex",
        },
        tab.append_newline,
        status,
        hint,
        if tab.send_input.is_empty() {
            String::from("<empty>")
        } else {
            tab.send_input.clone()
        }
    );

    frame.render_widget(
        Paragraph::new(body)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_session_form(frame: &mut Frame, form: &SessionForm) {
    let rows = form.rows();
    let footer_lines = form.footer_lines();
    let height = (rows.len() + footer_lines.len() + 4) as u16;
    let area = centered_rect(82, height, frame.area());
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(footer_lines.len() as u16),
        ])
        .margin(1)
        .split(area);

    let items: Vec<ListItem> = rows
        .into_iter()
        .enumerate()
        .map(|(idx, row)| ListItem::new(row).style(focus_style(idx == form.focused_field)))
        .collect();

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().borders(Borders::ALL).title(form.title()),
        area,
    );
    frame.render_widget(List::new(items), inner[0]);
    frame.render_widget(Paragraph::new(footer_lines.join("\n")), inner[1]);
}

fn format_console_line(line: &ConsoleLine, display: DisplayOptions) -> String {
    let mut prefix = Vec::new();
    if display.show_timestamp {
        prefix.push(format!("[{}]", format_elapsed(line.elapsed)));
    }
    if display.show_direction {
        prefix.push(format!(
            "[{}]",
            match line.direction {
                LineDirection::In => "IN",
                LineDirection::Out => "OUT",
            }
        ));
    }
    if display.show_pipeline {
        prefix.push(format!("[{}]", line.pipeline));
    }
    if prefix.is_empty() {
        line.text.clone()
    } else {
        format!("{} {}", prefix.join(" "), line.text)
    }
}

fn format_hex_line(line: &HexLine, display: DisplayOptions) -> String {
    let mut prefix = Vec::new();
    if display.show_timestamp {
        prefix.push(format!("[{}]", format_elapsed(line.elapsed)));
    }
    if display.show_direction {
        prefix.push(format!(
            "[{}]",
            match line.direction {
                LineDirection::In => "IN",
                LineDirection::Out => "OUT",
            }
        ));
    }
    if display.show_pipeline {
        prefix.push(format!("[{}]", line.pipeline));
    }
    let base = format!("{}  |{}|", line.hex, line.ascii);
    if prefix.is_empty() {
        base
    } else {
        format!("{} {}", prefix.join(" "), base)
    }
}

fn format_elapsed(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:05.2}")
    } else {
        format!("{:02}:{:02}", (secs / 60.0) as u64, (secs % 60.0) as u64)
    }
}

fn focus_style(focused: bool) -> Style {
    if focused {
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height.min(area.height)),
            Constraint::Fill(1),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width.min(area.width)),
            Constraint::Fill(1),
        ])
        .split(vertical[1])[1]
}
