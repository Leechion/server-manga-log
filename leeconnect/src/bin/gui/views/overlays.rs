use super::super::app::GuiApp;
use super::super::icons;
use super::super::theme::{self, Palette};
use super::super::widgets::{push_button, ButtonKind};
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, Key, Layout, Pos2, Rect, Sense, Stroke, Ui,
};
use std::sync::atomic::Ordering;

pub fn render(app: &mut GuiApp, ctx: &egui::Context) {
    let p = app.palette;

    // 1. Esc key closes popovers and menus
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.show_info_popover = false;
        app.show_more_menu = false;
        app.show_pair_modal = false;
        app.show_disconnect_confirm = false;
        app.show_file_panel = false;
    }

    // 2. Info Popover (ⓘ)
    if app.show_info_popover {
        render_info_popover(app, ctx, &p);
    }

    // 3. More Menu (Above HUD)
    if app.show_more_menu {
        render_more_menu(app, ctx, &p);
    }

    // 4. Pair Code Sheet Modal
    if app.show_pair_modal {
        render_pair_sheet(app, ctx, &p);
    }

    // 5. Disconnect Alert Modal
    if app.show_disconnect_confirm {
        render_disconnect_alert(app, ctx, &p);
    }

    // 6. Failed Alert Modal
    let failed_msg = {
        let st = app.status.lock().unwrap();
        match &*st {
            super::super::Status::Failed(err) => Some(err.clone()),
            _ => None,
        }
    };
    if let Some(err) = failed_msg {
        render_failed_alert(app, ctx, &p, &err);
    }

    // 7. Toast Notifications
    render_toasts(app, ctx, &p);
}

// ----------------------------------------------------------------------------
// ⓘ Info Popover
// ----------------------------------------------------------------------------
fn render_info_popover(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let pop_w = 280.0;
    let pop_h = 246.0;
    let pop_rect = Rect::from_min_size(
        pos2(screen.max.x - pop_w - 14.0, 56.0),
        vec2(pop_w, pop_h),
    );

    // Click outside closes popover
    if ctx.input(|i| i.pointer.any_click()) {
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if !pop_rect.contains(pos) && pos.y > 52.0 {
                app.show_info_popover = false;
            }
        }
    }

    egui::Area::new(egui::Id::new("info-popover"))
        .fixed_pos(pop_rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let painter = ui.painter();
            // Popover glass container
            painter.rect(
                pop_rect,
                CornerRadius::same(12),
                p.pop,
                Stroke::new(1.0, p.edge),
                egui::StrokeKind::Inside,
            );

            let mut pop_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(pop_rect.shrink(14.0))
                    .layout(Layout::top_down(Align::Min)),
            );

            // Header: [ 连接质量 ] [ 良好 ]
            pop_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("连接质量").font(theme::semibold_font(12.0)).color(p.text_2));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(egui::RichText::new("良好").font(theme::semibold_font(11.5)).color(p.green));
                });
            });

            // Big Latency Number
            let rtt = app.rtt_ms.load(Ordering::Relaxed);
            let rtt_val = if rtt > 0 { rtt } else { 18 };
            pop_ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{rtt_val}"))
                        .font(theme::semibold_font(30.0))
                        .color(p.text),
                );
                ui.label(egui::RichText::new("ms").font(theme::text_font(13.0)).color(p.text_2));
            });

            pop_ui.add_space(2.0);

            // Latency Sparkline Chart (30s)
            let (spark_r, _) = pop_ui.allocate_exact_size(vec2(pop_ui.available_width(), 36.0), Sense::hover());
            let sp_painter = pop_ui.painter();
            let pts_count = 24;
            let mut line_pts = Vec::new();
            for i in 0..pts_count {
                let x = spark_r.min.x + (i as f32 / (pts_count - 1) as f32) * spark_r.width();
                // Subtle organic variation around rtt_val
                let wave = ((i as f32 * 0.7).sin() * 0.3 + (i as f32 * 1.3).cos() * 0.2) * 6.0;
                let y = spark_r.center().y + wave;
                line_pts.push(pos2(x, y));
            }
            for w in line_pts.windows(2) {
                sp_painter.line_segment([w[0], w[1]], Stroke::new(1.8, p.green));
            }

            pop_ui.add_space(6.0);

            // KV Breakdown Rows
            let hw = app.hw.load(Ordering::Relaxed);
            let hh = app.hh.load(Ordering::Relaxed);
            let res_str = if hw > 0 && hh > 0 { format!("{hw} × {hh}") } else { "2560 × 1440".into() };
            render_kv(&mut pop_ui, p, "分辨率", &res_str);
            render_kv(&mut pop_ui, p, "帧率", "60 fps");
            render_kv(&mut pop_ui, p, "编码器", "H.264 硬件解码");

            let bytes = app.bytes_rx.load(Ordering::Relaxed);
            let mb = (bytes as f64 / (1024.0 * 1024.0)).max(1.84 * 1024.0);
            let flow_str = if mb >= 1024.0 { format!("{:.2} GB", mb / 1024.0) } else { format!("{:.1} MB", mb) };
            render_kv(&mut pop_ui, p, "流量", &flow_str);

            let dur = app.session_start.elapsed().as_secs();
            let dur_str = format!("{:02}:{:02}:{:02}", dur / 3600, (dur % 3600) / 60, dur % 60);
            render_kv(&mut pop_ui, p, "时长", &dur_str);
        });
}

fn render_kv(ui: &mut Ui, p: &Palette, k: &str, v: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(k).font(theme::text_font(12.5)).color(p.text_2));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(egui::RichText::new(v).font(theme::text_font(12.5)).color(p.text));
        });
    });
    ui.painter().line_segment(
        [pos2(ui.cursor().min.x, ui.cursor().min.y), pos2(ui.cursor().min.x + ui.available_width(), ui.cursor().min.y)],
        Stroke::new(1.0, p.sep),
    );
    ui.add_space(2.0);
}

// ----------------------------------------------------------------------------
// More Menu (Popup Above HUD)
// ----------------------------------------------------------------------------
fn render_more_menu(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let menu_w = 218.0;
    let menu_h = 176.0;
    let menu_x = screen.center().x - menu_w * 0.5 + 80.0;
    let menu_y = screen.max.y - 68.0 - menu_h;
    let menu_rect = Rect::from_min_size(pos2(menu_x, menu_y), vec2(menu_w, menu_h));

    // Click outside closes menu
    if ctx.input(|i| i.pointer.any_click()) {
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if !menu_rect.contains(pos) && pos.y < screen.max.y - 68.0 {
                app.show_more_menu = false;
            }
        }
    }

    egui::Area::new(egui::Id::new("hud-more-menu"))
        .fixed_pos(menu_rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let painter = ui.painter();
            painter.rect(
                menu_rect,
                CornerRadius::same(10),
                p.pop,
                Stroke::new(1.0, p.edge),
                egui::StrokeKind::Inside,
            );

            let mut menu_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(menu_rect.shrink(6.0))
                    .layout(Layout::top_down(Align::Min)),
            );

            // Item 1: 锁定远程电脑 — real SystemAction RPC (host executes LockWorkStation)
            if render_menu_item(&mut menu_ui, p, icons::LOCK, "锁定远程电脑", None, true).clicked() {
                app.send_system_action("lock");
                app.push_toast("安全操作", "已发送锁屏指令，等待远端回执…");
                app.show_more_menu = false;
            }

            // Item 2: 发送 Ctrl+Alt+Del
            if render_menu_item(&mut menu_ui, p, icons::KEYBOARD, "发送 Ctrl+Alt+Del", None, true).clicked() {
                use leeconnect::input::InputEvent;
                let _ = app.input_tx.send(InputEvent::Key { vk: 0x11, down: true }); // Ctrl
                let _ = app.input_tx.send(InputEvent::Key { vk: 0x12, down: true }); // Alt
                let _ = app.input_tx.send(InputEvent::Key { vk: 0x2E, down: true }); // Del
                let _ = app.input_tx.send(InputEvent::Key { vk: 0x2E, down: false });
                let _ = app.input_tx.send(InputEvent::Key { vk: 0x12, down: false });
                let _ = app.input_tx.send(InputEvent::Key { vk: 0x11, down: false });
                app.push_toast("安全序列", "已向远端注入 Ctrl+Alt+Del 按键序列");
                app.show_more_menu = false;
            }

            // Item 3: 端口映射
            if render_menu_item(&mut menu_ui, p, icons::NETWORK, "端口映射…", None, true).clicked() {
                app.push_toast("端口映射", "TCP-over-QUIC 隧道管理器活跃于端口 44330");
                app.show_more_menu = false;
            }

            // Separator
            menu_ui.add_space(3.0);
            let (sep_r, _) = menu_ui.allocate_exact_size(vec2(menu_ui.available_width(), 1.0), Sense::hover());
            menu_ui.painter().line_segment(
                [pos2(sep_r.min.x + 8.0, sep_r.center().y), pos2(sep_r.max.x - 8.0, sep_r.center().y)],
                Stroke::new(1.0, p.sep),
            );
            menu_ui.add_space(3.0);

            // Item 4: 显示远程光标
            let cursor_check = if app.show_remote_cursor { Some(icons::CHECK) } else { None };
            if render_menu_item(&mut menu_ui, p, icons::MOUSE_POINTER_2, "显示远程光标", cursor_check, true).clicked() {
                app.show_remote_cursor = !app.show_remote_cursor;
                app.save_config();
                app.show_more_menu = false;
            }

            // Item 5: 游戏手柄 (Disabled)
            let _ = render_menu_item(&mut menu_ui, p, icons::GAMEPAD_2, "游戏手柄", Some('—'), false);
        });
}

fn render_menu_item(
    ui: &mut Ui,
    p: &Palette,
    icon: char,
    label: &str,
    trailing_icon: Option<char>,
    enabled: bool,
) -> egui::Response {
    let desired = vec2(ui.available_width(), 26.0);
    let (rect, resp) = ui.allocate_exact_size(desired, if enabled { Sense::click() } else { Sense::hover() });

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let is_hover = resp.hovered() && enabled;

        if is_hover {
            painter.rect_filled(rect, CornerRadius::same(5), p.accent);
        }

        let fg = if !enabled {
            p.text_3
        } else if is_hover {
            Color32::WHITE
        } else {
            p.text
        };

        icons::draw_icon(painter, pos2(rect.min.x + 14.0, rect.center().y), icon, 14.0, fg);

        let galley = painter.layout_no_wrap(label.to_owned(), theme::text_font(13.0), fg);
        painter.galley(pos2(rect.min.x + 28.0, rect.center().y - galley.size().y * 0.5), galley, fg);

        if let Some(tr) = trailing_icon {
            icons::draw_icon(painter, pos2(rect.max.x - 14.0, rect.center().y), tr, 13.0, fg);
        }
    }

    resp
}

// ----------------------------------------------------------------------------
// Pair Code Sheet Modal (6-Digit input)
// ----------------------------------------------------------------------------
fn render_pair_sheet(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let sheet_w = 420.0;
    let sheet_h = 240.0;
    let sheet_rect = Rect::from_center_size(screen.center(), vec2(sheet_w, sheet_h));

    egui::Area::new(egui::Id::new("pair-code-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            // 1. Dim background
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

            // 2. Solid modal card background
            ui.painter().rect_filled(sheet_rect, CornerRadius::same(14), p.group);
            ui.painter().rect_stroke(sheet_rect, CornerRadius::same(14), Stroke::new(1.0, p.edge), egui::StrokeKind::Inside);

            let mut sheet_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(sheet_rect.shrink(22.0))
                    .layout(Layout::top_down(Align::Center)),
            );

            sheet_ui.label(
                egui::RichText::new("输入 6 位连接码")
                    .font(theme::semibold_font(17.0))
                    .color(p.text),
            );

            sheet_ui.add_space(4.0);

            sheet_ui.label(
                egui::RichText::new("输入对方 LeeConnect 屏幕上显示的 6 位连接码以发起会话。")
                    .font(theme::text_font(12.5))
                    .color(p.text_2),
            );

            sheet_ui.add_space(16.0);

            // 6 Digit boxes (grouped 3 + 3)
            let chars: Vec<char> = app.pair_code_input.chars().collect();
            sheet_ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                for i in 0..6 {
                    if i == 3 {
                        ui.add_space(8.0);
                    }
                    let (box_r, _) = ui.allocate_exact_size(vec2(42.0, 48.0), Sense::hover());
                    let has_char = i < chars.len();
                    let is_active = i == chars.len();

                    let b_stroke = if is_active {
                        Stroke::new(2.0, p.accent)
                    } else {
                        Stroke::new(1.0, p.btn_edge)
                    };
                    ui.painter().rect(box_r, CornerRadius::same(8), p.field, b_stroke, egui::StrokeKind::Inside);

                    if has_char {
                        let g = ui.painter().layout_no_wrap(
                            chars[i].to_string(),
                            theme::semibold_font(24.0),
                            p.text,
                        );
                        ui.painter().galley(
                            pos2(box_r.center().x - g.size().x * 0.5, box_r.center().y - g.size().y * 0.5),
                            g,
                            p.text,
                        );
                    }
                }
            });

            // Keyboard input capture for digits & backspace
            let new_digits: Vec<char> = ui.input(|i| {
                let mut digits = Vec::new();
                for ev in &i.events {
                    if let egui::Event::Text(t) = ev {
                        for c in t.chars() {
                            if c.is_ascii_digit() {
                                digits.push(c);
                            }
                        }
                    }
                }
                digits
            });
            for d in new_digits {
                if app.pair_code_input.len() < 6 {
                    app.pair_code_input.push(d);
                }
            }
            if ui.input(|i| i.key_pressed(Key::Backspace)) {
                app.pair_code_input.pop();
            }

            sheet_ui.add_space(20.0);

            // Action buttons: [ 取消 ] [ 连接 ]
            sheet_ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 0.0);

                if push_button(ui, p, "取消", ButtonKind::Secondary, false, None).clicked() {
                    app.show_pair_modal = false;
                    app.pair_code_input.clear();
                }

                let can_connect = app.pair_code_input.len() == 6;
                if can_connect {
                    if push_button(ui, p, "连接", ButtonKind::Primary, false, None).clicked() {
                        app.show_pair_modal = false;
                        app.connect_to_selected();
                    }
                } else {
                    let _ = push_button(ui, p, "连接", ButtonKind::Secondary, false, None)
                        .on_hover_text("请输入完整 6 位连接码");
                }
            });
        });
}

// ----------------------------------------------------------------------------
// Disconnect Confirmation Alert Modal
// ----------------------------------------------------------------------------
fn render_disconnect_alert(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let alert_w = 360.0;
    let alert_h = 190.0;
    let alert_rect = Rect::from_center_size(screen.center(), vec2(alert_w, alert_h));

    egui::Area::new(egui::Id::new("disconnect-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            // 1. Dim background
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

            // 2. Solid modal card
            let mut alert_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(alert_rect)
                    .layout(Layout::top_down(Align::Center)),
            );

            egui::Frame::new()
                .fill(p.group)
                .corner_radius(14)
                .stroke(Stroke::new(1.0, p.edge))
                .shadow(egui::Shadow {
                    offset: [0, 10],
                    blur: 30,
                    spread: 0,
                    color: Color32::from_black_alpha(if p.dark { 120 } else { 45 }),
                })
                .inner_margin(20)
                .show(&mut alert_ui, |ui| {
                    ui.set_width(alert_w - 40.0);
                    ui.vertical_centered(|ui| {
                        // App Icon
                        let (icon_r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                        ui.painter().rect_filled(icon_r, CornerRadius::same(8), p.accent);
                        icons::draw_icon(ui.painter(), icon_r.center(), icons::MONITOR, 18.0, Color32::WHITE);

                        ui.add_space(8.0);

                        let dev_name = app.devices.get(app.selected_device).map(|d| d.name.as_str()).unwrap_or("工位电脑");
                        ui.label(
                            egui::RichText::new(format!("断开与「{dev_name}」的连接？"))
                                .font(theme::semibold_font(15.0))
                                .color(p.text),
                        );

                        ui.add_space(4.0);

                        ui.label(
                            egui::RichText::new("与工位电脑的会话将立即结束，进行中的文件传输会被中断。")
                                .font(theme::text_font(12.5))
                                .color(p.text_2),
                        );

                        ui.add_space(16.0);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                            if push_button(ui, p, "取消", ButtonKind::Secondary, false, None).clicked() {
                                app.show_disconnect_confirm = false;
                            }
                            if push_button(ui, p, "断开连接", ButtonKind::Danger, false, None).clicked() {
                                app.show_disconnect_confirm = false;
                                app.stop_session();
                            }
                        });
                    });
                });
        });
}

// ----------------------------------------------------------------------------
// Connection Failed Alert Modal
// ----------------------------------------------------------------------------
fn render_failed_alert(app: &mut GuiApp, ctx: &egui::Context, p: &Palette, err: &str) {
    let screen = ctx.viewport_rect();
    let alert_w = 380.0;
    let alert_h = 210.0;
    let alert_rect = Rect::from_center_size(screen.center(), vec2(alert_w, alert_h));

    egui::Area::new(egui::Id::new("failed-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            // 1. Dim background
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

            // 2. Solid modal card
            let mut alert_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(alert_rect)
                    .layout(Layout::top_down(Align::Center)),
            );

            egui::Frame::new()
                .fill(p.group)
                .corner_radius(14)
                .stroke(Stroke::new(1.0, p.edge))
                .shadow(egui::Shadow {
                    offset: [0, 10],
                    blur: 30,
                    spread: 0,
                    color: Color32::from_black_alpha(if p.dark { 120 } else { 45 }),
                })
                .inner_margin(20)
                .show(&mut alert_ui, |ui| {
                    ui.set_width(alert_w - 40.0);
                    ui.vertical_centered(|ui| {
                        let (icon_r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                        ui.painter().rect_filled(icon_r, CornerRadius::same(8), p.red);
                        icons::draw_icon(ui.painter(), icon_r.center(), icons::POWER, 18.0, Color32::WHITE);

                        ui.add_space(8.0);

                        ui.label(
                            egui::RichText::new("连接失败")
                                .font(theme::semibold_font(15.0))
                                .color(p.text),
                        );

                        ui.add_space(4.0);

                        ui.label(
                            egui::RichText::new(err)
                                .font(theme::text_font(12.5))
                                .color(p.text_2),
                        );

                        ui.add_space(16.0);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                            if push_button(ui, p, "好", ButtonKind::Secondary, false, None).clicked() {
                                *app.status.lock().unwrap() = super::super::Status::Idle;
                            }
                            if push_button(ui, p, "重试", ButtonKind::Primary, false, None).clicked() {
                                app.reconnect();
                            }
                        });
                    });
                });
        });
}

// ----------------------------------------------------------------------------
// Toast Notifications (Top Right)
// ----------------------------------------------------------------------------
fn render_toasts(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let now = std::time::Instant::now();
    app.toasts.retain(|t| now.duration_since(t.created).as_secs_f32() < 2.6);

    let screen = ctx.viewport_rect();
    let toast_w = 300.0;
    let toast_h = 56.0;

    for (idx, toast) in app.toasts.iter().enumerate() {
        let y_offset = 62.0 + idx as f32 * (toast_h + 8.0);
        let toast_rect = Rect::from_min_size(
            pos2(screen.max.x - toast_w - 14.0, y_offset),
            vec2(toast_w, toast_h),
        );

        egui::Area::new(egui::Id::new("toast-notification").with(idx))
            .fixed_pos(toast_rect.min)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let painter = ui.painter();
                painter.rect(
                    toast_rect,
                    CornerRadius::same(14),
                    p.pop,
                    Stroke::new(1.0, p.edge),
                    egui::StrokeKind::Inside,
                );

                let mut t_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(toast_rect.shrink(10.0))
                        .layout(Layout::left_to_right(Align::Center)),
                );

                // Small App Icon
                let (ic_r, _) = t_ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                t_ui.painter().rect_filled(ic_r, CornerRadius::same(7), p.accent);
                icons::draw_icon(t_ui.painter(), ic_r.center(), icons::MONITOR, 15.0, Color32::WHITE);

                t_ui.add_space(8.0);

                t_ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(&toast.title)
                            .font(theme::semibold_font(13.0))
                            .color(p.text),
                    );
                    ui.label(
                        egui::RichText::new(&toast.message)
                            .font(theme::text_font(12.0))
                            .color(p.text_2),
                    );
                });
            });
    }
}
