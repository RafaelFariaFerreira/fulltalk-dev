//! Focus detection on Linux through the AT-SPI accessibility bus.
//!
//! Two layers, like a screen reader:
//! 1. **Event tracking** — a background task listens to `object:state-changed:focused`
//!    and remembers the last object that gained focus. At query time it is
//!    re-verified live (still `Focused`, and inside the active window?), so a
//!    stale value is never trusted.
//! 2. **Tree scan fallback** — find the `Active` top-level window and search it
//!    for the `Focused` descendant (`Collection` when the toolkit supports it,
//!    otherwise a bounded walk over showing children; GTK 4 needs the latter).
//!
//! Chromium and Electron applications are a special case: they register their
//! windows but keep everything inside them private unless a screen reader is
//! running. Turning that switch on for the whole session would slow every
//! browser down, so such a window is reported as [`FocusTarget::Opaque`] and
//! the delivery policy decides what to do with it.
//!
//! Every D-Bus call is bounded by a timeout: a frozen application must never
//! freeze dictation. Failures degrade to [`FocusTarget::Unknown`], which the
//! delivery policy treats as "clipboard only".

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use atspi::connection::AccessibilityConnection;
use atspi::events::object::StateChangedEvent;
use atspi::events::{Event, ObjectEvents};
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::application::ApplicationProxy;
use atspi::proxy::collection::CollectionProxy;
use atspi::{MatchType, ObjectMatchRule, ObjectRefOwned, Role, SortOrder, State, StateSet};
use fulltalk_core::delivery::FocusTarget;
use fulltalk_core::ports::FocusProbe;
use futures_lite::StreamExt;

const CALL_TIMEOUT: Duration = Duration::from_millis(250);
const TOTAL_TIMEOUT: Duration = Duration::from_millis(900);
/// Upper bound on nodes visited by the fallback walk.
const MAX_WALK_NODES: usize = 1500;
/// Upper bound on ancestors climbed to find the window an object lives in.
const MAX_ANCESTORS: usize = 64;

/// Classifies the focused accessible object.
pub(crate) fn classify(role: Role, states: StateSet) -> FocusTarget {
    if role == Role::Terminal {
        FocusTarget::Terminal
    } else if states.contains(State::Editable) && !states.contains(State::ReadOnly) {
        FocusTarget::TextInput
    } else {
        FocusTarget::Other
    }
}

/// Whether an active window in which no focused widget was found is one that
/// simply cannot be looked into: a Chromium-based application (that is what
/// Chrome and Electron report as their toolkit) exposing no readable child.
pub(crate) fn hides_its_widgets(toolkit: &str, readable_children: usize) -> bool {
    toolkit.eq_ignore_ascii_case("chromium") && readable_children == 0
}

type Tracked = Arc<Mutex<Option<ObjectRefOwned>>>;

/// AT-SPI focus probe.
pub struct AtspiFocusProbe {
    a11y: Option<AccessibilityConnection>,
    last_focused: Tracked,
}

impl std::fmt::Debug for AtspiFocusProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AtspiFocusProbe")
            .field("connected", &self.a11y.is_some())
            .finish_non_exhaustive()
    }
}

impl AtspiFocusProbe {
    /// Connects to the accessibility bus, asks toolkits to expose their widget
    /// trees (Chromium/Electron only do so when accessibility is enabled) and
    /// starts tracking focus changes in the background.
    pub fn connect() -> Self {
        let last_focused = Tracked::default();
        let a11y = zbus::block_on(async {
            if let Err(err) = atspi::connection::set_session_accessibility(true).await {
                tracing::debug!(%err, "could not enable session accessibility");
            }
            AccessibilityConnection::new().await
        });
        let a11y = match a11y {
            Ok(conn) => conn,
            Err(err) => {
                tracing::warn!(%err, "AT-SPI unavailable; text fields cannot be detected");
                return Self {
                    a11y: None,
                    last_focused,
                };
            }
        };

        let tracker = (a11y.clone(), Arc::clone(&last_focused));
        let spawned = std::thread::Builder::new()
            .name("atspi-focus".into())
            .spawn(move || zbus::block_on(track_focus(tracker.0, tracker.1)));
        if let Err(err) = spawned {
            tracing::warn!(%err, "focus tracker thread failed to start");
        }
        Self {
            a11y: Some(a11y),
            last_focused,
        }
    }

    async fn query(
        a11y: &AccessibilityConnection,
        tracked: Option<ObjectRefOwned>,
    ) -> Option<FocusTarget> {
        let conn = a11y.connection();
        if let Some(object) = tracked
            && let Some(target) = describe_if_focused(conn, &object).await
        {
            if in_active_window(conn, &object).await {
                return Some(target);
            }
            tracing::debug!("the last focused object is not in the active window, ignoring it");
        }
        scan(a11y).await
    }
}

impl FocusProbe for AtspiFocusProbe {
    fn focused(&self) -> FocusTarget {
        let Some(a11y) = &self.a11y else {
            return FocusTarget::Unknown;
        };
        let tracked = self.last_focused.lock().ok().and_then(|g| g.clone());
        let target = zbus::block_on(with_timeout(TOTAL_TIMEOUT, Self::query(a11y, tracked)))
            .flatten()
            .unwrap_or(FocusTarget::Unknown);
        tracing::debug!(?target, "focus probed");
        target
    }
}

/// Background loop: remember the most recent object that gained focus.
async fn track_focus(a11y: AccessibilityConnection, last: Tracked) {
    if let Err(err) = a11y.register_event::<StateChangedEvent>().await {
        tracing::warn!(%err, "cannot subscribe to AT-SPI focus events");
        return;
    }
    let mut events = std::pin::pin!(a11y.event_stream());
    while let Some(event) = events.next().await {
        let Ok(Event::Object(ObjectEvents::StateChanged(change))) = event else {
            continue;
        };
        if change.state != State::Focused {
            continue;
        }
        if let Ok(mut slot) = last.lock() {
            if change.enabled {
                *slot = Some(change.item);
            } else if slot.as_ref() == Some(&change.item) {
                *slot = None;
            }
        }
    }
}

/// Classifies `object` if it still holds keyboard focus.
async fn describe_if_focused(
    conn: &zbus::Connection,
    object: &ObjectRefOwned,
) -> Option<FocusTarget> {
    let proxy = bounded(object.as_accessible_proxy(conn)).await?;
    let states = bounded(proxy.get_state()).await?;
    if !states.contains(State::Focused) {
        return None;
    }
    let role = bounded(proxy.get_role()).await?;
    Some(classify(role, states))
}

/// Whether `object` sits in the window that is active right now.
///
/// A focus event is not proof of focus. GNOME Shell's stage calls itself
/// focused whenever no window holds the keyboard the usual way, which is the
/// case right after a click on our own never-focused window, and it would
/// otherwise hide the application the user is really typing in.
async fn in_active_window(conn: &zbus::Connection, object: &ObjectRefOwned) -> bool {
    let mut current = object.clone();
    for _ in 0..MAX_ANCESTORS {
        let Some(proxy) = bounded(current.as_accessible_proxy(conn)).await else {
            return false;
        };
        let Some(parent) = bounded(proxy.parent()).await else {
            return false;
        };
        let Some(parent_proxy) = bounded(parent.as_accessible_proxy(conn)).await else {
            return false;
        };
        // A top-level window is a direct child of its application.
        if bounded(parent_proxy.get_role()).await == Some(Role::Application) {
            return bounded(proxy.get_state())
                .await
                .is_some_and(|states| states.contains(State::Active));
        }
        current = parent;
    }
    false
}

/// Fallback: find the active window(s) and the focused descendant inside.
async fn scan(a11y: &AccessibilityConnection) -> Option<FocusTarget> {
    let conn = a11y.connection();
    let root = bounded(a11y.root_accessible_on_registry()).await?;
    let apps = bounded(root.get_children()).await?;

    let mut saw_active_window = false;
    for app_ref in &apps {
        let Some(app) = bounded(app_ref.as_accessible_proxy(conn)).await else {
            continue;
        };
        for window in bounded(app.get_children()).await.unwrap_or_default() {
            let Some(proxy) = bounded(window.as_accessible_proxy(conn)).await else {
                continue;
            };
            let active = bounded(proxy.get_state())
                .await
                .is_some_and(|s| s.contains(State::Active));
            if !active {
                continue;
            }
            saw_active_window = true;
            if let Some(focused) = find_focused(conn, &window).await
                && let Some(target) = describe_if_focused(conn, &focused).await
            {
                return Some(target);
            }
            if is_opaque(conn, app_ref, &proxy).await {
                return Some(FocusTarget::Opaque);
            }
        }
    }
    // An active window with nothing focused accepts no text; no accessible
    // active window at all means an app we cannot see into.
    saw_active_window.then_some(FocusTarget::Other)
}

/// See [`hides_its_widgets`].
async fn is_opaque(
    conn: &zbus::Connection,
    app: &ObjectRefOwned,
    window: &AccessibleProxy<'_>,
) -> bool {
    let Some(toolkit) = toolkit_name(conn, app).await else {
        return false;
    };
    let mut readable = 0;
    for child in bounded(window.get_children()).await.unwrap_or_default() {
        // A hidden tree still reports a child, but it is a null reference.
        if let Some(proxy) = bounded(child.as_accessible_proxy(conn)).await
            && bounded(proxy.get_state()).await.is_some()
        {
            readable += 1;
        }
    }
    hides_its_widgets(&toolkit, readable)
}

async fn toolkit_name(conn: &zbus::Connection, app: &ObjectRefOwned) -> Option<String> {
    let bus_name = app.name()?.clone();
    let application = bounded(async {
        ApplicationProxy::builder(conn)
            .destination(bus_name)?
            .path(app.path().clone())?
            .build()
            .await
    })
    .await?;
    bounded(application.toolkit_name()).await
}

async fn find_focused(conn: &zbus::Connection, window: &ObjectRefOwned) -> Option<ObjectRefOwned> {
    if let Some(found) = find_focused_via_collection(conn, window).await {
        return Some(found);
    }
    find_focused_via_walk(conn, window).await
}

async fn find_focused_via_collection(
    conn: &zbus::Connection,
    window: &ObjectRefOwned,
) -> Option<ObjectRefOwned> {
    let bus_name = window.name()?.clone();
    let collection = bounded(async {
        CollectionProxy::builder(conn)
            .destination(bus_name)?
            .path(window.path().clone())?
            .build()
            .await
    })
    .await?;
    let rule = ObjectMatchRule::builder()
        .states([State::Focused], MatchType::All)
        .build();
    bounded(collection.get_matches(rule, SortOrder::Canonical, 1, true))
        .await?
        .into_iter()
        .next()
}

/// Breadth-first walk over *showing* nodes only — the focused widget is always visible.
async fn find_focused_via_walk(
    conn: &zbus::Connection,
    window: &ObjectRefOwned,
) -> Option<ObjectRefOwned> {
    let mut queue = VecDeque::from([window.clone()]);
    let mut visited = 0usize;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        if visited > MAX_WALK_NODES {
            tracing::debug!("focus walk hit node limit");
            return None;
        }
        let Some(proxy): Option<AccessibleProxy<'_>> =
            bounded(node.as_accessible_proxy(conn)).await
        else {
            continue;
        };
        let Some(states) = bounded(proxy.get_state()).await else {
            continue;
        };
        if states.contains(State::Focused) {
            return Some(node);
        }
        if states.contains(State::Showing) || visited == 1 {
            queue.extend(bounded(proxy.get_children()).await.unwrap_or_default());
        }
    }
    None
}

/// Runs a fallible D-Bus call with [`CALL_TIMEOUT`]; errors and timeouts become `None`.
async fn bounded<T, E: std::fmt::Display>(fut: impl Future<Output = Result<T, E>>) -> Option<T> {
    match with_timeout(CALL_TIMEOUT, fut).await? {
        Ok(value) => Some(value),
        Err(err) => {
            tracing::trace!(%err, "AT-SPI call failed");
            None
        }
    }
}

async fn with_timeout<T>(limit: Duration, fut: impl Future<Output = T>) -> Option<T> {
    futures_lite::future::or(async { Some(fut.await) }, async {
        async_io::Timer::after(limit).await;
        None
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(list: &[State]) -> StateSet {
        list.iter().copied().collect()
    }

    #[test]
    fn terminals_are_recognized_by_role() {
        assert_eq!(
            classify(Role::Terminal, states(&[State::Focused])),
            FocusTarget::Terminal
        );
    }

    #[test]
    fn editable_widgets_accept_text() {
        for role in [Role::Entry, Role::Text, Role::DocumentWeb, Role::Paragraph] {
            assert_eq!(
                classify(role, states(&[State::Focused, State::Editable])),
                FocusTarget::TextInput,
                "{role:?}"
            );
        }
    }

    #[test]
    fn chromium_windows_with_a_hidden_tree_are_opaque() {
        // What Chrome and Electron report without a screen reader running.
        assert!(hides_its_widgets("Chromium", 0));
        assert!(hides_its_widgets("chromium", 0));
    }

    #[test]
    fn readable_or_non_chromium_windows_are_not_opaque() {
        // Chrome with accessibility on: its widgets can be trusted.
        assert!(!hides_its_widgets("Chromium", 3));
        // A GTK window with nothing focused really has nothing to paste into.
        assert!(!hides_its_widgets("GTK", 0));
        assert!(!hides_its_widgets("", 0));
    }

    #[test]
    fn read_only_or_non_editable_widgets_do_not() {
        assert_eq!(
            classify(Role::Entry, states(&[State::Editable, State::ReadOnly])),
            FocusTarget::Other
        );
        assert_eq!(
            classify(Role::Button, states(&[State::Focused])),
            FocusTarget::Other
        );
    }
}
