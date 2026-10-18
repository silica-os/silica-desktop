//! The system look: the Appearance choices every app follows live (accent, highlight, sidebar
//! icon size, scroll bars, click in the scroll bar, icon style, folder colour, glass, window
//! tint). Settings keeps them in `theme.json` (its own persistence, which the daemon reads at
//! start and Ossido watches for the glass and the tint) and tells the daemon with `SetLook`;
//! the apps read them from the typed `look.*` keys of `State`, so none of them parses JSON.

use std::collections::HashMap;
use std::path::Path;

use zbus::zvariant::{OwnedValue, Value};

/// The accent names (`multicolour` is the default).
pub const ACCENTS: &[&str] = &[
    "multicolour",
    "blue",
    "purple",
    "pink",
    "red",
    "orange",
    "yellow",
    "green",
    "graphite",
];

/// The range of the double click time, and its default (Flutter's own 300 ms).
pub const DOUBLE_CLICK_RANGE_MS: std::ops::RangeInclusive<u32> = 150..=1000;
pub const DOUBLE_CLICK_DEFAULT_MS: u32 = 300;

const SIDEBAR_SIZES: &[&str] = &["small", "medium", "large"];
const SCROLL_BARS: &[&str] = &["automatic", "whenScrolling", "always"];
const SCROLL_CLICKS: &[&str] = &["page", "spot"];
const ICON_STYLES: &[&str] = &["default", "dark", "clear", "tinted"];

/// The look in effect.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub accent: String,
    /// An accent name, or `automatic` (the accent).
    pub highlight: String,
    pub sidebar_icon_size: String,
    pub scroll_bars: String,
    pub scroll_click: String,
    pub icon_style: String,
    /// An accent name, or `automatic`.
    pub folder_colour: String,
    /// 0–1.
    pub glass: f64,
    pub window_tint: bool,
    /// Tab reaches every control.
    pub keyboard_navigation: bool,
    /// The longest time between the two clicks of a double click, in milliseconds.
    pub double_click_ms: u32,
    /// Glass drawn opaque (Accessibility › Display).
    pub reduce_transparency: bool,
    /// Animations skipped (Accessibility › Display).
    pub reduce_motion: bool,
    /// The high-contrast palette (Accessibility › Display).
    pub increase_contrast: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            accent: "multicolour".to_owned(),
            highlight: "automatic".to_owned(),
            sidebar_icon_size: "medium".to_owned(),
            scroll_bars: "automatic".to_owned(),
            scroll_click: "page".to_owned(),
            icon_style: "default".to_owned(),
            folder_colour: "automatic".to_owned(),
            glass: 0.5,
            window_tint: false,
            keyboard_navigation: false,
            double_click_ms: DOUBLE_CLICK_DEFAULT_MS,
            reduce_transparency: false,
            reduce_motion: false,
            increase_contrast: false,
        }
    }
}

fn choice(value: &serde_json::Value, allowed: &[&str], extra: Option<&str>) -> Option<String> {
    let text = value.as_str()?;
    (allowed.contains(&text) || extra == Some(text)).then(|| text.to_owned())
}

impl Look {
    /// Applies the keys of `json` that are valid and leaves the rest; false if one is not.
    pub fn merge(&mut self, json: &serde_json::Value) -> bool {
        let Some(object) = json.as_object() else {
            return false;
        };
        let mut next = self.clone();
        for (key, value) in object {
            let ok = match key.as_str() {
                "accent" => choice(value, ACCENTS, None).map(|v| next.accent = v),
                "highlight" => {
                    choice(value, ACCENTS, Some("automatic")).map(|v| next.highlight = v)
                }
                "sidebarIconSize" => {
                    choice(value, SIDEBAR_SIZES, None).map(|v| next.sidebar_icon_size = v)
                }
                "scrollBars" => choice(value, SCROLL_BARS, None).map(|v| next.scroll_bars = v),
                "scrollClick" => choice(value, SCROLL_CLICKS, None).map(|v| next.scroll_click = v),
                "iconStyle" => choice(value, ICON_STYLES, None).map(|v| next.icon_style = v),
                "folderColour" => {
                    choice(value, ACCENTS, Some("automatic")).map(|v| next.folder_colour = v)
                }
                "glass" => value
                    .as_f64()
                    .filter(|g| g.is_finite() && (0.0..=1.0).contains(g))
                    .map(|g| next.glass = g),
                "windowTint" => value.as_bool().map(|b| next.window_tint = b),
                "keyboardNavigation" => value.as_bool().map(|b| next.keyboard_navigation = b),
                "reduceTransparency" => value.as_bool().map(|b| next.reduce_transparency = b),
                "reduceMotion" => value.as_bool().map(|b| next.reduce_motion = b),
                "increaseContrast" => value.as_bool().map(|b| next.increase_contrast = b),
                "doubleClickMs" => value
                    .as_f64()
                    .filter(|n| n.fract() == 0.0 && n.is_finite())
                    .and_then(|n| u32::try_from(n as i64).ok())
                    .filter(|n| DOUBLE_CLICK_RANGE_MS.contains(n))
                    .map(|n| next.double_click_ms = n),
                // Unknown keys are ignored, as in every configuration file.
                _ => Some(()),
            };
            if ok.is_none() {
                return false;
            }
        }
        *self = next;
        true
    }

    /// The look saved in `theme.json`; anything missing or invalid is the default.
    pub fn read(file: &Path) -> Self {
        let mut look = Self::default();
        let json = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
        if let Some(serde_json::Value::Object(object)) = json {
            // One key at a time: a bad value must not drop the others.
            for (key, value) in object {
                let mut single = serde_json::Map::new();
                single.insert(key, value);
                look.merge(&serde_json::Value::Object(single));
            }
        }
        look
    }

    /// The `look.*` keys of the state dictionary.
    pub fn put(&self, put: &mut impl FnMut(&str, Value<'_>)) {
        put("look.accent", Value::from(self.accent.clone()));
        put("look.highlight", Value::from(self.highlight.clone()));
        put(
            "look.sidebarIconSize",
            Value::from(self.sidebar_icon_size.clone()),
        );
        put("look.scrollBars", Value::from(self.scroll_bars.clone()));
        put("look.scrollClick", Value::from(self.scroll_click.clone()));
        put("look.iconStyle", Value::from(self.icon_style.clone()));
        put("look.folderColour", Value::from(self.folder_colour.clone()));
        put("look.glass", Value::F64(self.glass));
        put("look.windowTint", Value::Bool(self.window_tint));
        put(
            "look.keyboardNavigation",
            Value::Bool(self.keyboard_navigation),
        );
        put("look.doubleClickMs", Value::U32(self.double_click_ms));
        put(
            "look.reduceTransparency",
            Value::Bool(self.reduce_transparency),
        );
        put("look.reduceMotion", Value::Bool(self.reduce_motion));
        put("look.increaseContrast", Value::Bool(self.increase_contrast));
    }
}

/// The values of a `SetLook` call as JSON (strings, booleans, numbers).
pub fn settings_json(settings: &HashMap<String, OwnedValue>) -> serde_json::Value {
    let mut json = serde_json::Map::new();
    for (key, value) in settings {
        let value = match &**value {
            Value::Bool(b) => serde_json::Value::from(*b),
            Value::F64(n) => serde_json::Value::from(*n),
            Value::U32(n) => serde_json::Value::from(f64::from(*n)),
            Value::I32(n) => serde_json::Value::from(f64::from(*n)),
            Value::I64(n) => serde_json::Value::from(*n as f64),
            Value::U64(n) => serde_json::Value::from(*n as f64),
            Value::Str(s) => serde_json::Value::from(s.as_str()),
            _ => serde_json::Value::Null,
        };
        json.insert(key.clone(), value);
    }
    serde_json::Value::Object(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_keeps_valid_keys_and_rejects_bad_ones() {
        let mut look = Look::default();
        assert!(look.merge(&json!({"accent": "green", "glass": 0.25, "x": 1})));
        assert!(look.merge(&json!({"keyboardNavigation": true, "doubleClickMs": 500})));
        assert!(look.keyboard_navigation && look.double_click_ms == 500);
        assert!(look.merge(&json!({"reduceMotion": true, "reduceTransparency": true})));
        assert!(look.reduce_motion && look.reduce_transparency);
        assert!(!look.merge(&json!({"reduceMotion": "yes"})));
        assert!(look.merge(&json!({"increaseContrast": true})));
        assert!(look.increase_contrast);
        assert!(!look.merge(&json!({"increaseContrast": 1})));
        assert!(!look.merge(&json!({"doubleClickMs": 50})));
        assert!(!look.merge(&json!({"doubleClickMs": 400.5})));
        assert_eq!(look.double_click_ms, 500);
        assert_eq!(look.accent, "green");
        assert_eq!(look.glass, 0.25);
        let before = look.clone();
        assert!(!look.merge(&json!({"accent": "mauve", "windowTint": true})));
        assert!(!look.merge(&json!({"glass": 3})));
        assert!(!look.merge(&json!("accent")));
        assert_eq!(look, before);
    }

    #[test]
    fn highlight_and_folder_accept_automatic_only() {
        let mut look = Look::default();
        assert!(look.merge(&json!({"highlight": "automatic", "folderColour": "pink"})));
        assert!(!look.merge(&json!({"accent": "automatic"})));
    }

    #[test]
    fn a_bad_value_in_the_file_does_not_drop_the_others() {
        let dir = std::env::temp_dir().join(format!("silica-look-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap_or_default();
        let file = dir.join("theme.json");
        std::fs::write(
            &file,
            r#"{"accent":"red","scrollBars":"sideways","iconStyle":"clear"}"#,
        )
        .unwrap_or_default();
        let look = Look::read(&file);
        assert_eq!(look.accent, "red");
        assert_eq!(look.scroll_bars, "automatic");
        assert_eq!(look.icon_style, "clear");
        assert_eq!(Look::read(&dir.join("missing.json")), Look::default());
        std::fs::remove_dir_all(&dir).unwrap_or_default();
    }
}
