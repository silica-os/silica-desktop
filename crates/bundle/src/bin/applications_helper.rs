//! `silica-applications-helper`: the privileged half of installing into `/Applications` for a
//! user who cannot write it (silica-os `docs/app-bundles.md` § 3). Run through `pkexec`, under
//! the polkit action `dev.silica.applications.install`:
//!
//! ```text
//! silica-applications-helper begin            a staging folder owned by the caller, printed
//! silica-applications-helper commit STAGING [--replace]
//!                                             the one Name.app in it, made root:wheel, moved in
//! silica-applications-helper remove Name.app  deletes an app of /Applications
//! ```
//!
//! The caller copies the bundle into the staging folder itself, as itself (so it never reads
//! anything it could not read already). `commit` walks the staging folder by file descriptors
//! without following links (the caller still owns it, and could swap a folder for a link while
//! the walk runs), takes ownership, clears set-user-ID, set-group-ID and sticky bits, makes the
//! files readable by everyone, and renames the bundle into place.

use std::ffi::{CStr, CString};
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags, RenameFlags};
use rustix::process::{Gid, Uid};

const APPLICATIONS: &str = "/var/Applications";
const STAGING_PREFIX: &str = ".install-";

fn fail(text: &str) -> ExitCode {
    eprintln!("silica-applications-helper: {text}");
    ExitCode::FAILURE
}

/// The caller (pkexec sets `PKEXEC_UID`).
fn caller() -> Result<Uid, String> {
    let uid: u32 = std::env::var("PKEXEC_UID")
        .map_err(|_| "run through pkexec".to_owned())?
        .parse()
        .map_err(|_| "bad PKEXEC_UID".to_owned())?;
    Ok(Uid::from_raw(uid))
}

/// The `wheel` group's id (`/etc/group`), else root's.
fn wheel() -> Gid {
    std::fs::read_to_string("/etc/group")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|l| {
                let mut f = l.split(':');
                (f.next() == Some("wheel")).then(|| f.nth(1)?.parse::<u32>().ok())?
            })
        })
        .map_or(Gid::ROOT, Gid::from_raw)
}

/// Whether `name` is a bundle name: `Something.app`, one path component.
fn valid_bundle_name(name: &str) -> bool {
    name.ends_with(".app")
        && name.len() > 4
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains(['/', '\0', '\n'])
}

fn open_dir_at(dir: impl AsFd, name: &CStr) -> rustix::io::Result<OwnedFd> {
    rustix::fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
}

/// Takes ownership of everything under the directory `fd` (not following links): owner root,
/// group `group`, no set-ID or sticky bits, readable (and searchable) by everyone.
fn adopt(fd: &OwnedFd, group: Gid, depth: usize) -> Result<(), String> {
    if depth > 64 {
        return Err("folders nested too deep".to_owned());
    }
    let mut dir = Dir::read_from(fd).map_err(|e| e.to_string())?;
    let mut names: Vec<CString> = Vec::new();
    while let Some(entry) = dir.read() {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        names.push(name.to_owned());
    }
    for name in names {
        let stat = rustix::fs::statat(fd, name.as_c_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|e| e.to_string())?;
        match FileType::from_raw_mode(stat.st_mode) {
            FileType::Symlink => {
                rustix::fs::chownat(
                    fd,
                    name.as_c_str(),
                    Some(Uid::ROOT),
                    Some(group),
                    AtFlags::SYMLINK_NOFOLLOW,
                )
                .map_err(|e| e.to_string())?;
            }
            FileType::Directory => {
                let child = open_dir_at(fd, &name).map_err(|e| e.to_string())?;
                adopt(&child, group, depth + 1)?;
                rustix::fs::fchown(&child, Some(Uid::ROOT), Some(group))
                    .map_err(|e| e.to_string())?;
                rustix::fs::fchmod(&child, Mode::from_raw_mode(0o755))
                    .map_err(|e| e.to_string())?;
            }
            FileType::RegularFile => {
                let file = rustix::fs::openat(
                    fd,
                    name.as_c_str(),
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                    Mode::empty(),
                )
                .map_err(|e| e.to_string())?;
                let st = rustix::fs::fstat(&file).map_err(|e| e.to_string())?;
                if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile {
                    return Err("an entry changed while it was installed".to_owned());
                }
                rustix::fs::fchown(&file, Some(Uid::ROOT), Some(group))
                    .map_err(|e| e.to_string())?;
                // Keep only the execute bits the owner had, for everyone.
                let exec = if st.st_mode & 0o100 != 0 { 0o755 } else { 0o644 };
                rustix::fs::fchmod(&file, Mode::from_raw_mode(exec)).map_err(|e| e.to_string())?;
            }
            _ => return Err("a bundle holds only files, folders and links".to_owned()),
        }
    }
    Ok(())
}

fn begin(uid: Uid) -> Result<PathBuf, String> {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let name: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let path = Path::new(APPLICATIONS).join(format!("{STAGING_PREFIX}{name}"));
    rustix::fs::mkdir(&path, Mode::from_raw_mode(0o700)).map_err(|e| e.to_string())?;
    rustix::fs::chown(&path, Some(uid), None).map_err(|e| e.to_string())?;
    Ok(path)
}

fn commit(staging: &str, uid: Uid, replace: bool) -> Result<PathBuf, String> {
    let staging_name = Path::new(staging)
        .strip_prefix(APPLICATIONS)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|n| n.starts_with(STAGING_PREFIX) && !n.contains('/'))
        .ok_or("not a staging folder of this helper")?
        .to_owned();
    let apps = rustix::fs::open(
        APPLICATIONS,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| e.to_string())?;
    let staging_c = CString::new(staging_name.clone()).map_err(|e| e.to_string())?;
    let staging_fd = open_dir_at(&apps, &staging_c).map_err(|e| e.to_string())?;
    let st = rustix::fs::fstat(&staging_fd).map_err(|e| e.to_string())?;
    if st.st_uid != uid.as_raw() {
        return Err("the staging folder is not the caller's".to_owned());
    }
    // Exactly one Name.app inside.
    let mut dir = Dir::read_from(&staging_fd).map_err(|e| e.to_string())?;
    let mut bundles = Vec::new();
    while let Some(entry) = dir.read() {
        let entry = entry.map_err(|e| e.to_string())?;
        let bytes = entry.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        bundles.push(entry.file_name().to_owned());
    }
    let [bundle] = bundles.as_slice() else {
        return Err("the staging folder must hold exactly one Name.app".to_owned());
    };
    let name = bundle.to_str().map_err(|e| e.to_string())?.to_owned();
    if !valid_bundle_name(&name) {
        return Err(format!("not a bundle name: {name}"));
    }
    let bundle_fd = open_dir_at(&staging_fd, bundle).map_err(|e| e.to_string())?;
    let group = wheel();
    adopt(&bundle_fd, group, 0)?;
    rustix::fs::fchown(&bundle_fd, Some(Uid::ROOT), Some(group)).map_err(|e| e.to_string())?;
    rustix::fs::fchmod(&bundle_fd, Mode::from_raw_mode(0o755)).map_err(|e| e.to_string())?;
    let flags = if replace {
        RenameFlags::EXCHANGE
    } else {
        RenameFlags::NOREPLACE
    };
    let target = CString::new(name.clone()).map_err(|e| e.to_string())?;
    match rustix::fs::renameat_with(&staging_fd, bundle.as_c_str(), &apps, target.as_c_str(), flags)
    {
        Ok(()) => {}
        // Nothing to exchange with: a plain move.
        Err(rustix::io::Errno::NOENT) if replace => rustix::fs::renameat_with(
            &staging_fd,
            bundle.as_c_str(),
            &apps,
            target.as_c_str(),
            RenameFlags::NOREPLACE,
        )
        .map_err(|e| e.to_string())?,
        Err(rustix::io::Errno::EXIST) => return Err(format!("{name} is already installed")),
        Err(e) => return Err(e.to_string()),
    }
    // The staging folder now holds the replaced app, if any: it goes with it.
    drop(bundle_fd);
    drop(staging_fd);
    let _ = std::fs::remove_dir_all(Path::new(APPLICATIONS).join(&staging_name));
    Ok(Path::new(APPLICATIONS).join(name))
}

fn remove(name: &str) -> Result<(), String> {
    if !valid_bundle_name(name) {
        return Err(format!("not a bundle name: {name}"));
    }
    let path = Path::new(APPLICATIONS).join(name);
    let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("{name}: {e}"))?;
    if !meta.is_dir() {
        return Err(format!("{name} is not an app bundle"));
    }
    // Out of sight first (one rename), then deleted (remove_dir_all does not follow links).
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let gone = Path::new(APPLICATIONS).join(format!(
        ".remove-{}",
        random.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    std::fs::rename(&path, &gone).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(&gone).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    if !rustix::process::geteuid().is_root() {
        return fail("needs root (run through pkexec)");
    }
    let uid = match caller() {
        Ok(uid) => uid,
        Err(e) => return fail(&e),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["begin"] => begin(uid).map(|p| println!("{}", p.display())),
        ["commit", staging] => commit(staging, uid, false).map(|p| println!("{}", p.display())),
        ["commit", staging, "--replace"] => {
            commit(staging, uid, true).map(|p| println!("{}", p.display()))
        }
        ["remove", name] => remove(name),
        _ => Err("usage: silica-applications-helper begin | commit STAGING [--replace] | remove Name.app".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}
