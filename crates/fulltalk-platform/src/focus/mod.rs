//! "Is a text field focused right now?" — per-OS accessibility APIs.

use fulltalk_core::delivery::FocusTarget;
use fulltalk_core::ports::FocusProbe;

mod classify;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// A probe that always answers [`FocusTarget::Unknown`] (→ clipboard only).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFocusProbe;

impl FocusProbe for NoFocusProbe {
    fn focused(&self) -> FocusTarget {
        FocusTarget::Unknown
    }
}

/// The best focus probe available on this OS.
pub fn system_probe() -> Box<dyn FocusProbe + Send> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::AtspiFocusProbe::connect())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::UiaFocusProbe)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::AxFocusProbe)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Box::new(NoFocusProbe)
    }
}
