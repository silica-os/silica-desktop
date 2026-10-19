//! What the Dock holds (docs/spec/dock.md D-00, D-08): the file manager first, the pinned apps
//! (dock.json `pinned`; the Apps item opens the Launcher; an app no longer installed shows as
//! missing), the open apps that are not pinned, with recent apps their section between two
//! separators, the separator, the folders (dock.json `folders`, the Downloads folder by
//! default), the minimised windows and the Trash; and the tiles' slot animations.

use std::path::PathBuf;

use crate::apps::{self, DesktopEntry};

use super::geometry::{GROW, GROW_FROM, GROW_TIME, SHRINK, SHRINK_TIME};

/// The file manager: always first, never removed or dragged (#141).
pub const FILE_MANAGER: &str = "dev.frascella.Mustache";
/// The Apps item: the Launcher (DOCK-02).
pub const APPS_ID: &str = "dev.frascella.Apps";
/// The most recent apps shown with "Show suggested and recent apps" (D-08).
pub const RECENTS_MAX: usize = 3;

/// Apps pinned when the configuration has none: the first installed of each group.
pub const DEFAULT_PINNED: &[&[&str]] = &[
    &[FILE_MANAGER, "org.gnome.Nautilus"],
    &[APPS_ID],
    &[
        "firefox_firefox",
        "firefox",
        "org.mozilla.firefox",
        "chromium_chromium",
        "google-chrome",
    ],
    &["dev.frascella.Mail"],
    &[
        "dev.frascella.Terminal",
        "org.gnome.Ptyxis",
        "org.gnome.Terminal",
    ],
    &["dev.frascella.TextEditor", "org.gnome.TextEditor"],
    &["dev.frascella.Calculator", "org.gnome.Calculator"],
    &["dev.frascella.Clock", "org.gnome.clocks"],
    &["dev.frascella.Settings", "org.gnome.Settings"],
];

/// What a tile holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// An app.
    App {
        /// Its entry.
        entry: DesktopEntry,
        /// Kept in the Dock.
        pinned: bool,
        /// The file manager: first and fixed.
        fixed: bool,
    },
    /// The Apps item (the Launcher).
    Apps,
    /// A pinned app that is not installed (a question mark; its menu removes it).
    Missing(String),
    /// A separator (the second one bounds the recent apps).
    Separator,
    /// A folder stack.
    Folder(PathBuf),
    /// A minimised window.
    Window {
        /// Its identifier.
        id: String,
        /// Its app.
        app: String,
        /// Its title.
        title: String,
    },
    /// The Trash.
    Trash,
}

impl Kind {
    /// The tile's stable identity.
    pub fn key(&self) -> String {
        match self {
            Kind::App { entry, .. } => format!("app:{}", entry.id),
            Kind::Apps => "apps".into(),
            Kind::Missing(id) => format!("missing:{id}"),
            Kind::Separator => "sep".into(),
            Kind::Folder(p) => format!("folder:{}", p.display()),
            Kind::Window { id, .. } => format!("win:{id}"),
            Kind::Trash => "trash".into(),
        }
    }

    /// The id kept in dock.json `pinned`, for items that can be pinned.
    pub fn pin_id(&self) -> Option<&str> {
        match self {
            Kind::App { entry, .. } => Some(&entry.id),
            Kind::Apps => Some(APPS_ID),
            Kind::Missing(id) => Some(id),
            _ => None,
        }
    }

    /// Pinned (or fixed).
    pub fn pinned(&self) -> bool {
        match self {
            Kind::App { pinned, fixed, .. } => *pinned || *fixed,
            Kind::Apps | Kind::Missing(_) => true,
            _ => false,
        }
    }

    /// The app id of an app tile.
    pub fn app_id(&self) -> Option<&str> {
        match self {
            Kind::App { entry, .. } => Some(&entry.id),
            _ => None,
        }
    }

    /// The name its label shows.
    pub fn label(&self) -> Option<String> {
        match self {
            Kind::App { entry, .. } => Some(entry.name.clone()),
            Kind::Apps => Some("Apps".into()),
            Kind::Missing(id) => Some(format!(
                "{}, not installed",
                id.rsplit('.').next().unwrap_or(id)
            )),
            Kind::Folder(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()),
            Kind::Window { title, .. } => Some(title.clone()),
            Kind::Trash => Some("Trash".into()),
            Kind::Separator => None,
        }
    }

    /// It can be dragged (D-05): apps but the file manager, folders, the Apps and missing items.
    pub fn draggable(&self) -> bool {
        match self {
            Kind::App { fixed, .. } => !fixed,
            Kind::Apps | Kind::Missing(_) | Kind::Folder(_) => true,
            _ => false,
        }
    }
}

/// A tile with its slot animation.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    /// What it holds.
    pub kind: Kind,
    /// When it began growing in.
    pub born: Option<f64>,
    /// When it began shrinking out (it goes at the end).
    pub dying: Option<f64>,
    /// When the app was launched from the Dock (it bounces until a window shows).
    pub launched: Option<f64>,
}

impl Tile {
    /// A tile at rest.
    pub fn new(kind: Kind) -> Tile {
        Tile {
            kind,
            born: None,
            dying: None,
            launched: None,
        }
    }

    /// Its size factor from growing in or shrinking out, and its icon's opacity.
    pub fn presence(&self, now: f64) -> (f32, f32) {
        if let Some(d) = self.dying {
            let s = 1.0 - SHRINK.at(((now - d) / SHRINK_TIME) as f32);
            return (s, s * s);
        }
        if let Some(b) = self.born {
            let s = GROW_FROM + (1.0 - GROW_FROM) * GROW.at(((now - b) / GROW_TIME) as f32);
            return (s, ((s - 0.2) / 0.8).clamp(0.0, 1.0));
        }
        (1.0, 1.0)
    }

    /// Still animating its slot.
    pub fn moving(&self, now: f64) -> bool {
        self.born.is_some_and(|b| now - b < GROW_TIME) || self.dying.is_some()
    }
}

/// An open window.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    /// Its identifier.
    pub id: String,
    /// Its app.
    pub app: String,
    /// Its title.
    pub title: String,
}

/// What the composition needs.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    /// Installed apps by id (a closure-free snapshot: tests give their own).
    pub entries: Vec<DesktopEntry>,
    /// dock.json `pinned` (or the defaults).
    pub pinned: Vec<String>,
    /// The folders.
    pub folders: Vec<PathBuf>,
    /// Open windows in the order they appeared.
    pub windows: Vec<Window>,
    /// Minimised windows, the first minimised first.
    pub minimized: Vec<String>,
    /// Minimised windows stay in their app's icon.
    pub minimize_to_app: bool,
    /// Recently closed apps (newest first) when recent apps are shown.
    pub recents: Option<Vec<String>>,
}

impl Inputs {
    fn entry(&self, id: &str) -> Option<&DesktopEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The entry a window's app id belongs to.
    fn owner(&self, app: &str) -> Option<DesktopEntry> {
        self.entry(app).cloned().or_else(|| {
            (!app.is_empty()).then(|| DesktopEntry {
                id: app.to_string(),
                name: app.rsplit('.').next().unwrap_or(app).to_string(),
                ..DesktopEntry::default()
            })
        })
    }
}

/// The tiles, in order, for `inp`.
pub fn compose(inp: &Inputs) -> Vec<Kind> {
    let mut items: Vec<Kind> = Vec::new();
    let mut shown: Vec<String> = Vec::new();
    if let Some(fm) = inp.entry(FILE_MANAGER) {
        items.push(Kind::App {
            entry: fm.clone(),
            pinned: true,
            fixed: true,
        });
        shown.push(FILE_MANAGER.into());
    }
    for id in &inp.pinned {
        if shown.contains(id) {
            continue;
        }
        shown.push(id.clone());
        if id == APPS_ID {
            items.push(Kind::Apps);
            continue;
        }
        match inp.entry(id) {
            Some(e) => items.push(Kind::App {
                entry: e.clone(),
                pinned: true,
                fixed: false,
            }),
            None if !inp.entries.is_empty() => items.push(Kind::Missing(id.clone())),
            None => {}
        }
    }
    let before = items.len();
    for w in &inp.windows {
        let Some(e) = inp.owner(&w.app) else { continue };
        if e.no_display || shown.contains(&e.id) {
            continue;
        }
        shown.push(e.id.clone());
        items.push(Kind::App {
            entry: e,
            pinned: false,
            fixed: false,
        });
    }
    if let Some(recents) = &inp.recents {
        for id in recents {
            if items.len() - before >= RECENTS_MAX {
                break;
            }
            let Some(e) = inp.entry(id) else { continue };
            if shown.contains(id) {
                continue;
            }
            shown.push(id.clone());
            items.push(Kind::App {
                entry: e.clone(),
                pinned: false,
                fixed: false,
            });
        }
        if items.len() > before {
            items.insert(before, Kind::Separator);
        }
    }
    items.push(Kind::Separator);
    items.extend(inp.folders.iter().cloned().map(Kind::Folder));
    for id in &inp.minimized {
        let Some(w) = inp.windows.iter().find(|w| &w.id == id) else {
            continue;
        };
        let owner = inp.owner(&w.app).map(|e| e.id);
        let in_app = owner.is_some_and(|o| items.iter().any(|k| k.app_id() == Some(&o)));
        if inp.minimize_to_app && in_app {
            continue;
        }
        items.push(Kind::Window {
            id: w.id.clone(),
            app: w.app.clone(),
            title: if w.title.is_empty() {
                inp.owner(&w.app).map(|e| e.name).unwrap_or_default()
            } else {
                w.title.clone()
            },
        });
    }
    items.push(Kind::Trash);
    // A second separator: keys must stay unique.
    items
}

/// Aligns `old` tiles with the new `kinds` at time `now`: a new item grows in (unless
/// `animate` is false, or it is a minimised window, which has its own effect), a missing one
/// shrinks out in place; finished ones are dropped.
pub fn sync(old: &[Tile], kinds: Vec<Kind>, now: f64, animate: bool) -> Vec<Tile> {
    let keyed: Vec<(String, Kind)> = {
        let mut seen: Vec<String> = Vec::new();
        kinds
            .into_iter()
            .map(|k| {
                let mut key = k.key();
                let mut n = 1;
                while seen.contains(&key) {
                    n += 1;
                    key = format!("{}#{n}", k.key());
                }
                seen.push(key.clone());
                (key, k)
            })
            .collect()
    };
    let key_of = |t: &Tile, i: usize, tiles: &[Tile]| -> String {
        let base = t.kind.key();
        let n = tiles
            .iter()
            .take(i)
            .filter(|x| x.kind.key() == base)
            .count();
        if n == 0 {
            base
        } else {
            format!("{base}#{}", n + 1)
        }
    };
    let old_keys: Vec<String> = old
        .iter()
        .enumerate()
        .map(|(i, t)| key_of(t, i, old))
        .collect();
    let mut out: Vec<(String, Tile)> = Vec::new();
    for (key, kind) in keyed {
        match old_keys
            .iter()
            .position(|k| *k == key)
            .and_then(|i| old.get(i))
        {
            Some(t) => {
                let mut t = t.clone();
                t.kind = kind;
                t.dying = None;
                out.push((key, t));
            }
            None => {
                let mut t = Tile::new(kind);
                if animate && !matches!(t.kind, Kind::Window { .. }) {
                    t.born = Some(now);
                }
                out.push((key, t));
            }
        }
    }
    // Tiles that went shrink out where they were: after the tile that preceded them.
    for (i, (key, t)) in old_keys.iter().zip(old).enumerate() {
        if out.iter().any(|(k, _)| k == key) {
            continue;
        }
        if !animate || matches!(t.kind, Kind::Window { .. }) {
            continue;
        }
        let mut t = t.clone();
        if t.dying.is_none() {
            t.dying = Some(now);
        }
        if t.dying.is_some_and(|d| now - d >= SHRINK_TIME) {
            continue;
        }
        let at = old_keys
            .iter()
            .take(i)
            .rev()
            .find_map(|prev| out.iter().position(|(k, _)| k == prev).map(|p| p + 1))
            .unwrap_or(0);
        out.insert(at.min(out.len()), (format!("{key}~"), t));
    }
    out.into_iter().map(|(_, t)| t).collect()
}

/// The pinned ids that are installed among `defaults` groups.
pub fn default_pinned(entries: &[DesktopEntry]) -> Vec<String> {
    DEFAULT_PINNED
        .iter()
        .filter_map(|group| {
            group
                .iter()
                .find(|id| **id == APPS_ID || entries.iter().any(|e| e.id == **id))
                .map(|id| id.to_string())
        })
        .collect()
}

/// The installed apps the Dock may show (every desktop entry, hidden ones included: a pinned
/// app hidden from menus still has its tile).
pub fn installed() -> Vec<DesktopEntry> {
    apps::all_entries()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: &str) -> DesktopEntry {
        DesktopEntry {
            id: id.into(),
            name: id.rsplit('.').next().unwrap_or(id).into(),
            ..DesktopEntry::default()
        }
    }

    fn w(id: &str, app: &str) -> Window {
        Window {
            id: id.into(),
            app: app.into(),
            title: format!("{app} window"),
        }
    }

    fn keys(kinds: &[Kind]) -> Vec<String> {
        kinds.iter().map(Kind::key).collect()
    }

    #[test]
    fn the_file_manager_is_first_then_pinned_running_folders_windows_trash() {
        let inp = Inputs {
            entries: vec![e(FILE_MANAGER), e("a.Mail"), e("x.Other")],
            pinned: vec![
                APPS_ID.into(),
                "a.Mail".into(),
                "gone.App".into(),
                FILE_MANAGER.into(),
            ],
            folders: vec![PathBuf::from("/home/u/Downloads")],
            windows: vec![w("w1", "x.Other"), w("w2", "a.Mail")],
            minimized: vec!["w1".into()],
            ..Inputs::default()
        };
        assert_eq!(
            keys(&compose(&inp)),
            [
                "app:dev.frascella.Mustache",
                "apps",
                "app:a.Mail",
                "missing:gone.App",
                "app:x.Other",
                "sep",
                "folder:/home/u/Downloads",
                "win:w1",
                "trash"
            ]
        );
        // With "into the app icon" the window stays in its app's icon.
        let into = Inputs {
            minimize_to_app: true,
            ..inp.clone()
        };
        assert!(!keys(&compose(&into)).contains(&"win:w1".to_string()));
        // Recent apps: a second separator bounds the open and recent ones, 3 at most.
        let recent = Inputs {
            recents: Some(vec!["a.Mail".into(), "r.One".into()]),
            entries: vec![e(FILE_MANAGER), e("a.Mail"), e("x.Other"), e("r.One")],
            ..inp
        };
        let k = keys(&compose(&recent));
        assert_eq!(k.iter().filter(|x| *x == "sep").count(), 2);
        assert_eq!(k.get(5).map(String::as_str), Some("app:x.Other"));
        assert_eq!(k.get(6).map(String::as_str), Some("app:r.One"));
    }

    #[test]
    fn defaults_take_the_first_installed_of_each_group() {
        let p = default_pinned(&[
            e("org.gnome.Nautilus"),
            e("firefox"),
            e("dev.frascella.Mail"),
        ]);
        assert_eq!(
            p,
            [
                "org.gnome.Nautilus",
                APPS_ID,
                "firefox",
                "dev.frascella.Mail"
            ]
        );
    }

    #[test]
    fn slots_grow_in_and_shrink_out_in_place() {
        let first = sync(
            &[],
            vec![Kind::Apps, Kind::Separator, Kind::Trash],
            0.0,
            false,
        );
        assert!(first.iter().all(|t| t.born.is_none()));
        let mail = Kind::App {
            entry: e("a.Mail"),
            pinned: false,
            fixed: false,
        };
        let grown = sync(
            &first,
            vec![Kind::Apps, mail.clone(), Kind::Separator, Kind::Trash],
            1.0,
            true,
        );
        assert_eq!(grown[1].born, Some(1.0));
        assert_eq!(grown[1].presence(1.0).0, GROW_FROM);
        assert_eq!(grown[1].presence(1.0 + GROW_TIME).0, 1.0);
        // It quits: the slot shrinks where it was, then goes.
        let quit = sync(
            &grown,
            vec![Kind::Apps, Kind::Separator, Kind::Trash],
            5.0,
            true,
        );
        assert_eq!(quit.len(), 4);
        assert_eq!(quit[1].kind, mail);
        assert_eq!(quit[1].dying, Some(5.0));
        let gone = sync(
            &quit,
            vec![Kind::Apps, Kind::Separator, Kind::Trash],
            5.0 + SHRINK_TIME,
            true,
        );
        assert_eq!(gone.len(), 3);
        // The quit shrinks to 0 in ~480 ms, its icon fading as the square.
        let (s, o) = quit[1].presence(5.0 + SHRINK_TIME / 2.0);
        assert!(s > 0.0 && s < 1.0 && (o - s * s).abs() < 1e-6);
    }

    #[test]
    fn only_movable_items_drag() {
        let fm = Kind::App {
            entry: e(FILE_MANAGER),
            pinned: true,
            fixed: true,
        };
        assert!(!fm.draggable());
        assert!(!Kind::Trash.draggable());
        assert!(!Kind::Separator.draggable());
        assert!(Kind::Folder(PathBuf::from("/x")).draggable());
        assert!(Kind::Apps.draggable());
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore = "reads this machine's apps"]
    fn print_live_composition() {
        let entries = super::installed();
        let pinned = super::default_pinned(&entries);
        eprintln!("pinned {pinned:?}");
        let inp = super::Inputs {
            entries,
            pinned,
            ..super::Inputs::default()
        };
        for k in super::compose(&inp) {
            eprintln!("{}", k.key());
        }
    }
}
