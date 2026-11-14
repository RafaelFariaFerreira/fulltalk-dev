//! Raw keyboard reading through evdev (Linux, any display server).
//!
//! Reads are passive: keys still reach the focused application. One thread per
//! keyboard blocks on its device; a light watcher picks up hot-plugged keyboards.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use evdev::{Device, EventSummary, KeyCode};
use fulltalk_core::chord::{ChordEdge, ChordTracker, InputKey, KeyEvent};
use fulltalk_core::shortcut::{Key, Modifier, Shortcut};

use super::{HotkeyError, HotkeyEvent, HotkeyHandler};
use crate::paste::VIRTUAL_KEYBOARD_NAME;

const RESCAN_INTERVAL: Duration = Duration::from_secs(4);

type Shared = Arc<Mutex<ChordTracker>>;

pub(super) struct EvdevHotkeys {
    tracker: Shared,
    handler: HotkeyHandler,
}

impl EvdevHotkeys {
    pub(super) fn start(shortcut: Shortcut, handler: HotkeyHandler) -> Result<Self, HotkeyError> {
        let tracker: Shared = Arc::new(Mutex::new(ChordTracker::new(shortcut)));
        let opened: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();

        let started = open_new_keyboards(&tracker, &handler, &opened);
        if started == 0 {
            return Err(HotkeyError::NoKeyboardAccess);
        }
        tracing::info!(keyboards = started, "listening for shortcut via evdev");

        let (t, h, o) = (
            Arc::clone(&tracker),
            Arc::clone(&handler),
            Arc::clone(&opened),
        );
        let _ = std::thread::Builder::new()
            .name("evdev-hotplug".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(RESCAN_INTERVAL);
                    open_new_keyboards(&t, &h, &o);
                }
            });
        Ok(Self { tracker, handler })
    }

    pub(super) fn set_shortcut(&self, shortcut: Shortcut) {
        let edge = self
            .tracker
            .lock()
            .ok()
            .and_then(|mut t| t.set_shortcut(shortcut));
        if edge == Some(ChordEdge::Released) {
            (self.handler)(HotkeyEvent::Released);
        }
    }

    pub(super) fn tracker(&self) -> Shared {
        Arc::clone(&self.tracker)
    }
}

/// Opens keyboards not opened yet; returns how many reader threads were started.
fn open_new_keyboards(
    tracker: &Shared,
    handler: &HotkeyHandler,
    opened: &Mutex<HashSet<PathBuf>>,
) -> usize {
    let Ok(mut opened) = opened.lock() else {
        return 0;
    };
    let mut started = 0;
    for (path, device) in evdev::enumerate() {
        if opened.contains(&path) || !is_keyboard(&device) {
            continue;
        }
        let name = device.name().unwrap_or("?").to_owned();
        let (t, h, p) = (Arc::clone(tracker), Arc::clone(handler), path.clone());
        let spawned = std::thread::Builder::new()
            .name(format!("evdev-{name}"))
            .spawn(move || read_device(device, &t, &h, &p));
        if spawned.is_ok() {
            tracing::debug!(%name, path = %path.display(), "watching keyboard");
            opened.insert(path);
            started += 1;
        }
    }
    started
}

fn is_keyboard(device: &Device) -> bool {
    if device
        .name()
        .is_some_and(|n| n.starts_with(VIRTUAL_KEYBOARD_NAME))
    {
        return false; // our own synthetic paste device
    }
    device.supported_keys().is_some_and(|keys| {
        keys.contains(KeyCode::KEY_A)
            && keys.contains(KeyCode::KEY_Z)
            && keys.contains(KeyCode::KEY_LEFTALT)
    })
}

fn read_device(mut device: Device, tracker: &Shared, handler: &HotkeyHandler, path: &Path) {
    loop {
        let events = match device.fetch_events() {
            Ok(events) => events,
            Err(err) => {
                tracing::debug!(%err, path = %path.display(), "keyboard went away");
                return;
            }
        };
        for event in events {
            // value: 0 = release, 1 = press, 2 = autorepeat (ignored).
            let EventSummary::Key(_, code, value @ 0..=1) = event.destructure() else {
                continue;
            };
            let key_event = KeyEvent {
                code: u32::from(code.code()),
                key: map_key(code),
                pressed: value == 1,
            };
            let edge = tracker.lock().ok().and_then(|mut t| t.handle(key_event));
            match edge {
                Some(ChordEdge::Activated) => handler(HotkeyEvent::Pressed),
                Some(ChordEdge::Released) => handler(HotkeyEvent::Released),
                None => {}
            }
        }
    }
}

/// Maps a physical evdev key (US positions) to our key model.
pub(crate) fn map_key(code: KeyCode) -> InputKey {
    const LETTERS: [(KeyCode, char); 26] = [
        (KeyCode::KEY_A, 'A'),
        (KeyCode::KEY_B, 'B'),
        (KeyCode::KEY_C, 'C'),
        (KeyCode::KEY_D, 'D'),
        (KeyCode::KEY_E, 'E'),
        (KeyCode::KEY_F, 'F'),
        (KeyCode::KEY_G, 'G'),
        (KeyCode::KEY_H, 'H'),
        (KeyCode::KEY_I, 'I'),
        (KeyCode::KEY_J, 'J'),
        (KeyCode::KEY_K, 'K'),
        (KeyCode::KEY_L, 'L'),
        (KeyCode::KEY_M, 'M'),
        (KeyCode::KEY_N, 'N'),
        (KeyCode::KEY_O, 'O'),
        (KeyCode::KEY_P, 'P'),
        (KeyCode::KEY_Q, 'Q'),
        (KeyCode::KEY_R, 'R'),
        (KeyCode::KEY_S, 'S'),
        (KeyCode::KEY_T, 'T'),
        (KeyCode::KEY_U, 'U'),
        (KeyCode::KEY_V, 'V'),
        (KeyCode::KEY_W, 'W'),
        (KeyCode::KEY_X, 'X'),
        (KeyCode::KEY_Y, 'Y'),
        (KeyCode::KEY_Z, 'Z'),
    ];
    const DIGITS: [KeyCode; 10] = [
        KeyCode::KEY_0,
        KeyCode::KEY_1,
        KeyCode::KEY_2,
        KeyCode::KEY_3,
        KeyCode::KEY_4,
        KeyCode::KEY_5,
        KeyCode::KEY_6,
        KeyCode::KEY_7,
        KeyCode::KEY_8,
        KeyCode::KEY_9,
    ];
    const FUNCTION: [KeyCode; 24] = [
        KeyCode::KEY_F1,
        KeyCode::KEY_F2,
        KeyCode::KEY_F3,
        KeyCode::KEY_F4,
        KeyCode::KEY_F5,
        KeyCode::KEY_F6,
        KeyCode::KEY_F7,
        KeyCode::KEY_F8,
        KeyCode::KEY_F9,
        KeyCode::KEY_F10,
        KeyCode::KEY_F11,
        KeyCode::KEY_F12,
        KeyCode::KEY_F13,
        KeyCode::KEY_F14,
        KeyCode::KEY_F15,
        KeyCode::KEY_F16,
        KeyCode::KEY_F17,
        KeyCode::KEY_F18,
        KeyCode::KEY_F19,
        KeyCode::KEY_F20,
        KeyCode::KEY_F21,
        KeyCode::KEY_F22,
        KeyCode::KEY_F23,
        KeyCode::KEY_F24,
    ];

    match code {
        KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => InputKey::Modifier(Modifier::Ctrl),
        KeyCode::KEY_LEFTALT | KeyCode::KEY_RIGHTALT => InputKey::Modifier(Modifier::Alt),
        KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => InputKey::Modifier(Modifier::Shift),
        KeyCode::KEY_LEFTMETA | KeyCode::KEY_RIGHTMETA => InputKey::Modifier(Modifier::Super),
        KeyCode::KEY_SPACE => InputKey::Key(Key::Space),
        other => {
            if let Some((_, c)) = LETTERS.iter().find(|(k, _)| *k == other) {
                InputKey::Key(Key::Letter(*c))
            } else if let Some(d) = DIGITS.iter().position(|k| *k == other) {
                InputKey::Key(Key::Digit(d as u8))
            } else if let Some(f) = FUNCTION.iter().position(|k| *k == other) {
                InputKey::Key(Key::Function(f as u8 + 1))
            } else {
                InputKey::Other
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_modifiers_on_both_sides() {
        assert_eq!(
            map_key(KeyCode::KEY_LEFTALT),
            InputKey::Modifier(Modifier::Alt)
        );
        assert_eq!(
            map_key(KeyCode::KEY_RIGHTALT),
            InputKey::Modifier(Modifier::Alt)
        );
        assert_eq!(
            map_key(KeyCode::KEY_RIGHTMETA),
            InputKey::Modifier(Modifier::Super)
        );
    }

    #[test]
    fn maps_letters_digits_and_function_keys() {
        assert_eq!(map_key(KeyCode::KEY_S), InputKey::Key(Key::Letter('S')));
        assert_eq!(map_key(KeyCode::KEY_0), InputKey::Key(Key::Digit(0)));
        assert_eq!(map_key(KeyCode::KEY_7), InputKey::Key(Key::Digit(7)));
        assert_eq!(map_key(KeyCode::KEY_F1), InputKey::Key(Key::Function(1)));
        assert_eq!(map_key(KeyCode::KEY_F24), InputKey::Key(Key::Function(24)));
        assert_eq!(map_key(KeyCode::KEY_SPACE), InputKey::Key(Key::Space));
    }

    #[test]
    fn unknown_keys_are_other() {
        assert_eq!(map_key(KeyCode::KEY_ENTER), InputKey::Other);
    }

    #[test]
    fn alt_s_drives_the_tracker_end_to_end() {
        let mut tracker = ChordTracker::new(Shortcut::default());
        let ev = |code: KeyCode, pressed| KeyEvent {
            code: u32::from(code.code()),
            key: map_key(code),
            pressed,
        };
        assert_eq!(tracker.handle(ev(KeyCode::KEY_LEFTALT, true)), None);
        assert_eq!(
            tracker.handle(ev(KeyCode::KEY_S, true)),
            Some(ChordEdge::Activated)
        );
        assert_eq!(
            tracker.handle(ev(KeyCode::KEY_S, false)),
            Some(ChordEdge::Released)
        );
    }
}
