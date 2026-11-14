//! OS hotkey registration (Windows, macOS, Linux/X11) via `global-hotkey`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use fulltalk_core::shortcut::Shortcut;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use super::{HotkeyError, HotkeyEvent, HotkeyHandler};

pub(super) struct OsHotkeys {
    manager: GlobalHotKeyManager,
    current: HotKey,
    current_id: Arc<AtomicU32>,
}

fn to_hotkey(shortcut: Shortcut) -> Result<HotKey, HotkeyError> {
    // Our canonical format ("Ctrl+Shift+Space", "Super+F12") is understood by global-hotkey.
    shortcut
        .to_string()
        .parse()
        .map_err(|e| HotkeyError::Other(format!("{e}")))
}

fn register_error(shortcut: Shortcut, err: &global_hotkey::Error) -> HotkeyError {
    match err {
        global_hotkey::Error::AlreadyRegistered(_) | global_hotkey::Error::FailedToRegister(_) => {
            HotkeyError::Taken(shortcut.to_string())
        }
        other => HotkeyError::Other(other.to_string()),
    }
}

impl OsHotkeys {
    pub(super) fn start(shortcut: Shortcut, handler: HotkeyHandler) -> Result<Self, HotkeyError> {
        let manager = GlobalHotKeyManager::new().map_err(|e| HotkeyError::Other(e.to_string()))?;
        let current = to_hotkey(shortcut)?;
        manager
            .register(current)
            .map_err(|e| register_error(shortcut, &e))?;

        let current_id = Arc::new(AtomicU32::new(current.id()));
        let id = Arc::clone(&current_id);
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.id() != id.load(Ordering::Relaxed) {
                return;
            }
            handler(match event.state() {
                HotKeyState::Pressed => HotkeyEvent::Pressed,
                HotKeyState::Released => HotkeyEvent::Released,
            });
        }));
        Ok(Self {
            manager,
            current,
            current_id,
        })
    }

    pub(super) fn set_shortcut(&mut self, shortcut: Shortcut) -> Result<(), HotkeyError> {
        let next = to_hotkey(shortcut)?;
        if next == self.current {
            return Ok(());
        }
        self.manager
            .register(next)
            .map_err(|e| register_error(shortcut, &e))?;
        if let Err(err) = self.manager.unregister(self.current) {
            tracing::warn!(%err, "could not unregister previous shortcut");
        }
        self.current = next;
        self.current_id.store(next.id(), Ordering::Relaxed);
        Ok(())
    }
}
