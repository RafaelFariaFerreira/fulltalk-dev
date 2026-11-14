//! User preferences.

use serde::{Deserialize, Serialize};

use crate::shortcut::Shortcut;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Hold to talk.
    pub shortcut: Shortcut,
    /// Paste into the focused text field / terminal when one is detected.
    pub auto_paste: bool,
    /// Start fulltalk dev when the user logs in.
    pub launch_at_login: bool,
    /// Last window position (logical pixels), restored on start.
    pub window_position: Option<[f32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: Shortcut::default(),
            auto_paste: true,
            launch_at_login: false,
            window_position: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_product_spec() {
        let s = Settings::default();
        assert_eq!(s.shortcut.to_string(), "Alt+S");
        assert!(s.auto_paste);
        assert!(!s.launch_at_login);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // Older settings files must keep working when new fields are added.
        let s: Settings = serde_json::from_str(r#"{ "shortcut": "Ctrl+Shift+Space" }"#).unwrap();
        assert_eq!(s.shortcut.to_string(), "Ctrl+Shift+Space");
        assert!(s.auto_paste);
    }

    #[test]
    fn invalid_shortcut_in_file_is_an_error_not_a_panic() {
        assert!(serde_json::from_str::<Settings>(r#"{ "shortcut": "nope" }"#).is_err());
    }
}
