//! User interface (egui).

pub mod icon;
mod main_view;
mod settings_view;
pub mod theme;
pub mod toast;
pub mod widgets;

#[cfg(feature = "screenshot")]
pub use settings_view::SIZE as SETTINGS_SIZE;
