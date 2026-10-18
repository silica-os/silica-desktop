//! Whole-icon tests: sources compiled by silica-assets, planned and rendered.

use std::fs;
use std::path::{Path, PathBuf};

use silica_assets::compile::{Options, compile};

use super::*;

const DEJAVU: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("icons-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn svg(body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1024\" height=\"1024\" \
         viewBox=\"0 0 1024 1024\">{body}</svg>"
    )
}

fn rect(x: u32, y: u32, w: u32, h: u32, fill: &str) -> String {
    format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{fill}\"/>")
}

fn png(side: u32, color: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, side, side);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = (0..side * side).flat_map(|_| color).collect();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&data)
        .unwrap();
    out
}

/// Sources: simple layers, the real rules, a square symbol for every glyph they name, and
/// DejaVu as Inter when the system has it.
fn assets(root: &Path) -> Assets {
    assets_with(root, None)
}

/// [`assets`] with the artwork of the archive type replaced by `archive` (SVG text).
fn assets_with(root: &Path, archive: Option<&str>) -> Assets {
    let icons = root.join("icons");
    fs::create_dir_all(icons.join("apps")).unwrap();
    for (name, layers) in [
        ("file-generic", vec!["back", "front"]),
        ("folder-generic", vec!["back", "middle", "front"]),
    ] {
        let list: Vec<_> = layers
            .iter()
            .map(|l| {
                let when = if *l == "middle" {
                    ",\"when\":\"not-empty\""
                } else {
                    ""
                };
                format!("{{\"role\":\"{l}\",\"file\":\"{l}.svg\"{when}}}")
            })
            .collect();
        write(
            &icons.join(format!("files/{name}/icon.json")),
            format!(
                "{{\"format\":1,\"id\":\"{name}\",\"layers\":[{}]}}",
                list.join(",")
            ),
        );
    }
    write(
        &icons.join("files/file-generic/back.svg"),
        svg(&rect(170, 60, 684, 904, "#f0f0f0")),
    );
    write(
        &icons.join("files/file-generic/front.svg"),
        svg(&rect(650, 60, 204, 204, "#c0c0c0")),
    );
    write(
        &icons.join("files/folder-generic/back.svg"),
        svg(&rect(34, 116, 956, 776, "#4497f8")),
    );
    write(
        &icons.join("files/folder-generic/middle.svg"),
        svg(&rect(92, 230, 840, 620, "#ffffff")),
    );
    write(
        &icons.join("files/folder-generic/front.svg"),
        svg(&rect(34, 293, 956, 599, "#82c4fd")),
    );

    // The glyph of a file nobody opens.
    write(
        &icons.join("files/system/unknown.svg"),
        svg(&rect(0, 0, 1024, 1024, "#0000c8")),
    );
    // The artwork of one system type; the others (executable...) are missing on purpose.
    write(
        &icons.join("files/system/archive.svg"),
        archive.map_or_else(|| svg(&rect(0, 0, 1024, 1024, "#c80000")), str::to_owned),
    );

    let rules: toml::Table = silica_assets::compile::DEFAULT_COMPOSE.parse().unwrap();
    let mut glyphs = Vec::new();
    for symbol in rules["folder"]["folders"].as_table().unwrap().values() {
        glyphs.push(symbol.as_str().unwrap().to_owned());
    }
    for weight in ["light", "regular", "semibold"] {
        for glyph in &glyphs {
            write(
                &icons.join(format!("symbols/{weight}/{glyph}.svg")),
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\">\
                 <rect x=\"4\" y=\"4\" width=\"16\" height=\"16\" fill=\"currentColor\"/></svg>",
            );
        }
    }
    let fonts = root.join("fonts");
    if let Ok(font) = fs::read(DEJAVU) {
        for name in [
            "Inter-Regular",
            "Inter-Medium",
            "Inter-SemiBold",
            "Inter-Bold",
            "Inter-Black",
        ] {
            write(&fonts.join(format!("{name}.ttf")), &font);
        }
    }
    write(&root.join("types.toml"), "[app]\n");
    let options = Options {
        icons,
        fonts: Some(fonts),
        types: Some(root.join("types.toml")),
        ..Options::default()
    };
    let (bytes, _) = compile(&options).unwrap();
    Assets::from_bytes(bytes).unwrap()
}

struct Setup {
    root: PathBuf,
    icons: Icons,
}

fn setup(name: &str) -> Setup {
    setup_with(name, None)
}

fn setup_with(name: &str, archive: Option<&str>) -> Setup {
    let root = tmp(name);
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    // The type database: names to types, and a text parent.
    write(
        &home.join(".local/share/mime/globs2"),
        "50:application/pdf:*.pdf\n50:text/plain:*.txt\n50:text/csv:*.csv\n50:application/x-blender:*.blend\n\
         50:application/json:*.json\n50:application/zip:*.zip\n",
    );
    write(
        &home.join(".local/share/mime/subclasses"),
        "text/csv text/plain\napplication/json text/plain\n",
    );
    let env_root = root.clone();
    let env = move |key: &str| -> Option<String> {
        let home = env_root.join("home");
        match key {
            "HOME" => Some(home.display().to_string()),
            "XDG_DATA_DIRS" => Some(env_root.join("nowhere").display().to_string()),
            "XDG_CONFIG_DIRS" => Some(env_root.join("nowhere").display().to_string()),
            _ => None,
        }
    };
    let icons = Icons::with_assets(assets_with(&root, archive), &env, root.join("cache")).unwrap();
    Setup { root, icons }
}

fn pixel(path: &Path, x: u32, y: u32) -> [u8; 4] {
    let img = image::open(path).unwrap().to_rgba8();
    img.get_pixel(x, y).0
}

fn near(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(&b).all(|(x, y)| x.abs_diff(*y) <= tolerance)
}

#[test]
fn an_empty_folder_has_no_middle_and_a_full_one_does() {
    let s = setup("middle");
    let empty = s.root.join("empty");
    let full = s.root.join("full");
    fs::create_dir_all(&empty).unwrap();
    fs::create_dir_all(&full).unwrap();
    fs::write(full.join("a"), "x").unwrap();
    // 256 px: the middle strip is canvas y 230..293, here (128, 66), above the front panel.
    let e = s.icons.icon(&empty, 256, 128.0).unwrap();
    let f = s.icons.icon(&full, 256, 128.0).unwrap();
    assert_ne!(e, f);
    assert!(near(pixel(&e, 128, 66), [0x44, 0x97, 0xf8, 255], 1));
    assert!(near(pixel(&f, 128, 66), [255, 255, 255, 255], 1));
    // The front panel is the same in both, the corners are transparent, and the size is exact.
    assert!(near(pixel(&e, 128, 200), [0x82, 0xc4, 0xfd, 255], 1));
    assert_eq!(pixel(&e, 2, 2)[3], 0);
    let img = image::open(&e).unwrap();
    assert_eq!((img.width(), img.height()), (256, 256));
    // Another size is another file, at that exact size.
    let small = s.icons.icon(&empty, 37, 18.5).unwrap();
    assert_eq!(image::open(&small).unwrap().width(), 37);
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn the_tint_reaches_back_front_and_symbol_but_not_the_middle() {
    let s = setup("tint");
    let rules = &s.icons.folder_rules;
    let look = FolderLook {
        non_empty: true,
        symbol: Some("doc-text".to_owned()),
        tint: Some([0xf2, 0x38, 0x2e]),
    };
    let plan = rules::plan_folder(rules, 256, 128.0, &look);
    let fonts = compose::Fonts::from_assets(&s.icons.assets);
    let tinted = compose::render(
        &s.icons.assets,
        s.icons.folder_entry,
        &plan,
        None,
        None,
        &[],
        &fonts,
    )
    .unwrap();
    let plain = rules::plan_folder(
        rules,
        256,
        128.0,
        &FolderLook {
            tint: None,
            ..look.clone()
        },
    );
    let plain = compose::render(
        &s.icons.assets,
        s.icons.folder_entry,
        &plain,
        None,
        None,
        &[],
        &fonts,
    )
    .unwrap();
    let at = |p: &resvg::tiny_skia::Pixmap, x: u32, y: u32| {
        let c = p.pixel(x, y).unwrap().demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    };
    // Back (above the panel, outside the middle): blue turned red.
    let back = at(&tinted, 40, 40);
    assert!(back[0] > back[2], "{back:?}");
    assert!(at(&plain, 40, 40)[2] > at(&plain, 40, 40)[0]);
    // Front panel too.
    let front = at(&tinted, 128, 200);
    assert!(front[0] > front[2], "{front:?}");
    // The middle stays white.
    assert_eq!(at(&tinted, 128, 66), [255, 255, 255, 255]);
    // The engraved symbol is tinted and darker than the panel.
    let symbol = at(&tinted, 128, 148);
    assert!(symbol[0] > symbol[2], "{symbol:?}");
    assert!(
        u32::from(symbol[0]) + u32::from(symbol[1]) < u32::from(front[0]) + u32::from(front[1])
    );
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn a_special_folder_gets_its_symbol_from_user_dirs() {
    let s = setup("special");
    let home = s.root.join("home");
    write(
        &home.join(".config/user-dirs.dirs"),
        "XDG_DOCUMENTS_DIR=\"$HOME/Documenti\"\n",
    );
    // `with_assets` read user-dirs at load: load again.
    let env = {
        let root = s.root.clone();
        move |key: &str| -> Option<String> {
            match key {
                "HOME" => Some(root.join("home").display().to_string()),
                "XDG_DATA_DIRS" | "XDG_CONFIG_DIRS" => {
                    Some(root.join("nowhere").display().to_string())
                }
                _ => None,
            }
        }
    };
    let icons =
        Icons::with_assets(assets(&s.root.join("again")), &env, s.root.join("cache2")).unwrap();
    let documents = home.join("Documenti");
    fs::create_dir_all(&documents).unwrap();
    assert_eq!(
        icons.folder_look(&documents).symbol.as_deref(),
        Some("doc-text")
    );
    assert_eq!(icons.folder_look(&home).symbol.as_deref(), Some("house"));
    let other = home.join("Other");
    fs::create_dir_all(&other).unwrap();
    assert_eq!(icons.folder_look(&other).symbol, None);
    assert!(!icons.folder_look(&other).non_empty);
    let _ = fs::remove_dir_all(&s.root);
}

const GREY: [u8; 4] = [0x5f, 0x63, 0x6b, 255];
const SHEET: [u8; 4] = [0xf0, 0xf0, 0xf0, 255];

/// A second service over the same home, so that entries and thumbnailers written after
/// `setup` are read.
fn reload(s: &Setup, tag: &str) -> Icons {
    let root = s.root.clone();
    let env = move |key: &str| -> Option<String> {
        match key {
            "HOME" => Some(root.join("home").display().to_string()),
            "XDG_DATA_DIRS" | "XDG_CONFIG_DIRS" => Some(root.join("nowhere").display().to_string()),
            _ => None,
        }
    };
    Icons::with_assets(
        assets(&s.root.join(tag)),
        &env,
        s.root.join(format!("cache-{tag}")),
    )
    .unwrap()
}

/// How many pixels of `img` in the rows `ys` are close to `color`, and their bounding rows.
fn count_color(
    path: &Path,
    ys: std::ops::Range<u32>,
    color: [u8; 4],
    tolerance: u8,
) -> (usize, Option<(u32, u32)>) {
    let img = image::open(path).unwrap().to_rgba8();
    let mut count = 0;
    let mut rows: Option<(u32, u32)> = None;
    for y in ys {
        for x in 0..img.width() {
            if near(img.get_pixel(x, y).0, color, tolerance) {
                count += 1;
                rows = Some(rows.map_or((y, y), |(a, b)| (a.min(y), b.max(y))));
            }
        }
    }
    (count, rows)
}

#[test]
fn the_shipped_rules_have_the_bigger_label_the_grey_and_the_bigger_folder_symbol() {
    let s = setup("shipped");
    let label = s.icons.file_rules.label.as_ref().unwrap();
    // 25 % over the former 111.67; the box still ends inside the sheet and starts under the
    // glyph box (which ends at 762).
    assert!((label.size / 111.67 - 1.25).abs() < 0.001, "{}", label.size);
    assert!(label.rect[1] + label.rect[3] / 2.0 > 762.0 + label.rect[3] / 4.0);
    assert!(label.rect[1] + label.rect[3] <= 1024.0 - 60.0);
    assert_eq!(
        (label.max_chars, label.min_size, label.upper),
        (5, 32.0, true)
    );
    // One grey for every file: the Amalfi Text/Secondary of the light palette.
    let tokens = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/amalfi/tokens.json"),
    )
    .unwrap();
    let tokens: serde_json::Value = serde_json::from_str(&tokens).unwrap();
    let secondary = tokens["colors"]["light"]["Text/Secondary"]
        .as_str()
        .unwrap();
    let hex = format!(
        "#{:02X}{:02X}{:02X}FF",
        label.color[0], label.color[1], label.color[2]
    );
    assert_eq!(secondary, hex);
    // The folder symbol box: 12 % bigger around the same center (512, 592).
    let b = s.icons.folder_rules.symbol_box;
    assert!((b[2] / 316.0 - 1.12).abs() < 0.001 && (b[3] / 316.0 - 1.12).abs() < 0.001);
    assert!((b[0] + b[2] / 2.0 - 512.0).abs() < 0.01 && (b[1] + b[3] / 2.0 - 592.0).abs() < 0.01);
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn a_file_without_an_app_is_the_unknown_glyph_with_the_extension_label() {
    let s = setup("unknown");
    // A known type (PDF) and an unknown one: no handler, so the question mark and the label.
    for name in ["a.pdf", "scene.blend", "x.zzz"] {
        let file = s.root.join(name);
        fs::write(&file, "x").unwrap();
        let look = s.icons.file_look(&file, 256.0);
        assert_eq!(look.glyph, Glyph::System("unknown".to_owned()), "{name}");
        let out = s.icons.icon(&file, 512, 256.0).unwrap();
        // The sheet around, the glyph (here a blue square) in the glyph box.
        assert!(near(pixel(&out, 100, 300), SHEET, 1), "{name}");
        assert!(near(pixel(&out, 256, 256), [0, 0, 200, 255], 2), "{name}");
        if Path::new(DEJAVU).is_file() {
            // The label with a glyph: the one grey, under the glyph box.
            let (n, rows) = count_color(&out, 360..470, GREY, 8);
            assert!(n > 200, "{name}: {n}");
            assert!(rows.is_some_and(|(a, b)| a > 370 && b < 460), "{rows:?}");
        }
    }
    // Same look, same file: a second PDF shares the cache entry with the first.
    let first = s.icons.icon(&s.root.join("a.pdf"), 512, 256.0).unwrap();
    let again = s.root.join("b.pdf");
    fs::write(&again, "y").unwrap();
    assert_eq!(s.icons.icon(&again, 512, 256.0), Some(first));
    // No extension, or one that is too long: the glyph and no label.
    let bare = s.root.join("README");
    let long = s.root.join("data.sqlite3x");
    for file in [&bare, &long] {
        fs::write(file, "x").unwrap();
        assert_eq!(
            s.icons.file_look(file, 256.0).glyph,
            Glyph::System("unknown".to_owned())
        );
        let out = s.icons.icon(file, 512, 256.0).unwrap();
        assert!(near(pixel(&out, 256, 256), [0, 0, 200, 255], 2));
        assert_eq!(count_color(&out, 0..512, GREY, 8).0, 0);
    }
    // Below 32 pt the label is gone, the glyph stays.
    if Path::new(DEJAVU).is_file() {
        let small = s.icons.icon(&s.root.join("a.pdf"), 48, 24.0).unwrap();
        assert_eq!(count_color(&small, 0..48, GREY, 8).0, 0);
    }
    let _ = fs::remove_dir_all(&s.root);
}

/// The real archive glyph of silica-icons (`files/system/archive.svg`, viewBox 500), when
/// `SILICA_ICONS_DIR` has it: an archive nobody handles shows it, with the ZIP label under.
#[test]
fn an_archive_without_a_handler_shows_the_archive_glyph_and_the_zip_label() {
    let Some(dir) = std::env::var_os("SILICA_ICONS_DIR") else {
        eprintln!("SILICA_ICONS_DIR not set: the real archive glyph is skipped");
        return;
    };
    let Ok(glyph) = fs::read_to_string(Path::new(&dir).join("files/system/archive.svg")) else {
        return;
    };
    let s = setup_with("real-archive", Some(&glyph));
    let zip = s.root.join("a.zip");
    fs::write(&zip, "x").unwrap();
    let look = s.icons.file_look(&zip, 256.0);
    assert_eq!(look.glyph, Glyph::System("archive".to_owned()));
    let out = s.icons.icon(&zip, 512, 256.0).unwrap();
    // The glyph box is drawn over the sheet: not the bare sheet any more.
    let img = image::open(&out).unwrap().to_rgba8();
    let drawn = (150..330u32)
        .flat_map(|y| (100..412u32).map(move |x| (x, y)))
        .filter(|&(x, y)| !near(img.get_pixel(x, y).0, SHEET, 6))
        .count();
    assert!(drawn > 2000, "the archive glyph is missing: {drawn}");
    if Path::new(DEJAVU).is_file() {
        // The ZIP label, in the one grey, under the glyph.
        let (n, rows) = count_color(&out, 330..480, GREY, 8);
        assert!(n > 100, "the ZIP label is missing: {n}");
        assert!(rows.is_some());
    }
    let _ = fs::remove_dir_all(&s.root);
}

/// A desktop entry of an app with an icon folder holding a green `icon.png`.
fn write_editor(s: &Setup, extra: &str) -> PathBuf {
    let app_dir = s.root.join("editor-icon");
    write(&app_dir.join("icon.png"), png(1024, [0, 200, 0, 255]));
    write(
        &s.root.join("home/.local/share/applications/editor.desktop"),
        format!(
            "[Desktop Entry]\nName=Editor\nX-Silica-Icon={}\nMimeType=text/plain;\n{extra}",
            app_dir.display()
        ),
    );
    app_dir
}

#[test]
fn the_default_app_icon_is_the_glyph_and_the_label_is_bigger_and_grey() {
    let s = setup("app");
    write_editor(&s, "");
    let icons = reload(&s, "again");
    let txt = s.root.join("note.txt");
    fs::write(&txt, "x").unwrap();
    // A CSV file inherits the handler of text/plain through its parent.
    let csv = s.root.join("t.csv");
    fs::write(&csv, "x").unwrap();
    for file in [&txt, &csv] {
        let out = icons.icon(file, 512, 256.0).unwrap();
        // The app icon covers canvas 253.10..770.9: pixels 126.5..385.4 of 512.
        assert!(near(pixel(&out, 256, 256), [0, 200, 0, 255], 2));
        assert!(near(pixel(&out, 135, 135), [0, 200, 0, 255], 2));
        assert!(near(pixel(&out, 120, 120), SHEET, 1));
        assert!(near(pixel(&out, 395, 395), SHEET, 1));
        if Path::new(DEJAVU).is_file() {
            // The label under the glyph: grey, and cap height 0.73 x 139.59 x 0.5 = 51 px
            // (it was 41 px at the former size).
            let (n, rows) = count_color(&out, 386..500, GREY, 8);
            assert!(n > 200, "{n}");
            let height = rows.map_or(0, |(a, b)| b - a + 1);
            assert!((46..=56).contains(&height), "{rows:?}");
        }
    }
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn an_icon_folder_with_icon_assets_is_composed_as_the_glyph() {
    let s = setup("composed");
    let app_dir = write_editor(&s, "");
    // A format-2 icon: a green body. icon.assets is the compiled form.
    let src = s.root.join("editor-src");
    write(
        &src.join("icon.json"),
        "{\"format\":2,\"id\":\"editor\",\"name\":\"Editor\",\"kind\":\"app\",\"keyline\":\"full\",\
         \"background\":{\"light\":{\"gradient\":[\"#00c800\",\"#00c800\"]}},\
         \"layers\":[{\"id\":\"body\",\"role\":\"body\"}]}",
    );
    write(
        &src.join("layers/body.svg"),
        svg(&rect(0, 0, 1024, 1024, "#00c800")),
    );
    write(&src.join("icon.png"), png(64, [255, 0, 0, 255]));
    fs::write(
        app_dir.join("icon.assets"),
        silica_assets::compile::app_icon(&src).unwrap(),
    )
    .unwrap();
    let icons = reload(&s, "again");
    let txt = s.root.join("note.txt");
    fs::write(&txt, "x").unwrap();
    assert!(
        matches!(icons.file_look(&txt, 256.0).glyph, Glyph::App(ref v) if v.starts_with("icon:"))
    );
    let out = icons.icon(&txt, 512, 256.0).unwrap();
    assert!(near(pixel(&out, 256, 256), [0, 200, 0, 255], 3));
    assert!(near(pixel(&out, 120, 120), SHEET, 1));
    // A damaged icon.assets falls back to the icon.png of the folder.
    fs::write(app_dir.join("icon.assets"), "not an icon").unwrap();
    let icons = reload(&s, "damaged");
    assert!(
        matches!(icons.file_look(&txt, 256.0).glyph, Glyph::App(ref v) if v.starts_with("file:"))
    );
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn a_handler_without_a_usable_icon_leaves_the_file_unknown() {
    let s = setup("third-party");
    let apps = s.root.join("home/.local/share/applications");
    write(
        &apps.join("viewer.desktop"),
        "[Desktop Entry]\nName=Viewer\nIcon=viewer\nMimeType=text/plain;\n",
    );
    write(
        &apps.join("broken.desktop"),
        format!(
            "[Desktop Entry]\nName=Broken\nX-Silica-Icon={}\nMimeType=image/x-nothing;\n",
            s.root.join("no-such-folder").display()
        ),
    );
    let icons = reload(&s, "again");
    let txt = s.root.join("note.txt");
    fs::write(&txt, "x").unwrap();
    assert_eq!(
        icons.file_look(&txt, 256.0).glyph,
        Glyph::System("unknown".to_owned())
    );
    let out = icons.icon(&txt, 512, 256.0).unwrap();
    assert!(near(pixel(&out, 256, 480), SHEET, 1));
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn a_document_foreground_replaces_the_app_icon_in_the_glyph_box() {
    let s = setup("document");
    let app_dir = write_editor(
        &s,
        "X-Silica-Document-Icons=text/plain=notes;image/png=missing;\n",
    );
    write(
        &app_dir.join("documents/notes.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\" viewBox=\"0 0 100 100\">\
         <rect width=\"100\" height=\"100\" fill=\"#0000c8\"/></svg>",
    );
    let icons = reload(&s, "again");
    let txt = s.root.join("note.txt");
    let csv = s.root.join("t.csv");
    let md = s.root.join("x.zzz");
    for file in [&txt, &csv, &md] {
        fs::write(file, "x").unwrap();
    }
    for file in [&txt, &csv] {
        assert!(matches!(
            icons.file_look(file, 256.0).glyph,
            Glyph::Document(_)
        ));
        let out = icons.icon(file, 512, 256.0).unwrap();
        // The glyph box is 131..381 of 512: blue inside, and not the app icon's green, whose
        // body reaches 126.5.
        assert!(near(pixel(&out, 256, 256), [0, 0, 200, 255], 2));
        assert!(near(pixel(&out, 135, 135), [0, 0, 200, 255], 2));
        assert!(near(pixel(&out, 128, 128), SHEET, 1));
        assert!(near(pixel(&out, 385, 385), SHEET, 1));
    }
    // A type the app does not list keeps the app icon (c); a missing document file as well.
    assert!(matches!(
        icons.file_look(&md, 256.0).glyph,
        Glyph::System(_)
    ));
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn icons_are_refused_beyond_the_size_limit_and_for_missing_files() {
    let s = setup("limits");
    let dir = s.root.join("d");
    fs::create_dir_all(&dir).unwrap();
    assert!(s.icons.icon(&dir, 0, 0.0).is_none());
    assert!(s.icons.icon(&dir, compose::MAX_SIDE + 1, 1.0).is_none());
    assert!(s.icons.icon(&s.root.join("missing"), 64, 32.0).is_none());
    let _ = fs::remove_dir_all(&s.root);
}

#[cfg(unix)]
fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    write(path, format!("#!/bin/sh\n{body}\n"));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Registers the type `application/x-sdoc` (`*.sdoc`) and a thumbnailer for it.
fn with_thumbnailer(s: &Setup, exec_body: &str) -> Icons {
    let home = s.root.join("home");
    write(
        &home.join(".local/share/mime/globs2"),
        "50:application/pdf:*.pdf\n50:text/plain:*.txt\n50:application/x-sdoc:*.sdoc\n50:image/png:*.png\n",
    );
    let bin = s.root.join("bin/maker");
    script(&bin, exec_body);
    write(
        &home.join(".local/share/thumbnailers/sdoc.thumbnailer"),
        format!(
            "[Thumbnailer Entry]\nTryExec={0}\nExec={0} %s %o\nMimeType=application/x-sdoc;image/png;\n",
            bin.display()
        ),
    );
    reload(s, "thumbs")
}

#[test]
fn an_app_thumbnailer_draws_the_preview_after_the_built_in_generators() {
    let s = setup("thumbnailer");
    let ready = s.root.join("ready.png");
    image::RgbaImage::from_pixel(32, 32, image::Rgba([200, 10, 10, 255]))
        .save(&ready)
        .unwrap();
    let icons = with_thumbnailer(&s, &format!("cp {} \"$2\"", ready.display()));
    let root = s.root.join("thumbs");
    let doc = s.root.join("a.sdoc");
    fs::write(&doc, "x").unwrap();
    let uri = format!("file://{}", doc.display());
    let (path, preview) =
        crate::thumbnails::icon_job(&root, Some(&icons), &uri, 128, 2.0, 0).unwrap();
    assert!(preview && path.starts_with(&root));
    assert!(near(pixel(&path, 10, 10), [200, 10, 10, 255], 1));
    // The flag that asks for the icon skips every preview.
    let (_, preview) = crate::thumbnails::icon_job(
        &root,
        Some(&icons),
        &uri,
        128,
        2.0,
        crate::thumbnails::ICON_NO_PREVIEW,
    )
    .unwrap();
    assert!(!preview);
    // A real image is drawn by the built-in generator even though a thumbnailer lists image/png.
    let pic = s.root.join("pic.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 200, 10, 255]))
        .save(&pic)
        .unwrap();
    let uri = format!("file://{}", pic.display());
    let (path, preview) =
        crate::thumbnails::icon_job(&root, Some(&icons), &uri, 128, 2.0, 0).unwrap();
    assert!(preview && near(pixel(&path, 10, 10), [10, 200, 10, 255], 1));
    // A type with no thumbnailer has no preview: the composed icon.
    let txt = s.root.join("n.txt");
    fs::write(&txt, "x").unwrap();
    let uri = format!("file://{}", txt.display());
    let (_, preview) = crate::thumbnails::icon_job(&root, Some(&icons), &uri, 128, 2.0, 0).unwrap();
    assert!(!preview);
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn a_failing_thumbnailer_falls_back_to_the_composed_icon_and_is_remembered() {
    let s = setup("thumbnailer-fail");
    let marker = s.root.join("runs");
    let icons = with_thumbnailer(&s, &format!("echo x >> {}\nexit 1", marker.display()));
    let root = s.root.join("thumbs");
    let doc = s.root.join("b.sdoc");
    fs::write(&doc, "x").unwrap();
    let uri = format!("file://{}", doc.display());
    for _ in 0..2 {
        let (path, preview) =
            crate::thumbnails::icon_job(&root, Some(&icons), &uri, 128, 2.0, 0).unwrap();
        assert!(!preview && path.starts_with(s.root.join("cache-thumbs")));
    }
    // The failure is on record: the program ran once.
    assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 1);
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn a_file_with_a_preview_answers_with_it_unless_the_icon_is_asked_for() {
    let s = setup("preview");
    let pic = s.root.join("pic.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([10, 200, 10, 255]))
        .save(&pic)
        .unwrap();
    let uri = format!("file://{}", pic.display());
    let root = s.root.join("thumbs");
    let (path, preview) =
        crate::thumbnails::icon_job(&root, Some(&s.icons), &uri, 128, 2.0, 0).unwrap();
    assert!(preview && path.starts_with(&root));
    let (path, preview) = crate::thumbnails::icon_job(
        &root,
        Some(&s.icons),
        &uri,
        128,
        2.0,
        crate::thumbnails::ICON_NO_PREVIEW,
    )
    .unwrap();
    assert!(!preview && path.starts_with(s.root.join("cache")));
    // Folders have no preview; without the icon service there is nothing to answer with.
    let folder = format!("file://{}", s.root.display());
    assert_eq!(
        crate::thumbnails::icon_job(&root, Some(&s.icons), &folder, 64, 1.0, 0).map(|a| a.1),
        Some(false)
    );
    assert!(crate::thumbnails::icon_job(&root, None, &folder, 64, 1.0, 0).is_none());
    let _ = fs::remove_dir_all(&s.root);
}

const ONE_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\" \
     viewBox=\"0 0 100 100\"><rect width=\"100\" height=\"100\" fill=\"#0000c8\"/></svg>";

/// An app with a Silica icon folder for `types`, and an optional document icons line.
fn editor_for(s: &Setup, types: &str, extra: &str) -> PathBuf {
    let app_dir = s.root.join("editor-icon");
    write(&app_dir.join("icon.png"), png(1024, [0, 200, 0, 255]));
    write(
        &s.root.join("home/.local/share/applications/editor.desktop"),
        format!(
            "[Desktop Entry]\nName=Editor\nX-Silica-Icon={}\nMimeType={types}\n{extra}",
            app_dir.display()
        ),
    );
    app_dir
}

#[test]
fn the_glyph_follows_the_precedence_from_the_full_icon_down_to_the_unknown_extension() {
    let s = setup("precedence");
    let txt = s.root.join("note.txt");
    let zip = s.root.join("a.zip");
    let run = s.root.join("a.run");
    for file in [&txt, &zip, &run] {
        fs::write(file, "x").unwrap();
    }
    let look = |icons: &Icons, file: &Path| icons.file_look(file, 256.0).glyph;
    // Nobody handles the types: the system type if its artwork exists, else the unknown glyph.
    let unknown = || Glyph::System("unknown".to_owned());
    assert_eq!(look(&s.icons, &txt), unknown());
    assert_eq!(look(&s.icons, &zip), Glyph::System("archive".to_owned()));
    // The executable type is listed but its artwork is missing: the unknown glyph.
    assert_eq!(look(&s.icons, &run), unknown());
    let out = s.icons.icon(&zip, 512, 256.0).unwrap();
    assert!(near(pixel(&out, 256, 256), [200, 0, 0, 255], 2));
    assert!(near(pixel(&out, 120, 120), SHEET, 1));
    let out = s.icons.icon(&run, 512, 256.0).unwrap();
    assert!(near(pixel(&out, 256, 256), [0, 0, 200, 255], 2));

    // The app icon beats the system type.
    let app = editor_for(&s, "text/plain;application/zip;", "");
    let icons = reload(&s, "app");
    assert!(matches!(look(&icons, &txt), Glyph::App(_)));
    assert!(matches!(look(&icons, &zip), Glyph::App(_)));

    // A handler without a Silica icon is not an app of ours: the system type stays.
    fs::remove_file(s.root.join("home/.local/share/applications/editor.desktop")).unwrap();
    write(
        &s.root.join("home/.local/share/applications/other.desktop"),
        "[Desktop Entry]\nName=Other\nIcon=other\nMimeType=application/zip;\n",
    );
    let icons = reload(&s, "third");
    assert_eq!(look(&icons, &zip), Glyph::System("archive".to_owned()));
    fs::remove_file(s.root.join("home/.local/share/applications/other.desktop")).unwrap();

    // The app foreground beats the app icon.
    editor_for(
        &s,
        "text/plain;application/zip;",
        "X-Silica-Document-Icons=text/plain=notes;application/zip=pack;\n",
    );
    write(&app.join("documents/notes.svg"), ONE_SVG);
    write(&app.join("documents/pack/foreground.svg"), ONE_SVG);
    let icons = reload(&s, "fg");
    assert!(matches!(look(&icons, &txt), Glyph::Document(_)));
    assert!(matches!(look(&icons, &zip), Glyph::Document(_)));

    // The app full icon beats the foreground (a folder with icon.svg and a foreground).
    write(
        &app.join("documents/notes/icon.svg"),
        svg(&rect(0, 0, 1024, 1024, "#8000c0")),
    );
    write(&app.join("documents/notes/foreground.svg"), ONE_SVG);
    let icons = reload(&s, "full");
    assert!(matches!(
        look(&icons, &txt),
        Glyph::Full { label: false, .. }
    ));
    assert!(matches!(look(&icons, &zip), Glyph::Document(_)));

    // A full icon that cannot be drawn is skipped: the app icon comes next.
    write(&app.join("documents/notes/icon.svg"), "not an svg");
    let icons = reload(&s, "broken");
    assert!(matches!(look(&icons, &txt), Glyph::App(_)));
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn a_full_icon_replaces_the_whole_sheet_in_both_forms() {
    let s = setup("full");
    let app = editor_for(
        &s,
        "text/plain;",
        "X-Silica-Document-Icons=text/plain=notes;\n",
    );
    let txt = s.root.join("note.txt");
    fs::write(&txt, "x").unwrap();
    // One file: icon.svg covers the canvas, the sheet is gone (the corner is the icon's), no
    // label.
    write(
        &app.join("documents/notes/icon.svg"),
        svg(&rect(0, 0, 1024, 1024, "#8000c0")),
    );
    let icons = reload(&s, "one");
    let out = icons.icon(&txt, 512, 256.0).unwrap();
    assert!(near(pixel(&out, 3, 3), [0x80, 0, 0xc0, 255], 1));
    assert!(near(pixel(&out, 256, 256), [0x80, 0, 0xc0, 255], 1));
    if Path::new(DEJAVU).is_file() {
        assert_eq!(count_color(&out, 386..500, GREY, 8).0, 0);
    }
    // Two layers, back then front.
    fs::remove_file(app.join("documents/notes/icon.svg")).unwrap();
    write(
        &app.join("documents/notes/back.svg"),
        svg(&rect(0, 0, 1024, 1024, "#00c000")),
    );
    write(
        &app.join("documents/notes/front.svg"),
        svg(&rect(400, 400, 224, 224, "#c0c000")),
    );
    let icons = reload(&s, "two");
    let out = icons.icon(&txt, 512, 256.0).unwrap();
    assert!(near(pixel(&out, 10, 10), [0, 0xc0, 0, 255], 1));
    assert!(near(pixel(&out, 256, 256), [0xc0, 0xc0, 0, 255], 1));
    // The `label` key in the folder asks for the extension label on the icon.
    write(&app.join("documents/notes/label"), "");
    let icons = reload(&s, "label");
    let out = icons.icon(&txt, 512, 256.0).unwrap();
    if Path::new(DEJAVU).is_file() {
        assert!(count_color(&out, 386..500, GREY, 8).0 > 200);
    }
    // Both forms are bounded: a layer above the size limit gives no full icon.
    let big = format!("{}{}", svg(""), " ".repeat(2 * 1024 * 1024 + 1));
    write(&app.join("documents/notes/back.svg"), big);
    let icons = reload(&s, "big");
    assert!(matches!(icons.file_look(&txt, 256.0).glyph, Glyph::App(_)));
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn a_preview_and_a_thumbnailer_beat_every_icon() {
    let s = setup("preview-over-icons");
    let app = s.root.join("editor-icon");
    write(&app.join("icon.png"), png(1024, [0, 200, 0, 255]));
    write(
        &s.root.join("home/.local/share/applications/editor.desktop"),
        format!(
            "[Desktop Entry]\nName=Editor\nX-Silica-Icon={}\nMimeType=application/x-sdoc;\n\
             X-Silica-Document-Icons=application/x-sdoc=doc;\n",
            app.display()
        ),
    );
    write(
        &app.join("documents/doc/icon.svg"),
        svg(&rect(0, 0, 1024, 1024, "#8000c0")),
    );
    let ready = s.root.join("ready.png");
    image::RgbaImage::from_pixel(32, 32, image::Rgba([200, 10, 10, 255]))
        .save(&ready)
        .unwrap();
    let icons = with_thumbnailer(&s, &format!("cp {} \"$2\"", ready.display()));
    let doc = s.root.join("a.sdoc");
    fs::write(&doc, "x").unwrap();
    assert!(matches!(
        icons.file_look(&doc, 128.0).glyph,
        Glyph::Full { .. }
    ));
    let root = s.root.join("thumbs");
    let uri = format!("file://{}", doc.display());
    let (path, preview) =
        crate::thumbnails::icon_job(&root, Some(&icons), &uri, 128, 2.0, 0).unwrap();
    assert!(preview && near(pixel(&path, 10, 10), [200, 10, 10, 255], 1));
    // Asked for the icon: the app's full icon.
    let (path, preview) = crate::thumbnails::icon_job(
        &root,
        Some(&icons),
        &uri,
        128,
        2.0,
        crate::thumbnails::ICON_NO_PREVIEW,
    )
    .unwrap();
    assert!(!preview && near(pixel(&path, 10, 10), [0x80, 0, 0xc0, 255], 1));
    let _ = fs::remove_dir_all(&s.root);
}
