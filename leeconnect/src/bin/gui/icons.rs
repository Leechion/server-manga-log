use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, Response, RichText, Ui};
use std::sync::Arc;

pub const ICONS_FAMILY: &str = "lucide";

// Lucide icon codepoints (verified from lucide-static font info.json)
pub const PANEL_LEFT: char = '\u{e12a}';
pub const SEARCH: char = '\u{e151}';
pub const MONITOR: char = '\u{e11d}';
pub const PC_CASE: char = '\u{e446}';
pub const LAPTOP: char = '\u{e1cd}';
pub const PLUS: char = '\u{e13d}';
pub const INFO: char = '\u{e0f9}';
pub const MAXIMIZE: char = '\u{e112}';
pub const MAXIMIZE_2: char = '\u{e113}';
pub const MINIMIZE: char = '\u{e11a}';
pub const CLIPBOARD: char = '\u{e085}';
pub const FOLDER_UP: char = '\u{e33d}';
pub const EYE_OFF: char = '\u{e0bb}';
pub const ELLIPSIS: char = '\u{e0b6}';
pub const POWER: char = '\u{e140}';
pub const LOCK: char = '\u{e10b}';
pub const KEYBOARD: char = '\u{e284}';
pub const NETWORK: char = '\u{e125}';
pub const GAMEPAD_2: char = '\u{e0df}';
pub const CHECK: char = '\u{e06c}';
pub const COPY: char = '\u{e09e}';
pub const CHEVRONS_UP_DOWN: char = '\u{e211}';
pub const MOUSE_POINTER_2: char = '\u{e1c3}';
pub const X: char = '\u{e1b2}';
pub const SUN: char = '\u{e178}';
pub const MOON: char = '\u{e11e}';
pub const SCREEN_SHARE: char = '\u{e14f}';
pub const SHIELD: char = '\u{e158}';
pub const SLIDERS: char = '\u{e29a}';
pub const CHEVRON_RIGHT: char = '\u{e06f}';
pub const VOLUME_2: char = '\u{e1ab}';
pub const VOLUME_X: char = '\u{e1ac}';
pub const PENCIL: char = '\u{e1f9}';
pub const APP_WINDOW: char = '\u{e426}';
pub const ROTATE_CW: char = '\u{e149}';
pub const ZOOM_IN: char = '\u{e1b6}';

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(Arc::from(ICONS_FAMILY)))
}

pub fn icon_label(ui: &mut Ui, icon: char, size: f32, color: Color32) -> Response {
    ui.label(RichText::new(icon.to_string()).font(icon_font(size)).color(color))
}

pub fn draw_icon(painter: &Painter, pos: Pos2, icon: char, size: f32, color: Color32) {
    painter.text(pos, Align2::CENTER_CENTER, icon.to_string(), icon_font(size), color);
}
