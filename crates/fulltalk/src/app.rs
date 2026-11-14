//! The application controller: owns the session state machine and executes
//! its effects against the recorder, the engine and the delivery worker.

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;
use fulltalk_core::delivery::Outcome;
use fulltalk_core::history::History;
use fulltalk_core::session::{Effect, Input, Notice, Session, State};
use fulltalk_core::settings::Settings;
use fulltalk_core::shortcut::Shortcut;
use fulltalk_core::store;
use fulltalk_platform::autostart::Autostart;
use fulltalk_platform::hotkey::{HotkeyEvent, HotkeyService};

use crate::audio::Recorder;
use crate::paths::Paths;
use crate::ui::toast::{Toast, ToastKind};
use crate::workers::{AppEvent, Deliverer, DeliveryJob, Engine, EngineStatus, Notifier};

pub const APP_NAME: &str = "fulltalk dev";
pub const SLOGAN: &str = "code is cheap, show me the talk";
pub const REPO_URL: &str = "https://github.com/RafaelFariaFerreira/fulltalk-dev";

pub struct FulltalkApp {
    pub(crate) session: Session,
    pub(crate) settings: Settings,
    pub(crate) history: History,
    pub(crate) engine_status: EngineStatus,
    pub(crate) hotkey_error: Option<String>,
    pub(crate) toast: Option<Toast>,
    pub(crate) settings_open: bool,
    pub(crate) capturing_shortcut: bool,
    pub(crate) shortcut_error: Option<String>,
    pub(crate) copied: Option<(usize, Instant)>,
    pub(crate) recording_since: Option<Instant>,
    pub(crate) autostart: Option<Autostart>,
    pub(crate) window_pos: Option<egui::Pos2>,
    #[cfg(feature = "screenshot")]
    shooter: Option<crate::screenshot::Shooter>,

    paths: Paths,
    recorder: Recorder,
    engine: Engine,
    deliverer: Deliverer,
    hotkeys: Option<HotkeyService>,
    notify: Notifier,
    events: Receiver<AppEvent>,
}

impl std::fmt::Debug for FulltalkApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FulltalkApp")
            .field("state", &self.session.state())
            .finish_non_exhaustive()
    }
}

impl FulltalkApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        paths: Paths,
        settings: Settings,
        history: History,
    ) -> Self {
        crate::ui::theme::install(&cc.egui_ctx);
        make_window_non_activating(cc);

        let (notify, events) = Notifier::new(cc.egui_ctx.clone());

        let hotkey_notify = notify.clone();
        let handler = Arc::new(move |e: HotkeyEvent| hotkey_notify.send(AppEvent::Hotkey(e)));
        let (hotkeys, hotkey_error) = match HotkeyService::start(settings.shortcut, handler) {
            Ok(service) => (Some(service), None),
            Err(err) => {
                tracing::warn!(%err, "global shortcut unavailable");
                (None, Some(err.to_string()))
            }
        };
        let modifiers = hotkeys
            .as_ref()
            .map_or_else(Default::default, HotkeyService::modifier_probe);

        let autostart = Autostart::for_current_exe(APP_NAME)
            .inspect_err(|err| tracing::warn!(%err, "autostart unavailable"))
            .ok();

        Self {
            session: Session::new(),
            engine: Engine::spawn(paths.models.clone(), notify.clone()),
            deliverer: Deliverer::spawn(notify.clone(), modifiers),
            recorder: Recorder::new(),
            engine_status: EngineStatus::Loading,
            toast: None,
            settings_open: false,
            capturing_shortcut: false,
            shortcut_error: None,
            copied: None,
            recording_since: None,
            window_pos: None,
            #[cfg(feature = "screenshot")]
            shooter: crate::screenshot::Shooter::from_env(),
            hotkey_error,
            hotkeys,
            autostart,
            settings,
            history,
            paths,
            notify,
            events,
        }
    }

    pub fn state(&self) -> State {
        self.session.state()
    }

    /// Drains worker events. Called once per frame.
    pub(crate) fn pump_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                AppEvent::Hotkey(_) if self.capturing_shortcut => {}
                AppEvent::Hotkey(HotkeyEvent::Pressed) => self.apply(Input::HotkeyPressed),
                AppEvent::Hotkey(HotkeyEvent::Released) => self.apply(Input::HotkeyReleased),
                AppEvent::CaptureFailed(reason) => {
                    // The device died mid-recording: release it before resetting.
                    self.recorder.stop();
                    self.recording_since = None;
                    self.apply(Input::CaptureFailed(reason));
                }
                AppEvent::Transcribed(Ok(text)) => self.apply(Input::TranscriptionSucceeded(text)),
                AppEvent::Transcribed(Err(reason)) => {
                    self.apply(Input::TranscriptionFailed(reason));
                }
                AppEvent::Engine(status) => self.engine_status = status,
                AppEvent::Delivered(result) => self.toast = Some(delivered_toast(result)),
                AppEvent::Copied(Ok(())) => {}
                AppEvent::Copied(Err(err)) => {
                    self.toast = Some(Toast::new(ToastKind::Error, "Não foi possível copiar", err));
                }
            }
        }
    }

    /// Feeds the state machine and runs the resulting effects.
    pub(crate) fn apply(&mut self, input: Input) {
        for effect in self.session.handle(input) {
            match effect {
                Effect::StartCapture => self.start_capture(),
                Effect::FinishCapture => self.finish_capture(),
                Effect::Deliver(text) => self.deliver(text),
                Effect::Notify(notice) => self.toast = Some(notice_toast(notice)),
            }
        }
    }

    fn start_capture(&mut self) {
        let notify = self.notify.clone();
        match self
            .recorder
            .start(move |reason| notify.send(AppEvent::CaptureFailed(reason)))
        {
            Ok(()) => {
                self.recording_since = Some(Instant::now());
                self.toast = None;
            }
            Err(reason) => self.apply(Input::CaptureFailed(reason)),
        }
    }

    fn finish_capture(&mut self) {
        self.recording_since = None;
        match self
            .recorder
            .stop()
            .map(fulltalk_core::audio::CaptureBuffer::finish)
        {
            Some(Ok(samples)) => self.engine.transcribe(samples),
            Some(Err(_)) | None => self.apply(Input::NothingCaptured),
        }
    }

    fn deliver(&mut self, text: String) {
        if self.history.push(&text, unix_now()) {
            self.persist_history();
        }
        self.deliverer.send(DeliveryJob::Transcription {
            text,
            auto_paste: self.settings.auto_paste,
        });
    }

    /// Copies a history entry (click on a row).
    pub(crate) fn copy_history(&mut self, index: usize) {
        if let Some(entry) = self.history.get(index) {
            self.deliverer.send(DeliveryJob::Copy(entry.text.clone()));
            self.copied = Some((index, Instant::now()));
        }
    }

    /// Restarts model download/loading after a failure.
    pub(crate) fn retry_engine(&mut self) {
        self.engine_status = EngineStatus::Loading;
        self.engine = Engine::spawn(self.paths.models.clone(), self.notify.clone());
    }

    pub(crate) fn recording_level(&self) -> f32 {
        self.recorder.level()
    }

    pub(crate) fn change_shortcut(&mut self, shortcut: Shortcut) {
        let result = match self.hotkeys.as_mut() {
            Some(h) => h.set_shortcut(shortcut).map_err(|e| e.to_string()),
            None => Ok(()),
        };
        match result {
            Ok(()) => {
                self.settings.shortcut = shortcut;
                self.shortcut_error = None;
                self.persist_settings();
            }
            Err(err) => self.shortcut_error = Some(err),
        }
    }

    pub(crate) fn set_launch_at_login(&mut self, enabled: bool) {
        let result = self.autostart.as_ref().map_or(Ok(()), |a| {
            a.set_enabled(enabled).map_err(|e| e.to_string())
        });
        match result {
            Ok(()) => {
                self.settings.launch_at_login = enabled;
                self.persist_settings();
            }
            Err(err) => {
                self.toast = Some(Toast::new(
                    ToastKind::Error,
                    "Inicialização automática",
                    err,
                ));
            }
        }
    }

    pub(crate) fn persist_settings(&self) {
        if let Err(err) = store::save(&self.paths.settings, &self.settings) {
            tracing::warn!(%err, "could not save settings");
        }
    }

    fn persist_history(&self) {
        if let Err(err) = store::save(&self.paths.history, &self.history) {
            tracing::warn!(%err, "could not save history");
        }
    }

    /// Saves the window position and quits.
    pub(crate) fn quit(&mut self, ctx: &egui::Context) {
        if let Some(pos) = self.window_pos {
            self.settings.window_position = Some([pos.x, pos.y]);
            self.persist_settings();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Whether something on screen is animating and needs frames.
    pub(crate) fn animating(&self) -> bool {
        matches!(self.state(), State::Recording(_) | State::Transcribing)
            || matches!(
                self.engine_status,
                EngineStatus::Downloading { .. } | EngineStatus::Loading
            )
    }
}

#[cfg(feature = "screenshot")]
impl FulltalkApp {
    /// Returns `true` when it drew the frame itself (settings scene).
    fn screenshot_tick(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::screenshot::Scene;
        let ctx = &ui.ctx().clone();
        let Some(mut shooter) = self.shooter.take() else {
            return false;
        };
        let mut ready = self.engine_status == EngineStatus::Ready;
        match shooter.scene {
            Scene::Settings => {
                // Secondary viewports cannot be captured; draw settings in the root instead.
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    crate::ui::SETTINGS_SIZE.into(),
                ));
            }
            Scene::Recording if ready && self.state() == State::Idle => {
                self.apply(Input::ButtonClicked);
            }
            Scene::Toast if ready && self.toast.is_none() => {
                self.toast = Some(delivered_toast(Ok(Outcome::Pasted)));
            }
            _ => {}
        }
        if shooter.scene == Scene::Toast {
            ready &= self
                .toast
                .as_ref()
                .is_some_and(|t| t.age() > Duration::from_millis(600));
        }
        shooter.tick(ctx, ready);
        let scene = shooter.scene;
        self.shooter = Some(shooter);
        if scene == Scene::Settings {
            crate::ui::widgets::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.settings_content(ui);
            });
            return true;
        }
        false
    }
}

impl eframe::App for FulltalkApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.pump_events();
        self.window_pos = ui.ctx().input(|i| i.viewport().outer_rect.map(|r| r.min));

        #[cfg(feature = "screenshot")]
        if self.screenshot_tick(ui) {
            return;
        }

        self.main_view(ui);
        if self.settings_open {
            self.settings_viewport(ui.ctx());
        }

        if self.animating() {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        } else if let Some(toast) = &self.toast {
            ui.ctx()
                .request_repaint_after(toast.remaining().min(Duration::from_millis(100)));
        }
        if self
            .copied
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_millis(1200))
        {
            self.copied = None;
        } else if self.copied.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4] // transparent: the card draws its own rounded background
    }
}

fn delivered_toast(result: Result<Outcome, String>) -> Toast {
    match result {
        Ok(Outcome::Pasted) => Toast::new(
            ToastKind::Success,
            "Transcrição pronta!",
            "Texto colado no campo ativo.",
        ),
        Ok(Outcome::Copied) => Toast::new(
            ToastKind::Success,
            "Transcrição pronta!",
            "Texto copiado para a área de transferência.",
        ),
        Ok(Outcome::CopiedPasteFailed(err)) => Toast::new(
            ToastKind::Warning,
            "Copiado, mas não colado",
            format!("{err}"),
        ),
        Err(err) => Toast::new(ToastKind::Error, "Não foi possível entregar o texto", err),
    }
}

fn notice_toast(notice: Notice) -> Toast {
    match notice {
        Notice::NothingHeard => Toast::new(
            ToastKind::Info,
            "Nada foi captado",
            "Segure o atalho enquanto fala.",
        ),
        Notice::Error(reason) => Toast::new(ToastKind::Error, "Algo deu errado", reason),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn make_window_non_activating(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::HasWindowHandle;
    match cc.window_handle() {
        Ok(handle) => fulltalk_platform::window::make_non_activating(handle.as_raw()),
        Err(err) => tracing::warn!(%err, "no window handle"),
    }
}
