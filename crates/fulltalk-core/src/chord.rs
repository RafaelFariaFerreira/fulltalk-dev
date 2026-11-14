//! Push-to-talk chord detection from raw key events.
//!
//! Some platforms (Linux/Wayland via evdev) only give us a stream of raw key
//! presses and releases. [`ChordTracker`] turns that stream into
//! "shortcut held" / "shortcut released" edges, which is what hold-to-talk needs.

use std::collections::HashMap;

use crate::shortcut::{Key, Modifier, Modifiers, Shortcut};

/// What a physical key means to us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputKey {
    Modifier(Modifier),
    Key(Key),
    /// Any key we do not care about (still tracked: it breaks a chord).
    Other,
}

/// A raw key transition. `code` identifies the physical key (left and right
/// Alt have different codes but both map to [`Modifier::Alt`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: u32,
    pub key: InputKey,
    pub pressed: bool,
}

impl KeyEvent {
    pub const fn press(code: u32, key: InputKey) -> Self {
        Self {
            code,
            key,
            pressed: true,
        }
    }

    pub const fn release(code: u32, key: InputKey) -> Self {
        Self {
            code,
            key,
            pressed: false,
        }
    }
}

/// Edge emitted when the shortcut starts or stops being held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordEdge {
    Activated,
    Released,
}

/// Tracks pressed keys and reports when exactly the configured chord is held.
#[derive(Debug, Clone)]
pub struct ChordTracker {
    shortcut: Shortcut,
    pressed: HashMap<u32, InputKey>,
    active: bool,
}

impl ChordTracker {
    pub fn new(shortcut: Shortcut) -> Self {
        Self {
            shortcut,
            pressed: HashMap::new(),
            active: false,
        }
    }

    /// Changes the shortcut. Releases an active chord so callers never get stuck "recording".
    pub fn set_shortcut(&mut self, shortcut: Shortcut) -> Option<ChordEdge> {
        self.shortcut = shortcut;
        self.active.then(|| {
            self.active = false;
            ChordEdge::Released
        })
    }

    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// Whether any modifier is physically held — synthetic paste must wait for this
    /// to be false, otherwise `Ctrl+V` arrives as `Alt+Ctrl+V`.
    pub fn any_modifier_held(&self) -> bool {
        self.pressed
            .values()
            .any(|k| matches!(k, InputKey::Modifier(_)))
    }

    /// Feeds one key event and returns an edge when the chord state changes.
    pub fn handle(&mut self, event: KeyEvent) -> Option<ChordEdge> {
        if event.pressed {
            self.pressed.insert(event.code, event.key);
        } else {
            self.pressed.remove(&event.code);
        }

        let held = self.chord_held();
        // Only the main key completes a chord: modifiers first, then the key.
        let completes = event.pressed && event.key == InputKey::Key(self.shortcut.key());
        match (self.active, held) {
            (false, true) if completes => {
                self.active = true;
                Some(ChordEdge::Activated)
            }
            (true, false) => {
                self.active = false;
                Some(ChordEdge::Released)
            }
            _ => None,
        }
    }

    /// Exact match: required modifiers and key held, nothing else held.
    fn chord_held(&self) -> bool {
        let mut modifiers = Modifiers::NONE;
        let mut key_held = false;
        for key in self.pressed.values() {
            match *key {
                InputKey::Modifier(m) => modifiers = modifiers.with(m),
                InputKey::Key(k) if k == self.shortcut.key() => key_held = true,
                InputKey::Key(_) | InputKey::Other => return false,
            }
        }
        key_held && modifiers == self.shortcut.modifiers()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT_ALT: u32 = 56;
    const RIGHT_ALT: u32 = 100;
    const LEFT_CTRL: u32 = 29;
    const S: u32 = 31;
    const T: u32 = 20;

    fn alt() -> InputKey {
        InputKey::Modifier(Modifier::Alt)
    }
    fn ctrl() -> InputKey {
        InputKey::Modifier(Modifier::Ctrl)
    }
    fn letter(c: char) -> InputKey {
        InputKey::Key(Key::Letter(c))
    }

    fn tracker() -> ChordTracker {
        ChordTracker::new("Alt+S".parse().unwrap())
    }

    #[test]
    fn activates_when_chord_is_pressed_and_releases_when_key_goes_up() {
        let mut t = tracker();
        assert_eq!(t.handle(KeyEvent::press(LEFT_ALT, alt())), None);
        assert_eq!(
            t.handle(KeyEvent::press(S, letter('S'))),
            Some(ChordEdge::Activated)
        );
        assert!(t.is_active());
        assert_eq!(
            t.handle(KeyEvent::release(S, letter('S'))),
            Some(ChordEdge::Released)
        );
    }

    #[test]
    fn releasing_the_modifier_first_also_ends_the_chord() {
        let mut t = tracker();
        t.handle(KeyEvent::press(LEFT_ALT, alt()));
        t.handle(KeyEvent::press(S, letter('S')));
        assert_eq!(
            t.handle(KeyEvent::release(LEFT_ALT, alt())),
            Some(ChordEdge::Released)
        );
        // Letting go of S afterwards is not a second release.
        assert_eq!(t.handle(KeyEvent::release(S, letter('S'))), None);
    }

    #[test]
    fn key_before_modifier_does_not_activate() {
        // Typing "s" then reaching for Alt is not a deliberate chord.
        let mut t = tracker();
        t.handle(KeyEvent::press(S, letter('S')));
        assert_eq!(t.handle(KeyEvent::press(LEFT_ALT, alt())), None);
        assert!(!t.is_active());
    }

    #[test]
    fn extra_modifier_prevents_activation() {
        let mut t = tracker();
        t.handle(KeyEvent::press(LEFT_CTRL, ctrl()));
        t.handle(KeyEvent::press(LEFT_ALT, alt()));
        assert_eq!(t.handle(KeyEvent::press(S, letter('S'))), None);
    }

    #[test]
    fn extra_key_while_held_releases_the_chord() {
        let mut t = tracker();
        t.handle(KeyEvent::press(LEFT_ALT, alt()));
        t.handle(KeyEvent::press(S, letter('S')));
        assert_eq!(
            t.handle(KeyEvent::press(T, letter('T'))),
            Some(ChordEdge::Released)
        );
    }

    #[test]
    fn either_side_of_a_modifier_counts() {
        let mut t = tracker();
        t.handle(KeyEvent::press(RIGHT_ALT, alt()));
        assert_eq!(
            t.handle(KeyEvent::press(S, letter('S'))),
            Some(ChordEdge::Activated)
        );
    }

    #[test]
    fn releasing_one_of_two_held_alts_keeps_the_chord() {
        let mut t = tracker();
        t.handle(KeyEvent::press(LEFT_ALT, alt()));
        t.handle(KeyEvent::press(RIGHT_ALT, alt()));
        t.handle(KeyEvent::press(S, letter('S')));
        assert_eq!(t.handle(KeyEvent::release(RIGHT_ALT, alt())), None);
        assert!(t.is_active());
    }

    #[test]
    fn reports_held_modifiers_for_safe_pasting() {
        let mut t = tracker();
        t.handle(KeyEvent::press(LEFT_ALT, alt()));
        t.handle(KeyEvent::press(S, letter('S')));
        t.handle(KeyEvent::release(S, letter('S')));
        assert!(t.any_modifier_held());
        t.handle(KeyEvent::release(LEFT_ALT, alt()));
        assert!(!t.any_modifier_held());
    }

    #[test]
    fn changing_shortcut_while_active_emits_release() {
        let mut t = tracker();
        t.handle(KeyEvent::press(LEFT_ALT, alt()));
        t.handle(KeyEvent::press(S, letter('S')));
        assert_eq!(
            t.set_shortcut("Ctrl+T".parse().unwrap()),
            Some(ChordEdge::Released)
        );
        assert!(!t.is_active());
    }

    #[test]
    fn function_key_without_modifiers_works() {
        let mut t = ChordTracker::new("F9".parse().unwrap());
        assert_eq!(
            t.handle(KeyEvent::press(67, InputKey::Key(Key::Function(9)))),
            Some(ChordEdge::Activated)
        );
    }
}
