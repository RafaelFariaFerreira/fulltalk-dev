//! Global hold-to-talk shortcut.
//!
//! * Linux on Wayland: compositors do not let apps grab keys. Where the desktop
//!   offers the global-shortcuts portal (GNOME 48+, KDE Plasma) the shortcut is
//!   bound through it. Elsewhere we read the keyboards directly through evdev,
//!   which requires membership in the `input` group.
//! * Windows, macOS and Linux on X11: OS-level hotkey registration through
//!   the `global-hotkey` crate, which also reports key release.

use std::sync::Arc;

use fulltalk_core::shortcut::Shortcut;
use thiserror::Error;

#[cfg(target_os = "linux")]
mod evdev_backend;
mod os_backend;
#[cfg(target_os = "linux")]
mod portal_backend;

/// Edge of the shortcut being held, or the news that it will never fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
    /// The desktop refused to bind the shortcut (its prompt was declined).
    /// Reported after `start`, because that prompt is answered asynchronously.
    Denied,
}

/// Callback invoked from a background thread for every edge.
pub type HotkeyHandler = Arc<dyn Fn(HotkeyEvent) + Send + Sync>;

#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("Atalho global desativado: rode scripts/setup-linux.sh e entre na sessão de novo.")]
    NoKeyboardAccess,
    #[error("Atalho global não autorizado: aceite o pedido do sistema para usar o atalho.")]
    Denied,
    #[error("o atalho {0} já está em uso por outro aplicativo")]
    Taken(String),
    #[error("atalho global indisponível: {0}")]
    Other(String),
}

/// Which mechanism delivers the shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Portal,
    Evdev,
    Os,
}

/// A running global shortcut listener.
pub struct HotkeyService {
    inner: Inner,
}

enum Inner {
    #[cfg(target_os = "linux")]
    Portal(portal_backend::PortalHotkeys),
    #[cfg(target_os = "linux")]
    Evdev(evdev_backend::EvdevHotkeys),
    Os(os_backend::OsHotkeys),
}

impl std::fmt::Debug for HotkeyService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotkeyService")
            .field("backend", &self.backend())
            .finish()
    }
}

impl HotkeyService {
    /// Starts listening. Must be called on the main thread (macOS requirement).
    pub fn start(shortcut: Shortcut, handler: HotkeyHandler) -> Result<Self, HotkeyError> {
        #[cfg(target_os = "linux")]
        if crate::is_wayland() {
            match portal_backend::PortalHotkeys::start(shortcut, Arc::clone(&handler)) {
                Ok(h) => {
                    return Ok(Self {
                        inner: Inner::Portal(h),
                    });
                }
                Err(err) => {
                    tracing::info!(%err, "no global-shortcuts portal, reading keyboards instead");
                }
            }
            return evdev_backend::EvdevHotkeys::start(shortcut, handler).map(|h| Self {
                inner: Inner::Evdev(h),
            });
        }
        os_backend::OsHotkeys::start(shortcut, handler).map(|h| Self {
            inner: Inner::Os(h),
        })
    }

    pub fn set_shortcut(&mut self, shortcut: Shortcut) -> Result<(), HotkeyError> {
        match &mut self.inner {
            #[cfg(target_os = "linux")]
            Inner::Portal(h) => {
                h.set_shortcut(shortcut);
                Ok(())
            }
            #[cfg(target_os = "linux")]
            Inner::Evdev(h) => {
                h.set_shortcut(shortcut);
                Ok(())
            }
            Inner::Os(h) => h.set_shortcut(shortcut),
        }
    }

    pub const fn backend(&self) -> Backend {
        match self.inner {
            #[cfg(target_os = "linux")]
            Inner::Portal(_) => Backend::Portal,
            #[cfg(target_os = "linux")]
            Inner::Evdev(_) => Backend::Evdev,
            Inner::Os(_) => Backend::Os,
        }
    }

    /// A cheap, thread-safe handle telling whether a modifier is physically held.
    /// Synthetic paste waits on it so `Ctrl+V` is not sent as `Alt+Ctrl+V`.
    pub fn modifier_probe(&self) -> ModifierProbe {
        match &self.inner {
            #[cfg(target_os = "linux")]
            Inner::Evdev(h) => ModifierProbe(Some(h.tracker())),
            #[cfg(target_os = "linux")]
            Inner::Portal(_) => ModifierProbe(None),
            Inner::Os(_) => ModifierProbe(None),
        }
    }
}

/// Thread-safe view of held modifiers (always `false` when unknown).
#[derive(Clone, Default)]
pub struct ModifierProbe(Option<Arc<std::sync::Mutex<fulltalk_core::chord::ChordTracker>>>);

impl std::fmt::Debug for ModifierProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ModifierProbe")
            .field(&self.0.is_some())
            .finish()
    }
}

impl ModifierProbe {
    pub fn modifiers_held(&self) -> bool {
        self.0
            .as_ref()
            .and_then(|t| t.lock().ok().map(|t| t.any_modifier_held()))
            .unwrap_or(false)
    }
}
