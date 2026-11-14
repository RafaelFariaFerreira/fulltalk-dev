//! The floating main window: header, mic button, status, shortcut hint and,
//! when the clock button is on, the recent transcriptions.

use eframe::egui::{self, Align, Layout, RichText, Sense, Ui, vec2};
use fulltalk_core::session::{Input, State, Trigger};

use super::theme::{self, icon};
use super::widgets::{self, MicMode};
use crate::app::{FulltalkApp, SLOGAN};
use crate::workers::EngineStatus;

impl FulltalkApp {
    pub(crate) fn main_view(&mut self, ui: &mut Ui) {
        let card = widgets::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            self.header(ui);
            ui.add_space(6.0);
            self.hero(ui);
            if self.settings.show_history {
                ui.add_space(14.0);
                widgets::divider(ui);
                ui.add_space(10.0);
                self.history_list(ui);
            }
            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                ui.label(
                    RichText::new(SLOGAN)
                        .font(theme::regular(11.0))
                        .color(theme::SUBTLE),
                );
            });
        });

        let card_rect = card.response.rect.shrink(f32::from(theme::WINDOW_MARGIN));
        if let Some(toast) = &self.toast
            && (toast.expired() || super::toast::show(ui, toast, card_rect))
        {
            self.toast = None;
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let (drag, ()) = widgets::window_header(ui, |ui| {
            widgets::logo(ui, 22.0);
            ui.add_space(4.0);
            ui.label(RichText::new("fulltalk").font(theme::semibold(16.0)));
            ui.label(
                RichText::new("dev")
                    .font(theme::regular(16.0))
                    .color(theme::PRIMARY),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, icon::X, "Fechar").clicked() {
                    self.quit(ui.ctx());
                }
                if widgets::icon_button(ui, icon::GEAR, "Configurações").clicked() {
                    self.settings_open = !self.settings_open;
                }
                let open = self.settings.show_history;
                let tooltip = if open {
                    "Ocultar últimas transcrições"
                } else {
                    "Mostrar últimas transcrições"
                };
                if widgets::icon_toggle(ui, icon::CLOCK_COUNTER_CLOCKWISE, tooltip, open).clicked()
                {
                    self.set_history_visible(ui.ctx(), !open);
                }
            });
        });
        self.drag_main_window(ui.ctx(), &drag);
    }

    fn hero(&mut self, ui: &mut Ui) {
        let mode = self.mic_mode();
        let (title, subtitle) = self.status_text();
        ui.vertical_centered(|ui| {
            let response = widgets::mic_button(ui, mode).on_hover_text(match self.state() {
                State::Recording(_) => "Clique para parar",
                _ => "Clique para falar",
            });
            if response.clicked() && !matches!(mode, MicMode::Unavailable) {
                tracing::debug!(state = ?self.state(), "microphone clicked");
                self.apply(Input::ButtonClicked);
            }
            ui.add_space(2.0);
            ui.label(RichText::new(title).font(theme::semibold(18.0)));
            ui.add_space(2.0);
            ui.add(
                egui::Label::new(
                    RichText::new(subtitle)
                        .font(theme::regular(13.0))
                        .color(theme::MUTED),
                )
                .wrap(),
            );
            if matches!(self.engine_status, EngineStatus::Failed(_)) {
                ui.add_space(6.0);
                let retry = ui.add(
                    egui::Label::new(
                        RichText::new("Tentar novamente")
                            .font(theme::semibold(13.0))
                            .color(theme::PRIMARY)
                            .underline(),
                    )
                    .sense(Sense::click()),
                );
                if retry
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.retry_engine();
                }
            }
            ui.add_space(12.0);
            self.shortcut_hint(ui);
        });
    }

    fn shortcut_hint(&self, ui: &mut Ui) {
        if let Some(err) = &self.hotkey_error {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(icon::WARNING_CIRCLE)
                        .font(theme::icons(15.0))
                        .color(theme::WARNING),
                );
                ui.label(
                    RichText::new(err)
                        .font(theme::regular(11.5))
                        .color(theme::WARNING),
                );
            });
            return;
        }
        let labels = self.settings.shortcut.labels();
        // A frame is only as wide as its content, which is known after it is
        // laid out, so the pill is centred with the width of the last frame.
        let width_id = ui.scope_id().with("shortcut_hint_width");
        let last_width: f32 = ui.ctx().data(|d| d.get_temp(width_id)).unwrap_or(0.0);
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - last_width) / 2.0).max(0.0));
            let pill = egui::Frame::new()
                .fill(theme::SURFACE_ALT)
                .corner_radius(20)
                .inner_margin(egui::Margin::symmetric(14, 7))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        widgets::logo(ui, 13.0);
                        ui.label(
                            RichText::new("Segure")
                                .font(theme::regular(12.5))
                                .color(theme::MUTED),
                        );
                        widgets::shortcut_caps(ui, &labels, false);
                        ui.label(
                            RichText::new("para falar")
                                .font(theme::regular(12.5))
                                .color(theme::MUTED),
                        );
                    });
                });
            let width = pill.response.rect.width();
            if (width - last_width).abs() > 0.5 {
                ui.ctx().data_mut(|d| d.insert_temp(width_id, width));
                ui.ctx().request_repaint();
            }
        });
    }

    fn history_list(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(icon::CLOCK_COUNTER_CLOCKWISE)
                    .font(theme::icons(17.0))
                    .color(theme::MUTED),
            );
            ui.label(RichText::new("Últimas transcrições").font(theme::semibold(13.5)));
        });
        ui.add_space(4.0);

        if self.history.is_empty() {
            ui.add_space(18.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(icon::CLIPBOARD_TEXT)
                        .font(theme::icons(28.0))
                        .color(theme::BORDER),
                );
                ui.label(
                    RichText::new(
                        "Suas 5 últimas transcrições aparecem aqui.\nClique em uma para copiar.",
                    )
                    .font(theme::regular(12.0))
                    .color(theme::SUBTLE),
                );
            });
            return;
        }

        ui.spacing_mut().item_spacing.y = 2.0;
        let copied = self.copied.map(|(i, _)| i);
        let mut clicked = None;
        for (i, entry) in self.history.iter().enumerate() {
            if widgets::history_row(ui, &entry.text, copied == Some(i)).clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = clicked {
            self.copy_history(i);
        }
        ui.allocate_space(vec2(0.0, 0.0));
    }

    fn mic_mode(&self) -> MicMode {
        match (self.state(), &self.engine_status) {
            (State::Recording(_), _) => MicMode::Recording {
                level: self.recording_level(),
            },
            (State::Transcribing, _) => MicMode::Busy,
            (State::Idle, EngineStatus::Downloading { done, total }) => MicMode::Preparing {
                progress: Some(*done as f32 / (*total).max(1) as f32),
            },
            (State::Idle, EngineStatus::Loading) => MicMode::Preparing { progress: None },
            (State::Idle, EngineStatus::Failed(_)) => MicMode::Unavailable,
            (State::Idle, EngineStatus::Ready) => MicMode::Idle,
        }
    }

    fn status_text(&self) -> (String, String) {
        let shortcut = self.settings.shortcut.to_string();
        match (self.state(), &self.engine_status) {
            (State::Recording(trigger), _) => {
                let secs = self.recording_since.map_or(0, |t| t.elapsed().as_secs());
                let how = match trigger {
                    Trigger::Hotkey => format!("Solte {shortcut} para transcrever"),
                    Trigger::Button => "Clique de novo para transcrever".to_owned(),
                };
                (
                    format!("Ouvindo sua fala…  {}:{:02}", secs / 60, secs % 60),
                    how,
                )
            }
            (State::Transcribing, EngineStatus::Ready) => (
                "Transcrevendo…".to_owned(),
                "Processado localmente, nada sai do seu computador.".to_owned(),
            ),
            (State::Transcribing, _) => (
                "Na fila…".to_owned(),
                "Transcrevo assim que o modelo terminar de carregar.".to_owned(),
            ),
            (State::Idle, EngineStatus::Downloading { done, total }) => (
                format!("Baixando modelo… {}%", done * 100 / (*total).max(1)),
                format!(
                    "Só na primeira vez ({:.0} MB). Depois tudo roda offline.",
                    *total as f64 / 1e6
                ),
            ),
            (State::Idle, EngineStatus::Loading) => (
                "Preparando…".to_owned(),
                "Carregando o modelo de voz na memória.".to_owned(),
            ),
            (State::Idle, EngineStatus::Failed(err)) => {
                ("Modelo indisponível".to_owned(), err.clone())
            }
            (State::Idle, EngineStatus::Ready) => (
                "Pronto para ouvir".to_owned(),
                format!("Clique no microfone ou segure {shortcut}."),
            ),
        }
    }
}
