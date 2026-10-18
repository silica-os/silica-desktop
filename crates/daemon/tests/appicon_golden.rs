//! The daemon's composer against PNGs rendered by `tools/compose.py` of silica-icons
//! (`tests/fixtures/appicon/gen.py`). The synthetic icon always runs; the pilot icons
//! (Mustache, Terminal, Clock) run when `SILICA_ICONS_DIR` points at a silica-icons checkout
//! on `amalfi-icons`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::path::{Path, PathBuf};

use silica_assets::appicon::AppIcon;
use silica_daemon::icons::appicon::{Appearance, compose};

const ACCENT: u32 = 0xd6_33_6c;
const LOOKS: [(&str, u32); 6] = [
    ("light", 0),
    ("dark", 4),
    ("clear", 1),
    ("clear-dark", 5),
    ("tinted", 2),
    ("tinted-dark", 6),
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/appicon")
}

fn load(dir: &Path) -> AppIcon {
    let bytes = silica_assets::compile::app_icon(dir).unwrap();
    AppIcon::decode(&bytes).unwrap()
}

fn golden(name: &str) -> Vec<u8> {
    let file = std::fs::File::open(fixtures().join("golden").join(name)).unwrap();
    let mut reader = png::Decoder::new(file).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

/// Returns (share of pixels whose alpha differs by more than 12 (the body edge), share of the others off by more than 12 in a colour channel as
/// shown on white, their mean absolute difference).
fn distance(a: &[u8], b: &[u8]) -> (f64, f64, f64) {
    assert_eq!(a.len(), b.len());
    let (mut edge, mut off, mut sum, mut counted) = (0usize, 0usize, 0u64, 0usize);
    for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
        if p[3].abs_diff(q[3]) > 12 {
            edge += 1;
            continue;
        }
        let over = |px: &[u8]| -> [f64; 3] {
            let al = f64::from(px[3]) / 255.0;
            [0, 1, 2].map(|i| f64::from(px[i]) * al + 255.0 * (1.0 - al))
        };
        let (x, y) = (over(p), over(q));
        let worst = (0..3).map(|i| (x[i] - y[i]).abs()).fold(0.0, f64::max);
        if worst > 12.0 {
            off += 1;
        }
        sum += (0..3).map(|i| (x[i] - y[i]).abs() as u64).sum::<u64>();
        counted += 1;
    }
    let n = (a.len() / 4) as f64;
    let counted = counted.max(1) as f64;
    (
        edge as f64 / n,
        off as f64 / counted,
        sum as f64 / (counted * 3.0),
    )
}

fn check(dir: &Path, name: &str, size: u32, looks: &[(&str, u32)]) {
    let icon = load(dir);
    for (look, word) in looks {
        let ap = Appearance::from_u32(*word).unwrap();
        let got = compose(&icon, size, size as f32, ap, ACCENT).unwrap();
        let want = golden(&format!("{name}-{size}-{look}.png"));
        let (edge, off, mean) = distance(&got, &want);
        eprintln!("{name} {size} {look}: edge {edge:.4} off {off:.4} mean {mean:.3}");
        assert!(
            edge < 0.01,
            "{name} {look}: {edge:.4} of the pixels differ in alpha"
        );
        assert!(off < 0.002, "{name} {look}: {off:.4} of the pixels differ");
        assert!(mean < 0.3, "{name} {look}: mean difference {mean:.3}");
    }
}

#[test]
fn the_synthetic_icon_matches_compose_py_in_every_appearance() {
    let dir = fixtures().join("synthetic");
    check(&dir, "synthetic", 128, &LOOKS);
    // Below the layer's min_size.
    check(&dir, "synthetic", 64, &LOOKS[..1]);
}

#[test]
fn the_pilot_icons_match_compose_py_without_their_dynamic_layers() {
    let Some(icons) = std::env::var_os("SILICA_ICONS_DIR").map(PathBuf::from) else {
        eprintln!("SILICA_ICONS_DIR not set: pilot goldens skipped");
        return;
    };
    for app in ["mustache", "terminal", "clock"] {
        let dir = icons.join("apps").join(app);
        if !dir.join("icon.json").is_file() {
            continue;
        }
        check(&dir, app, 128, &LOOKS);
    }
}

#[test]
fn clear_has_a_dark_form() {
    use silica_daemon::icons::appicon::Look;
    let clear = Appearance::from_u32(1).unwrap();
    assert_eq!((clear.look, clear.dark), (Look::Clear, false));
    let clear_dark = Appearance::from_u32(5).unwrap();
    assert_eq!((clear_dark.look, clear_dark.dark), (Look::Clear, true));
    assert!(Appearance::from_u32(4).unwrap().dark);
    assert!(Appearance::from_u32(6).unwrap().dark);
}

#[test]
fn the_service_composes_caches_and_refuses_what_it_cannot_do() {
    use silica_daemon::icons::appicon::AppIcons;
    use silica_daemon::icons::apps::Dirs;

    let tmp = std::env::temp_dir().join(format!("appicon-svc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let folder = tmp.join("icon");
    std::fs::create_dir_all(&folder).unwrap();
    let bytes = silica_assets::compile::app_icon(&fixtures().join("synthetic")).unwrap();
    std::fs::write(folder.join("icon.assets"), &bytes).unwrap();
    let svc = AppIcons::new(tmp.join("cache"), Dirs::default());
    let dir = folder.to_str().unwrap();

    let light = svc.icon(dir, 96, 1.0, 0, 0).unwrap();
    assert_eq!(
        png::Decoder::new(std::fs::File::open(&light).unwrap())
            .read_info()
            .unwrap()
            .info()
            .width,
        96
    );
    assert_eq!(svc.icon(dir, 96, 1.0, 0, 0).unwrap(), light, "cache hit");
    // The accent only matters for the tinted looks.
    assert_eq!(svc.icon(dir, 96, 1.0, 0, 0x112233).unwrap(), light);
    let a = svc.icon(dir, 96, 1.0, 2, 0x112233).unwrap();
    let b = svc.icon(dir, 96, 1.0, 2, 0x445566).unwrap();
    assert_ne!(a, b);
    assert_ne!(svc.icon(dir, 96, 1.0, 1, 0).unwrap(), light);
    // Refused: bad appearance, bad size, no compiled icon, an app that is not installed.
    assert!(svc.icon(dir, 96, 1.0, 3, 0).is_none());
    assert!(svc.icon(dir, 96, 1.0, 8, 0).is_none());
    assert!(svc.icon(dir, 0, 1.0, 0, 0).is_none());
    assert!(svc.icon(dir, 100_000, 1.0, 0, 0).is_none());
    assert!(svc.icon(tmp.to_str().unwrap(), 96, 1.0, 0, 0).is_none());
    assert!(svc.icon("dev.frascella.Nothing", 96, 1.0, 0, 0).is_none());
    // A damaged file is not composed.
    std::fs::write(folder.join("icon.assets"), &bytes[..bytes.len() / 2]).unwrap();
    assert!(svc.icon(dir, 80, 1.0, 0, 0).is_none());
    let _ = std::fs::remove_dir_all(&tmp);
}
