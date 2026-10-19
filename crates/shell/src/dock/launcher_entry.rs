//! What apps ask of their Dock icon over D-Bus: a badge, a progress bar, attention. Apps send it
//! with the de facto LauncherEntry API (signal `Update` of `com.canonical.Unity.LauncherEntry`
//! on the session bus, as GTK, Qt/KDE, Electron and Chromium apps do): app URI
//! `application://<desktop id>.desktop` and a property map with `count`, `count-visible`,
//! `progress`, `progress-visible`, `urgent`.

use std::collections::HashMap;

use zbus::zvariant::OwnedValue;

use super::geometry::badge_text;

/// The state of one app's Dock icon.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LauncherState {
    /// The badge text (SH-DK-29).
    pub badge: Option<String>,
    /// 0–1 (SH-DK-30).
    pub progress: Option<f32>,
    /// When the app asked for attention (`Ui::now`).
    pub urgent_since: Option<f64>,
}

impl LauncherState {
    /// Nothing to show.
    pub fn is_empty(&self) -> bool {
        self.badge.is_none() && self.progress.is_none() && self.urgent_since.is_none()
    }
}

/// The desktop id in a LauncherEntry URI (`application://firefox.desktop` → `firefox`).
pub fn app_id(uri: &str) -> Option<String> {
    let id = uri.strip_prefix("application://")?;
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    (!id.is_empty()).then(|| id.to_string())
}

/// A property of an update.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    /// A number.
    Num(f64),
    /// A flag.
    Bool(bool),
}

/// The properties of an update from the bus.
pub fn props(map: &HashMap<String, OwnedValue>) -> HashMap<String, Prop> {
    let mut out = HashMap::new();
    for (k, v) in map {
        let p = if let Ok(b) = bool::try_from(v) {
            Prop::Bool(b)
        } else if let Ok(n) = i64::try_from(v) {
            Prop::Num(n as f64)
        } else if let Ok(n) = u64::try_from(v) {
            Prop::Num(n as f64)
        } else if let Ok(n) = i32::try_from(v) {
            Prop::Num(f64::from(n))
        } else if let Ok(n) = u32::try_from(v) {
            Prop::Num(f64::from(n))
        } else if let Ok(n) = f64::try_from(v) {
            Prop::Num(n)
        } else {
            continue;
        };
        out.insert(k.clone(), p);
    }
    out
}

/// Applies one `Update` to an app's state at time `now`.
pub fn apply(state: &LauncherState, props: &HashMap<String, Prop>, now: f64) -> LauncherState {
    let num = |k: &str| match props.get(k) {
        Some(Prop::Num(n)) => Some(*n),
        _ => None,
    };
    let flag = |k: &str| match props.get(k) {
        Some(Prop::Bool(b)) => Some(*b),
        _ => None,
    };
    let mut next = state.clone();
    let (count, count_visible) = (num("count"), flag("count-visible"));
    if count.is_some() || count_visible.is_some() {
        let visible = count_visible.unwrap_or(state.badge.is_some());
        let n = count
            .map(|c| c as i64)
            .or_else(|| state.badge.as_deref().and_then(|b| b.parse().ok()));
        next.badge = match n {
            Some(n) if visible && n > 0 => Some(badge_text(n)),
            _ => None,
        };
    }
    let (progress, progress_visible) = (num("progress"), flag("progress-visible"));
    if progress.is_some() || progress_visible.is_some() {
        let visible = progress_visible.unwrap_or(state.progress.is_some());
        let p = progress.map(|p| p as f32).or(state.progress);
        next.progress = p.filter(|_| visible).map(|p| p.clamp(0.0, 1.0));
    }
    if let Some(u) = flag("urgent") {
        next.urgent_since = if u {
            Some(state.urgent_since.unwrap_or(now))
        } else {
            None
        };
    }
    next
}

/// Listens to the session bus; `update` runs on the main loop with (app id, properties).
pub fn listen(
    update: impl Fn(&mut vetro_lib::core::Ui, String, HashMap<String, Prop>) + Clone + 'static,
) {
    vetro_lib::platform::spawn(async move {
        use futures_lite::StreamExt;
        let Some(services) = vetro_lib::platform::services() else {
            return;
        };
        let Ok(conn) = services.connection().await else {
            return;
        };
        let Ok(rule) = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("com.canonical.Unity.LauncherEntry")
            .and_then(|b| b.member("Update"))
            .map(|b| b.build())
        else {
            return;
        };
        let Ok(mut stream) = zbus::MessageStream::for_match_rule(rule, conn, None).await else {
            return;
        };
        while let Some(Ok(msg)) = stream.next().await {
            let Ok((uri, map)) = msg
                .body()
                .deserialize::<(String, HashMap<String, OwnedValue>)>()
            else {
                continue;
            };
            let Some(id) = app_id(&uri) else { continue };
            let p = props(&map);
            let f = update.clone();
            vetro_lib::platform::post(move |ui| f(ui, id, p));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pairs: &[(&str, Prop)]) -> HashMap<String, Prop> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn updates_set_badge_progress_and_attention() {
        let now = 100.0;
        let mut s = apply(
            &LauncherState::default(),
            &p(&[
                ("count", Prop::Num(3.0)),
                ("count-visible", Prop::Bool(true)),
            ]),
            now,
        );
        assert_eq!(s.badge.as_deref(), Some("3"));
        s = apply(&s, &p(&[("count", Prop::Num(1234.0))]), now);
        assert_eq!(s.badge.as_deref(), Some("1234"));
        s = apply(&s, &p(&[("count-visible", Prop::Bool(false))]), now);
        assert_eq!(s.badge, None);
        s = apply(
            &s,
            &p(&[
                ("progress", Prop::Num(0.5)),
                ("progress-visible", Prop::Bool(true)),
            ]),
            now,
        );
        assert_eq!(s.progress, Some(0.5));
        s = apply(&s, &p(&[("urgent", Prop::Bool(true))]), now);
        assert_eq!(s.urgent_since, Some(now));
        // Already urgent: the hops keep their rhythm.
        s = apply(&s, &p(&[("urgent", Prop::Bool(true))]), now + 1.0);
        assert_eq!(s.urgent_since, Some(now));
        s = apply(
            &s,
            &p(&[
                ("urgent", Prop::Bool(false)),
                ("progress-visible", Prop::Bool(false)),
            ]),
            now,
        );
        assert!(s.is_empty());
        assert_eq!(
            app_id("application://firefox.desktop").as_deref(),
            Some("firefox")
        );
        assert_eq!(app_id("file:///x"), None);
    }
}
