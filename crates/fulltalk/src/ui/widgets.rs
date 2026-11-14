//! Small custom widgets drawn with the painter, matching the prototype.

use std::f32::consts::TAU;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Frame, Margin, Pos2, Rect, Response, Sense,
    Shadow, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use super::theme::{self, icon};

/// The rounded white card that holds each window's content.
pub fn card() -> Frame {
    Frame::new()
        .fill(theme::SURFACE)
        .corner_radius(theme::CARD_RADIUS)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::symmetric(18, 14))
        .outer_margin(Margin::same(theme::WINDOW_MARGIN))
        .shadow(Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(38),
        })
}

/// A soft inner panel (settings rows).
pub fn section() -> Frame {
    Frame::new()
        .fill(theme::SURFACE)
        .corner_radius(12)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::symmetric(14, 12))
}

/// Square icon button with a hover background.
pub fn icon_button(ui: &mut Ui, glyph: &str, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 8, theme::ROW_HOVER);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        theme::icons(19.0),
        if hovered { theme::TEXT } else { theme::MUTED },
    );
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

/// The waveform logo mark.
pub fn logo(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(height * 0.9, height), Sense::hover());
    let bar_w = height * 0.12;
    let gap = height * 0.075;
    let total = bar_w * 5.0 + gap * 4.0;
    let x0 = rect.center().x - total / 2.0;
    for (i, h) in super::icon::BARS.iter().enumerate() {
        let bh = height * h * 1.15;
        let x = x0 + i as f32 * (bar_w + gap);
        let bar = Rect::from_center_size(pos2(x + bar_w / 2.0, rect.center().y), vec2(bar_w, bh));
        ui.painter()
            .rect_filled(bar, CornerRadius::same((bar_w / 2.0) as u8), theme::PRIMARY);
    }
}

/// What the big microphone button shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MicMode {
    Idle,
    /// `level` is the live input level, 0–1.
    Recording {
        level: f32,
    },
    Busy,
    /// Model download/loading; `progress` 0–1 when known.
    Preparing {
        progress: Option<f32>,
    },
    Unavailable,
}

/// The large circular mic button with animated halo / progress ring.
pub fn mic_button(ui: &mut Ui, mode: MicMode) -> Response {
    let size = 132.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let painter = ui.painter_at(rect.expand(4.0));
    let c = rect.center();
    let t = ui.input(|i| i.time) as f32;
    let core_r = 38.0;
    let hovered = response.hovered() && !matches!(mode, MicMode::Busy | MicMode::Unavailable);

    // Halo: static when idle, breathing with the voice when recording.
    let (outer, inner) = match mode {
        MicMode::Recording { level } => {
            let breathe = (t * 3.2).sin() * 0.5 + 0.5;
            (58.0 + 6.0 * level + 2.5 * breathe, 48.0 + 4.0 * level)
        }
        _ => (56.0, 47.0),
    };
    painter.circle_filled(c, outer, theme::PRIMARY_SOFTER);
    painter.circle_filled(c, inner, theme::PRIMARY_SOFT);

    let fill = match mode {
        MicMode::Unavailable => theme::SUBTLE,
        _ if hovered => theme::PRIMARY_HOVER,
        _ => theme::PRIMARY,
    };
    painter.circle_filled(c, core_r, fill);
    painter.text(
        c,
        Align2::CENTER_CENTER,
        icon::MICROPHONE,
        theme::icons(36.0),
        Color32::WHITE,
    );

    match mode {
        MicMode::Busy => arc(&painter, c, inner + 3.0, t * 4.0, 0.28, theme::PRIMARY),
        MicMode::Preparing { progress: Some(p) } => {
            arc(
                &painter,
                c,
                inner + 3.0,
                -TAU / 4.0 / TAU,
                p.clamp(0.0, 1.0),
                theme::PRIMARY,
            );
        }
        MicMode::Preparing { progress: None } => {
            arc(&painter, c, inner + 3.0, t * 2.0, 0.2, theme::PRIMARY);
        }
        _ => {}
    }

    let enabled = !matches!(mode, MicMode::Unavailable);
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Draws an arc starting at `start_turns` (fraction of a full turn) spanning `len_turns`.
fn arc(painter: &egui::Painter, c: Pos2, r: f32, start_turns: f32, len_turns: f32, color: Color32) {
    let steps = 48;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = (start_turns + len_turns * i as f32 / steps as f32) * TAU;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    painter.add(Shape::line(points, Stroke::new(3.0, color)));
}

/// A keyboard key cap, e.g. `[Alt]`.
pub fn key_cap(ui: &mut Ui, label: &str, large: bool) {
    let font = if large {
        theme::semibold(15.0)
    } else {
        theme::semibold(12.0)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, theme::TEXT);
    let pad = if large {
        vec2(12.0, 6.0)
    } else {
        vec2(6.0, 2.0)
    };
    let size = galley.size() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 6, theme::SURFACE);
    painter.rect_stroke(rect, 6, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside);
    painter.galley(rect.min + pad, galley, theme::TEXT);
}

/// Renders `Alt + S` as key caps.
pub fn shortcut_caps(ui: &mut Ui, labels: &[String], large: bool) {
    ui.spacing_mut().item_spacing.x = if large { 8.0 } else { 4.0 };
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            ui.label(
                egui::RichText::new("+")
                    .font(theme::regular(if large { 16.0 } else { 12.0 }))
                    .color(theme::MUTED),
            );
        }
        key_cap(ui, label, large);
    }
}

/// iOS-style switch.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = vec2(44.0, 24.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
    let bg = lerp(theme::BORDER, theme::PRIMARY, how_on);
    let painter = ui.painter();
    painter.rect_filled(rect, 12, bg);
    let r = rect.height() / 2.0 - 3.0;
    let x = egui::lerp((rect.left() + r + 3.0)..=(rect.right() - r - 3.0), how_on);
    painter.circle_filled(pos2(x, rect.center().y), r, Color32::WHITE);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// One history entry: click anywhere to copy. Returns the row response.
pub fn history_row(ui: &mut Ui, text: &str, just_copied: bool) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered || just_copied {
        painter.rect_filled(rect, 10, theme::ROW_HOVER);
    }

    let icon_glyph = if just_copied {
        icon::CHECK_CIRCLE
    } else {
        icon::COPY
    };
    let icon_color = if just_copied || hovered {
        theme::PRIMARY
    } else {
        theme::SUBTLE
    };
    let icon_rect = Rect::from_center_size(
        pos2(rect.right() - 18.0, rect.center().y),
        Vec2::splat(20.0),
    );
    painter.text(
        icon_rect.center(),
        Align2::CENTER_CENTER,
        icon_glyph,
        theme::icons(18.0),
        icon_color,
    );

    let text_width = rect.width() - 52.0;
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_owned(),
        theme::regular(13.5),
        theme::TEXT,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_width);
    let galley = ui.painter().layout_job(job);
    let pos = pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, theme::TEXT);

    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if text.chars().count() > 40 {
            text
        } else {
            "Clique para copiar"
        })
}

/// A thin horizontal rule.
pub fn divider(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, theme::BORDER);
}
