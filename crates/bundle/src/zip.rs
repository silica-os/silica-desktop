//! `.silicaapp` archives: a zip of `Name.app` (deflate, Unix permissions and symbolic links in
//! the external attributes), for developers, CI and update downloads. Only what this writer
//! makes needs to be read back, but any plain zip of stored or deflated entries reads.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
const S_IFLNK: u32 = 0o120_000;
const S_IFDIR: u32 = 0o040_000;
const S_IFREG: u32 = 0o100_000;
/// The largest archive member read (no ZIP64).
const MAX_MEMBER: u64 = u32::MAX as u64;

struct Member {
    name: String,
    mode: u32,
    crc: u32,
    compressed: u64,
    size: u64,
    method: u16,
    offset: u64,
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// The entries under `root` (a bundle folder), with the bundle's name as the first component.
fn walk(root: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let mut items: Vec<_> = fs::read_dir(root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .flatten()
        .collect();
    items.sort_by_key(|e| e.file_name());
    for item in items {
        let name = item.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| format!("{}: a name that is not UTF-8", item.path().display()))?;
        let rel = format!("{prefix}/{name}");
        let path = item.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        out.push((rel.clone(), path.clone()));
        if meta.is_dir() {
            walk(&path, &rel, out)?;
        }
    }
    Ok(())
}

/// Writes `bundle` (a `Name.app` folder) into the archive `to`.
pub fn pack(bundle: &Path, to: &Path) -> Result<(), String> {
    let top = bundle
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("the bundle has no name")?
        .to_owned();
    let mut entries = vec![(top.clone(), bundle.to_path_buf())];
    walk(bundle, &top, &mut entries)?;
    let mut out: Vec<u8> = Vec::new();
    let mut members = Vec::new();
    for (name, path) in entries {
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let perm = meta.permissions().mode() & 0o7777;
        let (mode, data, name) = if meta.file_type().is_symlink() {
            let target = fs::read_link(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            (
                S_IFLNK | 0o777,
                target.as_os_str().as_encoded_bytes().to_vec(),
                name,
            )
        } else if meta.is_dir() {
            (S_IFDIR | perm, Vec::new(), format!("{name}/"))
        } else {
            let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            (S_IFREG | perm, data, name)
        };
        if data.len() as u64 > MAX_MEMBER {
            return Err(format!("{name}: larger than 4 GiB"));
        }
        let crc = crc32fast::hash(&data);
        let (method, stored) = if data.is_empty() || mode & 0o170_000 == S_IFLNK {
            (0u16, data.clone())
        } else {
            let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
            enc.write_all(&data).map_err(|e| e.to_string())?;
            let packed = enc.finish().map_err(|e| e.to_string())?;
            if packed.len() < data.len() {
                (8, packed)
            } else {
                (0, data.clone())
            }
        };
        let offset = out.len() as u64;
        if offset > MAX_MEMBER {
            return Err("the archive is larger than 4 GiB".to_owned());
        }
        put32(&mut out, LOCAL);
        put16(&mut out, 20);
        put16(&mut out, 0x0800); // UTF-8 names
        put16(&mut out, method);
        put16(&mut out, 0);
        put16(&mut out, 0x21); // 1980-01-01
        put32(&mut out, crc);
        put32(&mut out, u32::try_from(stored.len()).unwrap_or(u32::MAX));
        put32(&mut out, u32::try_from(data.len()).unwrap_or(u32::MAX));
        put16(
            &mut out,
            u16::try_from(name.len()).map_err(|_| "name too long")?,
        );
        put16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&stored);
        members.push(Member {
            name,
            mode,
            crc,
            compressed: stored.len() as u64,
            size: data.len() as u64,
            method,
            offset,
        });
    }
    let central_start = out.len() as u64;
    for m in &members {
        put32(&mut out, CENTRAL);
        put16(&mut out, (3 << 8) | 20); // made by Unix
        put16(&mut out, 20);
        put16(&mut out, 0x0800);
        put16(&mut out, m.method);
        put16(&mut out, 0);
        put16(&mut out, 0x21);
        put32(&mut out, m.crc);
        put32(&mut out, u32::try_from(m.compressed).unwrap_or(u32::MAX));
        put32(&mut out, u32::try_from(m.size).unwrap_or(u32::MAX));
        put16(&mut out, u16::try_from(m.name.len()).unwrap_or(u16::MAX));
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put32(&mut out, m.mode << 16);
        put32(&mut out, u32::try_from(m.offset).unwrap_or(u32::MAX));
        out.extend_from_slice(m.name.as_bytes());
    }
    let central_size = out.len() as u64 - central_start;
    put32(&mut out, END);
    put16(&mut out, 0);
    put16(&mut out, 0);
    let count = u16::try_from(members.len()).map_err(|_| "more than 65535 entries")?;
    put16(&mut out, count);
    put16(&mut out, count);
    put32(&mut out, u32::try_from(central_size).unwrap_or(u32::MAX));
    put32(
        &mut out,
        u32::try_from(central_start).map_err(|_| "archive too large")?,
    );
    put16(&mut out, 0);
    let tmp = to.with_extension("partial");
    fs::write(&tmp, &out).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, to).map_err(|e| format!("{}: {e}", to.display()))
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The members of an archive, from its central directory.
fn members(file: &mut File) -> Result<Vec<Member>, String> {
    let len = file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let tail_len = len.min(65_557);
    let mut tail = vec![0u8; usize::try_from(tail_len).map_err(|e| e.to_string())?];
    file.seek(SeekFrom::Start(len - tail_len))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut tail).map_err(|e| e.to_string())?;
    let end = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| u32_at(&tail, i) == Some(END))
        .ok_or("not a zip archive")?;
    let bad = || "damaged zip archive".to_owned();
    let count = usize::from(u16_at(&tail, end + 10).ok_or_else(bad)?);
    let size = u64::from(u32_at(&tail, end + 12).ok_or_else(bad)?);
    let start = u64::from(u32_at(&tail, end + 16).ok_or_else(bad)?);
    if start + size > len || size > 64 * 1024 * 1024 {
        return Err(bad());
    }
    let mut dir = vec![0u8; usize::try_from(size).map_err(|e| e.to_string())?];
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut dir).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(count);
    let mut at = 0usize;
    for _ in 0..count {
        if u32_at(&dir, at) != Some(CENTRAL) {
            return Err(bad());
        }
        let made_by = u16_at(&dir, at + 4).ok_or_else(bad)?;
        let method = u16_at(&dir, at + 10).ok_or_else(bad)?;
        let crc = u32_at(&dir, at + 16).ok_or_else(bad)?;
        let compressed = u64::from(u32_at(&dir, at + 20).ok_or_else(bad)?);
        let size = u64::from(u32_at(&dir, at + 24).ok_or_else(bad)?);
        let name_len = usize::from(u16_at(&dir, at + 28).ok_or_else(bad)?);
        let extra_len = usize::from(u16_at(&dir, at + 30).ok_or_else(bad)?);
        let comment_len = usize::from(u16_at(&dir, at + 32).ok_or_else(bad)?);
        let external = u32_at(&dir, at + 38).ok_or_else(bad)?;
        let offset = u64::from(u32_at(&dir, at + 42).ok_or_else(bad)?);
        let name = dir.get(at + 46..at + 46 + name_len).ok_or_else(bad)?;
        let name = String::from_utf8(name.to_vec()).map_err(|_| bad())?;
        let unix = made_by >> 8 == 3;
        let mode = if unix && external >> 16 != 0 {
            external >> 16
        } else if name.ends_with('/') {
            S_IFDIR | 0o755
        } else {
            S_IFREG | 0o644
        };
        out.push(Member {
            name,
            mode,
            crc,
            compressed,
            size,
            method,
            offset,
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

fn read_member(file: &mut File, m: &Member, limit: u64) -> Result<Vec<u8>, String> {
    let bad = || format!("{}: damaged", m.name);
    if m.size > limit || m.compressed > MAX_MEMBER {
        return Err(format!("{}: too large", m.name));
    }
    let mut head = [0u8; 30];
    file.seek(SeekFrom::Start(m.offset))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut head).map_err(|e| e.to_string())?;
    if u32_at(&head, 0) != Some(LOCAL) {
        return Err(bad());
    }
    let skip = u64::from(u16_at(&head, 26).ok_or_else(bad)?)
        + u64::from(u16_at(&head, 28).ok_or_else(bad)?);
    file.seek(SeekFrom::Start(m.offset + 30 + skip))
        .map_err(|e| e.to_string())?;
    let mut raw = vec![0u8; usize::try_from(m.compressed).map_err(|e| e.to_string())?];
    file.read_exact(&mut raw).map_err(|e| e.to_string())?;
    let data = match m.method {
        0 => raw,
        8 => {
            let mut out = Vec::with_capacity(usize::try_from(m.size).unwrap_or(0));
            DeflateDecoder::new(raw.as_slice())
                .take(m.size + 1)
                .read_to_end(&mut out)
                .map_err(|_| bad())?;
            out
        }
        other => return Err(format!("{}: compression method {other}", m.name)),
    };
    if data.len() as u64 != m.size || crc32fast::hash(&data) != m.crc {
        return Err(bad());
    }
    Ok(data)
}

/// Whether an archive path is a plain relative path.
fn safe_name(name: &str) -> bool {
    let trimmed = name.trim_end_matches('/');
    !trimmed.is_empty()
        && !trimmed.contains('\\')
        && Path::new(trimmed)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// Extracts the archive into the folder `into` (which must not exist yet) and returns the
/// path of the single top-level `*.app` it holds. Paths that leave the folder, links pointing
/// outside and members over `limit` bytes (in total) are refused.
pub fn unpack(archive: &Path, into: &Path, limit: u64) -> Result<PathBuf, String> {
    let mut file = File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let members = members(&mut file)?;
    let top: Vec<&str> = members
        .iter()
        .filter_map(|m| m.name.split('/').next())
        .collect();
    let first = top.first().copied().ok_or("an empty archive")?;
    if !first.ends_with(".app") || top.iter().any(|t| *t != first) {
        return Err("the archive must hold exactly one Name.app".to_owned());
    }
    fs::create_dir(into).map_err(|e| format!("{}: {e}", into.display()))?;
    let mut total = 0u64;
    let mut links = Vec::new();
    for m in &members {
        if !safe_name(&m.name) {
            return Err(format!("{}: unsafe path", m.name));
        }
        total += m.size;
        if total > limit {
            return Err("the archive is larger than allowed".to_owned());
        }
        let path = into.join(m.name.trim_end_matches('/'));
        let kind = m.mode & 0o170_000;
        if kind == S_IFDIR {
            fs::create_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            continue;
        }
        let data = read_member(&mut file, m, limit)?;
        if kind == S_IFLNK {
            links.push((path, data));
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let mut out = File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        out.write_all(&data)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(m.mode & 0o777))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    // Links last, so that no file is written through one; each must stay inside.
    for (path, target) in links {
        let target = String::from_utf8(target).map_err(|_| "a link that is not UTF-8")?;
        let rel = path.strip_prefix(into).map_err(|e| e.to_string())?;
        let depth = rel.components().count().saturating_sub(1) as i64;
        let mut level = depth;
        for c in Path::new(&target).components() {
            match c {
                Component::Normal(_) => level += 1,
                Component::ParentDir => level -= 1,
                Component::CurDir => {}
                _ => level = -1,
            }
            if level < 1 {
                return Err(format!("{}: a link that points outside", rel.display()));
            }
        }
        std::os::unix::fs::symlink(&target, &path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(into.join(first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_modes_and_links() {
        let root = std::env::temp_dir().join(format!("zip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let app = root.join("src/Hello.app/Contents");
        fs::create_dir_all(app.join("bin")).unwrap();
        fs::create_dir_all(app.join("lib")).unwrap();
        fs::write(app.join("Info.toml"), "x".repeat(10_000)).unwrap();
        fs::write(app.join("bin/hello"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(app.join("bin/hello"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(app.join("lib/libx.so.1"), [0u8, 1, 2, 3]).unwrap();
        std::os::unix::fs::symlink("libx.so.1", app.join("lib/libx.so")).unwrap();
        fs::write(app.join("empty"), "").unwrap();
        let archive = root.join("Hello.silicaapp");
        pack(&root.join("src/Hello.app"), &archive).unwrap();
        let out = unpack(&archive, &root.join("out"), 1 << 30).unwrap();
        assert_eq!(out, root.join("out/Hello.app"));
        let c = out.join("Contents");
        assert_eq!(
            fs::read_to_string(c.join("Info.toml")).unwrap().len(),
            10_000
        );
        assert_eq!(
            fs::metadata(c.join("bin/hello"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            fs::read_link(c.join("lib/libx.so")).unwrap(),
            Path::new("libx.so.1")
        );
        assert_eq!(fs::read(c.join("empty")).unwrap(), b"");
        // The same tree: the manifests agree.
        assert_eq!(
            crate::manifest::scan(&app).unwrap(),
            crate::manifest::scan(&c).unwrap()
        );
        // Over the size limit, or into an existing folder: refused.
        assert!(unpack(&archive, &root.join("small"), 100).is_err());
        assert!(unpack(&archive, &root.join("out"), 1 << 30).is_err());
        // A damaged byte fails the checksum.
        let mut bytes = fs::read(&archive).unwrap();
        let at = bytes.windows(8).position(|w| w == b"Info.tom").unwrap() + 20;
        bytes[at] ^= 0xff;
        fs::write(root.join("bad.silicaapp"), bytes).unwrap();
        assert!(unpack(&root.join("bad.silicaapp"), &root.join("bad"), 1 << 30).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unsafe_names_are_refused() {
        assert!(safe_name("Hello.app/Contents/x"));
        assert!(!safe_name("../x"));
        assert!(!safe_name("/etc/passwd"));
        assert!(!safe_name("Hello.app/../../x"));
        assert!(!safe_name(""));
    }
}
