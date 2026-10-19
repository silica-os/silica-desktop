//! Type-2 AppImages: an ELF runtime followed by a squashfs, marked with `AI\x02` at offset 8.
//! The registry does not mount them: their embedded desktop entry and icon (in the squashfs
//! root) are extracted once with `unsquashfs` into a cache keyed by the file's identity
//! (docs/spec/launch-services.md); without it the name comes from the file name. The update
//! information is the runtime's `.upd_info` ELF section.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::desktop::{self, DesktopEntry};

/// Whether the file starts like a type-2 AppImage.
pub fn is_appimage(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut head = [0u8; 11];
    file.read_exact(&mut head).is_ok() && is_appimage_header(&head)
}

/// The header test on the first 11 bytes.
pub fn is_appimage_header(head: &[u8]) -> bool {
    head.get(..4) == Some(b"\x7fELF") && head.get(8..11) == Some(b"AI\x02")
}

/// The name shown for an AppImage file: the file name without `.AppImage` (any case) and
/// without a trailing version or architecture (`Krita-5.2.1-x86_64.AppImage` → `Krita`).
pub fn display_name(path: &Path) -> String {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = match file.to_ascii_lowercase().rfind(".appimage") {
        Some(at) => file.get(..at).unwrap_or(&file).to_owned(),
        None => file.clone(),
    };
    let parts: Vec<&str> = stem.split(['-', '_']).collect();
    let mut keep = Vec::new();
    for part in parts {
        let starts_digit = part.starts_with(|c: char| c.is_ascii_digit())
            || (part.starts_with('v') && part.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));
        let arch = matches!(
            part,
            "x86_64" | "x86" | "64" | "amd64" | "aarch64" | "arm64" | "i686" | "armhf" | "linux"
        );
        if (starts_digit || arch) && !keep.is_empty() {
            break;
        }
        keep.push(part);
    }
    let name = keep.join(" ");
    if name.is_empty() { stem } else { name }
}

/// The registry id of an AppImage: `appimage.<name>` with only id characters.
pub fn id_for(path: &Path) -> String {
    let name: String = display_name(path)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("appimage.{name}")
}

/// The update information of the runtime (`.upd_info` section), e.g.
/// `zsync|https://example.org/App-latest-x86_64.AppImage.zsync`. None when absent or empty.
pub fn update_info(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let section = elf_section(&mut file, ".upd_info")?;
    let text: Vec<u8> = section.into_iter().take_while(|b| *b != 0).collect();
    let text = String::from_utf8(text).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Where the squashfs starts: the end of the ELF runtime (its section header table is last),
/// checked by the squashfs magic `hsqs` there.
pub fn payload_offset(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header).ok()?;
    if header.get(..4) != Some(b"\x7fELF") || header.get(4) != Some(&2) || header.get(5) != Some(&1)
    {
        return None;
    }
    let shoff = u64_at(&header, 0x28)?;
    let shentsize = u64::from(u16_at(&header, 0x3a)?);
    let shnum = u64::from(u16_at(&header, 0x3c)?);
    let end = shoff.checked_add(shentsize.checked_mul(shnum)?)?;
    let mut magic = [0u8; 4];
    file.seek(SeekFrom::Start(end)).ok()?;
    file.read_exact(&mut magic).ok()?;
    (&magic == b"hsqs").then_some(end)
}

/// What an AppImage carries about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Embedded {
    /// The file name of its desktop entry without `.desktop` (`org.kde.krita`).
    pub desktop_id: String,
    pub entry: DesktopEntry,
    /// Its icon, extracted (`.png` or `.svg`).
    pub icon: Option<PathBuf>,
}

/// The cache folder of one AppImage: named after its path, size and modification time, so that
/// a replaced or updated file is read again.
pub fn cache_dir(cache: &Path, path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    // FNV-1a over the identity: stable across runs (std's hasher is not).
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(path.as_os_str().as_encoded_bytes());
    feed(&meta.size().to_le_bytes());
    feed(&meta.mtime().to_le_bytes());
    feed(&meta.mtime_nsec().to_le_bytes());
    Some(cache.join(format!("{hash:016x}")))
}

/// The embedded metadata from the cache, when it was extracted before.
pub fn cached(cache: &Path, path: &Path) -> Option<Embedded> {
    read_extracted(&cache_dir(cache, path)?)
}

fn read_extracted(dir: &Path) -> Option<Embedded> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    let file = entries
        .iter()
        .find(|p| p.extension().is_some_and(|e| e == "desktop"))?;
    let text = std::fs::read_to_string(file).ok()?;
    if text.len() > 64 * 1024 {
        return None;
    }
    let entry = desktop::parse(&text);
    if !entry.is_application() && entry.name.is_none() {
        return None;
    }
    let desktop_id = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    // The icon the entry names, else `.DirIcon`.
    let named = entry.icon.as_deref().filter(|i| !i.contains('/'));
    let icon = named
        .and_then(|name| {
            ["png", "svg"]
                .iter()
                .map(|ext| dir.join(format!("{name}.{ext}")))
                .find(|p| p.is_file())
        })
        .or_else(|| Some(dir.join(".DirIcon")).filter(|p| p.is_file()));
    Some(Embedded {
        desktop_id,
        entry,
        icon,
    })
}

/// Extracts the desktop entry and the icons of the squashfs root into the cache (once per
/// file identity) and reads them. Needs `unsquashfs` (squashfs-tools).
pub fn extract(cache: &Path, path: &Path) -> Option<Embedded> {
    let dir = cache_dir(cache, path)?;
    if let Some(found) = read_extracted(&dir) {
        return Some(found);
    }
    let offset = payload_offset(path)?;
    std::fs::create_dir_all(cache).ok()?;
    let staging = dir.with_extension(format!("tmp{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let status = Command::new("unsquashfs")
        .args(["-no-progress", "-quiet", "-no-xattrs", "-o"])
        .arg(offset.to_string())
        .arg("-d")
        .arg(&staging)
        .arg(path)
        .args(["*.desktop", "*.png", "*.svg", ".DirIcon"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    // Only regular files are kept: a link (`.DirIcon` often is one) could point anywhere.
    if let Ok(read) = std::fs::read_dir(&staging) {
        for e in read.flatten() {
            if e.file_type().is_ok_and(|t| !t.is_file()) {
                let _ = std::fs::remove_file(e.path());
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    if !status.success() && read_extracted(&staging).is_none() {
        let _ = std::fs::remove_dir_all(&staging);
        return None;
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&staging, &dir).ok()?;
    read_extracted(&dir)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The contents of a section of a little-endian ELF64 file (at most 64 KiB).
fn elf_section(file: &mut File, name: &str) -> Option<Vec<u8>> {
    let mut header = [0u8; 64];
    file.read_exact(&mut header).ok()?;
    // ELF64, little endian.
    if header.get(..4) != Some(b"\x7fELF") || header.get(4) != Some(&2) || header.get(5) != Some(&1)
    {
        return None;
    }
    let shoff = u64_at(&header, 0x28)?;
    let shentsize = u64::from(u16_at(&header, 0x3a)?);
    let shnum = u64::from(u16_at(&header, 0x3c)?);
    let shstrndx = u64::from(u16_at(&header, 0x3e)?);
    if shentsize < 64 || shnum == 0 || shnum > 4096 || shstrndx >= shnum {
        return None;
    }
    let mut table = vec![0u8; usize::try_from(shentsize * shnum).ok()?];
    file.seek(SeekFrom::Start(shoff)).ok()?;
    file.read_exact(&mut table).ok()?;
    let entry = |i: u64| -> Option<(u32, u64, u64)> {
        let at = usize::try_from(i * shentsize).ok()?;
        Some((
            u32_at(&table, at)?,
            u64_at(&table, at + 0x18)?,
            u64_at(&table, at + 0x20)?,
        ))
    };
    let read = |file: &mut File, offset: u64, size: u64| -> Option<Vec<u8>> {
        if size > 64 * 1024 {
            return None;
        }
        let mut data = vec![0u8; usize::try_from(size).ok()?];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut data).ok()?;
        Some(data)
    };
    let (_, str_off, str_size) = entry(shstrndx)?;
    let names = read(file, str_off, str_size)?;
    for i in 0..shnum {
        let (name_at, offset, size) = entry(i)?;
        let start = usize::try_from(name_at).ok()?;
        let found: Vec<u8> = names
            .get(start..)?
            .iter()
            .copied()
            .take_while(|b| *b != 0)
            .collect();
        if found == name.as_bytes() {
            return read(file, offset, size);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn names_drop_versions_and_architectures() {
        let n = |s: &str| display_name(Path::new(s));
        assert_eq!(n("/a/Krita-5.2.1-x86_64.AppImage"), "Krita");
        assert_eq!(n("Obsidian-1.6.7.AppImage"), "Obsidian");
        assert_eq!(n("Some_App_v2.0.appimage"), "Some App");
        assert_eq!(n("Tool.AppImage"), "Tool");
        assert_eq!(n("2048-1.0.AppImage"), "2048");
        assert_eq!(
            id_for(Path::new("Some_App_v2.0.AppImage")),
            "appimage.Some_App"
        );
    }

    /// A minimal ELF64 with a section-name table and a `.upd_info` section.
    fn tiny_elf(update: &[u8]) -> Vec<u8> {
        let names = b"\0.shstrtab\0.upd_info\0";
        let mut file = vec![0u8; 64];
        file[..4].copy_from_slice(b"\x7fELF");
        file[4] = 2;
        file[5] = 1;
        file[8..11].copy_from_slice(b"AI\x02");
        let names_at = file.len() as u64;
        file.extend_from_slice(names);
        let upd_at = file.len() as u64;
        file.extend_from_slice(update);
        let shoff = file.len() as u64;
        let mut section = |name: u32, off: u64, size: u64| {
            let mut e = vec![0u8; 64];
            e[..4].copy_from_slice(&name.to_le_bytes());
            e[0x18..0x20].copy_from_slice(&off.to_le_bytes());
            e[0x20..0x28].copy_from_slice(&size.to_le_bytes());
            file.extend_from_slice(&e);
        };
        section(0, 0, 0);
        section(1, names_at, names.len() as u64);
        section(11, upd_at, update.len() as u64);
        file[0x28..0x30].copy_from_slice(&shoff.to_le_bytes());
        file[0x3a..0x3c].copy_from_slice(&64u16.to_le_bytes());
        file[0x3c..0x3e].copy_from_slice(&3u16.to_le_bytes());
        file[0x3e..0x40].copy_from_slice(&1u16.to_le_bytes());
        file
    }

    #[test]
    fn the_embedded_entry_and_icon_are_extracted_once() {
        let have = |tool: &str| {
            std::process::Command::new(tool)
                .arg("-version")
                .output()
                .is_ok()
        };
        if !have("mksquashfs") || !have("unsquashfs") {
            eprintln!("squashfs-tools missing: skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("appimage-embed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("root");
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        std::fs::write(
            root.join("org.example.Lemon.desktop"),
            "[Desktop Entry]\nType=Application\nName=Lemon\nName[it]=Limone\nExec=lemon %F\n\
             Icon=lemon\nCategories=Office;\nMimeType=text/x-lemon;\nStartupWMClass=lemon\n",
        )
        .unwrap();
        std::fs::write(root.join("lemon.png"), b"png").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", root.join(".DirIcon")).unwrap();
        std::fs::write(root.join("usr/bin/lemon"), b"#!/bin/sh\n").unwrap();
        let squash = dir.join("payload.squashfs");
        let made = std::process::Command::new("mksquashfs")
            .arg(&root)
            .arg(&squash)
            .args(["-quiet", "-no-progress", "-noappend"])
            .status()
            .unwrap();
        assert!(made.success());
        let mut file = tiny_elf(b"\0");
        let elf_len = file.len() as u64;
        file.extend_from_slice(&std::fs::read(&squash).unwrap());
        let app = dir.join("Lemon-1.0-x86_64.AppImage");
        std::fs::write(&app, &file).unwrap();
        assert_eq!(payload_offset(&app), Some(elf_len));
        let cache = dir.join("cache");
        assert_eq!(cached(&cache, &app), None);
        let found = extract(&cache, &app).unwrap();
        assert_eq!(found.desktop_id, "org.example.Lemon");
        assert_eq!(found.entry.name.as_deref(), Some("Lemon"));
        assert_eq!(
            found.entry.names.get("it").map(String::as_str),
            Some("Limone")
        );
        let icon = found.icon.clone().unwrap();
        assert_eq!(icon.file_name().unwrap(), "lemon.png");
        // Only the root's entries and icons, and no links.
        let dir_of = cache_dir(&cache, &app).unwrap();
        assert!(!dir_of.join("usr").exists());
        assert!(dir_of.join(".DirIcon").symlink_metadata().is_err());
        // From the cache from now on; a changed file is read again.
        assert_eq!(cached(&cache, &app), Some(found));
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&app, &file).unwrap();
        assert_eq!(cached(&cache, &app), None);
        // Not an AppImage with a squashfs: nothing.
        let plain = dir.join("Plain.AppImage");
        std::fs::write(&plain, tiny_elf(b"\0")).unwrap();
        assert_eq!(payload_offset(&plain), None);
        assert_eq!(extract(&cache, &plain), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_the_header_and_the_update_section() {
        let dir = std::env::temp_dir().join(format!("appimage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path: PathBuf = dir.join("App-1.0-x86_64.AppImage");
        std::fs::write(
            &path,
            tiny_elf(b"zsync|https://example.org/App.zsync\0\0\0"),
        )
        .unwrap();
        assert!(is_appimage(&path));
        assert_eq!(
            update_info(&path).as_deref(),
            Some("zsync|https://example.org/App.zsync")
        );
        std::fs::write(&path, tiny_elf(b"\0\0\0\0")).unwrap();
        assert_eq!(update_info(&path), None);
        std::fs::write(&path, b"\x7fELF\x02\x01\x01\0\0\0\0\0").unwrap();
        assert!(!is_appimage(&path));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
