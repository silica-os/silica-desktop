//! The quarantine attribute `user.silica.quarantine` (silica-os `docs/app-bundles.md` § 6):
//! `1;<flags>;<time>;<agent app id>;<origin>`.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const XATTR: &str = "user.silica.quarantine";

/// Saved by an app through the FileChooser portal.
pub const FROM_PORTAL: u16 = 0x0001;
/// Copied out of a disk image.
pub const FROM_VOLUME: u16 = 0x0002;
/// Marked with `silica-quarantine`.
pub const BY_HAND: u16 = 0x0004;

/// The contents of the attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quarantine {
    pub flags: u16,
    /// Unix seconds.
    pub time: u64,
    /// The app that brought the file in (an app id, may be empty).
    pub agent: String,
    /// Where it came from (a URL, may be empty).
    pub origin: String,
}

fn encode_field(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '%' || c == ';' || c.is_control() {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn decode_field(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl Quarantine {
    /// A mark made now.
    pub fn now(flags: u16, agent: &str, origin: &str) -> Quarantine {
        Quarantine {
            flags,
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            agent: agent.to_owned(),
            origin: origin.to_owned(),
        }
    }

    pub fn encode(&self) -> String {
        format!(
            "1;{:04x};{:x};{};{}",
            self.flags,
            self.time,
            encode_field(&self.agent),
            encode_field(&self.origin)
        )
    }

    pub fn parse(text: &str) -> Option<Quarantine> {
        let mut parts = text.trim_end_matches('\0').splitn(5, ';');
        if parts.next()? != "1" {
            return None;
        }
        let flags = u16::from_str_radix(parts.next()?, 16).ok()?;
        let time = u64::from_str_radix(parts.next()?, 16).ok()?;
        let agent = decode_field(parts.next()?)?;
        let origin = decode_field(parts.next().unwrap_or_default())?;
        Some(Quarantine {
            flags,
            time,
            agent,
            origin,
        })
    }
}

/// The quarantine of a file or folder (not following a final link). None when it has none or
/// the attribute is not ours.
pub fn read(path: &Path) -> Option<Quarantine> {
    let mut buf = vec![0u8; 4096];
    let n = rustix::fs::lgetxattr(path, XATTR, &mut buf[..]).ok()?;
    Quarantine::parse(std::str::from_utf8(buf.get(..n)?).ok()?)
}

/// Marks a file or folder.
pub fn write(path: &Path, quarantine: &Quarantine) -> std::io::Result<()> {
    rustix::fs::lsetxattr(
        path,
        XATTR,
        quarantine.encode().as_bytes(),
        rustix::fs::XattrFlags::empty(),
    )
    .map_err(std::io::Error::from)
}

/// Removes the mark; Ok when there was none.
pub fn clear(path: &Path) -> std::io::Result<()> {
    match rustix::fs::lremovexattr(path, XATTR) {
        Ok(()) | Err(rustix::io::Errno::NODATA) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_parses() {
        let q = Quarantine {
            flags: FROM_PORTAL | FROM_VOLUME,
            time: 0x6543_2100,
            agent: "org.mozilla.firefox".to_owned(),
            origin: "https://example.org/a;b%c\nd".to_owned(),
        };
        let text = q.encode();
        assert_eq!(
            text,
            "1;0003;65432100;org.mozilla.firefox;https://example.org/a%3Bb%25c%0Ad"
        );
        assert_eq!(Quarantine::parse(&text), Some(q));
        assert_eq!(Quarantine::parse("2;0;0;;"), None);
        assert_eq!(Quarantine::parse("1;zz;0;;"), None);
        assert_eq!(
            Quarantine::parse("1;0001;10;;").map(|q| q.origin),
            Some(String::new())
        );
    }

    #[test]
    fn marks_files_when_the_filesystem_allows() {
        let dir = std::env::temp_dir().join(format!("quarantine-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("app.AppImage");
        std::fs::write(&file, "x").unwrap();
        let q = Quarantine::now(BY_HAND, "", "https://example.org/app");
        match write(&file, &q) {
            Ok(()) => {
                assert_eq!(read(&file), Some(q));
                clear(&file).unwrap();
                assert_eq!(read(&file), None);
                clear(&file).unwrap();
            }
            // tmpfs without user xattrs (old kernels): nothing to check here.
            Err(e) => assert_eq!(e.raw_os_error(), Some(95), "{e}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
