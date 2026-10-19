//! Launch Services: `dev.frascella.Silica.Apps1` on `/dev/frascella/Silica/Apps1`, the app
//! registry of the session (docs/spec/launch-services.md).
//!
//! The registry itself is `silica_apps::Registry`; this module keeps it current (inotify on
//! every source folder, 150 ms of quiet before a rescan), remembers launches in `apps.bin`,
//! writes the compatibility desktop entries of bundles, and serves it on the bus. The rest of
//! the daemon (app icons, document icons, the Share menu) reads the same live registry through
//! [`live`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsStr;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustix::fs::inotify;
use silica_apps::db::Db;
use silica_apps::exec;
use silica_apps::registry::{App, Dirs, Kind, Registry};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::Value;
use zbus::{Connection, fdo, interface};

use crate::icons::mime::{self, MimeDb};

pub const APPS_PATH: &str = "/dev/frascella/Silica/Apps1";

/// Quiet time after a change before the registry is read again.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// How often the folders are read when inotify cannot watch them all.
const FALLBACK_RESCAN: Duration = Duration::from_secs(10);
/// How long the database remembers an app that is gone (a reinstall within this time is not
/// new).
const FORGET_AFTER: u64 = 30 * 24 * 3600;

/// The registry of this process, kept current by the `Apps1` service. None until it starts.
static LIVE: Mutex<Option<Arc<Registry>>> = Mutex::new(None);

/// The live registry, when the `Apps1` service runs in this process and reads the same folders
/// as `dirs` (tests build their own).
pub fn live(dirs: &Dirs) -> Option<Arc<Registry>> {
    let guard = LIVE.lock().ok()?;
    let registry = guard.as_ref()?;
    (registry.dirs().data_home == dirs.data_home
        && registry.dirs().config_home == dirs.config_home
        && registry.dirs().data_dirs == dirs.data_dirs)
        .then(|| Arc::clone(registry))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// What the service reads and writes.
#[derive(Clone, Debug)]
pub struct AppsConfig {
    pub dirs: Dirs,
    /// `apps.bin`.
    pub db_path: PathBuf,
    /// The folders of the shared MIME database (`<dir>/mime`).
    pub mime_dirs: Vec<PathBuf>,
    /// Whether to write the compatibility entries and run `update-desktop-database`.
    pub compat: bool,
    /// Whether this service publishes its registry as the process's [`live`] one.
    pub publish_live: bool,
}

impl AppsConfig {
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> AppsConfig {
        let cache = env("XDG_CACHE_HOME")
            .filter(|v| v.starts_with('/'))
            .unwrap_or_else(|| format!("{}/.cache", env("HOME").unwrap_or_default()));
        AppsConfig {
            // The daemon's registry extracts what AppImages embed (their entry and icon).
            dirs: Dirs {
                extract_appimages: true,
                ..Dirs::from_env(env)
            },
            db_path: Path::new(&cache).join("silica/apps.bin"),
            mime_dirs: mime::data_dirs(env),
            compat: true,
            publish_live: true,
        }
    }
}

/// A change of the registry, per app id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Added(String),
    Removed(String),
    Changed(String),
}

/// The changes from `old` to `new`.
pub fn diff(old: &Registry, new: &Registry) -> Vec<Change> {
    let mut changes = Vec::new();
    for app in new.apps() {
        match old.get(&app.id) {
            None => changes.push(Change::Added(app.id.clone())),
            Some(before) if before != app => changes.push(Change::Changed(app.id.clone())),
            Some(_) => {}
        }
    }
    for app in old.apps() {
        if new.get(&app.id).is_none() {
            changes.push(Change::Removed(app.id.clone()));
        }
    }
    changes
}

struct State {
    registry: Arc<Registry>,
    db: Db,
    /// The compatibility entries written, by file name.
    compat: BTreeMap<String, String>,
}

struct Shared {
    config: AppsConfig,
    mime: MimeDb,
    state: Mutex<State>,
    changes: async_channel::Sender<Vec<Change>>,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn registry(&self) -> Arc<Registry> {
        Arc::clone(&self.state().registry)
    }

    /// Reads every source again; returns the changes (also sent to the signal task).
    fn refresh(&self) -> Vec<Change> {
        let fresh = Registry::scan(self.config.dirs.clone());
        let changes = {
            let mut state = self.state();
            let changes = diff(&state.registry, &fresh);
            let now = now();
            let mut dirty = state
                .db
                .observe(fresh.apps().iter().map(|a| a.id.as_str()), now);
            let installed: HashSet<&str> = fresh.apps().iter().map(|a| a.id.as_str()).collect();
            let before = state.db.records.len();
            state.db.prune(&installed, now, FORGET_AFTER);
            dirty |= before != state.db.records.len();
            if dirty && let Err(err) = state.db.save(&self.config.db_path) {
                eprintln!(
                    "silica-daemon: apps: {}: {err}",
                    self.config.db_path.display()
                );
            }
            let fresh = Arc::new(fresh);
            if self.config.compat {
                let written = write_compat(&fresh, &state.compat);
                state.compat = written;
            }
            state.registry = Arc::clone(&fresh);
            if self.config.publish_live
                && let Ok(mut live) = LIVE.lock()
            {
                *live = Some(fresh);
            }
            changes
        };
        if !changes.is_empty() {
            let _ = self.changes.try_send(changes.clone());
        }
        changes
    }

    fn chain(&self, mime: &str) -> Vec<String> {
        if mime.starts_with("x-scheme-handler/") {
            return vec![mime.to_owned()];
        }
        self.mime.chain(mime)
    }

    /// The type of a URI: a file's by name, then by its first bytes; a folder is
    /// `inode/directory`; another URI is `x-scheme-handler/<scheme>`.
    fn type_of(&self, uri: &str) -> Option<String> {
        if let Some(path) = exec::target_path(uri) {
            let path = Path::new(&path);
            if path.is_dir() {
                return Some("inode/directory".to_owned());
            }
            let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
            return self
                .mime
                .for_name(name)
                .map(str::to_owned)
                .or_else(|| mime::sniff(&mime::read_head(path)).map(str::to_owned))
                .or_else(|| Some("application/octet-stream".to_owned()));
        }
        let (scheme, _) = uri.split_once(':')?;
        let valid = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        valid.then(|| format!("x-scheme-handler/{}", scheme.to_ascii_lowercase()))
    }
}

/// Writes the compatibility entries of the registry's bundles and AppImages, removes the stale
/// ones and refreshes the desktop database of the folder when something changed. Returns what
/// is written now.
fn write_compat(
    registry: &Registry,
    before: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let dir = registry.dirs().compat_dir();
    let wanted: BTreeMap<String, String> = registry
        .apps()
        .iter()
        .filter(|a| matches!(a.kind, Kind::Bundle | Kind::AppImage))
        .map(|a| (format!("{}.desktop", a.id), a.compat_entry()))
        .collect();
    let mut changed = false;
    if !wanted.is_empty() && std::fs::create_dir_all(&dir).is_err() {
        return before.clone();
    }
    for (name, text) in &wanted {
        let path = dir.join(name);
        let same = before.get(name) == Some(text)
            && std::fs::read_to_string(&path).is_ok_and(|t| &t == text);
        if same {
            continue;
        }
        let tmp = dir.join(format!(".{name}.tmp"));
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            changed = true;
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    if let Ok(read) = std::fs::read_dir(&dir) {
        for item in read.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if name.ends_with(".desktop") && !wanted.contains_key(&name) {
                let _ = std::fs::remove_file(item.path());
                changed = true;
            }
        }
    }
    if changed {
        // `update-desktop-database` keeps `mimeinfo.cache` for GLib and Qt; best effort.
        let _ = std::process::Command::new("update-desktop-database")
            .arg("-q")
            .arg(&dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    wanted
}

// -------------------------------------------------------------------------------- watching

/// Which events of a watched folder matter.
#[derive(Clone, Debug)]
enum Filter {
    Any,
    /// Only these names (a folder watched for one file, or for a child that does not exist
    /// yet).
    Names(HashSet<String>),
}

/// The folders to watch and their filters.
fn watch_plan(registry: &Registry) -> Vec<(PathBuf, Filter)> {
    let dirs = registry.dirs();
    let mut plan: Vec<(PathBuf, Filter)> = Vec::new();
    let want = |path: PathBuf, filter: Filter, plan: &mut Vec<(PathBuf, Filter)>| {
        // A folder that does not exist: its nearest existing parent, for the next name.
        let mut target = path;
        let mut filter = filter;
        while !target.is_dir() {
            let Some(name) = target.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                return;
            };
            let Some(parent) = target.parent().map(Path::to_path_buf) else {
                return;
            };
            filter = Filter::Names([name].into_iter().collect());
            target = parent;
        }
        match plan.iter_mut().find(|(p, _)| *p == target) {
            Some((_, existing)) => {
                if let (Filter::Names(a), Filter::Names(b)) = (existing.clone(), &filter) {
                    *existing = Filter::Names(a.union(b).cloned().collect());
                } else {
                    *existing = Filter::Any;
                }
            }
            None => plan.push((target, filter)),
        }
    };
    for (folder, _) in &dirs.app_folders {
        want(folder.clone(), Filter::Any, &mut plan);
    }
    for root in dirs.application_dirs() {
        want(root.clone(), Filter::Any, &mut plan);
        if let Ok(read) = std::fs::read_dir(&root) {
            for item in read.flatten() {
                if item.path().is_dir() {
                    want(item.path(), Filter::Any, &mut plan);
                }
            }
        }
    }
    want(
        dirs.config_home.clone(),
        Filter::Names(["mimeapps.list".to_owned()].into_iter().collect()),
        &mut plan,
    );
    // Each bundle's Contents/ (its Info.toml).
    for app in registry.apps() {
        if app.path.extension().is_some_and(|e| e == "app") {
            want(app.path.join("Contents"), Filter::Any, &mut plan);
        }
    }
    plan
}

struct Watches {
    fd: OwnedFd,
    filters: HashMap<i32, Filter>,
    /// Some folder could not be watched (out of inotify watches): read again every 10 s.
    incomplete: bool,
}

fn add_watches(plan: &[(PathBuf, Filter)]) -> Option<Watches> {
    let fd = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK).ok()?;
    let flags = inotify::WatchFlags::CREATE
        | inotify::WatchFlags::DELETE
        | inotify::WatchFlags::MOVED_FROM
        | inotify::WatchFlags::MOVED_TO
        | inotify::WatchFlags::CLOSE_WRITE
        | inotify::WatchFlags::ATTRIB
        | inotify::WatchFlags::DELETE_SELF
        | inotify::WatchFlags::MOVE_SELF
        | inotify::WatchFlags::ONLYDIR;
    let mut filters = HashMap::new();
    let mut incomplete = false;
    for (path, filter) in plan {
        match inotify::add_watch(&fd, path.as_path(), flags) {
            Ok(wd) => {
                filters.insert(wd, filter.clone());
            }
            Err(rustix::io::Errno::NOSPC) => incomplete = true,
            Err(_) => {}
        }
    }
    Some(Watches {
        fd,
        filters,
        incomplete,
    })
}

/// Reads the pending events; true when one of them matters.
fn drain(watches: &Watches) -> bool {
    let mut buf = [MaybeUninit::<u8>::uninit(); 8192];
    let mut reader = inotify::Reader::new(watches.fd.as_fd(), &mut buf);
    let mut relevant = false;
    loop {
        match reader.next() {
            Ok(event) => {
                let flags = event.events();
                if flags.contains(inotify::ReadFlags::QUEUE_OVERFLOW) {
                    relevant = true;
                    continue;
                }
                let Some(filter) = watches.filters.get(&event.wd()) else {
                    continue;
                };
                let matters = match filter {
                    Filter::Any => true,
                    Filter::Names(names) => event
                        .file_name()
                        .is_some_and(|n| names.contains(n.to_string_lossy().as_ref())),
                };
                // Our own compatibility entries' temporary files do not count.
                let ours = event
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(".tmp"));
                relevant |= matters && !ours;
            }
            Err(rustix::io::Errno::AGAIN) => break,
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => break,
        }
    }
    relevant
}

/// Waits for the fd to be readable for at most `timeout` (None: forever). True when readable.
fn wait(fd: &OwnedFd, timeout: Option<Duration>) -> bool {
    let mut fds = [rustix::event::PollFd::new(fd, rustix::event::PollFlags::IN)];
    let timeout = timeout.and_then(|t| rustix::event::Timespec::try_from(t).ok());
    matches!(rustix::event::poll(&mut fds, timeout.as_ref()), Ok(n) if n > 0)
}

/// The watcher thread: watches, waits for a relevant change, waits for quiet, rescans.
fn watch_loop(shared: Arc<Shared>) {
    loop {
        let plan = watch_plan(&shared.registry());
        let Some(watches) = add_watches(&plan) else {
            // No inotify at all: read the folders now and then.
            std::thread::sleep(FALLBACK_RESCAN);
            shared.refresh();
            continue;
        };
        // What changed between the last scan and the new watches.
        let changes = shared.refresh();
        if !changes.is_empty() {
            // The set of folders may differ (a new bundle's Contents/): watch again.
            continue;
        }
        let timeout = watches.incomplete.then_some(FALLBACK_RESCAN);
        let readable = wait(&watches.fd, timeout);
        if readable && !drain(&watches) {
            continue;
        }
        // Coalesce a burst (a copy of a large bundle writes many files).
        while wait(&watches.fd, Some(DEBOUNCE)) {
            drain(&watches);
        }
        drop(watches);
    }
}

// ------------------------------------------------------------------------------- properties

fn strings(items: &[String]) -> Value<'static> {
    Value::from(items.to_vec())
}

fn string_map(map: impl IntoIterator<Item = (String, String)>) -> Value<'static> {
    Value::from(map.into_iter().collect::<HashMap<String, String>>())
}

/// The properties of an app (`a{sv}`, docs/spec/launch-services.md).
pub fn properties(app: &App, db: &Db) -> HashMap<String, Value<'static>> {
    let mut p: HashMap<String, Value<'static>> = HashMap::new();
    let s = |k: &str, v: &str, p: &mut HashMap<String, Value<'static>>| {
        p.insert(k.to_owned(), Value::from(v.to_owned()));
    };
    s("id", &app.id, &mut p);
    s("name", &app.name, &mut p);
    s("kind", app.kind.as_str(), &mut p);
    s("path", &app.path.to_string_lossy(), &mut p);
    s("location", app.location.as_str(), &mut p);
    s("generic_name", &app.generic_name, &mut p);
    s("comment", &app.comment, &mut p);
    s("version", &app.version, &mut p);
    s("exec", &app.exec, &mut p);
    s("icon", &app.icon, &mut p);
    s("silica_icon", &app.silica_icon, &mut p);
    s("wm_class", &app.wm_class, &mut p);
    s("runtime", &app.runtime, &mut p);
    s("developer", &app.developer, &mut p);
    s(
        "app_kind",
        app.app_kind.map_or("", silica_apps::info::AppKind::as_str),
        &mut p,
    );
    s(
        "desktop_file",
        &app.desktop_file
            .as_ref()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        &mut p,
    );
    p.insert("names".to_owned(), string_map(app.names.clone()));
    p.insert("keywords".to_owned(), strings(&app.keywords));
    p.insert(
        "keywords_l10n".to_owned(),
        string_map(
            app.keywords_l10n
                .iter()
                .map(|(lang, words)| (lang.clone(), words.join(";"))),
        ),
    );
    p.insert("categories".to_owned(), strings(&app.categories));
    p.insert("mime_types".to_owned(), strings(&app.mime_types));
    p.insert("url_schemes".to_owned(), strings(&app.url_schemes));
    p.insert(
        "types".to_owned(),
        string_map(
            app.types
                .iter()
                .map(|(m, r)| (m.clone(), r.as_str().to_owned())),
        ),
    );
    p.insert(
        "document_icons".to_owned(),
        string_map(app.document_icons.iter().cloned()),
    );
    let actions: Vec<(String, String, String)> = app
        .actions
        .iter()
        .map(|a| (a.id.clone(), a.name.clone(), a.exec.clone()))
        .collect();
    p.insert("actions".to_owned(), Value::from(actions));
    if let Some(perms) = &app.permissions {
        let mut map: HashMap<String, Value<'static>> = HashMap::new();
        map.insert("network".to_owned(), Value::from(perms.network));
        map.insert(
            "devices".to_owned(),
            Value::from(perms.devices.as_str().to_owned()),
        );
        map.insert("filesystem".to_owned(), strings(&perms.filesystem));
        map.insert("talk".to_owned(), strings(&perms.talk));
        map.insert("background".to_owned(), Value::from(perms.background));
        p.insert("permissions".to_owned(), Value::from(map));
    }
    p.insert("no_display".to_owned(), Value::from(app.no_display));
    p.insert("terminal".to_owned(), Value::from(app.terminal));
    p.insert("share".to_owned(), Value::from(app.share));
    p.insert("new".to_owned(), Value::from(db.is_new(&app.id)));
    let record = db.records.get(&app.id).copied().unwrap_or_default();
    p.insert("added".to_owned(), Value::from(record.added));
    p.insert("last_used".to_owned(), Value::from(record.last_used));
    p
}

/// Starts `argv` detached (its own process group), reaped in the background. Returns the pid.
fn spawn(argv: &[String], token: Option<&str>) -> Result<u32, String> {
    let (program, args) = argv.split_first().ok_or("empty command")?;
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .process_group(0);
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        command.env("XDG_ACTIVATION_TOKEN", token);
    }
    let mut child = command.spawn().map_err(|e| format!("{program}: {e}"))?;
    let pid = child.id();
    let _ = std::thread::Builder::new()
        .name("app-child".to_owned())
        .spawn(move || {
            let _ = child.wait();
        });
    Ok(pid)
}

// -------------------------------------------------------------------------------- service

/// The `Apps1` service: the bus object and the watcher thread.
pub struct AppsService {
    connection: Connection,
    changes: async_channel::Receiver<Vec<Change>>,
}

impl AppsService {
    /// Reads the registry, serves it at [`APPS_PATH`] and starts watching.
    pub async fn serve(connection: Connection, config: AppsConfig) -> zbus::Result<AppsService> {
        let (tx, rx) = async_channel::unbounded();
        let db = Db::load(&config.db_path).unwrap_or_else(|| Db::new(now()));
        let mime = MimeDb::load(&config.mime_dirs);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                registry: Arc::new(Registry::default()),
                db,
                compat: BTreeMap::new(),
            }),
            config,
            mime,
            changes: tx,
        });
        // The first scan: everything is "added", which nobody needs to hear about.
        let first = Arc::clone(&shared);
        blocking::unblock(move || {
            first.refresh();
        })
        .await;
        while rx.try_recv().is_ok() {}
        connection
            .object_server()
            .at(
                APPS_PATH,
                Apps {
                    shared: Arc::clone(&shared),
                },
            )
            .await?;
        let watcher = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("apps-watch".to_owned())
            .spawn(move || watch_loop(watcher))
            .map_err(|e| zbus::Error::Failure(format!("apps watcher: {e}")))?;
        Ok(AppsService {
            connection,
            changes: rx,
        })
    }

    /// Emits the signals of the changes, forever.
    pub async fn run(&self) -> zbus::Result<()> {
        let server = self.connection.object_server();
        let iface = server.interface::<_, Apps>(APPS_PATH).await?;
        while let Ok(changes) = self.changes.recv().await {
            let emitter = iface.signal_emitter();
            for change in changes {
                let _ = match &change {
                    Change::Added(id) => Apps::added(emitter, id).await,
                    Change::Removed(id) => Apps::removed(emitter, id).await,
                    Change::Changed(id) => Apps::changed(emitter, id).await,
                };
            }
        }
        Ok(())
    }
}

/// The bus object.
pub struct Apps {
    shared: Arc<Shared>,
}

type AppTuple = (
    String,
    String,
    String,
    String,
    HashMap<String, Value<'static>>,
);

fn not_found(id: &str) -> fdo::Error {
    fdo::Error::Failed(format!("dev.frascella.Silica.Apps1.Error.NotFound: {id}"))
}

impl Apps {
    fn launch(&self, app: &App, uris: &[String], token: Option<&str>) -> fdo::Result<u32> {
        let file = app.desktop_file.as_ref().map(|p| p.to_string_lossy());
        let lines = exec::launch_lines(
            &app.exec,
            &exec::Context {
                targets: uris,
                icon: Some(app.icon.as_str()),
                name: Some(app.name.as_str()),
                desktop_file: file.as_deref(),
            },
        );
        let mut pid = 0;
        for line in lines {
            pid = spawn(&line, token).map_err(fdo::Error::Failed)?;
        }
        {
            let mut state = self.shared.state();
            state.db.launched(&app.id, now());
            let _ = state.db.save(&self.shared.config.db_path);
        }
        Ok(pid)
    }
}

#[interface(name = "dev.frascella.Silica.Apps1")]
impl Apps {
    /// Every app: id, name, kind, path, properties.
    fn list(&self) -> Vec<AppTuple> {
        let state = self.shared.state();
        state
            .registry
            .apps()
            .iter()
            .map(|a| {
                (
                    a.id.clone(),
                    a.name.clone(),
                    a.kind.as_str().to_owned(),
                    a.path.to_string_lossy().into_owned(),
                    properties(a, &state.db),
                )
            })
            .collect()
    }

    /// One app's properties.
    fn get(&self, id: &str) -> fdo::Result<HashMap<String, Value<'static>>> {
        let state = self.shared.state();
        let app = state.registry.get(id).ok_or_else(|| not_found(id))?;
        Ok(properties(app, &state.db))
    }

    /// The ids of the apps that open `type`, the default first.
    fn apps_for(&self, r#type: &str) -> Vec<String> {
        let chain = self.shared.chain(r#type);
        self.shared
            .registry()
            .apps_for(&chain)
            .iter()
            .map(|a| a.id.clone())
            .collect()
    }

    /// The default app of `type`, `""` when none.
    fn default_for(&self, r#type: &str) -> String {
        let chain = self.shared.chain(r#type);
        self.shared
            .registry()
            .default_for(&chain)
            .map(|a| a.id.clone())
            .unwrap_or_default()
    }

    /// Makes `id` the default app of `type` in the user's `mimeapps.list`.
    async fn set_default(
        &self,
        r#type: &str,
        id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        if !r#type.contains('/') || r#type.contains(['\n', '=', '[']) {
            return Err(fdo::Error::InvalidArgs(format!("not a type: {type}")));
        }
        let registry = self.shared.registry();
        let app = registry.get(id).ok_or_else(|| not_found(id))?;
        let desktop_id = app.desktop_id();
        let file = registry.dirs().user_mimeapps();
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let updated = silica_apps::mimeapps::with_default(&text, r#type, &desktop_id);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| fdo::Error::Failed(e.to_string()))?;
        }
        let tmp = file.with_extension("list.tmp");
        std::fs::write(&tmp, updated)
            .and_then(|()| std::fs::rename(&tmp, &file))
            .map_err(|e| fdo::Error::Failed(format!("{}: {e}", file.display())))?;
        let shared = Arc::clone(&self.shared);
        blocking::unblock(move || {
            shared.refresh();
        })
        .await;
        let app_id = app.id.clone();
        let _ = Apps::default_changed(&emitter, r#type, &app_id).await;
        Ok(())
    }

    /// Starts `id` with `uris`; returns the pid (of the last process when the app takes one
    /// file per run).
    fn open(
        &self,
        id: &str,
        uris: Vec<String>,
        options: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> fdo::Result<u32> {
        let registry = self.shared.registry();
        let app = registry.get(id).ok_or_else(|| not_found(id))?;
        let token = options
            .get("activation_token")
            .and_then(|v| String::try_from(v.clone()).ok());
        self.launch(app, &uris, token.as_deref())
    }

    /// Opens each URI with the default app of its type (one launch per app).
    fn open_uris(&self, uris: Vec<String>) -> fdo::Result<u32> {
        let registry = self.shared.registry();
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for uri in uris {
            let Some(kind) = self.shared.type_of(&uri) else {
                return Err(fdo::Error::InvalidArgs(format!("not a URI: {uri}")));
            };
            let chain = self.shared.chain(&kind);
            let app = registry
                .default_for(&chain)
                .ok_or_else(|| fdo::Error::Failed(format!("no app opens {kind}")))?;
            match groups.iter_mut().find(|(id, _)| *id == app.id) {
                Some((_, list)) => list.push(uri),
                None => groups.push((app.id.clone(), vec![uri])),
            }
        }
        let mut pid = 0;
        for (id, list) in groups {
            let app = registry.get(&id).ok_or_else(|| not_found(&id))?;
            pid = self.launch(app, &list, None)?;
        }
        Ok(pid)
    }

    /// The shell started `id` itself: remember the launch (the app is no longer new).
    async fn note_launched(
        &self,
        id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let was_new = {
            let mut state = self.shared.state();
            let Some(app_id) = state.registry.get(id).map(|a| a.id.clone()) else {
                return Err(not_found(id));
            };
            let was_new = state.db.is_new(&app_id);
            state.db.launched(&app_id, now());
            let _ = state.db.save(&self.shared.config.db_path);
            was_new.then_some(app_id)
        };
        if let Some(app_id) = was_new {
            let _ = Apps::changed(&emitter, &app_id).await;
        }
        Ok(())
    }

    /// Removes a Flatpak app (`flatpak uninstall`). Other kinds are files the file manager moves
    /// to the Trash: false.
    async fn uninstall(&self, id: &str) -> fdo::Result<bool> {
        let registry = self.shared.registry();
        let app = registry.get(id).ok_or_else(|| not_found(id))?;
        if app.kind != Kind::Flatpak {
            return Ok(false);
        }
        let home = registry.dirs().data_home.clone();
        let installation = if app.path.starts_with(&home) {
            "--user"
        } else {
            "--system"
        };
        let flatpak_id = app.id.clone();
        let status = blocking::unblock(move || {
            std::process::Command::new("flatpak")
                .args([
                    "uninstall",
                    "--noninteractive",
                    "-y",
                    installation,
                    &flatpak_id,
                ])
                .stdin(std::process::Stdio::null())
                .status()
        })
        .await
        .map_err(|e| fdo::Error::Failed(format!("flatpak: {e}")))?;
        Ok(status.success())
    }

    #[zbus(signal)]
    async fn added(emitter: &SignalEmitter<'_>, id: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn removed(emitter: &SignalEmitter<'_>, id: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, id: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn default_changed(
        emitter: &SignalEmitter<'_>,
        r#type: &str,
        id: &str,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watch_plan_climbs_to_existing_parents() {
        let root = std::env::temp_dir().join(format!("apps-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home/.local/share/applications/kde")).unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        let dirs = Dirs {
            config_home: root.join("config"),
            data_home: root.join("home/.local/share"),
            app_folders: vec![(root.join("home/Applications"), silica_apps::Location::User)],
            ..Dirs::default()
        };
        let plan = watch_plan(&Registry::scan(dirs));
        let find = |p: PathBuf| {
            plan.iter()
                .find(|(path, _)| *path == p)
                .map(|(_, f)| f.clone())
        };
        // ~/Applications does not exist: home is watched for that name.
        assert!(matches!(
            find(root.join("home")),
            Some(Filter::Names(n)) if n.contains("Applications")
        ));
        assert!(matches!(
            find(root.join("home/.local/share/applications")),
            Some(Filter::Any)
        ));
        assert!(matches!(
            find(root.join("home/.local/share/applications/kde")),
            Some(Filter::Any)
        ));
        assert!(matches!(
            find(root.join("config")),
            Some(Filter::Names(n)) if n.contains("mimeapps.list")
        ));
        let _ = std::fs::remove_dir_all(&root);
    }
}
