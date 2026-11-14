//! Focus detection on Windows through UI Automation.
#![allow(unsafe_code)]

use fulltalk_core::delivery::FocusTarget;
use fulltalk_core::ports::FocusProbe;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, UIA_ValuePatternId,
};

use super::classify;

#[derive(Debug, Default, Clone, Copy)]
pub struct UiaFocusProbe;

impl UiaFocusProbe {
    fn query() -> windows::core::Result<FocusTarget> {
        // SAFETY: plain COM calls on interfaces we own; COM is initialized for
        // this thread first (repeated initialization is harmless).
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let element = automation.GetFocusedElement()?;
            let control_type = element.CurrentControlType()?.0;
            let class_name = element.CurrentClassName()?.to_string();
            let read_only = element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()
                .and_then(|p| p.CurrentIsReadOnly().ok())
                .map(|b| b.as_bool());
            Ok(classify::windows(control_type, &class_name, read_only))
        }
    }
}

impl FocusProbe for UiaFocusProbe {
    fn focused(&self) -> FocusTarget {
        Self::query().unwrap_or_else(|err| {
            tracing::debug!(%err, "UI Automation query failed");
            FocusTarget::Unknown
        })
    }
}
