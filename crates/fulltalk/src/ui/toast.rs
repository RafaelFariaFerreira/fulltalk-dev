//! Transient notification shown at the top of the main card.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Frame, Margin, Order, Shadow, Stroke, Ui, vec2};

use super::theme::{self, icon};

const VISIBLE_FOR: Duration = Duration::from_millis(3200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub title: String,
    pub body: String,
    shown_at: Instant,
}

impl Toast {
    pub fn new(kind: ToastKind, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            body: body.into(),
            shown_at: Instant::now(),
        }
    }

    /// Errors stay a bit longer so they can be read.
    fn lifetime(&self) -> Duration {
        match self.kind {
            ToastKind::Error | ToastKind::Warning => VISIBLE_FOR * 2,
            _ => VISIBLE_FOR,
        }
    }

    #[cfg(feature = "screenshot")]
    pub fn age(&self) -> Duration {
        self.shown_at.elapsed()
    }

    pub fn remaining(&self) -> Duration {
        self.lifetime().saturating_sub(self.shown_at.elapsed())
    }

    pub fn expired(&self) -> bool {
        self.remaining().is_zero()
    }

    fn glyph_and_color(&self) -> (&'static str, Color32) {
        match self.kind {
            ToastKind::Success => (icon::CHECK_CIRCLE, theme::PRIMARY),
            ToastKind::Info => (icon::INFO, theme::MUTED),
            ToastKind::Warning => (icon::WARNING_CIRCLE, theme::WARNING),
            ToastKind::Error => (icon::WARNING_CIRCLE, theme::DANGER),
        }
    }
}

/// Shows `toast` floating over the top of `card_rect`. Returns `true` when it
/// was clicked (dismiss).
pub fn show(ui: &Ui, toast: &Toast, card_rect: egui::Rect) -> bool {
    let width = card_rect.width() - 24.0;
    let pos = card_rect.left_top() + vec2(12.0, 10.0);
    let (glyph, color) = toast.glyph_and_color();
    let area = egui::Area::new(egui::Id::unique("fulltalk-toast"))
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(theme::SURFACE)
                .corner_radius(14)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin::symmetric(14, 10))
                .shadow(Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: Color32::from_black_alpha(30),
                })
                .show(ui, |ui| {
                    ui.set_width(width - 28.0);
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            glyph,
                            theme::icons(21.0),
                            color,
                        );
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.label(egui::RichText::new(&toast.title).font(theme::semibold(13.5)));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&toast.body)
                                        .font(theme::regular(12.0))
                                        .color(theme::MUTED),
                                )
                                .wrap(),
                            );
                        });
                    });
                })
        });
    area.response.interact(egui::Sense::click()).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_toast_is_visible_and_errors_last_longer() {
        let ok = Toast::new(ToastKind::Success, "a", "b");
        let err = Toast::new(ToastKind::Error, "a", "b");
        assert!(!ok.expired());
        assert!(err.remaining() > ok.remaining());
    }
}
