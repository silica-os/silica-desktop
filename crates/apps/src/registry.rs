//! The registry: every installed app from the Applications folders (bundles, AppImages) and the
//! XDG `applications/` folders (desktop entries, Flatpak exports), with the rules of
//! `docs/spec/launch-services.md`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::desktop::{self, Action, DesktopEntry};
use crate::exec;
use crate::info::{AppKind, Info, Permissions, Role};
use crate::mimeapps::MimeApps;

/// The desktop name Silica sets in `XDG_CURRENT_DESKTOP`.
pub const DESKTOP: &str = "Silica";
/// The command that starts bundles and AppImages.
pub const LAUNCHER: &str = "silica-launch";
/// The development VM's system apps (`cargo xtask os deploy --apps`): treated like
/// `/usr/Applications` (root-owned; absent from released images).
pub const DEV_SYSTEM_APPS: &str = "/var/lib/silica-dev/Applications";

/// Whether `path` is a system app: in the image's `/usr`, or the development VM's stand-in.
pub fn is_system_path(path: &Path) -> bool {
    path.starts_with("/usr/") || path.starts_with(DEV_SYSTEM_APPS)
}

/// The folder of the compatibility entries, under `$XDG_DATA_HOME/applications`.
pub const COMPAT_DIR: &str = "silica-bundles";

/// Where an app comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Part of the image: a bundle of `/usr/Applications` or a desktop entry that is not
    /// Flatpak's.
    System,
    /// A bundle of `/Applications` or `~/Applications`.
    Bundle,
    Flatpak,
    AppImage,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::System => "system",
            Kind::Bundle => "bundle",
            Kind::Flatpak => "flatpak",
            Kind::AppImage => "appimage",
        }
    }
}

/// The folder an app lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Location {
    /// `/usr/Applications`.
    System,
    /// `/Applications`.
    Local,
    /// `~/Applications`.
    User,
    /// A mounted disk image.
    Volume,
    /// An XDG `applications/` folder.
    Xdg,
}

impl Location {
    pub fn as_str(self) -> &'static str {
        match self {
            Location::System => "system",
            Location::Local => "local",
            Location::User => "user",
            Location::Volume => "volume",
            Location::Xdg => "xdg",
        }
    }
}

/// An app of the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    /// The app id: the bundle id, or the desktop file id without `.desktop`.
    pub id: String,
    pub name: String,
    pub names: BTreeMap<String, String>,
    pub generic_name: String,
    pub comment: String,
    pub kind: Kind,
    pub location: Location,
    /// The bundle folder, the AppImage file, or the desktop entry file.
    pub path: PathBuf,
    pub version: String,
    /// The command line with its field codes.
    pub exec: String,
    pub icon: String,
    pub silica_icon: String,
    pub wm_class: String,
    pub keywords: Vec<String>,
    pub keywords_l10n: BTreeMap<String, Vec<String>>,
    pub categories: Vec<String>,
    /// The types it opens (editors first for bundles), URL schemes included as
    /// `x-scheme-handler/<scheme>`.
    pub mime_types: Vec<String>,
    pub types: BTreeMap<String, Role>,
    pub url_schemes: Vec<String>,
    pub document_icons: Vec<(String, String)>,
    pub actions: Vec<Action>,
    /// Bundles only.
    pub permissions: Option<Permissions>,
    pub app_kind: Option<AppKind>,
    pub runtime: String,
    pub developer: String,
    pub desktop_file: Option<PathBuf>,
    pub no_display: bool,
    pub terminal: bool,
    pub share: bool,
}

impl App {
    /// The name in `lang`.
    pub fn name_in(&self, lang: &str) -> &str {
        desktop::localized(&self.names, lang).map_or(self.name.as_str(), String::as_str)
    }

    /// The command line for `sh -c`, without field codes.
    pub fn command(&self) -> String {
        exec::strip_field_codes(&self.exec)
    }

    /// The arguments that open `targets` (files or URIs).
    pub fn argv(&self, targets: &[String]) -> Vec<String> {
        let file = self.desktop_file.as_ref().map(|p| p.to_string_lossy());
        exec::expand(
            &self.exec,
            &exec::Context {
                targets,
                icon: Some(self.icon.as_str()),
                name: Some(self.name.as_str()),
                desktop_file: file.as_deref(),
            },
        )
    }

    /// Whether the app declares `mime` (exactly).
    pub fn opens(&self, mime: &str) -> bool {
        self.mime_types.iter().any(|m| m == mime)
    }

    /// The desktop file id under which other desktops know the app (`mimeapps.list`).
    pub fn desktop_id(&self) -> String {
        match self.kind {
            Kind::Bundle | Kind::AppImage => format!("{COMPAT_DIR}-{}.desktop", self.id),
            Kind::System if self.desktop_file.is_none() => {
                format!("{COMPAT_DIR}-{}.desktop", self.id)
            }
            _ => format!("{}.desktop", self.id),
        }
    }

    /// The compatibility desktop entry of a bundle or an AppImage.
    pub fn compat_entry(&self) -> String {
        desktop::render(&DesktopEntry {
            name: Some(self.name.clone()),
            names: self.names.clone(),
            generic_name: Some(self.generic_name.clone()),
            comment: Some(self.comment.clone()),
            exec: Some(self.exec.clone()),
            icon: Some(self.icon.clone()),
            silica_icon: Some(self.silica_icon.clone()),
            wm_class: Some(self.wm_class.clone()),
            categories: self.categories.clone(),
            keywords: self.keywords.clone(),
            mime_types: self.mime_types.clone(),
            no_display: self.no_display,
            share: self.share,
            bundle: Some(self.path.to_string_lossy().into_owned()),
            ..DesktopEntry::default()
        })
    }
}

/// The folders the registry reads, injected so tests need no environment.
#[derive(Clone, Debug, Default)]
pub struct Dirs {
    pub config_home: PathBuf,
    pub config_dirs: Vec<PathBuf>,
    pub data_home: PathBuf,
    /// `XDG_DATA_DIRS`, with the Flatpak exports added when missing.
    pub data_dirs: Vec<PathBuf>,
    /// The Applications folders, most important first.
    pub app_folders: Vec<(PathBuf, Location)>,
    /// The Flatpak export folders (`…/exports/share`): entries under them are Flatpak apps.
    pub flatpak_exports: Vec<PathBuf>,
    /// The cache of AppImages' embedded desktop entries and icons
    /// (`$XDG_CACHE_HOME/silica/appimages`); None: they are named after their file.
    pub appimage_cache: Option<PathBuf>,
    /// Whether a scan extracts what the cache lacks (`unsquashfs`): the daemon's registry does;
    /// other readers (Ossido, `silica-launch`) only read the cache.
    pub extract_appimages: bool,
}

impl Dirs {
    /// The folders of the session (`HOME`, the XDG variables).
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Dirs {
        let home = env("HOME").unwrap_or_default();
        let absolute = |key: &str| env(key).filter(|v| v.starts_with('/'));
        let list = |value: Option<String>, default: &str| -> Vec<PathBuf> {
            value
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_owned())
                .split(':')
                .filter(|d| d.starts_with('/'))
                .map(PathBuf::from)
                .collect()
        };
        let mut data_dirs = list(env("XDG_DATA_DIRS"), "/usr/local/share:/usr/share");
        let mut flatpak_exports = Vec::new();
        for flatpak in [
            format!("{home}/.local/share/flatpak/exports/share"),
            "/var/lib/flatpak/exports/share".to_owned(),
        ] {
            if !flatpak.starts_with('/') {
                continue;
            }
            let path = PathBuf::from(&flatpak);
            if !data_dirs.iter().any(|d| d == &path) {
                data_dirs.push(path.clone());
            }
            flatpak_exports.push(path);
        }
        let mut app_folders = Vec::new();
        if home.starts_with('/') {
            app_folders.push((
                PathBuf::from(format!("{home}/Applications")),
                Location::User,
            ));
        }
        app_folders.push((PathBuf::from("/Applications"), Location::Local));
        // The development VM's stand-in for /usr/Applications (`cargo xtask os deploy`).
        app_folders.push((PathBuf::from(DEV_SYSTEM_APPS), Location::System));
        app_folders.push((PathBuf::from("/usr/Applications"), Location::System));
        Dirs {
            config_home: absolute("XDG_CONFIG_HOME")
                .map_or_else(|| PathBuf::from(format!("{home}/.config")), PathBuf::from),
            config_dirs: list(env("XDG_CONFIG_DIRS"), "/etc/xdg"),
            data_home: absolute("XDG_DATA_HOME").map_or_else(
                || PathBuf::from(format!("{home}/.local/share")),
                PathBuf::from,
            ),
            data_dirs,
            app_folders,
            flatpak_exports,
            appimage_cache: Some(
                absolute("XDG_CACHE_HOME")
                    .map_or_else(|| PathBuf::from(format!("{home}/.cache")), PathBuf::from)
                    .join("silica/appimages"),
            ),
            extract_appimages: false,
        }
    }

    /// The `applications/` folders, most important first.
    pub fn application_dirs(&self) -> Vec<PathBuf> {
        std::iter::once(&self.data_home)
            .chain(&self.data_dirs)
            .map(|d| d.join("applications"))
            .collect()
    }

    /// The `mimeapps.list` files, most specific first.
    pub fn mimeapps_files(&self) -> Vec<PathBuf> {
        let mut files = vec![self.config_home.join("mimeapps.list")];
        files.extend(self.config_dirs.iter().map(|d| d.join("mimeapps.list")));
        files.extend(
            self.application_dirs()
                .iter()
                .map(|d| d.join("mimeapps.list")),
        );
        files
    }

    /// The user's `mimeapps.list`, the one `SetDefault` writes.
    pub fn user_mimeapps(&self) -> PathBuf {
        self.config_home.join("mimeapps.list")
    }

    /// The folder of the compatibility entries.
    pub fn compat_dir(&self) -> PathBuf {
        self.data_home.join("applications").join(COMPAT_DIR)
    }

    /// The folders a watcher follows: the Applications folders and the `applications/` folders.
    pub fn watched(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.app_folders.iter().map(|(p, _)| p.clone()).collect();
        dirs.extend(self.application_dirs());
        dirs.push(self.config_home.clone());
        dirs
    }
}

/// The apps and the associations.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    apps: Vec<App>,
    index: HashMap<String, usize>,
    mimeapps: MimeApps,
    dirs: Dirs,
}

/// Whether a file name is an AppImage's.
fn appimage_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".appimage")
}

/// The app of a bundle folder, or None when it is not a valid bundle.
pub fn bundle_app(path: &Path, location: Location) -> Option<App> {
    let info = Info::read(path).ok()?;
    Some(app_of_info(&info, path, location))
}

/// The app of a parsed `Info.toml`.
pub fn app_of_info(info: &Info, path: &Path, location: Location) -> App {
    let icon_dir = Info::icon_dir(path);
    let png = icon_dir.join("icon.png");
    let silica_icon = if icon_dir.join("icon.assets").is_file() || png.is_file() {
        icon_dir.to_string_lossy().into_owned()
    } else {
        String::new()
    };
    let kind = if location == Location::System {
        Kind::System
    } else {
        Kind::Bundle
    };
    App {
        id: info.id.clone(),
        name: info.name.clone(),
        names: info.names.clone(),
        generic_name: info.generic_name.clone(),
        comment: info.comment.clone(),
        kind,
        location,
        path: path.to_path_buf(),
        version: info.version.clone(),
        exec: format!("{LAUNCHER} {} %U", exec::quote(&path.to_string_lossy())),
        icon: if png.is_file() {
            png.to_string_lossy().into_owned()
        } else {
            String::new()
        },
        silica_icon,
        wm_class: info.id.clone(),
        keywords: info.keywords.clone(),
        keywords_l10n: BTreeMap::new(),
        categories: info.categories.clone(),
        mime_types: info.mime_types(),
        types: info.types.clone(),
        url_schemes: info.url_schemes.clone(),
        document_icons: info
            .documents
            .iter()
            .map(|(m, n)| (m.clone(), n.clone()))
            .collect(),
        actions: Vec::new(),
        permissions: Some(info.permissions.clone()),
        app_kind: Some(info.kind),
        runtime: info.runtime.to_string(),
        developer: info.developer.name.clone(),
        desktop_file: None,
        no_display: false,
        terminal: false,
        share: info.share,
    }
}

/// The app of an AppImage file, with what it embeds when known: its desktop entry (name,
/// localized names, comment, categories, keywords, types, window class, and its id when the
/// entry's file name is a reverse-DNS id) and its icon.
pub fn appimage_app(
    path: &Path,
    location: Location,
    embedded: Option<&crate::appimage::Embedded>,
) -> App {
    let mut app = appimage_plain(path, location);
    let Some(e) = embedded else {
        return app;
    };
    let entry = &e.entry;
    if let Some(name) = entry.name.as_deref().filter(|n| !n.trim().is_empty()) {
        app.name = name.to_owned();
    }
    app.names.clone_from(&entry.names);
    app.generic_name = entry.generic_name.clone().unwrap_or_default();
    app.comment = entry.comment.clone().unwrap_or_default();
    app.categories.clone_from(&entry.categories);
    app.keywords.clone_from(&entry.keywords);
    app.keywords_l10n.clone_from(&entry.keywords_l10n);
    app.mime_types.clone_from(&entry.mime_types);
    app.url_schemes = entry
        .mime_types
        .iter()
        .filter_map(|m| m.strip_prefix("x-scheme-handler/"))
        .map(str::to_owned)
        .collect();
    app.wm_class = entry.wm_class.clone().unwrap_or_default();
    if let Some(icon) = &e.icon {
        app.icon = icon.to_string_lossy().into_owned();
    }
    if crate::info::valid_bundle_id(&e.desktop_id) {
        app.id.clone_from(&e.desktop_id);
    }
    app
}

fn appimage_plain(path: &Path, location: Location) -> App {
    let name = crate::appimage::display_name(path);
    App {
        id: crate::appimage::id_for(path),
        name,
        names: BTreeMap::new(),
        generic_name: String::new(),
        comment: String::new(),
        kind: Kind::AppImage,
        location,
        path: path.to_path_buf(),
        version: String::new(),
        exec: format!("{LAUNCHER} {} %U", exec::quote(&path.to_string_lossy())),
        icon: String::new(),
        silica_icon: String::new(),
        wm_class: String::new(),
        keywords: Vec::new(),
        keywords_l10n: BTreeMap::new(),
        categories: Vec::new(),
        mime_types: Vec::new(),
        types: BTreeMap::new(),
        url_schemes: Vec::new(),
        document_icons: Vec::new(),
        actions: Vec::new(),
        permissions: None,
        app_kind: None,
        runtime: String::new(),
        developer: String::new(),
        desktop_file: None,
        no_display: false,
        terminal: false,
        share: false,
    }
}

/// The app of a desktop entry.
pub fn entry_app(id: &str, entry: &DesktopEntry, file: &Path, flatpak: bool) -> App {
    let url_schemes = entry
        .mime_types
        .iter()
        .filter_map(|m| m.strip_prefix("x-scheme-handler/"))
        .map(str::to_owned)
        .collect();
    App {
        id: id.to_owned(),
        name: entry.name.clone().unwrap_or_else(|| id.to_owned()),
        names: entry.names.clone(),
        generic_name: entry.generic_name.clone().unwrap_or_default(),
        comment: entry.comment.clone().unwrap_or_default(),
        kind: if flatpak || entry.flatpak.is_some() {
            Kind::Flatpak
        } else {
            Kind::System
        },
        location: Location::Xdg,
        path: file.to_path_buf(),
        version: String::new(),
        exec: entry.exec.clone().unwrap_or_default(),
        icon: entry.icon.clone().unwrap_or_default(),
        silica_icon: entry.silica_icon.clone().unwrap_or_default(),
        wm_class: entry.wm_class.clone().unwrap_or_default(),
        keywords: entry.keywords.clone(),
        keywords_l10n: entry.keywords_l10n.clone(),
        categories: entry.categories.clone(),
        mime_types: entry.mime_types.clone(),
        types: BTreeMap::new(),
        url_schemes,
        document_icons: entry.document_icons.clone(),
        actions: entry.actions.clone(),
        permissions: None,
        app_kind: None,
        runtime: String::new(),
        developer: String::new(),
        desktop_file: Some(file.to_path_buf()),
        no_display: entry.no_display || !entry.shown_in(DESKTOP),
        terminal: entry.terminal,
        share: entry.share,
    }
}

/// The desktop entries under `root`, recursively: (id without `.desktop`, file).
fn entries_in(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for item in read.flatten() {
            let path = item.path();
            let Ok(kind) = item.file_type() else { continue };
            // Symbolic links to folders are followed like the specification's readers do, but
            // only one level of folders deep (no cycles).
            if kind.is_dir() || (kind.is_symlink() && path.is_dir()) {
                if dir == root {
                    stack.push(path);
                }
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy();
            if let Some(stem) = rel.strip_suffix(".desktop") {
                out.push((stem.replace('/', "-"), path.clone()));
            }
        }
    }
    out.sort();
    out
}

impl Registry {
    /// Reads every source.
    pub fn scan(dirs: Dirs) -> Registry {
        let mut apps: Vec<App> = Vec::new();
        let mut taken: HashSet<String> = HashSet::new();
        let mut bundle_paths: HashSet<PathBuf> = HashSet::new();
        // 1. The Applications folders, most important first; a folder listed twice (a link)
        //    is read once.
        let mut seen_folders: HashSet<PathBuf> = HashSet::new();
        for (folder, location) in &dirs.app_folders {
            let real = fs::canonicalize(folder).unwrap_or_else(|_| folder.clone());
            if !seen_folders.insert(real) {
                continue;
            }
            let Ok(read) = fs::read_dir(folder) else {
                continue;
            };
            let mut items: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
            items.sort();
            for path in items {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.starts_with('.') {
                    continue;
                }
                let app = if name.ends_with(".app") && path.is_dir() {
                    bundle_app(&path, *location)
                } else if appimage_name(&name)
                    && path.is_file()
                    && crate::appimage::is_appimage(&path)
                {
                    let embedded = dirs.appimage_cache.as_deref().and_then(|cache| {
                        if dirs.extract_appimages {
                            crate::appimage::extract(cache, &path)
                        } else {
                            crate::appimage::cached(cache, &path)
                        }
                    });
                    Some(appimage_app(&path, *location, embedded.as_ref()))
                } else {
                    None
                };
                if let Some(app) = app
                    && taken.insert(app.id.clone())
                {
                    bundle_paths.insert(app.path.clone());
                    apps.push(app);
                }
            }
        }
        // 2. The desktop entries: the first folder wins, even when the entry is hidden there.
        let mut entry_ids: HashSet<String> = HashSet::new();
        for root in dirs.application_dirs() {
            let flatpak = dirs.flatpak_exports.iter().any(|e| root.starts_with(e));
            for (id, file) in entries_in(&root) {
                if !entry_ids.insert(id.clone()) {
                    continue;
                }
                let Ok(text) = fs::read_to_string(&file) else {
                    continue;
                };
                let entry = desktop::parse(&text);
                if let Some(bundle) = &entry.bundle {
                    // A compatibility entry: the bundle itself is the app (or it is gone).
                    let bundle = PathBuf::from(bundle);
                    if bundle_paths.contains(&bundle) || !bundle.exists() {
                        continue;
                    }
                    if let Some(app) = bundle_app(&bundle, Location::Xdg)
                        && taken.insert(app.id.clone())
                    {
                        bundle_paths.insert(bundle);
                        apps.push(app);
                    }
                    continue;
                }
                if !entry.is_application() || taken.contains(&id) {
                    continue;
                }
                taken.insert(id.clone());
                apps.push(entry_app(&id, &entry, &file, flatpak));
            }
        }
        apps.sort_by(|a, b| a.id.cmp(&b.id));
        let index = apps
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.clone(), i))
            .collect();
        let mimeapps = MimeApps::load(&dirs.mimeapps_files());
        Registry {
            apps,
            index,
            mimeapps,
            dirs,
        }
    }

    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }

    pub fn apps(&self) -> &[App] {
        &self.apps
    }

    pub fn mimeapps(&self) -> &MimeApps {
        &self.mimeapps
    }

    /// The app `id` (with or without `.desktop`, or with the compatibility prefix).
    pub fn get(&self, id: &str) -> Option<&App> {
        let id = id.strip_suffix(".desktop").unwrap_or(id);
        self.index
            .get(id)
            .or_else(|| self.index.get(id.strip_prefix(&format!("{COMPAT_DIR}-"))?))
            .and_then(|&i| self.apps.get(i))
    }

    /// The app whose bundle or file is `path`.
    pub fn by_path(&self, path: &Path) -> Option<&App> {
        self.apps.iter().find(|a| a.path == path)
    }

    /// The app listed in `mimeapps.list` as `listed` (an id with `.desktop`), if installed.
    fn listed(&self, listed: &str) -> Option<&App> {
        self.get(listed)
    }

    /// The default app for the first type of `chain` (a type and its parents) that has one:
    /// `[Default Applications]`, then `[Added Associations]`, then the apps that declare it.
    pub fn default_for(&self, chain: &[String]) -> Option<&App> {
        chain.iter().find_map(|mime| self.default_for_type(mime))
    }

    fn default_for_type(&self, mime: &str) -> Option<&App> {
        for section in [&self.mimeapps.defaults, &self.mimeapps.added] {
            for listed in section.get(mime).into_iter().flatten() {
                if self.mimeapps.is_removed(mime, listed) {
                    continue;
                }
                if let Some(app) = self.listed(listed) {
                    return Some(app);
                }
            }
        }
        self.declaring(mime).into_iter().next()
    }

    /// The apps that declare `mime`, not removed for it: editors before viewers, then by name.
    fn declaring(&self, mime: &str) -> Vec<&App> {
        let mut apps: Vec<&App> = self
            .apps
            .iter()
            .filter(|a| a.opens(mime) && !self.mimeapps.is_removed(mime, &a.desktop_id()))
            .collect();
        apps.sort_by_cached_key(|a| {
            (
                a.types.get(mime).copied().unwrap_or(Role::Editor),
                a.no_display,
                a.name.to_lowercase(),
            )
        });
        apps
    }

    /// The apps that open the first types of `chain` (a type and its parents): the default
    /// first, then the added associations, then the apps that declare each type.
    pub fn apps_for(&self, chain: &[String]) -> Vec<&App> {
        let mut out: Vec<&App> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        if let Some(app) = self.default_for(chain) {
            seen.insert(app.id.as_str());
            out.push(app);
        }
        for mime in chain {
            for listed in self.mimeapps.added.get(mime).into_iter().flatten() {
                if self.mimeapps.is_removed(mime, listed) {
                    continue;
                }
                if let Some(app) = self.listed(listed)
                    && seen.insert(app.id.as_str())
                {
                    out.push(app);
                }
            }
            for app in self.declaring(mime) {
                if seen.insert(app.id.as_str()) {
                    out.push(app);
                }
            }
        }
        out
    }

    /// The apps that opt into the Share menu, by name.
    pub fn share_targets(&self) -> Vec<&App> {
        let mut apps: Vec<&App> = self
            .apps
            .iter()
            .filter(|a| a.share && !a.no_display)
            .collect();
        apps.sort_by_cached_key(|a| a.name.to_lowercase());
        apps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("registry-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    pub fn bundle(dir: &Path, name: &str, id: &str, extra: &str) -> PathBuf {
        let path = dir.join(format!("{name}.app"));
        write(
            &path.join("Contents/Info.toml"),
            &format!(
                "format = 1\nid = \"{id}\"\nname = \"{name}\"\nversion = \"1.0\"\nbuild = 1\n\
                 kind = \"native\"\nruntime = \"none\"\nexec = \"bin/{name}\"\n{extra}\n\
                 [developer]\nname = \"Dev\"\nkey = \"RWQ\"\n"
            ),
        );
        write(&path.join("Contents/Resources/icon/icon.png"), "png");
        path
    }

    pub fn dirs(root: &Path) -> Dirs {
        Dirs {
            config_home: root.join("config"),
            config_dirs: vec![],
            data_home: root.join("home/.local/share"),
            data_dirs: vec![root.join("usr/share"), root.join("flatpak/exports/share")],
            app_folders: vec![
                (root.join("home/Applications"), Location::User),
                (root.join("Applications"), Location::Local),
                (root.join("usr/Applications"), Location::System),
            ],
            flatpak_exports: vec![root.join("flatpak/exports/share")],
            appimage_cache: Some(root.join("cache/appimages")),
            extract_appimages: true,
        }
    }

    #[test]
    fn every_source_is_read_with_its_kind() {
        let root = tmp("sources");
        bundle(
            &root.join("Applications"),
            "Hello",
            "dev.example.Hello",
            "[types]\n\"text/x-hello\" = \"editor\"\n",
        );
        bundle(
            &root.join("usr/Applications"),
            "Calc",
            "dev.frascella.Calculator",
            "",
        );
        // The same id in ~/Applications wins over /Applications.
        bundle(
            &root.join("home/Applications"),
            "Hello Mine",
            "dev.example.Hello",
            "",
        );
        // An invalid bundle is ignored.
        write(
            &root.join("Applications/Broken.app/Contents/Info.toml"),
            "format = 1\n",
        );
        // An AppImage (header only) and a file that only has the name.
        let mut appimage = b"\x7fELF\x02\x01\x01\0AI\x02".to_vec();
        appimage.resize(64, 0);
        fs::write(
            root.join("Applications/Tool-1.0-x86_64.AppImage"),
            &appimage,
        )
        .unwrap();
        fs::write(root.join("Applications/Fake.AppImage"), b"#!/bin/sh\n").unwrap();
        write(
            &root.join("usr/share/applications/org.gnome.Gedit.desktop"),
            "[Desktop Entry]\nType=Application\nName=Gedit\nExec=gedit %U\nMimeType=text/plain;\n",
        );
        write(
            &root.join("flatpak/exports/share/applications/org.mozilla.firefox.desktop"),
            "[Desktop Entry]\nType=Application\nName=Firefox\nExec=/usr/bin/flatpak run org.mozilla.firefox @@u %u @@\nMimeType=text/html;x-scheme-handler/https;\nX-Flatpak=org.mozilla.firefox\n",
        );
        // A user entry hides the system one with Hidden=true.
        write(
            &root.join("usr/share/applications/hidden.desktop"),
            "[Desktop Entry]\nType=Application\nName=H\nExec=h\n",
        );
        write(
            &root.join("home/.local/share/applications/hidden.desktop"),
            "[Desktop Entry]\nType=Application\nHidden=true\n",
        );
        // The compatibility entry of the Hello bundle is not a second app; a stale one is
        // dropped.
        write(
            &root.join("home/.local/share/applications/silica-bundles/dev.example.Hello.desktop"),
            &format!(
                "[Desktop Entry]\nType=Application\nName=Hello\nExec=x\nX-Silica-Bundle={}\n",
                root.join("home/Applications/Hello Mine.app").display()
            ),
        );
        write(
            &root.join("home/.local/share/applications/silica-bundles/dev.gone.App.desktop"),
            "[Desktop Entry]\nType=Application\nName=Gone\nExec=x\nX-Silica-Bundle=/nonexistent/Gone.app\n",
        );
        let reg = Registry::scan(dirs(&root));
        let ids: Vec<&str> = reg.apps().iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "appimage.Tool",
                "dev.example.Hello",
                "dev.frascella.Calculator",
                "org.gnome.Gedit",
                "org.mozilla.firefox"
            ]
        );
        let hello = reg.get("dev.example.Hello").unwrap();
        assert_eq!(hello.name, "Hello Mine");
        assert_eq!(hello.kind, Kind::Bundle);
        assert_eq!(hello.location, Location::User);
        assert!(hello.exec.starts_with("silica-launch \""));
        assert!(hello.exec.ends_with("Hello Mine.app\" %U"));
        assert!(hello.silica_icon.ends_with("Contents/Resources/icon"));
        assert_eq!(
            reg.get("dev.frascella.Calculator").unwrap().kind,
            Kind::System
        );
        assert_eq!(reg.get("org.mozilla.firefox").unwrap().kind, Kind::Flatpak);
        assert_eq!(
            reg.get("org.mozilla.firefox").unwrap().url_schemes,
            ["https"]
        );
        assert_eq!(reg.get("appimage.Tool").unwrap().kind, Kind::AppImage);
        assert_eq!(
            reg.get("org.gnome.Gedit.desktop").unwrap().kind,
            Kind::System
        );
        assert!(
            reg.get("silica-bundles-dev.example.Hello.desktop")
                .is_some()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn associations_follow_mimeapps_and_roles() {
        let root = tmp("assoc");
        let apps = root.join("usr/share/applications");
        write(
            &apps.join("a-viewer.desktop"),
            "[Desktop Entry]\nType=Application\nName=A Viewer\nExec=a %f\nMimeType=text/plain;\n",
        );
        write(
            &apps.join("b-editor.desktop"),
            "[Desktop Entry]\nType=Application\nName=B Editor\nExec=b %f\nMimeType=text/plain;\n",
        );
        write(
            &apps.join("removed.desktop"),
            "[Desktop Entry]\nType=Application\nName=AAA Removed\nExec=r %f\nMimeType=text/plain;\n",
        );
        write(
            &apps.join("extra.desktop"),
            "[Desktop Entry]\nType=Application\nName=Extra\nExec=e %f\n",
        );
        bundle(
            &root.join("Applications"),
            "Viewer",
            "dev.example.Viewer",
            "[types]\n\"text/plain\" = \"viewer\"\n",
        );
        let reg = Registry::scan(dirs(&root));
        let chain = vec!["text/x-log".to_owned(), "text/plain".to_owned()];
        // No mimeapps.list: declared apps, editors (desktop entries count as editors) by name.
        let names: Vec<&str> = reg
            .apps_for(&chain)
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["A Viewer", "AAA Removed", "B Editor", "Viewer"]);
        write(
            &root.join("config/mimeapps.list"),
            "[Default Applications]\ntext/plain=gone.desktop;silica-bundles-dev.example.Viewer.desktop;\n\
             [Added Associations]\ntext/plain=extra.desktop;\n\
             [Removed Associations]\ntext/plain=removed.desktop;\n",
        );
        let reg = Registry::scan(dirs(&root));
        assert_eq!(reg.default_for(&chain).unwrap().id, "dev.example.Viewer");
        let names: Vec<&str> = reg
            .apps_for(&chain)
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["Viewer", "Extra", "A Viewer", "B Editor"]);
        assert!(reg.default_for(&["image/png".to_owned()]).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn compat_entries_point_back_at_the_bundle() {
        let root = tmp("compat");
        let path = bundle(
            &root.join("Applications"),
            "My App",
            "dev.example.My",
            "url_schemes = [\"myapp\"]\n",
        );
        let app = bundle_app(&path, Location::Local).unwrap();
        let entry = desktop::parse(&app.compat_entry());
        assert_eq!(entry.bundle.as_deref(), Some(path.to_str().unwrap()));
        assert_eq!(entry.mime_types, ["x-scheme-handler/myapp"]);
        let argv = exec::expand(
            entry.exec.as_deref().unwrap(),
            &exec::Context {
                targets: &["/tmp/a b".to_owned()],
                ..exec::Context::default()
            },
        );
        assert_eq!(
            argv,
            ["silica-launch", path.to_str().unwrap(), "file:///tmp/a%20b"]
        );
        assert_eq!(app.desktop_id(), "silica-bundles-dev.example.My.desktop");
        let _ = fs::remove_dir_all(&root);
    }
}
