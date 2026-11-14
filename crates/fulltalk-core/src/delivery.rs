//! Decides how a finished transcription reaches the user: paste or clipboard only.

use crate::ports::{Clipboard, FocusProbe, Paster, PortError};

/// What currently has keyboard focus, as far as the accessibility APIs can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    /// An editable text field, text area or rich-text editor.
    TextInput,
    /// A terminal emulator.
    Terminal,
    /// The active window belongs to an application that keeps its widgets to
    /// itself, so there is no telling what has focus inside it. Chromium and
    /// Electron do this on Linux unless a screen reader is running.
    Opaque,
    /// Something that does not accept text (a button, the desktop, …).
    Other,
    /// Focus could not be determined.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
    MacOs,
}

impl Platform {
    /// The platform this binary was compiled for.
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The keystroke that pastes in the focused application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteChord {
    CtrlV,
    /// Linux terminals reserve `Ctrl+V` for "literal next character".
    CtrlShiftV,
    CmdV,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryPlan {
    /// Leave the text in the clipboard for the user to paste later.
    ClipboardOnly,
    /// Copy to the clipboard, then send this paste keystroke.
    Paste(PasteChord),
}

impl DeliveryPlan {
    pub const fn pastes(self) -> bool {
        matches!(self, Self::Paste(_))
    }
}

/// Picks the delivery plan. The text always goes to the clipboard; pasting
/// happens when a text input is known to have focus, never when something
/// else is known to have it.
///
/// An [`FocusTarget::Opaque`] window gets the benefit of the doubt: the user
/// asked for dictation while working in it, a paste that lands on no text
/// field does nothing there, and the text is in the clipboard either way.
pub const fn plan(target: FocusTarget, platform: Platform, auto_paste: bool) -> DeliveryPlan {
    if !auto_paste {
        return DeliveryPlan::ClipboardOnly;
    }
    let chord = match (target, platform) {
        (FocusTarget::Other | FocusTarget::Unknown, _) => return DeliveryPlan::ClipboardOnly,
        (_, Platform::MacOs) => PasteChord::CmdV,
        (FocusTarget::Terminal, Platform::Linux) => PasteChord::CtrlShiftV,
        (FocusTarget::TextInput | FocusTarget::Terminal | FocusTarget::Opaque, _) => {
            PasteChord::CtrlV
        }
    };
    DeliveryPlan::Paste(chord)
}

/// What actually happened to a transcription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pasted,
    Copied,
    /// Copied, but the paste keystroke failed (text is still in the clipboard).
    CopiedPasteFailed(PortError),
}

/// Runs the delivery use case: clipboard first (so the text is never lost),
/// then a paste keystroke when the focused target accepts text.
pub fn deliver(
    text: &str,
    clipboard: &mut dyn Clipboard,
    focus: &dyn FocusProbe,
    paster: &mut dyn Paster,
    platform: Platform,
    auto_paste: bool,
) -> Result<Outcome, PortError> {
    // Probe before touching the clipboard: some clipboard managers steal focus briefly.
    let target = if auto_paste {
        focus.focused()
    } else {
        FocusTarget::Unknown
    };
    clipboard.set_text(text)?;
    match plan(target, platform, auto_paste) {
        DeliveryPlan::ClipboardOnly => Ok(Outcome::Copied),
        DeliveryPlan::Paste(chord) => Ok(paster
            .paste(chord)
            .map_or_else(Outcome::CopiedPasteFailed, |()| Outcome::Pasted)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::cell::Cell;

    #[derive(Default)]
    struct FakeClipboard {
        text: Option<String>,
        fail: bool,
    }
    impl Clipboard for FakeClipboard {
        fn set_text(&mut self, text: &str) -> Result<(), PortError> {
            if self.fail {
                return Err(PortError::new("clipboard indisponível"));
            }
            self.text = Some(text.to_owned());
            Ok(())
        }
    }

    struct FakeFocus {
        target: FocusTarget,
        probed: Cell<bool>,
    }
    impl FakeFocus {
        fn new(target: FocusTarget) -> Self {
            Self {
                target,
                probed: Cell::new(false),
            }
        }
    }
    impl FocusProbe for FakeFocus {
        fn focused(&self) -> FocusTarget {
            self.probed.set(true);
            self.target
        }
    }

    #[derive(Default)]
    struct FakePaster {
        sent: Vec<PasteChord>,
        fail: bool,
    }
    impl Paster for FakePaster {
        fn paste(&mut self, chord: PasteChord) -> Result<(), PortError> {
            if self.fail {
                return Err(PortError::new("sem permissão"));
            }
            self.sent.push(chord);
            Ok(())
        }
    }

    #[test]
    fn pastes_into_a_focused_text_field() {
        let (mut clip, focus, mut paster) = (
            FakeClipboard::default(),
            FakeFocus::new(FocusTarget::TextInput),
            FakePaster::default(),
        );
        let outcome = deliver("olá", &mut clip, &focus, &mut paster, Platform::Linux, true);
        assert_eq!(outcome, Ok(Outcome::Pasted));
        assert_eq!(clip.text.as_deref(), Some("olá"));
        assert_eq!(paster.sent, [PasteChord::CtrlV]);
    }

    #[test]
    fn pastes_into_a_window_that_hides_its_widgets() {
        // Chrome does not say whether a text field has focus; it still gets the text.
        let (mut clip, focus, mut paster) = (
            FakeClipboard::default(),
            FakeFocus::new(FocusTarget::Opaque),
            FakePaster::default(),
        );
        let outcome = deliver("olá", &mut clip, &focus, &mut paster, Platform::Linux, true);
        assert_eq!(outcome, Ok(Outcome::Pasted));
        assert_eq!(clip.text.as_deref(), Some("olá"));
        // Not the terminal chord: nothing says this is a terminal.
        assert_eq!(paster.sent, [PasteChord::CtrlV]);
    }

    #[test]
    fn opaque_windows_use_each_platforms_paste_chord() {
        assert_eq!(
            plan(FocusTarget::Opaque, Platform::MacOs, true),
            DeliveryPlan::Paste(PasteChord::CmdV)
        );
        assert_eq!(
            plan(FocusTarget::Opaque, Platform::Windows, true),
            DeliveryPlan::Paste(PasteChord::CtrlV)
        );
        assert_eq!(
            plan(FocusTarget::Opaque, Platform::Linux, false),
            DeliveryPlan::ClipboardOnly
        );
    }

    #[test]
    fn unknown_focus_is_still_never_pasted_into() {
        // No active window could be found at all: nothing to trust.
        for platform in [Platform::Linux, Platform::Windows, Platform::MacOs] {
            assert_eq!(
                plan(FocusTarget::Unknown, platform, true),
                DeliveryPlan::ClipboardOnly
            );
        }
    }

    #[test]
    fn only_copies_when_nothing_editable_is_focused() {
        let (mut clip, focus, mut paster) = (
            FakeClipboard::default(),
            FakeFocus::new(FocusTarget::Other),
            FakePaster::default(),
        );
        let outcome = deliver(
            "olá",
            &mut clip,
            &focus,
            &mut paster,
            Platform::Windows,
            true,
        );
        assert_eq!(outcome, Ok(Outcome::Copied));
        assert_eq!(clip.text.as_deref(), Some("olá"));
        assert!(paster.sent.is_empty());
    }

    #[test]
    fn does_not_probe_focus_when_auto_paste_is_off() {
        let (mut clip, focus, mut paster) = (
            FakeClipboard::default(),
            FakeFocus::new(FocusTarget::TextInput),
            FakePaster::default(),
        );
        let outcome = deliver("x", &mut clip, &focus, &mut paster, Platform::MacOs, false);
        assert_eq!(outcome, Ok(Outcome::Copied));
        assert!(!focus.probed.get());
    }

    #[test]
    fn paste_failure_still_leaves_text_in_clipboard() {
        let (mut clip, focus, mut paster) = (
            FakeClipboard::default(),
            FakeFocus::new(FocusTarget::Terminal),
            FakePaster {
                fail: true,
                ..FakePaster::default()
            },
        );
        let outcome = deliver(
            "ls -la",
            &mut clip,
            &focus,
            &mut paster,
            Platform::Linux,
            true,
        );
        assert_eq!(
            outcome,
            Ok(Outcome::CopiedPasteFailed(PortError::new("sem permissão")))
        );
        assert_eq!(clip.text.as_deref(), Some("ls -la"));
    }

    #[test]
    fn clipboard_failure_is_an_error_and_skips_paste() {
        let (mut clip, focus, mut paster) = (
            FakeClipboard {
                fail: true,
                ..FakeClipboard::default()
            },
            FakeFocus::new(FocusTarget::TextInput),
            FakePaster::default(),
        );
        assert!(deliver("x", &mut clip, &focus, &mut paster, Platform::Linux, true).is_err());
        assert!(paster.sent.is_empty());
    }
    use FocusTarget::{Other, Terminal, TextInput, Unknown};
    use Platform::{Linux, MacOs, Windows};

    #[rstest]
    #[case(TextInput, Linux, DeliveryPlan::Paste(PasteChord::CtrlV))]
    #[case(TextInput, Windows, DeliveryPlan::Paste(PasteChord::CtrlV))]
    #[case(TextInput, MacOs, DeliveryPlan::Paste(PasteChord::CmdV))]
    #[case(Terminal, Linux, DeliveryPlan::Paste(PasteChord::CtrlShiftV))]
    #[case(Terminal, Windows, DeliveryPlan::Paste(PasteChord::CtrlV))]
    #[case(Terminal, MacOs, DeliveryPlan::Paste(PasteChord::CmdV))]
    #[case(Other, Linux, DeliveryPlan::ClipboardOnly)]
    #[case(Unknown, Windows, DeliveryPlan::ClipboardOnly)]
    fn pastes_only_into_text_targets(
        #[case] target: FocusTarget,
        #[case] platform: Platform,
        #[case] expected: DeliveryPlan,
    ) {
        assert_eq!(plan(target, platform, true), expected);
    }

    #[test]
    fn auto_paste_disabled_always_uses_clipboard() {
        assert_eq!(plan(TextInput, Linux, false), DeliveryPlan::ClipboardOnly);
        assert!(!plan(Terminal, MacOs, false).pastes());
    }
}
