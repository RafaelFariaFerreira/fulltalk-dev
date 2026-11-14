//! Window tweaks the GUI toolkit does not expose.

use raw_window_handle::RawWindowHandle;

/// Keeps the main window floating: above every other window, and clickable
/// without stealing keyboard focus from the app the user is typing in (so
/// clicking the mic button still pastes into that app).
///
/// * X11 (and `XWayland`): `_NET_WM_STATE_ABOVE` plus `WM_HINTS.input = False`
///   (the window manager never focuses it). Both are re-asserted by
///   [`FloatingWindow::enforce`], because the toolkit asks for "always on top"
///   while the window is still hidden, and window managers ignore that request
///   for windows they are not managing yet.
/// * Windows: `WS_EX_NOACTIVATE`; the toolkit's own always-on-top is reliable.
/// * macOS: only the toolkit's always-on-top (non-activating is not supported yet).
#[derive(Debug)]
pub struct FloatingWindow {
    #[cfg(target_os = "linux")]
    x11: Option<x11::Hints>,
}

impl FloatingWindow {
    pub fn new(handle: RawWindowHandle) -> Self {
        match handle {
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xlib(h) => Self::x11(h.window as u32),
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xcb(h) => Self::x11(h.window.get()),
            #[cfg(target_os = "windows")]
            RawWindowHandle::Win32(h) => {
                win32::set_no_activate(h.hwnd.get());
                Self {}
            }
            other => {
                tracing::warn!(?other, "window handle without floating support");
                Self {
                    #[cfg(target_os = "linux")]
                    x11: None,
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn x11(window: u32) -> Self {
        let x11 = x11::Hints::connect(window)
            .inspect_err(|err| tracing::warn!(%err, "could not make the window floating"))
            .ok();
        let this = Self { x11 };
        this.enforce();
        this
    }

    /// Where the pointer is on the screen, in physical pixels, when the
    /// platform can tell. The app uses it to move the window by hand: a window
    /// that never takes focus cannot ask the window manager for a move (the
    /// toolkit only forwards that request for focused windows).
    pub fn pointer_position(&self) -> Option<(i32, i32)> {
        #[cfg(target_os = "linux")]
        {
            self.x11.as_ref().and_then(x11::Hints::pointer_position)
        }
        #[cfg(target_os = "windows")]
        {
            win32::cursor_position()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            None
        }
    }

    /// The part of the screen not covered by panels and docks, as
    /// `[x, y, width, height]` in physical pixels, when the platform reports it.
    pub fn work_area(&self) -> Option<[i32; 4]> {
        #[cfg(target_os = "linux")]
        {
            self.x11.as_ref().and_then(x11::Hints::work_area)
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Re-applies whatever the window manager is missing. Cheap (two X11 round
    /// trips), so the app calls it now and then while it is painting anyway.
    pub fn enforce(&self) {
        #[cfg(target_os = "linux")]
        if let Some(x11) = &self.x11
            && let Err(err) = x11.enforce()
        {
            tracing::debug!(%err, "could not re-assert the floating window hints");
        }
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use x11rb::connection::Connection;
    use x11rb::properties::WmHints;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, MapState, PropMode,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    /// `_NET_WM_STATE` client message: add the state.
    const NET_WM_STATE_ADD: u32 = 1;
    /// `_NET_WM_STATE` client message: sent by a normal application.
    const SOURCE_APPLICATION: u32 = 1;

    pub(super) struct Hints {
        conn: RustConnection,
        window: u32,
        root: u32,
        net_wm_state: u32,
        above: u32,
        workarea: u32,
    }

    impl std::fmt::Debug for Hints {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Hints")
                .field("window", &self.window)
                .finish_non_exhaustive()
        }
    }

    impl Hints {
        pub(super) fn connect(window: u32) -> Result<Self, String> {
            let (conn, screen) = x11rb::connect(None).map_err(text)?;
            let root = conn.setup().roots[screen].root;
            let atom = |name: &[u8]| -> Result<u32, String> {
                Ok(conn
                    .intern_atom(false, name)
                    .map_err(text)?
                    .reply()
                    .map_err(text)?
                    .atom)
            };
            let (net_wm_state, above) = (atom(b"_NET_WM_STATE")?, atom(b"_NET_WM_STATE_ABOVE")?);
            let workarea = atom(b"_NET_WORKAREA")?;
            Ok(Self {
                conn,
                window,
                root,
                net_wm_state,
                above,
                workarea,
            })
        }

        pub(super) fn enforce(&self) -> Result<(), String> {
            let input_fixed = self.refuse_input()?;
            let above_fixed = self.stay_above()?;
            if input_fixed || above_fixed {
                self.conn.flush().map_err(text)?;
                tracing::debug!(
                    input_fixed,
                    above_fixed,
                    "re-asserted floating window hints"
                );
            }
            Ok(())
        }

        /// `_NET_WORKAREA` of the current desktop.
        pub(super) fn work_area(&self) -> Option<[i32; 4]> {
            let reply = self
                .conn
                .get_property(false, self.root, self.workarea, AtomEnum::CARDINAL, 0, 4)
                .ok()?
                .reply()
                .ok()?;
            let values: Vec<i32> = reply
                .value32()?
                .filter_map(|v| i32::try_from(v).ok())
                .collect();
            <[i32; 4]>::try_from(values).ok()
        }

        /// The pointer in root-window coordinates.
        pub(super) fn pointer_position(&self) -> Option<(i32, i32)> {
            let pointer = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
            Some((i32::from(pointer.root_x), i32::from(pointer.root_y)))
        }

        /// `WM_HINTS.input = False`. Returns whether it had to be written.
        fn refuse_input(&self) -> Result<bool, String> {
            let mut hints = WmHints::get(&self.conn, self.window)
                .map_err(text)?
                .reply()
                .map_err(text)?
                .unwrap_or_default();
            if hints.input == Some(false) {
                return Ok(false);
            }
            hints.input = Some(false);
            hints.set(&self.conn, self.window).map_err(text)?;
            Ok(true)
        }

        /// `_NET_WM_STATE_ABOVE`. Returns whether it had to be requested.
        fn stay_above(&self) -> Result<bool, String> {
            let states: Vec<u32> = self
                .conn
                .get_property(false, self.window, self.net_wm_state, AtomEnum::ATOM, 0, 64)
                .map_err(text)?
                .reply()
                .map_err(text)?
                .value32()
                .into_iter()
                .flatten()
                .collect();
            if states.contains(&self.above) {
                return Ok(false);
            }
            let mapped = self
                .conn
                .get_window_attributes(self.window)
                .map_err(text)?
                .reply()
                .map_err(text)?
                .map_state
                != MapState::UNMAPPED;
            if mapped {
                // The window manager owns the state of a mapped window: ask it.
                let event = ClientMessageEvent::new(
                    32,
                    self.window,
                    self.net_wm_state,
                    [NET_WM_STATE_ADD, self.above, 0, SOURCE_APPLICATION, 0],
                );
                self.conn
                    .send_event(
                        false,
                        self.root,
                        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                        event,
                    )
                    .map_err(text)?;
            } else {
                // Not managed yet: the window manager reads the property when it maps.
                self.conn
                    .change_property32(
                        PropMode::APPEND,
                        self.window,
                        self.net_wm_state,
                        AtomEnum::ATOM,
                        &[self.above],
                    )
                    .map_err(text)?;
            }
            Ok(true)
        }
    }

    fn text(err: impl std::fmt::Display) -> String {
        err.to_string()
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod win32 {
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE,
    };

    pub(super) fn cursor_position() -> Option<(i32, i32)> {
        let mut point = POINT::default();
        // SAFETY: `point` is a valid, writable POINT for the duration of the call.
        unsafe { GetCursorPos(&raw mut point) }.ok()?;
        Some((point.x, point.y))
    }

    pub(super) fn set_no_activate(hwnd: isize) {
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        // SAFETY: `hwnd` comes from a live winit window owned by this process.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE.0 as isize);
        }
    }
}
