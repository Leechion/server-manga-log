use super::super::app::GuiApp;
use super::super::icons;
use super::super::theme;
use super::super::widgets::{hud_button, push_button, ButtonKind};
use egui::{
    pos2, vec2, Align, Color32, CornerRadius, CursorGrab, Key, Layout, Rect, Sense, Stroke, Ui,
    ViewportCommand,
};
use leeconnect::annotation::StrokePath;
use leeconnect::input::{InputEvent, MouseButton};
use leeconnect::msg::ControlMsg;
use super::super::app::SessionCmd;
use std::sync::atomic::Ordering;

pub fn render_body(app: &mut GuiApp, ui: &mut Ui) {
    let p = app.palette;
    let avail = ui.available_size();

    let (bg_rect, _) = ui.allocate_exact_size(avail, Sense::hover());
    ui.painter().rect_filled(bg_rect, CornerRadius::ZERO, p.live_bg);

    // Follow-mode automatic resolution sync (debounced)
    if app.follow_mode && app.last_follow_sent.elapsed().as_millis() >= 450 {
        let tw = (bg_rect.width().round() as u32).max(640);
        let th = (bg_rect.height().round() as u32).max(480);
        if (tw, th) != app.last_follow_size {
            app.last_follow_size = (tw, th);
            app.last_follow_sent = std::time::Instant::now();
            let _ = app.ctrl_tx.send(ControlMsg::SetResolution { width: tw, height: th });
        }
    }

    // F8 hotkey toggles relative mouse mode
    if ui.input(|i| i.key_pressed(Key::F8)) {
        app.relative_mouse_mode = !app.relative_mouse_mode;
        let grab = if app.relative_mouse_mode {
            CursorGrab::Locked
        } else {
            CursorGrab::None
        };
        ui.ctx().send_viewport_cmd(ViewportCommand::CursorGrab(grab));
        app.push_toast(
            "相对鼠标模式",
            if app.relative_mouse_mode {
                "已锁定鼠标用于游戏视角控制（按 F8 释放）"
            } else {
                "已恢复绝对坐标模式"
            },
        );
    }

    let hw = app.hw.load(Ordering::Relaxed) as usize;
    let hh = app.hh.load(Ordering::Relaxed) as usize;

    let tex = match &app.tex {
        Some(t) if hw > 0 && hh > 0 => t,
        _ => {
            let mut center_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bg_rect)
                    .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
            );
            center_ui.label(
                egui::RichText::new("正在等待远端视频流…")
                    .font(theme::text_font(14.0))
                    .color(Color32::from_white_alpha(160)),
            );
            return;
        }
    };

    // Aspect-fit video rectangle with rotation + zoom (UU: screen_orientation / screen_zoomscale)
    let rot = leeconnect::view::Rotation::from_deg(app.rotation_deg);
    let (rw, rh) = leeconnect::view::rotated_size(hw as f32, hh as f32, rot);
    let (dw, dh) =
        leeconnect::view::fit_size(bg_rect.width(), bg_rect.height(), hw as f32, hh as f32, rot, app.zoom);
    let disp = vec2(dw.max(10.0), dh.max(10.0));
    let screen_rect = Rect::from_center_size(bg_rect.center(), disp);

    let img = egui::Image::new(tex).max_size(vec2(rw.max(10.0), rh.max(10.0)));
    let img = if rot.deg() == 0 {
        img
    } else {
        img.rotate(rot.deg() as f32 * std::f32::consts::TAU / 360.0, egui::Vec2::splat(0.5))
    };
    ui.put(screen_rect, img);

    // Overlay existing & active annotation strokes (UU feature: annotation_v1 / v2)
    {
        let painter = ui.painter().with_clip_rect(screen_rect);
        let draw_stroke = |s: &StrokePath| {
            if s.points.len() < 2 {
                return;
            }
            let c = Color32::from_rgba_unmultiplied(s.color[0], s.color[1], s.color[2], s.color[3]);
            let pts: Vec<_> = s
                .points
                .iter()
                .map(|&[nx, ny]| {
                    pos2(
                        screen_rect.min.x + nx * screen_rect.width(),
                        screen_rect.min.y + ny * screen_rect.height(),
                    )
                })
                .collect();
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], Stroke::new(s.width_px, c));
            }
        };
        for s in &app.annotations {
            draw_stroke(s);
        }
        if let Some(s) = &app.active_stroke {
            draw_stroke(s);
        }
    }

    // Overlay the remote cursor when enabled (UU feature: mouse_cursor)
    if app.show_remote_cursor {
        let cur = app.slot.lock().unwrap().cursor;
        if let Some((cx, cy, visible)) = cur {
            if visible && hw > 0 && hh > 0 {
                let (px, py) = leeconnect::view::remote_to_pointer(
                    cx as f32,
                    cy as f32,
                    screen_rect.min.x,
                    screen_rect.min.y,
                    screen_rect.width(),
                    screen_rect.height(),
                    hw as f32,
                    hh as f32,
                    rot,
                );
                if Rect::from_min_size(screen_rect.min, disp).contains(pos2(px, py)) {
                    let painter = ui.painter();
                    let arm = 9.0;
                    let halo = Color32::from_black_alpha(180);
                    let line = Color32::WHITE;
                    for (dx, dy) in [(0.5, 0.5), (-0.5, -0.5)] {
                        painter.line_segment(
                            [pos2(px - arm + dx, py + dy), pos2(px + arm + dx, py + dy)],
                            Stroke::new(1.5, halo),
                        );
                        painter.line_segment(
                            [pos2(px + dx, py - arm + dy), pos2(px + dx, py + arm + dy)],
                            Stroke::new(1.5, halo),
                        );
                    }
                    painter.line_segment([pos2(px - arm, py), pos2(px + arm, py)], Stroke::new(1.0, line));
                    painter.line_segment([pos2(px, py - arm), pos2(px, py + arm)], Stroke::new(1.0, line));
                    painter.circle_filled(pos2(px, py), 1.5, line);
                }
            }
        }
    }

    // Remote input / annotation capture on screen_rect
    let input_id = ui.scope_id().with("remote-desktop-input");
    let resp = ui.interact(screen_rect, input_id, Sense::click_and_drag());

    if app.annotation_mode {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            if screen_rect.contains(pos) && screen_rect.width() > 1.0 && screen_rect.height() > 1.0 {
                let nx = ((pos.x - screen_rect.min.x) / screen_rect.width()).clamp(0.0, 1.0);
                let ny = ((pos.y - screen_rect.min.y) / screen_rect.height()).clamp(0.0, 1.0);
                if resp.drag_started() || (ui.input(|i| i.pointer.primary_pressed()) && app.active_stroke.is_none()) {
                    app.active_stroke = Some(StrokePath {
                        color: app.annotation_color,
                        width_px: 3.0,
                        points: vec![[nx, ny]],
                    });
                } else if ui.input(|i| i.pointer.primary_down()) {
                    if let Some(s) = app.active_stroke.as_mut() {
                        s.points.push([nx, ny]);
                    }
                }
            }
        }
        if resp.drag_stopped() || ui.input(|i| i.pointer.primary_released()) {
            if let Some(s) = app.active_stroke.take() {
                if s.points.len() >= 2 {
                    let _ = app.ctrl_tx.send(ControlMsg::AnnotationStroke {
                        color: s.color,
                        width_px: s.width_px as u8,
                        points: s.to_wire_points(),
                    });
                    app.annotations.push(s);
                }
            }
        }
    } else if app.relative_mouse_mode {
        // Relative game mouse mode: forward pointer deltas directly
        let delta = ui.input(|i| i.pointer.delta());
        if delta.x.abs() > 0.1 || delta.y.abs() > 0.1 {
            let _ = app.input_tx.send(InputEvent::MoveRel {
                dx: delta.x.round() as i32,
                dy: delta.y.round() as i32,
            });
        }
        if resp.clicked() {
            let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Left, down: true });
            let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Left, down: false });
        }
        if resp.secondary_clicked() {
            let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Right, down: true });
            let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Right, down: false });
        }
    } else if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
        if screen_rect.contains(pos) && screen_rect.width() > 1.0 && screen_rect.height() > 1.0 {
            let (rx_f, ry_f) = leeconnect::view::pointer_to_remote(
                pos.x,
                pos.y,
                screen_rect.min.x,
                screen_rect.min.y,
                screen_rect.width(),
                screen_rect.height(),
                hw as f32,
                hh as f32,
                rot,
            );
            let rx = rx_f.clamp(0.0, hw as f32 - 1.0) as u32;
            let ry = ry_f.clamp(0.0, hh as f32 - 1.0) as u32;
            let _ = app.input_tx.send(InputEvent::MoveAbs { x: rx, y: ry });

            if resp.clicked() {
                let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Left, down: true });
                let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Left, down: false });
            }
            if resp.secondary_clicked() {
                let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Right, down: true });
                let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Right, down: false });
            }
            if resp.drag_started() {
                let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Left, down: true });
            }
            if resp.drag_stopped() {
                let _ = app.input_tx.send(InputEvent::Button { button: MouseButton::Left, down: false });
            }
        }
    }

    // Mouse wheel & keyboard capture when remote desktop is hovered/active
    if resp.hovered() && !app.annotation_mode {
        let scroll = ui.input(|i| i.smooth_scroll_delta);
        if scroll.y.abs() > 0.5 {
            let _ = app.input_tx.send(InputEvent::Wheel {
                delta: (scroll.y * 3.0).round() as i32,
                horizontal: false,
            });
        }
        if scroll.x.abs() > 0.5 {
            let _ = app.input_tx.send(InputEvent::Wheel {
                delta: (scroll.x * 3.0).round() as i32,
                horizontal: true,
            });
        }

        let key_events: Vec<(u16, bool)> = ui.input(|i| {
            let mut out = Vec::new();
            for ev in &i.events {
                if let egui::Event::Key { key, pressed, .. } = ev {
                    if let Some(vk) = egui_key_to_vk(*key) {
                        out.push((vk, *pressed));
                    }
                }
            }
            out
        });
        for (vk, down) in key_events {
            let _ = app.input_tx.send(InputEvent::Key { vk, down });
        }
    }

    // Subtle status badge when relative mouse or follow mode is active
    if app.relative_mouse_mode || app.follow_mode {
        let label = match (app.relative_mouse_mode, app.follow_mode) {
            (true, true) => "相对鼠标已锁定 (F8 释放) · 跟随分辨率",
            (true, false) => "相对鼠标已锁定 (按 F8 释放)",
            (false, true) => "跟随窗口分辨率已开启",
            _ => "",
        };
        let badge_r = Rect::from_min_size(pos2(bg_rect.min.x + 12.0, bg_rect.min.y + 10.0), vec2(236.0, 24.0));
        ui.painter().rect_filled(badge_r, CornerRadius::same(7), Color32::from_black_alpha(150));
        let g = ui.painter().layout_no_wrap(label.to_owned(), theme::text_font(11.5), Color32::WHITE);
        ui.painter().galley(pos2(badge_r.min.x + 10.0, badge_r.center().y - g.size().y * 0.5), g, Color32::WHITE);
    }

    // ------------------------------------------------------------------------
    // Floating Annotation Mini Toolbar (when annotation_mode is active)
    // ------------------------------------------------------------------------
    if app.annotation_mode {
        let bar_w = 276.0;
        let bar_h = 36.0;
        let bar_rect = Rect::from_center_size(pos2(bg_rect.center().x, bg_rect.min.y + 28.0), vec2(bar_w, bar_h));
        ui.painter().rect(
            bar_rect,
            CornerRadius::same(10),
            p.hud,
            Stroke::new(1.0, Color32::from_white_alpha(30)),
            egui::StrokeKind::Inside,
        );
        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar_rect.shrink(6.0))
                .layout(Layout::left_to_right(Align::Center)),
        );
        bar_ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
        for rgba in [
            [255u8, 59, 48, 255],
            [10, 132, 255, 255],
            [52, 199, 89, 255],
            [255, 214, 10, 255],
        ] {
            let (cr, cresp) = bar_ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
            let c = Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
            bar_ui.painter().circle_filled(cr.center(), 7.0, c);
            if app.annotation_color == rgba {
                bar_ui.painter().circle_stroke(cr.center(), 9.0, Stroke::new(1.5, Color32::WHITE));
            }
            if cresp.clicked() {
                app.annotation_color = rgba;
            }
        }
        if push_button(&mut bar_ui, &p, "清空", ButtonKind::Secondary, false, None).clicked() {
            app.annotations.clear();
            let _ = app.ctrl_tx.send(ControlMsg::AnnotationClear);
        }
        if push_button(&mut bar_ui, &p, "完成", ButtonKind::Primary, false, None).clicked() {
            app.annotation_mode = false;
        }
    }

    // ------------------------------------------------------------------------
    // Floating HUD Toolbar (Bottom Center, 18px from bottom)
    // ------------------------------------------------------------------------
    let hud_w = 328.0;
    let hud_h = 44.0;
    let hud_bottom = bg_rect.max.y - 18.0;
    let hud_rect = Rect::from_center_size(pos2(bg_rect.center().x, hud_bottom - hud_h * 0.5), vec2(hud_w, hud_h));

    let pointer_near_bottom = ui.input(|i| {
        i.pointer.hover_pos().map_or(false, |p| p.y >= bg_rect.max.y - 120.0)
    });
    let hud_hovered = ui.input(|i| {
        i.pointer.hover_pos().map_or(false, |p| hud_rect.expand(10.0).contains(p))
    });

    if pointer_near_bottom || hud_hovered || app.show_more_menu {
        app.last_mouse_activity = std::time::Instant::now();
    }
    let is_hud_visible = app.last_mouse_activity.elapsed().as_secs_f32() < 3.0;

    if is_hud_visible {
        let painter = ui.painter();
        painter.rect(
            hud_rect,
            CornerRadius::same(14),
            p.hud,
            Stroke::new(1.0, Color32::from_white_alpha(26)),
            egui::StrokeKind::Inside,
        );

        let mut hud_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(hud_rect.shrink(5.0))
                .layout(Layout::left_to_right(Align::Center)),
        );
        hud_ui.spacing_mut().item_spacing = vec2(2.0, 0.0);

        // 1. Fullscreen
        let is_fs = app.is_fullscreen;
        if hud_button(&mut hud_ui, &p, icons::MAXIMIZE, is_fs, false, "全屏 (Ctrl+Cmd+F)").clicked() {
            app.is_fullscreen = !app.is_fullscreen;
            ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(app.is_fullscreen));
        }

        // 2. Clipboard (real QUIC FrameType::Clipboard push)
        if hud_button(&mut hud_ui, &p, icons::CLIPBOARD, false, false, "剪贴板同步").clicked() {
            if let Ok(Some(text)) = leeconnect::clipboard::read_clipboard_text() {
                let _ = app.cmd_tx.send(SessionCmd::SendClipboard(text.clone()));
                let preview = if text.chars().count() > 18 {
                    format!("{}…", text.chars().take(18).collect::<String>())
                } else {
                    text
                };
                let msg = format!("已发送本地剪贴板至远端: \"{preview}\"");
                app.push_toast("剪贴板已同步", &msg);
            } else {
                app.push_toast("剪贴板就绪", "当前本地剪贴板为空或非文本");
            }
        }

        // 3. File Transfer panel
        if hud_button(&mut hud_ui, &p, icons::FOLDER_UP, app.show_file_panel, false, "文件传输…").clicked() {
            app.show_file_panel = !app.show_file_panel;
            app.show_monitor_popover = false;
            app.show_more_menu = false;
        }

        // 4. Switch Screen (visual monitor picker)
        let screen_btn = hud_button(&mut hud_ui, &p, icons::SCREEN_SHARE, app.show_monitor_popover, false, "切换显示器");
        if screen_btn.clicked() {
            app.show_monitor_popover = !app.show_monitor_popover;
            app.show_file_panel = false;
            app.show_more_menu = false;
        }

        // 5. Privacy Screen — real RPC to host
        let privacy_active = app.privacy_screen_active;
        if hud_button(&mut hud_ui, &p, icons::EYE_OFF, privacy_active, false, "隐私屏").clicked() {
            let verb = if privacy_active { "privacy_off" } else { "privacy_on" };
            app.send_system_action(verb);
            app.privacy_screen_active = !privacy_active;
            app.push_toast(
                "隐私屏",
                if app.privacy_screen_active {
                    "已发送开启指令，远端物理屏将黑屏屏蔽（连按3次Esc本地解除）"
                } else {
                    "已发送关闭指令"
                },
            );
        }

        // 6. More Menu (…)
        if hud_button(&mut hud_ui, &p, icons::ELLIPSIS, app.show_more_menu, false, "更多操作…").clicked() {
            app.show_more_menu = !app.show_more_menu;
        }

        // 7. Divider Line
        let (sep_r, _) = hud_ui.allocate_exact_size(vec2(1.0, 20.0), Sense::hover());
        hud_ui.painter().line_segment(
            [pos2(sep_r.center().x, sep_r.min.y), pos2(sep_r.center().x, sep_r.max.y)],
            Stroke::new(1.0, Color32::from_white_alpha(35)),
        );

        // 8. Power / Disconnect
        if hud_button(&mut hud_ui, &p, icons::POWER, false, true, "断开连接").clicked() {
            app.show_disconnect_confirm = true;
        }
    }
}

fn egui_key_to_vk(k: egui::Key) -> Option<u16> {
    use egui::Key;
    match k {
        Key::A => Some(0x41), Key::B => Some(0x42), Key::C => Some(0x43),
        Key::D => Some(0x44), Key::E => Some(0x45), Key::F => Some(0x46),
        Key::G => Some(0x47), Key::H => Some(0x48), Key::I => Some(0x49),
        Key::J => Some(0x4A), Key::K => Some(0x4B), Key::L => Some(0x4C),
        Key::M => Some(0x4D), Key::N => Some(0x4E), Key::O => Some(0x4F),
        Key::P => Some(0x50), Key::Q => Some(0x51), Key::R => Some(0x52),
        Key::S => Some(0x53), Key::T => Some(0x54), Key::U => Some(0x55),
        Key::V => Some(0x56), Key::W => Some(0x57), Key::X => Some(0x58),
        Key::Y => Some(0x59), Key::Z => Some(0x5A),
        Key::Num0 => Some(0x30), Key::Num1 => Some(0x31), Key::Num2 => Some(0x32),
        Key::Num3 => Some(0x33), Key::Num4 => Some(0x34), Key::Num5 => Some(0x35),
        Key::Num6 => Some(0x36), Key::Num7 => Some(0x37), Key::Num8 => Some(0x38),
        Key::Num9 => Some(0x39),
        Key::Enter => Some(0x0D),
        Key::Escape => Some(0x1B),
        Key::Space => Some(0x20),
        Key::Backspace => Some(0x08),
        Key::Tab => Some(0x09),
        Key::Delete => Some(0x2E),
        Key::ArrowLeft => Some(0x25),
        Key::ArrowUp => Some(0x26),
        Key::ArrowRight => Some(0x27),
        Key::ArrowDown => Some(0x28),
        _ => None,
    }
}
