//! System clipboard via `arboard`.

use fulltalk_core::ports::{Clipboard, PortError};

/// Keeps one `arboard` handle alive for the app's lifetime: on X11 the owning
/// process must stay around to serve paste requests.
pub struct SystemClipboard(arboard::Clipboard);

impl std::fmt::Debug for SystemClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SystemClipboard")
    }
}

impl SystemClipboard {
    pub fn new() -> Result<Self, PortError> {
        arboard::Clipboard::new()
            .map(Self)
            .map_err(|e| PortError::new(format!("área de transferência indisponível: {e}")))
    }
}

impl Clipboard for SystemClipboard {
    fn set_text(&mut self, text: &str) -> Result<(), PortError> {
        self.0
            .set_text(text)
            .map_err(|e| PortError::new(format!("não foi possível copiar: {e}")))
    }
}
