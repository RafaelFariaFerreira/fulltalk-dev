//! Synthetic paste keystrokes.
//!
//! * Linux on Wayland: a uinput virtual keyboard (the only input path that works
//!   for every Wayland client). Requires write access to `/dev/uinput`.
//! * Everything else: `enigo` (`SendInput` on Windows, `CGEvent` on macOS, `XTest` on X11).

use fulltalk_core::delivery::PasteChord;
use fulltalk_core::ports::{Paster, PortError};

/// Name of our uinput device — the evdev hotkey reader ignores it.
pub const VIRTUAL_KEYBOARD_NAME: &str = "fulltalk virtual keyboard";

/// Builds the paster for this session.
pub fn system_paster() -> Result<Box<dyn Paster + Send>, PortError> {
    #[cfg(target_os = "linux")]
    if crate::is_wayland() {
        return uinput::UinputPaster::new().map(|p| Box::new(p) as Box<dyn Paster + Send>);
    }
    enigo_backend::EnigoPaster::new().map(|p| Box::new(p) as Box<dyn Paster + Send>)
}

/// Keys of a chord, modifiers first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChordKey {
    Ctrl,
    Shift,
    Command,
    V,
}

pub(crate) const fn chord_keys(chord: PasteChord) -> &'static [ChordKey] {
    match chord {
        PasteChord::CtrlV => &[ChordKey::Ctrl, ChordKey::V],
        PasteChord::CtrlShiftV => &[ChordKey::Ctrl, ChordKey::Shift, ChordKey::V],
        PasteChord::CmdV => &[ChordKey::Command, ChordKey::V],
    }
}

mod enigo_backend {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    use super::{ChordKey, PasteChord, Paster, PortError, chord_keys};

    pub(super) struct EnigoPaster(Enigo);

    impl EnigoPaster {
        pub(super) fn new() -> Result<Self, PortError> {
            Enigo::new(&Settings::default())
                .map(Self)
                .map_err(|e| PortError::new(format!("não foi possível simular o teclado: {e}")))
        }
    }

    fn key(k: ChordKey) -> Key {
        match k {
            ChordKey::Ctrl => Key::Control,
            ChordKey::Shift => Key::Shift,
            ChordKey::Command => Key::Meta,
            ChordKey::V => Key::Unicode('v'),
        }
    }

    impl Paster for EnigoPaster {
        fn paste(&mut self, chord: PasteChord) -> Result<(), PortError> {
            let keys = chord_keys(chord);
            let fail = |e: enigo::InputError| PortError::new(format!("falha ao colar: {e}"));
            for k in keys {
                self.0.key(key(*k), Direction::Press).map_err(fail)?;
            }
            for k in keys.iter().rev() {
                self.0.key(key(*k), Direction::Release).map_err(fail)?;
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
mod uinput {
    use std::time::Duration;

    use evdev::uinput::VirtualDevice;
    use evdev::{AttributeSet, EventType, InputEvent, KeyCode};

    use super::{ChordKey, PasteChord, Paster, PortError, VIRTUAL_KEYBOARD_NAME, chord_keys};

    /// Gap between synthetic key events; some clients drop events that arrive
    /// in the same frame as the modifier press.
    const STEP: Duration = Duration::from_millis(8);

    pub(super) struct UinputPaster(VirtualDevice);

    impl UinputPaster {
        pub(super) fn new() -> Result<Self, PortError> {
            let keys: AttributeSet<KeyCode> = [
                KeyCode::KEY_LEFTCTRL,
                KeyCode::KEY_LEFTSHIFT,
                KeyCode::KEY_LEFTMETA,
                KeyCode::KEY_V,
            ]
            .into_iter()
            .collect();
            let device = VirtualDevice::builder()
                .and_then(|b| b.name(VIRTUAL_KEYBOARD_NAME).with_keys(&keys))
                .and_then(evdev::uinput::VirtualDeviceBuilder::build)
                .map_err(|e| {
                    PortError::new(format!(
                        "sem acesso a /dev/uinput ({e}). Rode scripts/setup-linux.sh para habilitar a colagem automática."
                    ))
                })?;
            Ok(Self(device))
        }

        fn send(&mut self, code: KeyCode, pressed: bool) -> Result<(), PortError> {
            let event = InputEvent::new(EventType::KEY.0, code.code(), i32::from(pressed));
            self.0
                .emit(&[event])
                .map_err(|e| PortError::new(format!("falha ao colar: {e}")))?;
            std::thread::sleep(STEP);
            Ok(())
        }
    }

    const fn code(k: ChordKey) -> KeyCode {
        match k {
            ChordKey::Ctrl => KeyCode::KEY_LEFTCTRL,
            ChordKey::Shift => KeyCode::KEY_LEFTSHIFT,
            ChordKey::Command => KeyCode::KEY_LEFTMETA,
            ChordKey::V => KeyCode::KEY_V,
        }
    }

    impl Paster for UinputPaster {
        fn paste(&mut self, chord: PasteChord) -> Result<(), PortError> {
            let keys = chord_keys(chord);
            for k in keys {
                self.send(code(*k), true)?;
            }
            for k in keys.iter().rev() {
                self.send(code(*k), false)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_press_modifiers_before_v() {
        assert_eq!(chord_keys(PasteChord::CtrlV), [ChordKey::Ctrl, ChordKey::V]);
        assert_eq!(
            chord_keys(PasteChord::CtrlShiftV),
            [ChordKey::Ctrl, ChordKey::Shift, ChordKey::V]
        );
        assert_eq!(
            chord_keys(PasteChord::CmdV),
            [ChordKey::Command, ChordKey::V]
        );
    }
}
