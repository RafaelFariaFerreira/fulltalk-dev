//! Start on login (`~/.config/autostart` on Linux, Registry on Windows, Login Items on macOS).

use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use fulltalk_core::ports::PortError;

pub struct Autostart(AutoLaunch);

impl std::fmt::Debug for Autostart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Autostart")
    }
}

impl Autostart {
    /// Registers the currently running executable under `app_name`.
    pub fn for_current_exe(app_name: &str) -> Result<Self, PortError> {
        let exe = std::env::current_exe().map_err(|e| PortError::new(e.to_string()))?;
        let exe = exe.to_string_lossy();
        AutoLaunchBuilder::new()
            .set_app_name(app_name)
            .set_app_path(&exe)
            .build()
            .map(Self)
            .map_err(|e| PortError::new(format!("inicialização automática indisponível: {e}")))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), PortError> {
        let result = if enabled {
            self.0.enable()
        } else {
            self.0.disable()
        };
        result.map_err(|e| PortError::new(format!("não foi possível alterar a inicialização: {e}")))
    }

    pub fn is_enabled(&self) -> bool {
        self.0.is_enabled().unwrap_or(false)
    }
}
