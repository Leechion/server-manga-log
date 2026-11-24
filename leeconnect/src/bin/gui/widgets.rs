use super::icons;
use super::theme::{self, Palette};
use egui::{
    epaint::PathShape, pos2, vec2, Align, Align2, Color32, CornerRadius, InnerResponse, Layout,
    Mesh, Painter, Pos2, Rect, Response, Sense, Stroke, Ui,
};
use std::f32::consts::PI;

// ----------------------------------------------------------------------------
// Sidebar Item
// ----------------------------------------------------------------------------
pub fn side_item(
    ui: &mut Ui,
    p: &Palette,
    icon: char,
    name: &str,
    is_online: bool,
    is_self: bool,
    selected: bool,
) -> Response {
    let desired = vec2(ui.available_width(), 30.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Background: selected -> sel, hover -> hover
        if selected {
            painter.rect_filled(rect, CornerRadius::same(7), p.sel);
        } else if resp.hovered() {
            painter.rect_filled(rect, CornerRadius::same(7), p.hover);
        }

        // Icon
        let icon_color = if !is_online && !is_self {
            p.text_3
        } else {
            p.accent
        };
        icons::draw_icon(painter, pos2(rect.min.x + 18.0, rect.center().y), icon, 17.0, icon_color);

        // Name
        let name_color = if !is_online && !is_self {
            p.text_2
        } else {
            p.text
        };
        let font_id = theme::text_font(13.0);
        let name_galley = painter.layout_no_wrap(name.to_owned(), font_id, name_color);
        let text_pos = pos2(
            rect.min.x + 36.0,
            rect.center().y - name_galley.size().y * 0.5,
        );
        painter.galley(text_pos, name_galley, name_color);

        // Right side badge
        if is_self {
            let me_galley = painter.layout_no_wrap("本机".to_owned(), theme::text_font(11.0), p.text_3);
            let pos = pos2(rect.max.x - me_galley.size().x - 10.0, rect.center().y - me_galley.size().y * 0.5);
            painter.galley(pos, me_galley, p.text_3);
        } else if is_online {
            let dot_c = pos2(rect.max.x - 14.0, rect.center().y);
            painter.circle_filled(dot_c, 5.0, p.green.gamma_multiply(0.2));
            painter.circle_filled(dot_c, 3.0, p.green);
        } else {
            let dot_c = pos2(rect.max.x - 14.0, rect.center().y);
            painter.circle_filled(dot_c, 3.0, p.text_3);
        }
    }

    resp
}

// ----------------------------------------------------------------------------
// Inset Grouped Container
// ----------------------------------------------------------------------------
pub fn group<R>(ui: &mut Ui, p: &Palette, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    egui::Frame::new()
        .fill(p.group)
        .corner_radius(10)
        .stroke(Stroke::new(1.0, p.edge))
        .shadow(egui::Shadow {
            offset: [0, 1],
            blur: 3,
            spread: 0,
            color: Color32::from_black_alpha(if p.dark { 40 } else { 8 }),
        })
        .inner_margin(egui::Margin::symmetric(14, 0))
        .show(ui, add_contents)
}

pub fn group_row(ui: &mut Ui, p: &Palette, label: &str, draw_sep: bool, add_right: impl FnOnce(&mut Ui)) {
    let desired = vec2(ui.available_width(), 40.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());

    if draw_sep {
        ui.painter().line_segment(
            [pos2(rect.min.x, rect.min.y), pos2(rect.max.x, rect.min.y)],
            Stroke::new(1.0, p.sep),
        );
    }

    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );

    row_ui.label(
        egui::RichText::new(label)
            .font(theme::text_font(13.0))
            .color(p.text),
    );

    row_ui.with_layout(Layout::right_to_left(Align::Center), add_right);
}

// ----------------------------------------------------------------------------
// Push Buttons
// ----------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Secondary,
    Primary,
    Danger,
}

pub fn push_button(
    ui: &mut Ui,
    p: &Palette,
    text: &str,
    kind: ButtonKind,
    is_lg: bool,
    icon: Option<char>,
) -> Response {
    let h = if is_lg { 32.0 } else { 28.0 };
    let font_id = if is_lg { theme::semibold_font(13.5) } else { theme::semibold_font(13.0) };

    let text_color = match kind {
        ButtonKind::Secondary => p.text,
        ButtonKind::Primary | ButtonKind::Danger => Color32::WHITE,
    };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, text_color);
    let icon_w = if icon.is_some() { 20.0 } else { 0.0 };
    let pad_x = if is_lg { 20.0 } else { 14.0 };
    let desired = vec2(galley.size().x + icon_w + pad_x * 2.0, h);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let (bg, stroke) = match kind {
            ButtonKind::Secondary => {
                let bg = if resp.is_pointer_button_down_on() {
                    p.hover
                } else if resp.hovered() {
                    p.btn
                } else {
                    p.btn
                };
                (bg, Stroke::new(1.0, p.btn_edge))
            }
            ButtonKind::Primary => {
                let bg = if resp.is_pointer_button_down_on() {
                    p.accent.gamma_multiply(0.85)
                } else if resp.hovered() {
                    p.accent.gamma_multiply(0.92)
                } else {
                    p.accent
                };
                (bg, Stroke::new(1.0, p.accent.gamma_multiply(0.8)))
            }
            ButtonKind::Danger => {
                let bg = if resp.is_pointer_button_down_on() {
                    p.red.gamma_multiply(0.85)
                } else if resp.hovered() {
                    p.red.gamma_multiply(0.92)
                } else {
                    p.red
                };
                (bg, Stroke::new(1.0, p.red.gamma_multiply(0.8)))
            }
        };

        painter.rect(
            rect,
            CornerRadius::same(if is_lg { 8 } else { 7 }),
            bg,
            stroke,
            egui::StrokeKind::Inside,
        );

        let mut offset_x = rect.min.x + (rect.width() - (galley.size().x + icon_w)) * 0.5;
        if let Some(ic) = icon {
            icons::draw_icon(painter, pos2(offset_x + 8.0, rect.center().y), ic, 15.0, text_color);
            offset_x += 18.0;
        }

        let text_pos = pos2(
            offset_x,
            rect.center().y - galley.size().y * 0.5,
        );
        painter.galley(text_pos, galley, text_color);
    }

    resp
}

// ----------------------------------------------------------------------------
// Animated Switch
// ----------------------------------------------------------------------------
pub fn switch(ui: &mut Ui, p: &Palette, on: &mut bool) -> Response {
    let desired = vec2(32.0, 19.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());

    if resp.clicked() {
        *on = !*on;
    }

    if ui.is_rect_visible(rect) {
        let anim_val = ui.ctx().animate_bool(resp.id, *on);
        let painter = ui.painter();

        // Track color
        let off_bg = Color32::from_rgba_premultiplied(120, 120, 128, 60);
        let on_bg = p.accent;
        let track_bg = Color32::from_rgb(
            (off_bg.r() as f32 * (1.0 - anim_val) + on_bg.r() as f32 * anim_val) as u8,
            (off_bg.g() as f32 * (1.0 - anim_val) + on_bg.g() as f32 * anim_val) as u8,
            (off_bg.b() as f32 * (1.0 - anim_val) + on_bg.b() as f32 * anim_val) as u8,
        );

        painter.rect_filled(rect, CornerRadius::same(10), track_bg);

        // Thumb: 15px diameter
        let thumb_r = 7.5;
        let thumb_x = rect.min.x + 2.0 + thumb_r + anim_val * 13.0;
        let thumb_c = pos2(thumb_x, rect.center().y);

        // Soft thumb shadow
        painter.circle_filled(
            pos2(thumb_c.x, thumb_c.y + 0.5),
            thumb_r,
            Color32::from_black_alpha(35),
        );
        painter.circle_filled(thumb_c, thumb_r, Color32::WHITE);
    }

    resp
}

// ----------------------------------------------------------------------------
// Popup Button (Text + Chevrons-Up-Down)
// ----------------------------------------------------------------------------
pub fn popup_button(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    let font_id = theme::text_font(13.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, p.text_2);
    let desired = vec2(galley.size().x + 22.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if resp.hovered() {
            painter.rect_filled(rect, CornerRadius::same(6), p.hover);
        }
        let text_pos = pos2(rect.min.x + 6.0, rect.center().y - galley.size().y * 0.5);
        painter.galley(text_pos, galley, if resp.hovered() { p.text } else { p.text_2 });

        icons::draw_icon(
            painter,
            pos2(rect.max.x - 10.0, rect.center().y),
            icons::CHEVRONS_UP_DOWN,
            12.0,
            p.text_3,
        );
    }

    resp
}

// ----------------------------------------------------------------------------
// HUD Floating Button
// ----------------------------------------------------------------------------
pub fn hud_button(
    ui: &mut Ui,
    _p: &Palette,
    icon: char,
    active: bool,
    is_power: bool,
    tooltip: &str,
) -> Response {
    let desired = vec2(36.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        let (bg, fg) = if is_power {
            if resp.hovered() {
                (Color32::from_rgba_premultiplied(255, 69, 58, 50), Color32::from_rgb(0xFF, 0x7D, 0x74))
            } else {
                (Color32::TRANSPARENT, Color32::from_rgb(0xFF, 0x6B, 0x61))
            }
        } else if active {
            (
                Color32::from_rgba_premultiplied(10, 132, 255, 70),
                Color32::from_rgb(0x6C, 0xB6, 0xFF),
            )
        } else if resp.hovered() {
            (Color32::from_white_alpha(30), Color32::WHITE)
        } else {
            (Color32::TRANSPARENT, Color32::from_white_alpha(210))
        };

        if bg != Color32::TRANSPARENT {
            painter.rect_filled(rect, CornerRadius::same(9), bg);
        }

        icons::draw_icon(painter, rect.center(), icon, 18.0, fg);
    }

    resp.on_hover_text(tooltip)
}

// ----------------------------------------------------------------------------
// macOS 12-Spoke Spinner (Activity Indicator)
// ----------------------------------------------------------------------------
pub fn spinner(ui: &mut Ui, p: &Palette) -> Response {
    let desired = vec2(30.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::hover());

    if ui.is_rect_visible(rect) {
        ui.ctx().request_repaint();
        let time = ui.input(|i| i.time) as f32;
        let phase = ((time * 12.0) as i32).rem_euclid(12);

        let painter = ui.painter();
        let center = rect.center();
        let r_inner = 7.0;
        let r_outer = 13.0;

        for i in 0..12 {
            let angle = (i as f32) * (2.0 * PI / 12.0) - PI * 0.5;
            let dist = (phase - i).rem_euclid(12);
            let alpha = 0.18 + 0.67 * (1.0 - dist as f32 / 12.0);

            let color = p.text_2.gamma_multiply(alpha);
            let p1 = pos2(center.x + angle.cos() * r_inner, center.y + angle.sin() * r_inner);
            let p2 = pos2(center.x + angle.cos() * r_outer, center.y + angle.sin() * r_outer);
            painter.line_segment([p1, p2], Stroke::new(2.4, color));
        }
    }

    resp
}

// ----------------------------------------------------------------------------
// Device Illustration (Monitor / Laptop, 196px width)
// ----------------------------------------------------------------------------
pub fn dev_art(ui: &mut Ui, p: &Palette, is_laptop: bool, is_offline: bool) {
    let w = 196.0;
    let h = if is_laptop { 130.0 } else { 152.0 };
    let desired = vec2(w, h);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());

    let painter = ui.painter();

    // Screen dimension: 196x122
    let scr_w = 196.0;
    let scr_h = 122.0;
    let scr_rect = Rect::from_min_size(rect.min, vec2(scr_w, scr_h));

    // Outer bezel: dark with 11px corner radius
    let bezel_bg = Color32::from_rgb(0x18, 0x19, 0x1C);
    painter.rect(
        scr_rect,
        CornerRadius::same(11),
        bezel_bg,
        Stroke::new(1.0, Color32::from_white_alpha(18)),
        egui::StrokeKind::Inside,
    );

    // Inner screen display: shrink 5px
    let disp_rect = scr_rect.shrink(5.0);

    // Draw display gradient
    if is_offline {
        painter.rect_filled(disp_rect, CornerRadius::same(7), Color32::from_rgb(0x1F, 0x21, 0x26));
    } else {
        // Multi-color macOS gradient (Sky blue to dark navy)
        let c_top = Color32::from_rgb(0x7D, 0xA8, 0xEE);
        let c_mid = Color32::from_rgb(0x23, 0x58, 0xB6);
        let c_bot = Color32::from_rgb(0x0A, 0x18, 0x38);

        // Top half
        let mid_y = disp_rect.center().y;
        let mut m1 = Mesh::default();
        m1.colored_vertex(disp_rect.left_top(), c_top);
        m1.colored_vertex(disp_rect.right_top(), c_top);
        m1.colored_vertex(pos2(disp_rect.max.x, mid_y), c_mid);
        m1.colored_vertex(pos2(disp_rect.min.x, mid_y), c_mid);
        m1.add_triangle(0, 1, 2);
        m1.add_triangle(0, 2, 3);
        painter.add(m1);

        // Bottom half
        let mut m2 = Mesh::default();
        m2.colored_vertex(pos2(disp_rect.min.x, mid_y), c_mid);
        m2.colored_vertex(pos2(disp_rect.max.x, mid_y), c_mid);
        m2.colored_vertex(disp_rect.right_bottom(), c_bot);
        m2.colored_vertex(disp_rect.left_bottom(), c_bot);
        m2.add_triangle(0, 1, 2);
        m2.add_triangle(0, 2, 3);
        painter.add(m2);

        // Faux dock at bottom
        let dock_h = 10.0;
        let dock_rect = Rect::from_min_max(
            pos2(disp_rect.min.x, disp_rect.max.y - dock_h),
            disp_rect.max,
        );
        painter.rect_filled(dock_rect, CornerRadius::ZERO, Color32::from_black_alpha(110));
    }

    if is_laptop {
        // Laptop base
        let base_w = w * 1.16;
        let base_h = 8.0;
        let base_x = rect.min.x - (base_w - w) * 0.5;
        let base_y = scr_rect.max.y;
        let base_rect = Rect::from_min_size(pos2(base_x, base_y), vec2(base_w, base_h));
        painter.rect_filled(
            base_rect,
            CornerRadius {
                nw: 2,
                ne: 2,
                sw: 8,
                se: 8,
            },
            if p.dark { Color32::from_rgb(0x50, 0x53, 0x58) } else { Color32::from_rgb(0xC8, 0xCB, 0xD0) },
        );
        // Center thumb notch
        let notch_rect = Rect::from_center_size(pos2(base_rect.center().x, base_rect.min.y + 1.5), vec2(28.0, 3.0));
        painter.rect_filled(notch_rect, CornerRadius::same(2), Color32::from_black_alpha(35));
    } else {
        // Monitor stand: neck + foot
        let neck_top_w = 28.0;
        let neck_bot_w = 38.0;
        let neck_h = 18.0;
        let neck_top_y = scr_rect.max.y;
        let neck_c = rect.center().x;

        let p_tl = pos2(neck_c - neck_top_w * 0.5, neck_top_y);
        let p_tr = pos2(neck_c + neck_top_w * 0.5, neck_top_y);
        let p_br = pos2(neck_c + neck_bot_w * 0.5, neck_top_y + neck_h);
        let p_bl = pos2(neck_c - neck_bot_w * 0.5, neck_top_y + neck_h);

        let neck_color = if p.dark { Color32::from_rgb(0x5A, 0x5D, 0x62) } else { Color32::from_rgb(0xC0, 0xC4, 0xCA) };
        let shape = PathShape::convex_polygon(vec![p_tl, p_tr, p_br, p_bl], neck_color, Stroke::NONE);
        painter.add(shape);

        // Foot
        let foot_w = 84.0;
        let foot_h = 6.0;
        let foot_rect = Rect::from_center_size(pos2(neck_c, neck_top_y + neck_h + foot_h * 0.5), vec2(foot_w, foot_h));
        painter.rect_filled(
            foot_rect,
            CornerRadius {
                nw: 4,
                ne: 4,
                sw: 2,
                se: 2,
            },
            if p.dark { Color32::from_rgb(0x6A, 0x6D, 0x73) } else { Color32::from_rgb(0xD2, 0xD5, 0xDB) },
        );
    }
}
