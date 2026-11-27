use super::super::app::GuiApp;
use super::super::icons;
use super::super::theme;
use super::super::widgets::{dev_art, group, group_row, popup_button, push_button, switch, ButtonKind};
use egui::{vec2, Align, CornerRadius, Layout, Sense, Stroke, Ui};

pub fn render_body(app: &mut GuiApp, ui: &mut Ui) {
    let p = app.palette;
    let dev = match app.devices.get(app.selected_device) {
        Some(d) => d.clone(),
        None => return,
    };
    let is_self = dev.kind.contains("本机");
    let is_laptop = dev.name.to_lowercase().contains("laptop");
    let is_online = dev.online;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(28.0);

            let max_w = 540.0;
            let avail_w = ui.available_width();

            ui.allocate_ui_with_layout(
                vec2(avail_w, ui.available_height()),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.set_max_width(max_w);

                    // 1. Hero Section: Device Art + Name + Status + Connect Buttons
                    dev_art(ui, &p, is_laptop, !is_online);

                    ui.add_space(14.0);

                    ui.label(
                        egui::RichText::new(&dev.name)
                            .font(theme::semibold_font(22.0))
                            .color(p.text),
                    );

                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                        let dot_color = if is_online { p.green } else { p.text_3 };
                        let (d_rect, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                        ui.painter().circle_filled(d_rect.center(), 3.5, dot_color);

                        let status_str = if is_self {
                            "本机 · 正常运行 · Windows 11".to_owned()
                        } else if is_online {
                            format!("{} · Windows", dev.kind)
                        } else {
                            "离线 · 上次在线 11-21 14:02".to_owned()
                        };
                        ui.label(
                            egui::RichText::new(&status_str)
                                .font(theme::text_font(12.5))
                                .color(p.text_2),
                        );
                    });

                    ui.add_space(18.0);

                    if is_self {
                        let card_w = 320.0;
                        let card_h = 56.0;
                        let (c_rect, _) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::hover());
                        ui.painter().rect(
                            c_rect,
                            CornerRadius::same(12),
                            p.group,
                            Stroke::new(1.0, p.edge),
                            egui::StrokeKind::Inside,
                        );

                        let mut card_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(c_rect.shrink(12.0))
                                .layout(Layout::left_to_right(Align::Center)),
                        );

                        card_ui.vertical(|ui| {
                            ui.label(egui::RichText::new("本机连接码").font(theme::text_font(11.5)).color(p.text_2));
                            ui.label(egui::RichText::new("482 193").font(theme::semibold_font(18.0)).color(p.text));
                        });

                        card_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if push_button(ui, &p, "拷贝", ButtonKind::Secondary, false, Some(icons::COPY)).clicked() {
                                ui.ctx().copy_text("482 193".to_owned());
                                let _ = leeconnect::clipboard::write_clipboard_text("482 193");
                                app.push_toast("已拷贝", "本机连接码 482 193 已复制到系统剪贴板");
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(10.0, 0.0);

                            if push_button(ui, &p, "输入连接码…", ButtonKind::Secondary, true, Some(icons::KEYBOARD)).clicked() {
                                app.show_pair_modal = true;
                            }

                            if is_online {
                                if push_button(ui, &p, "连接", ButtonKind::Primary, true, None).clicked() {
                                    app.connect_to_selected();
                                }
                            } else {
                                let btn_resp = push_button(ui, &p, "设备离线", ButtonKind::Secondary, true, None);
                                let _ = btn_resp.on_hover_text("远端设备处于离线状态，请确认对方已开机并启动服务端");
                            }
                        });
                    }

                    ui.add_space(24.0);

                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.set_width(max_w);

                        // Group 1: 「设备信息」
                        ui.label(
                            egui::RichText::new("设备信息")
                                .font(theme::semibold_font(12.0))
                                .color(p.text_2),
                        );
                        ui.add_space(6.0);

                        group(ui, &p, |ui| {
                            ui.set_width(max_w);
                            let res_str = if is_laptop { "1920 × 1080" } else { "2560 × 1440" };
                            let last_str = if is_self { "当前" } else if is_online { "刚刚" } else { "11-21 14:02" };
                            let link_str = if is_self { "回环连接" } else if is_online { "局域网直连 · QUIC" } else { "—" };

                            group_row(ui, &p, "分辨率", false, |ui| {
                                ui.label(egui::RichText::new(res_str).font(theme::text_font(13.0)).color(p.text_2));
                            });
                            group_row(ui, &p, "上次连接", true, |ui| {
                                ui.label(egui::RichText::new(last_str).font(theme::text_font(13.0)).color(p.text_2));
                            });
                            group_row(ui, &p, "连接方式", true, |ui| {
                                ui.label(egui::RichText::new(link_str).font(theme::text_font(13.0)).color(p.text_2));
                            });
                        });

                        // Group 2: 「连接选项」 (Remote devices)
                        if !is_self {
                            ui.add_space(20.0);
                            ui.label(
                                egui::RichText::new("连接选项")
                                    .font(theme::semibold_font(12.0))
                                    .color(p.text_2),
                            );
                            ui.add_space(6.0);

                            group(ui, &p, |ui| {
                                ui.set_width(max_w);
                                group_row(ui, &p, "画质与色度档位", false, |ui| {
                                    let label = match app.quality_preset {
                                        0 => "流畅 · 30fps",
                                        1 => "均衡 · 60fps",
                                        _ => "清晰 · 90fps · 4:4:4",
                                    };
                                    if popup_button(ui, &p, label).clicked() {
                                        app.quality_preset = (app.quality_preset + 1) % 3;
                                        app.save_config();
                                    }
                                });

                                group_row(ui, &p, "优先硬件编码 (MF / NVENC)", true, |ui| {
                                    if switch(ui, &p, &mut app.prefer_hw_enc).changed() {
                                        app.save_config();
                                    }
                                });

                                group_row(ui, &p, "连接后跟随窗口分辨率", true, |ui| {
                                    if switch(ui, &p, &mut app.follow_mode).changed() {
                                        app.save_config();
                                    }
                                });

                                group_row(ui, &p, "播放远程声音", true, |ui| {
                                    if switch(ui, &p, &mut app.play_remote_audio).changed() {
                                        app.audio_enabled.store(app.play_remote_audio, std::sync::atomic::Ordering::Relaxed);
                                        app.save_config();
                                    }
                                });

                                group_row(ui, &p, "同步剪贴板", true, |ui| {
                                    if switch(ui, &p, &mut app.sync_clipboard).changed() {
                                        app.save_config();
                                    }
                                });
                            });
                        }

                        // Group 3: 「本机能力与网络诊断」
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("本机能力与网络穿透")
                                    .font(theme::semibold_font(12.0))
                                    .color(p.text_2),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.link(egui::RichText::new("STUN / 中继设置…").font(theme::text_font(12.0))).clicked() {
                                    app.show_net_modal = true;
                                }
                            });
                        });
                        ui.add_space(6.0);

                        group(ui, &p, |ui| {
                            ui.set_width(max_w);
                            let enc_str = if app.env_caps.nvenc {
                                "NVIDIA NVENC + MF 就绪"
                            } else if app.env_caps.mf_h264_hw {
                                "Media Foundation 硬编就绪"
                            } else {
                                "OpenH264 软编 + HDR 色调映射"
                            };
                            let gp_str = if app.env_caps.vigem {
                                "ViGEmBus + XInput 就绪"
                            } else {
                                "XInput 就绪（未装 ViGEmBus）"
                            };
                            group_row(ui, &p, "视频编解码能力", false, |ui| {
                                ui.label(egui::RichText::new(enc_str).font(theme::text_font(12.5)).color(p.text_2));
                            });
                            group_row(ui, &p, "虚拟手柄驱动", true, |ui| {
                                ui.label(egui::RichText::new(gp_str).font(theme::text_font(12.5)).color(p.text_2));
                            });
                        });

                        ui.add_space(36.0);
                    });
                },
            );
        });
}
