//! The user's settings files that Ossido applies: `$XDG_CONFIG_HOME/silica/*.json` (usually
//! `~/.config/silica`), written by Settings (`docs/control.md` › Settings files).
//!
//! The folder is watched with inotify: a file written in place or renamed over (Settings writes
//! a temporary file and renames it) is read again and applied at once. A missing, unreadable or
//! malformed file means the defaults; a value out of range is ignored, the others still apply.

use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use rustix::fs::inotify;
use serde_json::Value;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use tracing::{info, warn};

use smithay::input::keyboard::XkbConfig;

use crate::state::{Ossido, State};

/// The settings folder: `$XDG_CONFIG_HOME/silica`, or `~/.config/silica`.
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("silica"))
}

/// The JSON object in `dir/name`; `None` if missing or not an object.
pub fn read_json(dir: &Path, name: &str) -> Option<serde_json::Map<String, Value>> {
    let text = std::fs::read_to_string(dir.join(name)).ok()?;
    match serde_json::from_str(&text) {
        Ok(Value::Object(map)) => Some(map),
        Ok(_) => None,
        Err(err) => {
            warn!("{name}: {err}");
            None
        }
    }
}

/// An integer in `range`, or `None`.
pub fn int_in(map: &serde_json::Map<String, Value>, key: &str, range: (i64, i64)) -> Option<i64> {
    let value = map.get(key)?;
    let n = value.as_i64().or_else(|| {
        value
            .as_f64()
            .filter(|f| f.is_finite())
            .map(|f| f.round() as i64)
    })?;
    (range.0..=range.1).contains(&n).then_some(n)
}

/// A number in `range`, or `None`.
pub fn number_in(
    map: &serde_json::Map<String, Value>,
    key: &str,
    range: (f64, f64),
) -> Option<f64> {
    let n = map.get(key)?.as_f64().filter(|f| f.is_finite())?;
    (range.0..=range.1).contains(&n).then_some(n)
}

pub fn bool_of(map: &serde_json::Map<String, Value>, key: &str) -> Option<bool> {
    map.get(key)?.as_bool()
}

/// A short string (an xkb name, an option list): no control characters, at most 256 bytes.
pub fn text_of(map: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    let text = map.get(key)?.as_str()?;
    (text.len() <= 256 && !text.chars().any(char::is_control)).then(|| text.to_owned())
}

// --- keyboard.json -------------------------------------------------------------------------

pub const KEYBOARD_FILE: &str = "keyboard.json";

/// One input source: an xkb layout with its variant (`us`, `it`, `de(nodeadkeys)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub layout: String,
    pub variant: String,
}

impl Layout {
    /// `layout` or `layout(variant)`: letters, digits, `_` and `-` only.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (layout, variant) = match text.split_once('(') {
            Some((layout, rest)) => (layout, rest.strip_suffix(')')?),
            None => (text, ""),
        };
        let name = |s: &str| {
            s.len() <= 64
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        };
        (!layout.is_empty() && name(layout) && name(variant)).then(|| Layout {
            layout: layout.to_owned(),
            variant: variant.to_owned(),
        })
    }

    pub fn name(&self) -> String {
        if self.variant.is_empty() {
            self.layout.clone()
        } else {
            format!("{}({})", self.layout, self.variant)
        }
    }
}

/// `keyboard.json`: key repeat and input sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardConfig {
    /// Before the first repeat, in ms.
    pub repeat_delay: i32,
    /// Repeats per second; 0: no repeat.
    pub repeat_rate: i32,
    /// The input sources, in order; empty: xkb's default (or `XKB_DEFAULT_LAYOUT`).
    pub layouts: Vec<Layout>,
    pub model: String,
    pub options: String,
}

/// At most this many input sources (xkb keeps 4 groups per keymap).
pub const MAX_LAYOUTS: usize = 4;

impl KeyboardConfig {
    /// The defaults are `defaults` (key repeat from `OSSIDO_KEY_REPEAT` or the built-in
    /// values); the file's valid keys win.
    pub fn parse(map: Option<&serde_json::Map<String, Value>>, defaults: (i32, i32)) -> Self {
        let mut config = KeyboardConfig {
            repeat_delay: defaults.0,
            repeat_rate: defaults.1,
            layouts: Vec::new(),
            model: String::new(),
            options: String::new(),
        };
        let Some(map) = map else {
            return config;
        };
        if let Some(delay) = int_in(map, "repeatDelay", (100, 3000)) {
            config.repeat_delay = delay as i32;
        }
        if let Some(rate) = int_in(map, "repeatRate", (0, 100)) {
            config.repeat_rate = rate as i32;
        }
        if let Some(Value::Array(list)) = map.get("layouts") {
            config.layouts = list
                .iter()
                .filter_map(Value::as_str)
                .filter_map(Layout::parse)
                .take(MAX_LAYOUTS)
                .collect();
        }
        config.model = text_of(map, "model").unwrap_or_default();
        config.options = text_of(map, "options").unwrap_or_default();
        config
    }

    /// The xkb names: comma-separated layouts and variants.
    pub fn xkb_names(&self) -> (String, String) {
        let layouts: Vec<&str> = self.layouts.iter().map(|l| l.layout.as_str()).collect();
        let variants: Vec<&str> = self.layouts.iter().map(|l| l.variant.as_str()).collect();
        (layouts.join(","), variants.join(","))
    }
}

// --- input.json ----------------------------------------------------------------------------

pub const INPUT_FILE: &str = "input.json";

/// How a touchpad makes a secondary click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMethod {
    /// Two fingers anywhere (libinput's clickfinger).
    TwoFinger,
    /// The bottom right corner (libinput's button areas).
    BottomRight,
    /// The bottom left corner (button areas, left-handed).
    BottomLeft,
}

/// The settings of one kind of pointer device; `None` keeps libinput's default for the device.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerSettings {
    /// Pointer acceleration speed, -1–1.
    pub speed: Option<f64>,
    pub natural_scroll: Option<bool>,
    /// Multiplies scroll deltas, 0.1–10 (1: unchanged).
    pub scroll_speed: f64,
    /// Primary and secondary buttons swapped.
    pub left_handed: Option<bool>,
    /// Touchpads only.
    pub click_method: Option<ClickMethod>,
    pub tap_to_click: Option<bool>,
    pub disable_while_typing: Option<bool>,
}

impl Default for PointerSettings {
    fn default() -> Self {
        Self {
            speed: None,
            natural_scroll: None,
            scroll_speed: 1.0,
            left_handed: None,
            click_method: None,
            tap_to_click: None,
            disable_while_typing: None,
        }
    }
}

/// `input.json`: mice and touchpads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputConfig {
    pub mouse: PointerSettings,
    pub touchpad: PointerSettings,
}

impl InputConfig {
    pub fn parse(map: Option<&serde_json::Map<String, Value>>) -> Self {
        let mut config = InputConfig::default();
        let Some(map) = map else {
            return config;
        };
        let common = |group: &serde_json::Map<String, Value>| PointerSettings {
            speed: number_in(group, "speed", (-1.0, 1.0)),
            natural_scroll: bool_of(group, "naturalScroll"),
            scroll_speed: number_in(group, "scrollSpeed", (0.1, 10.0)).unwrap_or(1.0),
            ..PointerSettings::default()
        };
        if let Some(Value::Object(mouse)) = map.get("mouse") {
            config.mouse = PointerSettings {
                left_handed: match mouse.get("secondaryClick").and_then(Value::as_str) {
                    Some("right") => Some(false),
                    Some("left") => Some(true),
                    _ => None,
                },
                ..common(mouse)
            };
        }
        if let Some(Value::Object(touchpad)) = map.get("touchpad") {
            let click = match touchpad.get("secondaryClick").and_then(Value::as_str) {
                Some("twoFinger") => Some(ClickMethod::TwoFinger),
                Some("bottomRight") => Some(ClickMethod::BottomRight),
                Some("bottomLeft") => Some(ClickMethod::BottomLeft),
                _ => None,
            };
            config.touchpad = PointerSettings {
                click_method: click,
                left_handed: click.map(|c| c == ClickMethod::BottomLeft),
                tap_to_click: bool_of(touchpad, "tapToClick"),
                disable_while_typing: bool_of(touchpad, "disableWhileTyping"),
                ..common(touchpad)
            };
        }
        config
    }
}

// --- theme.json ----------------------------------------------------------------------------

pub const THEME_FILE: &str = "theme.json";

/// `theme.json` (Settings › Appearance): the part Ossido applies, the glass level. The rest of
/// the file is the look the apps follow (silica-daemon `SetLook`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeConfig {
    /// 0 clear to 1 tinted; 0.5, the measured material, by default.
    pub glass: f32,
    /// Accessibility › Display › Reduce Transparency: the glass is drawn opaque.
    pub reduce_transparency: bool,
    /// Accessibility › Display › Reduce Motion: Ossido's own animations are skipped.
    pub reduce_motion: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            glass: ossido_render::glass_profile::GLASS_LEVEL_DEFAULT,
            reduce_transparency: false,
            reduce_motion: false,
        }
    }
}

impl ThemeConfig {
    pub fn parse(map: Option<&serde_json::Map<String, Value>>) -> Self {
        let glass = map
            .and_then(|map| number_in(map, "glass", (0.0, 1.0)))
            .map_or(Self::default().glass, |g| g as f32);
        Self {
            glass,
            reduce_transparency: map
                .and_then(|map| bool_of(map, "reduceTransparency"))
                .unwrap_or(false),
            reduce_motion: map
                .and_then(|map| bool_of(map, "reduceMotion"))
                .unwrap_or(false),
        }
    }
}

// --- power.json ----------------------------------------------------------------------------

pub const POWER_FILE: &str = "power.json";

/// `power.json`: idle times, in minutes (decimals allowed); absent or 0: never.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PowerConfig {
    pub display_off_minutes: Option<f64>,
    pub sleep_minutes: Option<f64>,
    /// Never sleep on idle (silica-daemon also holds logind's idle inhibitor).
    pub prevent_sleep: bool,
}

impl PowerConfig {
    pub fn parse(map: Option<&serde_json::Map<String, Value>>) -> Self {
        let Some(map) = map else {
            return Self::default();
        };
        let minutes = |key| number_in(map, key, (0.0, 100_000.0)).filter(|m| *m > 0.0);
        Self {
            display_off_minutes: minutes("displayOffMinutes"),
            sleep_minutes: minutes("sleepMinutes"),
            prevent_sleep: bool_of(map, "preventSleep").unwrap_or(false),
        }
    }
}

impl State {
    /// A file of the settings folder changed.
    pub fn config_changed(&mut self, name: &str) {
        match name {
            KEYBOARD_FILE => self.apply_keyboard_config(),
            INPUT_FILE => self.apply_input_config(),
            POWER_FILE => self.apply_power_config(),
            THEME_FILE => self.apply_theme_config(),
            crate::displays::DISPLAYS_FILE => self.apply_displays_config(),
            _ => {}
        }
    }

    /// `input.json`: to every mouse and touchpad now, and to each one that appears later.
    pub fn apply_input_config(&mut self) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let config = InputConfig::parse(read_json(&dir, INPUT_FILE).as_ref());
        if config != self.input_config {
            self.input_config = config;
            #[cfg(feature = "drm")]
            self.configure_input_devices();
        }
    }

    /// `theme.json`: the glass level, drawn from the next frame.
    pub fn apply_theme_config(&mut self) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let config = ThemeConfig::parse(read_json(&dir, THEME_FILE).as_ref());
        if config != self.theme_config {
            let was = self.theme_config.reduce_motion;
            self.theme_config = config;
            // Reduce Motion: no animation of Ossido's own, and back to the chosen speed after.
            if config.reduce_motion && !was {
                self.saved_animation_speed = self.animation_speed;
                self.animation_speed = 0.0;
            } else if !config.reduce_motion && was {
                self.animation_speed = self.saved_animation_speed;
            }
            self.wake();
        }
    }

    /// Reads and applies every settings file (at start).
    pub fn apply_all_config(&mut self) {
        self.apply_keyboard_config();
        self.apply_input_config();
        self.apply_power_config();
        self.apply_theme_config();
        self.apply_displays_config();
    }

    /// `keyboard.json`: the repeat goes to every bound keyboard (`wl_keyboard.repeat_info`),
    /// a new keymap to the clients when the sources change.
    pub fn apply_keyboard_config(&mut self) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let map = read_json(&dir, KEYBOARD_FILE);
        let config = KeyboardConfig::parse(map.as_ref(), crate::state::key_repeat());
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let old = std::mem::replace(&mut self.keyboard_config, config.clone());
        if (old.repeat_delay, old.repeat_rate) != (config.repeat_delay, config.repeat_rate) {
            keyboard.change_repeat_info(config.repeat_rate, config.repeat_delay);
            info!(
                "key repeat: {} ms, {}/s",
                config.repeat_delay, config.repeat_rate
            );
        }
        if (&old.layouts, &old.model, &old.options)
            != (&config.layouts, &config.model, &config.options)
        {
            let (layout, variant) = config.xkb_names();
            let xkb = XkbConfig {
                rules: "",
                model: &config.model,
                layout: &layout,
                variant: &variant,
                options: (!config.options.is_empty()).then(|| config.options.clone()),
            };
            match keyboard.set_xkb_config(self, xkb) {
                Ok(()) => info!("keyboard layouts: {layout} ({variant})"),
                Err(err) => {
                    // A layout xkb does not know: keep the keymap in use.
                    warn!("keyboard.json: layouts {layout} ({variant}): {err:?}");
                    self.keyboard_config.layouts = old.layouts;
                    self.keyboard_config.model = old.model;
                    self.keyboard_config.options = old.options;
                }
            }
        }
    }

    /// The index of the active input source.
    pub fn active_layout(&mut self) -> u32 {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return 0;
        };
        keyboard.with_xkb_state(self, |context| {
            context
                .xkb()
                .lock()
                .map(|xkb| xkb.active_layout().0)
                .unwrap_or(0)
        })
    }

    /// Switches to the next input source; false with only one.
    pub fn next_input_source(&mut self) -> bool {
        if self.keyboard_config.layouts.len() < 2 {
            return false;
        }
        let Some(keyboard) = self.seat.get_keyboard() else {
            return false;
        };
        keyboard.with_xkb_state(self, |mut context| context.cycle_next_layout());
        true
    }

    /// `input.state`: the settings in use, by kind of device, and the kinds connected.
    pub fn input_state(&self) -> Value {
        let settings = |s: &PointerSettings| {
            serde_json::json!({
                "speed": s.speed,
                "naturalScroll": s.natural_scroll,
                "scrollSpeed": s.scroll_speed,
                "leftHanded": s.left_handed,
                "secondaryClick": s.click_method.map(|m| match m {
                    ClickMethod::TwoFinger => "twoFinger",
                    ClickMethod::BottomRight => "bottomRight",
                    ClickMethod::BottomLeft => "bottomLeft",
                }),
                "tapToClick": s.tap_to_click,
                "disableWhileTyping": s.disable_while_typing,
            })
        };
        let report = self.system.reported();
        serde_json::json!({
            "mouse": settings(&self.input_config.mouse),
            "touchpad": settings(&self.input_config.touchpad),
            "devices": {
                "mouse": report.get("input.mouse").and_then(Value::as_bool).unwrap_or(false),
                "touchpad": report.get("input.touchpad").and_then(Value::as_bool).unwrap_or(false),
            },
        })
    }

    /// `keyboard.state`.
    pub fn keyboard_state(&mut self) -> Value {
        let active = self.active_layout();
        let config = &self.keyboard_config;
        serde_json::json!({
            "repeatDelay": config.repeat_delay,
            "repeatRate": config.repeat_rate,
            "layouts": config.layouts.iter().map(Layout::name).collect::<Vec<_>>(),
            "model": config.model,
            "options": config.options,
            "active": active,
        })
    }
}

// --- watching ------------------------------------------------------------------------------

/// Watches `dir` (created if missing) and calls `State::config_changed` with the name of each
/// file written, renamed or removed there.
pub fn watch(handle: &LoopHandle<'static, Ossido>, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let fd: OwnedFd = inotify::init(inotify::CreateFlags::NONBLOCK | inotify::CreateFlags::CLOEXEC)
        .map_err(|e| format!("inotify: {e}"))?;
    inotify::add_watch(
        &fd,
        dir,
        inotify::WatchFlags::CLOSE_WRITE
            | inotify::WatchFlags::MOVED_TO
            | inotify::WatchFlags::DELETE
            | inotify::WatchFlags::MOVED_FROM,
    )
    .map_err(|e| format!("inotify {}: {e}", dir.display()))?;
    handle
        .insert_source(
            Generic::new(fd, Interest::READ, Mode::Level),
            |_, fd, data| {
                data.state.wake();
                let mut names: Vec<String> = Vec::new();
                let mut buffer = [std::mem::MaybeUninit::<u8>::uninit(); 4096];
                let mut reader = inotify::Reader::new(&*fd, &mut buffer);
                while let Ok(event) = reader.next() {
                    if let Some(name) = event.file_name().and_then(|n| n.to_str().ok())
                        && !names.iter().any(|n| n == name)
                    {
                        names.push(name.to_owned());
                    }
                }
                for name in names {
                    data.state.config_changed(&name);
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| format!("inotify source: {e}"))?;
    info!("settings folder: {}", dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn theme_accessibility_flags_are_read() {
        let parse = |json: &str| match serde_json::from_str::<Value>(json) {
            Ok(Value::Object(map)) => ThemeConfig::parse(Some(&map)),
            _ => ThemeConfig::parse(None),
        };
        let both = parse(r#"{"reduceMotion": true, "reduceTransparency": true}"#);
        assert!(both.reduce_motion && both.reduce_transparency);
        let junk = parse(r#"{"reduceMotion": "yes"}"#);
        assert!(!junk.reduce_motion && !junk.reduce_transparency);
        assert!(!ThemeConfig::parse(None).reduce_motion);
    }

    #[test]
    fn theme_glass_level_is_read_and_checked() {
        let parse = |json: &str| match serde_json::from_str::<Value>(json) {
            Ok(Value::Object(map)) => ThemeConfig::parse(Some(&map)),
            _ => ThemeConfig::parse(None),
        };
        assert_eq!(parse(r#"{"glass": 0.8}"#).glass, 0.8);
        assert_eq!(parse(r#"{"glass": 3}"#), ThemeConfig::default());
        assert_eq!(
            parse(r#"{"glass": "x", "accent": "red"}"#),
            ThemeConfig::default()
        );
        assert_eq!(ThemeConfig::parse(None).glass, 0.5);
    }

    use super::*;

    fn map(text: &str) -> serde_json::Map<String, Value> {
        match serde_json::from_str(text).unwrap() {
            Value::Object(map) => map,
            _ => panic!("not an object"),
        }
    }

    #[test]
    fn input_file_by_device_kind() {
        assert_eq!(InputConfig::parse(None), InputConfig::default());
        let config = InputConfig::parse(Some(&map(
            r#"{"mouse": {"speed": -0.5, "naturalScroll": false, "secondaryClick": "left",
                          "scrollSpeed": 2},
                "touchpad": {"speed": 3, "tapToClick": true, "secondaryClick": "bottomLeft",
                             "disableWhileTyping": false, "scrollSpeed": 0},
                "doubleClickMs": 400}"#,
        )));
        assert_eq!(config.mouse.speed, Some(-0.5));
        assert_eq!(config.mouse.natural_scroll, Some(false));
        assert_eq!(config.mouse.left_handed, Some(true));
        assert_eq!(config.mouse.scroll_speed, 2.0);
        assert_eq!(config.mouse.click_method, None);
        // Out of range: libinput's default speed, the normal scroll.
        assert_eq!(config.touchpad.speed, None);
        assert_eq!(config.touchpad.scroll_speed, 1.0);
        assert_eq!(config.touchpad.tap_to_click, Some(true));
        assert_eq!(config.touchpad.click_method, Some(ClickMethod::BottomLeft));
        assert_eq!(config.touchpad.left_handed, Some(true));
        assert_eq!(config.touchpad.disable_while_typing, Some(false));
        let two = InputConfig::parse(Some(&map(
            r#"{"touchpad": {"secondaryClick": "twoFinger"}}"#,
        )));
        assert_eq!(two.touchpad.left_handed, Some(false));
    }

    #[test]
    fn layouts_take_an_optional_variant() {
        assert_eq!(
            Layout::parse("de(nodeadkeys)"),
            Some(Layout {
                layout: "de".into(),
                variant: "nodeadkeys".into()
            })
        );
        assert_eq!(Layout::parse(" it ").unwrap().name(), "it");
        for bad in ["", "(x)", "us(", "us,it", "us(x;y)", "a b"] {
            assert_eq!(Layout::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn keyboard_file_over_the_defaults() {
        let defaults = (375, 11);
        let none = KeyboardConfig::parse(None, defaults);
        assert_eq!((none.repeat_delay, none.repeat_rate), defaults);
        assert!(none.layouts.is_empty());

        let config = KeyboardConfig::parse(
            Some(&map(r#"{"repeatDelay": 250, "repeatRate": 30.4,
                    "layouts": ["us", "it", 3, "bad layout", "de(nodeadkeys)", "fr", "es"],
                    "options": "caps:escape"}"#)),
            defaults,
        );
        assert_eq!((config.repeat_delay, config.repeat_rate), (250, 30));
        assert_eq!(
            config.xkb_names(),
            ("us,it,de,fr".to_owned(), ",,nodeadkeys,".to_owned())
        );
        assert_eq!(config.options, "caps:escape");

        // Out of range: that key keeps its default, rate 0 turns repeat off.
        let config = KeyboardConfig::parse(
            Some(&map(r#"{"repeatDelay": 5, "repeatRate": 0}"#)),
            defaults,
        );
        assert_eq!((config.repeat_delay, config.repeat_rate), (375, 0));
    }
}
