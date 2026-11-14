//! Debug helper: dumps the accessible tree of the active windows (role + key states).
#[cfg(target_os = "linux")]
fn main() {
    use atspi::State;
    use atspi::proxy::accessible::ObjectRefExt;
    let filter = std::env::args().nth(1).unwrap_or_default();
    zbus::block_on(async {
        let a11y = atspi::connection::AccessibilityConnection::new()
            .await
            .unwrap();
        let conn = a11y.connection();
        let root = a11y.root_accessible_on_registry().await.unwrap();
        for app in root.get_children().await.unwrap() {
            let Ok(p) = app.as_accessible_proxy(conn).await else {
                continue;
            };
            let name = p.name().await.unwrap_or_default();
            if !name.contains(&filter) {
                continue;
            }
            let mut stack = vec![(app.clone(), 0usize)];
            let mut n = 0;
            while let Some((node, depth)) = stack.pop() {
                n += 1;
                if n > 400 || depth > 25 {
                    break;
                }
                let Ok(np) = node.as_accessible_proxy(conn).await else {
                    continue;
                };
                let st = np.get_state().await.unwrap_or_default();
                let role = np
                    .get_role()
                    .await
                    .map_or_else(|_| String::new(), |r| format!("{r:?}"));
                let flags: Vec<&str> = [
                    (State::Active, "ACTIVE"),
                    (State::Focused, "FOCUSED"),
                    (State::Showing, "showing"),
                    (State::Editable, "editable"),
                    (State::Focusable, "focusable"),
                ]
                .iter()
                .filter(|(s, _)| st.contains(*s))
                .map(|(_, l)| *l)
                .collect();
                println!(
                    "{}{role} {:?} {flags:?}",
                    "  ".repeat(depth),
                    np.name()
                        .await
                        .unwrap_or_default()
                        .chars()
                        .take(30)
                        .collect::<String>()
                );
                for c in np
                    .get_children()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .rev()
                {
                    stack.push((c, depth + 1));
                }
            }
        }
    });
}
#[cfg(not(target_os = "linux"))]
fn main() {}
