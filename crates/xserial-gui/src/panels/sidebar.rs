use crate::app::ConnectionStatus;
use egui::{Color32, RichText, Ui};

pub struct SessionListItem {
    pub id: u64,
    pub status: ConnectionStatus,
    pub transport_summary: String,
}

pub fn render(
    ui: &mut Ui,
    sessions: &[SessionListItem],
    active: &mut usize,
    on_new: &mut bool,
    on_delete: &mut Option<usize>,
) {
    ui.heading("Sessions");

    if ui
        .button(RichText::new("+ New Session").color(Color32::GREEN))
        .clicked()
    {
        *on_new = true;
    }

    ui.separator();

    for (i, session) in sessions.iter().enumerate() {
        let (color, status_tag) = match &session.status {
            ConnectionStatus::Connected => (Color32::GREEN, "[connected]"),
            ConnectionStatus::Disconnected => (Color32::GRAY, "[disconnected]"),
            ConnectionStatus::Connecting => (Color32::YELLOW, "[connecting]"),
            ConnectionStatus::Error(_) => (Color32::RED, "[error]"),
        };

        let label = format!(
            "{} Session {}\n{}",
            status_tag, session.id, session.transport_summary
        );
        if ui
            .selectable_label(*active == i, RichText::new(label).color(color))
            .clicked()
        {
            *active = i;
        }
    }

    ui.separator();

    if ui
        .button(RichText::new("Delete").color(Color32::RED))
        .clicked()
    {
        *on_delete = Some(*active);
    }
}
