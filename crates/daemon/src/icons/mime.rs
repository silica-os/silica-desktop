//! The shared-mime-info database (names to types, aliases, parents) and the resolution of a
//! file to its kind or category with the map compiled in `silica.assets`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use silica_assets::{Assets, KeySpace};

/// The parts of shared-mime-info the resolution needs.
#[derive(Clone, Debug, Default)]
pub struct MimeDb {
    /// (weight, glob, type) from `globs2`.
    globs: Vec<(u32, String, String)>,
    aliases: HashMap<String, String>,
    parents: HashMap<String, Vec<String>>,
}

impl MimeDb {
    pub fn from_texts(globs2: &str, aliases: &str, subclasses: &str) -> MimeDb {
        let mut db = MimeDb::default();
        for line in globs2.lines().filter(|l| !l.starts_with('#')) {
            let mut parts = line.splitn(4, ':');
            if let (Some(w), Some(mime), Some(glob)) = (parts.next(), parts.next(), parts.next())
                && let Ok(weight) = w.parse()
            {
                db.globs.push((weight, glob.to_owned(), mime.to_owned()));
            }
        }
        for line in aliases.lines().filter(|l| !l.starts_with('#')) {
            if let Some((alias, canonical)) = line.split_once(' ') {
                db.aliases
                    .insert(alias.to_owned(), canonical.trim().to_owned());
            }
        }
        for line in subclasses.lines().filter(|l| !l.starts_with('#')) {
            if let Some((child, parent)) = line.split_once(' ') {
                db.parents
                    .entry(child.to_owned())
                    .or_default()
                    .push(parent.trim().to_owned());
            }
        }
        db
    }

    /// Loads `mime/{globs2,aliases,subclasses}` from the first of `dirs` that has them.
    pub fn load(dirs: &[PathBuf]) -> MimeDb {
        for dir in dirs {
            let read = |name: &str| fs::read_to_string(dir.join("mime").join(name)).ok();
            if let Some(globs) = read("globs2") {
                return MimeDb::from_texts(
                    &globs,
                    &read("aliases").unwrap_or_default(),
                    &read("subclasses").unwrap_or_default(),
                );
            }
        }
        MimeDb::default()
    }

    /// The canonical name of a type.
    pub fn canonical<'a>(&'a self, mime: &'a str) -> &'a str {
        self.aliases.get(mime).map_or(mime, String::as_str)
    }

    /// The type a file name suggests: the heaviest glob, then the longest. Handles literal
    /// names and `*.ext` suffix globs (also `*.tar.gz`), case-insensitively for suffixes.
    pub fn for_name(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase();
        let mut best: Option<(u32, usize, &str)> = None;
        for (weight, glob, mime) in &self.globs {
            let hit = match glob.strip_prefix('*') {
                Some(suffix) if !suffix.contains(['*', '?', '[']) => {
                    lower.ends_with(&suffix.to_lowercase())
                }
                Some(_) => false,
                None => !glob.contains(['*', '?', '[']) && glob == name,
            };
            if hit && best.is_none_or(|(w, l, _)| (*weight, glob.len()) > (w, l)) {
                best = Some((*weight, glob.len(), mime));
            }
        }
        best.map(|(_, _, mime)| mime)
    }

    /// The type and its ancestors, nearest first: `[type, parent, grandparent...]`.
    pub fn chain(&self, mime: &str) -> Vec<String> {
        let mut out = vec![self.canonical(mime).to_owned()];
        let mut at = 0;
        while let Some(current) = out.get(at).cloned() {
            for parent in self.parents.get(&current).into_iter().flatten() {
                let parent = self.canonical(parent).to_owned();
                if !out.contains(&parent) && out.len() < 32 {
                    out.push(parent);
                }
            }
            at += 1;
        }
        out
    }
}

/// The type of a file whose name says nothing: a few magic numbers.
pub fn sniff(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if head.starts_with(b"\x7fELF") {
        Some("application/x-pie-executable")
    } else if head.starts_with(b"#!") {
        Some("application/x-shellscript")
    } else {
        None
    }
}

/// The category of a type that the map does not know, by the rules of silica-icons.
pub fn mime_category(mime: &str) -> &'static str {
    let (major, minor) = mime.split_once('/').unwrap_or((mime, ""));
    let has = |needles: &[&str]| needles.iter().any(|n| minor.contains(n));
    match major {
        "image" | "audio" | "video" | "font" | "model" => "media",
        "text" if has(&["csv", "json", "xml", "yaml", "calendar", "vcard", "sql"]) => "data",
        "text"
            if has(&[
                "x-c",
                "rust",
                "python",
                "javascript",
                "java",
                "go",
                "script",
                "css",
            ]) =>
        {
            "code"
        }
        "text" => "document",
        "application" if has(&["json", "xml", "yaml", "toml", "sql", "sqlite", "database"]) => {
            "data"
        }
        "application"
            if has(&[
                "zip",
                "tar",
                "gzip",
                "bzip",
                "xz",
                "zstd",
                "rar",
                "7z",
                "compressed",
                "debian",
                "rpm",
                "java-archive",
                "flatpak",
            ]) =>
        {
            "archive"
        }
        "application"
            if has(&[
                "pdf",
                "msword",
                "rtf",
                "opendocument",
                "officedocument",
                "epub",
                "postscript",
                "abiword",
            ]) =>
        {
            "document"
        }
        "application" if has(&["javascript", "x-shellscript", "x-perl", "x-php", "x-ruby"]) => {
            "code"
        }
        _ => "other",
    }
}

/// The category a kind of file belongs to (for the color of a label next to an app icon).
pub fn kind_category(kind: &str) -> &'static str {
    match kind {
        "pdf" | "text" | "document" | "presentation" | "web" => "document",
        "spreadsheet" => "data",
        "code" | "executable" => "code",
        "archive" | "disk-image" => "archive",
        "image" | "audio" | "video" | "font" => "media",
        _ => "other",
    }
}

/// What the type map says about a file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    /// The kind (`pdf`), if the file has one.
    pub kind: Option<String>,
    /// The category (`data`): the map's, or the one the type suggests.
    pub category: String,
}

fn entry_name<'a>(assets: &'a Assets, space: KeySpace, key: &str) -> Option<&'a str> {
    Some(assets.entry(assets.lookup(space, key)?)?.name())
}

/// Resolves a file by the order of the icon format: the type and its parents in the map
/// (exact, then `major/*`), then the extension, otherwise no kind and the category of the
/// type. An override of the extension decides the category of a file with no kind.
pub fn resolve(
    assets: &Assets,
    db: &MimeDb,
    mime: Option<&str>,
    extension: Option<&str>,
) -> Resolved {
    let from_ext = extension.and_then(|e| entry_name(assets, KeySpace::Ext, e));
    let ext_category = from_ext.and_then(|n| n.strip_prefix("category."));
    let mut hit: Option<&str> = None;
    if let Some(mime) = mime {
        for candidate in db.chain(mime) {
            let wildcard = candidate
                .split_once('/')
                .map(|(major, _)| format!("{major}/*"));
            hit = entry_name(assets, KeySpace::Mime, &candidate).or_else(|| {
                wildcard
                    .as_deref()
                    .and_then(|w| entry_name(assets, KeySpace::Mime, w))
            });
            if hit.is_some() {
                break;
            }
        }
    }
    let by_mime = mime.map_or("other", |m| mime_category(db.canonical(m)));
    let hit = hit.or(from_ext);
    match hit {
        Some(name) => match name.strip_prefix("kind.") {
            Some(kind) => Resolved {
                kind: Some(kind.to_owned()),
                category: ext_category
                    .unwrap_or_else(|| kind_category(kind))
                    .to_owned(),
            },
            None => Resolved {
                kind: None,
                category: ext_category
                    .or_else(|| name.strip_prefix("category."))
                    .unwrap_or(by_mime)
                    .to_owned(),
            },
        },
        None => Resolved {
            kind: None,
            category: by_mime.to_owned(),
        },
    }
}

/// The directories whose `mime/` folder is searched: `$XDG_DATA_HOME`, `$XDG_DATA_DIRS`.
pub fn data_dirs(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = env("XDG_DATA_HOME")
        .filter(|v| v.starts_with('/'))
        .or_else(|| env("HOME").map(|h| format!("{h}/.local/share")));
    dirs.extend(home.map(PathBuf::from));
    let system = env("XDG_DATA_DIRS")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    dirs.extend(
        system
            .split(':')
            .filter(|d| d.starts_with('/'))
            .map(PathBuf::from),
    );
    dirs
}

/// The lowercase extension after the last dot, if the name has one that is not the whole name.
pub fn extension_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty() && !ext.contains(' ')).then(|| ext.to_lowercase())
}

/// Reads the first bytes of a file for [`sniff`].
pub fn read_head(path: &Path) -> Vec<u8> {
    use std::io::Read;
    let mut head = vec![0u8; 8];
    let n = fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    head.truncate(n);
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLOBS: &str = "# comment\n\
        50:application/pdf:*.pdf\n\
        50:text/plain:*.txt\n\
        50:application/gzip:*.gz\n\
        50:application/x-compressed-tar:*.tar.gz\n\
        50:text/x-makefile:Makefile\n\
        50:application/json:*.json\n\
        50:application/x-blender:*.blend\n";
    const ALIASES: &str = "text/xml application/xml\napplication/x-pdf application/pdf\n";
    const SUBCLASSES: &str = "application/json application/json5\n\
        application/json5 text/javascript\n\
        text/javascript text/plain\n\
        text/x-makefile text/plain\n\
        application/x-compressed-tar application/gzip\n";

    fn db() -> MimeDb {
        MimeDb::from_texts(GLOBS, ALIASES, SUBCLASSES)
    }

    #[test]
    fn names_aliases_and_parents() {
        let db = db();
        assert_eq!(db.for_name("Report.PDF"), Some("application/pdf"));
        assert_eq!(
            db.for_name("a.tar.gz"),
            Some("application/x-compressed-tar")
        );
        assert_eq!(db.for_name("a.gz"), Some("application/gzip"));
        assert_eq!(db.for_name("Makefile"), Some("text/x-makefile"));
        assert_eq!(db.for_name("makefile.txt"), Some("text/plain"));
        assert_eq!(db.for_name("noext"), None);
        assert_eq!(db.canonical("text/xml"), "application/xml");
        assert_eq!(
            db.chain("application/json"),
            [
                "application/json",
                "application/json5",
                "text/javascript",
                "text/plain"
            ]
        );
        assert_eq!(extension_of("a.tar.GZ").as_deref(), Some("gz"));
        assert_eq!(extension_of(".bashrc"), None);
        assert_eq!(extension_of("Makefile"), None);
        assert_eq!(extension_of("trailing."), None);
    }

    #[test]
    fn magic_numbers() {
        assert_eq!(sniff(b"%PDF-1.7"), Some("application/pdf"));
        assert_eq!(sniff(b"\x7fELF\x02"), Some("application/x-pie-executable"));
        assert_eq!(sniff(b"#!/bin/sh"), Some("application/x-shellscript"));
        assert_eq!(sniff(b"hello"), None);
    }

    #[test]
    fn categories_by_type() {
        assert_eq!(mime_category("text/csv"), "data");
        assert_eq!(mime_category("text/plain"), "document");
        assert_eq!(mime_category("application/zip"), "archive");
        assert_eq!(mime_category("application/x-blender"), "other");
        assert_eq!(mime_category("image/x-xcf"), "media");
        assert_eq!(mime_category("application/pdf"), "document");
        assert_eq!(kind_category("spreadsheet"), "data");
    }
}
