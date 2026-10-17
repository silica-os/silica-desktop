//! What the user chose for a folder (colour and symbol), where it is stored by Mustache and
//! the shell (`shell/lib/system/folder_style.dart`), and which special folder a path is.
//!
//! - `user.silica.folder`: JSON such as `{"tint":"#e8336d","symbol":"music-note"}` (an
//!   extended attribute; on file systems without them, the GIO metadata
//!   `metadata::silica-folder`);
//! - `user.xdg.tags`: comma-separated tags; the first one named like a colour tints the folder
//!   when the JSON has no tint.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rustix::fs::getxattr;
use rustix::io::Errno;

pub const STYLE_ATTRIBUTE: &str = "user.silica.folder";
pub const TAGS_ATTRIBUTE: &str = "user.xdg.tags";
const STYLE_METADATA: &str = "metadata::silica-folder";

/// The colour names of the tint labels and the tags that mean them.
const TAG_NAMES: [(&str, [&str; 2]); 7] = [
    ("red", ["Red", "Rosso"]),
    ("orange", ["Orange", "Arancio"]),
    ("yellow", ["Yellow", "Giallo"]),
    ("green", ["Green", "Verde"]),
    ("blue", ["Blue", "Blu"]),
    ("purple", ["Purple", "Viola"]),
    ("gray", ["Gray", "Grigio"]),
];

/// A tint as stored: `#rrggbb` or a label name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tint {
    Hex([u8; 3]),
    Label(String),
}

/// The stored style of a folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub tint: Option<Tint>,
    pub symbol: Option<String>,
}

fn valid_symbol(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// The stored JSON; malformed fields are dropped, `labels` are the valid label names.
pub fn parse_style(json: &str, labels: &dyn Fn(&str) -> bool) -> Style {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Style::default();
    };
    let tint = value.get("tint").and_then(|v| v.as_str()).and_then(|t| {
        parse_hex(t).map(Tint::Hex).or_else(|| {
            let lower = t.to_lowercase();
            labels(&lower).then_some(Tint::Label(lower))
        })
    });
    let symbol = value
        .get("symbol")
        .and_then(|v| v.as_str())
        .filter(|s| valid_symbol(s))
        .map(str::to_owned);
    Style { tint, symbol }
}

/// The label a tag names, in English or Italian.
pub fn tag_label(tag: &str) -> Option<&'static str> {
    TAG_NAMES
        .iter()
        .find(|(_, names)| names.contains(&tag))
        .map(|(label, _)| *label)
}

/// An attribute: its value, or `Err(true)` when the file system has no user attributes.
fn attribute(path: &Path, name: &str) -> Result<Option<String>, bool> {
    let mut buffer = vec![0u8; 4096];
    match getxattr(path, name, &mut buffer) {
        Ok(n) => {
            buffer.truncate(n);
            Ok(String::from_utf8(buffer).ok())
        }
        Err(Errno::NOTSUP) => Err(true),
        Err(_) => Ok(None),
    }
}

fn gio_metadata(path: &Path) -> Option<String> {
    let output = Command::new("gio")
        .args(["info", "-a", STYLE_METADATA])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let prefix = format!("{STYLE_METADATA}: ");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|l| l.trim().strip_prefix(&prefix).map(str::to_owned))
}

/// Reads the style of the folder at `path`: the JSON (attribute, else GIO metadata) and, for
/// the tint, the colour tags.
pub fn read_style(path: &Path, labels: &dyn Fn(&str) -> bool) -> Style {
    let json = match attribute(path, STYLE_ATTRIBUTE) {
        Ok(value) => value,
        Err(_) => gio_metadata(path),
    };
    let mut style = json.map(|j| parse_style(&j, labels)).unwrap_or_default();
    if style.tint.is_none()
        && let Ok(Some(tags)) = attribute(path, TAGS_ATTRIBUTE)
    {
        style.tint = tags
            .split(',')
            .find_map(|t| tag_label(t.trim()))
            .map(|l| Tint::Label(l.to_owned()));
    }
    style
}

/// Whether the folder has at least one item. An unreadable folder counts as empty.
pub fn has_items(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut d| d.next().is_some())
}

/// The value of a `XDG_<KEY>_DIR` line of `user-dirs.dirs`, `$HOME` expanded.
pub fn user_dirs(text: &str, home: &str) -> Vec<(String, PathBuf)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (key, value) = line.split_once('=')?;
            let key = key.strip_prefix("XDG_")?.strip_suffix("_DIR")?;
            let value = value.trim().trim_matches('"');
            let path = value.replace("$HOME", home);
            path.starts_with('/')
                .then(|| (key.to_owned(), PathBuf::from(path)))
        })
        .collect()
}

/// The special-folder key of `path` (`HOME`, `DOCUMENTS`, `APPLICATIONS`, `LIBRARY`...), if
/// any. `dirs` are the user dirs, `data_dirs` the XDG data folders.
pub fn special_key(
    path: &Path,
    home: &Path,
    dirs: &[(String, PathBuf)],
    data_home: &Path,
    data_dirs: &[PathBuf],
) -> Option<String> {
    if path == home {
        return Some("HOME".to_owned());
    }
    if let Some((key, _)) = dirs.iter().find(|(_, p)| p == path) {
        return Some(key.clone());
    }
    if path == home.join("Applications")
        || std::iter::once(data_home)
            .chain(data_dirs.iter().map(PathBuf::as_path))
            .any(|d| path == d.join("applications"))
    {
        return Some("APPLICATIONS".to_owned());
    }
    (path == data_home).then(|| "LIBRARY".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(name: &str) -> bool {
        ["red", "blue", "gray"].contains(&name)
    }

    #[test]
    fn the_stored_style_is_validated() {
        let s = parse_style(r##"{"tint":"#E8336D","symbol":"music-note"}"##, &labels);
        assert_eq!(s.tint, Some(Tint::Hex([0xE8, 0x33, 0x6D])));
        assert_eq!(s.symbol.as_deref(), Some("music-note"));
        let s = parse_style(r#"{"tint":"Blue","symbol":"../etc"}"#, &labels);
        assert_eq!(s.tint, Some(Tint::Label("blue".into())));
        assert_eq!(s.symbol, None);
        assert_eq!(
            parse_style(r#"{"tint":"mauve"}"#, &labels),
            Style::default()
        );
        assert_eq!(parse_style("not json", &labels), Style::default());
        assert_eq!(
            parse_style(r##"{"tint":"#12345"}"##, &labels),
            Style::default()
        );
        assert_eq!(parse_hex("#é12345"), None);
    }

    #[test]
    fn tags_name_colours_in_two_languages() {
        assert_eq!(tag_label("Rosso"), Some("red"));
        assert_eq!(tag_label("Green"), Some("green"));
        assert_eq!(tag_label("Work"), None);
    }

    #[test]
    fn special_folders() {
        let text = "# c\nXDG_DOCUMENTS_DIR=\"$HOME/Documenti\"\nXDG_DOWNLOAD_DIR=\"$HOME/Download\"\nXDG_BAD_DIR=\"rel\"\n";
        let dirs = user_dirs(text, "/h");
        assert_eq!(dirs.len(), 2);
        let data_home = PathBuf::from("/h/.local/share");
        let data_dirs = [PathBuf::from("/usr/share")];
        let key =
            |p: &str| special_key(Path::new(p), Path::new("/h"), &dirs, &data_home, &data_dirs);
        assert_eq!(key("/h").as_deref(), Some("HOME"));
        assert_eq!(key("/h/Documenti").as_deref(), Some("DOCUMENTS"));
        assert_eq!(key("/h/Download").as_deref(), Some("DOWNLOAD"));
        assert_eq!(key("/h/Applications").as_deref(), Some("APPLICATIONS"));
        assert_eq!(
            key("/usr/share/applications").as_deref(),
            Some("APPLICATIONS")
        );
        assert_eq!(key("/h/.local/share").as_deref(), Some("LIBRARY"));
        assert_eq!(key("/h/Documents"), None);
    }

    #[test]
    fn folders_with_items() {
        let dir = std::env::temp_dir().join(format!("folder-items-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(!has_items(&dir));
        fs::write(dir.join("a"), "x").unwrap();
        assert!(has_items(&dir));
        assert!(!has_items(&dir.join("missing")));
        let _ = fs::remove_dir_all(&dir);
    }
}
