//! Gatekeeper: `dev.frascella.Silica.Trust1` on `/dev/frascella/Silica/Trust1`
//! (docs/spec/trust.md). Decides whether an app bundle or an AppImage may open, asks the shell to
//! show the alerts, keeps the approvals and the recently blocked apps.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use event_listener::Event;
use futures_lite::future;
use silica_apps::info::Info;
use silica_bundle::quarantine;
pub use silica_bundle::trust::{BundleFacts, Policy, Verdict, decide_bundle};
use silica_bundle::trust::{DeveloperStatus, Integrity, Roots, verify_bundle};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::Value;
use zbus::{Connection, fdo, interface};

pub const TRUST_PATH: &str = "/dev/frascella/Silica/Trust1";
/// The polkit action of Open Anyway.
pub const OVERRIDE_ACTION: &str = "dev.silica.trust.override";
/// How long Open Anyway is offered after a block.
pub const OPEN_ANYWAY_WINDOW: u64 = 3600;
/// How long a prompt waits for the shell.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(600);
/// Prompts waiting at once; more are answered `cancel`.
const MAX_PROMPTS: usize = 8;
/// The folders of system apps: always allowed.
const SYSTEM_PREFIXES: [&str; 2] = ["/usr/", "/var/lib/flatpak/app/"];

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// ------------------------------------------------------------------------------- the store

/// An approval: a signed bundle by (id, developer key id), so an update by the same developer
/// stays approved (a modified bundle is still blocked: integrity comes first); an unsigned
/// bundle or an AppImage by (path, inode, size, mtime).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Approval {
    Bundle {
        id: String,
        manifest: String,
    },
    File {
        path: String,
        inode: u64,
        size: u64,
        mtime: i64,
    },
}

/// A recently blocked app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocked {
    pub path: String,
    pub name: String,
    pub time: u64,
}

/// A cached verification, valid while the bundle's files keep their identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cached {
    pub stamp: Stamp,
    pub integrity: u8,
    pub reason: String,
    pub developer: u8,
    pub manifest: String,
    /// How long the full verification took, in milliseconds.
    pub cost_ms: u32,
}

/// A bundle whose full verification takes less than this is verified at every launch; a larger
/// one uses the cache and is verified again in the background.
pub const CACHE_FROM_MS: u32 = 200;

/// What makes a cached verification stale: the inode and modification times of the bundle, its
/// `Contents/`, `Info.toml` and the manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Stamp {
    pub inode: u64,
    pub times: [i64; 4],
}

fn mtime_ns(path: &Path) -> i64 {
    std::fs::symlink_metadata(path)
        .map(|m| {
            m.mtime()
                .saturating_mul(1_000_000_000)
                .saturating_add(m.mtime_nsec())
        })
        .unwrap_or(0)
}

/// The stamp of a bundle now.
pub fn stamp(bundle: &Path) -> Stamp {
    Stamp {
        inode: std::fs::metadata(bundle).map(|m| m.ino()).unwrap_or(0),
        times: [
            mtime_ns(bundle),
            mtime_ns(&bundle.join("Contents")),
            mtime_ns(&bundle.join("Contents/Info.toml")),
            mtime_ns(&bundle.join("Contents/_Signature/manifest")),
        ],
    }
}

/// `trust.bin`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Store {
    pub approvals: HashSet<Approval>,
    pub blocked: Vec<Blocked>,
    pub cache: BTreeMap<String, Cached>,
    /// AppImages the user runs in the sandbox (Get Info › Run in sandbox), by path.
    pub sandboxed: BTreeSet<String>,
}

const MAGIC: &[u8; 4] = b"STRU";
/// 2 adds the sandboxed AppImages after the cache; 1 is read as having none.
const VERSION: u8 = 2;

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&u32::try_from(s.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.bytes.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn str(&mut self) -> Option<String> {
        let n = usize::try_from(self.u32()?).ok()?;
        if n > 1 << 20 {
            return None;
        }
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    fn count(&mut self) -> Option<usize> {
        let n = usize::try_from(self.u32()?).ok()?;
        (n <= 100_000).then_some(n)
    }
}

impl Store {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        let mut approvals: Vec<&Approval> = self.approvals.iter().collect();
        approvals.sort();
        out.extend_from_slice(&u32::try_from(approvals.len()).unwrap_or(0).to_le_bytes());
        for a in approvals {
            match a {
                Approval::Bundle { id, manifest } => {
                    out.push(1);
                    put_str(&mut out, id);
                    put_str(&mut out, manifest);
                }
                Approval::File {
                    path,
                    inode,
                    size,
                    mtime,
                } => {
                    out.push(2);
                    put_str(&mut out, path);
                    out.extend_from_slice(&inode.to_le_bytes());
                    out.extend_from_slice(&size.to_le_bytes());
                    out.extend_from_slice(&mtime.to_le_bytes());
                }
            }
        }
        out.extend_from_slice(&u32::try_from(self.blocked.len()).unwrap_or(0).to_le_bytes());
        for b in &self.blocked {
            put_str(&mut out, &b.path);
            put_str(&mut out, &b.name);
            out.extend_from_slice(&b.time.to_le_bytes());
        }
        out.extend_from_slice(&u32::try_from(self.cache.len()).unwrap_or(0).to_le_bytes());
        for (path, c) in &self.cache {
            put_str(&mut out, path);
            out.extend_from_slice(&c.stamp.inode.to_le_bytes());
            for t in c.stamp.times {
                out.extend_from_slice(&t.to_le_bytes());
            }
            out.push(c.integrity);
            put_str(&mut out, &c.reason);
            out.push(c.developer);
            put_str(&mut out, &c.manifest);
            out.extend_from_slice(&c.cost_ms.to_le_bytes());
        }
        out.extend_from_slice(
            &u32::try_from(self.sandboxed.len())
                .unwrap_or(0)
                .to_le_bytes(),
        );
        for path in &self.sandboxed {
            put_str(&mut out, path);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Store> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return None;
        }
        let version = r.u8()?;
        if version != 1 && version != VERSION {
            return None;
        }
        let mut store = Store::default();
        for _ in 0..r.count()? {
            let approval = match r.u8()? {
                1 => Approval::Bundle {
                    id: r.str()?,
                    manifest: r.str()?,
                },
                2 => Approval::File {
                    path: r.str()?,
                    inode: r.u64()?,
                    size: r.u64()?,
                    mtime: r.i64()?,
                },
                _ => return None,
            };
            store.approvals.insert(approval);
        }
        for _ in 0..r.count()? {
            store.blocked.push(Blocked {
                path: r.str()?,
                name: r.str()?,
                time: r.u64()?,
            });
        }
        for _ in 0..r.count()? {
            let path = r.str()?;
            let inode = r.u64()?;
            let times = [r.i64()?, r.i64()?, r.i64()?, r.i64()?];
            let cached = Cached {
                stamp: Stamp { inode, times },
                integrity: r.u8()?,
                reason: r.str()?,
                developer: r.u8()?,
                manifest: r.str()?,
                cost_ms: r.u32()?,
            };
            store.cache.insert(path, cached);
        }
        if version >= 2 {
            for _ in 0..r.count()? {
                store.sandboxed.insert(r.str()?);
            }
        }
        (r.at == bytes.len()).then_some(store)
    }

    pub fn load(path: &Path) -> Store {
        std::fs::read(path)
            .ok()
            .and_then(|b| Store::decode(&b))
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, self.encode())?;
        std::fs::rename(&tmp, path)
    }
}

fn integrity_code(i: &Integrity) -> (u8, String) {
    match i {
        Integrity::Valid => (0, String::new()),
        Integrity::Unsigned => (1, String::new()),
        Integrity::Tampered(why) => (2, why.clone()),
        Integrity::Invalid(why) => (3, why.clone()),
    }
}

fn integrity_of(code: u8, reason: &str) -> Integrity {
    match code {
        0 => Integrity::Valid,
        1 => Integrity::Unsigned,
        2 => Integrity::Tampered(reason.to_owned()),
        _ => Integrity::Invalid(reason.to_owned()),
    }
}

fn developer_code(d: DeveloperStatus) -> u8 {
    match d {
        DeveloperStatus::Verified => 0,
        DeveloperStatus::SelfSigned => 1,
        DeveloperStatus::Unsigned => 2,
        DeveloperStatus::Revoked => 3,
    }
}

fn developer_of(code: u8) -> DeveloperStatus {
    match code {
        0 => DeveloperStatus::Verified,
        1 => DeveloperStatus::SelfSigned,
        3 => DeveloperStatus::Revoked,
        _ => DeveloperStatus::Unsigned,
    }
}

// -------------------------------------------------------------------------------- service

/// Where the service keeps and reads things.
#[derive(Clone, Debug)]
pub struct TrustConfig {
    /// `~/.local/state/silica` (trust.bin, the user's revocation list).
    pub state_dir: PathBuf,
    /// The Settings store folder (`settings.bin` holds `security.appSources`).
    pub settings_dir: Option<PathBuf>,
    /// The CA keys (`/usr/share/silica/trust/developer-ca`).
    pub ca_dir: PathBuf,
    /// The revocation lists, oldest source first.
    pub revocation_lists: Vec<PathBuf>,
    /// The system bus, for polkit (Open Anyway).
    pub system_bus: Option<Connection>,
    /// The command that opens an approved app (`silica-launch`).
    pub launcher: String,
    /// Where "Move to Applications" copies an app started from a disk image (`/Applications`).
    pub applications_dir: PathBuf,
    /// Where disk images are mounted (`$XDG_RUNTIME_DIR/silica-volumes`).
    pub volumes_dir: PathBuf,
}

impl TrustConfig {
    pub fn from_env(system_bus: Option<Connection>) -> TrustConfig {
        let var = |k: &str| std::env::var(k).ok().filter(|v| v.starts_with('/'));
        let home = std::env::var("HOME").unwrap_or_default();
        let state = var("XDG_STATE_HOME").unwrap_or_else(|| format!("{home}/.local/state"));
        let state_dir = PathBuf::from(state).join("silica");
        TrustConfig {
            revocation_lists: vec![
                PathBuf::from(silica_bundle::trust::SYSTEM_REVOKED),
                state_dir.join("revoked.list"),
            ],
            state_dir,
            settings_dir: crate::settings::SettingsStore::config_dir(),
            ca_dir: PathBuf::from(silica_bundle::trust::SYSTEM_CA_DIR),
            system_bus,
            launcher: "silica-launch".to_owned(),
            applications_dir: PathBuf::from("/Applications"),
            volumes_dir: PathBuf::from(
                std::env::var("XDG_RUNTIME_DIR")
                    .ok()
                    .filter(|v| v.starts_with('/'))
                    .unwrap_or_else(|| format!("/run/user/{}", rustix::process::getuid().as_raw())),
            )
            .join("silica-volumes"),
        }
    }
}

/// The policy in `settings.bin` (`trusted` when unset or unreadable).
pub fn read_policy(settings_dir: Option<&Path>) -> Policy {
    let value = settings_dir
        .and_then(|dir| std::fs::read(dir.join("settings.bin")).ok())
        .map(|bytes| crate::settings::decode(&bytes))
        .and_then(|values| values.get("security.appSources").cloned());
    match value {
        Some(crate::settings::Setting::Text(text)) if text == "anywhere" => Policy::Anywhere,
        Some(crate::settings::Setting::Text(text)) if text == "flathub" => Policy::Flathub,
        _ => Policy::Trusted,
    }
}

/// What an assessment says.
#[derive(Clone, Debug)]
pub struct Assessment {
    pub verdict: Verdict,
    pub path: PathBuf,
    pub kind: &'static str,
    pub id: String,
    pub name: String,
    pub developer: String,
    pub developer_status: DeveloperStatus,
    pub quarantine: Option<quarantine::Quarantine>,
    pub reason: &'static str,
    /// The approval accepting would record.
    pub approval: Option<Approval>,
    /// An AppImage the user runs in the sandbox.
    pub sandbox: bool,
}

impl Assessment {
    pub fn info(&self) -> HashMap<String, Value<'static>> {
        let mut map: HashMap<String, Value<'static>> = HashMap::new();
        let mut s = |k: &str, v: &str| {
            map.insert(k.to_owned(), Value::from(v.to_owned()));
        };
        s("path", &self.path.to_string_lossy());
        s("kind", self.kind);
        s("id", &self.id);
        s("name", &self.name);
        s("developer", &self.developer);
        s("developer_status", self.developer_status.as_str());
        s("reason", self.reason);
        s(
            "origin",
            self.quarantine.as_ref().map_or("", |q| q.origin.as_str()),
        );
        s(
            "agent",
            self.quarantine.as_ref().map_or("", |q| q.agent.as_str()),
        );
        map.insert(
            "quarantined".to_owned(),
            Value::from(self.quarantine.is_some()),
        );
        map.insert("sandbox".to_owned(), Value::from(self.sandbox));
        map
    }
}

struct Prompt {
    assessment: Assessment,
    answer: Option<String>,
}

struct State {
    store: Store,
    prompts: BTreeMap<u32, Prompt>,
    next: u32,
    /// The bus name of the shell that draws the alerts (`RegisterAgent`).
    agent: Option<String>,
}

struct Shared {
    config: TrustConfig,
    state: Mutex<State>,
    answered: Event,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn store_path(&self) -> PathBuf {
        self.config.state_dir.join("trust.bin")
    }

    fn save(&self, store: &Store) {
        if let Err(err) = store.save(&self.store_path()) {
            eprintln!("silica-daemon: trust.bin: {err}");
        }
    }

    fn roots(&self) -> Roots {
        Roots::load(&self.config.ca_dir, &self.config.revocation_lists)
    }

    /// The verification of a bundle. A large bundle (its last full verification took
    /// [`CACHE_FROM_MS`] or more) whose stamp still matches is answered from the cache and
    /// verified again in the background: a file changed in place (which no folder time shows)
    /// moves it to `block` for the next launch and drops its approval. Others are verified in
    /// full every time.
    fn verification(self: &Arc<Self>, bundle: &Path) -> (Integrity, DeveloperStatus, String) {
        let key = bundle.to_string_lossy().into_owned();
        let now_stamp = stamp(bundle);
        let cached = self.state().store.cache.get(&key).cloned();
        if let Some(c) = cached
            && c.stamp == now_stamp
            && now_stamp.inode != 0
            && c.cost_ms >= CACHE_FROM_MS
        {
            // Checked again in full in the background: a file changed in place updates the cache,
            // and the next launch is blocked (integrity comes before any approval).
            let shared = Arc::clone(self);
            let path = bundle.to_path_buf();
            let _ = std::thread::Builder::new()
                .name("trust-reverify".to_owned())
                .spawn(move || {
                    shared.verify_now(&path, now_stamp);
                });
            return (
                integrity_of(c.integrity, &c.reason),
                developer_of(c.developer),
                c.manifest,
            );
        }
        self.verify_now(bundle, now_stamp)
    }

    /// Verifies in full and caches the result.
    fn verify_now(&self, bundle: &Path, now_stamp: Stamp) -> (Integrity, DeveloperStatus, String) {
        let started = std::time::Instant::now();
        let v = verify_bundle(bundle, &self.roots());
        let cost_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        // What approvals are keyed on: the developer key (empty for an unsigned bundle).
        let manifest = v.key_id.map(|k| k.to_string()).unwrap_or_default();
        let (code, reason) = integrity_code(&v.integrity);
        let fresh = Cached {
            stamp: now_stamp,
            integrity: code,
            reason,
            developer: developer_code(v.developer),
            manifest: manifest.clone(),
            cost_ms,
        };
        let mut state = self.state();
        let previous = state
            .store
            .cache
            .insert(bundle.to_string_lossy().into_owned(), fresh.clone());
        // The same outcome for the same files (what most launches find): trust.bin is not
        // written again, unless the bundle crossed the cost threshold.
        if previous.is_some_and(|p| {
            p.stamp == fresh.stamp
                && p.integrity == fresh.integrity
                && p.reason == fresh.reason
                && p.developer == fresh.developer
                && p.manifest == fresh.manifest
                && (p.cost_ms >= CACHE_FROM_MS) == (fresh.cost_ms >= CACHE_FROM_MS)
        }) {
            return (v.integrity, v.developer, manifest);
        }
        // A bounded cache: the oldest entries (by path order) go first.
        while state.store.cache.len() > 512 {
            let Some(first) = state.store.cache.keys().next().cloned() else {
                break;
            };
            state.store.cache.remove(&first);
        }
        let store = state.store.clone();
        drop(state);
        self.save(&store);
        (v.integrity, v.developer, manifest)
    }

    fn assess(self: &Arc<Self>, path: &Path) -> Assessment {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let text = path.to_string_lossy().into_owned();
        let mut a = Assessment {
            verdict: Verdict::Block,
            path: path.clone(),
            kind: "unknown",
            id: String::new(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            developer: String::new(),
            developer_status: DeveloperStatus::Unsigned,
            quarantine: quarantine::read(&path),
            reason: "invalid",
            approval: None,
            sandbox: false,
        };
        if SYSTEM_PREFIXES.iter().any(|p| text.starts_with(p))
            || silica_apps::registry::is_system_path(&path)
        {
            a.verdict = Verdict::Allow;
            a.kind = "system";
            a.reason = "";
            if let Ok(info) = Info::read(&path) {
                a.id = info.id;
                a.name = info.name;
                a.developer = info.developer.name;
            }
            return a;
        }
        if path.join("Contents/Info.toml").exists() || text.ends_with(".app") {
            a.kind = "bundle";
            if let Ok(info) = Info::read(&path) {
                a.id.clone_from(&info.id);
                a.name.clone_from(&info.name);
                a.developer.clone_from(&info.developer.name);
            }
            let (integrity, developer, manifest) = self.verification(&path);
            let approval = (!a.id.is_empty() && !manifest.is_empty()).then(|| Approval::Bundle {
                id: a.id.clone(),
                manifest: manifest.clone(),
            });
            // Unsigned bundles are approved by path and identity, like files.
            let approval = approval.or_else(|| file_approval(&path.join("Contents/Info.toml")));
            let approved = approval
                .as_ref()
                .is_some_and(|ap| self.state().store.approvals.contains(ap));
            a.developer_status = developer;
            let facts = BundleFacts {
                integrity,
                developer,
                quarantined: a.quarantine.is_some(),
                approved,
            };
            let (verdict, reason) =
                decide_bundle(&facts, read_policy(self.config.settings_dir.as_deref()));
            a.verdict = verdict;
            a.reason = reason;
            a.approval = approval;
            return a;
        }
        if silica_apps::appimage::is_appimage(&path) {
            a.kind = "appimage";
            a.sandbox = self.state().store.sandboxed.contains(&text);
            a.name = silica_apps::appimage::display_name(&path);
            a.id = silica_apps::appimage::id_for(&path);
            a.approval = file_approval(&path);
            let approved = a
                .approval
                .as_ref()
                .is_some_and(|ap| self.state().store.approvals.contains(ap));
            // Not yet approved: confirmed, or blocked when only Flathub's apps may open.
            let flathub = read_policy(self.config.settings_dir.as_deref()) == Policy::Flathub;
            (a.verdict, a.reason) = match (approved, flathub) {
                (true, _) => (Verdict::Allow, ""),
                (false, true) => (Verdict::Block, "policy"),
                (false, false) => (Verdict::Confirm, ""),
            };
            return a;
        }
        a
    }

    fn approve(&self, a: &Assessment) {
        let _ = quarantine::clear(&a.path);
        let mut state = self.state();
        if let Some(approval) = &a.approval {
            state.store.approvals.insert(approval.clone());
        }
        state.store.blocked.retain(|b| Path::new(&b.path) != a.path);
        let store = state.store.clone();
        drop(state);
        self.save(&store);
    }

    fn note_blocked(&self, a: &Assessment) {
        let mut state = self.state();
        let now = now();
        state.store.blocked.retain(|b| {
            Path::new(&b.path) != a.path && now.saturating_sub(b.time) < OPEN_ANYWAY_WINDOW
        });
        state.store.blocked.push(Blocked {
            path: a.path.to_string_lossy().into_owned(),
            name: a.name.clone(),
            time: now,
        });
        let store = state.store.clone();
        drop(state);
        self.save(&store);
    }
}

fn file_approval(path: &Path) -> Option<Approval> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Approval::File {
        path: path.to_string_lossy().into_owned(),
        inode: meta.ino(),
        size: meta.len(),
        mtime: meta.mtime(),
    })
}

/// Moves `path` to the user's Trash (`gio trash`, the freedesktop trash of its file system).
fn trash(path: &Path) -> bool {
    std::process::Command::new("gio")
        .arg("trash")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The bus object.
pub struct Trust {
    shared: Arc<Shared>,
}

impl Trust {
    pub fn new(config: TrustConfig) -> Trust {
        let store = Store::load(&config.state_dir.join("trust.bin"));
        Trust {
            shared: Arc::new(Shared {
                config,
                state: Mutex::new(State {
                    store,
                    prompts: BTreeMap::new(),
                    next: 1,
                    agent: None,
                }),
                answered: Event::new(),
            }),
        }
    }

    /// Asks polkit whether the user may override the block (an administrator's password).
    async fn authorized(&self) -> bool {
        let Some(bus) = &self.shared.config.system_bus else {
            return false;
        };
        let pid = std::process::id();
        let start = std::fs::read_to_string("/proc/self/stat")
            .ok()
            .and_then(|s| {
                let rest = s.rsplit_once(')')?.1.to_owned();
                rest.split_whitespace().nth(19)?.parse::<u64>().ok()
            })
            .unwrap_or(0);
        let mut details: HashMap<&str, Value<'_>> = HashMap::new();
        details.insert("pid", Value::from(pid));
        details.insert("start-time", Value::from(start));
        let subject = ("unix-process", details);
        let empty: HashMap<&str, &str> = HashMap::new();
        let reply = bus
            .call_method(
                Some("org.freedesktop.PolicyKit1"),
                "/org/freedesktop/PolicyKit1/Authority",
                Some("org.freedesktop.PolicyKit1.Authority"),
                "CheckAuthorization",
                &(subject, OVERRIDE_ACTION, empty, 1u32, ""),
            )
            .await;
        match reply {
            Ok(message) => message
                .body()
                .deserialize::<(bool, bool, HashMap<String, String>)>()
                .is_ok_and(|(ok, _, _)| ok),
            Err(err) => {
                eprintln!("silica-daemon: trust: polkit: {err}");
                false
            }
        }
    }
}

impl Trust {
    /// Shows an alert through the shell (the registered agent) and waits for the choice:
    /// `cancel` at once without a shell, or after [`PROMPT_TIMEOUT`].
    async fn ask(
        &self,
        connection: &Connection,
        emitter: &SignalEmitter<'_>,
        verdict: &str,
        a: &Assessment,
    ) -> fdo::Result<String> {
        // Nobody to ask (no shell, or it went away): not opened, at once.
        let agent = self.shared.state().agent.clone();
        let present = match agent {
            Some(name) => match zbus::names::BusName::try_from(name.as_str()) {
                Ok(bus_name) => fdo::DBusProxy::new(connection)
                    .await?
                    .name_has_owner(bus_name)
                    .await
                    .unwrap_or(false),
                Err(_) => false,
            },
            None => false,
        };
        if !present {
            return Ok("cancel".to_owned());
        }
        let id = {
            let mut state = self.shared.state();
            if state.prompts.len() >= MAX_PROMPTS {
                return Ok("cancel".to_owned());
            }
            let id = state.next;
            state.next = state.next.wrapping_add(1).max(1);
            state.prompts.insert(
                id,
                Prompt {
                    assessment: a.clone(),
                    answer: None,
                },
            );
            id
        };
        Self::prompt_requested(emitter, id, verdict, a.info())
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        let wait = async {
            loop {
                let listener = self.shared.answered.listen();
                if let Some(answer) = self
                    .shared
                    .state()
                    .prompts
                    .get(&id)
                    .and_then(|p| p.answer.clone())
                {
                    return answer;
                }
                listener.await;
            }
        };
        let timeout = async {
            async_io::Timer::after(PROMPT_TIMEOUT).await;
            "cancel".to_owned()
        };
        let choice = future::or(wait, timeout).await;
        self.shared.state().prompts.remove(&id);
        let _ = Self::prompt_closed(emitter, id, &choice).await;
        Ok(choice)
    }
}

/// Copies the bundle `source` (on a disk image) into `applications` as Move to Applications
/// does (docs/spec/launch.md): under its own name, or the next free one ("Name 2.app"), marked
/// with the quarantine attribute of the image it came from; through the polkit helper when
/// the folder is not writable. Returns the installed path.
pub fn move_to_applications(
    source: &Path,
    applications: &Path,
    origin: &str,
) -> Result<PathBuf, String> {
    let name = source
        .file_name()
        .ok_or_else(|| format!("{}: no name", source.display()))?
        .to_string_lossy()
        .into_owned();
    let free = |n: &str| applications.join(n).symlink_metadata().is_err();
    let stem = name.strip_suffix(".app").unwrap_or(&name).to_owned();
    let target_name = if free(&name) {
        name.clone()
    } else {
        (2..10_000)
            .map(|n| format!("{stem} {n}.app"))
            .find(|n| free(n))
            .ok_or("no free name")?
    };
    let mark = quarantine::Quarantine::now(quarantine::FROM_VOLUME, "dev.frascella.Silica", origin);
    let copy = |from: &Path, to: &Path| -> Result<(), String> {
        let status = std::process::Command::new("cp")
            .args(["-a", "--no-preserve=ownership"])
            .arg(from)
            .arg(to)
            .status()
            .map_err(|e| format!("cp: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("cp: {status}"))
        }
    };
    let probe = applications.join(format!(".silica-move-{}", std::process::id()));
    let writable = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    if writable {
        let staging = applications.join(format!(".{target_name}.installing"));
        let _ = std::fs::remove_dir_all(&staging);
        if let Err(e) = copy(source, &staging) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
        let _ = quarantine::write(&staging, &mark);
        let target = applications.join(&target_name);
        return std::fs::rename(&staging, &target)
            .map(|()| target)
            .map_err(|e| {
                let _ = std::fs::remove_dir_all(&staging);
                e.to_string()
            });
    }
    // Through polkit, as Mustache does: a staging folder of the helper's, the copy, commit.
    let helper = |args: &[&str]| -> Result<String, String> {
        let out = std::process::Command::new("pkexec")
            .arg(APPLICATIONS_HELPER)
            .args(args)
            .output()
            .map_err(|e| format!("pkexec: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
        } else {
            Err("not allowed".to_owned())
        }
    };
    let staging = PathBuf::from(helper(&["begin"])?);
    let inside = staging.join(&target_name);
    copy(source, &inside)?;
    let _ = quarantine::write(&inside, &mark);
    helper(&["commit", &staging.to_string_lossy()]).map(PathBuf::from)
}

/// The privileged helper of /Applications (pkexec runs only this path).
const APPLICATIONS_HELPER: &str = "/usr/libexec/silica/silica-applications-helper";

type PendingTuple = (u32, String, HashMap<String, Value<'static>>);

#[interface(name = "dev.frascella.Silica.Trust1")]
impl Trust {
    /// The verdict on an app bundle or an AppImage, with what it is made of.
    async fn assess(&self, path: &str) -> (String, HashMap<String, Value<'static>>) {
        let shared = Arc::clone(&self.shared);
        let path = PathBuf::from(path);
        let a = blocking::unblock(move || shared.assess(&path)).await;
        (a.verdict.as_str().to_owned(), a.info())
    }

    /// Assesses again and, unless allowed, shows the alert in the shell and waits for the
    /// choice: `open`, `cancel` or `trash`.
    async fn prompt(
        &self,
        path: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let shared = Arc::clone(&self.shared);
        let target = PathBuf::from(path);
        let a = blocking::unblock(move || shared.assess(&target)).await;
        if a.verdict == Verdict::Allow {
            return Ok("open".to_owned());
        }
        if a.verdict == Verdict::Block {
            self.shared.note_blocked(&a);
        }
        let choice = self
            .ask(connection, &emitter, a.verdict.as_str(), &a)
            .await?;
        let choice = match (choice.as_str(), a.verdict) {
            ("open", Verdict::Confirm) => {
                self.shared.approve(&a);
                "open"
            }
            ("trash", _) => {
                let path = a.path.clone();
                if blocking::unblock(move || trash(&path)).await {
                    "trash"
                } else {
                    "cancel"
                }
            }
            _ => "cancel",
        };
        Ok(choice.to_owned())
    }

    /// For an app bundle started from a disk image: asks the shell to suggest moving it to
    /// Applications (the alert `volume`: Move to Applications, Not Now) and, on `move`, copies
    /// it there in quarantine (docs/spec/launch.md). Returns `moved`, `cancel`, or `failed:
    /// <reason>`; `cancel` at once for anything not a bundle on an attached disk image.
    async fn suggest_move(
        &self,
        path: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        let volumes = self.shared.config.volumes_dir.clone();
        let Some((_, image)) = crate::volumes::volume_of(&volumes, &path) else {
            return Ok("cancel".to_owned());
        };
        let Ok(info) = Info::read(&path) else {
            return Ok("cancel".to_owned());
        };
        let a = Assessment {
            verdict: Verdict::Allow,
            path: path.clone(),
            kind: "bundle",
            id: info.id,
            name: info.name,
            developer: info.developer.name,
            developer_status: DeveloperStatus::Unsigned,
            quarantine: None,
            reason: "",
            approval: None,
            sandbox: false,
        };
        let choice = self.ask(connection, &emitter, "volume", &a).await?;
        if choice != "move" {
            return Ok("cancel".to_owned());
        }
        let applications = self.shared.config.applications_dir.clone();
        let origin = format!("file://{}", image.display());
        let moved =
            blocking::unblock(move || move_to_applications(&path, &applications, &origin)).await;
        Ok(match moved {
            Ok(_) => "moved".to_owned(),
            Err(e) => format!("failed: {e}"),
        })
    }

    /// Runs an AppImage in the sandbox from now on, or not (docs/spec/launch.md): kept in
    /// trust.bin by path. False for anything but an AppImage.
    async fn set_sandbox(&self, path: &str, on: bool) -> bool {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        let target = path.clone();
        if !blocking::unblock(move || silica_apps::appimage::is_appimage(&target)).await {
            return false;
        }
        let text = path.to_string_lossy().into_owned();
        let store = {
            let mut state = self.shared.state();
            let changed = if on {
                state.store.sandboxed.insert(text)
            } else {
                state.store.sandboxed.remove(&text)
            };
            changed.then(|| state.store.clone())
        };
        if let Some(store) = store {
            self.shared.save(&store);
        }
        true
    }

    /// The shell registers as the one that draws the alerts; prompts fail at once without it.
    fn register_agent(&self, #[zbus(header)] header: zbus::message::Header<'_>) -> fdo::Result<()> {
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::Failed("no sender".to_owned()))?;
        self.shared.state().agent = Some(sender.to_string());
        Ok(())
    }

    /// The shell's answer to a prompt.
    fn answer(&self, id: u32, choice: &str) -> fdo::Result<()> {
        if !matches!(choice, "open" | "cancel" | "trash" | "move") {
            return Err(fdo::Error::InvalidArgs(format!("not a choice: {choice}")));
        }
        let mut state = self.shared.state();
        let prompt = state
            .prompts
            .get_mut(&id)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("no prompt {id}")))?;
        prompt.answer = Some(choice.to_owned());
        drop(state);
        self.shared.answered.notify(usize::MAX);
        Ok(())
    }

    /// The prompts waiting for an answer (a restarted shell shows them again).
    fn pending(&self) -> Vec<PendingTuple> {
        self.shared
            .state()
            .prompts
            .iter()
            .filter(|(_, p)| p.answer.is_none())
            .map(|(id, p)| {
                (
                    *id,
                    p.assessment.verdict.as_str().to_owned(),
                    p.assessment.info(),
                )
            })
            .collect()
    }

    /// The apps blocked in the last hour: path, name, time.
    fn blocked(&self) -> Vec<(String, String, u64)> {
        let now = now();
        self.shared
            .state()
            .store
            .blocked
            .iter()
            .filter(|b| now.saturating_sub(b.time) < OPEN_ANYWAY_WINDOW)
            .map(|b| (b.path.clone(), b.name.clone(), b.time))
            .collect()
    }

    /// Open Anyway: with an administrator's authorization, approves a recently blocked app and
    /// opens it. False when refused.
    async fn open_anyway(&self, path: &str) -> fdo::Result<bool> {
        let target = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        let recent = {
            let now = now();
            self.shared.state().store.blocked.iter().any(|b| {
                Path::new(&b.path) == target && now.saturating_sub(b.time) < OPEN_ANYWAY_WINDOW
            })
        };
        if !recent {
            return Err(fdo::Error::AccessDenied(
                "Open Anyway is offered only for an hour after the app was blocked".to_owned(),
            ));
        }
        let shared = Arc::clone(&self.shared);
        let probe = target.clone();
        let a = blocking::unblock(move || shared.assess(&probe)).await;
        if matches!(a.verdict, Verdict::Malware) || a.reason == "tampered" {
            return Err(fdo::Error::AccessDenied(
                "a damaged or revoked app cannot be opened".to_owned(),
            ));
        }
        if !self.authorized().await {
            return Ok(false);
        }
        self.shared.approve(&a);
        let launcher = self.shared.config.launcher.clone();
        let _ = std::process::Command::new(&launcher)
            .arg(&a.path)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map(|mut child| {
                std::thread::spawn(move || {
                    let _ = child.wait();
                })
            });
        Ok(true)
    }

    #[zbus(signal)]
    async fn prompt_requested(
        emitter: &SignalEmitter<'_>,
        id: u32,
        verdict: &str,
        info: HashMap<String, Value<'_>>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn prompt_closed(emitter: &SignalEmitter<'_>, id: u32, choice: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_round_trips_and_refuses_damage() {
        let mut store = Store::default();
        store.approvals.insert(Approval::Bundle {
            id: "dev.x".into(),
            manifest: "abc".into(),
        });
        store.approvals.insert(Approval::File {
            path: "/a.AppImage".into(),
            inode: 7,
            size: 9,
            mtime: -1,
        });
        store.blocked.push(Blocked {
            path: "/b.app".into(),
            name: "B".into(),
            time: 5,
        });
        store.cache.insert(
            "/b.app".into(),
            Cached {
                stamp: Stamp {
                    inode: 3,
                    times: [1, 2, 3, 4],
                },
                integrity: 2,
                reason: "bin/x: modified".into(),
                developer: 2,
                manifest: "m".into(),
                cost_ms: 250,
            },
        );
        // A file of version 1 (no sandboxed AppImages) is still read.
        let mut v1 = store.encode();
        v1.truncate(v1.len() - 4);
        v1[4] = 1;
        assert_eq!(Store::decode(&v1), Some(store.clone()));
        store
            .sandboxed
            .insert("/home/u/Applications/Lemon.AppImage".into());
        let bytes = store.encode();
        assert_eq!(Store::decode(&bytes), Some(store));
        for cut in 0..bytes.len() {
            assert!(Store::decode(&bytes[..cut]).is_none());
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(Store::decode(&longer).is_none());
    }

    #[test]
    fn the_policy_comes_from_the_settings_store() {
        let dir = std::env::temp_dir().join(format!("trust-policy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_policy(Some(&dir)), Policy::Trusted);
        let mut values = BTreeMap::new();
        values.insert(
            "security.appSources".to_owned(),
            crate::settings::Setting::Text("anywhere".into()),
        );
        std::fs::write(dir.join("settings.bin"), crate::settings::encode(&values)).unwrap();
        assert_eq!(read_policy(Some(&dir)), Policy::Anywhere);
        values.insert(
            "security.appSources".to_owned(),
            crate::settings::Setting::Text("flathub".into()),
        );
        std::fs::write(dir.join("settings.bin"), crate::settings::encode(&values)).unwrap();
        assert_eq!(read_policy(Some(&dir)), Policy::Flathub);
        assert_eq!(read_policy(None), Policy::Trusted);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
