//! Per-user file locations.

use std::path::PathBuf;

use anyhow::Context;
use directories::ProjectDirs;

#[derive(Debug, Clone)]
pub struct Paths {
    pub settings: PathBuf,
    pub history: PathBuf,
    pub models: PathBuf,
}

impl Paths {
    /// Linux: `~/.config/fulltalk`, `~/.local/share/fulltalk`.
    /// Windows: `%APPDATA%\fulltalk`. macOS: `~/Library/Application Support/dev.fulltalk.fulltalk`.
    ///
    /// `FULLTALK_HOME` relocates settings and history (handy for demos and
    /// manual testing); the large model stays in the shared data directory.
    pub fn resolve() -> anyhow::Result<Self> {
        let dirs = ProjectDirs::from("dev", "fulltalk", "fulltalk")
            .context("could not determine the home directory")?;
        let models = dirs.data_dir().join("models");
        if let Some(home) = std::env::var_os("FULLTALK_HOME").map(PathBuf::from) {
            return Ok(Self {
                settings: home.join("settings.json"),
                history: home.join("history.json"),
                models,
            });
        }
        Ok(Self {
            settings: dirs.config_dir().join("settings.json"),
            history: dirs.data_dir().join("history.json"),
            models,
        })
    }
}
