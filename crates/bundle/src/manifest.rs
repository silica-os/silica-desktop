//! The manifest of a bundle: every entry of `Contents/` but `_Signature/`, with its type,
//! permissions and SHA-256 (silica-os `docs/app-bundles.md` § 4).

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::SIGNATURE_DIR;

const HEADER: &str = "silica-manifest 1";
/// Entries beyond this are refused (a bundle is not a whole disk).
const MAX_ENTRIES: usize = 500_000;

/// What an entry is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    File {
        mode: u32,
        sha256: String,
    },
    Dir {
        mode: u32,
    },
    /// The SHA-256 of the link's target text.
    Link {
        sha256: String,
    },
}

/// An entry of the manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Relative to `Contents/`, with `/` separators.
    pub path: String,
    pub kind: Kind,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(buf.get(..n).unwrap_or_default());
    }
    Ok(hex(&hasher.finalize()))
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Whether a path component is acceptable in a bundle.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['\n', '\r', '\\']) && name != "." && name != ".."
}

/// Whether the link at `rel` (relative to `Contents/`) pointing at `target` stays inside.
fn link_inside(rel: &str, target: &Path) -> bool {
    if target.is_absolute() {
        return false;
    }
    let mut depth: i64 = rel.matches('/').count() as i64;
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir => depth -= 1,
            Component::CurDir => {}
            _ => return false,
        }
        if depth < 0 {
            return false;
        }
    }
    true
}

/// Reads the entries of `contents` (a bundle's `Contents/`), sorted by path bytes.
pub fn scan(contents: &Path) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut stack: Vec<PathBuf> = vec![PathBuf::new()];
    while let Some(rel_dir) = stack.pop() {
        let dir = contents.join(&rel_dir);
        let read = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for item in read {
            let item = item.map_err(|e| format!("{}: {e}", dir.display()))?;
            let name = item.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| format!("{}: a name that is not UTF-8", dir.display()))?;
            if rel_dir.as_os_str().is_empty() && name == SIGNATURE_DIR {
                continue;
            }
            if !valid_name(name) {
                return Err(format!("{}: invalid name {name:?}", dir.display()));
            }
            let rel = rel_dir.join(name);
            let rel_text = rel.to_string_lossy().into_owned();
            let path = contents.join(&rel);
            let meta =
                fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let mode = meta.permissions().mode() & 0o7777;
            let kind = if meta.file_type().is_symlink() {
                let target =
                    fs::read_link(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                if !link_inside(&rel_text, &target) {
                    return Err(format!(
                        "{rel_text}: a link that points outside the bundle ({})",
                        target.display()
                    ));
                }
                Kind::Link {
                    sha256: sha256(target.as_os_str().as_encoded_bytes()),
                }
            } else if meta.is_dir() {
                stack.push(rel.clone());
                Kind::Dir { mode }
            } else if meta.is_file() {
                Kind::File {
                    mode,
                    sha256: sha256_file(&path).map_err(|e| format!("{}: {e}", path.display()))?,
                }
            } else {
                return Err(format!("{rel_text}: not a file, a folder or a link"));
            };
            entries.push(Entry {
                path: rel_text,
                kind,
            });
            if entries.len() > MAX_ENTRIES {
                return Err(format!("more than {MAX_ENTRIES} entries"));
            }
        }
    }
    entries.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    Ok(entries)
}

/// The manifest text of the entries.
pub fn render(entries: &[Entry]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for e in entries {
        let line = match &e.kind {
            Kind::File { mode, sha256 } => format!("f {mode:04o} {sha256} {}", e.path),
            Kind::Dir { mode } => format!("d {mode:04o} - {}", e.path),
            Kind::Link { sha256 } => format!("l - {sha256} {}", e.path),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Parses a manifest; the entries must be sorted and unique.
pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err("not a silica manifest".to_owned());
    }
    let mut entries: Vec<Entry> = Vec::new();
    for line in lines {
        let mut parts = line.splitn(4, ' ');
        let (Some(kind), Some(mode), Some(hash), Some(path)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(format!("bad manifest line: {line}"));
        };
        if path.is_empty() || path.starts_with('/') || path.split('/').any(|c| !valid_name(c)) {
            return Err(format!("bad path in the manifest: {path}"));
        }
        let parse_mode = |m: &str| {
            u32::from_str_radix(m, 8)
                .ok()
                .filter(|m| *m <= 0o7777)
                .ok_or_else(|| format!("bad mode: {line}"))
        };
        let valid_hash = |h: &str| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit());
        let kind = match kind {
            "f" if valid_hash(hash) => Kind::File {
                mode: parse_mode(mode)?,
                sha256: hash.to_owned(),
            },
            "d" if hash == "-" => Kind::Dir {
                mode: parse_mode(mode)?,
            },
            "l" if mode == "-" && valid_hash(hash) => Kind::Link {
                sha256: hash.to_owned(),
            },
            _ => return Err(format!("bad manifest line: {line}")),
        };
        if let Some(last) = entries.last()
            && last.path.as_bytes() >= path.as_bytes()
        {
            return Err(format!("manifest not sorted at {path}"));
        }
        entries.push(Entry {
            path: path.to_owned(),
            kind,
        });
        if entries.len() > MAX_ENTRIES {
            return Err(format!("more than {MAX_ENTRIES} entries"));
        }
    }
    Ok(entries)
}

/// Compares the manifest with the folder: the first difference, or Ok.
pub fn check(contents: &Path, expected: &[Entry]) -> Result<(), String> {
    let actual = scan(contents)?;
    let mut a = actual.iter().peekable();
    let mut e = expected.iter().peekable();
    loop {
        match (a.peek(), e.peek()) {
            (None, None) => return Ok(()),
            (Some(x), None) => return Err(format!("{}: not in the manifest", x.path)),
            (None, Some(y)) => return Err(format!("{}: missing", y.path)),
            (Some(x), Some(y)) => match x.path.as_bytes().cmp(y.path.as_bytes()) {
                std::cmp::Ordering::Less => {
                    return Err(format!("{}: not in the manifest", x.path));
                }
                std::cmp::Ordering::Greater => return Err(format!("{}: missing", y.path)),
                std::cmp::Ordering::Equal => {
                    if x.kind != y.kind {
                        return Err(format!("{}: modified", x.path));
                    }
                    a.next();
                    e.next();
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    pub fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("manifest-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn contents(root: &Path) -> PathBuf {
        let c = root.join("Hello.app/Contents");
        fs::create_dir_all(c.join("bin")).unwrap();
        fs::create_dir_all(c.join("_Signature")).unwrap();
        fs::write(c.join("Info.toml"), "id").unwrap();
        fs::write(c.join("bin/hello"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(c.join("bin/hello"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(c.join("_Signature/manifest"), "ignored").unwrap();
        symlink("hello", c.join("bin/hi")).unwrap();
        c
    }

    #[test]
    fn round_trip_and_tampering() {
        let root = tmp("rt");
        let c = contents(&root);
        let entries = scan(&c).unwrap();
        let text = render(&entries);
        assert!(text.starts_with("silica-manifest 1\nf 06"));
        assert!(text.contains(" bin/hello\n"));
        assert!(text.contains("l - "));
        assert!(!text.contains("_Signature"));
        assert_eq!(parse(&text).unwrap(), entries);
        assert_eq!(check(&c, &entries), Ok(()));
        // A changed file, an extra file, a missing file, a changed mode.
        fs::write(c.join("bin/hello"), "#!/bin/sh\nevil\n").unwrap();
        assert_eq!(check(&c, &entries), Err("bin/hello: modified".to_owned()));
        fs::write(c.join("bin/hello"), "#!/bin/sh\n").unwrap();
        fs::write(c.join("bin/extra"), "x").unwrap();
        assert_eq!(
            check(&c, &entries),
            Err("bin/extra: not in the manifest".to_owned())
        );
        fs::remove_file(c.join("bin/extra")).unwrap();
        fs::set_permissions(c.join("bin/hello"), fs::Permissions::from_mode(0o4755)).unwrap();
        assert_eq!(check(&c, &entries), Err("bin/hello: modified".to_owned()));
        fs::set_permissions(c.join("bin/hello"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_file(c.join("Info.toml")).unwrap();
        assert_eq!(check(&c, &entries), Err("Info.toml: missing".to_owned()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn links_out_and_bad_manifests_are_refused() {
        let root = tmp("bad");
        let c = contents(&root);
        symlink("../../../etc/passwd", c.join("bin/out")).unwrap();
        assert!(scan(&c).unwrap_err().contains("outside"));
        fs::remove_file(c.join("bin/out")).unwrap();
        symlink("/etc/passwd", c.join("abs")).unwrap();
        assert!(scan(&c).is_err());
        fs::remove_file(c.join("abs")).unwrap();
        symlink("../Info.toml", c.join("bin/up")).unwrap();
        assert!(scan(&c).is_ok());
        assert!(parse("other\n").is_err());
        assert!(parse("silica-manifest 1\nf 0644 xyz a\n").is_err());
        assert!(parse("silica-manifest 1\nd 0755 - b\nd 0755 - a\n").is_err());
        assert!(parse("silica-manifest 1\nd 0755 - ../a\n").is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
