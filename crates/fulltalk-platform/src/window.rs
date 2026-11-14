//! Window tweaks the GUI toolkit does not expose.

use raw_window_handle::RawWindowHandle;

/// Makes the window clickable without stealing keyboard focus from the app the
/// user is typing in — so clicking the mic button still pastes into that app.
///
/// * X11 (and `XWayland`): `WM_HINTS.input = False` (window manager never focuses it).
/// * Windows: `WS_EX_NOACTIVATE`.
/// * macOS: not supported yet (no-op).
pub fn make_non_activating(handle: RawWindowHandle) {
    let result: Result<(), String> = match handle {
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xlib(h) => x11::set_no_input(h.window as u32),
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xcb(h) => x11::set_no_input(h.window.get()),
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(h) => win32::set_no_activate(h.hwnd.get()),
        other => Err(format!("unsupported window handle {other:?}")),
    };
    if let Err(err) = result {
        tracing::warn!(%err, "could not make the window non-activating");
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use x11rb::connection::Connection;
    use x11rb::properties::WmHints;

    pub(super) fn set_no_input(window: u32) -> Result<(), String> {
        let (conn, _) = x11rb::connect(None).map_err(|e| e.to_string())?;
        let mut hints = WmHints::get(&conn, window)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        hints.input = Some(false);
        hints.set(&conn, window).map_err(|e| e.to_string())?;
        conn.flush().map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod win32 {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE,
    };

    pub(super) fn set_no_activate(hwnd: isize) -> Result<(), String> {
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        // SAFETY: `hwnd` comes from a live winit window owned by this process.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE.0 as isize);
        }
        Ok(())
    }
}
