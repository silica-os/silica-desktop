//! `org.freedesktop.impl.portal.Settings`: Silica's appearance for every app
//! (`org.freedesktop.appearance` › `color-scheme`: 0 no preference, 1 dark, 2 light).
//! The source is the daemon (`dev.frascella.Silica.System1`, key `appearance.dark`); without
//! the daemon, the file `$XDG_CONFIG_HOME/silica/appearance.json`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

pub const APPEARANCE: &str = "org.freedesktop.appearance";
pub const COLOR_SCHEME: &str = "color-scheme";
pub const ACCENT_COLOR: &str = "accent-color";
/// No accent chosen (the multicolour accent): `accent-color` outside 0–1, as the portal
/// specification says for "no preference".
pub const NO_ACCENT: u32 = u32::MAX;
pub const DAEMON_NAME: &str = "dev.frascella.Silica";
pub const DAEMON_PATH: &str = "/dev/frascella/Silica/System1";
pub const DAEMON_INTERFACE: &str = "dev.frascella.Silica.System1";

/// The `color-scheme` value for the appearance.
pub fn color_scheme(dark: bool) -> u32 {
    if dark { 1 } else { 2 }
}

/// The `accent-color` value `(ddd)`: the accent as red, green and blue 0–1, or out of range when
/// none (`NO_ACCENT`).
pub fn accent_color(rgb: u32) -> (f64, f64, f64) {
    if rgb == NO_ACCENT {
        return (-1.0, -1.0, -1.0);
    }
    let channel = |shift: u32| f64::from((rgb >> shift) & 0xFF) / 255.0;
    (channel(16), channel(8), channel(0))
}

/// The colour of an accent by name (the light appearance's, as silica_ui draws it);
/// `NO_ACCENT` for `multicolour` and unknown names.
pub fn accent_rgb(name: &str) -> u32 {
    match name {
        "blue" => 0x007AFF,
        "purple" => 0x953D96,
        "pink" => 0xF74F9F,
        "red" => 0xE0393E,
        "orange" => 0xF98A17,
        "yellow" => 0xFFC727,
        "green" => 0x62BA46,
        "graphite" => 0x989898,
        _ => NO_ACCENT,
    }
}

/// `$XDG_CONFIG_HOME/silica/appearance.json` (usually `~/.config`).
pub fn appearance_file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("silica").join("appearance.json"))
}

/// `{"dark": true}` in the file; without the file, light.
pub fn read_file(file: &Path) -> bool {
    std::fs::read_to_string(file).is_ok_and(|text| text.replace(' ', "").contains("\"dark\":true"))
}

/// A namespace asked by `ReadAll`: empty, or with a trailing `*` as a prefix.
fn matches(pattern: &str, namespace: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => namespace.starts_with(prefix),
        None => pattern.is_empty() || pattern == namespace,
    }
}

#[derive(Clone)]
pub struct Settings {
    pub dark: Arc<AtomicBool>,
    /// The accent as `0xRRGGBB`, or `NO_ACCENT`.
    pub accent: Arc<AtomicU32>,
}

impl Settings {
    fn appearance(&self) -> HashMap<String, OwnedValue> {
        let scheme = OwnedValue::from(color_scheme(self.dark.load(Ordering::Relaxed)));
        let accent = Value::from(accent_color(self.accent.load(Ordering::Relaxed)));
        let mut map = HashMap::from([(COLOR_SCHEME.to_owned(), scheme)]);
        if let Ok(accent) = OwnedValue::try_from(accent) {
            map.insert(ACCENT_COLOR.to_owned(), accent);
        }
        map
    }
}

#[interface(name = "org.freedesktop.impl.portal.Settings")]
impl Settings {
    async fn read_all(
        &self,
        namespaces: Vec<String>,
    ) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut out = HashMap::new();
        if namespaces.is_empty() || namespaces.iter().any(|p| matches(p, APPEARANCE)) {
            out.insert(APPEARANCE.to_owned(), self.appearance());
        }
        out
    }

    async fn read(&self, namespace: String, key: String) -> zbus::fdo::Result<OwnedValue> {
        if namespace == APPEARANCE
            && let Some(value) = self.appearance().remove(&key)
        {
            return Ok(value);
        }
        Err(zbus::fdo::Error::Failed(format!(
            "unknown setting: {namespace} {key}"
        )))
    }

    #[zbus(signal)]
    pub async fn setting_changed(
        emitter: &SignalEmitter<'_>,
        namespace: &str,
        key: &str,
        value: Value<'_>,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    fn version(&self) -> u32 {
        2
    }
}

/// The daemon state (`State`): `appearance.dark`, if present.
fn dark_in(state: &HashMap<String, OwnedValue>) -> Option<bool> {
    state
        .get("appearance.dark")
        .and_then(|v| bool::try_from(v).ok())
}

/// The daemon state (`State`): the accent in `look.accent`, if present, as `0xRRGGBB`.
fn accent_in(state: &HashMap<String, OwnedValue>) -> Option<u32> {
    let name = state.get("look.accent")?;
    let name = <&str>::try_from(name).ok()?;
    Some(accent_rgb(name))
}

/// Follows the daemon: reads the state and listens to `Changed`; on every appearance change it
/// updates `dark` and emits `SettingChanged`. Returns when the daemon is gone or the bus closes.
pub async fn follow_daemon(
    connection: &zbus::Connection,
    settings: &Settings,
    path: &str,
) -> zbus::Result<()> {
    use futures_lite::StreamExt;
    let proxy = zbus::Proxy::new(connection, DAEMON_NAME, DAEMON_PATH, DAEMON_INTERFACE).await?;
    let mut changes = proxy.receive_signal("Changed").await?;
    let state: HashMap<String, OwnedValue> = proxy.call("State", &()).await?;
    let emitter = SignalEmitter::new(connection, path.to_owned())?;
    let publish = async |dark: bool| -> zbus::Result<()> {
        if settings.dark.swap(dark, Ordering::Relaxed) != dark {
            Settings::setting_changed(
                &emitter,
                APPEARANCE,
                COLOR_SCHEME,
                Value::from(color_scheme(dark)),
            )
            .await?;
        }
        Ok(())
    };
    let publish_accent = async |rgb: u32| -> zbus::Result<()> {
        if settings.accent.swap(rgb, Ordering::Relaxed) != rgb {
            Settings::setting_changed(
                &emitter,
                APPEARANCE,
                ACCENT_COLOR,
                Value::from(accent_color(rgb)),
            )
            .await?;
        }
        Ok(())
    };
    if let Some(dark) = dark_in(&state) {
        publish(dark).await?;
    }
    if let Some(rgb) = accent_in(&state) {
        publish_accent(rgb).await?;
    }
    while let Some(message) = changes.next().await {
        let Ok(state) = message.body().deserialize::<HashMap<String, OwnedValue>>() else {
            continue;
        };
        if let Some(dark) = dark_in(&state) {
            publish(dark).await?;
        }
        if let Some(rgb) = accent_in(&state) {
            publish_accent(rgb).await?;
        }
    }
    Ok(())
}
