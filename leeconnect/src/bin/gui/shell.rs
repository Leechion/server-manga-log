use super::icons;
use super::theme::{self, Palette};
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, Layout, Rect, Sense, Stroke, Ui,
    ViewportCommand,
};

pub struct ShellAction {
    pub toggle_sidebar: bool,
    pub toggle_info: bool,
    pub toggle_theme: bool,
    pub toggle_net: bool,
}

pub fn render_toolbar(
    ui: &mut Ui,
    p: &Palette,
    device_name: &str,
    status_text: &str,
    is_online: bool,
    is_live: bool,
    sidebar_open: bool,
    show_info_active: bool,
    show_net_active: bool,
    search_query: &mut String,
) -> ShellAction {
    let mut action = ShellAction {
        toggle_sidebar: false,
        toggle_info: false,
        toggle_theme: false,
        toggle_net: false,
    };

    let tb_height = 52.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), tb_height), Sense::click_and_drag());

    // Window dragging on toolbar blank area
    if resp.double_clicked() {
        let is_max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!is_max));
    } else if resp.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }

    // Toolbar background and bottom hairline border
    ui.painter().rect_filled(rect, CornerRadius::ZERO, p.content);
    ui.painter().line_segment(
        [pos2(rect.min.x, rect.max.y), pos2(rect.max.x, rect.max.y)],
        Stroke::new(1.0, p.sep),
    );

    // Left side: Sidebar Toggle + Title Group
    let mut left_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                pos2(rect.min.x + 12.0, rect.min.y),
                vec2(360.0, tb_height),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );

    // Sidebar toggle button (30x28)
    let (s_rect, s_resp) = left_ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
    if s_resp.hovered() {
        left_ui.painter().rect_filled(s_rect, CornerRadius::same(6), p.hover);
    } else if sidebar_open {
        left_ui.painter().rect_filled(
            s_rect,
            CornerRadius::same(6),
            p.accent.gamma_multiply(0.12),
        );
    }
    let s_color = if sidebar_open { p.accent } else { p.text_2 };
    icons::draw_icon(left_ui.painter(), s_rect.center(), icons::PANEL_LEFT, 17.0, s_color);
    if s_resp.clicked() {
        action.toggle_sidebar = true;
    }

    left_ui.add_space(6.0);

    // Title group with drag capability
    let title_id = left_ui.scope_id().with("title-drag-area");
    let title_rect = Rect::from_min_size(pos2(rect.min.x + 46.0, rect.min.y), vec2(300.0, tb_height));
    let title_resp = left_ui.interact(title_rect, title_id, Sense::click_and_drag());
    if title_resp.double_clicked() {
        let is_max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!is_max));
    } else if title_resp.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }

    left_ui.vertical(|ui| {
        ui.add_space(2.0);
        // Main title (14pt semibold)
        ui.label(
            egui::RichText::new(device_name)
                .font(theme::semibold_font(14.0))
                .color(p.text),
        );
        // Subtitle status row (dot + text)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(5.0, 0.0);
            let dot_color = if is_online { p.green } else { p.text_3 };
            let (d_rect, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
            ui.painter().circle_filled(d_rect.center(), 3.0, dot_color);
            ui.label(
                egui::RichText::new(status_text)
                    .font(theme::text_font(11.0))
                    .color(p.text_2),
            );
        });
    });

    // Right side: Window buttons + Search/Info + Settings/Net + Theme toggle
    let win_btn_w = 46.0;
    let win_btns_total_w = win_btn_w * 3.0;

    let right_btns_w = if is_live { 118.0 } else { 278.0 };
    let right_x = rect.max.x - win_btns_total_w - right_btns_w;

    let mut right_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                pos2(right_x, rect.min.y),
                vec2(right_btns_w, tb_height),
            ))
            .layout(Layout::right_to_left(Align::Center)),
    );

    right_ui.add_space(8.0);

    // Theme toggle button (Sun / Moon)
    let (t_rect, t_resp) = right_ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
    if t_resp.hovered() {
        right_ui.painter().rect_filled(t_rect, CornerRadius::same(6), p.hover);
    }
    let t_icon = if p.dark { icons::SUN } else { icons::MOON };
    icons::draw_icon(right_ui.painter(), t_rect.center(), t_icon, 16.0, p.text_2);
    if t_resp.clicked() {
        action.toggle_theme = true;
    }

    right_ui.add_space(4.0);

    // Network & Hardware Capabilities Settings button
    let (n_rect, n_resp) = right_ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
    if n_resp.hovered() {
        right_ui.painter().rect_filled(n_rect, CornerRadius::same(6), p.hover);
    } else if show_net_active {
        right_ui.painter().rect_filled(
            n_rect,
            CornerRadius::same(6),
            p.accent.gamma_multiply(0.14),
        );
    }
    let n_color = if show_net_active { p.accent } else { p.text_2 };
    icons::draw_icon(right_ui.painter(), n_rect.center(), icons::SLIDERS, 16.0, n_color);
    if n_resp.clicked() {
        action.toggle_net = true;
    }
    let _ = n_resp.on_hover_text("网络穿透、中继与硬件设置");

    right_ui.add_space(6.0);

    if is_live {
        // Info button (ⓘ)
        let (i_rect, i_resp) = right_ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
        if i_resp.hovered() {
            right_ui.painter().rect_filled(i_rect, CornerRadius::same(6), p.hover);
        } else if show_info_active {
            right_ui.painter().rect_filled(
                i_rect,
                CornerRadius::same(6),
                p.accent.gamma_multiply(0.14),
            );
        }
        let i_color = if show_info_active { p.accent } else { p.text_2 };
        icons::draw_icon(right_ui.painter(), i_rect.center(), icons::INFO, 17.0, i_color);
        if i_resp.clicked() {
            action.toggle_info = true;
        }
    } else {
        // Search bar (190x28)
        let search_w = 190.0;
        let search_h = 28.0;
        let (search_rect, _) = right_ui.allocate_exact_size(vec2(search_w, search_h), Sense::hover());

        let mut search_ui = right_ui.new_child(
            egui::UiBuilder::new()
                .max_rect(search_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );

        search_ui.painter().rect_filled(search_rect, CornerRadius::same(7), p.hover);
        icons::draw_icon(
            search_ui.painter(),
            pos2(search_rect.min.x + 14.0, search_rect.center().y),
            icons::SEARCH,
            14.0,
            p.text_3,
        );

        let input_rect = Rect::from_min_max(
            pos2(search_rect.min.x + 28.0, search_rect.min.y + 2.0),
            pos2(search_rect.max.x - 8.0, search_rect.max.y - 2.0),
        );
        let mut input_child = search_ui.new_child(
            egui::UiBuilder::new().max_rect(input_rect).layout(Layout::left_to_right(Align::Center)),
        );
        input_child.add(
            egui::TextEdit::singleline(search_query)
                .hint_text("搜索设备…")
                .font(theme::text_font(13.0)),
        );
    }

    // Windows Titlebar Control Buttons (Minimize, Maximize/Restore, Close)
    let min_rect = Rect::from_min_size(pos2(rect.max.x - win_btn_w * 3.0, rect.min.y), vec2(win_btn_w, tb_height));
    let max_rect = Rect::from_min_size(pos2(rect.max.x - win_btn_w * 2.0, rect.min.y), vec2(win_btn_w, tb_height));
    let close_rect = Rect::from_min_size(pos2(rect.max.x - win_btn_w, rect.min.y), vec2(win_btn_w, tb_height));

    let min_resp = ui.interact(min_rect, ui.scope_id().with("win-min"), Sense::click());
    let max_resp = ui.interact(max_rect, ui.scope_id().with("win-max"), Sense::click());
    let close_resp = ui.interact(close_rect, ui.scope_id().with("win-close"), Sense::click());

    let painter = ui.painter();

    // Minimize button
    if min_resp.hovered() {
        painter.rect_filled(min_rect, CornerRadius::ZERO, p.hover);
    }
    painter.line_segment(
        [
            pos2(min_rect.center().x - 5.0, min_rect.center().y),
            pos2(min_rect.center().x + 5.0, min_rect.center().y),
        ],
        Stroke::new(1.0, p.text),
    );
    if min_resp.clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
    }

    // Maximize/Restore button
    let is_max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    if max_resp.hovered() {
        painter.rect_filled(max_rect, CornerRadius::ZERO, p.hover);
    }
    if is_max {
        // Restore symbol (two overlapping squares)
        let r1 = Rect::from_center_size(pos2(max_rect.center().x - 1.0, max_rect.center().y + 1.0), vec2(8.0, 8.0));
        let r2 = Rect::from_center_size(pos2(max_rect.center().x + 1.0, max_rect.center().y - 1.0), vec2(8.0, 8.0));
        painter.rect_stroke(r2, CornerRadius::ZERO, Stroke::new(1.0, p.text), egui::StrokeKind::Inside);
        painter.rect_stroke(r1, CornerRadius::ZERO, Stroke::new(1.0, p.text), egui::StrokeKind::Inside);
    } else {
        let box_r = Rect::from_center_size(max_rect.center(), vec2(10.0, 10.0));
        painter.rect_stroke(box_r, CornerRadius::ZERO, Stroke::new(1.0, p.text), egui::StrokeKind::Inside);
    }
    if max_resp.clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!is_max));
    }

    // Close button (red background on hover)
    if close_resp.hovered() {
        painter.rect_filled(close_rect, CornerRadius::ZERO, Color32::from_rgb(0xC4, 0x2B, 0x1C));
        icons::draw_icon(painter, close_rect.center(), icons::X, 12.0, Color32::WHITE);
    } else {
        icons::draw_icon(painter, close_rect.center(), icons::X, 12.0, p.text);
    }
    if close_resp.clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }

    action
}
