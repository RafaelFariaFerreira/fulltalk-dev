//! The settings window (a separate, focusable viewport so it can capture keys).

use eframe::egui::{self, Align, Layout, RichText, Sense, Ui, ViewportBuilder, ViewportId, vec2};

use super::theme::{self, icon};
use super::widgets;
use crate::app::{APP_NAME, FulltalkApp, REPO_URL, SLOGAN};
use crate::keymap;

pub const SIZE: [f32; 2] = [420.0, 486.0];

impl FulltalkApp {
    pub(crate) fn settings_viewport(&mut self, ctx: &egui::Context) {
        let mut builder = ViewportBuilder::default()
            .with_title(format!("Configurações — {APP_NAME}"))
            .with_inner_size(SIZE)
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top();
        if let Some(main) = ctx.input(|i| i.viewport().outer_rect) {
            let monitor_w = ctx.input(|i| i.viewport().monitor_size.map_or(f32::MAX, |s| s.x));
            let right = main.right() + 4.0;
            let x = if right + SIZE[0] <= monitor_w {
                right
            } else {
                (main.left() - SIZE[0] - 4.0).max(0.0)
            };
            builder = builder.with_position([x, main.top()]);
        }

        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("settings"),
            builder,
            |ui, _class| {
                if ui.input(|i| i.viewport().close_requested()) {
                    self.settings_open = false;
                }
                widgets::card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    self.settings_content(ui);
                });
            },
        );
    }

    pub(crate) fn settings_content(&mut self, ui: &mut Ui) {
        let header = ui.horizontal(|ui| {
            ui.label(RichText::new(icon::GEAR).font(theme::icons(21.0)));
            ui.label(RichText::new("Configurações").font(theme::semibold(16.0)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, icon::X, "Fechar").clicked() {
                    self.settings_open = false;
                    self.capturing_shortcut = false;
                }
            });
        });
        let drag = ui.interact(
            header.response.rect,
            ui.scope_id().with("drag"),
            Sense::drag(),
        );
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        ui.add_space(10.0);

        self.shortcut_section(ui);
        ui.add_space(8.0);

        let mut auto_paste = self.settings.auto_paste;
        if toggle_row(
            ui,
            icon::CURSOR_TEXT,
            "Colar automaticamente",
            "Cola no campo de texto ou terminal ativo. Fora deles, só copia.",
            &mut auto_paste,
        ) {
            self.settings.auto_paste = auto_paste;
            self.persist_settings();
        }
        ui.add_space(8.0);

        let mut launch = self.settings.launch_at_login;
        if toggle_row(
            ui,
            icon::ROCKET_LAUNCH,
            "Iniciar com o sistema",
            "Abre o app automaticamente ao entrar na sessão.",
            &mut launch,
        ) {
            self.set_launch_at_login(launch);
        }
        ui.add_space(8.0);

        widgets::section().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon::INFO)
                        .font(theme::icons(21.0))
                        .color(theme::MUTED),
                );
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        RichText::new(format!("Sobre o {APP_NAME}")).font(theme::semibold(13.5)),
                    );
                    ui.label(
                        RichText::new(format!("Versão {} · {SLOGAN}", env!("CARGO_PKG_VERSION")))
                            .font(theme::regular(11.5))
                            .color(theme::MUTED),
                    );
                    ui.label(
                        RichText::new("Parakeet TDT 0.6B v3 (NVIDIA) · 100% local")
                            .font(theme::regular(11.5))
                            .color(theme::MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::GITHUB_LOGO, "Código no GitHub").clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(REPO_URL));
                    }
                });
            });
        });
    }

    fn shortcut_section(&mut self, ui: &mut Ui) {
        widgets::section().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Atalho para falar").font(theme::semibold(13.5)));
            ui.label(
                RichText::new("Segure para gravar, solte para transcrever.")
                    .font(theme::regular(12.0))
                    .color(theme::MUTED),
            );
            ui.add_space(8.0);

            egui::Frame::new()
                .fill(theme::SURFACE_ALT)
                .corner_radius(10)
                .stroke(egui::Stroke::new(
                    1.0,
                    if self.capturing_shortcut {
                        theme::PRIMARY
                    } else {
                        theme::BORDER
                    },
                ))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.set_min_height(32.0);
                        if self.capturing_shortcut {
                            ui.label(
                                RichText::new("Pressione a nova combinação… (Esc cancela)")
                                    .font(theme::regular(13.0))
                                    .color(theme::PRIMARY),
                            );
                            self.capture_shortcut(ui);
                        } else {
                            widgets::shortcut_caps(ui, &self.settings.shortcut.labels(), true);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (glyph, tip) = if self.capturing_shortcut {
                                (icon::X, "Cancelar")
                            } else {
                                (icon::PENCIL_SIMPLE, "Alterar atalho")
                            };
                            if widgets::icon_button(ui, glyph, tip).clicked() {
                                self.capturing_shortcut = !self.capturing_shortcut;
                                self.shortcut_error = None;
                            }
                        });
                    });
                });
            if let Some(err) = &self.shortcut_error {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(err)
                        .font(theme::regular(11.5))
                        .color(theme::DANGER),
                );
            }
        });
    }

    /// Reads key presses while in capture mode.
    fn capture_shortcut(&mut self, ui: &Ui) {
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        let Some((key, modifiers)) = pressed else {
            return;
        };
        if key == egui::Key::Escape {
            self.capturing_shortcut = false;
            return;
        }
        match keymap::shortcut(key, modifiers) {
            Some(Ok(shortcut)) => {
                self.capturing_shortcut = false;
                self.change_shortcut(shortcut);
            }
            Some(Err(err)) => self.shortcut_error = Some(err.to_string()),
            None => self.shortcut_error = Some("Essa tecla não pode ser usada no atalho.".into()),
        }
    }
}

/// A settings row with an icon, title, description and a switch. Returns `true` when toggled.
fn toggle_row(ui: &mut Ui, glyph: &str, title: &str, description: &str, value: &mut bool) -> bool {
    let mut changed = false;
    widgets::section().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(glyph)
                    .font(theme::icons(21.0))
                    .color(theme::MUTED),
            );
            ui.add_space(4.0);
            ui.allocate_ui_with_layout(
                vec2(ui.available_width() - 56.0, 40.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(title).font(theme::semibold(13.5)));
                    ui.add(
                        egui::Label::new(
                            RichText::new(description)
                                .font(theme::regular(11.5))
                                .color(theme::MUTED),
                        )
                        .wrap(),
                    );
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                changed = widgets::toggle(ui, value).changed();
            });
        });
    });
    changed
}
