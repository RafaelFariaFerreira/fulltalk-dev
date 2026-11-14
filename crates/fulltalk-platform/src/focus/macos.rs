//! Focus detection on macOS through the Accessibility (AX) API.
//!
//! Requires the "Accessibility" permission (System Settings → Privacy & Security),
//! which macOS also demands for the global shortcut and synthetic paste.
#![allow(unsafe_code)]

use accessibility_sys::{
    AXIsProcessTrusted, AXUIElementCopyAttributeValue, AXUIElementCreateSystemWide,
    AXUIElementIsAttributeSettable, AXUIElementRef, kAXErrorSuccess, kAXFocusedUIElementAttribute,
    kAXRoleAttribute, kAXValueAttribute,
};
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use fulltalk_core::delivery::FocusTarget;
use fulltalk_core::ports::FocusProbe;

use super::classify;

#[derive(Debug, Default, Clone, Copy)]
pub struct AxFocusProbe;

/// Reads an attribute as an owned CF object.
///
/// SAFETY: `element` must be a valid `AXUIElementRef`.
unsafe fn copy_attribute(element: AXUIElementRef, name: &str) -> Option<CFType> {
    let name = CFString::new(name);
    let mut value = std::ptr::null();
    // SAFETY: valid element and attribute name; on success we own `value` (Create rule).
    let err = unsafe {
        AXUIElementCopyAttributeValue(element, name.as_concrete_TypeRef(), &raw mut value)
    };
    (err == kAXErrorSuccess && !value.is_null())
        .then(|| unsafe { CFType::wrap_under_create_rule(value) })
}

impl FocusProbe for AxFocusProbe {
    fn focused(&self) -> FocusTarget {
        // SAFETY: AX calls on objects we create and own; all refs are released by
        // their `CFType` wrappers.
        unsafe {
            if !AXIsProcessTrusted() {
                return FocusTarget::Unknown;
            }
            let system = CFType::wrap_under_create_rule(AXUIElementCreateSystemWide().cast());
            let Some(focused) = copy_attribute(
                system.as_CFTypeRef() as AXUIElementRef,
                kAXFocusedUIElementAttribute,
            ) else {
                return FocusTarget::Unknown;
            };
            let element = focused.as_CFTypeRef() as AXUIElementRef;
            let role = copy_attribute(element, kAXRoleAttribute)
                .and_then(|r| r.downcast::<CFString>())
                .map(|s| s.to_string())
                .unwrap_or_default();
            let mut settable = 0u8;
            let attr = CFString::new(kAXValueAttribute);
            let value_settable = AXUIElementIsAttributeSettable(
                element,
                attr.as_concrete_TypeRef(),
                (&raw mut settable).cast(),
            ) == kAXErrorSuccess
                && settable != 0;
            classify::macos(&role, value_settable)
        }
    }
}
