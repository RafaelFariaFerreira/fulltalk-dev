//! Converts egui keyboard input into a [`Shortcut`] (used when recording a new
//! shortcut in the settings window).

use eframe::egui;
use fulltalk_core::shortcut::{Key, Modifier, Modifiers, Shortcut, ShortcutError};

/// Maps an egui key to our model; `None` for keys a shortcut cannot use.
pub fn key(key: egui::Key) -> Option<Key> {
    use egui::Key as K;
    let name = key.name();
    match key {
        K::Space => Some(Key::Space),
        K::Num0
        | K::Num1
        | K::Num2
        | K::Num3
        | K::Num4
        | K::Num5
        | K::Num6
        | K::Num7
        | K::Num8
        | K::Num9 => name.parse().ok().map(Key::Digit),
        _ if name.len() == 1 && name.chars().all(|c| c.is_ascii_alphabetic()) => name
            .chars()
            .next()
            .map(|c| Key::Letter(c.to_ascii_uppercase())),
        _ => name
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=24).contains(n))
            .map(Key::Function),
    }
}

pub fn modifiers(m: egui::Modifiers) -> Modifiers {
    let mut out = Modifiers::NONE;
    if m.ctrl {
        out = out.with(Modifier::Ctrl);
    }
    if m.alt {
        out = out.with(Modifier::Alt);
    }
    if m.shift {
        out = out.with(Modifier::Shift);
    }
    if m.mac_cmd {
        out = out.with(Modifier::Super);
    }
    out
}

/// Builds a shortcut from a key press, or `None` if the key is not usable.
pub fn shortcut(
    key_pressed: egui::Key,
    m: egui::Modifiers,
) -> Option<Result<Shortcut, ShortcutError>> {
    key(key_pressed).map(|k| Shortcut::new(modifiers(m), k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_digits_function_keys_and_space() {
        assert_eq!(key(egui::Key::S), Some(Key::Letter('S')));
        assert_eq!(key(egui::Key::Num7), Some(Key::Digit(7)));
        assert_eq!(key(egui::Key::F12), Some(Key::Function(12)));
        assert_eq!(key(egui::Key::Space), Some(Key::Space));
        assert_eq!(key(egui::Key::Enter), None);
        assert_eq!(key(egui::Key::Escape), None);
    }

    #[test]
    fn builds_shortcuts_with_modifiers() {
        let m = egui::Modifiers {
            alt: true,
            shift: true,
            ..Default::default()
        };
        let s = shortcut(egui::Key::T, m).unwrap().unwrap();
        assert_eq!(s.to_string(), "Alt+Shift+T");
    }

    #[test]
    fn plain_letters_are_rejected() {
        let s = shortcut(egui::Key::T, egui::Modifiers::NONE).unwrap();
        assert_eq!(s, Err(ShortcutError::NeedsModifier));
    }
}
