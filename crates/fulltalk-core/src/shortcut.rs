//! Keyboard shortcuts (e.g. `Alt+S`): parsing, formatting and validation.
//!
//! A [`Shortcut`] is a set of modifiers plus exactly one main [`Key`]. It
//! serializes as its human-readable form so the settings file stays editable.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A modifier key. Left and right variants are treated as the same modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    /// Windows key on Windows/Linux, Command on macOS.
    Super,
}

impl Modifier {
    /// All modifiers in canonical display order.
    pub const ALL: [Modifier; 4] = [Self::Ctrl, Self::Alt, Self::Shift, Self::Super];

    fn label(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Super => "Super",
        }
    }

    fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "ctl" => Some(Self::Ctrl),
            "alt" | "option" | "opt" => Some(Self::Alt),
            "shift" => Some(Self::Shift),
            "super" | "win" | "windows" | "meta" | "cmd" | "command" => Some(Self::Super),
            _ => None,
        }
    }
}

/// A set of modifiers, stored as flags so it is `Copy` and cheap to compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);

    const fn bit(modifier: Modifier) -> u8 {
        match modifier {
            Modifier::Ctrl => 1,
            Modifier::Alt => 1 << 1,
            Modifier::Shift => 1 << 2,
            Modifier::Super => 1 << 3,
        }
    }

    #[must_use]
    pub const fn with(self, modifier: Modifier) -> Self {
        Self(self.0 | Self::bit(modifier))
    }

    pub const fn contains(self, modifier: Modifier) -> bool {
        self.0 & Self::bit(modifier) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Iterates the modifiers in canonical order (Ctrl, Alt, Shift, Super).
    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL.into_iter().filter(move |m| self.contains(*m))
    }
}

impl FromIterator<Modifier> for Modifiers {
    fn from_iter<I: IntoIterator<Item = Modifier>>(iter: I) -> Self {
        iter.into_iter().fold(Self::NONE, Self::with)
    }
}

/// The main (non-modifier) key of a shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// `A`–`Z`, stored uppercase.
    Letter(char),
    /// `0`–`9`.
    Digit(u8),
    /// `F1`–`F24`.
    Function(u8),
    Space,
}

impl Key {
    /// Whether the key is safe to use without modifiers (it does not type text).
    pub const fn is_function(self) -> bool {
        matches!(self, Self::Function(_))
    }

    fn parse(token: &str) -> Option<Self> {
        let mut chars = token.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_alphabetic() => {
                return Some(Self::Letter(c.to_ascii_uppercase()));
            }
            (Some(c), None) if c.is_ascii_digit() => {
                return c.to_digit(10).map(|d| Self::Digit(d as u8));
            }
            _ => {}
        }
        let lower = token.to_ascii_lowercase();
        if lower == "space" {
            return Some(Self::Space);
        }
        let n: u8 = lower.strip_prefix('f')?.parse().ok()?;
        (1..=24).contains(&n).then_some(Self::Function(n))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Letter(c) => write!(f, "{c}"),
            Self::Digit(d) => write!(f, "{d}"),
            Self::Function(n) => write!(f, "F{n}"),
            Self::Space => f.write_str("Space"),
        }
    }
}

/// Why a shortcut string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShortcutError {
    #[error("o atalho está vazio")]
    Empty,
    #[error("tecla desconhecida: {0}")]
    UnknownKey(String),
    #[error("o atalho precisa de uma tecla principal (ex.: Alt+S)")]
    MissingKey,
    #[error("o atalho só pode ter uma tecla principal")]
    MultipleKeys,
    #[error("use pelo menos um modificador (Ctrl, Alt, Shift ou Super) com essa tecla")]
    NeedsModifier,
}

/// A global shortcut such as `Alt+S` or `Ctrl+Shift+Space`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Shortcut {
    modifiers: Modifiers,
    key: Key,
}

impl Shortcut {
    /// Builds a shortcut, rejecting combinations that would hijack normal typing.
    pub fn new(modifiers: Modifiers, key: Key) -> Result<Self, ShortcutError> {
        if modifiers.is_empty() && !key.is_function() {
            return Err(ShortcutError::NeedsModifier);
        }
        Ok(Self { modifiers, key })
    }

    pub const fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub const fn key(&self) -> Key {
        self.key
    }

    /// Labels for rendering key caps, e.g. `["Alt", "S"]`.
    pub fn labels(&self) -> Vec<String> {
        self.modifiers
            .iter()
            .map(|m| m.label().to_owned())
            .chain(std::iter::once(self.key.to_string()))
            .collect()
    }
}

impl Default for Shortcut {
    /// `Alt+S` — "S" de "speak".
    fn default() -> Self {
        Self {
            modifiers: Modifiers::NONE.with(Modifier::Alt),
            key: Key::Letter('S'),
        }
    }
}

impl FromStr for Shortcut {
    type Err = ShortcutError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(ShortcutError::Empty);
        }
        let mut modifiers = Modifiers::NONE;
        let mut key = None;
        for token in s.split('+').map(str::trim) {
            if let Some(modifier) = Modifier::parse(token) {
                modifiers = modifiers.with(modifier);
            } else if let Some(parsed) = Key::parse(token) {
                if key.replace(parsed).is_some() {
                    return Err(ShortcutError::MultipleKeys);
                }
            } else {
                return Err(ShortcutError::UnknownKey(token.to_owned()));
            }
        }
        Self::new(modifiers, key.ok_or(ShortcutError::MissingKey)?)
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.labels().join("+"))
    }
}

impl TryFrom<String> for Shortcut {
    type Error = ShortcutError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Shortcut> for String {
    fn from(value: Shortcut) -> Self {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn default_is_alt_s() {
        assert_eq!(Shortcut::default().to_string(), "Alt+S");
    }

    #[rstest]
    #[case("Alt+S", "Alt+S")]
    #[case("alt+s", "Alt+S")]
    #[case(" ctrl + shift + space ", "Ctrl+Shift+Space")]
    #[case("Shift+Ctrl+1", "Ctrl+Shift+1")]
    #[case("cmd+option+t", "Alt+Super+T")]
    #[case("F9", "F9")]
    #[case("Win+F12", "Super+F12")]
    fn parses_and_formats_canonically(#[case] input: &str, #[case] expected: &str) {
        let shortcut: Shortcut = input.parse().unwrap();
        assert_eq!(shortcut.to_string(), expected);
    }

    #[rstest]
    #[case("", ShortcutError::Empty)]
    #[case("Alt", ShortcutError::MissingKey)]
    #[case("Alt+S+T", ShortcutError::MultipleKeys)]
    #[case("Alt+Enter", ShortcutError::UnknownKey("Enter".into()))]
    #[case("S", ShortcutError::NeedsModifier)]
    #[case("F25", ShortcutError::UnknownKey("F25".into()))]
    fn rejects_invalid_shortcuts(#[case] input: &str, #[case] expected: ShortcutError) {
        assert_eq!(input.parse::<Shortcut>(), Err(expected));
    }

    #[test]
    fn labels_are_ready_for_key_caps() {
        let shortcut: Shortcut = "Ctrl+Alt+K".parse().unwrap();
        assert_eq!(shortcut.labels(), ["Ctrl", "Alt", "K"]);
    }

    #[test]
    fn serializes_as_human_readable_string() {
        let shortcut: Shortcut = "Ctrl+Shift+V".parse().unwrap();
        let json = serde_json::to_string(&shortcut).unwrap();
        assert_eq!(json, "\"Ctrl+Shift+V\"");
        assert_eq!(serde_json::from_str::<Shortcut>(&json).unwrap(), shortcut);
    }

    #[test]
    fn deserializing_an_invalid_string_fails() {
        assert!(serde_json::from_str::<Shortcut>("\"Q\"").is_err());
    }
}
