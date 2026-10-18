//! The Settings service: `dev.frascella.Silica.Settings1` on `/dev/frascella/Silica/Settings1`.
//!
//! One typed store for the settings the shell applies live and the Settings app edits. Every
//! key has a schema (name, D-Bus type, default, range or set of values) in [`KEYS`]; a value
//! that does not fit is refused with `InvalidArgs`, so a reader never meets a broken one.
//!
//! - `Get(s key) -> v`, `GetAll() -> a{sv}` (every key, defaults included), `Set(s key, v)`,
//!   `SetMany(a{sv})` (all or none, one `Changed` per key), `Reset(s key)` (back to the
//!   default), `Changed(s key, v value)`.
//! - Types: `b` bool, `x` integer (range), `d` number (range), `s` text (an enum has a fixed
//!   set), `as` and `ad` lists.
//! - Per-app notification keys are `notifications.apps.<app id>.<field>`.
//! - On disk: a compact binary file, `$XDG_CONFIG_HOME/silica/settings.bin`, holding only the
//!   values that differ from the default. The first time (no file yet) the values of the old
//!   JSON files in the same folder are imported ([`import_json`]); they are left in place.
//!   Format: `SSET`, version `1`, count `u32`, then per entry the key (`u16` length, UTF-8),
//!   a tag (0 bool, 1 integer, 2 number, 3 text, 4 text list, 5 number list) and the payload
//!   (little endian; text: `u32` length; lists: `u32` count). A damaged file counts as empty.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::Value as Json;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{Array, Value};
use zbus::{fdo, interface};

pub const SETTINGS_PATH: &str = "/dev/frascella/Silica/Settings1";
const FILE_NAME: &str = "settings.bin";
const MAGIC: &[u8; 4] = b"SSET";
const VERSION: u8 = 1;
/// Longest text, list entry and key accepted.
const MAX_TEXT: usize = 4096;
const MAX_LIST: usize = 256;

/// A value of the store.
#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    Bool(bool),
    Int(i64),
    Num(f64),
    Text(String),
    Texts(Vec<String>),
    Nums(Vec<f64>),
}

impl Setting {
    /// The D-Bus variant of the value.
    pub fn to_variant(&self) -> Value<'static> {
        match self {
            Self::Bool(b) => Value::Bool(*b),
            Self::Int(i) => Value::I64(*i),
            Self::Num(d) => Value::F64(*d),
            Self::Text(s) => Value::from(s.clone()),
            Self::Texts(list) => Value::Array(Array::from(list.clone())),
            Self::Nums(list) => Value::Array(Array::from(list.clone())),
        }
    }
}

/// What a key accepts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Bool,
    Int(i64, i64),
    Num(f64, f64),
    Text,
    Enum(&'static [&'static str]),
    Texts,
    /// Empty, or exactly this many numbers.
    Nums(usize),
}

/// A key's default.
#[derive(Debug, Clone, Copy)]
enum Dflt {
    Bool(bool),
    Int(i64),
    Text(&'static str),
    /// The empty list.
    Empty,
}

/// A key of the schema.
#[derive(Debug, Clone, Copy)]
pub struct KeyDef {
    pub name: &'static str,
    pub kind: Kind,
    default: Dflt,
}

impl KeyDef {
    pub fn default_value(&self) -> Setting {
        match (self.default, self.kind) {
            (Dflt::Bool(b), _) => Setting::Bool(b),
            (Dflt::Int(i), Kind::Num(..)) => Setting::Num(i as f64),
            (Dflt::Int(i), _) => Setting::Int(i),
            (Dflt::Text(s), _) => Setting::Text(s.to_owned()),
            (Dflt::Empty, Kind::Nums(_)) => Setting::Nums(Vec::new()),
            (Dflt::Empty, _) => Setting::Texts(Vec::new()),
        }
    }

    /// The D-Bus signature of the values.
    pub fn signature(&self) -> &'static str {
        match self.kind {
            Kind::Bool => "b",
            Kind::Int(..) => "x",
            Kind::Num(..) => "d",
            Kind::Text | Kind::Enum(_) => "s",
            Kind::Texts => "as",
            Kind::Nums(_) => "ad",
        }
    }
}

const fn k(name: &'static str, kind: Kind, default: Dflt) -> KeyDef {
    KeyDef {
        name,
        kind,
        default,
    }
}

const SOUND: &[&str] = &["always", "active", "never"];
const CLOCK_DATE: &[&str] = &["auto", "always", "never"];
const AUTOHIDE: &[&str] = &["never", "fullscreen", "always"];
const POSITION: &[&str] = &["left", "bottom", "right"];
const MINIMIZE: &[&str] = &["genie", "scale"];
const LAUNCH: &[&str] = &["bounce", "none"];
const PREVIEWS: &[&str] = &["always", "unlocked", "never"];
const GROUPING: &[&str] = &["auto", "app", "off"];
const FIT: &[&str] = &["fill", "fit", "stretch", "center", "tile"];
const SAVERS: &[&str] = &["drift", "clock", "colors"];
const SHOT_MODE: &[&str] = &["screen", "window", "selection"];

/// Every static key. `passwordDelay` is in seconds, -1 is never. The wallpaper is the first
/// of `image`, `color`, `id` that is not empty.
pub const KEYS: &[KeyDef] = &[
    k("dock.magnify", Kind::Bool, Dflt::Bool(false)),
    k("dock.recents", Kind::Bool, Dflt::Bool(false)),
    k("dock.autohide", Kind::Bool, Dflt::Bool(false)),
    k("dock.position", Kind::Enum(POSITION), Dflt::Text("bottom")),
    k(
        "dock.minimizeEffect",
        Kind::Enum(MINIMIZE),
        Dflt::Text("genie"),
    ),
    k("dock.indicators", Kind::Bool, Dflt::Bool(true)),
    k("dock.tileSize", Kind::Int(16, 128), Dflt::Int(60)),
    k("dock.magnifiedSize", Kind::Int(16, 128), Dflt::Int(64)),
    k(
        "dock.launchAnimation",
        Kind::Enum(LAUNCH),
        Dflt::Text("bounce"),
    ),
    k("dock.minimizeToApp", Kind::Bool, Dflt::Bool(false)),
    k("menubar.sound", Kind::Enum(SOUND), Dflt::Text("active")),
    k("menubar.bluetooth", Kind::Bool, Dflt::Bool(false)),
    k("menubar.battery", Kind::Bool, Dflt::Bool(true)),
    k("menubar.batteryPercent", Kind::Bool, Dflt::Bool(false)),
    k("menubar.focus", Kind::Bool, Dflt::Bool(false)),
    k("menubar.clock.hour24", Kind::Bool, Dflt::Bool(true)),
    k("menubar.clock.seconds", Kind::Bool, Dflt::Bool(false)),
    k("menubar.clock.weekday", Kind::Bool, Dflt::Bool(true)),
    k(
        "menubar.clock.date",
        Kind::Enum(CLOCK_DATE),
        Dflt::Text("auto"),
    ),
    k("menubar.recentItems", Kind::Int(0, 50), Dflt::Int(10)),
    k(
        "menubar.autohide",
        Kind::Enum(AUTOHIDE),
        Dflt::Text("fullscreen"),
    ),
    k("menubar.background", Kind::Bool, Dflt::Bool(false)),
    k("wallpaper.image", Kind::Text, Dflt::Text("")),
    k("wallpaper.color", Kind::Text, Dflt::Text("")),
    k("wallpaper.id", Kind::Text, Dflt::Text("")),
    k("wallpaper.fit", Kind::Enum(FIT), Dflt::Text("fill")),
    k("lockscreen.showUser", Kind::Bool, Dflt::Bool(true)),
    k("lockscreen.showHints", Kind::Bool, Dflt::Bool(true)),
    k("lockscreen.passwordHint", Kind::Text, Dflt::Text("")),
    k("lockscreen.helpAddress", Kind::Text, Dflt::Text("")),
    k("lockscreen.showPowerButtons", Kind::Bool, Dflt::Bool(false)),
    k(
        "lockscreen.passwordDelay",
        Kind::Int(-1, 86_400),
        Dflt::Int(900),
    ),
    k("screensaver.minutes", Kind::Int(0, 1440), Dflt::Int(0)),
    k("screensaver.saver", Kind::Enum(SAVERS), Dflt::Text("drift")),
    k(
        "notifications.previews",
        Kind::Enum(PREVIEWS),
        Dflt::Text("always"),
    ),
    k(
        "notifications.grouping",
        Kind::Enum(GROUPING),
        Dflt::Text("auto"),
    ),
    k("controlcenter.modules", Kind::Texts, Dflt::Empty),
    k("screenshot.destination", Kind::Text, Dflt::Text("desktop")),
    k("screenshot.timer", Kind::Int(0, 60), Dflt::Int(0)),
    k("screenshot.thumbnail", Kind::Bool, Dflt::Bool(true)),
    k("screenshot.rememberSelection", Kind::Bool, Dflt::Bool(true)),
    k("screenshot.pointer", Kind::Bool, Dflt::Bool(false)),
    k(
        "screenshot.mode",
        Kind::Enum(SHOT_MODE),
        Dflt::Text("selection"),
    ),
    k("screenshot.selection", Kind::Nums(4), Dflt::Empty),
    k("screenshot.bar", Kind::Nums(2), Dflt::Empty),
    k("screenshot.recentFolders", Kind::Texts, Dflt::Empty),
];

/// Keys Ossido still reads from JSON (`key`, file, JSON key); the store keeps the files in
/// step until Ossido reads the service. A `passwordDelay` of -1 is `null` there.
const MIRRORED: &[(&str, &str, &str)] = &[
    ("dock.minimizeToApp", "dock.json", "minimizeToApp"),
    (
        "lockscreen.passwordDelay",
        "lockscreen.json",
        "passwordDelay",
    ),
    ("screensaver.minutes", "screensaver.json", "minutes"),
    ("screensaver.saver", "screensaver.json", "saver"),
];

const APP_PREFIX: &str = "notifications.apps.";
const APP_STYLE: &[&str] = &["banners", "alerts", "none"];
const APP_PREVIEWS: &[&str] = &["default", "always", "unlocked", "never"];
const APP_GROUPING: &[&str] = &["default", "auto", "app", "off"];

/// The definition of `name`: a static key, or a per-app notification key
/// (`notifications.apps.<app>.<style|previews|grouping|showInCenter|sound>`).
pub fn lookup(name: &str) -> Option<KeyDef> {
    if let Some(def) = KEYS.iter().find(|def| def.name == name) {
        return Some(*def);
    }
    let rest = name.strip_prefix(APP_PREFIX)?;
    let (app, field) = rest.rsplit_once('.')?;
    if app.is_empty() || app.len() > 200 || app.chars().any(char::is_control) {
        return None;
    }
    let (kind, default) = match field {
        "style" => (Kind::Enum(APP_STYLE), Dflt::Text("banners")),
        "previews" => (Kind::Enum(APP_PREVIEWS), Dflt::Text("default")),
        "grouping" => (Kind::Enum(APP_GROUPING), Dflt::Text("default")),
        "showInCenter" | "sound" => (Kind::Bool, Dflt::Bool(true)),
        _ => return None,
    };
    // The name is not 'static: dynamic keys are only checked and stored by name.
    Some(KeyDef {
        name: "notifications.apps.*",
        kind,
        default,
    })
}

/// Checks [`Setting`] against the kind; `None` when it does not fit.
fn accept(kind: Kind, value: Setting) -> Option<Setting> {
    let ok = match (&kind, &value) {
        (Kind::Bool, Setting::Bool(_)) => true,
        (Kind::Int(lo, hi), Setting::Int(i)) => (lo..=hi).contains(&i),
        (Kind::Num(lo, hi), Setting::Num(d)) => d.is_finite() && *d >= *lo && *d <= *hi,
        (Kind::Text, Setting::Text(s)) => s.len() <= MAX_TEXT,
        (Kind::Enum(set), Setting::Text(s)) => set.contains(&s.as_str()),
        (Kind::Texts, Setting::Texts(list)) => {
            list.len() <= MAX_LIST && list.iter().all(|s| s.len() <= MAX_TEXT)
        }
        (Kind::Nums(n), Setting::Nums(list)) => {
            (list.is_empty() || list.len() == *n) && list.iter().all(|d| d.is_finite())
        }
        _ => false,
    };
    ok.then_some(value)
}

/// A D-Bus variant as a [`Setting`]; integers of any width are accepted for integers and
/// numbers (clients infer signatures from JSON).
fn from_variant(value: &Value<'_>) -> Option<Setting> {
    Some(match value {
        Value::Value(inner) => return from_variant(inner),
        Value::Bool(b) => Setting::Bool(*b),
        Value::U8(i) => Setting::Int(i64::from(*i)),
        Value::I16(i) => Setting::Int(i64::from(*i)),
        Value::U16(i) => Setting::Int(i64::from(*i)),
        Value::I32(i) => Setting::Int(i64::from(*i)),
        Value::U32(i) => Setting::Int(i64::from(*i)),
        Value::I64(i) => Setting::Int(*i),
        Value::U64(i) => Setting::Int(i64::try_from(*i).ok()?),
        Value::F64(d) => Setting::Num(*d),
        Value::Str(s) => Setting::Text(s.as_str().to_owned()),
        Value::Array(array) => {
            let items: Vec<&Value<'_>> = array.iter().collect();
            if items.iter().all(|v| matches!(v, Value::Str(_))) {
                Setting::Texts(
                    items
                        .iter()
                        .filter_map(|v| match v {
                            Value::Str(s) => Some(s.as_str().to_owned()),
                            _ => None,
                        })
                        .collect(),
                )
            } else {
                let mut nums = Vec::with_capacity(items.len());
                for item in items {
                    nums.push(match from_variant(item)? {
                        Setting::Num(d) => d,
                        Setting::Int(i) => i as f64,
                        _ => return None,
                    });
                }
                Setting::Nums(nums)
            }
        }
        _ => return None,
    })
}

/// A value converted to what the key's kind wants (a whole number given to a number key).
fn coerce(kind: Kind, value: Setting) -> Setting {
    match (kind, value) {
        (Kind::Num(..), Setting::Int(i)) => Setting::Num(i as f64),
        (Kind::Int(..), Setting::Num(d)) if d.fract() == 0.0 && d.abs() < 9.0e15 => {
            Setting::Int(d as i64)
        }
        (Kind::Nums(_), Setting::Texts(list)) if list.is_empty() => Setting::Nums(Vec::new()),
        (_, other) => other,
    }
}

/// Why a write was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    UnknownKey(String),
    BadValue(String),
}

impl From<Refused> for fdo::Error {
    fn from(err: Refused) -> Self {
        match err {
            Refused::UnknownKey(key) => fdo::Error::InvalidArgs(format!("unknown key: {key}")),
            Refused::BadValue(key) => fdo::Error::InvalidArgs(format!(
                "value does not fit the schema of {key} (see crates/daemon/src/settings.rs)"
            )),
        }
    }
}

/// The values that differ from the default, and the file that keeps them.
#[derive(Debug, Default)]
pub struct SettingsStore {
    values: BTreeMap<String, Setting>,
    file: Option<PathBuf>,
}

impl SettingsStore {
    /// A store that lives only in memory.
    pub fn in_memory() -> Self {
        Self::default()
    }

    /// The store of `dir` (`<dir>/settings.bin`). Without the file, the old JSON files of
    /// the folder are imported and the binary file is written.
    pub fn open(dir: &Path) -> Self {
        let file = dir.join(FILE_NAME);
        let mut store = Self {
            values: BTreeMap::new(),
            file: Some(file.clone()),
        };
        match std::fs::read(&file) {
            Ok(bytes) => store.values = decode(&bytes),
            Err(_) => {
                for (key, value) in import_json(dir) {
                    if let Some(setting) = validate(&key, value) {
                        store.values.insert(key, setting);
                    }
                }
                if !store.values.is_empty() {
                    let _ = store.save();
                }
            }
        }
        store
    }

    /// The default store location: `$XDG_CONFIG_HOME/silica`.
    pub fn config_dir() -> Option<PathBuf> {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        let base = var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(base.join("silica"))
    }

    pub fn get(&self, key: &str) -> Result<Setting, Refused> {
        let def = lookup(key).ok_or_else(|| Refused::UnknownKey(key.to_owned()))?;
        Ok(self
            .values
            .get(key)
            .cloned()
            .unwrap_or_else(|| def.default_value()))
    }

    /// Every static key with its value, and the per-app keys that are set.
    pub fn get_all(&self) -> BTreeMap<String, Setting> {
        let mut all: BTreeMap<String, Setting> = KEYS
            .iter()
            .map(|def| (def.name.to_owned(), def.default_value()))
            .collect();
        all.extend(self.values.iter().map(|(k, v)| (k.clone(), v.clone())));
        all
    }

    /// Sets `key`; the keys whose value changed (empty when it was already that value).
    pub fn set(&mut self, key: &str, value: Setting) -> Result<Vec<String>, Refused> {
        self.set_many(vec![(key.to_owned(), value)])
    }

    /// All or none. Returns the keys whose value changed.
    pub fn set_many(&mut self, entries: Vec<(String, Setting)>) -> Result<Vec<String>, Refused> {
        let mut checked = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            let def = lookup(&key).ok_or_else(|| Refused::UnknownKey(key.clone()))?;
            let value = accept(def.kind, coerce(def.kind, value))
                .ok_or_else(|| Refused::BadValue(key.clone()))?;
            checked.push((key, def, value));
        }
        let mut changed = Vec::new();
        for (key, def, value) in checked {
            let before = self.get(&key)?;
            if before == value {
                continue;
            }
            if value == def.default_value() {
                self.values.remove(&key);
            } else {
                self.values.insert(key.clone(), value);
            }
            changed.push(key);
        }
        if !changed.is_empty() {
            let _ = self.save();
            for key in &changed {
                self.mirror(key);
            }
        }
        Ok(changed)
    }

    /// Back to the default; whether the value changed.
    pub fn reset(&mut self, key: &str) -> Result<bool, Refused> {
        lookup(key).ok_or_else(|| Refused::UnknownKey(key.to_owned()))?;
        let had = self.values.remove(key).is_some();
        if had {
            let _ = self.save();
            self.mirror(key);
        }
        Ok(had)
    }

    /// Keeps the JSON file Ossido reads in step with [`MIRRORED`] keys: the value when it
    /// differs from the default, otherwise no key (Ossido's default is the same).
    fn mirror(&self, key: &str) {
        let Some((_, file, name)) = MIRRORED.iter().find(|(k, ..)| *k == key) else {
            return;
        };
        let Some(dir) = self.file.as_deref().and_then(Path::parent) else {
            return;
        };
        let mut map = read_json(dir, file).unwrap_or_default();
        match self.values.get(key) {
            Some(Setting::Bool(b)) => {
                map.insert((*name).to_owned(), Json::Bool(*b));
            }
            Some(Setting::Int(-1)) => {
                map.insert((*name).to_owned(), Json::Null);
            }
            Some(Setting::Int(i)) => {
                map.insert((*name).to_owned(), Json::from(*i));
            }
            Some(Setting::Text(s)) => {
                map.insert((*name).to_owned(), Json::from(s.as_str()));
            }
            _ => {
                map.remove(*name);
            }
        }
        let path = dir.join(file);
        let temporary = path.with_extension("json.tmp");
        let text = Json::Object(map).to_string();
        if std::fs::write(&temporary, text).is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write and rename: a reader never sees half a file.
        let temporary = file.with_extension("bin.tmp");
        std::fs::write(&temporary, encode(&self.values))?;
        std::fs::rename(&temporary, file)
    }
}

/// `value` as the key's kind wants it, or `None`.
fn validate(key: &str, value: Setting) -> Option<Setting> {
    let def = lookup(key)?;
    accept(def.kind, coerce(def.kind, value))
}

// ---- binary format ----

fn put_str(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&u32::try_from(text.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
}

/// The file's bytes for `values`.
pub fn encode(values: &BTreeMap<String, Setting>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&u32::try_from(values.len()).unwrap_or(0).to_le_bytes());
    for (key, value) in values {
        out.extend_from_slice(&u16::try_from(key.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        match value {
            Setting::Bool(b) => {
                out.push(0);
                out.push(u8::from(*b));
            }
            Setting::Int(i) => {
                out.push(1);
                out.extend_from_slice(&i.to_le_bytes());
            }
            Setting::Num(d) => {
                out.push(2);
                out.extend_from_slice(&d.to_le_bytes());
            }
            Setting::Text(s) => {
                out.push(3);
                put_str(&mut out, s);
            }
            Setting::Texts(list) => {
                out.push(4);
                out.extend_from_slice(&u32::try_from(list.len()).unwrap_or(0).to_le_bytes());
                for s in list {
                    put_str(&mut out, s);
                }
            }
            Setting::Nums(list) => {
                out.push(5);
                out.extend_from_slice(&u32::try_from(list.len()).unwrap_or(0).to_le_bytes());
                for d in list {
                    out.extend_from_slice(&d.to_le_bytes());
                }
            }
        }
    }
    out
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(n)?;
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn text(&mut self) -> Option<String> {
        let n = usize::try_from(self.u32()?).ok()?;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    fn count(&mut self) -> Option<usize> {
        let n = usize::try_from(self.u32()?).ok()?;
        // No list is longer than what is left (each item takes at least 4 bytes).
        (n <= self.0.len()).then_some(n)
    }
}

/// The values in the file's bytes. A damaged file gives what was read before the damage; keys
/// the schema does not know (a newer version) or values that do not fit are dropped.
pub fn decode(bytes: &[u8]) -> BTreeMap<String, Setting> {
    let mut values = BTreeMap::new();
    let mut r = Reader(bytes);
    if r.take(4) != Some(MAGIC.as_slice()) || r.u8() != Some(VERSION) {
        return values;
    }
    let Some(count) = r.u32() else {
        return values;
    };
    for _ in 0..count {
        let Some(entry) = read_entry(&mut r) else {
            break;
        };
        let (key, value) = entry;
        if let Some(value) = validate(&key, value) {
            values.insert(key, value);
        }
    }
    values
}

fn read_entry(r: &mut Reader<'_>) -> Option<(String, Setting)> {
    let n = usize::from(r.u16()?);
    let key = String::from_utf8(r.take(n)?.to_vec()).ok()?;
    let value = match r.u8()? {
        0 => Setting::Bool(r.u8()? != 0),
        1 => Setting::Int(r.i64()?),
        2 => Setting::Num(r.f64()?),
        3 => Setting::Text(r.text()?),
        4 => {
            let n = r.count()?;
            let mut list = Vec::with_capacity(n);
            for _ in 0..n {
                list.push(r.text()?);
            }
            Setting::Texts(list)
        }
        5 => {
            let n = r.count()?;
            let mut list = Vec::with_capacity(n);
            for _ in 0..n {
                list.push(r.f64()?);
            }
            Setting::Nums(list)
        }
        _ => return None,
    };
    Some((key, value))
}

// ---- the old JSON files ----

fn read_json(dir: &Path, name: &str) -> Option<serde_json::Map<String, Json>> {
    let text = std::fs::read_to_string(dir.join(name)).ok()?;
    match serde_json::from_str::<Json>(&text).ok()? {
        Json::Object(map) => Some(map),
        _ => None,
    }
}

fn setting_of(json: &Json) -> Option<Setting> {
    Some(match json {
        Json::Bool(b) => Setting::Bool(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Setting::Int(i),
            None => Setting::Num(n.as_f64()?),
        },
        Json::String(s) => Setting::Text(s.clone()),
        _ => return None,
    })
}

/// The settings in the JSON files of `dir` (`dock.json`, `menubar.json`, ...), as
/// `(key, value)`; values are not validated here. Used once, to fill a new store.
pub fn import_json(dir: &Path) -> Vec<(String, Setting)> {
    let mut out = Vec::new();
    fn plain(
        out: &mut Vec<(String, Setting)>,
        dir: &Path,
        file: &str,
        prefix: &str,
        names: &[&str],
    ) {
        let Some(map) = read_json(dir, file) else {
            return;
        };
        for name in names {
            if let Some(value) = map.get(*name).and_then(setting_of) {
                out.push((format!("{prefix}.{name}"), value));
            }
        }
    }
    plain(
        &mut out,
        dir,
        "dock.json",
        "dock",
        &[
            "magnify",
            "recents",
            "autohide",
            "position",
            "minimizeEffect",
            "indicators",
            "tileSize",
            "magnifiedSize",
            "launchAnimation",
            "minimizeToApp",
        ],
    );
    plain(
        &mut out,
        dir,
        "menubar.json",
        "menubar",
        &[
            "sound",
            "bluetooth",
            "battery",
            "batteryPercent",
            "focus",
            "recentItems",
            "autohide",
            "background",
        ],
    );
    if let Some(Json::Object(clock)) =
        read_json(dir, "menubar.json").and_then(|m| m.get("clock").cloned())
    {
        for name in ["hour24", "seconds", "weekday", "date"] {
            if let Some(value) = clock.get(name).and_then(setting_of) {
                out.push((format!("menubar.clock.{name}"), value));
            }
        }
    }
    if let Some(map) = read_json(dir, "wallpaper.json") {
        for (json, key) in [
            ("image", "wallpaper.image"),
            ("color", "wallpaper.color"),
            ("wallpaper", "wallpaper.id"),
            ("fit", "wallpaper.fit"),
        ] {
            if let Some(value) = map.get(json).and_then(setting_of) {
                out.push((key.to_owned(), value));
            }
        }
    }
    if let Some(map) = read_json(dir, "lockscreen.json") {
        if let Some(value) = map.get("showUser").and_then(setting_of) {
            out.push(("lockscreen.showUser".to_owned(), value));
        }
        match map.get("passwordDelay") {
            Some(Json::Null) => out.push(("lockscreen.passwordDelay".to_owned(), Setting::Int(-1))),
            Some(json) => {
                if let Some(value) = setting_of(json) {
                    out.push(("lockscreen.passwordDelay".to_owned(), value));
                }
            }
            None => {}
        }
    }
    plain(
        &mut out,
        dir,
        "screensaver.json",
        "screensaver",
        &["minutes", "saver"],
    );
    if let Some(map) = read_json(dir, "notifications.json") {
        // `automatic` and `byApp` are the older spellings of the grouping.
        fn grouping(json: &Json) -> Option<&str> {
            json.as_str().map(|s| match s {
                "automatic" => "auto",
                "byApp" => "app",
                other => other,
            })
        }
        if let Some(value) = map.get("previews").and_then(setting_of) {
            out.push(("notifications.previews".to_owned(), value));
        }
        if let Some(name) = map.get("grouping").and_then(grouping) {
            out.push((
                "notifications.grouping".to_owned(),
                Setting::Text(name.to_owned()),
            ));
        }
        if let Some(Json::Object(apps)) = map.get("apps") {
            for (app, entry) in apps {
                let Json::Object(entry) = entry else {
                    continue;
                };
                for field in ["style", "previews", "grouping", "showInCenter", "sound"] {
                    let value = match (field, entry.get(field)) {
                        ("grouping", Some(json)) => {
                            grouping(json).map(|s| Setting::Text(s.to_owned()))
                        }
                        (_, Some(json)) => setting_of(json),
                        _ => None,
                    };
                    if let Some(value) = value {
                        out.push((format!("{APP_PREFIX}{app}.{field}"), value));
                    }
                }
            }
        }
    }
    if let Some(Json::Array(modules)) =
        read_json(dir, "controlcenter.json").and_then(|m| m.get("modules").cloned())
    {
        let list: Vec<String> = modules
            .iter()
            .filter_map(|m| {
                let id = m.get("id")?.as_str()?;
                let size = m.get("size")?.as_str()?;
                Some(format!("{id}:{size}"))
            })
            .collect();
        out.push(("controlcenter.modules".to_owned(), Setting::Texts(list)));
    }
    if let Some(map) = read_json(dir, "screenshot.json") {
        for (json, key) in [
            ("destination", "screenshot.destination"),
            ("timer", "screenshot.timer"),
            ("thumbnail", "screenshot.thumbnail"),
            ("remember_selection", "screenshot.rememberSelection"),
            ("pointer", "screenshot.pointer"),
            ("mode", "screenshot.mode"),
        ] {
            if let Some(value) = map.get(json).and_then(setting_of) {
                out.push((key.to_owned(), value));
            }
        }
        for (json, key) in [
            ("selection", "screenshot.selection"),
            ("bar", "screenshot.bar"),
        ] {
            if let Some(Json::Array(list)) = map.get(json) {
                let nums: Vec<f64> = list.iter().filter_map(Json::as_f64).collect();
                if nums.len() == list.len() {
                    out.push((key.to_owned(), Setting::Nums(nums)));
                }
            }
        }
        if let Some(Json::Array(list)) = map.get("recent_folders") {
            let folders: Vec<String> = list
                .iter()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect();
            out.push((
                "screenshot.recentFolders".to_owned(),
                Setting::Texts(folders),
            ));
        }
    }
    out
}

// ---- D-Bus ----

/// `dev.frascella.Silica.Settings1`.
pub struct Settings {
    store: SettingsStore,
}

impl Settings {
    pub fn new(store: SettingsStore) -> Self {
        Self { store }
    }

    /// The service on the default store (`$XDG_CONFIG_HOME/silica`).
    pub fn from_env() -> Self {
        Self::new(
            SettingsStore::config_dir()
                .map_or_else(SettingsStore::in_memory, |dir| SettingsStore::open(&dir)),
        )
    }
}

#[interface(name = "dev.frascella.Silica.Settings1")]
impl Settings {
    /// The value of `key` (its default if never set). `InvalidArgs` for an unknown key.
    fn get(&self, key: &str) -> fdo::Result<Value<'static>> {
        Ok(self.store.get(key)?.to_variant())
    }

    /// Every static key with its value, and the per-app keys that are set.
    fn get_all(&self) -> HashMap<String, Value<'static>> {
        self.store
            .get_all()
            .into_iter()
            .map(|(key, value)| (key, value.to_variant()))
            .collect()
    }

    /// Sets `key`. `InvalidArgs` for an unknown key or a value outside the schema.
    async fn set(
        &mut self,
        key: &str,
        value: Value<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let setting = from_variant(&value).ok_or_else(|| Refused::BadValue(key.to_owned()))?;
        let changed = self.store.set(key, setting)?;
        self.announce(&emitter, changed).await
    }

    /// Sets several keys at once: all or none, then one `Changed` per key that changed.
    async fn set_many(
        &mut self,
        values: HashMap<String, Value<'_>>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let mut entries = Vec::with_capacity(values.len());
        for (key, value) in values {
            let setting = from_variant(&value).ok_or_else(|| Refused::BadValue(key.clone()))?;
            entries.push((key, setting));
        }
        let changed = self.store.set_many(entries)?;
        self.announce(&emitter, changed).await
    }

    /// Back to the default of `key`.
    async fn reset(
        &mut self,
        key: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let changed = self.store.reset(key)?;
        self.announce(
            &emitter,
            if changed {
                vec![key.to_owned()]
            } else {
                Vec::new()
            },
        )
        .await
    }

    /// A key took a new value (also after `Reset`: the default).
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, key: &str, value: Value<'_>) -> zbus::Result<()>;
}

impl Settings {
    async fn announce(&self, emitter: &SignalEmitter<'_>, keys: Vec<String>) -> fdo::Result<()> {
        for key in keys {
            let value = self.store.get(&key)?.to_variant();
            Self::changed(emitter, &key, value).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("silica-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn schema_defaults_fit_their_kind() {
        for def in KEYS {
            let value = def.default_value();
            assert_eq!(accept(def.kind, value.clone()), Some(value), "{}", def.name);
        }
        let mut names: Vec<_> = KEYS.iter().map(|d| d.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), KEYS.len(), "duplicate key");
    }

    #[test]
    fn get_set_reset() {
        let mut s = SettingsStore::in_memory();
        assert_eq!(s.get("dock.tileSize"), Ok(Setting::Int(60)));
        assert_eq!(
            s.set("dock.tileSize", Setting::Int(90)),
            Ok(vec!["dock.tileSize".into()])
        );
        assert_eq!(s.get("dock.tileSize"), Ok(Setting::Int(90)));
        // The same value again changes nothing.
        assert_eq!(s.set("dock.tileSize", Setting::Int(90)), Ok(vec![]));
        assert_eq!(s.reset("dock.tileSize"), Ok(true));
        assert_eq!(s.reset("dock.tileSize"), Ok(false));
        assert_eq!(s.get("dock.tileSize"), Ok(Setting::Int(60)));
        // Setting the default is the same as the reset: nothing is kept.
        s.set("dock.tileSize", Setting::Int(90)).unwrap();
        s.set("dock.tileSize", Setting::Int(60)).unwrap();
        assert!(s.values.is_empty());
    }

    #[test]
    fn refuses_what_the_schema_does_not_allow() {
        let mut s = SettingsStore::in_memory();
        let bad = |r: Result<Vec<String>, Refused>| matches!(r, Err(Refused::BadValue(_)));
        assert!(bad(s.set("dock.tileSize", Setting::Int(500))));
        assert!(bad(s.set("dock.tileSize", Setting::Text("big".into()))));
        assert!(bad(s.set("dock.position", Setting::Text("top".into()))));
        assert!(bad(s.set("dock.autohide", Setting::Int(1))));
        assert!(bad(
            s.set("screenshot.selection", Setting::Nums(vec![1.0, 2.0]))
        ));
        assert_eq!(
            s.set("nope", Setting::Bool(true)),
            Err(Refused::UnknownKey("nope".into()))
        );
        assert!(
            s.set("notifications.apps.org.x.Y.nothing", Setting::Bool(true))
                .is_err()
        );
        assert_eq!(s.get("nope"), Err(Refused::UnknownKey("nope".into())));
        // A whole number is fine for a number, and a list of numbers for a rectangle.
        assert!(
            s.set(
                "screenshot.selection",
                Setting::Nums(vec![1.0, 2.0, 3.0, 4.0])
            )
            .is_ok()
        );
        assert!(s.set("screenshot.selection", Setting::Nums(vec![])).is_ok());
    }

    #[test]
    fn set_many_is_all_or_none() {
        let mut s = SettingsStore::in_memory();
        let r = s.set_many(vec![
            ("dock.autohide".into(), Setting::Bool(true)),
            ("dock.position".into(), Setting::Text("top".into())),
        ]);
        assert!(r.is_err());
        assert_eq!(s.get("dock.autohide"), Ok(Setting::Bool(false)));
        let r = s
            .set_many(vec![
                ("dock.autohide".into(), Setting::Bool(true)),
                ("dock.position".into(), Setting::Text("left".into())),
            ])
            .unwrap();
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn per_app_notification_keys() {
        let mut s = SettingsStore::in_memory();
        let key = "notifications.apps.org.example.Mail.showInCenter";
        assert_eq!(s.get(key), Ok(Setting::Bool(true)));
        s.set(key, Setting::Bool(false)).unwrap();
        assert_eq!(s.get_all().get(key), Some(&Setting::Bool(false)));
        assert!(
            s.set(
                "notifications.apps.org.example.Mail.style",
                Setting::Text("popup".into())
            )
            .is_err()
        );
        assert!(lookup("notifications.apps..sound").is_none());
    }

    #[test]
    fn binary_round_trip_and_damage() {
        let mut values = BTreeMap::new();
        values.insert("dock.autohide".to_owned(), Setting::Bool(true));
        values.insert("dock.tileSize".to_owned(), Setting::Int(100));
        values.insert(
            "wallpaper.color".to_owned(),
            Setting::Text("#102030".into()),
        );
        values.insert(
            "controlcenter.modules".to_owned(),
            Setting::Texts(vec!["wifi:large".into(), "dnd:small".into()]),
        );
        values.insert("screenshot.bar".to_owned(), Setting::Nums(vec![10.5, 20.0]));
        let bytes = encode(&values);
        assert_eq!(decode(&bytes), values);
        // Truncated at every length, or with junk: never a panic, only a prefix of the values.
        for n in 0..bytes.len() {
            let _ = decode(bytes.get(..n).unwrap_or_default());
        }
        assert!(decode(b"garbage").is_empty());
        // An entry the schema does not know (or that does not fit) is dropped.
        let mut other = values.clone();
        other.insert("future.key".to_owned(), Setting::Bool(true));
        other.insert("dock.tileSize".to_owned(), Setting::Int(9999));
        let decoded = decode(&encode(&other));
        assert!(!decoded.contains_key("future.key") && !decoded.contains_key("dock.tileSize"));
        assert_eq!(decoded.get("dock.autohide"), Some(&Setting::Bool(true)));
    }

    #[test]
    fn persists_between_runs() {
        let d = dir("persist");
        let mut s = SettingsStore::open(&d);
        s.set("dock.autohide", Setting::Bool(true)).unwrap();
        s.set("menubar.clock.date", Setting::Text("never".into()))
            .unwrap();
        drop(s);
        assert!(d.join("settings.bin").exists());
        let s = SettingsStore::open(&d);
        assert_eq!(s.get("dock.autohide"), Ok(Setting::Bool(true)));
        assert_eq!(
            s.get("menubar.clock.date"),
            Ok(Setting::Text("never".into()))
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn imports_the_json_files_once() {
        let d = dir("import");
        let write = |name: &str, text: &str| std::fs::write(d.join(name), text).unwrap();
        write(
            "dock.json",
            r#"{"tileSize": 122, "autohide": true, "position": "left", "pinned": [], "launchAnimation": "none"}"#,
        );
        write(
            "menubar.json",
            r#"{"sound": "never", "clock": {"hour24": false, "date": "always"}, "recentItems": 7}"#,
        );
        write("wallpaper.json", r##"{"color": "#102030", "fit": "tile"}"##);
        write(
            "lockscreen.json",
            r#"{"passwordDelay": null, "showUser": false}"#,
        );
        write("screensaver.json", r#"{"minutes": 10, "saver": "clock"}"#);
        write(
            "notifications.json",
            r#"{"previews": "never", "grouping": "byApp", "apps": {"org.x.Mail": {"showInCenter": false, "previews": "always", "grouping": "off"}}}"#,
        );
        write(
            "controlcenter.json",
            r#"{"modules": [{"id": "wifi", "size": "large"}]}"#,
        );
        write(
            "screenshot.json",
            r#"{"timer": 5, "pointer": true, "selection": [1, 2, 3, 4], "recent_folders": ["/tmp"], "destination": "folder:/tmp"}"#,
        );
        let s = SettingsStore::open(&d);
        let text = |k: &str| s.get(k).unwrap();
        assert_eq!(text("dock.tileSize"), Setting::Int(122));
        assert_eq!(text("dock.autohide"), Setting::Bool(true));
        assert_eq!(text("dock.position"), Setting::Text("left".into()));
        assert_eq!(text("dock.launchAnimation"), Setting::Text("none".into()));
        assert_eq!(text("menubar.sound"), Setting::Text("never".into()));
        assert_eq!(text("menubar.clock.hour24"), Setting::Bool(false));
        assert_eq!(text("menubar.recentItems"), Setting::Int(7));
        assert_eq!(text("wallpaper.color"), Setting::Text("#102030".into()));
        assert_eq!(text("wallpaper.fit"), Setting::Text("tile".into()));
        assert_eq!(text("lockscreen.passwordDelay"), Setting::Int(-1));
        assert_eq!(text("lockscreen.showUser"), Setting::Bool(false));
        assert_eq!(text("screensaver.minutes"), Setting::Int(10));
        assert_eq!(text("notifications.grouping"), Setting::Text("app".into()));
        assert_eq!(
            text("notifications.apps.org.x.Mail.showInCenter"),
            Setting::Bool(false)
        );
        assert_eq!(
            text("notifications.apps.org.x.Mail.grouping"),
            Setting::Text("off".into())
        );
        assert_eq!(
            text("controlcenter.modules"),
            Setting::Texts(vec!["wifi:large".into()])
        );
        assert_eq!(
            text("screenshot.selection"),
            Setting::Nums(vec![1.0, 2.0, 3.0, 4.0])
        );
        assert_eq!(
            text("screenshot.destination"),
            Setting::Text("folder:/tmp".into())
        );
        // Once: later edits of the old files are not read again.
        write("dock.json", r#"{"autohide": false}"#);
        drop(s);
        let s = SettingsStore::open(&d);
        assert_eq!(s.get("dock.autohide"), Ok(Setting::Bool(true)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn mirrors_the_keys_ossido_reads() {
        let d = dir("mirror");
        std::fs::write(d.join("lockscreen.json"), r#"{"showUser": false}"#).unwrap();
        let mut s = SettingsStore::open(&d);
        let json = |name: &str| {
            serde_json::from_str::<Json>(&std::fs::read_to_string(d.join(name)).unwrap()).unwrap()
        };
        s.set("lockscreen.passwordDelay", Setting::Int(-1)).unwrap();
        assert_eq!(json("lockscreen.json")["passwordDelay"], Json::Null);
        // The other keys of the file stay.
        assert_eq!(json("lockscreen.json")["showUser"], Json::Bool(false));
        s.set("lockscreen.passwordDelay", Setting::Int(60)).unwrap();
        assert_eq!(json("lockscreen.json")["passwordDelay"], Json::from(60));
        s.reset("lockscreen.passwordDelay").unwrap();
        assert!(json("lockscreen.json").get("passwordDelay").is_none());
        s.set("dock.minimizeToApp", Setting::Bool(true)).unwrap();
        assert_eq!(json("dock.json")["minimizeToApp"], Json::Bool(true));
        s.set("screensaver.saver", Setting::Text("clock".into()))
            .unwrap();
        assert_eq!(json("screensaver.json")["saver"], Json::from("clock"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn variants_convert_both_ways() {
        let v = Setting::Texts(vec!["a".into(), "b".into()]).to_variant();
        assert_eq!(
            from_variant(&v),
            Some(Setting::Texts(vec!["a".into(), "b".into()]))
        );
        let v = Setting::Nums(vec![1.5, 2.0]).to_variant();
        assert_eq!(from_variant(&v), Some(Setting::Nums(vec![1.5, 2.0])));
        assert_eq!(from_variant(&Value::I32(5)), Some(Setting::Int(5)));
        assert_eq!(from_variant(&Value::U64(u64::MAX)), None);
        assert_eq!(from_variant(&Value::Bool(true)), Some(Setting::Bool(true)));
    }
}
