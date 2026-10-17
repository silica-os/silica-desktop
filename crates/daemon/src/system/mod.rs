//! The system state for the Control Center and the menu bar:
//! `dev.frascella.Silica.System1` on `/dev/frascella/Silica/System1`.
//!
//! The sources are the system bus services (NetworkManager, BlueZ, UPower, logind),
//! the backlight in sysfs and PipeWire audio with `wpctl` (`backends.rs`). Every
//! source is optional: if it is missing, its keys say `available = false` and the rest
//! works. The state is read all together (`State`) and every change emits `Changed`
//! with the whole state: for the shell that is simpler than one signal per property.

mod backends;
mod details;
pub mod display;
pub mod focus;
pub mod look;

pub use details::{Network, Output, parse_outputs};
pub use display::NightLight;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use event_listener::Event;
use futures_lite::future;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, interface};

pub const SYSTEM_PATH: &str = "/dev/frascella/Silica/System1";
pub const SYSTEM_INTERFACE: &str = "dev.frascella.Silica.System1";

/// Where the daemon finds the sources; tests replace them with fake services.
#[derive(Clone, Debug)]
pub struct SystemConfig {
    /// System bus (NetworkManager, BlueZ, UPower, logind); `None` if absent.
    pub system_bus: Option<Connection>,
    /// The backlight directory (`/sys/class/backlight`).
    pub backlight_dir: PathBuf,
    /// PipeWire's `wpctl` program.
    pub wpctl: PathBuf,
    /// How often to reread brightness and volume, which have no signals.
    pub poll: Duration,
    /// Where the appearance (light or dark) is saved; `None`: only in memory.
    pub appearance_file: Option<PathBuf>,
    /// `theme.json`, the look Settings saved (read at start; Settings tells the daemon with
    /// `SetLook`).
    pub theme_file: Option<PathBuf>,
    /// Where the night light is saved (`nightlight.json`); `None`: only in memory.
    pub nightlight_file: Option<PathBuf>,
    /// The Focus modes (`focus.json`).
    pub focus_file: Option<PathBuf>,
    /// Settings › Energy (`power.json`): `preventSleep` holds logind's idle inhibitor.
    pub power_file: Option<PathBuf>,
    /// Ossido's actions socket; `None`: the one of `WAYLAND_DISPLAY`.
    pub actions_socket: Option<PathBuf>,
    /// Where the sun is computed from; `None` at start is filled from the time zone.
    pub place: Option<crate::sun::Place>,
    /// The local UTC offset in seconds; `None`: from `date +%z` at every check.
    pub utc_offset: Option<i64>,
    /// How often the schedules (night light) are checked.
    pub schedule_tick: Duration,
}

/// `$XDG_CONFIG_HOME/silica/appearance.json` (usually `~/.config`).
pub fn default_appearance_file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("silica").join("appearance.json"))
}

/// The appearance modes: fixed light or dark, or `auto` (dark from sunset to sunrise).
pub const APPEARANCE_MODES: &[&str] = &["light", "dark", "auto"];

/// The saved mode: `"mode"` in `appearance.json`, otherwise from `"dark"` (files written
/// before the mode existed).
pub fn read_appearance_mode(file: &std::path::Path) -> String {
    let json = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let mode = json
        .as_ref()
        .and_then(|j| j.get("mode"))
        .and_then(serde_json::Value::as_str)
        .filter(|m| APPEARANCE_MODES.contains(m));
    match mode {
        Some(mode) => mode.to_owned(),
        None if read_appearance(file) => "dark".to_owned(),
        None => "light".to_owned(),
    }
}

fn write_appearance(file: &std::path::Path, mode: &str, dark: bool) -> Result<(), String> {
    display::write_json(file, &serde_json::json!({ "mode": mode, "dark": dark }))
}

/// The saved appearance: `{"dark": true}`; without the file or unreadable, light.
pub fn read_appearance(file: &std::path::Path) -> bool {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json.get("dark").and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            system_bus: None,
            backlight_dir: PathBuf::from("/sys/class/backlight"),
            wpctl: PathBuf::from("wpctl"),
            poll: Duration::from_secs(1),
            appearance_file: default_appearance_file(),
            theme_file: display::config_file("theme.json"),
            nightlight_file: display::config_file("nightlight.json"),
            power_file: display::config_file("power.json"),
            focus_file: display::config_file("focus.json"),
            actions_socket: None,
            place: crate::sun::local_place(),
            utc_offset: None,
            schedule_tick: Duration::from_secs(30),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wifi {
    pub enabled: bool,
    /// The name of the primary connection, if wireless.
    pub ssid: String,
    /// 0–100.
    pub strength: u8,
    /// Wired primary connection.
    pub wired: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Battery {
    pub percent: f64,
    pub charging: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Audio {
    /// 0–1.
    pub volume: f64,
    pub muted: bool,
    /// The name of the default output (PipeWire's `node.description`).
    pub name: String,
}

/// An app that recorded with the microphone or the camera: now (`in_use`) or recently.
#[derive(Clone, Debug, PartialEq)]
pub struct PrivacyUse {
    pub app: String,
    /// `microphone` or `camera`.
    pub kind: &'static str,
    pub in_use: bool,
}

/// A value Ossido reports (`ReportCompositor`).
#[derive(Clone, Debug, PartialEq)]
pub enum Reported {
    Bool(bool),
    Number(f64),
    Text(String),
}

/// The keys Ossido may report, and only these: pointer kinds, outputs.
pub const REPORTED_KEYS: &[&str] = &["input.touchpad", "input.mouse", "displays"];

/// The report's JSON object: the known keys with a boolean, number or string value; the rest
/// is ignored. `None` if it is not a JSON object.
pub fn parse_report(json: &str) -> Option<std::collections::BTreeMap<String, Reported>> {
    let serde_json::Value::Object(map) = serde_json::from_str(json).ok()? else {
        return None;
    };
    Some(
        map.into_iter()
            .filter(|(key, _)| REPORTED_KEYS.contains(&key.as_str()))
            .filter_map(|(key, value)| {
                let value = match value {
                    serde_json::Value::Bool(b) => Reported::Bool(b),
                    serde_json::Value::Number(n) => Reported::Number(n.as_f64()?),
                    serde_json::Value::String(s) if s.len() <= 65536 => Reported::Text(s),
                    _ => return None,
                };
                Some((key, value))
            })
            .collect(),
    )
}

/// The whole state; `None` means an absent source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemState {
    pub wifi: Option<Wifi>,
    pub bluetooth: Option<bool>,
    pub battery: Option<Battery>,
    /// 0–1.
    pub brightness: Option<f64>,
    /// How many times the brightness was requested with `SetBrightness` (Silica's keys and
    /// sliders). A change coming from outside (adaptive brightness, another app) changes
    /// `brightness` but not this: the HUD appears only when this changes.
    pub brightness_requests: u64,
    pub audio: Option<Audio>,
    pub do_not_disturb: bool,
    /// The active Focus mode (`dnd` for Do Not Disturb), if any.
    pub focus: Option<focus::FocusMode>,
    /// Dark appearance (the whole system: Ossido, shell, apps through the Settings portal).
    pub dark: bool,
    /// `light`, `dark` or `auto` (follows the sun).
    pub appearance_mode: String,
    /// The Appearance choices apps follow live (accent, scroll bars, icon style…).
    pub look: look::Look,
    /// The apps recording now or recently, the ones in use first (the privacy pill).
    pub privacy: Vec<PrivacyUse>,
    /// The keyboard backlight, 0–1; `None` without a backlit keyboard.
    pub keyboard_brightness: Option<f64>,
    /// How many times Silica set it (`SetKeyboardBrightness`): the HUD appears only then.
    pub keyboard_requests: u64,
    /// What Ossido reported (`input.touchpad`, `input.mouse`, …).
    pub compositor: std::collections::BTreeMap<String, Reported>,
    /// The night light's settings and whether it is warming the displays now.
    pub night_light: NightLight,
    pub night_light_active: bool,
    /// `preventSleep` of `power.json`, and whether logind's idle inhibitor is held for it.
    pub prevent_sleep: bool,
    pub sleep_inhibited: bool,
}

impl SystemState {
    /// The dictionary of `State` and `Changed`: flat keys, `*.available` per source.
    pub fn to_dict(&self) -> HashMap<String, OwnedValue> {
        let mut dict = HashMap::new();
        let mut put = |key: &str, value: Value<'_>| {
            if let Ok(owned) = OwnedValue::try_from(value) {
                dict.insert(key.to_owned(), owned);
            }
        };
        let wifi = self.wifi.clone().unwrap_or_default();
        put("wifi.available", Value::Bool(self.wifi.is_some()));
        put("wifi.enabled", Value::Bool(wifi.enabled));
        put("wifi.ssid", Value::from(wifi.ssid));
        put("wifi.strength", Value::U8(wifi.strength));
        put("wifi.wired", Value::Bool(wifi.wired));
        put("bluetooth.available", Value::Bool(self.bluetooth.is_some()));
        put(
            "bluetooth.powered",
            Value::Bool(self.bluetooth.unwrap_or(false)),
        );
        let battery = self.battery.clone().unwrap_or_default();
        put("battery.present", Value::Bool(self.battery.is_some()));
        put("battery.percent", Value::F64(battery.percent));
        put("battery.charging", Value::Bool(battery.charging));
        put(
            "brightness.available",
            Value::Bool(self.brightness.is_some()),
        );
        put("brightness", Value::F64(self.brightness.unwrap_or(0.0)));
        put("brightness.requests", Value::U64(self.brightness_requests));
        let audio = self.audio.clone().unwrap_or_default();
        put("audio.available", Value::Bool(self.audio.is_some()));
        put("audio.volume", Value::F64(audio.volume));
        put("audio.muted", Value::Bool(audio.muted));
        put("audio.name", Value::from(audio.name));
        put("dnd", Value::Bool(self.do_not_disturb));
        let mode = self.focus.clone().unwrap_or_default();
        put("focus", Value::from(mode.id));
        put("focus.name", Value::from(mode.name));
        put("focus.symbol", Value::from(mode.symbol));
        put("focus.allowedApps", Value::from(mode.allowed_apps));
        put("focus.allowedPeople", Value::from(mode.allowed_people));
        put("appearance.dark", Value::Bool(self.dark));
        put("appearance.mode", Value::from(self.appearance_mode.clone()));
        self.look.put(&mut put);
        // "app<US>kind<US>1|0" per app: a flat list the shell splits.
        let privacy: Vec<String> = self
            .privacy
            .iter()
            .map(|p| format!("{}\u{1f}{}\u{1f}{}", p.app, p.kind, u8::from(p.in_use)))
            .collect();
        put("privacy", Value::from(privacy));
        put(
            "keyboard.available",
            Value::Bool(self.keyboard_brightness.is_some()),
        );
        put(
            "keyboard.brightness",
            Value::F64(self.keyboard_brightness.unwrap_or(0.0)),
        );
        put("keyboard.requests", Value::U64(self.keyboard_requests));
        let light = &self.night_light;
        put("nightlight.enabled", Value::Bool(light.enabled));
        put("nightlight.temperature", Value::U32(light.temperature));
        put("nightlight.schedule", Value::from(light.schedule.clone()));
        put("nightlight.from", Value::from(light.from.clone()));
        put("nightlight.to", Value::from(light.to.clone()));
        put("nightlight.active", Value::Bool(self.night_light_active));
        put("power.preventSleep", Value::Bool(self.prevent_sleep));
        put("power.inhibited", Value::Bool(self.sleep_inhibited));
        for (key, value) in &self.compositor {
            match value {
                Reported::Bool(b) => put(key, Value::Bool(*b)),
                Reported::Number(n) => put(key, Value::F64(*n)),
                Reported::Text(s) => put(key, Value::from(s.as_str())),
            }
        }
        dict
    }
}

#[derive(Clone)]
struct Shared {
    state: Arc<Mutex<SystemState>>,
    /// Wakes the loop that emits `Changed`.
    changed: Arc<Event>,
    /// Wakes the schedules (a setting changed).
    schedule: Arc<Event>,
    /// logind's idle inhibitor while `preventSleep` is on: closing it releases it.
    inhibitor: Arc<Mutex<Option<zbus::zvariant::OwnedFd>>>,
    config: SystemConfig,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, SystemState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Changes the state; if it differs, wakes the emission of `Changed`.
    fn update(&self, change: impl FnOnce(&mut SystemState)) {
        let mut state = self.state();
        let before = state.clone();
        change(&mut state);
        if *state != before {
            drop(state);
            self.changed.notify(usize::MAX);
        }
    }
}

/// The `System1` service on a session bus connection.
pub struct SystemService {
    connection: Connection,
    shared: Shared,
}

impl SystemService {
    /// Publishes the interface; the sources start with `run`.
    pub async fn serve(connection: Connection, config: SystemConfig) -> zbus::Result<Self> {
        let dark = config
            .appearance_file
            .as_deref()
            .is_some_and(read_appearance);
        let shared = Shared {
            state: Arc::new(Mutex::new(SystemState {
                dark,
                appearance_mode: config
                    .appearance_file
                    .as_deref()
                    .map_or_else(|| "light".to_owned(), read_appearance_mode),
                look: config
                    .theme_file
                    .as_deref()
                    .map_or_else(look::Look::default, look::Look::read),
                ..SystemState::default()
            })),
            changed: Arc::new(Event::new()),
            schedule: Arc::new(Event::new()),
            inhibitor: Arc::new(Mutex::new(None)),
            config,
        };
        connection
            .object_server()
            .at(
                SYSTEM_PATH,
                System {
                    shared: shared.clone(),
                },
            )
            .await?;
        Ok(Self { connection, shared })
    }

    pub fn state(&self) -> SystemState {
        self.shared.state().clone()
    }

    /// Reads the sources and follows their changes; returns only on a session bus
    /// error.
    pub async fn run(&self) -> zbus::Result<()> {
        let sources = backends::run(&self.shared);
        let emit = async {
            let mut last = None;
            loop {
                let listener = self.shared.changed.listen();
                let state = self.shared.state().clone();
                if last.as_ref() != Some(&state) {
                    let emitter = SignalEmitter::new(&self.connection, SYSTEM_PATH)?;
                    System::changed(&emitter, state.to_dict()).await?;
                    last = Some(state);
                }
                listener.await;
            }
        };
        let schedules = async {
            let mut last = None;
            let mut first = true;
            loop {
                let listener = self.shared.schedule.listen();
                display::night_light_tick(&self.shared, &mut last, first).await;
                appearance_tick(&self.shared).await;
                power_tick(&self.shared).await;
                first = false;
                future::or(
                    async {
                        async_io::Timer::after(self.shared.config.schedule_tick).await;
                    },
                    listener,
                )
                .await;
            }
        };
        future::or(
            async {
                future::zip(sources, schedules).await;
                future::pending::<zbus::Result<()>>().await
            },
            emit,
        )
        .await
    }
}

struct System {
    shared: Shared,
}

/// `preventSleep`: holds logind's `idle` inhibitor while it is on (the idle action of logind
/// does not suspend; a sleep the user asks for still happens), releases it when it goes off.
async fn power_tick(shared: &Shared) {
    let prevent = shared.config.power_file.as_deref().is_some_and(|file| {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|json| {
                json.get("preventSleep")
                    .and_then(serde_json::Value::as_bool)
            })
            .unwrap_or(false)
    });
    let held = shared
        .inhibitor
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some();
    if prevent && !held {
        if let Some(bus) = &shared.config.system_bus {
            let reply = bus
                .call_method(
                    Some("org.freedesktop.login1"),
                    "/org/freedesktop/login1",
                    Some("org.freedesktop.login1.Manager"),
                    "Inhibit",
                    &(
                        "idle",
                        "Silica",
                        "Settings: prevent automatic sleep",
                        "block",
                    ),
                )
                .await;
            match reply.and_then(|r| r.body().deserialize::<zbus::zvariant::OwnedFd>()) {
                Ok(fd) => {
                    *shared.inhibitor.lock().unwrap_or_else(|e| e.into_inner()) = Some(fd);
                }
                Err(err) => tracing::warn!("logind inhibitor: {err}"),
            }
        }
    } else if !prevent && held {
        *shared.inhibitor.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    let inhibited = shared
        .inhibitor
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some();
    shared.update(|state| {
        state.prevent_sleep = prevent;
        state.sleep_inhibited = inhibited;
    });
}

/// Whether it is night now at the configured place.
async fn night_now(shared: &Shared) -> bool {
    let offset = match shared.config.utc_offset {
        Some(offset) => offset,
        None => crate::sun::local_offset().await,
    };
    crate::sun::is_night(shared.config.place, crate::sun::unix_now(), offset)
}

/// The automatic appearance: the mode is read again from its file (Settings may write it),
/// and in `auto` the dark appearance follows the sun; a change is saved for the portal.
async fn appearance_tick(shared: &Shared) {
    let file = shared.config.appearance_file.clone();
    let mode = match &file {
        Some(file) => read_appearance_mode(file),
        None => shared.state().appearance_mode.clone(),
    };
    let dark = match mode.as_str() {
        "auto" => night_now(shared).await,
        "dark" => true,
        _ => false,
    };
    let changed = {
        let state = shared.state();
        state.dark != dark || state.appearance_mode != mode
    };
    if !changed {
        return;
    }
    if let Some(file) = &file
        && let Err(err) = write_appearance(file, &mode, dark)
    {
        tracing::warn!("appearance: {err}");
    }
    shared.update(|state| {
        state.dark = dark;
        state.appearance_mode = mode;
    });
}

fn failed(error: impl std::fmt::Display) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(error.to_string())
}

#[interface(name = "dev.frascella.Silica.System1")]
impl System {
    /// The whole state.
    fn state(&self) -> HashMap<String, OwnedValue> {
        self.shared.state().to_dict()
    }

    async fn set_wifi_enabled(&self, enabled: bool) -> zbus::fdo::Result<()> {
        backends::set_wifi_enabled(&self.shared, enabled)
            .await
            .map_err(failed)
    }

    async fn set_bluetooth_powered(&self, powered: bool) -> zbus::fdo::Result<()> {
        backends::set_bluetooth_powered(&self.shared, powered)
            .await
            .map_err(failed)
    }

    /// Volume of the default output, 0–1.
    async fn set_volume(&self, volume: f64) -> zbus::fdo::Result<()> {
        if !volume.is_finite() {
            return Err(zbus::fdo::Error::InvalidArgs("non-finite volume".into()));
        }
        backends::set_volume(&self.shared, volume.clamp(0.0, 1.0))
            .await
            .map_err(failed)
    }

    async fn set_muted(&self, muted: bool) -> zbus::fdo::Result<()> {
        backends::set_muted(&self.shared, muted)
            .await
            .map_err(failed)
    }

    /// Screen brightness, 0–1.
    async fn set_brightness(&self, brightness: f64) -> zbus::fdo::Result<()> {
        if !brightness.is_finite() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "non-finite brightness".into(),
            ));
        }
        backends::set_brightness(&self.shared, brightness.clamp(0.0, 1.0))
            .await
            .map_err(failed)?;
        self.shared
            .update(|s| s.brightness_requests = s.brightness_requests.wrapping_add(1));
        Ok(())
    }

    /// The keyboard backlight, from 0 to 1 (UPower's `KbdBacklight`).
    async fn set_keyboard_brightness(&self, brightness: f64) -> zbus::fdo::Result<()> {
        if !brightness.is_finite() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "non-finite brightness".into(),
            ));
        }
        backends::set_keyboard_brightness(&self.shared, brightness)
            .await
            .map_err(failed)?;
        self.shared
            .update(|s| s.keyboard_requests = s.keyboard_requests.wrapping_add(1));
        Ok(())
    }

    /// The visible Wi-Fi networks: (name, signal 0–100, protected, saved, in use), the one in
    /// use first, then the saved ones, then by signal.
    async fn wifi_networks(&self) -> zbus::fdo::Result<Vec<(String, u8, bool, bool, bool)>> {
        let list = details::wifi_networks(&self.shared).await.map_err(failed)?;
        Ok(list
            .into_iter()
            .map(|n| (n.ssid, n.strength, n.secure, n.known, n.active))
            .collect())
    }

    /// Connects to a saved network.
    async fn connect_wifi(&self, ssid: String) -> zbus::fdo::Result<()> {
        details::connect_wifi(&self.shared, &ssid)
            .await
            .map_err(failed)
    }

    /// The paired Bluetooth devices: (address, name, icon name, connected), connected first.
    async fn bluetooth_devices(&self) -> zbus::fdo::Result<Vec<(String, String, String, bool)>> {
        let list = details::bluetooth_devices(&self.shared)
            .await
            .map_err(failed)?;
        Ok(list
            .into_iter()
            .map(|d| (d.address, d.name, d.icon, d.connected))
            .collect())
    }

    /// Connects or disconnects a paired Bluetooth device.
    async fn connect_bluetooth(&self, address: String, connect: bool) -> zbus::fdo::Result<()> {
        details::connect_bluetooth(&self.shared, &address, connect)
            .await
            .map_err(failed)
    }

    /// The audio outputs: (id, name, default).
    async fn audio_outputs(&self) -> zbus::fdo::Result<Vec<(u32, String, bool)>> {
        let list = details::audio_outputs(&self.shared.config.wpctl)
            .await
            .map_err(failed)?;
        Ok(list
            .into_iter()
            .map(|o| (o.id, o.name, o.default))
            .collect())
    }

    /// Chooses the default audio output.
    async fn set_audio_output(&self, id: u32) -> zbus::fdo::Result<()> {
        details::set_audio_output(&self.shared.config.wpctl, id)
            .await
            .map_err(failed)
    }

    /// Do Not Disturb: the shell does not show banners.
    fn set_do_not_disturb(&self, enabled: bool) {
        self.shared.update(|state| {
            state.do_not_disturb = enabled;
            state.focus = enabled.then(focus::FocusMode::do_not_disturb);
        });
    }

    /// Dark or light appearance, for the whole system; it is saved (`appearance.json`).
    /// Also fixes the mode to `light` or `dark` (it leaves `auto`).
    fn set_dark(&self, dark: bool) -> zbus::fdo::Result<()> {
        let mode = if dark { "dark" } else { "light" };
        if let Some(file) = &self.shared.config.appearance_file {
            write_appearance(file, mode, dark).map_err(failed)?;
        }
        self.shared.update(|state| {
            state.dark = dark;
            state.appearance_mode = mode.to_owned();
        });
        Ok(())
    }

    /// `light`, `dark` or `auto`: in `auto` the daemon turns the dark appearance on at sunset
    /// and off at sunrise (the time zone's coordinates; 19:00–07:00 without them). Saved in
    /// `appearance.json` (`mode`).
    async fn set_appearance_mode(&self, mode: &str) -> zbus::fdo::Result<()> {
        if !APPEARANCE_MODES.contains(&mode) {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "appearance mode: light, dark or auto, not {mode:?}"
            )));
        }
        let dark = match mode {
            "dark" => true,
            "light" => false,
            _ => night_now(&self.shared).await,
        };
        if let Some(file) = &self.shared.config.appearance_file {
            write_appearance(file, mode, dark).map_err(failed)?;
        }
        self.shared.update(|state| {
            state.dark = dark;
            state.appearance_mode = mode.to_owned();
        });
        Ok(())
    }

    /// The system look: any of `accent`, `highlight`, `sidebarIconSize`, `scrollBars`,
    /// `scrollClick`, `iconStyle`, `folderColour` (strings), `glass` (d, 0–1), `windowTint` (b).
    /// Apps follow it live through `State`; Settings keeps it in `theme.json`.
    fn set_look(&self, settings: HashMap<String, OwnedValue>) -> zbus::fdo::Result<()> {
        let mut next = self.shared.state().look.clone();
        if !next.merge(&look::settings_json(&settings)) {
            return Err(zbus::fdo::Error::InvalidArgs(
                "look: accent and folderColour/highlight (an accent name, or automatic), \
                 sidebarIconSize small|medium|large, scrollBars automatic|whenScrolling|always, \
                 scrollClick page|spot, iconStyle default|dark|clear|tinted, glass 0–1, \
                 windowTint bool"
                    .into(),
            ));
        }
        self.shared.update(|state| state.look = next);
        Ok(())
    }

    /// The displays, from Ossido (`output.list`): a JSON array.
    async fn outputs(&self) -> zbus::fdo::Result<String> {
        display::ossido(
            &self.shared,
            silica_control::commands::OUTPUT_LIST,
            serde_json::json!({}),
        )
        .await
        .map(|list| list.to_string())
        .map_err(failed)
    }

    /// Changes a display (`output.configure`: `{connector?, mode?, scale?, transform?}` as JSON)
    /// and returns it as JSON. Ossido saves the choice for that display.
    async fn configure_output(&self, json: &str) -> zbus::fdo::Result<String> {
        let params: serde_json::Value =
            serde_json::from_str(json).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        if !params.is_object() {
            return Err(zbus::fdo::Error::InvalidArgs("not a JSON object".into()));
        }
        display::ossido(
            &self.shared,
            silica_control::commands::OUTPUT_CONFIGURE,
            params,
        )
        .await
        .map(|output| output.to_string())
        .map_err(failed)
    }

    /// The night light: any of `enabled` (b), `temperature` (u, 1900–6500 K), `schedule`
    /// (`off`, `sunset`, `custom`), `from` and `to` (`HH:MM`). Saved in `nightlight.json`.
    fn set_night_light(&self, settings: HashMap<String, OwnedValue>) -> zbus::fdo::Result<()> {
        let mut json = serde_json::Map::new();
        for (key, value) in &settings {
            let value = match &**value {
                Value::Bool(b) => serde_json::Value::from(*b),
                Value::U32(n) => serde_json::Value::from(*n),
                Value::I32(n) => serde_json::Value::from(*n),
                Value::U64(n) => serde_json::Value::from(*n),
                Value::I64(n) => serde_json::Value::from(*n),
                Value::F64(n) => serde_json::Value::from(n.round() as i64),
                Value::Str(s) => serde_json::Value::from(s.as_str()),
                _ => serde_json::Value::Null,
            };
            json.insert(key.clone(), value);
        }
        let mut light = match &self.shared.config.nightlight_file {
            Some(file) => display::read_night_light(file),
            None => self.shared.state().night_light.clone(),
        };
        if !light.merge(&serde_json::Value::Object(json)) {
            return Err(zbus::fdo::Error::InvalidArgs(
                "night light: enabled, temperature 1900–6500, schedule off|sunset|custom, from/to HH:MM"
                    .into(),
            ));
        }
        if let Some(file) = &self.shared.config.nightlight_file {
            display::write_json(file, &light.to_json()).map_err(failed)?;
        }
        self.shared.update(|state| state.night_light = light);
        self.shared.schedule.notify(usize::MAX);
        Ok(())
    }

    /// Turns a Focus mode on (`id` of `focus.json`, or `dnd`), or off with an empty id. Any
    /// mode silences banners like Do Not Disturb (`dnd` follows it); the shell lets through the
    /// mode's allowed apps. Saved as `active` in `focus.json`.
    fn set_focus(&self, id: &str) -> zbus::fdo::Result<()> {
        let modes = self
            .shared
            .config
            .focus_file
            .as_deref()
            .map(focus::read_modes)
            .unwrap_or_default();
        let mode =
            if id.is_empty() {
                None
            } else {
                Some(focus::find(&modes, id).ok_or_else(|| {
                    zbus::fdo::Error::InvalidArgs(format!("no Focus mode {id:?}"))
                })?)
            };
        if let Some(file) = &self.shared.config.focus_file {
            focus::save_active(file, id).map_err(failed)?;
        }
        self.shared.update(|state| {
            state.do_not_disturb = mode.is_some();
            state.focus = mode;
        });
        Ok(())
    }

    /// Sleeps now (logind's `Suspend`): Ossido asks when the idle time of `power.json` is up.
    async fn suspend(&self) -> zbus::fdo::Result<()> {
        let bus = self
            .shared
            .config
            .system_bus
            .as_ref()
            .ok_or_else(|| failed("no system bus"))?;
        bus.call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "Suspend",
            &(false,),
        )
        .await
        .map(|_| ())
        .map_err(failed)
    }

    /// Ossido's report (a JSON object of the keys in `REPORTED_KEYS`): they join the state.
    fn report_compositor(&self, json: &str) -> zbus::fdo::Result<()> {
        let report = parse_report(json)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a JSON object".into()))?;
        self.shared.update(|state| state.compositor = report);
        Ok(())
    }

    #[zbus(signal)]
    async fn changed(
        emitter: &SignalEmitter<'_>,
        state: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}
