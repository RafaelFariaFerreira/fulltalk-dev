//! Background workers. The UI thread never blocks: model download/loading,
//! inference and delivery (focus probing, clipboard, paste) run here and report
//! back through [`AppEvent`]s.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use fulltalk_asr::model::{HttpFetch, ModelStore, PARAKEET_TDT_V3_INT8};
use fulltalk_asr::{EngineOptions, ParakeetEngine};
use fulltalk_core::delivery::{self, Outcome, Platform};
use fulltalk_core::ports::{FocusProbe, Paster, PortError, SpeechToText};
use fulltalk_platform::clipboard::SystemClipboard;
use fulltalk_platform::hotkey::{HotkeyEvent, ModifierProbe};

/// Everything that can wake the UI up.
#[derive(Debug)]
pub enum AppEvent {
    Hotkey(HotkeyEvent),
    Engine(EngineStatus),
    Transcribed(Result<String, String>),
    Delivered(Result<Outcome, String>),
    Copied(Result<(), String>),
    CaptureFailed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineStatus {
    Downloading { done: u64, total: u64 },
    Loading,
    Ready,
    Failed(String),
}

/// Sends events to the UI and wakes it up.
#[derive(Clone)]
pub struct Notifier {
    tx: Sender<AppEvent>,
    ctx: eframe::egui::Context,
}

impl std::fmt::Debug for Notifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Notifier")
    }
}

impl Notifier {
    pub fn new(ctx: eframe::egui::Context) -> (Self, Receiver<AppEvent>) {
        let (tx, rx) = mpsc::channel();
        (Self { tx, ctx }, rx)
    }

    pub fn send(&self, event: AppEvent) {
        let _ = self.tx.send(event);
        self.ctx.request_repaint();
    }
}

// ---------------------------------------------------------------------------
// Speech recognition
// ---------------------------------------------------------------------------

/// Handle to the inference thread. Jobs submitted before the model is ready
/// simply wait in the queue.
#[derive(Debug)]
pub struct Engine {
    jobs: Sender<Vec<f32>>,
}

impl Engine {
    pub fn spawn(models_root: PathBuf, notify: Notifier) -> Self {
        let (jobs, rx) = mpsc::channel::<Vec<f32>>();
        std::thread::Builder::new()
            .name("asr".into())
            .spawn(move || run_engine(&models_root, &notify, &rx))
            .expect("failed to spawn the ASR thread");
        Self { jobs }
    }

    pub fn transcribe(&self, samples: Vec<f32>) {
        let _ = self.jobs.send(samples);
    }
}

fn run_engine(models_root: &std::path::Path, notify: &Notifier, jobs: &Receiver<Vec<f32>>) {
    let status = |s| notify.send(AppEvent::Engine(s));
    let mut engine = match prepare_engine(models_root, &status) {
        Ok(engine) => {
            status(EngineStatus::Ready);
            Some(engine)
        }
        Err(err) => {
            tracing::error!(%err, "speech engine unavailable");
            status(EngineStatus::Failed(err.to_string()));
            None
        }
    };

    for samples in jobs {
        let result = match engine.as_mut() {
            Some(engine) => {
                let started = Instant::now();
                let result = engine.transcribe(&samples).map_err(|e| e.to_string());
                tracing::info!(
                    audio_s = samples.len() as f32 / 16_000.0,
                    took_ms = started.elapsed().as_millis(),
                    "transcribed"
                );
                result
            }
            None => Err("o modelo de voz não está disponível".to_owned()),
        };
        notify.send(AppEvent::Transcribed(result));
    }
}

fn prepare_engine(
    models_root: &std::path::Path,
    status: &impl Fn(EngineStatus),
) -> Result<ParakeetEngine, PortError> {
    let store = ModelStore::new(models_root, PARAKEET_TDT_V3_INT8);
    if !store.is_installed() {
        tracing::info!(dir = %store.dir().display(), "downloading speech model");
        let mut last_pct = u64::MAX;
        store
            .install(&HttpFetch, |done, total| {
                let pct = done * 100 / total.max(1);
                if pct != last_pct {
                    last_pct = pct;
                    status(EngineStatus::Downloading { done, total });
                }
            })
            .map_err(|e| PortError::new(e.to_string()))?;
    }
    status(EngineStatus::Loading);
    let started = Instant::now();
    let engine = ParakeetEngine::load(store.dir(), EngineOptions::default())?;
    tracing::info!(
        took_ms = started.elapsed().as_millis(),
        "speech model loaded"
    );
    Ok(engine)
}

// ---------------------------------------------------------------------------
// Delivery (clipboard / paste)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum DeliveryJob {
    /// A fresh transcription: paste when a text field has focus, else copy.
    Transcription { text: String, auto_paste: bool },
    /// A history item clicked by the user: copy only.
    Copy(String),
}

/// Owns the clipboard, focus probe and paster on a dedicated thread (AT-SPI
/// queries and the paste keystrokes take a few milliseconds each).
#[derive(Debug)]
pub struct Deliverer {
    jobs: Sender<DeliveryJob>,
}

/// How long to wait for the user to let go of the shortcut's modifiers.
const MODIFIER_RELEASE_TIMEOUT: Duration = Duration::from_millis(1500);

impl Deliverer {
    pub fn spawn(notify: Notifier, modifiers: ModifierProbe) -> Self {
        let (jobs, rx) = mpsc::channel::<DeliveryJob>();
        std::thread::Builder::new()
            .name("delivery".into())
            .spawn(move || run_delivery(&notify, &modifiers, &rx))
            .expect("failed to spawn the delivery thread");
        Self { jobs }
    }

    pub fn send(&self, job: DeliveryJob) {
        let _ = self.jobs.send(job);
    }
}

fn run_delivery(notify: &Notifier, modifiers: &ModifierProbe, jobs: &Receiver<DeliveryJob>) {
    use fulltalk_core::ports::Clipboard as _;

    let mut clipboard = SystemClipboard::new();
    let focus = fulltalk_platform::focus::system_probe();
    let mut paster: Box<dyn Paster + Send> = fulltalk_platform::paste::system_paster()
        .unwrap_or_else(|err| {
            tracing::warn!(%err, "automatic paste disabled");
            Box::new(UnavailablePaster(err))
        });

    for job in jobs {
        let Ok(clipboard) = clipboard.as_mut() else {
            let err = clipboard
                .as_ref()
                .err()
                .map_or_else(String::new, ToString::to_string);
            notify.send(AppEvent::Delivered(Err(err)));
            continue;
        };
        match job {
            DeliveryJob::Copy(text) => {
                let result = clipboard.set_text(&text).map_err(|e| e.to_string());
                notify.send(AppEvent::Copied(result));
            }
            DeliveryJob::Transcription { text, auto_paste } => {
                wait_for_modifier_release(modifiers);
                let result = delivery::deliver(
                    &text,
                    clipboard,
                    focus.as_ref() as &dyn FocusProbe,
                    paster.as_mut(),
                    Platform::current(),
                    auto_paste,
                )
                .map_err(|e| e.to_string());
                notify.send(AppEvent::Delivered(result));
            }
        }
    }
}

fn wait_for_modifier_release(modifiers: &ModifierProbe) {
    let deadline = Instant::now() + MODIFIER_RELEASE_TIMEOUT;
    while modifiers.modifiers_held() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Paster used when synthetic input is not permitted; explains why.
struct UnavailablePaster(PortError);

impl Paster for UnavailablePaster {
    fn paste(&mut self, _: fulltalk_core::delivery::PasteChord) -> Result<(), PortError> {
        Err(self.0.clone())
    }
}
