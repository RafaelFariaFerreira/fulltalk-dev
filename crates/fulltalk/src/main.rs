//! # fulltalk dev
//!
//! *code is cheap, show me the talk.*
//!
//! A tiny always-on-top window that turns speech into text, locally. Click the
//! mic or hold the shortcut (default `Alt+S`), talk, let go: the transcription
//! is pasted into the focused text field/terminal, or left in the clipboard.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app;
mod audio;
mod keymap;
mod memory;
mod paths;
#[cfg(feature = "screenshot")]
mod screenshot;
mod ui;
mod workers;

use eframe::egui::ViewportBuilder;
use fulltalk_core::history::History;
use fulltalk_core::settings::Settings;
use fulltalk_core::store;

use crate::app::{APP_NAME, FulltalkApp, window_size};
use crate::paths::Paths;

fn main() -> anyhow::Result<()> {
    memory::tune_allocator();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("FULLTALK_LOG")
                .unwrap_or_else(|_| "info,fulltalk=debug".into()),
        )
        .init();

    let paths = Paths::resolve()?;
    let settings: Settings = store::load_or_default(&paths.settings);
    let history = store::load_or_default::<History>(&paths.history).normalized();

    let mut viewport = ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_app_id("fulltalk")
        .with_inner_size(window_size(&settings))
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_icon(ui::icon::icon_data(128));
    // A minimized window is sent to its screen corner on the first frame.
    if let Some([x, y]) = settings.window_position.filter(|_| !settings.minimized) {
        viewport = viewport.with_position([x, y]);
    }

    let options = eframe::NativeOptions {
        viewport,
        // Wayland compositors do not let clients stay on top or avoid taking
        // focus; `XWayland` honours both, so the window uses X11 on Linux.
        event_loop_builder: Some(Box::new(|builder| {
            #[cfg(target_os = "linux")]
            {
                use winit::platform::x11::EventLoopBuilderExtX11;
                builder.with_x11();
            }
            #[cfg(not(target_os = "linux"))]
            let _ = builder;
        })),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(FulltalkApp::new(cc, paths, settings, history)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
