use egui::{
    Color32, Context, CornerRadius, FontFamily, FontId, Shadow, Stroke, Visuals,
};
use std::sync::Arc;

pub const FONT_SEMIBOLD: &str = "semibold";

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub content: Color32,
    pub side: Color32,
    pub group: Color32,
    pub edge: Color32,
    pub field: Color32,
    pub btn: Color32,
    pub btn_edge: Color32,
    pub text: Color32,
    pub text_2: Color32,
    pub text_3: Color32,
    pub sep: Color32,
    pub hover: Color32,
    pub sel: Color32,
    pub pop: Color32,
    pub accent: Color32,
    pub green: Color32,
    pub orange: Color32,
    pub red: Color32,
    pub live_bg: Color32,
    pub hud: Color32,
}

pub const fn rgb(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        (r as u16 * a as u16 / 255) as u8,
        (g as u16 * a as u16 / 255) as u8,
        (b as u16 * a as u16 / 255) as u8,
        a,
    )
}

pub const LIGHT: Palette = Palette {
    dark: false,
    content: rgb(0xF6F6F8),
    side: rgb(0xECECEF),
    group: rgb(0xFFFFFF),
    edge: rgba(0, 0, 0, 18),
    field: rgb(0xFFFFFF),
    btn: rgb(0xFFFFFF),
    btn_edge: rgba(0, 0, 0, 33),
    text: rgb(0x1D1D1F),
    text_2: rgba(60, 60, 67, 163),
    text_3: rgba(60, 60, 67, 97),
    sep: rgba(60, 60, 67, 31),
    hover: rgba(0, 0, 0, 11),
    sel: rgba(0, 0, 0, 19),
    pop: rgba(250, 250, 252, 235),
    accent: rgb(0x007AFF),
    green: rgb(0x34C759),
    orange: rgb(0xFF9F0A),
    red: rgb(0xFF3B30),
    live_bg: rgb(0x0A0A0B),
    hud: rgba(32, 32, 36, 190),
};

pub const DARK: Palette = Palette {
    dark: true,
    content: rgb(0x1F1F21),
    side: rgb(0x2A2A2D),
    group: rgb(0x2B2B2E),
    edge: rgba(255, 255, 255, 15),
    field: rgb(0x1C1C1E),
    btn: rgb(0x3A3A3D),
    btn_edge: rgba(255, 255, 255, 20),
    text: rgb(0xF5F5F7),
    text_2: rgba(235, 235, 245, 158),
    text_3: rgba(235, 235, 245, 82),
    sep: rgba(255, 255, 255, 20),
    hover: rgba(255, 255, 255, 15),
    sel: rgba(255, 255, 255, 25),
    pop: rgba(44, 44, 48, 235),
    accent: rgb(0x0A84FF),
    green: rgb(0x30D158),
    orange: rgb(0xFF9F0A),
    red: rgb(0xFF453A),
    live_bg: rgb(0x0A0A0B),
    hud: rgba(32, 32, 36, 190),
};

pub fn text_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(Arc::from(FONT_SEMIBOLD)))
}

pub fn mono_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn install_fonts(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 1. Register Lucide icon font
    fonts.font_data.insert(
        super::icons::ICONS_FAMILY.to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/lucide.ttf"
        ))),
    );
    fonts.families.insert(
        FontFamily::Name(Arc::from(super::icons::ICONS_FAMILY)),
        vec![super::icons::ICONS_FAMILY.to_owned()],
    );

    // 2. Windows system fonts (Segoe UI + 微软雅黑 UI)
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let fonts_dir = std::path::Path::new(&windir).join("Fonts");

    let yahei_reg = fonts_dir.join("msyh.ttc");
    let yahei_bd = fonts_dir.join("msyhbd.ttc");
    let segoe_reg = fonts_dir.join("segoeui.ttf");
    let segoe_sb = fonts_dir.join("seguisb.ttf");

    if let Ok(bytes) = std::fs::read(&segoe_reg) {
        fonts
            .font_data
            .insert("SegoeUI".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    }
    if let Ok(bytes) = std::fs::read(&segoe_sb) {
        fonts
            .font_data
            .insert("SegoeUI-Semibold".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    }
    if let Ok(bytes) = std::fs::read(&yahei_reg) {
        fonts
            .font_data
            .insert("MicrosoftYaHei".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    }
    if let Ok(bytes) = std::fs::read(&yahei_bd) {
        fonts
            .font_data
            .insert("MicrosoftYaHei-Bold".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    }

    // Set font family fallbacks
    // Proportional: SegoeUI (Latin) -> MicrosoftYaHei (CJK)
    let mut prop = Vec::new();
    if fonts.font_data.contains_key("SegoeUI") {
        prop.push("SegoeUI".to_owned());
    }
    if fonts.font_data.contains_key("MicrosoftYaHei") {
        prop.push("MicrosoftYaHei".to_owned());
    }
    fonts.families.insert(FontFamily::Proportional, prop);

    // Semibold: SegoeUI-Semibold -> MicrosoftYaHei-Bold
    let mut sb = Vec::new();
    if fonts.font_data.contains_key("SegoeUI-Semibold") {
        sb.push("SegoeUI-Semibold".to_owned());
    }
    if fonts.font_data.contains_key("MicrosoftYaHei-Bold") {
        sb.push("MicrosoftYaHei-Bold".to_owned());
    }
    if fonts.font_data.contains_key("MicrosoftYaHei") {
        sb.push("MicrosoftYaHei".to_owned());
    }
    fonts
        .families
        .insert(FontFamily::Name(Arc::from(FONT_SEMIBOLD)), sb);

    // Monospace: fallback to MicrosoftYaHei for any CJK
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        if fonts.font_data.contains_key("MicrosoftYaHei") {
            mono.push("MicrosoftYaHei".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &Context, p: &Palette) {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.content;
    v.window_fill = p.group;
    v.extreme_bg_color = p.field;
    v.faint_bg_color = p.hover;
    v.window_corner_radius = CornerRadius::same(12);
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 128 } else { 56 }),
    };
    v.popup_shadow = v.window_shadow;

    v.selection.bg_fill = p.accent.gamma_multiply(0.3);
    v.selection.stroke = Stroke::new(1.0, p.accent);

    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.corner_radius = CornerRadius::same(7);
    }

    v.widgets.inactive.weak_bg_fill = p.btn;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.btn_edge);
    v.widgets.hovered.weak_bg_fill = p.btn;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.btn_edge);
    v.widgets.active.weak_bg_fill = p.sel;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.btn_edge);

    v.hyperlink_color = p.accent;

    ctx.set_visuals(v);
}
