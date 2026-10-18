//! Format 2 app icons in `silica.assets` (composites of the six themes, dynamic layers as slots)
//! and in `icon.assets` (calendar text fields). The synthetic icons always run; the 21 apps of
//! silica-icons run when `SILICA_ICONS_DIR` points at a checkout on `amalfi-icons`.
#![cfg(feature = "compile")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::*;
use silica_assets::appicon::{AppIcon, DynamicKind, Refresh, TextCase};
use silica_assets::compile::{Options, app_icon};
use silica_assets::*;

const THEMES: [Theme; 6] = [
    Theme::Default,
    Theme::Dark,
    Theme::Clear,
    Theme::ClearDark,
    Theme::Tinted,
    Theme::TintedDark,
];

const ICON_JSON: &str = r##"{
  "format": 2, "id": "f2", "name": "F2", "kind": "app", "keyline": "full",
  "background": { "light": { "gradient": ["#ffffff", "#dde3ec"] } },
  "layers": [
    { "id": "disc", "role": "body" },
    { "id": "month", "role": "dynamic", "tinted": { "mode": "accent" },
      "dynamic": { "kind": "calendar-month", "font": "Inter", "size": 190, "weight": 900,
                   "tracking": 0, "color": "#f5222f", "dark_color": "#ff5a62",
                   "box": [212, 144, 600, 230], "case": "upper", "format": "short",
                   "refresh": "midnight" } },
    { "id": "day", "role": "dynamic",
      "dynamic": { "kind": "calendar-day", "font": "Inter", "size": 308, "weight": 700,
                   "color": "#ffffff", "box": [212, 426, 600, 372], "refresh": "midnight" } },
    { "id": "hand", "role": "dynamic", "min_size": 64,
      "dynamic": { "kind": "clock-hand", "unit": "second", "pivot": [500, 510] } }
  ],
  "fallback": { "light": "icon.png", "dark": "icon-dark.png" }
}"##;

fn synthetic(icons: &Path) -> PathBuf {
    let dir = icons.join("apps/f2");
    fs::create_dir_all(dir.join("layers")).unwrap();
    fs::write(dir.join("icon.json"), ICON_JSON).unwrap();
    for (name, body) in [
        ("disc", rect(212, 212, 600, 600, "#f5222f")),
        ("disc-dark", rect(212, 212, 600, 600, "#ff5a62")),
        ("month", rect(300, 160, 424, 190, "#f5222f")),
        ("day", rect(300, 440, 424, 330, "#ffffff")),
        ("hand", rect(490, 200, 20, 312, "#000000")),
    ] {
        fs::write(dir.join(format!("layers/{name}.svg")), svg(&body)).unwrap();
    }
    for name in ["icon.png", "icon-dark.png"] {
        fs::write(dir.join(name), png_bytes(1024, 1024, |_, _| [9, 9, 9, 255])).unwrap();
    }
    dir
}

fn compile_assets(options: &Options) -> Assets {
    let (bytes, _) = silica_assets::compile::compile(options).unwrap();
    Assets::from_bytes(bytes).unwrap()
}

#[test]
fn a_format_two_app_gets_six_composites_and_its_dynamic_slots() {
    let fixture = fixture();
    synthetic(&fixture.options.icons);
    let assets = compile_assets(&fixture.options);
    let id = assets.lookup(KeySpace::App, "f2").expect("f2 compiled");
    let entry = assets.entry(id).unwrap();
    assert_ne!(entry.flags() & FLAG_HAS_DARK, 0);
    assert_ne!(entry.flags() & FLAG_HAS_DYNAMIC, 0);
    for theme in THEMES {
        let v = assets
            .pick(id, Query::composite(theme, 1024))
            .unwrap_or_else(|| panic!("no composite for {theme:?}"));
        assert_eq!(v.layer, Layer::Composite);
        assert_eq!(v.size_px, 1024);
    }
    // The static layer is drawn, the body corner outside the mask is clear.
    let kinds: Vec<SlotKind> = entry.slots().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            SlotKind::CalendarMonth,
            SlotKind::CalendarDay,
            SlotKind::ClockHand
        ]
    );
    let month = entry.slot(0).unwrap();
    assert_eq!(month.rect, [212.0, 144.0, 600.0, 230.0]);
    assert_eq!(
        (month.font, month.size, month.weight),
        (Some("Inter"), 190, 900)
    );
    assert_eq!(month.color, [0xF5222FFF, 0xFF5A62FF]);
    assert_eq!(month.format, Some("month:upper:short"));
    let day = entry.slot(1).unwrap();
    assert_eq!(day.format, Some("day"));
    assert_eq!(day.rect, [212.0, 426.0, 600.0, 372.0]);
    let hand = entry.slot(2).unwrap();
    assert_eq!((hand.unit, hand.min_size), (Unit::Second as u8, 64));
    assert_eq!(hand.rect[..2], [500.0, 510.0]);
    assert!(
        assets
            .pick(
                id,
                Query {
                    theme: Theme::Default,
                    layer: Layer::HandSecond,
                    state: State::Normal,
                    side: 256
                }
            )
            .is_some()
    );
}

#[test]
fn calendar_text_fields_reach_icon_assets() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = synthetic(tmp.path());
    let icon = AppIcon::decode(&app_icon(&dir).unwrap()).unwrap();
    let month = icon.layers[1].dynamic.clone().unwrap();
    assert_eq!(month.kind, DynamicKind::CalendarMonth);
    let t = month.text.unwrap();
    assert_eq!(t.font, "Inter");
    assert_eq!((t.size, t.weight, t.tracking), (190.0, 900, 0.0));
    assert_eq!(t.color, [[0xf5, 0x22, 0x2f], [0xff, 0x5a, 0x62]]);
    assert_eq!(t.rect, [212.0, 144.0, 600.0, 230.0]);
    assert_eq!((t.case, t.format.as_str()), (TextCase::Upper, "short"));
    assert_eq!(t.refresh, Refresh::Midnight);
    let day = icon.layers[2].dynamic.clone().unwrap().text.unwrap();
    assert_eq!((day.case, day.format.as_str()), (TextCase::AsIs, ""));
    assert_eq!(day.color[0], day.color[1], "no dark_color: the same colour");
    // Only the calendar kinds carry text.
    assert!(icon.layers[3].dynamic.clone().unwrap().text.is_none());
    // Round trip through the bytes.
    let again = AppIcon::decode(&icon_bytes(&icon)).unwrap();
    assert_eq!(again, icon);
}

fn icon_bytes(icon: &AppIcon) -> Vec<u8> {
    icon.encode()
}

#[test]
fn calendar_text_without_a_box_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = synthetic(tmp.path());
    let json = ICON_JSON.replace("\"box\": [212, 144, 600, 230], ", "");
    fs::write(dir.join("icon.json"), json).unwrap();
    let err = app_icon(&dir).unwrap_err().to_string();
    assert!(err.contains("box"), "{err}");
}

fn icons_checkout() -> Option<PathBuf> {
    let icons = PathBuf::from(std::env::var_os("SILICA_ICONS_DIR")?);
    icons.join("apps").is_dir().then_some(icons)
}

#[test]
fn all_the_apps_of_silica_icons_compile_in_both_files() {
    let Some(icons) = icons_checkout() else {
        eprintln!("SILICA_ICONS_DIR not set: the 21 apps skipped");
        return;
    };
    let mut apps: Vec<String> = fs::read_dir(icons.join("apps"))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| icons.join("apps").join(n).join("icon.json").is_file())
        .collect();
    apps.sort();
    // The Silica logo (apps/silica) is format 1 and not an app icon; the others are format 2.
    assert!(apps.len() >= 21, "{apps:?}");

    // icon.assets, one by one.
    for app in &apps {
        let dir = icons.join("apps").join(app);
        let icon = AppIcon::decode(&app_icon(&dir).unwrap_or_else(|e| panic!("{app}: {e}")))
            .unwrap_or_else(|| panic!("{app}: does not decode"));
        assert!(!icon.layers.is_empty(), "{app}");
    }

    // silica.assets: every app, none left out (no shell_apps key).
    let tmp = tempfile::tempdir().unwrap();
    let types = tmp.path().join("types.toml");
    fs::write(&types, "[app]\n").unwrap();
    let options = Options {
        icons: icons.clone(),
        types: Some(types),
        ..Options::default()
    };
    let assets = compile_assets(&options);
    for app in &apps {
        let id = assets
            .lookup(KeySpace::App, app)
            .unwrap_or_else(|| panic!("{app} not in silica.assets"));
        for theme in THEMES {
            let v = assets
                .pick(id, Query::composite(theme, 1024))
                .unwrap_or_else(|| panic!("{app}: no {theme:?} composite"));
            assert_eq!(v.layer, Layer::Composite, "{app}");
        }
    }
    // Calendar: the text fields are slots; Clock: hands and cap.
    let calendar = assets
        .entry(assets.lookup(KeySpace::App, "calendar").unwrap())
        .unwrap();
    let kinds: Vec<SlotKind> = calendar.slots().map(|s| s.kind).collect();
    assert_eq!(kinds, [SlotKind::CalendarMonth, SlotKind::CalendarDay]);
    let clock = assets
        .entry(assets.lookup(KeySpace::App, "clock").unwrap())
        .unwrap();
    assert_eq!(
        clock
            .slots()
            .filter(|s| s.kind == SlotKind::ClockHand)
            .count(),
        3
    );
    assert_eq!(
        clock
            .slots()
            .filter(|s| s.kind == SlotKind::ClockCap)
            .count(),
        1
    );
}
