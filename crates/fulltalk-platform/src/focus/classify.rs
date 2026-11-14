//! Pure classification rules for Windows and macOS accessibility data, kept
//! free of FFI so they are unit-tested on every CI runner.

use fulltalk_core::delivery::FocusTarget;

/// UI Automation control type ids (`UIA_*ControlTypeId`).
pub mod uia {
    pub const EDIT: i32 = 50_004;
    pub const DOCUMENT: i32 = 50_030;
}

/// Window classes of Windows console hosts / terminals.
const WINDOWS_TERMINAL_CLASSES: [&str; 3] =
    ["ConsoleWindowClass", "TermControl", "PseudoConsoleWindow"];

/// Classifies a focused UI Automation element.
///
/// `read_only` comes from the Value pattern when the element exposes one.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn windows(control_type: i32, class_name: &str, read_only: Option<bool>) -> FocusTarget {
    if WINDOWS_TERMINAL_CLASSES
        .iter()
        .any(|c| class_name.contains(c))
    {
        return FocusTarget::Terminal;
    }
    match (control_type, read_only) {
        (_, Some(true)) => FocusTarget::Other,
        (uia::EDIT, _) | (uia::DOCUMENT, Some(false)) => FocusTarget::TextInput,
        _ => FocusTarget::Other,
    }
}

/// Classifies a focused macOS accessibility element by its `AXRole`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn macos(role: &str, value_settable: bool) -> FocusTarget {
    match role {
        "AXTextField" | "AXTextArea" | "AXSearchField" | "AXComboBox" => FocusTarget::TextInput,
        // Rich editors in browsers/Electron expose a settable web area.
        "AXWebArea" | "AXGroup" if value_settable => FocusTarget::TextInput,
        _ => FocusTarget::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_edits_accept_text_unless_read_only() {
        assert_eq!(windows(uia::EDIT, "Edit", None), FocusTarget::TextInput);
        assert_eq!(
            windows(uia::EDIT, "Edit", Some(false)),
            FocusTarget::TextInput
        );
        assert_eq!(windows(uia::EDIT, "Edit", Some(true)), FocusTarget::Other);
    }

    #[test]
    fn windows_documents_need_an_explicit_writable_value() {
        // A browser page body is a Document too — never paste into it blindly.
        assert_eq!(
            windows(uia::DOCUMENT, "Chrome_RenderWidgetHostHWND", None),
            FocusTarget::Other
        );
        assert_eq!(
            windows(uia::DOCUMENT, "_WwG", Some(false)),
            FocusTarget::TextInput
        );
    }

    #[test]
    fn windows_terminals_are_detected_by_class() {
        assert_eq!(windows(50_020, "TermControl", None), FocusTarget::Terminal);
        assert_eq!(
            windows(50_033, "ConsoleWindowClass", None),
            FocusTarget::Terminal
        );
    }

    #[test]
    fn windows_buttons_are_other() {
        assert_eq!(windows(50_000, "Button", None), FocusTarget::Other);
    }

    #[test]
    fn macos_text_roles_accept_text() {
        for role in ["AXTextField", "AXTextArea", "AXSearchField", "AXComboBox"] {
            assert_eq!(macos(role, false), FocusTarget::TextInput, "{role}");
        }
    }

    #[test]
    fn macos_web_areas_only_when_editable() {
        assert_eq!(macos("AXWebArea", true), FocusTarget::TextInput);
        assert_eq!(macos("AXWebArea", false), FocusTarget::Other);
        assert_eq!(macos("AXButton", true), FocusTarget::Other);
    }
}
