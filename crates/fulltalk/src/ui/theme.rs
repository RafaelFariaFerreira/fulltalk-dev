//! Visual identity: palette, typography and icon glyphs.

use std::sync::Arc;

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};

pub const PRIMARY: Color32 = Color32::from_rgb(0x5B, 0x3F, 0xE4);
pub const PRIMARY_HOVER: Color32 = Color32::from_rgb(0x4E, 0x33, 0xD6);
pub const PRIMARY_SOFT: Color32 = Color32::from_rgb(0xEE, 0xEB, 0xFF);
pub const PRIMARY_SOFTER: Color32 = Color32::from_rgb(0xF6, 0xF4, 0xFF);
pub const SURFACE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const SURFACE_ALT: Color32 = Color32::from_rgb(0xF7, 0xF6, 0xFC);
pub const ROW_HOVER: Color32 = Color32::from_rgb(0xEF, 0xEC, 0xFD);
pub const BORDER: Color32 = Color32::from_rgb(0xE8, 0xE6, 0xF3);
pub const TEXT: Color32 = Color32::from_rgb(0x1B, 0x1A, 0x2E);
pub const MUTED: Color32 = Color32::from_rgb(0x6B, 0x6A, 0x80);
pub const SUBTLE: Color32 = Color32::from_rgb(0x9A, 0x99, 0xAD);
pub const DANGER: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
pub const WARNING: Color32 = Color32::from_rgb(0xC2, 0x6B, 0x00);

pub const CARD_RADIUS: u8 = 18;
/// Transparent margin around the card so its shadow is visible.
pub const WINDOW_MARGIN: i8 = 12;

const SEMIBOLD: &str = "inter-semibold";
/// Icons get their own family: Inter also defines private-use glyphs that
/// would otherwise shadow Phosphor's codepoints.
const ICONS: &str = "icons";

/// Phosphor icon glyphs (regular weight).
pub mod icon {
    pub const CHECK_CIRCLE: &str = "\u{E184}";
    pub const CLOCK_COUNTER_CLOCKWISE: &str = "\u{E1A0}";
    pub const COPY: &str = "\u{E1CA}";
    pub const GEAR: &str = "\u{E270}";
    pub const INFO: &str = "\u{E2CE}";
    pub const MICROPHONE: &str = "\u{E326}";
    pub const PENCIL_SIMPLE: &str = "\u{E3B4}";
    pub const ROCKET_LAUNCH: &str = "\u{E3FE}";
    pub const CLIPBOARD_TEXT: &str = "\u{E198}";
    pub const WARNING_CIRCLE: &str = "\u{E4E2}";
    pub const X: &str = "\u{E4F6}";
    pub const CURSOR_TEXT: &str = "\u{E7D8}";
    pub const GITHUB_LOGO: &str = "\u{E576}";
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn icons(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

/// Installs fonts and light visuals.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let font = |bytes: &'static [u8]| Arc::new(FontData::from_static(bytes));
    fonts.font_data.insert(
        "inter".into(),
        font(include_bytes!("../../assets/fonts/Inter-Regular.ttf")),
    );
    fonts.font_data.insert(
        SEMIBOLD.into(),
        font(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf")),
    );
    fonts.font_data.insert(
        "phosphor".into(),
        font(include_bytes!("../../assets/fonts/Phosphor.ttf")),
    );
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["inter".into()]);
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), vec![SEMIBOLD.into()]);
    fonts
        .families
        .insert(FontFamily::Name(ICONS.into()), vec!["phosphor".into()]);
    ctx.set_fonts(fonts);

    let mut visuals = Visuals::light();
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.window_fill = SURFACE;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = PRIMARY_SOFT;
    visuals.selection.stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.hovered.bg_fill = ROW_HOVER;
    ctx.set_visuals(visuals);
}
