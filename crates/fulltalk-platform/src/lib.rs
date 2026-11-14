//! # fulltalk-platform
//!
//! Operating-system adapters for **fulltalk dev**, implementing the ports of
//! [`fulltalk_core::ports`]:
//!
//! | Concern            | Linux (Wayland)        | Linux (X11)   | Windows        | macOS          |
//! |--------------------|------------------------|---------------|----------------|----------------|
//! | Hold-to-talk key   | evdev                  | global-hotkey | global-hotkey  | global-hotkey  |
//! | Paste keystroke    | uinput                 | enigo (XTest) | enigo          | enigo          |
//! | Focused text field | AT-SPI                 | AT-SPI        | UI Automation  | AX API         |
//! | Clipboard          | arboard (via XWayland) | arboard       | arboard        | arboard        |

pub mod autostart;
pub mod clipboard;
pub mod focus;
pub mod hotkey;
pub mod paste;
pub mod window;

/// Whether this is a Linux Wayland session (needs evdev/uinput instead of X11 APIs).
pub fn is_wayland() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland")))
}
