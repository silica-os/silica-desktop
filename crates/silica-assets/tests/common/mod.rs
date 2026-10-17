//! Shared by the integration tests: a source tree built from `tests/fixtures` (JSON and TOML,
//! committed) plus the SVG, PNG, font and sound files, generated here so the repository has no
//! asset files without a license row.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::fs;
use std::path::{Path, PathBuf};

use silica_assets::compile::Options;

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub options: Options,
}

impl Fixture {
    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

pub fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub fn svg(body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1024\" height=\"1024\" \
         viewBox=\"0 0 1024 1024\">{body}</svg>"
    )
}

pub fn rect(x: u32, y: u32, w: u32, h: u32, fill: &str) -> String {
    format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{fill}\"/>")
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// A straight (not premultiplied) RGBA PNG.
pub fn png_bytes(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut data = Vec::new();
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&pixel(x, y));
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&data)
        .unwrap();
    out
}

/// The straight color of the left half of the rendered icon of theme number `theme`.
pub fn rendered_color(theme: u8) -> [u8; 4] {
    [200 - 10 * theme, 100, 50, 128]
}

pub fn premultiply(c: [u8; 4]) -> [u8; 4] {
    let a = u32::from(c[3]);
    let m = |v: u8| ((u32::from(v) * a + 127) / 255) as u8;
    [m(c[0]), m(c[1]), m(c[2]), c[3]]
}

pub const FONT_BYTES: &[u8] = b"\x00\x01\x00\x00 not a real font, only bytes to carry";
pub const SOUND_BYTES: &[u8] = b"RIFF....WAVE not a real sound";

/// Builds the fixture sources in a temporary directory.
pub fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    copy_tree(&fixtures.join("icons"), &root.join("icons"));

    let icons = root.join("icons");
    // Mail: rendered PNGs, left half colored, right half transparent.
    let themes = [
        "icon",
        "icon-dark",
        "icon-clear",
        "icon-clear-dark",
        "icon-tinted",
        "icon-tinted-dark",
    ];
    for (i, name) in themes.iter().enumerate() {
        let color = rendered_color(i as u8);
        write(
            &icons.join(format!("apps/mail/{name}.png")),
            png_bytes(
                1024,
                1024,
                move |x, _| if x < 512 { color } else { [0, 0, 0, 0] },
            ),
        );
    }
    // Clock and Calendar: composed from SVG layers.
    let apps = icons.join("apps");
    write(
        &apps.join("clock/background.svg"),
        svg(&rect(0, 0, 1024, 1024, "#ffffff")),
    );
    write(
        &apps.join("clock/background-dark.svg"),
        svg(&rect(0, 0, 1024, 1024, "#222222")),
    );
    write(
        &apps.join("clock/foreground.svg"),
        svg(&rect(100, 100, 300, 300, "#ff0000")),
    );
    write(
        &apps.join("clock/foreground-dark.svg"),
        svg(&rect(100, 100, 300, 300, "#00ff00")),
    );
    write(
        &apps.join("clock/hand-hour.svg"),
        svg(&rect(500, 200, 24, 312, "#000000")),
    );
    write(
        &apps.join("clock/hand-hour-dark.svg"),
        svg(&rect(500, 200, 24, 312, "#ffffff")),
    );
    write(
        &apps.join("clock/hand-minute.svg"),
        svg(&rect(504, 100, 16, 412, "#000000")),
    );
    write(
        &apps.join("clock/hand-second.svg"),
        svg(&rect(508, 60, 8, 452, "#ff8800")),
    );
    write(
        &apps.join("clock/cap.svg"),
        svg("<circle cx=\"512\" cy=\"512\" r=\"30\" fill=\"#ff8800\"/>"),
    );
    write(
        &apps.join("calendar/background.svg"),
        svg(&rect(100, 100, 824, 824, "#ffffff")),
    );

    let files = icons.join("files");
    // The layers of the generic file and folder (each a distinct color, so tests can tell them
    // apart), and the other file icons as before.
    for (file, color) in [("back", "#eeeeee"), ("front", "#dddddd")] {
        write(
            &files.join(format!("file-generic/{file}.svg")),
            svg(&rect(170, 60, 684, 904, color)),
        );
    }
    for (file, area, color) in [
        ("back", (34, 116, 956, 776), "#4497f8"),
        ("middle", (92, 230, 840, 620), "#ffffff"),
        ("front", (34, 293, 956, 599), "#82c4fd"),
    ] {
        write(
            &files.join(format!("folder-generic/{file}.svg")),
            svg(&rect(area.0, area.1, area.2, area.3, color)),
        );
    }
    for name in ["trash-empty", "volume-external", "stack-close"] {
        write(
            &files.join(name).join("icon.svg"),
            svg(&rect(170, 60, 684, 904, "#eeeeee")),
        );
        write(
            &files.join(name).join("icon-dark.svg"),
            svg(&rect(170, 60, 684, 904, "#333333")),
        );
    }

    let badges = icons.join("badges");
    for name in ["alias", "unreadable"] {
        write(
            &badges.join(format!("{name}.svg")),
            svg("<circle cx=\"512\" cy=\"512\" r=\"412\" fill=\"#888888\"/>"),
        );
        write(
            &badges.join(format!("{name}-dark.svg")),
            svg("<circle cx=\"512\" cy=\"512\" r=\"412\" fill=\"#cccccc\"/>"),
        );
    }

    let symbol = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\">\
                  <path d=\"M3 12L21 12\" stroke=\"currentColor\"/></svg>";
    write(&icons.join("symbols/regular/key.svg"), symbol);
    write(&icons.join("symbols/regular/gear.svg"), symbol);
    write(&icons.join("symbols/semibold/key.svg"), symbol);
    write(&icons.join("symbols/semibold/gear.svg"), symbol);
    write(&icons.join("symbols/light/key.svg"), symbol);
    write(&icons.join("symbols/light/gear.svg"), symbol);

    let fonts = root.join("fonts");
    write(&fonts.join("Inter-Regular.ttf"), FONT_BYTES);
    write(&fonts.join("Mono.otf"), b"OTTO mono");
    write(&fonts.join("OFL.txt"), b"license text is not a font");

    let sounds = root.join("sounds");
    write(&sounds.join("alert-drop.wav"), SOUND_BYTES);

    // silica-wallpapers: one set, with a title, light and dark PNGs and a light preview.
    let wallpapers = root.join("wallpapers");
    let set = wallpapers.join("amalfi");
    write(
        &set.join("silica/light.png"),
        png_bytes(128, 72, |_, _| [250, 200, 100, 255]),
    );
    write(
        &set.join("silica/dark.png"),
        png_bytes(64, 36, |_, _| [10, 20, 40, 255]),
    );
    write(&set.join("previews/silica-light.jpg"), JPEG_BYTES);
    write(
        &set.join("wallpapers.json"),
        r#"{"version":"Amalfi","wallpapers":[{"id":"silica","title":"Silica test","variants":{"light":{"file":"silica/light.png"},"dark":{"file":"silica/dark.png"}}}]}"#,
    );

    let options = Options {
        icons,
        brand: None,
        wallpapers: Some(wallpapers),
        fonts: Some(fonts),
        sounds: Some(sounds),
        types: Some(fixtures.join("types.toml")),
        compose: Some(fixtures.join("compose.toml")),
        content_version: Some("silica-assets test".to_owned()),
    };
    Fixture { dir, options }
}

pub fn compile_fixture(fixture: &Fixture) -> Vec<u8> {
    silica_assets::compile::compile(&fixture.options).unwrap().0
}

pub fn path_of(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

/// A 4 x 2 baseline JPEG header up to the start of frame (enough for the size; never decoded).
pub const JPEG_BYTES: &[u8] = &[
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x4a, 0x46, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x02, 0x00,
    0x04, 0x01, 0x01, 0x11, 0x00, 0xff, 0xd9,
];
