//! The shared-mime-info database (names to types, aliases, parents) and a few magic numbers.
//! It only finds the app that opens a file and the text of its extension label: a file has no
//! kind, category or symbol of its own.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
}
