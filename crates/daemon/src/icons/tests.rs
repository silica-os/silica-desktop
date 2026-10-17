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

    let rules: toml::Table = silica_assets::compile::DEFAULT_COMPOSE.parse().unwrap();
    let mut glyphs = Vec::new();
    for kind in rules["file"]["kinds"].as_table().unwrap().values() {
        glyphs.push(kind["glyph"].as_str().unwrap().to_owned());
    }
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
    let root = tmp(name);
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    // The type database: names to types, and a text parent.
    write(
        &home.join(".local/share/mime/globs2"),
        "50:application/pdf:*.pdf\n50:text/plain:*.txt\n50:text/csv:*.csv\n50:application/x-blender:*.blend\n\
         50:application/json:*.json\n",
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
    let icons = Icons::with_assets(assets(&root), &env, root.join("cache")).unwrap();
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
    let tinted =
        compose::render(&s.icons.assets, s.icons.folder_entry, &plan, None, &fonts).unwrap();
    let plain = rules::plan_folder(
        rules,
        256,
        128.0,
        &FolderLook {
            tint: None,
            ..look.clone()
        },
    );
    let plain =
        compose::render(&s.icons.assets, s.icons.folder_entry, &plain, None, &fonts).unwrap();
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

#[test]
fn a_file_without_an_app_shows_the_symbol_of_its_kind() {
    let s = setup("symbol");
    let pdf = s.root.join("a.pdf");
    fs::write(&pdf, "x").unwrap();
    let out = s.icons.icon(&pdf, 512, 256.0).unwrap();
    // The sheet, and at the center of the glyph box the red of the kind.
    assert!(near(pixel(&out, 100, 300), [0xf0, 0xf0, 0xf0, 255], 1));
    assert!(near(pixel(&out, 256, 256), [0xe2, 0x33, 0x2b, 255], 1));
    // Same look, same file: a second PDF shares the cache entry.
    let again = s.root.join("b.pdf");
    fs::write(&again, "y").unwrap();
    assert_eq!(s.icons.icon(&again, 512, 256.0), Some(out.clone()));
    // The label "PDF" sits in the label box when Inter is there (here DejaVu stands in).
    if Path::new(DEJAVU).is_file() {
        let img = image::open(&out).unwrap().to_rgba8();
        let red_in_label = (0..512u32)
            .flat_map(|x| (382..450).map(move |y| (x, y)))
            .filter(|&(x, y)| near(img.get_pixel(x, y).0, [0xe2, 0x33, 0x2b, 255], 8))
            .count();
        assert!(red_in_label > 100, "{red_in_label}");
        // Below 32 pt the label is gone.
        let small = s.icons.icon(&pdf, 48, 24.0).unwrap();
        let img = image::open(&small).unwrap().to_rgba8();
        let label_rows = (36..45u32)
            .flat_map(|y| (0..48u32).map(move |x| (x, y)))
            .filter(|&(x, y)| near(img.get_pixel(x, y).0, [0xe2, 0x33, 0x2b, 255], 8))
            .count();
        assert_eq!(label_rows, 0);
    }
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn an_unknown_file_with_a_short_extension_gets_the_label_alone() {
    let s = setup("alone");
    let blend = s.root.join("scene.blend");
    fs::write(&blend, "x").unwrap();
    let look = s.icons.file_look(&blend, 256.0);
    assert_eq!(look.glyph, Glyph::None);
    assert_eq!(look.extension.as_deref(), Some("blend"));
    // Nothing in the middle of the sheet but the label's row: the glyph box is empty.
    let out = s.icons.icon(&blend, 512, 256.0).unwrap();
    assert!(near(pixel(&out, 256, 160), [0xf0, 0xf0, 0xf0, 255], 1));
    // A name with a long extension and one without: the fallback kind's symbol (a question).
    let long = s.root.join("data.sqlite3x");
    fs::write(&long, "x").unwrap();
    let out = s.icons.icon(&long, 512, 256.0).unwrap();
    assert!(near(pixel(&out, 256, 256), [0x8e, 0x8e, 0x93, 255], 1));
    let _ = fs::remove_dir_all(&s.root);
}

#[test]
fn the_default_app_icon_replaces_the_symbol_and_is_fitted_to_its_body() {
    let s = setup("app");
    let apps = s.root.join("home/.local/share/applications");
    let app_dir = s.root.join("editor-icon");
    write(&app_dir.join("icon.png"), png(1024, [0, 200, 0, 255]));
    write(
        &apps.join("editor.desktop"),
        format!(
            "[Desktop Entry]\nName=Editor\nX-Silica-Icon={}\nMimeType=text/plain;\n",
            app_dir.display()
        ),
    );
    // Applications are read at load: build a second service over the same home.
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
        assert!(near(pixel(&out, 120, 120), [0xf0, 0xf0, 0xf0, 255], 1));
        assert!(near(pixel(&out, 395, 395), [0xf0, 0xf0, 0xf0, 255], 1));
    }
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

#[test]
fn files_resolve_to_kinds_and_categories_through_the_compiled_map() {
    let s = setup("resolve");
    let r = |mime: Option<&str>, ext: Option<&str>| {
        mime::resolve(&s.icons.assets, &s.icons.mime, mime, ext)
    };
    let got = r(Some("application/pdf"), Some("pdf"));
    assert_eq!(
        (got.kind.as_deref(), got.category.as_str()),
        (Some("pdf"), "document")
    );
    // The parent decides when the type has no entry of its own.
    let got = r(Some("text/csv"), Some("csv"));
    assert_eq!(got.kind.as_deref(), Some("spreadsheet"));
    assert_eq!(got.category, "data");
    // An explicit category entry beats the text parent: JSON is data, not text.
    let got = r(Some("application/json"), Some("json"));
    assert_eq!((got.kind, got.category.as_str()), (None, "data"));
    // An extension override gives the category of a file with no kind.
    let got = r(Some("application/x-blender"), Some("blend"));
    assert_eq!((got.kind, got.category.as_str()), (None, "media"));
    // Wildcard types, the extension alone, and nothing at all.
    assert_eq!(
        r(Some("image/x-unknown"), None).kind.as_deref(),
        Some("image")
    );
    assert_eq!(r(None, Some("rs")).kind.as_deref(), Some("code"));
    let got = r(None, None);
    assert_eq!((got.kind, got.category.as_str()), (None, "other"));
    let got = r(Some("application/x-nothing"), Some("zzz"));
    assert_eq!((got.kind, got.category.as_str()), (None, "other"));
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
