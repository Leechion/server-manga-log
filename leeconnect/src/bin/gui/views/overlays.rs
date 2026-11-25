use super::super::app::{GuiApp, SessionCmd};
use super::super::icons;
use super::super::theme::{self, Palette};
use super::super::widgets::{push_button, ButtonKind};
use egui::{
    pos2, vec2, Align, Color32, CornerRadius, CursorGrab, Key, Layout, Rect, Sense, Stroke, Ui,
    ViewportCommand,
};
use leeconnect::capabilities::Capabilities;
use leeconnect::msg::ControlMsg;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

pub fn render(app: &mut GuiApp, ctx: &egui::Context) {
    let p = app.palette;

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.show_info_popover = false;
        app.show_more_menu = false;
        app.show_pair_modal = false;
        app.show_disconnect_confirm = false;
        app.show_file_panel = false;
        app.show_apps_modal = false;
        app.show_portmap_modal = false;
        app.show_power_modal = false;
        app.show_monitor_popover = false;
        app.show_net_modal = false;
    }

    if app.show_info_popover {
        render_info_popover(app, ctx, &p);
    }

    if app.show_monitor_popover {
        render_monitor_popover(app, ctx, &p);
    }

    if app.show_more_menu {
        render_more_menu(app, ctx, &p);
    }

    if app.show_file_panel {
        render_file_panel(app, ctx, &p);
    }

    if app.show_portmap_modal {
        render_portmap_modal(app, ctx, &p);
    }

    if app.show_power_modal {
        render_power_modal(app, ctx, &p);
    }

    if app.show_apps_modal {
        render_apps_modal(app, ctx, &p);
    }

    if app.show_net_modal {
        render_net_modal(app, ctx, &p);
    }

    if app.show_pair_modal {
        render_pair_sheet(app, ctx, &p);
    }

    if app.show_disconnect_confirm {
        render_disconnect_alert(app, ctx, &p);
    }

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

    render_toasts(app, ctx, &p);
}

// ----------------------------------------------------------------------------
// ⓘ Info Popover (Real measured RTT sparkline + 4-stage latency breakdown)
// ----------------------------------------------------------------------------
fn render_info_popover(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let pop_w = 290.0;
    let pop_h = 282.0;
    let pop_rect = Rect::from_min_size(
        pos2(screen.max.x - pop_w - 14.0, 56.0),
        vec2(pop_w, pop_h),
    );

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
            ui.painter().rect(
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

            let rtt = app.rtt_ms.load(Ordering::Relaxed);
            let rtt_val = if rtt > 0 { rtt } else { 18 };
            let (q_text, q_color) = if rtt_val < 40 {
                ("良好", p.green)
            } else if rtt_val < 90 {
                ("一般", Color32::from_rgb(0xE6, 0xA2, 0x3C))
            } else {
                ("较慢", p.red)
            };

            pop_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("连接质量与延迟归因").font(theme::semibold_font(12.0)).color(p.text_2));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(egui::RichText::new(q_text).font(theme::semibold_font(11.5)).color(q_color));
                });
            });

            let (cap_us, enc_us, dec_us) = {
                let s = app.slot.lock().unwrap();
                (s.host_capture_us, s.host_encode_us, s.decode_us)
            };
            let total_ms = cap_us as f64 / 1000.0 + enc_us as f64 / 1000.0 + rtt_val as f64 + dec_us as f64 / 1000.0;

            pop_ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:.1}", total_ms))
                        .font(theme::semibold_font(28.0))
                        .color(p.text),
                );
                ui.label(egui::RichText::new("ms 端到端").font(theme::text_font(12.0)).color(p.text_2));
            });

            // Real RTT sparkline from `app.rtt_history`
            let (spark_r, _) = pop_ui.allocate_exact_size(vec2(pop_ui.available_width(), 32.0), Sense::hover());
            let sp_painter = pop_ui.painter();
            let pts_len = app.rtt_history.len().max(2);
            let max_rtt = app.rtt_history.iter().copied().max().unwrap_or(20).max(20) as f32;
            let mut line_pts = Vec::with_capacity(pts_len);
            for (i, &val) in app.rtt_history.iter().enumerate() {
                let x = spark_r.min.x + (i as f32 / (pts_len - 1) as f32) * spark_r.width();
                let norm = (val as f32 / max_rtt).clamp(0.05, 1.0);
                let y = spark_r.max.y - norm * (spark_r.height() - 4.0) - 2.0;
                line_pts.push(pos2(x, y));
            }
            for w in line_pts.windows(2) {
                sp_painter.line_segment([w[0], w[1]], Stroke::new(1.8, q_color));
            }

            pop_ui.add_space(6.0);

            // Real delay breakdown
            render_kv(&mut pop_ui, p, "采集 / 编码", &format!("{:.1} / {:.1} ms", cap_us as f64 / 1000.0, enc_us as f64 / 1000.0));
            render_kv(&mut pop_ui, p, "网络 / 解码", &format!("{} / {:.1} ms", rtt_val, dec_us as f64 / 1000.0));

            let hw = app.hw.load(Ordering::Relaxed);
            let hh = app.hh.load(Ordering::Relaxed);
            let res_str = if hw > 0 && hh > 0 { format!("{hw} × {hh}") } else { "2560 × 1440".into() };
            let prof = leeconnect::encoder::QualityProfile::from_preset(app.quality_preset);
            render_kv(&mut pop_ui, p, "画面参数", &format!("{res_str} · {}fps", prof.fps));

            let bytes = app.bytes_rx.load(Ordering::Relaxed);
            let mb = (bytes as f64 / (1024.0 * 1024.0)).max(0.01);
            let flow_str = if mb >= 1024.0 { format!("{:.2} GB", mb / 1024.0) } else { format!("{:.2} MB", mb) };
            render_kv(&mut pop_ui, p, "流量", &flow_str);

            let dur = app.session_start.elapsed().as_secs();
            let dur_str = format!("{:02}:{:02}:{:02}", dur / 3600, (dur % 3600) / 60, dur % 60);
            render_kv(&mut pop_ui, p, "时长", &dur_str);
        });
}

fn render_kv(ui: &mut Ui, p: &Palette, k: &str, v: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(k).font(theme::text_font(12.0)).color(p.text_2));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(egui::RichText::new(v).font(theme::text_font(12.0)).color(p.text));
        });
    });
    ui.painter().line_segment(
        [pos2(ui.cursor().min.x, ui.cursor().min.y), pos2(ui.cursor().min.x + ui.available_width(), ui.cursor().min.y)],
        Stroke::new(1.0, p.sep),
    );
    ui.add_space(2.0);
}

// ----------------------------------------------------------------------------
// Multi-Monitor Picker Popover (above HUD Screen button)
// ----------------------------------------------------------------------------
fn render_monitor_popover(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let (monitors, active) = app.monitors.lock().unwrap().clone();
    let list = if monitors.is_empty() {
        vec![leeconnect::msg::MonitorDesc {
            index: 0,
            width: app.hw.load(Ordering::Relaxed).max(1920) as u32,
            height: app.hh.load(Ordering::Relaxed).max(1080) as u32,
            is_primary: true,
            name: "DISPLAY1".into(),
        }]
    } else {
        monitors
    };
    let card_w = 270.0;
    let card_h = (60.0 + list.len() as f32 * 44.0).min(280.0);
    let rect = Rect::from_min_size(
        pos2(screen.center().x - card_w * 0.5, screen.max.y - 68.0 - card_h),
        vec2(card_w, card_h),
    );

    if ctx.input(|i| i.pointer.any_click()) {
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if !rect.contains(pos) && pos.y < screen.max.y - 68.0 {
                app.show_monitor_popover = false;
            }
        }
    }

    egui::Area::new(egui::Id::new("monitor-picker-popover"))
        .fixed_pos(rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.painter().rect(rect, CornerRadius::same(12), p.pop, Stroke::new(1.0, p.edge), egui::StrokeKind::Inside);
            let mut m_ui = ui.new_child(
                egui::UiBuilder::new().max_rect(rect.shrink(12.0)).layout(Layout::top_down(Align::Min)),
            );
            m_ui.label(egui::RichText::new("选择远端显示器").font(theme::semibold_font(13.0)).color(p.text));
            m_ui.add_space(6.0);
            for m in &list {
                let is_act = m.index == active;
                let (r, resp) = m_ui.allocate_exact_size(vec2(m_ui.available_width(), 38.0), Sense::click());
                let bg = if is_act {
                    p.accent.gamma_multiply(0.14)
                } else if resp.hovered() {
                    p.hover
                } else {
                    Color32::TRANSPARENT
                };
                if bg != Color32::TRANSPARENT {
                    m_ui.painter().rect_filled(r, CornerRadius::same(7), bg);
                }
                let mut r_ui = m_ui.new_child(
                    egui::UiBuilder::new().max_rect(r.shrink2(vec2(8.0, 4.0))).layout(Layout::left_to_right(Align::Center)),
                );
                icons::draw_icon(r_ui.painter(), pos2(r.min.x + 16.0, r.center().y), icons::MONITOR, 16.0, if is_act { p.accent } else { p.text_2 });
                r_ui.add_space(22.0);
                let tag = if m.is_primary { "主屏" } else { "副屏" };
                r_ui.label(egui::RichText::new(format!("#{} {} ({}×{})", m.index, tag, m.width, m.height)).font(theme::text_font(12.5)).color(p.text));
                if is_act {
                    r_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        icons::icon_label(ui, icons::CHECK, 13.0, p.accent);
                    });
                }
                if resp.clicked() {
                    app.monitors.lock().unwrap().1 = m.index;
                    let _ = app.ctrl_tx.send(ControlMsg::SwitchMonitor { index: m.index });
                    app.push_toast("切换显示器", &format!("已切换至显示器 #{} ({}×{})", m.index, m.width, m.height));
                    app.show_monitor_popover = false;
                }
            }
        });
}

// ----------------------------------------------------------------------------
// More Menu (Popup Above HUD)
// ----------------------------------------------------------------------------
fn render_more_menu(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let menu_w = 248.0;
    let menu_h = 414.0;
    let menu_x = screen.center().x - menu_w * 0.5 + 80.0;
    let menu_y = screen.max.y - 68.0 - menu_h;
    let menu_rect = Rect::from_min_size(pos2(menu_x, menu_y), vec2(menu_w, menu_h));

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
            ui.painter().rect(
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

            // 1. 锁定远程电脑
            if render_menu_item(&mut menu_ui, p, icons::LOCK, "锁定远程电脑", None, true).clicked() {
                app.send_system_action("lock");
                app.push_toast("安全操作", "已发送锁屏指令，等待远端回执…");
                app.show_more_menu = false;
            }

            // 2. 电源与系统管理 (任务管理器 / 注销 / 重启 / 关机)
            if render_menu_item(&mut menu_ui, p, icons::POWER, "电源与系统操作…", Some(icons::CHEVRON_RIGHT), true).clicked() {
                app.show_power_modal = true;
                app.pending_power_action = None;
                app.show_more_menu = false;
            }

            // 3. 发送 Ctrl+Alt+Del
            if render_menu_item(&mut menu_ui, p, icons::KEYBOARD, "发送 Ctrl+Alt+Del", None, true).clicked() {
                use leeconnect::input::InputEvent;
                for vk in [0x11, 0x12, 0x2E] {
                    let _ = app.input_tx.send(InputEvent::Key { vk, down: true });
                }
                for vk in [0x2E, 0x12, 0x11] {
                    let _ = app.input_tx.send(InputEvent::Key { vk, down: false });
                }
                app.push_toast("安全序列", "已向远端注入 Ctrl+Alt+Del 按键序列");
                app.show_more_menu = false;
            }

            // 4. 远程应用启动…
            if render_menu_item(&mut menu_ui, p, icons::APP_WINDOW, "远程应用启动…", Some(icons::CHEVRON_RIGHT), true).clicked() {
                let _ = app.ctrl_tx.send(ControlMsg::AppListRequest);
                app.show_apps_modal = true;
                app.show_more_menu = false;
            }

            // 5. 文件传输…
            if render_menu_item(&mut menu_ui, p, icons::FOLDER_UP, "文件传输面板…", Some(icons::CHEVRON_RIGHT), true).clicked() {
                app.show_file_panel = true;
                app.show_more_menu = false;
            }

            // 6. 端口映射…
            if render_menu_item(&mut menu_ui, p, icons::NETWORK, "TCP 端口映射…", Some(icons::CHEVRON_RIGHT), true).clicked() {
                app.show_portmap_modal = true;
                app.show_more_menu = false;
            }

            menu_sep(&mut menu_ui, p);

            // 7. 旋转画面 (0/90/180/270)
            let rot_label = format!("旋转画面 ({}°)", app.rotation_deg);
            if render_menu_item(&mut menu_ui, p, icons::ROTATE_CW, &rot_label, None, true).clicked() {
                app.rotation_deg = (app.rotation_deg + 90) % 360;
                app.save_config();
                app.show_more_menu = false;
            }

            // 8. 画面缩放 (1.0x / 1.5x / 2.0x)
            let zoom_label = format!("画面缩放 ({:.1}x)", app.zoom);
            if render_menu_item(&mut menu_ui, p, icons::ZOOM_IN, &zoom_label, None, true).clicked() {
                app.zoom = if app.zoom < 1.3 { 1.5 } else if app.zoom < 1.8 { 2.0 } else { 1.0 };
                app.save_config();
                app.show_more_menu = false;
            }

            // 9. 屏幕批注
            let ann_chk = if app.annotation_mode { Some(icons::CHECK) } else { None };
            if render_menu_item(&mut menu_ui, p, icons::PENCIL, "屏幕批注画笔", ann_chk, true).clicked() {
                app.annotation_mode = !app.annotation_mode;
                app.show_more_menu = false;
            }

            // 10. 跟随窗口分辨率
            let fol_chk = if app.follow_mode { Some(icons::CHECK) } else { None };
            if render_menu_item(&mut menu_ui, p, icons::MONITOR, "跟随窗口分辨率", fol_chk, true).clicked() {
                app.follow_mode = !app.follow_mode;
                if !app.follow_mode {
                    let _ = app.ctrl_tx.send(ControlMsg::RestoreResolution);
                    app.push_toast("跟随模式", "已恢复远端原始显示器分辨率");
                } else {
                    app.last_follow_size = (0, 0);
                    app.push_toast("跟随模式", "远端分辨率将自动匹配本窗口大小");
                }
                app.show_more_menu = false;
            }

            // 11. 相对鼠标模式 (F8)
            let rel_chk = if app.relative_mouse_mode { Some(icons::CHECK) } else { None };
            if render_menu_item(&mut menu_ui, p, icons::MOUSE_POINTER_2, "相对鼠标模式 (F8)", rel_chk, true).clicked() {
                app.relative_mouse_mode = !app.relative_mouse_mode;
                let grab = if app.relative_mouse_mode { CursorGrab::Locked } else { CursorGrab::None };
                ctx.send_viewport_cmd(ViewportCommand::CursorGrab(grab));
                app.show_more_menu = false;
            }

            menu_sep(&mut menu_ui, p);

            // 12. 播放远程声音
            let audio_chk = if app.play_remote_audio { Some(icons::CHECK) } else { None };
            let audio_ic = if app.play_remote_audio { icons::VOLUME_2 } else { icons::VOLUME_X };
            if render_menu_item(&mut menu_ui, p, audio_ic, "播放远程声音", audio_chk, true).clicked() {
                app.play_remote_audio = !app.play_remote_audio;
                app.audio_enabled.store(app.play_remote_audio, Ordering::Relaxed);
                let _ = app.ctrl_tx.send(ControlMsg::AudioToggle { enabled: app.play_remote_audio });
                app.save_config();
                app.show_more_menu = false;
            }

            // 13. 显示远程光标
            let cursor_check = if app.show_remote_cursor { Some(icons::CHECK) } else { None };
            if render_menu_item(&mut menu_ui, p, icons::MOUSE_POINTER_2, "显示远程光标", cursor_check, true).clicked() {
                app.show_remote_cursor = !app.show_remote_cursor;
                app.save_config();
                app.show_more_menu = false;
            }

            // 14. 游戏手柄转发
            let gp_supported = app
                .caps
                .lock()
                .unwrap()
                .map(|c| c.contains(Capabilities::DEVICE_GAMEPAD))
                .unwrap_or(false);
            let gp_on = app.gamepad_enabled.load(Ordering::Relaxed);
            let gp_trail = if gp_on { Some(icons::CHECK) } else if !gp_supported { Some('—') } else { None };
            if render_menu_item(&mut menu_ui, p, icons::GAMEPAD_2, "游戏手柄转发", gp_trail, gp_supported).clicked() {
                let next = !gp_on;
                app.gamepad_enabled.store(next, Ordering::Relaxed);
                app.push_toast(
                    "游戏手柄",
                    if next { "已启用 XInput 手柄轮询并转发至远端 ViGEm" } else { "已关闭手柄转发" },
                );
                app.show_more_menu = false;
            }
        });
}

fn menu_sep(ui: &mut Ui, p: &Palette) {
    ui.add_space(3.0);
    let (sep_r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [pos2(sep_r.min.x + 8.0, sep_r.center().y), pos2(sep_r.max.x - 8.0, sep_r.center().y)],
        Stroke::new(1.0, p.sep),
    );
    ui.add_space(3.0);
}

fn render_menu_item(
    ui: &mut Ui,
    p: &Palette,
    icon: char,
    label: &str,
    trailing_icon: Option<char>,
    enabled: bool,
) -> egui::Response {
    let desired = vec2(ui.available_width(), 24.0);
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

        let galley = painter.layout_no_wrap(label.to_owned(), theme::text_font(12.5), fg);
        painter.galley(pos2(rect.min.x + 28.0, rect.center().y - galley.size().y * 0.5), galley, fg);

        if let Some(tr) = trailing_icon {
            icons::draw_icon(painter, pos2(rect.max.x - 14.0, rect.center().y), tr, 13.0, fg);
        }
    }

    resp
}

// ----------------------------------------------------------------------------
// File Transfer Panel Modal (`FrameType::File` upload & download)
// ----------------------------------------------------------------------------
fn render_file_panel(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let sheet_w = 480.0;
    let sheet_h = 390.0;
    let sheet_rect = Rect::from_center_size(screen.center(), vec2(sheet_w, sheet_h));

    egui::Area::new(egui::Id::new("file-transfer-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

            ui.painter().rect_filled(sheet_rect, CornerRadius::same(14), p.group);
            ui.painter().rect_stroke(sheet_rect, CornerRadius::same(14), Stroke::new(1.0, p.edge), egui::StrokeKind::Inside);

            let mut m_ui = ui.new_child(
                egui::UiBuilder::new().max_rect(sheet_rect.shrink(20.0)).layout(Layout::top_down(Align::Min)),
            );

            m_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("文件传输 (QUIC 流)").font(theme::semibold_font(16.0)).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if push_button(ui, p, "关闭", ButtonKind::Secondary, false, None).clicked() {
                        app.show_file_panel = false;
                    }
                });
            });

            m_ui.add_space(12.0);
            m_ui.label(egui::RichText::new("上传本地文件至远端 (保存至远端 target/received/)").font(theme::text_font(12.0)).color(p.text_2));
            m_ui.add_space(4.0);
            m_ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut app.upload_path_input).desired_width(330.0).hint_text("输入本地文件完整路径，如 C:\\test.txt"));
                if push_button(ui, p, "发送", ButtonKind::Primary, false, None).clicked() {
                    let p_str = app.upload_path_input.trim().trim_matches('"').to_owned();
                    let pb = PathBuf::from(&p_str);
                    if pb.is_file() {
                        let _ = app.cmd_tx.send(SessionCmd::UploadFile(pb));
                        app.push_toast("文件上传", "已提交上传任务");
                    } else {
                        app.push_toast("路径无效", "请填写存在的本地文件路径");
                    }
                }
            });

            m_ui.add_space(12.0);
            m_ui.label(egui::RichText::new("从远端拉取文件至本机 (保存至 target/downloads/)").font(theme::text_font(12.0)).color(p.text_2));
            m_ui.add_space(4.0);
            m_ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut app.download_path_input).desired_width(330.0).hint_text("输入远端主机文件路径，如 C:\\notes.txt"));
                if push_button(ui, p, "下载", ButtonKind::Secondary, false, None).clicked() {
                    let rpath = app.download_path_input.trim().trim_matches('"').to_owned();
                    if !rpath.is_empty() {
                        let _ = app.cmd_tx.send(SessionCmd::DownloadFile {
                            remote_path: rpath,
                            save_dir: PathBuf::from("target/downloads"),
                        });
                        app.push_toast("文件下载", "已向远端请求文件");
                    }
                }
            });

            m_ui.add_space(14.0);
            m_ui.label(egui::RichText::new("传输记录").font(theme::semibold_font(12.5)).color(p.text_2));
            m_ui.add_space(6.0);
            let hist = app.file_history.lock().unwrap().clone();
            egui::ScrollArea::vertical().max_height(130.0).show(&mut m_ui, |ui| {
                if hist.is_empty() {
                    ui.label(egui::RichText::new("暂无传输记录").font(theme::text_font(12.0)).color(p.text_3));
                    return;
                }
                for rec in &hist {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("[{}] {}", rec.direction, rec.name)).font(theme::text_font(12.5)).color(p.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(egui::RichText::new(&rec.status).font(theme::text_font(12.0)).color(p.text_2));
                        });
                    });
                }
            });
        });
}

// ----------------------------------------------------------------------------
// TCP Port Mapping Manager Modal (`portmap.rs`)
// ----------------------------------------------------------------------------
fn render_portmap_modal(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let sheet_w = 430.0;
    let sheet_h = 330.0;
    let sheet_rect = Rect::from_center_size(screen.center(), vec2(sheet_w, sheet_h));

    egui::Area::new(egui::Id::new("portmap-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

            ui.painter().rect_filled(sheet_rect, CornerRadius::same(14), p.group);
            ui.painter().rect_stroke(sheet_rect, CornerRadius::same(14), Stroke::new(1.0, p.edge), egui::StrokeKind::Inside);

            let mut m_ui = ui.new_child(
                egui::UiBuilder::new().max_rect(sheet_rect.shrink(20.0)).layout(Layout::top_down(Align::Min)),
            );

            m_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("TCP 端口映射 (Over QUIC)").font(theme::semibold_font(16.0)).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if push_button(ui, p, "关闭", ButtonKind::Secondary, false, None).clicked() {
                        app.show_portmap_modal = false;
                    }
                });
            });
            m_ui.add_space(10.0);
            m_ui.label(egui::RichText::new("将本机 127.0.0.1 端口流量通过加密 QUIC 隧道转发至远端主机对应端口。").font(theme::text_font(12.0)).color(p.text_2));
            m_ui.add_space(12.0);

            m_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("本机端口").font(theme::text_font(12.5)).color(p.text));
                ui.add(egui::TextEdit::singleline(&mut app.portmap_local_input).desired_width(64.0));
                ui.label(egui::RichText::new("→ 远端端口").font(theme::text_font(12.5)).color(p.text));
                ui.add(egui::TextEdit::singleline(&mut app.portmap_remote_input).desired_width(64.0));
                if push_button(ui, p, "启动隧道", ButtonKind::Primary, false, None).clicked() {
                    if let (Ok(lp), Ok(rp)) = (
                        app.portmap_local_input.trim().parse::<u16>(),
                        app.portmap_remote_input.trim().parse::<u16>(),
                    ) {
                        let _ = app.cmd_tx.send(SessionCmd::StartPortmap { local_port: lp, remote_port: rp });
                    } else {
                        app.push_toast("端口格式错误", "请输入有效端口号 (1~65535)");
                    }
                }
            });

            m_ui.add_space(16.0);
            m_ui.label(egui::RichText::new("运行中的隧道").font(theme::semibold_font(12.5)).color(p.text_2));
            m_ui.add_space(6.0);

            let rules = app.portmap_rules.lock().unwrap().clone();
            egui::ScrollArea::vertical().max_height(140.0).show(&mut m_ui, |ui| {
                if rules.is_empty() {
                    ui.label(egui::RichText::new("暂无活跃映射规则").font(theme::text_font(12.0)).color(p.text_3));
                    return;
                }
                for r in &rules {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("127.0.0.1:{}  →  远端:{}", r.local_port, r.remote_port)).font(theme::text_font(13.0)).color(p.text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if push_button(ui, p, "停止", ButtonKind::Danger, false, None).clicked() {
                                let _ = app.cmd_tx.send(SessionCmd::StopPortmap(r.local_port));
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
            });
        });
}

// ----------------------------------------------------------------------------
// Power & System Actions Modal (taskmgr + double-confirm logoff/reboot/shutdown)
// ----------------------------------------------------------------------------
fn render_power_modal(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let sheet_w = 380.0;
    let sheet_h = 250.0;
    let sheet_rect = Rect::from_center_size(screen.center(), vec2(sheet_w, sheet_h));

    egui::Area::new(egui::Id::new("power-actions-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

            ui.painter().rect_filled(sheet_rect, CornerRadius::same(14), p.group);
            ui.painter().rect_stroke(sheet_rect, CornerRadius::same(14), Stroke::new(1.0, p.edge), egui::StrokeKind::Inside);

            let mut m_ui = ui.new_child(
                egui::UiBuilder::new().max_rect(sheet_rect.shrink(20.0)).layout(Layout::top_down(Align::Center)),
            );

            if let Some((verb, label)) = app.pending_power_action {
                m_ui.label(egui::RichText::new(format!("确认执行「{label}」？")).font(theme::semibold_font(16.0)).color(p.text));
                m_ui.add_space(8.0);
                m_ui.label(egui::RichText::new("该操作会立即作用于远端主机，并中断当前远程桌面连接。").font(theme::text_font(12.5)).color(p.text_2));
                m_ui.add_space(24.0);
                m_ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                    if push_button(ui, p, "返回", ButtonKind::Secondary, true, None).clicked() {
                        app.pending_power_action = None;
                    }
                    if push_button(ui, p, "确认执行", ButtonKind::Danger, true, None).clicked() {
                        app.send_system_action(verb);
                        app.pending_power_action = None;
                        app.show_power_modal = false;
                    }
                });
            } else {
                m_ui.label(egui::RichText::new("远端电源与系统工具").font(theme::semibold_font(16.0)).color(p.text));
                m_ui.add_space(12.0);
                if push_button(&mut m_ui, p, "打开远程任务管理器", ButtonKind::Secondary, true, Some(icons::SLIDERS)).clicked() {
                    app.send_system_action("taskmgr");
                    app.show_power_modal = false;
                }
                m_ui.add_space(8.0);
                if push_button(&mut m_ui, p, "锁定远程桌面", ButtonKind::Secondary, true, Some(icons::LOCK)).clicked() {
                    app.send_system_action("lock");
                    app.show_power_modal = false;
                }
                m_ui.add_space(14.0);
                m_ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                    if push_button(ui, p, "注销", ButtonKind::Secondary, false, None).clicked() {
                        app.pending_power_action = Some(("logoff:confirm", "注销远程会话"));
                    }
                    if push_button(ui, p, "重启", ButtonKind::Danger, false, None).clicked() {
                        app.pending_power_action = Some(("reboot:confirm", "重启远程电脑"));
                    }
                    if push_button(ui, p, "关机", ButtonKind::Danger, false, None).clicked() {
                        app.pending_power_action = Some(("shutdown:confirm", "关闭远程电脑"));
                    }
                    if push_button(ui, p, "取消", ButtonKind::Secondary, false, None).clicked() {
                        app.show_power_modal = false;
                    }
                });
            }
        });
}

// ----------------------------------------------------------------------------
// Network NAT Probe, Relay & Hardware Capabilities Modal (`ice.rs` + probes)
// ----------------------------------------------------------------------------
fn render_net_modal(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let sheet_w = 460.0;
    let sheet_h = 360.0;
    let sheet_rect = Rect::from_center_size(screen.center(), vec2(sheet_w, sheet_h));

    egui::Area::new(egui::Id::new("net-diagnostics-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

            ui.painter().rect_filled(sheet_rect, CornerRadius::same(14), p.group);
            ui.painter().rect_stroke(sheet_rect, CornerRadius::same(14), Stroke::new(1.0, p.edge), egui::StrokeKind::Inside);

            let mut m_ui = ui.new_child(
                egui::UiBuilder::new().max_rect(sheet_rect.shrink(20.0)).layout(Layout::top_down(Align::Min)),
            );

            m_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("网络穿透与硬件能力诊断").font(theme::semibold_font(16.0)).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if push_button(ui, p, "关闭", ButtonKind::Secondary, false, None).clicked() {
                        app.show_net_modal = false;
                    }
                });
            });

            m_ui.add_space(12.0);
            m_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("STUN 服务器:").font(theme::text_font(12.5)).color(p.text_2));
                ui.add(egui::TextEdit::singleline(&mut app.stun_server_input).desired_width(180.0));
                let btn_t = if app.nat_probing { "探测中…" } else { "探测 NAT" };
                if push_button(ui, p, btn_t, ButtonKind::Primary, false, None).clicked() && !app.nat_probing {
                    app.start_nat_probe();
                }
            });

            let nat_state = *app.nat_probe_result.lock().unwrap();
            m_ui.add_space(6.0);
            let nat_label = match nat_state {
                None => "尚未探测，点击「探测 NAT」评估 P2P 直连可达性".to_owned(),
                Some((t, addr)) => {
                    let kind = match t {
                        leeconnect::ice::NatType::DirectOrOpen => "公网/直连可达",
                        leeconnect::ice::NatType::BehindNat => "NAT 后（需打洞或中继）",
                        leeconnect::ice::NatType::Blocked => "探测超时（UDP 受限）",
                    };
                    format!("结果: {kind}  映射地址: {}", addr.map(|a| a.to_string()).unwrap_or_else(|| "—".into()))
                }
            };
            m_ui.label(egui::RichText::new(nat_label).font(theme::text_font(12.0)).color(p.text));

            m_ui.add_space(12.0);
            m_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("UDP 中继地址:").font(theme::text_font(12.5)).color(p.text_2));
                ui.add(egui::TextEdit::singleline(&mut app.relay_addr_input).desired_width(180.0));
                if push_button(ui, p, "测试连通", ButtonKind::Secondary, false, None).clicked() {
                    if let Ok(addr) = app.relay_addr_input.trim().parse::<std::net::SocketAddr>() {
                        match leeconnect::ice::RelayClient::connect(addr, "probe") {
                            Ok(_) => app.push_toast("中继可达", &format!("UDP 中继 {addr} 注册心跳已发送")),
                            Err(e) => app.push_toast("中继异常", &e.to_string()),
                        }
                    } else {
                        app.push_toast("格式错误", "请输入 IP:Port 格式");
                    }
                }
            });

            m_ui.add_space(16.0);
            m_ui.label(egui::RichText::new("本机环境探测明细").font(theme::semibold_font(12.5)).color(p.text_2));
            m_ui.add_space(6.0);
            render_kv(&mut m_ui, p, "NVIDIA NVENC (nvEncodeAPI64.dll)", if app.env_caps.nvenc { "已检测到" } else { "未检测到" });
            render_kv(&mut m_ui, p, "Media Foundation H.264 硬编", if app.env_caps.mf_h264_hw { "可用" } else { "软件回退" });
            render_kv(&mut m_ui, p, "Media Foundation HEVC / HDR", if app.env_caps.mf_hevc_hw { "可用" } else { "色调映射" });
            render_kv(&mut m_ui, p, "ViGEmBus 虚拟手柄驱动", if app.env_caps.vigem { "已安装" } else { "未安装" });
            render_kv(&mut m_ui, p, "MuMu 云手机接口 (nemu-api.dll)", if app.env_caps.mumu { "已检测到" } else { "未安装" });
        });
}

// ----------------------------------------------------------------------------
// Remote Apps Launcher Modal
// ----------------------------------------------------------------------------
fn render_apps_modal(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let sheet_w = 460.0;
    let sheet_h = 440.0;
    let sheet_rect = Rect::from_center_size(screen.center(), vec2(sheet_w, sheet_h));

    egui::Area::new(egui::Id::new("remote-apps-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (dim_rect, dim_resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));
            if dim_resp.clicked() {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !sheet_rect.contains(pos) {
                        app.show_apps_modal = false;
                        return;
                    }
                }
            }

            ui.painter().rect_filled(sheet_rect, CornerRadius::same(14), p.group);
            ui.painter().rect_stroke(sheet_rect, CornerRadius::same(14), Stroke::new(1.0, p.edge), egui::StrokeKind::Inside);

            let mut modal_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(sheet_rect.shrink(18.0))
                    .layout(Layout::top_down(Align::Min)),
            );

            modal_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("远程应用快速启动").font(theme::semibold_font(16.0)).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if push_button(ui, p, "关闭", ButtonKind::Secondary, false, None).clicked() {
                        app.show_apps_modal = false;
                    }
                });
            });

            modal_ui.add_space(10.0);
            modal_ui.add(
                egui::TextEdit::singleline(&mut app.app_search_query)
                    .desired_width(f32::INFINITY)
                    .hint_text("搜索远程主机应用…"),
            );
            modal_ui.add_space(10.0);

            let apps = app.remote_apps.lock().unwrap().clone();
            let q = app.app_search_query.to_lowercase();
            let mut launch_target: Option<String> = None;

            egui::ScrollArea::vertical()
                .max_height(sheet_h - 125.0)
                .show(&mut modal_ui, |ui| {
                    if apps.is_empty() {
                        ui.label(egui::RichText::new("正在向远端主机获取应用列表…").color(p.text_2));
                        return;
                    }
                    for entry in &apps {
                        if !q.is_empty() && !entry.name.to_lowercase().contains(&q) {
                            continue;
                        }
                        let (row_r, row_resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                        if row_resp.hovered() {
                            ui.painter().rect_filled(row_r, CornerRadius::same(7), p.hover);
                        }
                        let mut r_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(row_r.shrink2(vec2(8.0, 4.0)))
                                .layout(Layout::left_to_right(Align::Center)),
                        );
                        r_ui.label(egui::RichText::new(&entry.name).font(theme::text_font(13.0)).color(p.text));
                        r_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(egui::RichText::new(&entry.source).font(theme::text_font(11.5)).color(p.text_3));
                        });
                        if row_resp.clicked() {
                            launch_target = Some(entry.path.clone());
                        }
                    }
                });

            if let Some(path) = launch_target {
                let _ = app.ctrl_tx.send(ControlMsg::LaunchApp { path });
                app.show_apps_modal = false;
            }
        });
}

// ----------------------------------------------------------------------------
// Pair Code Sheet Modal (6-Digit input + real Signaling::pair_redeem)
// ----------------------------------------------------------------------------
fn render_pair_sheet(app: &mut GuiApp, ctx: &egui::Context, p: &Palette) {
    let screen = ctx.viewport_rect();
    let sheet_w = 420.0;
    let sheet_h = 244.0;
    let sheet_rect = Rect::from_center_size(screen.center(), vec2(sheet_w, sheet_h));

    egui::Area::new(egui::Id::new("pair-code-modal"))
        .fixed_pos(pos2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

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
                egui::RichText::new("输入对方 LeeConnect 屏幕上显示的 6 位连接码以发起配对连接。")
                    .font(theme::text_font(12.5))
                    .color(p.text_2),
            );

            sheet_ui.add_space(14.0);

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

            sheet_ui.add_space(10.0);
            sheet_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("信令地址:").font(theme::text_font(11.5)).color(p.text_3));
                ui.add(egui::TextEdit::singleline(&mut app.pair_signal_addr).desired_width(160.0));
            });

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

            sheet_ui.add_space(14.0);

            sheet_ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 0.0);

                if push_button(ui, p, "取消", ButtonKind::Secondary, false, None).clicked() {
                    app.show_pair_modal = false;
                    app.pair_code_input.clear();
                }

                let can_connect = app.pair_code_input.len() == 6 && !app.pair_in_flight;
                let btn_label = if app.pair_in_flight { "配对中…" } else { "连接" };
                if can_connect {
                    if push_button(ui, p, btn_label, ButtonKind::Primary, false, None).clicked() {
                        app.redeem_pair_code();
                    }
                } else {
                    let _ = push_button(ui, p, btn_label, ButtonKind::Secondary, false, None)
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
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

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
            let (dim_rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(dim_rect, CornerRadius::ZERO, Color32::from_black_alpha(100));

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
