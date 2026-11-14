//! # fulltalk-core
//!
//! Platform-free domain logic for **fulltalk dev** — *code is cheap, show me the talk*.
//!
//! Everything here is deterministic and covered by unit tests: the dictation
//! [`session`] state machine, hold-to-talk [`chord`] detection, [`shortcut`]
//! parsing, the recent-transcription [`history`], the paste-or-clipboard
//! [`delivery`] policy, [`audio`] preparation and [`settings`] persistence.
//! The OS-specific and model-specific adapters live in sibling crates and plug
//! in through the traits defined in [`ports`].

