//! `.sxv` disk images, the Silica eXecutable Volume (silica-os `docs/app-bundles.md` § 5): a
//! 4096-byte header, a compressed read-only filesystem (EROFS, or squashfs), and a signature
//! block over the two. Built with `mkfs.erofs`, mounted without privileges with `erofsfuse`.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::minisign::{Hasher, PublicKey, SecretKey};

pub const MAGIC: &[u8; 8] = b"SILICAXV";
pub const VERSION: u16 = 1;
pub const HEADER_LEN: u64 = 4096;
/// The layout file at the top of a volume.
pub const WINDOW_FILE: &str = ".silica-window.toml";
pub const LICENSE_FILE: &str = "LICENSE.txt";
/// The largest signature block read.
const MAX_SIGNATURE_BLOCK: u64 = 64 * 1024;

/// The filesystem of the payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filesystem {
    Erofs,
    Squashfs,
}

impl Filesystem {
    pub fn as_str(self) -> &'static str {
        match self {
            Filesystem::Erofs => "erofs",
            Filesystem::Squashfs => "squashfs",
        }
    }
}

/// The header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u16,
    pub filesystem: Filesystem,
    pub has_license: bool,
    pub payload_offset: u64,
    pub payload_len: u64,
    pub signature_offset: u64,
    pub name: String,
}

/// Whether a volume name is acceptable (it becomes a folder name).
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.contains(['/', '\0', '\n', '\r'])
}

impl Header {
    pub fn encode(&self) -> Result<[u8; HEADER_LEN as usize], String> {
        if !valid_name(&self.name) {
            return Err(format!("not a volume name: {:?}", self.name));
        }
        let mut out = [0u8; HEADER_LEN as usize];
        let mut put = |at: usize, bytes: &[u8]| {
            if let Some(slot) = out.get_mut(at..at + bytes.len()) {
                slot.copy_from_slice(bytes);
            }
        };
        put(0, MAGIC);
        put(8, &self.version.to_le_bytes());
        put(
            10,
            &[match self.filesystem {
                Filesystem::Erofs => 1,
                Filesystem::Squashfs => 2,
            }],
        );
        put(11, &[u8::from(self.has_license)]);
        put(16, &self.payload_offset.to_le_bytes());
        put(24, &self.payload_len.to_le_bytes());
        put(32, &self.signature_offset.to_le_bytes());
        let name = self.name.as_bytes();
        put(40, &u16::try_from(name.len()).unwrap_or(0).to_le_bytes());
        put(42, name);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Header, String> {
        let bad = |why: &str| format!("not a Silica disk image: {why}");
        if bytes.get(..8) != Some(MAGIC.as_slice()) {
            return Err(bad("no SILICAXV magic"));
        }
        let u16_at = |at: usize| -> Option<u16> {
            Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
        };
        let u64_at = |at: usize| -> Option<u64> {
            Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
        };
        let version = u16_at(8).ok_or_else(|| bad("short header"))?;
        if version != VERSION {
            return Err(format!(
                "disk image version {version} (this Silica reads {VERSION})"
            ));
        }
        let filesystem = match bytes.get(10) {
            Some(1) => Filesystem::Erofs,
            Some(2) => Filesystem::Squashfs,
            _ => return Err(bad("unknown filesystem")),
        };
        let has_license = bytes.get(11).is_some_and(|f| f & 1 == 1);
        let payload_offset = u64_at(16).ok_or_else(|| bad("short header"))?;
        let payload_len = u64_at(24).ok_or_else(|| bad("short header"))?;
        let signature_offset = u64_at(32).ok_or_else(|| bad("short header"))?;
        let name_len = usize::from(u16_at(40).ok_or_else(|| bad("short header"))?);
        let name = bytes
            .get(42..42 + name_len)
            .and_then(|n| std::str::from_utf8(n).ok())
            .ok_or_else(|| bad("bad name"))?
            .to_owned();
        if !valid_name(&name) {
            return Err(bad("bad name"));
        }
        if payload_offset != HEADER_LEN
            || payload_offset.checked_add(payload_len) != Some(signature_offset)
        {
            return Err(bad("inconsistent offsets"));
        }
        Ok(Header {
            version,
            filesystem,
            has_license,
            payload_offset,
            payload_len,
            signature_offset,
            name,
        })
    }
}

/// The signature of an image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signature {
    /// Valid, by `key`, with the developer certificate when there is one.
    Valid {
        key: PublicKey,
        cert: Option<String>,
    },
    /// Present but wrong: the image was altered or damaged.
    Invalid(String),
    Unsigned,
}

/// What [`inspect`] finds.
#[derive(Clone, Debug)]
pub struct Image {
    pub header: Header,
    pub signature: Signature,
}

fn take_block(block: &[u8], at: &mut usize) -> Option<Vec<u8>> {
    let len = usize::try_from(u32::from_le_bytes(
        block.get(*at..*at + 4)?.try_into().ok()?,
    ))
    .ok()?;
    let data = block.get(*at + 4..*at + 4 + len)?.to_vec();
    *at += 4 + len;
    Some(data)
}

fn put_block(out: &mut Vec<u8>, data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(data);
}

/// Hashes the header and the payload of an image (BLAKE2b-512), as they are signed.
fn digest(file: &mut File, header: &Header) -> Result<[u8; 64], String> {
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut hasher = Hasher::default();
    let mut left = header.signature_offset;
    let mut buf = vec![0u8; 1024 * 1024];
    while left > 0 {
        let want = usize::try_from(left.min(buf.len() as u64)).map_err(|e| e.to_string())?;
        let chunk = buf.get_mut(..want).ok_or("buffer")?;
        file.read_exact(chunk)
            .map_err(|e| format!("reading the image: {e}"))?;
        hasher.update(chunk);
        left -= want as u64;
    }
    Ok(hasher.finish())
}

/// The trusted comment of an image's signature.
pub fn comment(name: &str) -> String {
    format!("silica-volume name={name}")
}

/// Reads and verifies an image (without mounting it).
pub fn inspect(path: &Path) -> Result<Image, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let mut head = vec![0u8; HEADER_LEN as usize];
    file.read_exact(&mut head)
        .map_err(|_| "not a Silica disk image: too short".to_owned())?;
    let header = Header::decode(&head)?;
    if header.signature_offset > len {
        return Err("the image is truncated".to_owned());
    }
    let block_len = len - header.signature_offset;
    if block_len == 0 {
        return Ok(Image {
            header,
            signature: Signature::Unsigned,
        });
    }
    if block_len > MAX_SIGNATURE_BLOCK {
        return Ok(Image {
            header,
            signature: Signature::Invalid("the signature block is too large".to_owned()),
        });
    }
    let mut block = vec![0u8; usize::try_from(block_len).map_err(|e| e.to_string())?];
    file.seek(SeekFrom::Start(header.signature_offset))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut block).map_err(|e| e.to_string())?;
    let mut at = 0;
    let parts = (
        take_block(&block, &mut at),
        take_block(&block, &mut at),
        take_block(&block, &mut at),
    );
    let (Some(key), Some(sig), Some(cert)) = parts else {
        return Ok(Image {
            header,
            signature: Signature::Invalid("damaged signature block".to_owned()),
        });
    };
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap_or_default();
    let key = match PublicKey::parse(&text(key)) {
        Ok(key) => key,
        Err(e) => {
            return Ok(Image {
                header,
                signature: Signature::Invalid(format!("developer key: {e}")),
            });
        }
    };
    let digest = digest(&mut file, &header)?;
    let signature = match key.verify_prehashed(&digest, &text(sig)) {
        Ok(c) if c == comment(&header.name) => Signature::Valid {
            key,
            cert: Some(text(cert)).filter(|c| !c.is_empty()),
        },
        Ok(_) => Signature::Invalid("signed for another volume".to_owned()),
        Err(e) => Signature::Invalid(e.to_string()),
    };
    Ok(Image { header, signature })
}

// --------------------------------------------------------------------------------- building

/// What goes into an image besides the app.
#[derive(Clone, Debug, Default)]
pub struct BuildOptions {
    /// The volume name (default: the app's name).
    pub name: Option<String>,
    /// A `.silica-window.toml` to use (default: a two-item layout).
    pub window: Option<PathBuf>,
    /// A background PNG (and its `@2x` next to it, when present).
    pub background: Option<PathBuf>,
    pub license: Option<PathBuf>,
    pub readme: Option<PathBuf>,
    /// `lz4hc` (default) or `zstd`.
    pub compression: Option<String>,
    /// `mkfs.erofs` (default: from `PATH`).
    pub mkfs: Option<PathBuf>,
}

/// The default layout: the app on the left, Applications on the right.
pub fn default_window(app_name: &str) -> String {
    format!(
        "width = 540\nheight = 380\nview = \"icons\"\nicon_size = 128\ntext_size = 13\n\n\
         [[item]]\nname = {app:?}\nx = 140\ny = 190\n\n[[item]]\nname = \"Applications\"\nx = 400\ny = 190\n",
        app = format!("{app_name}.app")
    )
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let status = Command::new("cp")
        .arg("-a")
        .arg("--no-preserve=ownership")
        .arg(from)
        .arg(to)
        .status()
        .map_err(|e| format!("cp: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cp {} failed", from.display()))
    }
}

/// Builds `out` from the bundle `app`, signed with `key` (and its developer certificate).
pub fn build(
    app: &Path,
    options: &BuildOptions,
    key: Option<&SecretKey>,
    cert: Option<&str>,
    out: &Path,
) -> Result<Header, String> {
    let info = silica_apps::info::Info::read(app).map_err(|e| e.to_string())?;
    let app_name = app
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("the bundle has no name")?;
    let name = options.name.clone().unwrap_or_else(|| info.name.clone());
    if !valid_name(&name) {
        return Err(format!("not a volume name: {name:?}"));
    }
    let staging = out.with_extension("staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    let result = (|| {
        copy_tree(app, &staging.join(app_name))?;
        std::os::unix::fs::symlink("/Applications", staging.join("Applications"))
            .map_err(|e| e.to_string())?;
        let window = match &options.window {
            Some(path) => {
                fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?
            }
            None => default_window(app_name.trim_end_matches(".app")),
        };
        parse_window(&window)?;
        fs::write(staging.join(WINDOW_FILE), &window).map_err(|e| e.to_string())?;
        if let Some(bg) = &options.background {
            let dir = staging.join(".background");
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let file = bg.file_name().ok_or("background without a name")?;
            fs::copy(bg, dir.join(file)).map_err(|e| format!("{}: {e}", bg.display()))?;
            let stem = bg.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let hi = bg.with_file_name(format!("{stem}@2x.png"));
            if hi.is_file() {
                fs::copy(&hi, dir.join(format!("{stem}@2x.png"))).map_err(|e| e.to_string())?;
            }
        }
        if let Some(license) = &options.license {
            fs::copy(license, staging.join(LICENSE_FILE))
                .map_err(|e| format!("{}: {e}", license.display()))?;
        }
        if let Some(readme) = &options.readme {
            fs::copy(readme, staging.join("README.txt"))
                .map_err(|e| format!("{}: {e}", readme.display()))?;
        }
        let payload = out.with_extension("erofs");
        let compression = options.compression.as_deref().unwrap_or("lz4hc");
        if !matches!(compression, "lz4hc" | "lz4" | "zstd") {
            return Err(format!("unknown compression {compression}"));
        }
        let mkfs = options
            .mkfs
            .clone()
            .unwrap_or_else(|| PathBuf::from("mkfs.erofs"));
        let output = Command::new(&mkfs)
            .arg(format!("-z{compression}"))
            .arg("--all-root")
            .arg("-T0")
            .arg(&payload)
            .arg(&staging)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("{}: {e} (install erofs-utils)", mkfs.display()))?;
        if !output.status.success() {
            return Err(format!(
                "mkfs.erofs: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let payload_len = fs::metadata(&payload).map_err(|e| e.to_string())?.len();
        let header = Header {
            version: VERSION,
            filesystem: Filesystem::Erofs,
            has_license: options.license.is_some(),
            payload_offset: HEADER_LEN,
            payload_len,
            signature_offset: HEADER_LEN + payload_len,
            name: name.clone(),
        };
        let partial = out.with_extension("partial");
        {
            let mut file =
                File::create(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
            file.write_all(&header.encode()?)
                .map_err(|e| e.to_string())?;
            let mut source = File::open(&payload).map_err(|e| e.to_string())?;
            std::io::copy(&mut source, &mut file).map_err(|e| e.to_string())?;
            file.flush().map_err(|e| e.to_string())?;
        }
        let _ = fs::remove_file(&payload);
        if let Some(key) = key {
            let mut file = File::options()
                .read(true)
                .append(true)
                .open(&partial)
                .map_err(|e| e.to_string())?;
            let digest = digest(&mut file, &header)?;
            let mut block = Vec::new();
            put_block(&mut block, key.public().to_file().as_bytes());
            put_block(
                &mut block,
                key.sign_prehashed(&digest, &comment(&name)).as_bytes(),
            );
            put_block(&mut block, cert.unwrap_or_default().as_bytes());
            file.write_all(&block).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        fs::rename(&partial, out).map_err(|e| format!("{}: {e}", out.display()))?;
        Ok(header)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

// ---------------------------------------------------------------------------------- layout

/// An item of the volume window.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct WindowItem {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

fn default_width() -> f64 {
    540.0
}
fn default_height() -> f64 {
    380.0
}
fn default_view() -> String {
    "icons".to_owned()
}
fn default_icon_size() -> f64 {
    128.0
}
fn default_text_size() -> f64 {
    13.0
}

/// `.silica-window.toml`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Window {
    #[serde(default = "default_width")]
    pub width: f64,
    #[serde(default = "default_height")]
    pub height: f64,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default = "default_view")]
    pub view: String,
    #[serde(default = "default_icon_size")]
    pub icon_size: f64,
    #[serde(default = "default_text_size")]
    pub text_size: f64,
    #[serde(default, rename = "item")]
    pub items: Vec<WindowItem>,
}

/// Parses and bounds a layout.
pub fn parse_window(text: &str) -> Result<Window, String> {
    if text.len() > 64 * 1024 {
        return Err(".silica-window.toml is too large".to_owned());
    }
    let mut window: Window =
        toml::from_str(text).map_err(|e| format!(".silica-window.toml: {}", e.message()))?;
    let clamp = |v: f64, lo: f64, hi: f64| if v.is_finite() { v.clamp(lo, hi) } else { lo };
    window.width = clamp(window.width, 200.0, 4000.0);
    window.height = clamp(window.height, 150.0, 3000.0);
    window.icon_size = clamp(window.icon_size, 16.0, 512.0);
    window.text_size = clamp(window.text_size, 9.0, 32.0);
    if !matches!(window.view.as_str(), "icons" | "list") {
        window.view = "icons".to_owned();
    }
    window.background = window
        .background
        .filter(|b| !b.is_empty() && !b.contains('/') && !b.starts_with('.'));
    window
        .items
        .retain(|i| valid_name(&i.name) && i.x.is_finite() && i.y.is_finite());
    window.items.truncate(64);
    Ok(window)
}

// -------------------------------------------------------------------------------- mounting

/// Whether `dir` is a mount point now (`/proc/self/mountinfo`).
pub fn is_mounted(dir: &Path) -> bool {
    let Ok(info) = fs::read_to_string("/proc/self/mountinfo") else {
        return false;
    };
    let want = dir.to_string_lossy();
    info.lines().any(|line| {
        line.split(' ')
            .nth(4)
            .is_some_and(|mp| unescape_mount(mp) == want)
    })
}

/// mountinfo escapes spaces, tabs, newlines and backslashes as octal.
fn unescape_mount(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'\\'
            && let Some(oct) = bytes.get(i + 1..i + 4)
            && let Ok(s) = std::str::from_utf8(oct)
            && let Ok(v) = u8::from_str_radix(s, 8)
        {
            out.push(v);
            i += 4;
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Mounts the payload of `image` (read-only, through FUSE) on the existing empty folder `dir`.
/// `tool`: the `erofsfuse` (or `squashfuse`) to run; None takes it from `PATH`.
pub fn mount(image: &Path, header: &Header, dir: &Path, tool: Option<&Path>) -> Result<(), String> {
    let mut command = match header.filesystem {
        Filesystem::Erofs => {
            let mut c = Command::new(tool.unwrap_or(Path::new("erofsfuse")));
            c.arg(format!("--offset={}", header.payload_offset))
                .arg(image)
                .arg(dir);
            c
        }
        Filesystem::Squashfs => {
            let mut c = Command::new(tool.unwrap_or(Path::new("squashfuse")));
            c.arg("-o")
                .arg(format!("offset={}", header.payload_offset))
                .arg(image)
                .arg(dir);
            c
        }
    };
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", header.filesystem.as_str()))?;
    // The FUSE tools may exit 0 without mounting (fusermount refused): check the table.
    if is_mounted(dir) {
        return Ok(());
    }
    Err(format!(
        "mounting failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Unmounts a FUSE mount.
pub fn unmount(dir: &Path) -> Result<(), String> {
    let output = Command::new("fusermount3")
        .arg("-u")
        .arg(dir)
        .stdin(Stdio::null())
        .output()
        .or_else(|_| Command::new("fusermount").arg("-u").arg(dir).output())
        .map_err(|e| format!("fusermount3: {e}"))?;
    if output.status.success() && !is_mounted(dir) {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            version: VERSION,
            filesystem: Filesystem::Erofs,
            has_license: true,
            payload_offset: HEADER_LEN,
            payload_len: 1000,
            signature_offset: HEADER_LEN + 1000,
            name: "Hello 1.0".to_owned(),
        }
    }

    #[test]
    fn headers_round_trip_and_refuse() {
        let h = header();
        let bytes = h.encode().unwrap();
        assert_eq!(Header::decode(&bytes).unwrap(), h);
        let mut bad = bytes;
        bad[0] = b'X';
        assert!(Header::decode(&bad).is_err());
        let mut bad = bytes;
        bad[8] = 2;
        assert!(Header::decode(&bad).unwrap_err().contains("version 2"));
        let mut bad = bytes;
        bad[24] = 0xff; // payload length no longer matches the signature offset
        assert!(Header::decode(&bad).is_err());
        assert!(
            Header {
                name: "a/b".into(),
                ..header()
            }
            .encode()
            .is_err()
        );
        assert!(Header::decode(&bytes[..20]).is_err());
    }

    #[test]
    fn layouts_are_bounded() {
        let w = parse_window(
            "width = 99999\nheight = 300\nbackground = \"../../etc/x.png\"\nview = \"cover\"\n\
             [[item]]\nname = \"Hello.app\"\nx = 10\ny = 20\n[[item]]\nname = \"../x\"\nx = 1\ny = 1\n",
        )
        .unwrap();
        assert_eq!(w.width, 4000.0);
        assert_eq!(w.background, None);
        assert_eq!(w.view, "icons");
        assert_eq!(w.items.len(), 1);
        let d = parse_window(&default_window("Hello")).unwrap();
        assert_eq!(d.items.len(), 2);
        assert_eq!(d.items[0].name, "Hello.app");
        assert!(parse_window("width = \"x\"").is_err());
    }

    #[test]
    fn mountinfo_escapes() {
        assert_eq!(unescape_mount("/run/a\\040b"), "/run/a b");
        assert_eq!(unescape_mount("/plain"), "/plain");
    }

    /// The whole path with the real tools, when they are installed (`mkfs.erofs` from
    /// erofs-utils): build, inspect, alter a byte, and see the signature fail.
    #[test]
    fn build_and_verify_with_mkfs_erofs() {
        let mkfs = std::env::var_os("SILICA_MKFS_EROFS")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::split_paths(&std::env::var_os("PATH")?)
                    .map(|d| d.join("mkfs.erofs"))
                    .find(|p| p.is_file())
            });
        let Some(mkfs) = mkfs else {
            eprintln!("mkfs.erofs not installed: skipped");
            return;
        };
        let root = std::env::temp_dir().join(format!("sxv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let key = SecretKey::from_seed([4; 32], [4; 8]);
        let app = root.join("Hello.app");
        fs::create_dir_all(app.join("Contents/bin")).unwrap();
        fs::write(
            app.join("Contents/Info.toml"),
            format!(
                "format = 1\nid = \"dev.example.Hello\"\nname = \"Hello\"\nversion = \"1\"\nbuild = 1\n\
                 kind = \"native\"\nruntime = \"none\"\nexec = \"bin/hello\"\n\
                 [developer]\nname = \"Example\"\nkey = \"{}\"\n",
                key.public().to_base64()
            ),
        )
        .unwrap();
        fs::write(app.join("Contents/bin/hello"), "#!/bin/sh\n").unwrap();
        fs::write(root.join("LICENSE"), "Be nice.").unwrap();
        let out = root.join("Hello.sxv");
        let options = BuildOptions {
            license: Some(root.join("LICENSE")),
            mkfs: Some(mkfs),
            ..BuildOptions::default()
        };
        let header = build(&app, &options, Some(&key), None, &out).unwrap();
        assert_eq!(header.name, "Hello");
        assert!(header.has_license);
        let image = inspect(&out).unwrap();
        assert_eq!(image.header, header);
        assert!(
            matches!(&image.signature, Signature::Valid { key: k, cert: None } if *k == key.public())
        );
        // One byte of the payload changed: the signature fails.
        let mut bytes = fs::read(&out).unwrap();
        let at = HEADER_LEN as usize + 1500;
        bytes[at] ^= 0x55;
        let bad = root.join("Bad.sxv");
        fs::write(&bad, &bytes).unwrap();
        assert!(matches!(
            inspect(&bad).unwrap().signature,
            Signature::Invalid(_)
        ));
        // Unsigned.
        let plain = root.join("Plain.sxv");
        build(&app, &options, None, None, &plain).unwrap();
        assert_eq!(inspect(&plain).unwrap().signature, Signature::Unsigned);
        // A renamed volume (header changed) breaks it too.
        let mut bytes = fs::read(&out).unwrap();
        bytes[42] = b'J';
        fs::write(&bad, &bytes).unwrap();
        assert!(matches!(
            inspect(&bad).unwrap().signature,
            Signature::Invalid(_)
        ));
        let _ = fs::remove_dir_all(&root);
    }
}
