//! Global shortcut through the XDG desktop portal (Linux on Wayland).
//!
//! Wayland compositors do not let applications grab keys, but GNOME 48+ and
//! KDE Plasma implement `org.freedesktop.portal.GlobalShortcuts`: the app asks
//! for a shortcut, the desktop confirms it with the user once, and from then
//! on reports both press and release. No group membership or device access is
//! involved, and the keys never reach the focused application.
//!
//! The desktop owns the binding and remembers it per shortcut id, so every
//! shortcut gets its own id: picking another one in the settings binds a new
//! id, and the desktop asks again instead of silently keeping the old keys.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use fulltalk_core::shortcut::{Key, Modifier, Shortcut};
use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use super::{HotkeyError, HotkeyEvent, HotkeyHandler};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
/// Matches the installed `fulltalk.desktop`, which is how the desktop names us.
const APP_ID: &str = "fulltalk";
const DESCRIPTION: &str = "Segurar para ditar";

type Results = HashMap<String, OwnedValue>;

pub(super) struct PortalHotkeys {
    connection: Connection,
    handler: HotkeyHandler,
    shared: Arc<Shared>,
}

struct Shared {
    /// Bumped on every (re)bind; a listener from an older generation stops.
    generation: AtomicU64,
    session: Mutex<Option<OwnedObjectPath>>,
}

impl Shared {
    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }
}

impl PortalHotkeys {
    /// Fails fast when the desktop has no global-shortcuts portal, so the
    /// caller can fall back. The binding itself happens in the background: the
    /// first time, it waits for the user to answer the desktop's prompt.
    pub(super) fn start(shortcut: Shortcut, handler: HotkeyHandler) -> Result<Self, HotkeyError> {
        let connection = Connection::session().map_err(other)?;
        connection
            .call_method(
                Some(DESTINATION),
                PATH,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(INTERFACE, "version"),
            )
            .map_err(other)?;
        register_app_id(&connection);

        let this = Self {
            connection,
            handler,
            shared: Arc::new(Shared {
                generation: AtomicU64::new(0),
                session: Mutex::new(None),
            }),
        };
        this.bind(shortcut);
        Ok(this)
    }

    pub(super) fn set_shortcut(&self, shortcut: Shortcut) {
        self.bind(shortcut);
    }

    fn bind(&self, shortcut: Shortcut) {
        let generation = self.shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.close_session();
        let (connection, handler, shared) = (
            self.connection.clone(),
            Arc::clone(&self.handler),
            Arc::clone(&self.shared),
        );
        let spawned = std::thread::Builder::new()
            .name("portal-hotkey".into())
            .spawn(move || run(&connection, shortcut, &handler, &shared, generation));
        if let Err(err) = spawned {
            tracing::warn!(%err, "could not start the portal shortcut listener");
            (self.handler)(HotkeyEvent::Denied);
        }
    }

    fn close_session(&self) {
        let session = self.shared.session.lock().ok().and_then(|mut s| s.take());
        if let Some(session) = session {
            close(&self.connection, &session);
        }
    }
}

impl Drop for PortalHotkeys {
    fn drop(&mut self) {
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.close_session();
    }
}

fn run(
    connection: &Connection,
    shortcut: Shortcut,
    handler: &HotkeyHandler,
    shared: &Shared,
    generation: u64,
) {
    let session = match bind(connection, shortcut, generation) {
        Ok(session) => session,
        Err(err) => {
            tracing::warn!(%err, "the desktop did not bind the shortcut");
            if shared.is_current(generation) {
                handler(HotkeyEvent::Denied);
            }
            return;
        }
    };
    // The shortcut may have been changed while the desktop's prompt was open.
    if !shared.is_current(generation) {
        close(connection, &session);
        return;
    }
    if let Ok(mut slot) = shared.session.lock() {
        *slot = Some(session.clone());
    }
    tracing::info!(%shortcut, "listening for shortcut via the desktop portal");
    if let Err(err) = listen(connection, &session, shortcut, handler, shared, generation) {
        tracing::warn!(%err, "portal shortcut listener stopped");
    }
}

/// Creates a session and binds the shortcut in it. Blocks while the desktop
/// shows its confirmation prompt (only the first time a shortcut is used).
fn bind(
    connection: &Connection,
    shortcut: Shortcut,
    generation: u64,
) -> Result<OwnedObjectPath, HotkeyError> {
    let create_token = format!("fulltalk_create_{generation}");
    let session_token = format!("fulltalk_{generation}");
    let response = response_stream(connection, &create_token)?;
    let options: HashMap<&str, Value<'_>> = HashMap::from([
        ("handle_token", Value::from(create_token.as_str())),
        ("session_handle_token", Value::from(session_token.as_str())),
    ]);
    connection
        .call_method(
            Some(DESTINATION),
            PATH,
            Some(INTERFACE),
            "CreateSession",
            &(options,),
        )
        .map_err(other)?;
    let mut results = await_response(response)?.ok_or_else(|| other("session refused"))?;
    let handle = results
        .remove("session_handle")
        .and_then(|v| String::try_from(v).ok())
        .ok_or_else(|| other("no session handle"))?;
    let session = OwnedObjectPath::try_from(handle).map_err(other)?;

    let bind_token = format!("fulltalk_bind_{generation}");
    let response = response_stream(connection, &bind_token)?;
    let id = shortcut_id(shortcut);
    let trigger = portal_trigger(shortcut);
    let shortcuts = vec![(
        id.as_str(),
        HashMap::from([
            ("description", Value::from(DESCRIPTION)),
            ("preferred_trigger", Value::from(trigger.as_str())),
        ]),
    )];
    let options: HashMap<&str, Value<'_>> =
        HashMap::from([("handle_token", Value::from(bind_token.as_str()))]);
    let session_path: &ObjectPath<'_> = &session;
    let bound = connection
        .call_method(
            Some(DESTINATION),
            PATH,
            Some(INTERFACE),
            "BindShortcuts",
            &(session_path, shortcuts, "", options),
        )
        .map_err(other)
        .and_then(|_| await_response(response));
    match bound {
        Ok(Some(_)) => Ok(session),
        Ok(None) => {
            close(connection, &session);
            Err(HotkeyError::Denied)
        }
        Err(err) => {
            close(connection, &session);
            Err(err)
        }
    }
}

fn listen(
    connection: &Connection,
    session: &OwnedObjectPath,
    shortcut: Shortcut,
    handler: &HotkeyHandler,
    shared: &Shared,
    generation: u64,
) -> Result<(), HotkeyError> {
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(DESTINATION)
        .map_err(other)?
        .interface(INTERFACE)
        .map_err(other)?
        .path(PATH)
        .map_err(other)?
        .build();
    let signals = MessageIterator::for_match_rule(rule, connection, Some(64)).map_err(other)?;
    let id = shortcut_id(shortcut);
    let mut hold = Hold::default();
    for message in signals {
        if !shared.is_current(generation) {
            break;
        }
        let message = message.map_err(other)?;
        let header = message.header();
        let Some(member) = header.member() else {
            continue;
        };
        let body = message.body();
        // Activated / Deactivated: (session, shortcut id, timestamp, options).
        let Ok((handle, shortcut_id, _, _)) =
            body.deserialize::<(OwnedObjectPath, String, u64, Results)>()
        else {
            continue;
        };
        if handle != *session || shortcut_id != id {
            continue;
        }
        if let Some(event) = hold.on_signal(member.as_str()) {
            handler(event);
        }
    }
    Ok(())
}

/// Turns the portal's signals into clean edges. While the keys are held the
/// desktop repeats `Activated` at the keyboard's repeat rate.
#[derive(Debug, Default)]
struct Hold {
    held: bool,
}

impl Hold {
    fn on_signal(&mut self, member: &str) -> Option<HotkeyEvent> {
        match (member, self.held) {
            ("Activated", false) => {
                self.held = true;
                Some(HotkeyEvent::Pressed)
            }
            ("Deactivated", true) => {
                self.held = false;
                Some(HotkeyEvent::Released)
            }
            _ => None,
        }
    }
}

/// Subscribes to the `Response` of the request that will use `token`. Must be
/// called before the request is made, or a fast answer could be missed.
fn response_stream(connection: &Connection, token: &str) -> Result<MessageIterator, HotkeyError> {
    let sender = connection
        .unique_name()
        .map(|name| name.trim_start_matches(':').replace('.', "_"))
        .ok_or_else(|| other("no D-Bus name"))?;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")
        .map_err(other)?
        .member("Response")
        .map_err(other)?
        .path(format!("{PATH}/request/{sender}/{token}"))
        .map_err(other)?
        .build();
    MessageIterator::for_match_rule(rule, connection, Some(4)).map_err(other)
}

/// Waits for a portal request to finish. `None` means the user (or the
/// desktop) declined it.
fn await_response(mut stream: MessageIterator) -> Result<Option<Results>, HotkeyError> {
    let message = stream
        .next()
        .ok_or_else(|| other("the portal went away"))?
        .map_err(other)?;
    let (code, results) = message
        .body()
        .deserialize::<(u32, Results)>()
        .map_err(other)?;
    Ok((code == 0).then_some(results))
}

/// Tells the portal which installed app this is. Without it a binary started
/// from a terminal is attributed to the terminal (or to nothing at all).
fn register_app_id(connection: &Connection) {
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    if let Err(err) = connection.call_method(
        Some(DESTINATION),
        PATH,
        Some("org.freedesktop.host.portal.Registry"),
        "Register",
        &(APP_ID, options),
    ) {
        tracing::debug!(%err, "could not register the app id with the portal");
    }
}

fn close(connection: &Connection, session: &OwnedObjectPath) {
    let path: &ObjectPath<'_> = session;
    if let Err(err) = connection.call_method(
        Some(DESTINATION),
        path,
        Some("org.freedesktop.portal.Session"),
        "Close",
        &(),
    ) {
        tracing::debug!(%err, "could not close the portal session");
    }
}

fn other(err: impl std::fmt::Display) -> HotkeyError {
    HotkeyError::Other(err.to_string())
}

/// The shortcut in the XDG shortcuts syntax the portal expects: `Alt+S` is
/// `ALT+s`, `Ctrl+Shift+Space` is `CTRL+SHIFT+space`.
pub(crate) fn portal_trigger(shortcut: Shortcut) -> String {
    let key = match shortcut.key() {
        Key::Letter(c) => c.to_ascii_lowercase().to_string(),
        Key::Digit(d) => d.to_string(),
        Key::Function(n) => format!("F{n}"),
        Key::Space => "space".to_owned(),
    };
    shortcut
        .modifiers()
        .iter()
        .map(|modifier| match modifier {
            Modifier::Ctrl => "CTRL",
            Modifier::Alt => "ALT",
            Modifier::Shift => "SHIFT",
            Modifier::Super => "LOGO",
        })
        .map(str::to_owned)
        .chain(std::iter::once(key))
        .collect::<Vec<_>>()
        .join("+")
}

/// One id per shortcut (`hold-to-talk-alt-s`), see the module docs for why.
pub(crate) fn shortcut_id(shortcut: Shortcut) -> String {
    format!(
        "hold-to-talk-{}",
        shortcut.to_string().to_ascii_lowercase().replace('+', "-")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(text: &str) -> Shortcut {
        text.parse().unwrap()
    }

    #[test]
    fn default_shortcut_uses_the_xdg_trigger_syntax() {
        assert_eq!(portal_trigger(Shortcut::default()), "ALT+s");
    }

    #[test]
    fn triggers_cover_every_modifier_and_key_kind() {
        assert_eq!(
            portal_trigger(shortcut("Ctrl+Shift+Space")),
            "CTRL+SHIFT+space"
        );
        assert_eq!(portal_trigger(shortcut("Super+F12")), "LOGO+F12");
        assert_eq!(portal_trigger(shortcut("Ctrl+Alt+7")), "CTRL+ALT+7");
        assert_eq!(portal_trigger(shortcut("F9")), "F9");
    }

    #[test]
    fn each_shortcut_has_its_own_stable_id() {
        assert_eq!(shortcut_id(Shortcut::default()), "hold-to-talk-alt-s");
        assert_eq!(
            shortcut_id(shortcut("Ctrl+Shift+Space")),
            "hold-to-talk-ctrl-shift-space"
        );
        assert_ne!(
            shortcut_id(shortcut("Alt+S")),
            shortcut_id(shortcut("Alt+D"))
        );
    }

    #[test]
    fn hold_reports_one_press_and_one_release() {
        let mut hold = Hold::default();
        assert_eq!(hold.on_signal("Activated"), Some(HotkeyEvent::Pressed));
        assert_eq!(hold.on_signal("Deactivated"), Some(HotkeyEvent::Released));
    }

    #[test]
    fn key_repeat_does_not_press_again() {
        // GNOME re-sends Activated every ~30 ms while the keys stay down.
        let mut hold = Hold::default();
        hold.on_signal("Activated");
        assert_eq!(hold.on_signal("Activated"), None);
        assert_eq!(hold.on_signal("Activated"), None);
        assert_eq!(hold.on_signal("Deactivated"), Some(HotkeyEvent::Released));
    }

    #[test]
    fn release_without_a_press_is_ignored() {
        let mut hold = Hold::default();
        assert_eq!(hold.on_signal("Deactivated"), None);
        assert_eq!(hold.on_signal("ShortcutsChanged"), None);
    }
}
