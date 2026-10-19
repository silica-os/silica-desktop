//! Software update: `dev.frascella.Silica.Updates1` on `/dev/frascella/Silica/Updates1`
//! (docs/spec/updates.md). Bundle feeds (signed, zsync deltas, atomic swap), AppImages (their
//! zsync update information), Flatpak, and the system image (bootc).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Deserialize;
use silica_apps::info::Info;
use silica_apps::registry::{App, Dirs, Kind, Registry};
use silica_bundle::minisign::PublicKey;
use silica_bundle::trust::{Integrity, Roots, verify_bundle};
use silica_bundle::zsync;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::Value;
use zbus::{fdo, interface};

pub const UPDATES_PATH: &str = "/dev/frascella/Silica/Updates1";
/// The largest feed read.
const MAX_FEED: u64 = 1024 * 1024;
/// The largest bundle archive downloaded.
const MAX_ARCHIVE: u64 = 4 * 1024 * 1024 * 1024;

/// A bundle's update feed (`update.feed`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Feed {
    pub format: u32,
    pub id: String,
    pub version: String,
    pub build: u64,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub zsync: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub minimum_silica: Option<String>,
}

/// The trusted comment of a feed's signature.
pub fn feed_comment(id: &str, build: u64) -> String {
    format!("silica-feed id={id} build={build}")
}

/// Parses and checks a feed against the installed bundle: signed by its developer key, for its
/// id, https, and newer. Ok(None) when it announces nothing newer.
pub fn check_feed(text: &str, signature: &str, info: &Info) -> Result<Option<Feed>, String> {
    let key = PublicKey::from_base64(info.developer.key.trim()).map_err(|e| e.to_string())?;
    let comment = key
        .verify(text.as_bytes(), signature)
        .map_err(|e| format!("the feed's signature: {e}"))?;
    let feed: Feed = toml::from_str(text).map_err(|e| format!("the feed: {}", e.message()))?;
    if feed.format != 1 {
        return Err(format!("unknown feed format {}", feed.format));
    }
    if comment != feed_comment(&feed.id, feed.build) {
        return Err("the feed's signature is for another feed".to_owned());
    }
    if feed.id != info.id {
        return Err(format!("the feed is for {}, not {}", feed.id, info.id));
    }
    if !feed.url.starts_with("https://")
        || feed
            .zsync
            .as_deref()
            .is_some_and(|z| !z.starts_with("https://"))
    {
        return Err("the feed's URLs must be https".to_owned());
    }
    if feed.sha256.len() != 64 || !feed.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the feed has no valid sha256".to_owned());
    }
    Ok((feed.build > info.build).then_some(feed))
}

/// Downloads `url` (https only) into memory, at most `limit` bytes; `range` asks for a part.
fn fetch(url: &str, limit: u64, range: Option<(u64, u64)>) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err(format!("not an https URL: {url}"));
    }
    let mut command = Command::new("curl");
    command
        .args([
            "-fsSL",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-time",
            "600",
        ])
        .arg("--max-filesize")
        .arg(limit.to_string());
    if let Some((start, end)) = range {
        command
            .arg("-r")
            .arg(format!("{start}-{}", end.saturating_sub(1)));
    }
    let output = command
        .arg(url)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{url}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.len() as u64 > limit {
        return Err(format!("{url}: larger than allowed"));
    }
    Ok(output.stdout)
}

/// The URL `relative` resolves to next to `base` (zsync control files name their file
/// relatively).
fn resolve_url(base: &str, relative: &str) -> String {
    if relative.contains("://") {
        return relative.to_owned();
    }
    match base.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{relative}"),
        None => relative.to_owned(),
    }
}

/// Downloads the target of a zsync control file, reusing what `seed` already has. Falls back to
/// the whole file when the delta does not check out.
fn zsync_download(control_url: &str, seed: &[u8], limit: u64) -> Result<Vec<u8>, String> {
    let control = zsync::Control::parse(&fetch(control_url, 64 * 1024 * 1024, None)?)?;
    if control.length > limit {
        return Err("the update is larger than allowed".to_owned());
    }
    let url = resolve_url(control_url, &control.url);
    let plan = zsync::plan(&control, seed);
    let ranges = zsync::missing(&control, &plan);
    let mut fetched = Vec::with_capacity(ranges.len());
    for (start, end) in &ranges {
        let bytes = fetch(&url, end - start, Some((*start, *end)))?;
        fetched.push((*start, bytes));
    }
    zsync::assemble(&control, seed, &plan, &fetched).or_else(|_| {
        let whole = fetch(&url, limit, None)?;
        zsync::assemble(
            &control,
            &[],
            &vec![None; control.blocks.len()],
            &[(0, whole)],
        )
    })
}

/// An update found by `Check`.
#[derive(Clone, Debug)]
pub struct Available {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub current: String,
    pub available: String,
    pub details: BTreeMap<String, String>,
    pub size: u64,
}

/// Whether an app with this id runs now (the app registry of the session,
/// `$XDG_RUNTIME_DIR/silica-apps/<id>.<pid>.sock`).
pub fn running(runtime_dir: &Path, id: &str) -> bool {
    let Ok(read) = std::fs::read_dir(runtime_dir.join("silica-apps")) else {
        return false;
    };
    read.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        name.strip_suffix(".sock")
            .and_then(|stem| stem.rsplit_once('.'))
            .is_some_and(|(app, pid)| {
                app == id
                    && pid
                        .parse::<u32>()
                        .is_ok_and(|p| Path::new(&format!("/proc/{p}")).exists())
            })
    })
}

/// Swaps a staged `.<Name>.app.new` in place of the bundle: rename the old away, the new in,
/// remove the old.
pub fn swap(bundle: &Path, staged: &Path) -> Result<(), String> {
    let parent = bundle.parent().ok_or("no folder")?;
    let name = bundle
        .file_name()
        .ok_or("no name")?
        .to_string_lossy()
        .into_owned();
    let old = parent.join(format!(".{name}.old"));
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(bundle, &old).map_err(|e| format!("{}: {e}", bundle.display()))?;
    if let Err(e) = std::fs::rename(staged, bundle) {
        // Put the old one back.
        let _ = std::fs::rename(&old, bundle);
        return Err(format!("{}: {e}", staged.display()));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Where the service keeps things.
#[derive(Clone, Debug)]
pub struct UpdatesConfig {
    pub dirs: Dirs,
    /// `~/.cache/silica/updates`: the seeds of the deltas.
    pub cache: PathBuf,
    pub runtime_dir: PathBuf,
    pub ca_dir: PathBuf,
    pub revocation_lists: Vec<PathBuf>,
    /// The Settings store (`updates.autoCheck`, `updates.autoInstall`).
    pub settings_dir: Option<PathBuf>,
    /// `/sys/class`: power supplies and network interfaces (the daily check's conditions).
    pub sys_class: PathBuf,
}

impl UpdatesConfig {
    pub fn from_env() -> UpdatesConfig {
        let env = |k: &str| std::env::var(k).ok();
        let home = env("HOME").unwrap_or_default();
        let cache = env("XDG_CACHE_HOME")
            .filter(|v| v.starts_with('/'))
            .unwrap_or_else(|| format!("{home}/.cache"));
        let trust = crate::trust::TrustConfig::from_env(None);
        UpdatesConfig {
            dirs: Dirs::from_env(&env),
            cache: PathBuf::from(cache).join("silica/updates"),
            runtime_dir: PathBuf::from(
                env("XDG_RUNTIME_DIR")
                    .unwrap_or_else(|| format!("/run/user/{}", rustix::process::getuid().as_raw())),
            ),
            ca_dir: trust.ca_dir,
            revocation_lists: trust.revocation_lists,
            settings_dir: trust.settings_dir,
            sys_class: PathBuf::from("/sys/class"),
        }
    }
}

struct State {
    found: HashMap<String, Available>,
    /// Bundles downloaded while their app ran: (bundle, staged copy).
    staged: HashMap<String, (PathBuf, PathBuf)>,
}

/// The bus object.
pub struct Updates {
    config: UpdatesConfig,
    state: Arc<Mutex<State>>,
}

fn registry(config: &UpdatesConfig) -> Arc<Registry> {
    crate::apps::live(&config.dirs).unwrap_or_else(|| Arc::new(Registry::scan(config.dirs.clone())))
}

impl Updates {
    pub fn new(config: UpdatesConfig) -> Updates {
        Updates {
            config,
            state: Arc::new(Mutex::new(State {
                found: HashMap::new(),
                staged: HashMap::new(),
            })),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Swaps the staged bundles whose app no longer runs; returns their ids.
    fn finish_staged(&self) -> Vec<String> {
        let staged: Vec<(String, (PathBuf, PathBuf))> = self
            .state()
            .staged
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut done = Vec::new();
        for (id, (bundle, new)) in staged {
            if running(&self.config.runtime_dir, &id) {
                continue;
            }
            if swap(&bundle, &new).is_ok() {
                done.push(id.clone());
            }
            self.state().staged.remove(&id);
        }
        done
    }
}

/// Checks a bundle's feed.
fn check_bundle(app: &App) -> Result<Option<Available>, String> {
    let info = Info::read(&app.path).map_err(|e| e.to_string())?;
    let Some(feed_url) = info.update.feed.clone() else {
        return Ok(None);
    };
    let text = String::from_utf8(fetch(&feed_url, MAX_FEED, None)?).map_err(|e| e.to_string())?;
    let signature = String::from_utf8(fetch(&format!("{feed_url}.minisig"), 64 * 1024, None)?)
        .map_err(|e| e.to_string())?;
    let Some(feed) = check_feed(&text, &signature, &info)? else {
        return Ok(None);
    };
    let mut details = BTreeMap::new();
    details.insert("notes".to_owned(), feed.notes.clone());
    details.insert("path".to_owned(), app.path.to_string_lossy().into_owned());
    details.insert("url".to_owned(), feed.url.clone());
    details.insert("sha256".to_owned(), feed.sha256.clone());
    details.insert("build".to_owned(), feed.build.to_string());
    if let Some(z) = &feed.zsync {
        details.insert("zsync".to_owned(), z.clone());
    }
    Ok(Some(Available {
        id: app.id.clone(),
        kind: "bundle",
        name: app.name.clone(),
        current: info.version,
        available: feed.version,
        details,
        size: feed.size,
    }))
}

/// Checks an AppImage's update information (`zsync|URL`): the control file's SHA-1 against the
/// file's.
fn check_appimage(app: &App) -> Result<Option<Available>, String> {
    let Some(update) = silica_apps::appimage::update_info(&app.path) else {
        return Ok(None);
    };
    let Some(url) = update.strip_prefix("zsync|") else {
        return Ok(None);
    };
    let control = zsync::Control::parse(&fetch(url, 64 * 1024 * 1024, None)?)?;
    let bytes = std::fs::read(&app.path).map_err(|e| e.to_string())?;
    let current: [u8; 20] = <sha1::Sha1 as sha1::Digest>::digest(&bytes).into();
    if current == control.sha1 {
        return Ok(None);
    }
    let mut details = BTreeMap::new();
    details.insert("path".to_owned(), app.path.to_string_lossy().into_owned());
    details.insert("zsync".to_owned(), url.to_owned());
    Ok(Some(Available {
        id: app.id.clone(),
        kind: "appimage",
        name: app.name.clone(),
        current: String::new(),
        available: control.filename.clone(),
        details,
        size: control.length,
    }))
}

/// Parses `flatpak remote-ls --updates --columns=application,version,branch`.
pub fn parse_flatpak_updates(text: &str, installation: &str) -> Vec<Available> {
    text.lines()
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let id = cols.next()?.trim();
            if id.is_empty() || id == "Application ID" {
                return None;
            }
            let version = cols.next().unwrap_or("").trim().to_owned();
            let branch = cols.next().unwrap_or("").trim().to_owned();
            let mut details = BTreeMap::new();
            details.insert("installation".to_owned(), installation.to_owned());
            details.insert("branch".to_owned(), branch.clone());
            Some(Available {
                id: id.to_owned(),
                kind: "flatpak",
                name: id.to_owned(),
                current: String::new(),
                available: if version.is_empty() { branch } else { version },
                details,
                size: 0,
            })
        })
        .collect()
}

fn check_flatpak() -> Vec<Available> {
    let mut out = Vec::new();
    for installation in ["--system", "--user"] {
        let Ok(output) = Command::new("flatpak")
            .args([
                "remote-ls",
                "--updates",
                installation,
                "--columns=application,version,branch",
            ])
            .stdin(Stdio::null())
            .output()
        else {
            continue;
        };
        if output.status.success() {
            out.extend(parse_flatpak_updates(
                &String::from_utf8_lossy(&output.stdout),
                installation,
            ));
        }
    }
    out
}

/// Parses `bootc upgrade --check`: the version available, or None.
pub fn parse_bootc_check(text: &str) -> Option<String> {
    if text.contains("No changes") || text.contains("No update available") {
        return None;
    }
    let version = text.lines().find_map(|l| {
        l.trim()
            .strip_prefix("Version:")
            .map(|v| v.trim().to_owned())
    });
    if text.contains("Update available") {
        return Some(version.unwrap_or_default());
    }
    version
}

fn check_system() -> Option<Available> {
    let output = Command::new("bootc")
        .args(["upgrade", "--check"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let version = parse_bootc_check(&text)?;
    let current = std::fs::read_to_string("/usr/lib/os-release")
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                l.strip_prefix("IMAGE_VERSION=")
                    .map(|v| v.trim_matches('"').to_owned())
            })
        })
        .unwrap_or_default();
    Some(Available {
        id: "system".to_owned(),
        kind: "system",
        name: "Silica".to_owned(),
        current,
        available: version,
        details: BTreeMap::new(),
        size: 0,
    })
}

/// Installs a bundle update: download (delta when a seed is kept), check, unpack beside it,
/// verify (same id, same developer key, the announced build), swap or stage.
fn update_bundle(
    config: &UpdatesConfig,
    a: &Available,
) -> Result<(String, Option<(PathBuf, PathBuf)>), String> {
    let bundle = PathBuf::from(a.details.get("path").ok_or("no path")?);
    let info = Info::read(&bundle).map_err(|e| e.to_string())?;
    let url = a.details.get("url").ok_or("no url")?;
    let sha256 = a.details.get("sha256").ok_or("no sha256")?;
    let build: u64 = a
        .details
        .get("build")
        .and_then(|b| b.parse().ok())
        .ok_or("no build")?;
    let cache = config.cache.join(&info.id);
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let seed_path = cache.join("current.silicaapp");
    let seed = std::fs::read(&seed_path).unwrap_or_default();
    let mut bytes = match a.details.get("zsync") {
        Some(control) if !seed.is_empty() => zsync_download(control, &seed, MAX_ARCHIVE).ok(),
        _ => None,
    }
    .unwrap_or_default();
    if silica_bundle::manifest::sha256(&bytes) != *sha256 {
        bytes = fetch(url, MAX_ARCHIVE, None)?;
    }
    if silica_bundle::manifest::sha256(&bytes) != *sha256 {
        return Err("the download does not match the feed's sha256".to_owned());
    }
    let archive = cache.join("next.silicaapp");
    std::fs::write(&archive, &bytes).map_err(|e| e.to_string())?;
    let parent = bundle.parent().ok_or("no folder")?;
    let unpack_dir = parent.join(format!(".update-{}", info.id));
    let _ = std::fs::remove_dir_all(&unpack_dir);
    let unpacked = silica_bundle::zip::unpack(&archive, &unpack_dir, MAX_ARCHIVE)?;
    let check = (|| {
        let roots = Roots::load(&config.ca_dir, &config.revocation_lists);
        let v = verify_bundle(&unpacked, &roots);
        if v.integrity != Integrity::Valid {
            return Err(format!(
                "the update is not validly signed: {:?}",
                v.integrity
            ));
        }
        let new = v.info.ok_or("the update has no Info.toml")?;
        if new.id != info.id || new.developer.key.trim() != info.developer.key.trim() {
            return Err("the update is for another app or from another developer".to_owned());
        }
        if new.build != build {
            return Err("the update is not the build the feed announced".to_owned());
        }
        Ok(())
    })();
    if let Err(e) = check {
        let _ = std::fs::remove_dir_all(&unpack_dir);
        return Err(e);
    }
    let name = bundle
        .file_name()
        .ok_or("no name")?
        .to_string_lossy()
        .into_owned();
    let staged = parent.join(format!(".{name}.new"));
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::rename(&unpacked, &staged).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&unpack_dir);
    // The new archive is the seed of the next delta.
    let _ = std::fs::rename(&archive, &seed_path);
    if running(&config.runtime_dir, &info.id) {
        return Ok(("pending_restart".to_owned(), Some((bundle, staged))));
    }
    swap(&bundle, &staged)?;
    Ok(("installed".to_owned(), None))
}

/// Replaces an AppImage with its newer version (zsync, the current file as the seed).
fn update_appimage(a: &Available) -> Result<String, String> {
    let path = PathBuf::from(a.details.get("path").ok_or("no path")?);
    let control = a.details.get("zsync").ok_or("no zsync")?;
    let seed = std::fs::read(&path).map_err(|e| e.to_string())?;
    let bytes = zsync_download(control, &seed, MAX_ARCHIVE)?;
    let name = path
        .file_name()
        .ok_or("no name")?
        .to_string_lossy()
        .into_owned();
    let new = path.with_file_name(format!(".{name}.new"));
    std::fs::write(&new, bytes).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&path)
        .map(|m| m.permissions().mode())
        .unwrap_or(0o755);
    let _ = std::fs::set_permissions(&new, std::fs::Permissions::from_mode(mode & 0o777));
    std::fs::rename(&new, &path).map_err(|e| e.to_string())?;
    Ok("installed".to_owned())
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

type UpdateTuple = (
    String,
    String,
    String,
    String,
    String,
    HashMap<String, Value<'static>>,
);

/// How often the background check runs (docs/spec/updates.md, silica-desktop#256).
pub const CHECK_EVERY_SECS: u64 = 24 * 60 * 60;
/// How often the background thread looks whether a check is due and possible.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(15 * 60);
/// The first look after the daemon starts (the session settles first).
const FIRST_LOOK: std::time::Duration = std::time::Duration::from_secs(120);

/// Whether the computer runs on mains power: a `Mains` supply online, or no battery at all (a
/// desktop or a virtual machine).
pub fn on_ac(sys_class: &Path) -> bool {
    let Ok(read) = std::fs::read_dir(sys_class.join("power_supply")) else {
        return true;
    };
    let mut battery = false;
    for supply in read.flatten().map(|e| e.path()) {
        let kind = std::fs::read_to_string(supply.join("type")).unwrap_or_default();
        let online = std::fs::read_to_string(supply.join("online")).unwrap_or_default();
        match kind.trim() {
            "Mains" | "USB" if online.trim() == "1" => return true,
            "Battery" => battery = true,
            _ => {}
        }
    }
    !battery
}

/// Whether a network is up: an interface other than loopback whose link is up.
pub fn online(sys_class: &Path) -> bool {
    let Ok(read) = std::fs::read_dir(sys_class.join("net")) else {
        return false;
    };
    read.flatten().any(|e| {
        e.file_name() != "lo"
            && std::fs::read_to_string(e.path().join("operstate"))
                .is_ok_and(|s| matches!(s.trim(), "up" | "unknown"))
            && std::fs::read_to_string(e.path().join("carrier")).is_ok_and(|s| s.trim() == "1")
    })
}

/// Whether the daily check is due: never done, a day ago or more, or in the future (the clock
/// moved back).
pub fn due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|last| last > now || now - last >= CHECK_EVERY_SECS)
}

/// A boolean of the Settings store (`settings.bin`), or `default`.
fn setting_bool(settings_dir: Option<&Path>, key: &str, default: bool) -> bool {
    settings_dir
        .and_then(|dir| std::fs::read(dir.join("settings.bin")).ok())
        .map(|bytes| crate::settings::decode(&bytes))
        .and_then(|values| match values.get(key) {
            Some(crate::settings::Setting::Bool(b)) => Some(*b),
            _ => None,
        })
        .unwrap_or(default)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The notification of a background check: title and body, in the session's language.
pub fn notification_text(
    found: usize,
    installed: Option<usize>,
    italian: bool,
) -> (String, String) {
    let title = if italian {
        "Aggiornamento software"
    } else {
        "Software Update"
    };
    let body = match (installed, italian) {
        (Some(n), false) => format!(
            "{n} update{} installed. Open Settings › General › Software Update for the rest.",
            if n == 1 { "" } else { "s" }
        ),
        (Some(n), true) => format!(
            "{n} aggiornament{} installat{}. Il resto è in Impostazioni › Generali › Aggiornamento software.",
            if n == 1 { "o" } else { "i" },
            if n == 1 { "o" } else { "i" }
        ),
        (None, false) => format!(
            "{found} update{} available in Settings › General › Software Update.",
            if found == 1 { " is" } else { "s are" }
        ),
        (None, true) => format!(
            "{found} aggiornament{} in Impostazioni › Generali › Aggiornamento software.",
            if found == 1 {
                "o disponibile"
            } else {
                "i disponibili"
            }
        ),
    };
    (title.to_owned(), body)
}

fn notify(title: &str, body: &str) {
    let Ok(bus) = zbus::blocking::Connection::session() else {
        return;
    };
    let hints: HashMap<&str, Value<'_>> =
        HashMap::from([("desktop-entry", Value::from("dev.frascella.Settings"))]);
    let _ = bus.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            title,
            0u32,
            "",
            title,
            body,
            Vec::<&str>::new(),
            hints,
            -1i32,
        ),
    );
}

impl Updates {
    /// Starts the background check: every 15 minutes it looks whether a check is due (once a
    /// day, `updates.autoCheck`), the computer is on mains power and online; then it checks,
    /// keeps what it found for `Update`, and posts a notification. With `updates.autoInstall`
    /// it installs what it found first (but the system image, which needs the administrator's
    /// password: it is only announced).
    pub fn start_daily(&self) {
        let config = self.config.clone();
        let state = Arc::clone(&self.state);
        let _ = std::thread::Builder::new()
            .name("updates-daily".to_owned())
            .spawn(move || {
                std::thread::sleep(FIRST_LOOK);
                loop {
                    daily_tick(&config, &state);
                    std::thread::sleep(LOOK_EVERY);
                }
            });
    }
}

/// One look of the background check.
fn daily_tick(config: &UpdatesConfig, state: &Mutex<State>) {
    let settings = config.settings_dir.as_deref();
    if !setting_bool(settings, "updates.autoCheck", true) {
        return;
    }
    let stamp = config.cache.join("last-check");
    let last = std::fs::read_to_string(&stamp)
        .ok()
        .and_then(|t| t.trim().parse().ok());
    let now = now_secs();
    if !due(last, now) || !on_ac(&config.sys_class) || !online(&config.sys_class) {
        return;
    }
    let found = check_all(config);
    let _ = std::fs::create_dir_all(&config.cache);
    let _ = std::fs::write(&stamp, now.to_string());
    {
        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
        s.found = found.iter().map(|a| (a.id.clone(), a.clone())).collect();
    }
    if found.is_empty() {
        return;
    }
    let italian = std::env::var("LANG").is_ok_and(|l| l.starts_with("it"));
    let installed = setting_bool(settings, "updates.autoInstall", false).then(|| {
        found
            .iter()
            .filter(|a| a.kind != "system")
            .filter(|a| !record(state, &a.id, install(config, a)).starts_with("failed"))
            .count()
    });
    let (title, body) = notification_text(found.len(), installed, italian);
    notify(&title, &body);
}

/// Looks for updates of every source (blocking: network and commands).
fn check_all(config: &UpdatesConfig) -> Vec<Available> {
    let mut found = Vec::new();
    for app in registry(config).apps() {
        let result = match app.kind {
            Kind::Bundle => check_bundle(app),
            Kind::AppImage => check_appimage(app),
            _ => Ok(None),
        };
        match result {
            Ok(Some(a)) => found.push(a),
            Ok(None) => {}
            Err(e) => eprintln!("silica-daemon: updates: {}: {e}", app.id),
        }
    }
    found.extend(check_flatpak());
    found.extend(check_system());
    found
}

/// Installs one update (blocking): the outcome, and the staged bundle when its app runs.
fn install(
    config: &UpdatesConfig,
    found: &Available,
) -> Result<(String, Option<(PathBuf, PathBuf)>), String> {
    match found.kind {
        "bundle" => update_bundle(config, found),
        "appimage" => update_appimage(found).map(|r| (r, None)),
        "flatpak" => {
            let installation = found
                .details
                .get("installation")
                .cloned()
                .unwrap_or_else(|| "--system".to_owned());
            run(
                "flatpak",
                &["update", "--noninteractive", "-y", &installation, &found.id],
            )
            .map(|()| ("installed".to_owned(), None))
        }
        "system" => run("pkexec", &["bootc", "upgrade"]).map(|()| ("staged".to_owned(), None)),
        other => Err(format!("unknown kind {other}")),
    }
}

/// Keeps the outcome of an install in the state; the result string of `Update`.
fn record(
    state: &Mutex<State>,
    id: &str,
    result: Result<(String, Option<(PathBuf, PathBuf)>), String>,
) -> String {
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    match result {
        Ok((outcome, staged)) => {
            if let Some(staged) = staged {
                state.staged.insert(id.to_owned(), staged);
            }
            state.found.remove(id);
            outcome
        }
        Err(e) => format!("failed: {e}"),
    }
}

fn update_tuple(a: Available) -> UpdateTuple {
    let mut details: HashMap<String, Value<'static>> = a
        .details
        .into_iter()
        .map(|(k, v)| (k, Value::from(v)))
        .collect();
    details.insert("size".to_owned(), Value::from(a.size));
    (
        a.id,
        a.kind.to_owned(),
        a.name,
        a.current,
        a.available,
        details,
    )
}

#[interface(name = "dev.frascella.Silica.Updates1")]
impl Updates {
    /// Looks for updates of every source.
    async fn check(&self) -> Vec<UpdateTuple> {
        let config = self.config.clone();
        let found = blocking::unblock(move || check_all(&config)).await;
        self.finish_staged();
        let mut state = self.state();
        state.found = found.iter().map(|a| (a.id.clone(), a.clone())).collect();
        drop(state);
        found.into_iter().map(update_tuple).collect()
    }

    /// Installs the update of `id` found by the last `Check`: `installed`, `pending_restart`
    /// (the app runs: swapped when it quits) or `staged` (the system: at the next restart).
    async fn update(
        &self,
        id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let found = self.state().found.get(id).cloned().ok_or_else(|| {
            fdo::Error::InvalidArgs(format!("no update found for {id}: Check first"))
        })?;
        let _ = Self::progress(&emitter, id, 0.0).await;
        let config = self.config.clone();
        let result = blocking::unblock(move || install(&config, &found)).await;
        let outcome = record(&self.state, id, result);
        let _ = Self::progress(&emitter, id, 1.0).await;
        let _ = Self::finished(&emitter, id, &outcome).await;
        Ok(outcome)
    }

    /// Installs every update found by the last `Check`.
    async fn update_all(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Vec<(String, String)> {
        let ids: Vec<String> = self.state().found.keys().cloned().collect();
        let mut out = Vec::new();
        for id in ids {
            let result = self
                .update(&id, emitter.clone())
                .await
                .unwrap_or_else(|e| format!("failed: {e}"));
            out.push((id, result));
        }
        out
    }

    #[zbus(signal)]
    async fn progress(emitter: &SignalEmitter<'_>, id: &str, fraction: f64) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn finished(emitter: &SignalEmitter<'_>, id: &str, result: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use silica_bundle::minisign::SecretKey;

    fn info(key: &SecretKey, build: u64) -> Info {
        Info::parse(&format!(
            "format = 1\nid = \"dev.example.Hello\"\nname = \"Hello\"\nversion = \"1.0\"\nbuild = {build}\n\
             kind = \"native\"\nruntime = \"none\"\nexec = \"bin/hello\"\n\
             [update]\nfeed = \"https://example.org/feed.toml\"\n\
             [developer]\nname = \"E\"\nkey = \"{}\"\n",
            key.public().to_base64()
        ))
        .unwrap()
    }

    fn feed(build: u64) -> String {
        format!(
            "format = 1\nid = \"dev.example.Hello\"\nversion = \"1.1\"\nbuild = {build}\n\
             url = \"https://example.org/Hello.silicaapp\"\nsha256 = \"{}\"\nsize = 10\n",
            "a".repeat(64)
        )
    }

    #[test]
    fn feeds_must_be_signed_by_the_developer_and_newer() {
        let dev = SecretKey::from_seed([1; 32], [1; 8]);
        let evil = SecretKey::from_seed([2; 32], [2; 8]);
        let installed = info(&dev, 5);
        let text = feed(6);
        let sig = dev.sign(text.as_bytes(), &feed_comment("dev.example.Hello", 6));
        assert_eq!(
            check_feed(&text, &sig, &installed).unwrap().unwrap().build,
            6
        );
        // Not newer.
        let old = feed(5);
        let sig_old = dev.sign(old.as_bytes(), &feed_comment("dev.example.Hello", 5));
        assert_eq!(check_feed(&old, &sig_old, &installed).unwrap(), None);
        // Signed by someone else, or the comment of another build.
        let forged = evil.sign(text.as_bytes(), &feed_comment("dev.example.Hello", 6));
        assert!(check_feed(&text, &forged, &installed).is_err());
        let replayed = dev.sign(text.as_bytes(), &feed_comment("dev.example.Hello", 4));
        assert!(check_feed(&text, &replayed, &installed).is_err());
        // http URLs are refused.
        let http = text.replace("https://example.org/Hello", "http://example.org/Hello");
        let sig_http = dev.sign(http.as_bytes(), &feed_comment("dev.example.Hello", 6));
        assert!(check_feed(&http, &sig_http, &installed).is_err());
    }

    #[test]
    fn flatpak_and_bootc_outputs() {
        let list = parse_flatpak_updates(
            "org.mozilla.firefox\t131.0\tstable\norg.freedesktop.Platform\t\t24.08\n",
            "--system",
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].available, "131.0");
        assert_eq!(list[1].available, "24.08");
        assert_eq!(parse_bootc_check("No changes in: ostree-image\n"), None);
        assert_eq!(
            parse_bootc_check(
                "Update available for: docker://x\n  Version: 0.2.0\n  Digest: sha256:x\n"
            ),
            Some("0.2.0".to_owned())
        );
    }

    #[test]
    fn swapping_is_atomic_and_restores_on_failure() {
        let root = std::env::temp_dir().join(format!("updates-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Hello.app")).unwrap();
        std::fs::write(root.join("Hello.app/v"), "1").unwrap();
        std::fs::create_dir_all(root.join(".Hello.app.new")).unwrap();
        std::fs::write(root.join(".Hello.app.new/v"), "2").unwrap();
        swap(&root.join("Hello.app"), &root.join(".Hello.app.new")).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("Hello.app/v")).unwrap(),
            "2"
        );
        assert!(!root.join(".Hello.app.old").exists());
        // A missing staged copy: the old one stays.
        assert!(swap(&root.join("Hello.app"), &root.join(".nothing")).is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("Hello.app/v")).unwrap(),
            "2"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn running_apps_are_found_by_their_socket() {
        let root = std::env::temp_dir().join(format!("updates-run-{}", std::process::id()));
        std::fs::create_dir_all(root.join("silica-apps")).unwrap();
        std::fs::write(
            root.join(format!(
                "silica-apps/dev.example.Hello.{}.sock",
                std::process::id()
            )),
            "",
        )
        .unwrap();
        std::fs::write(root.join("silica-apps/dev.example.Gone.999999999.sock"), "").unwrap();
        assert!(running(&root, "dev.example.Hello"));
        assert!(!running(&root, "dev.example.Gone"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_daily_check_waits_for_power_network_and_its_day() {
        let root = std::env::temp_dir().join(format!("updates-daily-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let put = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        // No supplies, no interfaces: on AC (a desktop), offline.
        assert!(on_ac(&root));
        assert!(!online(&root));
        put("power_supply/BAT0/type", "Battery\n");
        assert!(!on_ac(&root));
        put("power_supply/AC/type", "Mains\n");
        put("power_supply/AC/online", "0\n");
        assert!(!on_ac(&root));
        put("power_supply/AC/online", "1\n");
        assert!(on_ac(&root));
        put("net/lo/operstate", "unknown\n");
        put("net/lo/carrier", "1\n");
        assert!(!online(&root));
        put("net/eth0/operstate", "down\n");
        put("net/eth0/carrier", "0\n");
        assert!(!online(&root));
        put("net/eth0/operstate", "up\n");
        put("net/eth0/carrier", "1\n");
        assert!(online(&root));
        let day = CHECK_EVERY_SECS;
        assert!(due(None, 1000));
        assert!(!due(Some(1000), 1000 + day - 1));
        assert!(due(Some(1000), 1000 + day));
        assert!(due(Some(5000), 1000));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_notification_says_what_was_found_or_installed() {
        assert_eq!(
            notification_text(1, None, false).1,
            "1 update is available in Settings › General › Software Update."
        );
        assert_eq!(
            notification_text(3, Some(2), false).1,
            "2 updates installed. Open Settings › General › Software Update for the rest."
        );
        assert_eq!(notification_text(2, None, true).0, "Aggiornamento software");
        assert!(
            notification_text(1, Some(1), true)
                .1
                .starts_with("1 aggiornamento installato.")
        );
    }

    #[test]
    fn relative_zsync_urls() {
        assert_eq!(
            resolve_url("https://e.org/a/x.zsync", "x.bin"),
            "https://e.org/a/x.bin"
        );
        assert_eq!(
            resolve_url("https://e.org/a/x.zsync", "https://f.org/y"),
            "https://f.org/y"
        );
    }
}
