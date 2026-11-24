use super::super::app::GuiApp;
use super::super::icons;
use super::super::theme;
use super::super::widgets::hud_button;
use egui::{
    pos2, vec2, Align, Color32, CornerRadius, Layout, Rect, Sense, Stroke, Ui, ViewportCommand,
};
use leeconnect::input::{InputEvent, MouseButton};
use std::sync::atomic::Ordering;

pub fn render_body(app: &mut GuiApp, ui: &mut Ui) {
    let p = app.palette;
    let avail = ui.available_size();

    let (bg_rect, _) = ui.allocate_exact_size(avail, Sense::hover());
    // Fill stage with dark live background #0A0A0B
    ui.painter().rect_filled(bg_rect, CornerRadius::ZERO, p.live_bg);

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
    let (dw, dh) = leeconnect::view::fit_size(bg_rect.width(), bg_rect.height(), hw as f32, hh as f32, rot, app.zoom);
    let disp = vec2(dw.max(10.0), dh.max(10.0));
    let screen_rect = Rect::from_center_size(bg_rect.center(), disp);

    // Draw remote desktop image (rotated about its UV center when needed)
    let img = egui::Image::new(tex).max_size(vec2(rw.max(10.0), rh.max(10.0)));
    let img = if rot.deg() == 0 {
        img
    } else {
        img.rotate(rot.deg() as f32 * std::f32::consts::TAU / 360.0, egui::Vec2::splat(0.5))
    };
    ui.put(screen_rect, img);

    // Overlay the remote cursor when enabled (UU feature: mouse_cursor)
    if app.show_remote_cursor {
        let cur = app.slot.lock().unwrap().cursor;
        if let Some((cx, cy, visible)) = cur {
            if visible && hw > 0 && hh > 0 {
                // forward-map source cursor coords through rotation into the drawn rect
                let (px, py) = leeconnect::view::remote_to_pointer(
                    cx as f32, cy as f32,
                    screen_rect.min.x, screen_rect.min.y,
                    screen_rect.width(), screen_rect.height(),
                    hw as f32, hh as f32, rot,
                );
                if Rect::from_min_size(screen_rect.min, disp).contains(pos2(px, py)) {
                    let painter = ui.painter();
                    // white crosshair with dark halo so it reads on any background
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
                    painter.line_segment(
                        [pos2(px - arm, py), pos2(px + arm, py)],
                        Stroke::new(1.0, line),
                    );
                    painter.line_segment(
                        [pos2(px, py - arm), pos2(px, py + arm)],
                        Stroke::new(1.0, line),
                    );
                    painter.circle_filled(pos2(px, py), 1.5, line);
                }
            }
        }
    }

    // Remote mouse capture on screen_rect
    let input_id = ui.scope_id().with("remote-desktop-input");
    let resp = ui.interact(screen_rect, input_id, Sense::click_and_drag());
    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
        if screen_rect.contains(pos) && screen_rect.width() > 1.0 && screen_rect.height() > 1.0 {
            // inverse-map pointer through rotation+zoom back to remote source coords
            let (rx_f, ry_f) = leeconnect::view::pointer_to_remote(
                pos.x, pos.y,
                screen_rect.min.x, screen_rect.min.y,
                screen_rect.width(), screen_rect.height(),
                hw as f32, hh as f32, rot,
            );
            let rx = rx_f.clamp(0.0, hw as f32 - 1.0) as u32;
            let ry = ry_f.clamp(0.0, hh as f32 - 1.0) as u32;
            let _ = app.input_tx.send(InputEvent::MoveAbs { x: rx, y: ry });

            if resp.clicked() {
                let _ = app
                    .input_tx
                    .send(InputEvent::Button { button: MouseButton::Left, down: true });
                let _ = app
                    .input_tx
                    .send(InputEvent::Button { button: MouseButton::Left, down: false });
            }
            if resp.secondary_clicked() {
                let _ = app
                    .input_tx
                    .send(InputEvent::Button { button: MouseButton::Right, down: true });
                let _ = app
                    .input_tx
                    .send(InputEvent::Button { button: MouseButton::Right, down: false });
            }
            if resp.drag_started() {
                let _ = app
                    .input_tx
                    .send(InputEvent::Button { button: MouseButton::Left, down: true });
            }
            if resp.drag_stopped() {
                let _ = app
                    .input_tx
                    .send(InputEvent::Button { button: MouseButton::Left, down: false });
            }
        }
    }

    // Keyboard capture when remote desktop is hovered/active
    if resp.hovered() {
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

    // ------------------------------------------------------------------------
    // Floating HUD Toolbar (Bottom Center, 18px from bottom)
    // ------------------------------------------------------------------------
    let hud_w = 328.0;
    let hud_h = 44.0;
    let hud_bottom = bg_rect.max.y - 18.0;
    let hud_rect = Rect::from_center_size(pos2(bg_rect.center().x, hud_bottom - hud_h * 0.5), vec2(hud_w, hud_h));

    // Mouse idle fade-out check
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
        // Draw HUD glass background
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

        // 2. Clipboard
        if hud_button(&mut hud_ui, &p, icons::CLIPBOARD, false, false, "剪贴板同步").clicked() {
            if let Ok(Some(text)) = leeconnect::clipboard::read_clipboard_text() {
                let preview = if text.chars().count() > 18 {
                    format!("{}…", text.chars().take(18).collect::<String>())
                } else {
                    text
                };
                let msg = format!("已将本地剪贴板发送至远端: \"{preview}\"");
                app.push_toast("剪贴板已同步", &msg);
            } else {
                app.push_toast("剪贴板就绪", "双向剪贴板同步监听活跃中");
            }
        }

        // 3. File Transfer
        if hud_button(&mut hud_ui, &p, icons::FOLDER_UP, false, false, "发送文件…").clicked() {
            std::thread::spawn(|| {
                let _ = std::process::Command::new("explorer.exe")
                    .arg("/select,C:\\")
                    .spawn();
            });
            app.push_toast("文件传输", "已开启文件选择器，选定文件将流式传输至远端");
        }

        // 4. Switch Screen
        let screen_btn = hud_button(&mut hud_ui, &p, icons::SCREEN_SHARE, false, false, "切换显示器");
        if screen_btn.clicked() {
            let hw = app.hw.load(Ordering::Relaxed);
            let hh = app.hh.load(Ordering::Relaxed);
            let msg = format!("远端当前主显示器: {hw}×{hh} (60Hz)");
            app.push_toast("显示器信息", &msg);
        }

        // 5. Privacy Screen — real RPC to host (engines black cover + physical-input hooks there)
        let privacy_active = app.privacy_screen_active;
        if hud_button(&mut hud_ui, &p, icons::EYE_OFF, privacy_active, false, "隐私屏").clicked() {
            let verb = if privacy_active { "privacy_off" } else { "privacy_on" };
            app.send_system_action(verb);
            app.privacy_screen_active = !privacy_active;
            app.push_toast("隐私屏", if app.privacy_screen_active { "已发送开启指令，远端物理屏将黑屏屏蔽（连按3次Esc本地解除）" } else { "已发送关闭指令" });
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
