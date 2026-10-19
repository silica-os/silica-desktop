//! Installed apps: desktop entries (freedesktop Desktop Entry Specification) by id from the
//! data folders, and their icons from the icon themes (`hicolor`), `pixmaps` or a path,
//! decoded once (PNG through Skia's codecs, SVG through its SVG renderer).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use skia_safe::{Data, Image};

/// An app's desktop entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DesktopEntry {
    /// The id (the file name without `.desktop`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// `Icon`.
    pub icon: String,
    /// Hidden from menus (`NoDisplay`).
    pub no_display: bool,
    /// `X-Silica-Icon`: a Silica app's icon folder (a path) or a shell icon's name.
    pub silica_icon: String,
    /// `Categories`.
    pub categories: Vec<String>,
    /// `Keywords` and `GenericName`, for searching.
    pub keywords: Vec<String>,
    /// Not an app to show (`Hidden`, `Type` other than Application, `OnlyShowIn` others).
    pub hidden: bool,
}

/// The data folders: `XDG_DATA_HOME`, then `XDG_DATA_DIRS`.
pub fn data_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    out.extend(home);
    let dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    out.extend(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    out
}

/// Parses a desktop entry's `[Desktop Entry]` group.
pub fn parse(id: &str, text: &str) -> DesktopEntry {
    let mut e = DesktopEntry {
        id: id.to_string(),
        ..DesktopEntry::default()
    };
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_group {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k.trim() {
            "Name" => e.name = v.trim().to_string(),
            "Icon" => e.icon = v.trim().to_string(),
            "NoDisplay" => e.no_display = v.trim() == "true",
            "X-Silica-Icon" => e.silica_icon = v.trim().to_string(),
            "Categories" => e.categories = list(v),
            "Keywords" => e.keywords.extend(list(v)),
            "GenericName" => e.keywords.push(v.trim().to_string()),
            "Hidden" => e.hidden |= v.trim() == "true",
            "Type" => e.hidden |= v.trim() != "Application",
            "OnlyShowIn" => e.hidden |= !list(v).iter().any(|d| d == "Silica"),
            _ => {}
        }
    }
    if e.name.is_empty() {
        e.name = id.rsplit('.').next().unwrap_or(id).to_string();
    }
    e
}

fn list(v: &str) -> Vec<String> {
    v.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Every app to show, by name (the first data folder wins for an id).
pub fn all() -> Vec<DesktopEntry> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for d in data_dirs() {
        let Ok(rd) = std::fs::read_dir(d.join("applications")) else {
            continue;
        };
        for f in rd.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".desktop") else {
                continue;
            };
            if !seen.insert(id.to_string()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(f.path()) else {
                continue;
            };
            let e = parse(id, &text);
            if !e.no_display && !e.hidden {
                out.push(e);
            }
        }
    }
    out.sort_by_key(|e| e.name.to_lowercase());
    out
}

/// The desktop entry of app `id`, from the first data folder that has it.
pub fn entry(id: &str) -> Option<DesktopEntry> {
    if id.is_empty() || id.contains('/') {
        return None;
    }
    data_dirs().into_iter().find_map(|d| {
        let text =
            std::fs::read_to_string(d.join("applications").join(format!("{id}.desktop"))).ok()?;
        Some(parse(id, &text))
    })
}

/// The file of icon `name` (an absolute path, or a name in the themes).
pub fn icon_file(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let p = Path::new(name);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    if name.contains('/') {
        return None;
    }
    let sizes = [
        "512x512", "256x256", "scalable", "128x128", "96x96", "64x64", "48x48",
    ];
    for d in data_dirs() {
        for size in sizes {
            for ext in ["png", "svg"] {
                let f = d
                    .join("icons/hicolor")
                    .join(size)
                    .join("apps")
                    .join(format!("{name}.{ext}"));
                if f.is_file() {
                    return Some(f);
                }
            }
        }
        for ext in ["png", "svg"] {
            let f = d.join("pixmaps").join(format!("{name}.{ext}"));
            if f.is_file() {
                return Some(f);
            }
        }
    }
    None
}

/// The rendered PNG of a Silica icon (docs/assets-format.md "App icons"): `icon.png`, or
/// `icon-dark.png` in the dark appearance, in the app's icon folder (an absolute
/// `X-Silica-Icon`) or in the icons folder for a name (`SILICA_ICONS_DIR`, the data folders'
/// `silica/icons`, a silica-icons checkout in `~/silica`).
pub fn silica_icon_file(value: &str, dark: bool) -> Option<PathBuf> {
    if value.is_empty() || value.split('/').any(|p| p == "..") {
        return None;
    }
    let folders: Vec<PathBuf> = if value.starts_with('/') {
        vec![PathBuf::from(value)]
    } else if value.contains('/') {
        return None;
    } else {
        icon_roots()
            .into_iter()
            .map(|r| r.join("apps").join(value))
            .collect()
    };
    let names: &[&str] = if dark {
        &["icon-dark.png", "icon.png"]
    } else {
        &["icon.png"]
    };
    folders
        .iter()
        .flat_map(|f| names.iter().map(move |n| f.join(n)))
        .find(|p| p.is_file())
}

/// The folders holding Silica's icons, most important first: `SILICA_ICONS_DIR`
/// (development), then `silica/icons` in the data folders, then a silica-icons checkout in
/// `~/silica`.
fn icon_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("SILICA_ICONS_DIR").filter(|d| !d.is_empty()) {
        roots.push(PathBuf::from(d));
    }
    roots.extend(data_dirs().into_iter().map(|d| d.join("silica/icons")));
    if let Some(h) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(h).join("silica/silica-icons"));
    }
    roots
}

/// A shared Silica icon (`<category>/<name>/<file>`, e.g. `files/trash-full/icon.png`),
/// decoded and cached; with `dark`, the `-dark` rendering when there is one.
pub fn shared_icon(category: &str, name: &str, file: &str, dark: bool) -> Option<Image> {
    let dark_file = file.replace(".png", "-dark.png");
    let path = icon_roots().into_iter().find_map(|r| {
        let dir = r.join(category).join(name);
        [dark.then_some(dark_file.as_str()), Some(file)]
            .into_iter()
            .flatten()
            .map(|f| dir.join(f))
            .find(|p| p.is_file())
    })?;
    cached(&path)
}

/// The silica-icons name of one of Silica's own apps (`dev.frascella.TextEditor` →
/// `text-editor`), for entries installed without `X-Silica-Icon`.
pub fn own_icon_name(id: &str) -> Option<String> {
    let last = id.strip_prefix("dev.frascella.")?;
    let mut out = String::new();
    for (i, ch) in last.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
    }
    (!out.is_empty()).then_some(out)
}

thread_local! {
    static ICONS: RefCell<HashMap<PathBuf, Option<Image>>> = RefCell::new(HashMap::new());
}

/// The size SVG icons are rendered at (device pixels; a magnified Dock icon at 2×).
const SVG_SIZE: i32 = 256;

/// The largest side an icon is kept at (a 128 pt magnified Dock icon at 2×): the 1024 px
/// renderings would otherwise take 4 MB each.
const MAX_ICON: i32 = 256;

/// `img` scaled down so that its longer side is at most `max` pixels (as it is when smaller).
pub fn downscale(img: Image, max: i32) -> Image {
    let (w, h) = (img.width(), img.height());
    if w <= max && h <= max || w <= 0 || h <= 0 {
        return img;
    }
    let k = max as f32 / w.max(h) as f32;
    let (nw, nh) = (
        ((w as f32 * k).round() as i32).max(1),
        ((h as f32 * k).round() as i32).max(1),
    );
    let Some(mut surface) = skia_safe::surfaces::raster_n32_premul((nw, nh)) else {
        return img;
    };
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);
    canvas.draw_image_rect_with_sampling_options(
        &img,
        None,
        skia_safe::Rect::from_wh(nw as f32, nh as f32),
        skia_safe::SamplingOptions::from(skia_safe::CubicResampler::mitchell()),
        &skia_safe::Paint::default(),
    );
    surface.image_snapshot()
}

fn decode(path: &Path) -> Option<Image> {
    let bytes = std::fs::read(path).ok()?;
    if path.extension().is_some_and(|e| e == "svg") {
        let dom = skia_safe::svg::Dom::from_bytes(&bytes, skia_safe::FontMgr::empty()).ok()?;
        let mut surface = skia_safe::surfaces::raster_n32_premul((SVG_SIZE, SVG_SIZE))?;
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::TRANSPARENT);
        let root = dom.root();
        let size = root.intrinsic_size();
        let (w, h) = if size.width > 0.0 && size.height > 0.0 {
            (size.width, size.height)
        } else {
            (SVG_SIZE as f32, SVG_SIZE as f32)
        };
        let s = SVG_SIZE as f32 / w.max(h);
        canvas.scale((s, s));
        let mut dom = dom;
        dom.set_container_size((w, h));
        dom.render(canvas);
        Some(surface.image_snapshot())
    } else {
        Image::from_encoded(Data::new_copy(&bytes)).map(|i| downscale(i, MAX_ICON))
    }
}

/// The decoded icon of app `entry` (cached per file): its Silica icon, else its theme icon.
pub fn icon(entry: &DesktopEntry, dark: bool) -> Option<Image> {
    let path = silica_icon_file(&entry.silica_icon, dark)
        .or_else(|| {
            // Silica's own apps always get their Silica icon, never the theme's.
            own_icon_name(&entry.id).and_then(|n| silica_icon_file(&n, dark))
        })
        .or_else(|| icon_file(&entry.icon))?;
    cached(&path)
}

/// The decoded icon `name` of the themes (or a path).
pub fn icon_named(name: &str) -> Option<Image> {
    let path = icon_file(name)?;
    cached(&path)
}

fn cached(path: &Path) -> Option<Image> {
    let path = path.to_path_buf();
    ICONS.with(|c| {
        c.borrow_mut()
            .entry(path.clone())
            .or_insert_with(|| decode(&path))
            .clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_parse() {
        let e = parse(
            "dev.frascella.Mail",
            "[Desktop Entry]\nName=Mail\nIcon=dev.frascella.Mail\nExec=mail %u\n[Desktop Action New]\nName=New\n",
        );
        assert_eq!(e.name, "Mail");
        assert_eq!(e.icon, "dev.frascella.Mail");
        assert!(!e.no_display);
        assert_eq!(parse("org.x.Thing", "").name, "Thing");
        assert_eq!(entry("../etc"), None);
        assert_eq!(
            own_icon_name("dev.frascella.TextEditor").as_deref(),
            Some("text-editor")
        );
        assert_eq!(own_icon_name("org.gnome.Nautilus"), None);
    }
}

#[cfg(test)]
mod svg_tests {
    #[test]
    fn svg_icons_decode() {
        let p =
            std::path::Path::new("/usr/share/icons/hicolor/scalable/apps/org.gnome.Nautilus.svg");
        if !p.is_file() {
            return;
        }
        let bytes = std::fs::read(p).unwrap();
        let dom = skia_safe::svg::Dom::from_bytes(&bytes, skia_safe::FontMgr::empty());
        assert!(dom.is_ok(), "{:?}", dom.err());
        let img = super::decode(p).unwrap();
        assert_eq!(img.width(), 256);
    }
}
