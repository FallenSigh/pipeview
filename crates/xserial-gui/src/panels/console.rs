use crate::app::{DisplayOptions, SearchState};
use crate::buffers::{ConsoleLine, LineDirection, TextBuffer};
use egui::{
    Color32, Label, ScrollArea, TextStyle, TextWrapMode, Ui,
    text::{LayoutJob, TextFormat},
};

pub fn render(
    ui: &mut Ui,
    buf: &TextBuffer,
    display: DisplayOptions,
    search: Option<&SearchState>,
) -> usize {
    let line_count = buf.len();
    let row_height = ui.text_style_height(&TextStyle::Monospace);
    ScrollArea::both().stick_to_bottom(true).show_rows(
        ui,
        row_height,
        line_count,
        |ui, row_range| {
            for row in row_range {
                if let Some(line) = buf.get(row) {
                    ui.add(
                        Label::new(format_console_line(line, display, search, ui.style()))
                            .wrap_mode(TextWrapMode::Extend),
                    );
                }
            }
        },
    );
    line_count
}

fn highlight_matches(
    text: &str,
    query: &str,
    case_sensitive: bool,
    default_format: TextFormat,
    highlight_format: TextFormat,
) -> LayoutJob {
    let search_text = if case_sensitive {
        text.to_string()
    } else {
        text.to_lowercase()
    };
    let query = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let mut job = LayoutJob::default();
    let mut last_end = 0;
    let mut idx = 0;
    while let Some(pos) = search_text[idx..].find(&query) {
        let start = idx + pos;
        let end = start + query.len();
        if last_end < start {
            job.append(&text[last_end..start], 0.0, default_format.clone());
        }
        job.append(&text[start..end], 0.0, highlight_format.clone());
        last_end = end;
        idx = end;
    }
    if last_end < text.len() {
        job.append(&text[last_end..], 0.0, default_format);
    }
    job
}

fn monospace_format(style: &egui::Style) -> TextFormat {
    TextFormat {
        font_id: style
            .text_styles
            .get(&TextStyle::Monospace)
            .cloned()
            .unwrap_or_default(),
            color: Color32::WHITE,
        ..Default::default()
    }
}

pub fn format_console_line(
    line: &ConsoleLine,
    display: DisplayOptions,
    search: Option<&SearchState>,
    style: &egui::Style,
) -> LayoutJob {
    let mut prefix = Vec::new();
    if display.show_timestamp {
        let ts = line.elapsed.as_secs_f64();
        let time = if ts < 60.0 {
            format!("{:05.2}", ts)
        } else {
            format!("{:02}:{:02}", (ts / 60.0) as u64, (ts % 60.0) as u64)
        };
        prefix.push(format!("[{time}]"));
    }
    if display.show_direction {
        let dir = match line.direction {
            LineDirection::In => "IN",
            LineDirection::Out => "OUT",
        };
        prefix.push(format!("[{dir}]"));
    }
    if display.show_pipeline {
        prefix.push(format!("[{}]", line.pipeline));
    }
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{} ", prefix.join(" "))
    };
    let full_text = format!("{prefix}{}", line.text);

    let default = monospace_format(style);
    let highlight = TextFormat {
        font_id: default.font_id.clone(),
        color: Color32::BLACK,
        background: Color32::from_rgb(255, 255, 0),
        ..Default::default()
    };

    match search {
        Some(state) if state.active && !state.query.is_empty() => {
            highlight_matches(&full_text, &state.query, state.case_sensitive, default, highlight)
        }
        _ => LayoutJob::single_section(full_text, default),
    }
}
