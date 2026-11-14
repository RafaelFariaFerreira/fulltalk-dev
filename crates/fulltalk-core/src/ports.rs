//! Boundaries to the outside world (hexagonal "ports").
//!
//! The app wires real adapters (ONNX model, OS clipboard, accessibility APIs,
//! synthetic keyboard); tests use in-memory fakes.

use thiserror::Error;

use crate::delivery::{FocusTarget, PasteChord};

/// Error raised by an adapter, already phrased for the user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct PortError(pub String);

impl PortError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// Speech recognition over 16 kHz mono samples.
pub trait SpeechToText: Send {
    fn transcribe(&mut self, samples_16k_mono: &[f32]) -> Result<String, PortError>;
}

pub trait Clipboard {
    fn set_text(&mut self, text: &str) -> Result<(), PortError>;
}

/// Tells what currently has keyboard focus in *another* application.
pub trait FocusProbe {
    fn focused(&self) -> FocusTarget;
}

/// Sends a synthetic paste keystroke to the focused application.
pub trait Paster {
    fn paste(&mut self, chord: PasteChord) -> Result<(), PortError>;
}
