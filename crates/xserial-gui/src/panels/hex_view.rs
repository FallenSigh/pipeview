use crate::app::DisplayOptions;
use crate::buffers::{HexBuffer, HexLine, LineDirection};
use egui::{Label, RichText, ScrollArea, TextStyle, TextWrapMode, Ui};

pub fn render(ui: &mut Ui, buf: &HexBuffer, display: DisplayOptions) -> usize {
    let line_count = buf.len();
    if line_count == 0 {
        ui.label("no hex data");
        return 0;
    }
    let row_height = ui.text_style_height(&TextStyle::Monospace);
    ScrollArea::vertical().stick_to_bottom(true).show_rows(
        ui,
        row_height,
        line_count,
        |ui, row_range| {
            for row in row_range {
                if let Some(line) = buf.get(row) {
                    ui.add(
                        Label::new(RichText::new(format_hex_line(line, display)).monospace())
                            .wrap_mode(TextWrapMode::Extend),
                    );
                }
            }
        },
    );
    line_count
}

fn format_hex_line(line: &HexLine, display: DisplayOptions) -> String {
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
    format!("{prefix}{}  |{}|", line.hex, line.ascii)
}
