//! Developer tool (feature `screenshot`): renders the app, saves a PNG and exits.
//! Used to produce the README images reproducibly on any platform.
//!
//! ```sh
//! FULLTALK_HOME=/tmp/demo FULLTALK_SCREENSHOT=main.png \
//!   cargo run -p fulltalk --features screenshot
//! ```
//!
//! * `FULLTALK_SCREENSHOT` — output path (required).
//! * `FULLTALK_SCREENSHOT_SCENE` — `main` (default), `history`, `settings`,
//!   `recording`, `toast`, `mini` or `mini-recording`.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, ColorImage, ViewportCommand, ViewportId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Main,
    /// The main window with the recent transcriptions expanded.
    History,
    Settings,
    Recording,
    Toast,
    /// The minimized window: just the microphone.
    Mini,
    MiniRecording,
}

#[derive(Debug)]
pub struct Shooter {
    path: PathBuf,
    pub scene: Scene,
    started: Instant,
    requested: bool,
}

impl Shooter {
    pub fn from_env() -> Option<Self> {
        let path = PathBuf::from(std::env::var_os("FULLTALK_SCREENSHOT")?);
        let scene = match std::env::var("FULLTALK_SCREENSHOT_SCENE").as_deref() {
            Ok("history") => Scene::History,
            Ok("settings") => Scene::Settings,
            Ok("recording") => Scene::Recording,
            Ok("toast") => Scene::Toast,
            Ok("mini") => Scene::Mini,
            Ok("mini-recording") => Scene::MiniRecording,
            _ => Scene::Main,
        };
        Some(Self {
            path,
            scene,
            started: Instant::now(),
            requested: false,
        })
    }

    /// Requests the capture once the UI had time to settle.
    pub fn tick(&mut self, ctx: &egui::Context, ready: bool) {
        ctx.request_repaint_after(Duration::from_millis(50));
        if self.requested || !ready || self.started.elapsed() < Duration::from_millis(1800) {
            return;
        }
        self.requested = true;
        let path = self.path.clone();
        ctx.send_viewport_cmd_to(
            ViewportId::ROOT,
            ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
                match save_png(&path, &image) {
                    Ok(()) => println!("screenshot saved to {}", path.display()),
                    Err(err) => eprintln!("screenshot failed: {err}"),
                }
                std::process::exit(0);
            })),
        );
    }
}

fn save_png(path: &PathBuf, image: &Arc<ColorImage>) -> Result<(), Box<dyn std::error::Error>> {
    let [w, h] = image.size;
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_srgba_unmultiplied)
        .collect();
    writer.write_image_data(&bytes)?;
    Ok(())
}
