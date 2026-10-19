//! Type-2 AppImages: an ELF runtime followed by a squashfs, marked with `AI\x02` at offset 8.
//! The registry does not mount them: the name comes from the file name, the update information
//! from the runtime's `.upd_info` ELF section.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

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
