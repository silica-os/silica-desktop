//! The app that opens a type, and its Silica icon.
//!
//! The default handler is the first desktop id of `[Default Applications]` for the type in the
//! `mimeapps.list` files (XDG order), else of `[Added Associations]`, else the first desktop
//! entry (by file name) whose `MimeType=` lists the type. Only an app with a Silica icon gives
//! one: `X-Silica-Icon=` (an absolute icon folder, whose `icon.assets` is composed, or a plain
//! name of `silica.assets`), else an absolute `Icon=` PNG. Any other handler gives none and the
//! file is an unknown extension. An app may also draw its own documents: `X-Silica-Document-Icons=`
//! maps MIME types to `documents/<name>.svg` in its icon folder (docs/thumbnails.md).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where an app icon comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppIcon {
    /// An icon folder (`icon.assets`, else `icon.png`).
    Folder(PathBuf),
    /// A PNG file.
    File(PathBuf),
    /// An app icon of `silica.assets`.
    Assets(String),
}

/// The XDG directories, injected so tests need no environment.
#[derive(Clone, Debug, Default)]
pub struct Dirs {
    pub config_home: PathBuf,
    pub config_dirs: Vec<PathBuf>,
    pub data_home: PathBuf,
    pub data_dirs: Vec<PathBuf>,
}

impl Dirs {
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Dirs {
        let home = env("HOME").unwrap_or_default();
        let absolute = |key: &str| env(key).filter(|v| v.starts_with('/'));
        let list = |value: Option<String>, default: &str| -> Vec<PathBuf> {
            value
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_owned())
                .split(':')
                .filter(|d| d.starts_with('/'))
                .map(PathBuf::from)
                .collect()
        };
        Dirs {
            config_home: absolute("XDG_CONFIG_HOME")
                .map_or_else(|| PathBuf::from(format!("{home}/.config")), PathBuf::from),
            config_dirs: list(env("XDG_CONFIG_DIRS"), "/etc/xdg"),
            data_home: absolute("XDG_DATA_HOME").map_or_else(
                || PathBuf::from(format!("{home}/.local/share")),
                PathBuf::from,
            ),
            data_dirs: list(env("XDG_DATA_DIRS"), "/usr/local/share:/usr/share"),
        }
    }

    fn data(&self) -> impl Iterator<Item = &PathBuf> {
        std::iter::once(&self.data_home).chain(&self.data_dirs)
    }

    /// The `mimeapps.list` files, most specific first.
    fn mimeapps(&self) -> Vec<PathBuf> {
        let mut files = vec![self.config_home.join("mimeapps.list")];
        files.extend(self.config_dirs.iter().map(|d| d.join("mimeapps.list")));
        files.extend(self.data().map(|d| d.join("applications/mimeapps.list")));
        files
    }
}

/// The ids listed for `mime` in a section of a `mimeapps.list` text.
pub fn listed(text: &str, section: &str, mime: &str) -> Vec<String> {
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = name == section;
        } else if inside
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == mime
        {
            return value
                .split(';')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect();
        }
    }
    Vec::new()
}

/// The `[Desktop Entry]` keys we use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub silica_icon: Option<String>,
    /// `X-Silica-Document-Icons=`: (MIME type or `major/*`, document icon name).
    pub document_icons: Vec<(String, String)>,
    pub icon: Option<String>,
    pub mime_types: Vec<String>,
}

pub fn parse_entry(text: &str) -> Entry {
    let mut entry = Entry::default();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
        } else if inside && let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "X-Silica-Icon" => entry.silica_icon = Some(value.trim().to_owned()),
                "X-Silica-Document-Icons" => entry.document_icons = document_icons(value),
                "Icon" => entry.icon = Some(value.trim().to_owned()),
                "MimeType" => {
                    entry.mime_types = value
                        .split(';')
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                _ => {}
            }
        }
    }
    entry
}

/// The names of a document icon: letters, digits, `.`, `_`, `-`, not starting with a dot.
pub fn valid_document_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Parses `mime/type=name;major/*=name;`. Invalid items are dropped.
fn document_icons(value: &str) -> Vec<(String, String)> {
    value
        .split(';')
        .filter_map(|item| {
            let (mime, name) = item.split_once('=')?;
            let (mime, name) = (mime.trim(), name.trim());
            (mime.contains('/') && valid_document_name(name))
                .then(|| (mime.to_owned(), name.to_owned()))
        })
        .collect()
}

/// The Silica icon an entry declares, if any.
pub fn icon_of(entry: &Entry) -> Option<AppIcon> {
    if let Some(value) = &entry.silica_icon {
        return if value.starts_with('/') {
            Some(AppIcon::Folder(PathBuf::from(value)))
        } else if value.is_empty() {
            None
        } else {
            Some(AppIcon::Assets(value.clone()))
        };
    }
    entry
        .icon
        .as_ref()
        .filter(|i| i.starts_with('/') && i.ends_with(".png"))
        .map(|i| AppIcon::File(PathBuf::from(i)))
}

/// Finds and reads desktop entries.
pub struct Apps {
    dirs: Dirs,
    /// The desktop entry files by id, read once: (id, entry), sorted by id.
    entries: Vec<(String, Entry)>,
    mimeapps: Vec<String>,
}

impl Apps {
    pub fn load(dirs: Dirs) -> Apps {
        let mut by_id: HashMap<String, Entry> = HashMap::new();
        // The user's entries win over the system's with the same id.
        let roots: Vec<PathBuf> = dirs.data().map(|d| d.join("applications")).collect();
        for root in roots.iter().rev() {
            collect(root, root, &mut by_id);
        }
        let mut entries: Vec<_> = by_id.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mimeapps = dirs
            .mimeapps()
            .iter()
            .filter_map(|p| fs::read_to_string(p).ok())
            .collect();
        Apps {
            dirs,
            entries,
            mimeapps,
        }
    }

    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }

    fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries
            .binary_search_by(|(k, _)| k.as_str().cmp(id))
            .ok()
            .and_then(|i| self.entries.get(i))
            .map(|(_, e)| e)
    }

    /// The absolute icon folder (`X-Silica-Icon=`) of the entry `id` (with `.desktop`).
    pub fn silica_icon_dir(&self, id: &str) -> Option<PathBuf> {
        self.entry(id)?
            .silica_icon
            .as_deref()
            .filter(|v| v.starts_with('/'))
            .map(PathBuf::from)
    }

    /// The desktop id of the app that opens `mime`.
    pub fn handler(&self, mime: &str) -> Option<&str> {
        for section in ["Default Applications", "Added Associations"] {
            for text in &self.mimeapps {
                for id in listed(text, section, mime) {
                    if let Some((key, _)) = self.entries.iter().find(|(k, _)| *k == id) {
                        return Some(key.as_str());
                    }
                }
            }
        }
        self.entries
            .iter()
            .find(|(_, e)| e.mime_types.iter().any(|m| m == mime))
            .map(|(id, _)| id.as_str())
    }

    /// The desktop id of the app that opens the first type of `chain` (the type and its
    /// parents) that has a handler.
    pub fn handler_for(&self, chain: &[String]) -> Option<&str> {
        chain.iter().find_map(|m| self.handler(m))
    }

    /// The Silica icon of the app that opens `mime`: the first type of `chain` (the type and
    /// its parents) that has a handler decides, and the handler needs a Silica icon.
    pub fn icon_for(&self, chain: &[String]) -> Option<AppIcon> {
        icon_of(self.entry(self.handler_for(chain)?)?)
    }

    /// The document foreground the app `id` declares for the first type of `chain` it lists
    /// (exact type, then `major/*`): `<icon folder>/documents/<name>.svg`, if the file is there.
    pub fn document_icon(&self, id: &str, chain: &[String]) -> Option<PathBuf> {
        let entry = self.entry(id)?;
        let dir = self.silica_icon_dir(id)?;
        let find = |key: &str| {
            entry
                .document_icons
                .iter()
                .find(|(m, _)| m == key)
                .map(|(_, n)| n.as_str())
        };
        let name = chain.iter().find_map(|mime| {
            find(mime).or_else(|| {
                let (major, _) = mime.split_once('/')?;
                find(&format!("{major}/*"))
            })
        })?;
        let path = dir.join("documents").join(format!("{name}.svg"));
        path.is_file().then_some(path)
    }
}

fn collect(root: &Path, dir: &Path, out: &mut HashMap<String, Entry>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for item in read.flatten() {
        let path = item.path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "desktop")
            && let (Ok(rel), Ok(text)) = (path.strip_prefix(root), fs::read_to_string(&path))
        {
            // `a/b.desktop` has the id `a-b.desktop`.
            let id = rel.to_string_lossy().replace('/', "-");
            out.insert(id, parse_entry(&text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "[Added Associations]\n\
        text/plain=editor.desktop;other.desktop;\n\
        [Default Applications]\n\
        application/pdf=preview.desktop;\n\
        text/html=firefox.desktop;\n";

    #[test]
    fn mimeapps_sections() {
        assert_eq!(
            listed(LIST, "Default Applications", "application/pdf"),
            ["preview.desktop"]
        );
        assert_eq!(
            listed(LIST, "Added Associations", "text/plain"),
            ["editor.desktop", "other.desktop"]
        );
        assert!(listed(LIST, "Default Applications", "text/plain").is_empty());
        assert!(listed(LIST, "Removed Associations", "text/plain").is_empty());
    }

    #[test]
    fn entries_declare_a_silica_icon_or_not() {
        let e = parse_entry(
            "[Desktop Entry]\nName=P\nX-Silica-Icon=/usr/lib64/silica/apps/p/icon\nIcon=/x.png\nMimeType=a/b;c/d;\n[Desktop Action x]\nIcon=no\n",
        );
        assert_eq!(e.mime_types, ["a/b", "c/d"]);
        assert_eq!(
            icon_of(&e),
            Some(AppIcon::Folder(PathBuf::from(
                "/usr/lib64/silica/apps/p/icon"
            )))
        );
        let named = parse_entry("[Desktop Entry]\nX-Silica-Icon=mission-control\n");
        assert_eq!(
            icon_of(&named),
            Some(AppIcon::Assets("mission-control".into()))
        );
        let plain = parse_entry("[Desktop Entry]\nIcon=/opt/app.png\n");
        assert_eq!(
            icon_of(&plain),
            Some(AppIcon::File(PathBuf::from("/opt/app.png")))
        );
        // A theme icon name is not a Silica icon.
        assert_eq!(
            icon_of(&parse_entry("[Desktop Entry]\nIcon=firefox\n")),
            None
        );
        assert_eq!(
            icon_of(&parse_entry("[Desktop Entry]\nX-Silica-Icon=\n")),
            None
        );
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apps-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_default_handler_decides_and_needs_a_silica_icon() {
        let root = tmp("handler");
        let apps = root.join("data/applications");
        fs::create_dir_all(&apps).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        let write = |name: &str, text: &str| fs::write(apps.join(name), text).unwrap();
        write(
            "preview.desktop",
            "[Desktop Entry]\nX-Silica-Icon=/apps/preview/icon\nMimeType=application/pdf;\n",
        );
        write(
            "evince.desktop",
            "[Desktop Entry]\nIcon=evince\nMimeType=application/pdf;\n",
        );
        write(
            "editor.desktop",
            "[Desktop Entry]\nX-Silica-Icon=/apps/editor/icon\nMimeType=text/plain;\n",
        );
        write(
            "firefox.desktop",
            "[Desktop Entry]\nIcon=firefox\nMimeType=text/html;\n",
        );
        write(
            "mpv.desktop",
            "[Desktop Entry]\nX-Silica-Icon=/apps/mpv/icon\nMimeType=video/mp4;\n",
        );
        fs::write(
            root.join("config/mimeapps.list"),
            "[Default Applications]\napplication/pdf=evince.desktop;preview.desktop;\n\
             text/html=gone.desktop;firefox.desktop;\n",
        )
        .unwrap();
        let dirs = Dirs {
            config_home: root.join("config"),
            config_dirs: vec![],
            data_home: root.join("data"),
            data_dirs: vec![],
        };
        let apps = Apps::load(dirs);
        let chain = |m: &str| vec![m.to_owned()];
        // The listed default is evince, which has no Silica icon: the file keeps its symbol.
        assert_eq!(apps.handler("application/pdf"), Some("evince.desktop"));
        assert_eq!(apps.icon_for(&chain("application/pdf")), None);
        // A missing first id is skipped; Firefox has no Silica icon either.
        assert_eq!(apps.handler("text/html"), Some("firefox.desktop"));
        assert_eq!(apps.icon_for(&chain("text/html")), None);
        // Without mimeapps.list the desktop entries decide.
        assert_eq!(
            apps.icon_for(&chain("text/plain")),
            Some(AppIcon::Folder(PathBuf::from("/apps/editor/icon")))
        );
        assert_eq!(
            apps.icon_for(&chain("video/mp4")),
            Some(AppIcon::Folder(PathBuf::from("/apps/mpv/icon")))
        );
        // The parents are tried in order.
        let chain = vec!["text/x-notes".to_owned(), "text/plain".to_owned()];
        assert!(apps.icon_for(&chain).is_some());
        assert_eq!(apps.icon_for(&["image/png".to_owned()]), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn document_icons_are_parsed_and_matched_by_type_then_wildcard() {
        let e = parse_entry(
            "[Desktop Entry]\nX-Silica-Document-Icons=application/pdf=pdf;text/*=text;bad=x;text/y=../up;image/png=;\n",
        );
        assert_eq!(
            e.document_icons,
            [
                ("application/pdf".to_owned(), "pdf".to_owned()),
                ("text/*".to_owned(), "text".to_owned())
            ]
        );
        let root = tmp("docicons");
        let apps_dir = root.join("data/applications");
        let icon = root.join("app/icon");
        fs::create_dir_all(&apps_dir).unwrap();
        fs::create_dir_all(icon.join("documents")).unwrap();
        fs::write(icon.join("documents/pdf.svg"), "<svg/>").unwrap();
        fs::write(icon.join("documents/text.svg"), "<svg/>").unwrap();
        fs::write(
            apps_dir.join("p.desktop"),
            format!(
                "[Desktop Entry]\nX-Silica-Icon={}\nMimeType=application/pdf;text/plain;\n\
                 X-Silica-Document-Icons=application/pdf=pdf;text/*=text;image/png=missing;\n",
                icon.display()
            ),
        )
        .unwrap();
        let apps = Apps::load(Dirs {
            config_home: root.join("config"),
            config_dirs: vec![],
            data_home: root.join("data"),
            data_dirs: vec![],
        });
        let chain = |m: &str| vec![m.to_owned()];
        assert_eq!(
            apps.document_icon("p.desktop", &chain("application/pdf")),
            Some(icon.join("documents/pdf.svg"))
        );
        // The wildcard matches the types of its group.
        assert_eq!(
            apps.document_icon("p.desktop", &chain("text/markdown")),
            Some(icon.join("documents/text.svg"))
        );
        // A parent in the chain counts; a name whose file is missing, or no entry, gives none.
        let md = vec!["x/y".to_owned(), "application/pdf".to_owned()];
        assert!(apps.document_icon("p.desktop", &md).is_some());
        assert_eq!(apps.document_icon("p.desktop", &chain("image/png")), None);
        assert_eq!(apps.document_icon("p.desktop", &chain("audio/mp3")), None);
        assert_eq!(
            apps.document_icon("none.desktop", &chain("text/plain")),
            None
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_xdg_dirs_have_defaults() {
        let env = |k: &str| (k == "HOME").then(|| "/h".to_owned());
        let d = Dirs::from_env(&env);
        assert_eq!(d.config_home, PathBuf::from("/h/.config"));
        assert_eq!(d.data_home, PathBuf::from("/h/.local/share"));
        assert_eq!(
            d.data_dirs,
            [
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share")
            ]
        );
        assert_eq!(d.config_dirs, [PathBuf::from("/etc/xdg")]);
    }
}
