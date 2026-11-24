use super::super::app::{GuiApp, Stage};
use super::super::theme;
use super::super::widgets::{push_button, spinner, ButtonKind};
use egui::Ui;
use std::time::Instant;

pub fn render_body(app: &mut GuiApp, ui: &mut Ui, stage: Stage, _started: Instant) {
    let p = app.palette;
    let dev_name = app
        .devices
        .get(app.selected_device)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "工位电脑".to_owned());

    let step_text = match stage {
        Stage::Handshake => "正在安全握手 (QUIC + TLS 1.3)…",
        Stage::Negotiate => "正在协商分辨率与 H.264 编码…",
        Stage::Stream => "正在接收画面首帧…",
    };

    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            // 12-spoke macOS spinner
            spinner(ui, &p);

            ui.add_space(14.0);

            // Headline
            ui.label(
                egui::RichText::new(format!("正在连接「{dev_name}」"))
                    .font(theme::semibold_font(15.0))
                    .color(p.text),
            );

            ui.add_space(4.0);

            // Step text
            ui.label(
                egui::RichText::new(step_text)
                    .font(theme::text_font(12.5))
                    .color(p.text_2),
            );

            ui.add_space(18.0);

            // Cancel button
            if push_button(ui, &p, "取消", ButtonKind::Secondary, false, None).clicked() {
                app.stop_session();
            }
        });
    });
}
