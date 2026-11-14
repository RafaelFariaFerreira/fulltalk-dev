//! The dictation session state machine.
//!
//! The UI and platform layers translate user input into [`Input`]s; the session
//! answers with [`Effect`]s to execute. Keeping this pure makes every rule of
//! the push-to-talk flow unit-testable without audio, models or windows.

use crate::text;

/// What started the current recording — decides what is allowed to stop it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Click to start, click again to stop.
    Button,
    /// Recording lasts while the shortcut is held.
    Hotkey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Idle,
    Recording(Trigger),
    Transcribing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    ButtonClicked,
    HotkeyPressed,
    HotkeyReleased,
    /// The microphone could not be opened or failed mid-recording.
    CaptureFailed(String),
    /// The recording was too short or silent to be worth transcribing.
    NothingCaptured,
    TranscriptionSucceeded(String),
    TranscriptionFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    NothingHeard,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    StartCapture,
    /// Stop the microphone and transcribe what was captured.
    FinishCapture,
    /// Hand the final text to the delivery layer (clipboard / paste / history).
    Deliver(String),
    Notify(Notice),
}

#[derive(Debug, Clone, Default)]
pub struct Session {
    state: State,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn state(&self) -> State {
        self.state
    }

    /// Applies an input and returns the effects the caller must run, in order.
    pub fn handle(&mut self, input: Input) -> Vec<Effect> {
        use Effect::{Deliver, FinishCapture, Notify, StartCapture};
        use State::{Idle, Recording, Transcribing};

        let (next, effects) = match (self.state, input) {
            (Idle, Input::ButtonClicked) => (Recording(Trigger::Button), vec![StartCapture]),
            (Idle, Input::HotkeyPressed) => (Recording(Trigger::Hotkey), vec![StartCapture]),

            // The button always stops; the hotkey only stops what it started.
            (Recording(_), Input::ButtonClicked)
            | (Recording(Trigger::Hotkey), Input::HotkeyReleased) => {
                (Transcribing, vec![FinishCapture])
            }

            (Recording(_), Input::CaptureFailed(reason))
            | (Transcribing, Input::TranscriptionFailed(reason)) => {
                (Idle, vec![Notify(Notice::Error(reason))])
            }

            (Recording(_) | Transcribing, Input::NothingCaptured) => {
                (Idle, vec![Notify(Notice::NothingHeard)])
            }

            (Transcribing, Input::TranscriptionSucceeded(raw)) => {
                let clean = text::normalize(&raw);
                if clean.is_empty() {
                    (Idle, vec![Notify(Notice::NothingHeard)])
                } else {
                    (Idle, vec![Deliver(clean)])
                }
            }

            // Everything else is a no-op (e.g. clicks while busy, stray releases).
            (state, _) => (state, Vec::new()),
        };
        self.state = next;
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Effect::{Deliver, FinishCapture, Notify, StartCapture};

    fn session_in(state: State) -> Session {
        Session { state }
    }

    #[test]
    fn starts_idle() {
        assert_eq!(Session::new().state(), State::Idle);
    }

    #[test]
    fn button_toggles_recording() {
        let mut s = Session::new();
        assert_eq!(s.handle(Input::ButtonClicked), [StartCapture]);
        assert_eq!(s.state(), State::Recording(Trigger::Button));
        assert_eq!(s.handle(Input::ButtonClicked), [FinishCapture]);
        assert_eq!(s.state(), State::Transcribing);
    }

    #[test]
    fn hotkey_records_while_held() {
        let mut s = Session::new();
        assert_eq!(s.handle(Input::HotkeyPressed), [StartCapture]);
        assert_eq!(s.state(), State::Recording(Trigger::Hotkey));
        assert_eq!(s.handle(Input::HotkeyReleased), [FinishCapture]);
        assert_eq!(s.state(), State::Transcribing);
    }

    #[test]
    fn hotkey_release_does_not_stop_a_button_recording() {
        let mut s = session_in(State::Recording(Trigger::Button));
        assert!(s.handle(Input::HotkeyPressed).is_empty());
        assert!(s.handle(Input::HotkeyReleased).is_empty());
        assert_eq!(s.state(), State::Recording(Trigger::Button));
    }

    #[test]
    fn button_can_stop_a_hotkey_recording() {
        let mut s = session_in(State::Recording(Trigger::Hotkey));
        assert_eq!(s.handle(Input::ButtonClicked), [FinishCapture]);
    }

    #[test]
    fn input_is_ignored_while_transcribing() {
        let mut s = session_in(State::Transcribing);
        assert!(s.handle(Input::ButtonClicked).is_empty());
        assert!(s.handle(Input::HotkeyPressed).is_empty());
        assert_eq!(s.state(), State::Transcribing);
    }

    #[test]
    fn successful_transcription_is_normalized_and_delivered() {
        let mut s = session_in(State::Transcribing);
        assert_eq!(
            s.handle(Input::TranscriptionSucceeded("  Olá,   mundo. ".into())),
            [Deliver("Olá, mundo.".into())]
        );
        assert_eq!(s.state(), State::Idle);
    }

    #[test]
    fn empty_transcription_notifies_nothing_heard() {
        let mut s = session_in(State::Transcribing);
        assert_eq!(
            s.handle(Input::TranscriptionSucceeded(" . ".into())),
            [Notify(Notice::NothingHeard)]
        );
        assert_eq!(s.state(), State::Idle);
    }

    #[test]
    fn nothing_captured_returns_to_idle() {
        let mut s = session_in(State::Transcribing);
        assert_eq!(
            s.handle(Input::NothingCaptured),
            [Notify(Notice::NothingHeard)]
        );
        assert_eq!(s.state(), State::Idle);
    }

    #[test]
    fn failures_surface_an_error_and_reset() {
        let mut s = session_in(State::Recording(Trigger::Hotkey));
        assert_eq!(
            s.handle(Input::CaptureFailed("sem microfone".into())),
            [Notify(Notice::Error("sem microfone".into()))]
        );
        assert_eq!(s.state(), State::Idle);

        let mut s = session_in(State::Transcribing);
        assert_eq!(
            s.handle(Input::TranscriptionFailed("modelo".into())),
            [Notify(Notice::Error("modelo".into()))]
        );
        assert_eq!(s.state(), State::Idle);
    }

    #[test]
    fn stray_results_while_idle_are_ignored() {
        let mut s = Session::new();
        assert!(
            s.handle(Input::TranscriptionSucceeded("oi".into()))
                .is_empty()
        );
        assert!(s.handle(Input::HotkeyReleased).is_empty());
        assert_eq!(s.state(), State::Idle);
    }
}
