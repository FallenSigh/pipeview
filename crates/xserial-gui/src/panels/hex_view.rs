use crate::app::DisplayOptions;
use crate::buffers::{HexBuffer, LineDirection};
use egui::{ScrollArea, Ui};

pub fn render(ui: &mut Ui, buf: &HexBuffer, display: DisplayOptions) {
    let mut lines = buf.iter().peekable();
    if lines.peek().is_none() {
        ui.label("no hex data");
        return;
    }
    ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        for line in lines {
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
            ui.monospace(format!("{prefix}{}  |{}|", line.hex, line.ascii));
        }
    });
}
