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
use fulltalk_platform::hotkey::{HotkeyError, HotkeyEvent, HotkeyService};
use fulltalk_platform::window::FloatingWindow;

use crate::audio::Recorder;
use crate::paths::Paths;
use crate::ui::toast::{Toast, ToastKind};
use crate::workers::{AppEvent, Deliverer, DeliveryJob, Engine, EngineStatus, Notifier};

pub const APP_NAME: &str = "fulltalk dev";
pub const SLOGAN: &str = "code is cheap, show me the talk";
pub const REPO_URL: &str = "https://github.com/RafaelFariaFerreira/fulltalk-dev";

/// A window move in progress: where the pointer and the window's top-left
/// corner were when it started, both in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WindowDrag {
    pointer: (i32, i32),
    window: [f32; 2],
}

impl WindowDrag {
    /// Where the window's corner belongs now that the pointer is at `pointer`.
    /// Anchored to the start of the drag, so lag in the window actually
    /// moving never feeds back into the result.
    fn window_at(self, pointer: (i32, i32)) -> [f32; 2] {
        [
            self.window[0] + (pointer.0 - self.pointer.0) as f32,
            self.window[1] + (pointer.1 - self.pointer.1) as f32,
        ]
    }
}

/// How often a painting app re-checks its floating window hints.
const FLOATING_CHECK_INTERVAL: Duration = Duration::from_millis(500);

/// Main window sizes, including the transparent shadow margin: just the
/// microphone, or with the recent transcriptions expanded below it.
const COMPACT_SIZE: [f32; 2] = [372.0, 372.0];
const EXPANDED_SIZE: [f32; 2] = [372.0, 604.0];
/// The minimized window: the microphone alone, with room for its halo.
const MINI_SIZE: [f32; 2] = [64.0, 64.0];
/// Gap between the minimized window and the screen corner.
const MINI_MARGIN: f32 = 12.0;

/// The window size for the given mode.
pub const fn window_size(settings: &Settings) -> [f32; 2] {
    match (settings.minimized, settings.show_history) {
        (true, _) => MINI_SIZE,
        (false, true) => EXPANDED_SIZE,
        (false, false) => COMPACT_SIZE,
    }
}

/// Top-left corner that puts a window of `size` in the top-right corner of
/// `area` (`[x, y, width, height]`), `margin` away from its edges.
fn top_right_corner(area: [f32; 4], size: [f32; 2], margin: f32) -> [f32; 2] {
    let [x, y, width, _] = area;
    [(x + width - size[0] - margin).max(x), y + margin]
}

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
    floating: Option<FloatingWindow>,
    floating_checked: Instant,
    window_drag: Option<WindowDrag>,
    /// Whether a window that started minimized was already moved to its corner.
    corner_placed: bool,
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
        let floating = floating_window(cc);

        let (notify, events) = Notifier::new(cc.egui_ctx.clone());

        let hotkey_notify = notify.clone();
        let handler = Arc::new(move |e: HotkeyEvent| hotkey_notify.send(AppEvent::Hotkey(e)));
        // README screenshots must look the same on any machine and must not
        // make the desktop prompt for a shortcut, so they run without one.
        let shooting =
            cfg!(feature = "screenshot") && std::env::var_os("FULLTALK_SCREENSHOT").is_some();
        let (hotkeys, hotkey_error) = if shooting {
            (None, None)
        } else {
            match HotkeyService::start(settings.shortcut, handler) {
                Ok(service) => (Some(service), None),
                Err(err) => {
                    tracing::warn!(%err, "global shortcut unavailable");
                    (None, Some(err.to_string()))
                }
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
            floating,
            floating_checked: Instant::now(),
            window_drag: None,
            corner_placed: false,
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
                AppEvent::Hotkey(HotkeyEvent::Denied) => {
                    self.hotkey_error = Some(HotkeyError::Denied.to_string());
                }
                AppEvent::Hotkey(event) if self.capturing_shortcut => {
                    tracing::debug!(?event, "shortcut ignored while a new one is captured");
                }
                AppEvent::Hotkey(HotkeyEvent::Pressed) => {
                    tracing::debug!(state = ?self.state(), "shortcut pressed");
                    self.hotkey_error = None; // it evidently works now
                    self.apply(Input::HotkeyPressed);
                }
                AppEvent::Hotkey(HotkeyEvent::Released) => {
                    tracing::debug!(state = ?self.state(), "shortcut released");
                    self.apply(Input::HotkeyReleased);
                }
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
                Effect::DiscardCapture => {
                    self.recording_since = None;
                    drop(self.recorder.stop());
                }
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
                if self.hotkeys.is_some() {
                    self.hotkey_error = None; // a new binding gets a fresh chance
                }
                self.settings.shortcut = shortcut;
                self.shortcut_error = None;
                self.persist_settings();
            }
            Err(err) => self.shortcut_error = Some(err),
        }
    }

    /// Expands or collapses the recent transcriptions, resizing the window to fit.
    pub(crate) fn set_history_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.settings.show_history = visible;
        self.persist_settings();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
            window_size(&self.settings).into(),
        ));
    }

    /// Shrinks the window to the microphone in the top-right corner of the
    /// screen, or brings the full window back to where it was.
    pub(crate) fn set_minimized(&mut self, ctx: &egui::Context, minimized: bool) {
        if minimized {
            // Remember where the full window lives before it leaves.
            if let Some(pos) = self.window_pos {
                self.settings.window_position = Some([pos.x, pos.y]);
            }
            self.settings_open = false;
            self.capturing_shortcut = false;
        }
        self.settings.minimized = minimized;
        self.persist_settings();
        self.window_drag = None;
        self.place_window(ctx);
    }

    /// Applies the size and position of the current mode.
    fn place_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
            window_size(&self.settings).into(),
        ));
        let position = if self.settings.minimized {
            Some(self.screen_corner(ctx))
        } else {
            self.settings.window_position
        };
        if let Some([x, y]) = position {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        }
    }

    /// Where the minimized window goes, in points: the top-right corner of
    /// the work area (so it clears panels and docks), or of the monitor when
    /// the platform does not report one.
    fn screen_corner(&self, ctx: &egui::Context) -> [f32; 2] {
        let scale = ctx.pixels_per_point();
        let area = self
            .floating
            .as_ref()
            .and_then(FloatingWindow::work_area)
            .map(|area| area.map(|v| v as f32 / scale))
            .or_else(|| {
                ctx.input(|i| i.viewport().monitor_size)
                    .map(|size| [0.0, 0.0, size.x, size.y])
            })
            .unwrap_or([0.0, 0.0, 1280.0, 720.0]);
        top_right_corner(area, MINI_SIZE, MINI_MARGIN)
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
        // While minimized the saved position stays that of the full window.
        if let Some(pos) = self.window_pos.filter(|_| !self.settings.minimized) {
            self.settings.window_position = Some([pos.x, pos.y]);
            self.persist_settings();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Moves the main window while its header is dragged.
    ///
    /// The window never takes focus, and the toolkit only asks the window
    /// manager to move a window that has it. So the window follows the
    /// pointer's position on the screen instead. Where the platform cannot
    /// report that position, the window manager's move is the fallback.
    pub(crate) fn drag_main_window(&mut self, ctx: &egui::Context, header: &egui::Response) {
        if !header.dragged() {
            self.window_drag = None;
            return;
        }
        let pointer = self
            .floating
            .as_ref()
            .and_then(FloatingWindow::pointer_position);
        let scale = ctx.pixels_per_point();
        if header.drag_started() {
            let window = ctx.input(|i| i.viewport().outer_rect.map(|r| r.min));
            // A drag is only recognised after the pointer has travelled a few
            // points. Anchor to where it went down, so the spot that was
            // grabbed stays under the pointer.
            let travelled =
                ctx.input(
                    |i| match (i.pointer.press_origin(), i.pointer.latest_pos()) {
                        (Some(origin), Some(now)) => (now - origin) * scale,
                        _ => egui::Vec2::ZERO,
                    },
                );
            match (pointer, window) {
                (Some(pointer), Some(window)) => {
                    self.window_drag = Some(WindowDrag {
                        pointer: (
                            pointer.0 - travelled.x.round() as i32,
                            pointer.1 - travelled.y.round() as i32,
                        ),
                        window: [window.x * scale, window.y * scale],
                    });
                }
                _ => ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag),
            }
        }
        if let (Some(drag), Some(pointer)) = (self.window_drag, pointer) {
            let [x, y] = drag.window_at(pointer);
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                x / scale,
                y / scale,
            )));
        }
    }

    /// Re-asserts "always on top, never focused" now and then. The first
    /// checks matter most: the window only becomes visible after its first
    /// frame, and that is when the window manager starts listening.
    fn keep_floating(&mut self) {
        if self.floating_checked.elapsed() >= FLOATING_CHECK_INTERVAL
            && let Some(floating) = &self.floating
        {
            floating.enforce();
            self.floating_checked = Instant::now();
        }
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
            Scene::History if !self.settings.show_history => {
                self.settings.show_history = true; // not persisted: demo profile
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    window_size(&self.settings).into(),
                ));
            }
            Scene::Mini | Scene::MiniRecording if !self.settings.minimized => {
                self.settings.minimized = true; // not persisted: demo profile
                self.corner_placed = true; // stay put, the capture is all that matters
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    window_size(&self.settings).into(),
                ));
            }
            Scene::Recording | Scene::MiniRecording if ready && self.state() == State::Idle => {
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
        self.keep_floating();

        #[cfg(feature = "screenshot")]
        if self.screenshot_tick(ui) {
            return;
        }

        if self.settings.minimized {
            // A window that starts minimized still has to be sent to its corner.
            if !self.corner_placed {
                self.corner_placed = true;
                self.place_window(ui.ctx());
            }
            self.mini_view(ui);
        } else {
            self.main_view(ui);
            if self.settings_open {
                self.settings_viewport(ui.ctx());
            }
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

fn floating_window(cc: &eframe::CreationContext<'_>) -> Option<FloatingWindow> {
    use raw_window_handle::HasWindowHandle;
    cc.window_handle()
        .inspect_err(|err| tracing::warn!(%err, "no window handle"))
        .ok()
        .map(|handle| FloatingWindow::new(handle.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_size_follows_the_mode() {
        let mut settings = Settings::default();
        assert_eq!(window_size(&settings), COMPACT_SIZE);
        settings.show_history = true;
        assert_eq!(window_size(&settings), EXPANDED_SIZE);
        // Minimized wins: the history is not shown there.
        settings.minimized = true;
        assert_eq!(window_size(&settings), MINI_SIZE);
    }

    #[test]
    fn minimized_window_sits_in_the_top_right_of_the_work_area() {
        // A 1920x1080 screen with a 32 px top bar and a 67 px dock on the left.
        let corner = top_right_corner([67.0, 32.0, 1853.0, 1048.0], [64.0, 64.0], 12.0);
        assert_eq!(corner, [1844.0, 44.0]);
    }

    #[test]
    fn minimized_window_never_leaves_a_tiny_work_area() {
        let corner = top_right_corner([100.0, 0.0, 50.0, 50.0], [64.0, 64.0], 12.0);
        assert_eq!(corner, [100.0, 12.0]);
    }

    #[test]
    fn dragged_window_follows_the_pointer() {
        let drag = WindowDrag {
            pointer: (816, 203),
            window: [636.0, 162.0],
        };
        assert_eq!(drag.window_at((816, 203)), [636.0, 162.0]);
        assert_eq!(drag.window_at((936, 263)), [756.0, 222.0]);
        assert_eq!(drag.window_at((716, 103)), [536.0, 62.0]);
    }

    #[test]
    fn dragged_window_position_does_not_drift() {
        // Each position derives from where the drag started, so revisiting a
        // pointer position always lands the window on the same spot.
        let drag = WindowDrag {
            pointer: (100, 100),
            window: [10.0, 20.0],
        };
        let there = drag.window_at((400, 50));
        let _ = drag.window_at((-30, 900));
        assert_eq!(drag.window_at((400, 50)), there);
    }
}
