use crate::app::ConnectionStatus;
use egui::{Color32, RichText, Ui};

pub fn render(
    ui: &mut Ui,
    sessions: &[(u64, ConnectionStatus)], // (id, 连接状态)
    active: &mut usize,                   // 当前选中的会话索引
    on_new: &mut bool,                    // 点击了"新建"
    on_delete: &mut Option<usize>,        // 点击了"删除"
) {
    ui.heading("Sessions");

    // 新建按钮 — 绿色文字
    if ui
        .button(RichText::new("+ New Session").color(Color32::GREEN))
        .clicked()
    {
        *on_new = true;
    }

    ui.separator();

    // 会话列表
    for (i, (id, status)) in sessions.iter().enumerate() {
        let (color, icon) = match status {
            ConnectionStatus::Connected => (Color32::GREEN, "●"),
            ConnectionStatus::Disconnected => (Color32::GRAY, "○"),
            ConnectionStatus::Connecting => (Color32::YELLOW, "◌"),
            ConnectionStatus::Error(_) => (Color32::RED, "⨉"),
        };

        // selectable_label: 点击高亮，clicked() 判断是否被点击
        let label = format!("{} Session {}", icon, id);
        if ui
            .selectable_label(*active == i, RichText::new(label).color(color))
            .clicked()
        {
            *active = i;
        }
    }

    ui.separator();

    // 删除按钮 — 红色文字
    if ui
        .button(RichText::new("Delete").color(Color32::RED))
        .clicked()
    {
        *on_delete = Some(*active);
    }
}
