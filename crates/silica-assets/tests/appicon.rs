//! `icon.assets`: compiling app icon folders (format 1 and 2) and reading them back.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::fs;
use std::path::Path;

use silica_assets::appicon::{AppIcon, Background, Blend, Mode, Role, Shadow};
use silica_assets::compile::app_icon;

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><rect width="400" height="400" fill="red"/></svg>"#;

fn folder(dir: &Path, json: &str, files: &[&str]) {
    fs::create_dir_all(dir.join("layers")).unwrap();
    for f in files {
        fs::write(dir.join(f), format!("{SVG}<!-- {f} -->")).unwrap();
    }
    fs::write(dir.join("icon.json"), json).unwrap();
}

#[test]
fn format_two_resolves_defaults_and_dark_overrides() {
    let tmp = tempfile::tempdir().unwrap();
    folder(
        tmp.path(),
        r##"{"format":2,"id":"t","background":{"light":{"gradient":["#ffffff","#000000"]}},
        "layers":[
         {"id":"a","role":"body","dark":{"opacity":0.5,"clear":{"luma":[10,20]}},"clear":{"mode":"glass"}},
         {"id":"b","role":"highlight","min_size":32},
         {"id":"hand","role":"dynamic","dynamic":{"kind":"clock-hand","unit":"minute","pivot":[500,510]}}]}"##,
        &[
            "layers/a.svg",
            "layers/a-dark.svg",
            "layers/b.svg",
            "layers/hand.svg",
        ],
    );
    let icon = AppIcon::decode(&app_icon(tmp.path()).unwrap()).unwrap();
    assert_eq!(icon.layers.len(), 3);
    assert!(matches!(
        icon.background[0],
        Background::Gradient([255, 255, 255], [0, 0, 0])
    ));
    // Dark default: graphite.
    assert!(matches!(
        icon.background[1],
        Background::Gradient([0x3a, 0x3a, 0x3c], [0x14, 0x14, 0x16])
    ));
    let a = &icon.layers[0];
    assert_eq!(a.role, Role::Body);
    assert_eq!(a.forms[0].shadow, Shadow::Neutral);
    assert_eq!(a.forms[0].clear.mode, Mode::Glass);
    assert_eq!(a.forms[1].opacity, 0.5);
    assert_eq!(a.forms[1].clear.luma, Some([10.0, 20.0]));
    assert_eq!(a.forms[1].clear.mode, Mode::Glass, "missing keys inherit");
    assert_ne!(
        a.forms[0].svg, a.forms[1].svg,
        "a-dark.svg is picked for the dark form"
    );
    let b = &icon.layers[1];
    assert_eq!(
        (b.forms[0].clear.mode, b.forms[0].clear.opacity),
        (Mode::Keep, 0.6)
    );
    assert_eq!(b.forms[0].shadow, Shadow::None);
    assert_eq!(b.forms[0].min_size, 32.0);
    assert_eq!(
        b.forms[0].svg, b.forms[1].svg,
        "no dark file: the same blob"
    );
    assert_eq!(b.forms[0].blend, Blend::Normal);
    let hand = icon.layers[2].dynamic.clone().unwrap();
    assert_eq!(hand.pivot, [500.0, 510.0]);
    // Deterministic.
    assert_eq!(app_icon(tmp.path()).unwrap(), app_icon(tmp.path()).unwrap());
}

#[test]
fn format_one_is_a_background_and_one_body_layer() {
    let tmp = tempfile::tempdir().unwrap();
    folder(
        tmp.path(),
        r#"{"format":1,"id":"m","layers":[{"role":"background","file":"background.svg"},
        {"role":"foreground","file":"foreground.svg","shadow":"neutral"}],
        "variants":{"dark":{"background":"background-dark.svg","foreground":"foreground-dark.svg"}}}"#,
        &[
            "background.svg",
            "background-dark.svg",
            "foreground.svg",
            "foreground-dark.svg",
        ],
    );
    let icon = AppIcon::decode(&app_icon(tmp.path()).unwrap()).unwrap();
    assert_eq!(icon.layers.len(), 1);
    let l = &icon.layers[0];
    assert_eq!(
        (l.role, l.forms[0].clear.mode, l.forms[0].tinted.mode),
        (Role::Body, Mode::Ramp, Mode::Ramp)
    );
    assert_ne!(l.forms[0].svg, l.forms[1].svg);
    assert!(matches!(icon.background[0], Background::Svg(_)));
}

#[test]
fn bad_sources_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = |json: &str, files: &[&str]| {
        folder(tmp.path(), json, files);
        assert!(app_icon(tmp.path()).is_err(), "{json}");
    };
    let bg = r##""background":{"light":{"gradient":["#fff000","#000fff"]}}"##;
    bad(
        &format!(r#"{{"format":2,{bg},"layers":[{{"id":"a","role":"body"}}]}}"#),
        &[],
    );
    bad(
        &format!(r#"{{"format":2,{bg},"layers":[{{"id":"a","role":"nope"}}]}}"#),
        &["layers/a.svg"],
    );
    bad(
        &format!(
            r#"{{"format":2,{bg},"layers":[{{"id":"a","role":"body","tinted":{{"mode":"glass"}}}}]}}"#
        ),
        &["layers/a.svg"],
    );
    bad(
        &format!(
            r#"{{"format":2,{bg},"layers":[{{"id":"a","role":"body","clear":{{"mode":"glass"}}}},{{"id":"b","role":"body","clear":{{"mode":"glass"}}}}]}}"#
        ),
        &["layers/a.svg", "layers/b.svg"],
    );
    bad(
        &format!(r#"{{"format":2,{bg},"layers":[{{"id":"a","role":"body","file":"../x.svg"}}]}}"#),
        &["layers/a.svg"],
    );
    bad(r#"{"format":7}"#, &[]);
}

#[test]
fn malformed_bytes_never_panic() {
    let tmp = tempfile::tempdir().unwrap();
    folder(
        tmp.path(),
        r##"{"format":2,"id":"t","background":{"light":{"gradient":["#ffffff","#000000"]}},"layers":[{"id":"a","role":"body"}]}"##,
        &["layers/a.svg"],
    );
    let bytes = app_icon(tmp.path()).unwrap();
    assert!(AppIcon::decode(&bytes).is_some());
    for n in 0..bytes.len() {
        assert!(AppIcon::decode(&bytes[..n]).is_none());
    }
    for i in 0..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[i] ^= 0xff;
        let _ = AppIcon::decode(&flipped);
    }
    assert!(AppIcon::decode(b"").is_none());
    assert!(AppIcon::digest(&bytes).is_some());
}
