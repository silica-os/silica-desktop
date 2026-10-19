//! The Dock (docs/spec/dock.md for behaviour, dock-stacks.md for folders, the kit's `Dock`
//! masters for layout): a glass shelf at the bottom of the first output.
//!
//! - [`model`]: what it holds and the slots' grow and shrink;
//! - [`geometry`]: sizes at any icon canvas, magnification, the measured curves;
//! - [`view`]: drawing, the pointer (magnification, labels, presses, dragging icons, resizing
//!   from the separator), drops from other apps, auto-hide;
//! - [`menus`]: the context menus;
//! - [`stack`]: folder stacks (pile, fan, grid, list);
//! - [`launcher_entry`]: badges, progress and attention asked over D-Bus.
//!
//! The surface spans the output's width with room above the shelf for magnified, bouncing and
//! dragged icons and the labels; its input region is the shelf and what rises above it, so the
//! rest passes through.

pub mod geometry;
pub mod launcher_entry;
pub mod menus;
pub mod model;
pub mod stack;
pub mod view;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use skia_safe::Image;
use vetro_lib::core::shell::ShellRequest;
use vetro_lib::core::window::{Edges, KeyboardMode, Layer};
use vetro_lib::core::{Prop, Ui, Value, ViewId, WindowId, WindowKind};
use vetro_lib::geom::Size;
use vetro_lib::services::settings::{SettingValue, SettingsMap};

use self::geometry::{BOTTOM_GAP, Layout, Magnify, Metrics, Slot};
use self::launcher_entry::LauncherState;
use self::model::{Inputs, Kind, Tile, Window};

/// Room above the shelf in the surface: magnified, bouncing and dragged icons, labels, the
/// "Remove" capsule.
pub const HEADROOM: f32 = 320.0;

/// The minimise effect (`dock.minimizeEffect`, SH-DK-09; Ossido draws it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Effect {
    /// Genie.
    #[default]
    Genie,
    /// Scale.
    Scale,
}

/// The Dock's settings (`dock.*`, as Settings writes them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Magnification on.
    pub magnify: bool,
    /// The peak canvas with magnification.
    pub magnified: f32,
    /// The resting canvas.
    pub canvas: f32,
    /// Hidden below the screen until the pointer reaches its last row.
    pub autohide: bool,
    /// The dots under open apps.
    pub indicators: bool,
    /// The icon bounces while an app opens.
    pub bounce: bool,
    /// Minimised windows go into their app's icon.
    pub minimize_to_app: bool,
    /// The minimise effect.
    pub effect: Effect,
    /// Recent apps between two separators.
    pub recents: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            magnify: false,
            magnified: 65.0,
            canvas: geometry::DEFAULT_CANVAS,
            autohide: false,
            indicators: true,
            bounce: true,
            minimize_to_app: false,
            effect: Effect::Genie,
            recents: false,
        }
    }
}

impl Options {
    /// From the settings.
    pub fn from_settings(s: &SettingsMap) -> Options {
        let d = Options::default();
        let flag = |k: &str, dflt: bool| s.get(k).and_then(SettingValue::as_bool).unwrap_or(dflt);
        let canvas = s
            .get("dock.tileSize")
            .and_then(SettingValue::as_int)
            .map_or(d.canvas, |v| Metrics::new(v as f32).canvas);
        Options {
            magnify: flag("dock.magnify", d.magnify),
            magnified: s
                .get("dock.magnifiedSize")
                .and_then(SettingValue::as_int)
                .map_or(d.magnified, |v| v as f32)
                .clamp(canvas, geometry::MAX_CANVAS),
            canvas,
            autohide: flag("dock.autohide", d.autohide),
            indicators: flag("dock.indicators", d.indicators),
            bounce: s
                .get("dock.launchAnimation")
                .and_then(SettingValue::as_text)
                .is_none_or(|v| v != "none"),
            minimize_to_app: flag("dock.minimizeToApp", d.minimize_to_app),
            effect: match s.get("dock.minimizeEffect").and_then(SettingValue::as_text) {
                Some("scale") => Effect::Scale,
                _ => Effect::Genie,
            },
            recents: flag("dock.recents", d.recents),
        }
    }

    /// The peak scale.
    pub fn peak(&self) -> f32 {
        if self.magnify {
            self.magnified / self.canvas
        } else {
            1.0
        }
    }
}

/// A slot of the layout: a tile, or a gap where a dragged icon would land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRef {
    /// Tile `i`.
    Tile(usize),
    /// A gap.
    Gap,
}

/// A gap opening or closing (D-05, dock-stacks.md): after the tile with this key (`None`: at
/// the start).
#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    /// The key of the tile before it.
    pub after: Option<String>,
    /// Opening (else closing).
    pub opening: bool,
    /// When the phase started, and its presence then.
    pub since: f64,
    /// See [`Gap::since`].
    pub from: f32,
    /// A gap for a folder dragged in from another app (its own curve).
    pub folder: bool,
}

impl Gap {
    /// Its presence (0–1) at `now`.
    pub fn presence(&self, now: f64) -> f32 {
        let (dur, f): (f64, fn(f32) -> f32) = if self.folder && self.opening {
            (geometry::FOLDER_GAP_TIME, |t| geometry::FOLDER_GAP.at(t))
        } else {
            (geometry::GAP_TIME, geometry::gap_progress)
        };
        let t = (((now - self.since) / dur) as f32).clamp(0.0, 1.0);
        if self.opening {
            self.from + (1.0 - self.from) * f(t)
        } else {
            self.from * (1.0 - f(t))
        }
    }

    /// Closed and finished.
    pub fn gone(&self, now: f64) -> bool {
        !self.opening && now - self.since >= geometry::GAP_TIME
    }

    /// Done moving.
    pub fn settled(&self, now: f64) -> bool {
        let dur = if self.folder && self.opening {
            geometry::FOLDER_GAP_TIME
        } else {
            geometry::GAP_TIME
        };
        now - self.since >= dur
    }
}

/// The Dock's state.
#[derive(Default)]
pub struct DockState {
    /// The tiles, in order.
    pub tiles: Vec<Tile>,
    /// What the tiles are made from.
    pub inputs: Inputs,
    /// Window ids, the most recently focused last.
    pub focus: Vec<String>,
    /// Apps closed recently, the newest first.
    pub closed: Vec<String>,
    /// Thumbnails of minimised windows.
    pub thumbnails: HashMap<String, Image>,
    /// Badges, progress and attention by app id.
    pub launcher: HashMap<String, LauncherState>,
    /// The settings.
    pub options: Options,
    /// The surface and its view.
    pub surface: Option<(WindowId, ViewId)>,
    /// The surface's size.
    pub size: Size,
    /// Gaps opening and closing.
    pub gaps: Vec<Gap>,
    /// The tile being dragged (its key), when a drag is on.
    pub dragging: Option<String>,
    /// The open stack: its tile's key, its popup and view.
    pub stack: Option<(String, WindowId, ViewId)>,
    /// The tile whose menu is open, and the menu.
    pub menu: Option<(usize, WindowId)>,
    /// The Trash has files.
    pub trash_full: bool,
    /// Auto-hide: shown (0–1) from, to, since, duration.
    pub hide: (f32, f32, f64, f64),
    /// Opens the Launcher (the Apps item).
    pub on_launcher: Option<Callback>,
    /// Magnification: progress then, its target, since when; the pointer x it follows.
    pub mag: (f32, f32, f64),
    /// See [`DockState::mag`].
    pub peak_x: f32,
}

/// Runs with the core.
pub type Callback = Rc<dyn Fn(&mut Ui)>;

/// A shared Dock state.
pub type Shared = Rc<RefCell<DockState>>;

impl DockState {
    /// The metrics at the current size (a separator drag shows its own).
    pub fn metrics(&self) -> Metrics {
        Metrics::new(self.options.canvas)
    }

    /// The window of `app` used last.
    pub fn window_of(&self, app: &str) -> Option<String> {
        self.focus
            .iter()
            .rev()
            .find(|w| {
                self.inputs
                    .windows
                    .iter()
                    .any(|x| &x.id == *w && x.app == app)
            })
            .cloned()
            .or_else(|| {
                self.inputs
                    .windows
                    .iter()
                    .rev()
                    .find(|w| w.app == app)
                    .map(|w| w.id.clone())
            })
    }

    /// The windows of `app`.
    pub fn windows_of(&self, app: &str) -> Vec<&Window> {
        self.inputs
            .windows
            .iter()
            .filter(|w| w.app == app)
            .collect()
    }

    /// `app` has windows.
    pub fn running(&self, app: &str) -> bool {
        self.inputs.windows.iter().any(|w| w.app == app)
    }

    /// The auto-hide shown fraction at `now`.
    pub fn shown(&self, now: f64) -> f32 {
        if !self.options.autohide {
            return 1.0;
        }
        let (from, to, since, dur) = self.hide;
        let t = (((now - since) / dur.max(0.001)) as f32).clamp(0.0, 1.0);
        // Up decelerating, down accelerating.
        let e = if to > from {
            1.0 - (1.0 - t).powi(2)
        } else {
            t * t
        };
        from + (to - from) * e
    }

    /// The magnification progress at `now`.
    pub fn mag_at(&self, now: f64) -> f32 {
        let (from, to, since) = self.mag;
        if from == to {
            return to;
        }
        let dur = geometry::enter_duration(self.options.peak());
        let t = (((now - since) / dur) as f32).clamp(0.0, 1.0);
        from + (to - from) * geometry::enter_progress(t)
    }

    /// The shelf's top in the surface at `now`.
    pub fn shelf_y(&self, now: f64) -> f32 {
        let m = self.metrics();
        let hidden = (m.tile_height() + BOTTOM_GAP) * (1.0 - self.shown(now));
        self.size.height - BOTTOM_GAP - m.tile_height() + hidden
    }

    /// The slots at `now`: the tiles (the dragged one left out) with the gaps among them.
    pub fn slots(&self, now: f64) -> (Vec<Slot>, Vec<SlotRef>) {
        let m = self.metrics();
        let mut slots = Vec::new();
        let mut refs = Vec::new();
        let gap_slot = |g: &Gap| Slot {
            presence: g.presence(now),
            flat: true,
            ..Slot::tile(m.tile())
        };
        for g in self.gaps.iter().filter(|g| g.after.is_none()) {
            slots.push(gap_slot(g));
            refs.push(SlotRef::Gap);
        }
        for (i, t) in self.tiles.iter().enumerate() {
            let key = t.kind.key();
            if self.dragging.as_deref() != Some(&key) {
                let (presence, opacity) = t.presence(now);
                let base = match t.kind {
                    Kind::Separator => m.separator(),
                    _ => m.tile(),
                };
                slots.push(Slot {
                    base,
                    presence,
                    opacity,
                    lift: self.lift(t, now) * m.k(),
                    flat: matches!(t.kind, Kind::Separator),
                });
                refs.push(SlotRef::Tile(i));
            }
            for g in self
                .gaps
                .iter()
                .filter(|g| g.after.as_deref() == Some(&key))
            {
                slots.push(gap_slot(g));
                refs.push(SlotRef::Gap);
            }
        }
        (slots, refs)
    }

    /// The bounce or attention hop of a tile.
    fn lift(&self, t: &Tile, now: f64) -> f32 {
        let mut lift = 0.0;
        if let Some(l) = t.launched {
            let still = t.kind.app_id().is_some_and(|a| !self.running(a))
                && now - l < geometry::BOUNCE_LIMIT;
            let hops = ((now - l) / geometry::BOUNCE_PERIOD).floor();
            if hops < 1.0 || still {
                lift = geometry::bounce_height(now - l);
            }
        }
        if let Some(since) = t
            .kind
            .app_id()
            .and_then(|a| self.launcher.get(a))
            .and_then(|s| s.urgent_since)
        {
            lift = lift.max(geometry::attention_height(now - since));
        }
        lift
    }

    /// Where everything is at `now`.
    pub fn frame(&self, now: f64) -> (Layout, Vec<SlotRef>) {
        let m = self.metrics();
        let (slots, refs) = self.slots(now);
        let mag = self.mag_at(now);
        let g = (mag > 0.0).then_some(Magnify {
            x: self.peak_x,
            peak: self.options.peak(),
            progress: mag,
        });
        (
            geometry::layout(&m, &slots, self.size.width, self.shelf_y(now), g),
            refs,
        )
    }

    /// The cell of tile `i` in a frame.
    pub fn cell_of(l: &Layout, refs: &[SlotRef], i: usize) -> Option<geometry::Cell> {
        refs.iter()
            .position(|r| *r == SlotRef::Tile(i))
            .and_then(|s| l.cells.get(s).copied())
    }

    /// Anything still moving.
    pub fn animating(&self, now: f64) -> bool {
        let (_, to, _) = self.mag;
        self.mag_at(now) != to
            || self
                .tiles
                .iter()
                .any(|t| t.moving(now) || t.launched.is_some())
            || self.gaps.iter().any(|g| !g.settled(now) || g.gone(now))
            || self.shown(now) != self.hide.1
            || self.launcher.values().any(|s| s.urgent_since.is_some())
    }

    /// Recomposes the tiles from the inputs.
    pub fn recompose(&mut self, now: f64, animate: bool) {
        self.inputs.minimize_to_app = self.options.minimize_to_app;
        self.inputs.recents = self.options.recents.then(|| self.closed.clone());
        let kinds = model::compose(&self.inputs);
        let animate = animate && self.shown(now) > 0.0;
        self.tiles = model::sync(&self.tiles, kinds, now, animate);
    }
}

// ----- Configuration -------------------------------------------------------------------------

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
}

fn dock_json() -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(config_dir()?.join("silica/dock.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn json_list(key: &str) -> Option<Vec<String>> {
    let v = dock_json()?;
    let list = v.get(key)?.as_array()?;
    Some(
        list.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
}

/// Writes `key` of dock.json, keeping its other keys.
fn save_json(key: &str, value: serde_json::Value) {
    let Some(dir) = config_dir().map(|d| d.join("silica")) else {
        return;
    };
    let mut v = dock_json().unwrap_or_else(|| serde_json::json!({}));
    if let Some(o) = v.as_object_mut() {
        o.insert(key.into(), value);
    }
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(&v) {
        let tmp = dir.join("dock.json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("dock.json"));
        }
    }
}

/// The downloads folder: xdg-user-dirs' `XDG_DOWNLOAD_DIR`, else `~/Downloads`.
pub fn downloads_dir() -> Option<PathBuf> {
    user_dir("XDG_DOWNLOAD_DIR", "Downloads")
}

/// A user folder of xdg-user-dirs (`key` in `user-dirs.dirs`), else `~/<fallback>`; only one
/// that exists.
pub fn user_dir(key: &str, fallback: &str) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let prefix = format!("{key}=");
    let from_dirs = config_dir()
        .and_then(|d| std::fs::read_to_string(d.join("user-dirs.dirs")).ok())
        .and_then(|text| {
            text.lines().find_map(|l| {
                let v = l.trim().strip_prefix(prefix.as_str())?;
                let v = v.trim_matches('"');
                let p = match v.strip_prefix("$HOME") {
                    Some(rest) => home.join(rest.trim_start_matches('/')),
                    None => PathBuf::from(v),
                };
                (p.is_absolute() && p != home).then_some(p)
            })
        });
    from_dirs
        .or_else(|| Some(home.join(fallback)))
        .filter(|p| p.is_dir())
}

fn configured_folders() -> Vec<PathBuf> {
    match json_list("folders") {
        Some(list) => list
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_dir())
            .collect(),
        None => downloads_dir().into_iter().collect(),
    }
}

// ----- Files ---------------------------------------------------------------------------------

/// The home Trash (freedesktop Trash specification).
pub fn trash_dir() -> Option<PathBuf> {
    Some(data_home()?.join("Trash"))
}

fn trash_has_files() -> bool {
    trash_dir()
        .and_then(|d| std::fs::read_dir(d.join("files")).ok())
        .is_some_and(|mut r| r.next().is_some())
}

/// Empties the home Trash: its `files` and `info` folders.
pub fn empty_trash() {
    let Some(dir) = trash_dir() else { return };
    for sub in ["files", "info"] {
        let Ok(entries) = std::fs::read_dir(dir.join(sub)) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            // A file that cannot go stays; the rest is emptied anyway.
            let _ = if p.is_dir() && !p.is_symlink() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
        }
    }
}

/// Moves `paths` to the home Trash with their `.trashinfo` (freedesktop Trash specification).
pub fn move_to_trash(paths: &[PathBuf]) {
    let Some(dir) = trash_dir() else { return };
    let (files, info) = (dir.join("files"), dir.join("info"));
    let _ = std::fs::create_dir_all(&files);
    let _ = std::fs::create_dir_all(&info);
    let t = crate::time::now();
    let date = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    );
    for p in paths {
        let Some(name) = p.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let mut target = name.clone();
        let mut n = 2;
        while files.join(&target).exists() || info.join(format!("{target}.trashinfo")).exists() {
            target = format!("{name}.{n}");
            n += 1;
        }
        let record = format!(
            "[Trash Info]\nPath={}\nDeletionDate={date}\n",
            percent_path(p)
        );
        if std::fs::write(info.join(format!("{target}.trashinfo")), record).is_err() {
            continue;
        }
        if std::fs::rename(p, files.join(&target)).is_err() {
            let _ = std::fs::remove_file(info.join(format!("{target}.trashinfo")));
        }
    }
}

fn percent_path(p: &Path) -> String {
    let mut out = String::new();
    for b in p.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Moves `paths` into `folder` (a copy across file systems, then the original goes).
pub fn move_into(paths: &[PathBuf], folder: &Path) {
    for p in paths {
        let Some(name) = p.file_name() else { continue };
        let to = folder.join(name);
        if to.exists() || p.parent() == Some(folder) {
            continue;
        }
        if std::fs::rename(p, &to).is_err() && p.is_file() && std::fs::copy(p, &to).is_ok() {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// The local paths of dropped URIs.
pub fn uri_paths(items: &[String]) -> Vec<PathBuf> {
    items
        .iter()
        .filter_map(|u| u.strip_prefix("file://"))
        .map(|p| PathBuf::from(percent_decode(p)))
        .filter(|p| p.is_absolute())
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%'
            && let Some(h) = bytes.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(&String::from_utf8_lossy(h), 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Opens `path` with its default app (silica-daemon's Apps1).
pub fn open_path(path: &Path) {
    let uri = format!("file://{}", path.display());
    vetro_lib::platform::spawn(async move {
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.apps().await
        {
            let _ = p.open_uris(&[uri.as_str()]).await;
        }
    });
}

/// Opens `paths` with app `id`.
pub fn open_with(id: &str, paths: &[PathBuf]) {
    let id = id.to_string();
    let uris: Vec<String> = paths
        .iter()
        .map(|p| format!("file://{}", p.display()))
        .collect();
    vetro_lib::platform::spawn(async move {
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.apps().await
        {
            let refs: Vec<&str> = uris.iter().map(String::as_str).collect();
            let _ = p.open(&id, &refs, HashMap::new()).await;
        }
    });
}

/// Writes a setting through Settings1.
pub fn set_setting(key: &'static str, value: zbus::zvariant::OwnedValue) {
    vetro_lib::platform::spawn(async move {
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.settings_proxy().await
        {
            let _ = p.set(key, &zbus::zvariant::Value::from(value)).await;
        }
    });
}

// ----- Life cycle ----------------------------------------------------------------------------

/// Opens the Dock on the first output.
pub fn open(ui: &mut Ui, state: &Shared) {
    let now = ui.now;
    let output = ui.outputs().first().map(|o| (o.name.clone(), o.size.width));
    let width = output.as_ref().map_or(1280.0, |o| o.1);
    {
        let mut s = state.borrow_mut();
        s.inputs.entries = model::installed();
        s.inputs.pinned =
            json_list("pinned").unwrap_or_else(|| model::default_pinned(&s.inputs.entries));
        s.inputs.folders = configured_folders();
        s.trash_full = trash_has_files();
        let m = s.metrics();
        let _ = m;
        s.size = Size::new(width, surface_height());
        s.hide = if s.options.autohide {
            (0.0, 0.0, now, 0.0)
        } else {
            (1.0, 1.0, now, 0.0)
        };
        s.recompose(now, false);
    }
    let (size, zone) = {
        let s = state.borrow();
        (s.size, exclusive_zone(&s))
    };
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Top,
            anchor: Edges::BOTTOM | Edges::LEFT | Edges::RIGHT,
            exclusive: zone,
            margin: [0; 4],
            keyboard: KeyboardMode::None,
            namespace: "dock".into(),
            output: output.map(|o| o.0),
        },
        size,
    );
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
    }
    let view = ui.create(view::DockView::new(state.clone()));
    let _ = ui.set(view, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(view, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, view);
    ui.show_window(w);
    state.borrow_mut().surface = Some((w, view));
    view::sync_region(ui, state);
    apply_effect(ui, state);
    watch(ui, state.clone());
    let st = state.clone();
    launcher_entry::listen(move |ui, id, props| {
        let now = ui.now;
        {
            let mut s = st.borrow_mut();
            let old = s.launcher.get(&id).cloned().unwrap_or_default();
            let next = launcher_entry::apply(&old, &props, now);
            if next.is_empty() {
                s.launcher.remove(&id);
            } else {
                s.launcher.insert(id, next);
            }
        }
        animate(ui, &st);
    });
}

/// The surface's height: the largest shelf, its gap, the headroom (constant, so a resize never
/// changes the surface).
pub fn surface_height() -> f32 {
    Metrics::new(geometry::MAX_CANVAS).tile_height() + BOTTOM_GAP + HEADROOM
}

fn exclusive_zone(s: &DockState) -> i32 {
    if s.options.autohide {
        0
    } else {
        (s.metrics().tile_height() + BOTTOM_GAP) as i32
    }
}

/// Follows the Trash, the folders and the installed apps every two seconds.
fn watch(ui: &mut Ui, state: Shared) {
    ui.after(2.0, move |ui| {
        let now = ui.now;
        let full = trash_has_files();
        let changed = {
            let mut s = state.borrow_mut();
            let mut changed = full != s.trash_full;
            s.trash_full = full;
            let entries = model::installed();
            if entries.len() != s.inputs.entries.len() {
                s.inputs.entries = entries;
                s.recompose(now, true);
                changed = true;
            }
            changed
        };
        // Folder piles follow their contents.
        repaint(ui, &state);
        if changed {
            animate(ui, &state);
        }
        watch(ui, state);
    });
}

/// The settings changed.
pub fn settings_changed(ui: &mut Ui, state: &Shared, settings: &SettingsMap) {
    let o = Options::from_settings(settings);
    let now = ui.now;
    let (old, surface) = {
        let s = state.borrow();
        (s.options, s.surface)
    };
    if old == o {
        return;
    }
    {
        let mut s = state.borrow_mut();
        s.options = o;
        if o.autohide != old.autohide {
            s.hide = if o.autohide {
                (1.0, 0.0, now, geometry::AUTOHIDE_HIDE)
            } else {
                (1.0, 1.0, now, 0.0)
            };
        }
        s.recompose(now, false);
    }
    if let Some((w, _)) = surface {
        let (zone, size) = {
            let s = state.borrow();
            (exclusive_zone(&s), s.size)
        };
        ui.set_exclusive_zone(w, zone);
        ui.request(vetro_lib::core::Request::Resize(w, size));
    }
    if o.effect != old.effect {
        apply_effect(ui, state);
    }
    view::sync_region(ui, state);
    animate(ui, state);
}

fn apply_effect(ui: &mut Ui, state: &Shared) {
    let effect = match state.borrow().options.effect {
        Effect::Genie => "genie",
        Effect::Scale => "scale",
    };
    let params = serde_json::json!({ "effect": effect }).to_string();
    ui.shell(ShellRequest::ExecuteWith(
        "window.minimize_effect".into(),
        params,
    ));
}

/// The windows changed: the running apps follow.
pub fn window_changed(
    ui: &mut Ui,
    state: &Shared,
    identifier: &str,
    app_id: &str,
    title: &str,
    open: bool,
) {
    let now = ui.now;
    // The shell's own windows (its panels) are not apps of the Dock.
    if open && app_id == crate::APP_ID {
        return;
    }
    {
        let mut s = state.borrow_mut();
        let pos = s.inputs.windows.iter().position(|w| w.id == identifier);
        match (pos, open) {
            (Some(i), true) => {
                if let Some(w) = s.inputs.windows.get_mut(i) {
                    w.app = app_id.to_string();
                    w.title = title.to_string();
                }
            }
            (Some(i), false) => {
                let w = s.inputs.windows.remove(i);
                s.focus.retain(|f| f != identifier);
                s.inputs.minimized.retain(|m| m != identifier);
                s.thumbnails.remove(identifier);
                if !s.running(&w.app) {
                    s.closed.retain(|a| *a != w.app);
                    s.closed.insert(0, w.app.clone());
                    s.closed.truncate(8);
                }
            }
            (None, true) => {
                s.inputs.windows.push(Window {
                    id: identifier.into(),
                    app: app_id.into(),
                    title: title.into(),
                });
                s.closed.retain(|a| a != app_id);
            }
            (None, false) => {}
        }
        s.recompose(now, true);
    }
    animate(ui, state);
    send_icons(ui, state);
}

/// A window took the focus: it is its app's most recent; its app's attention request ends.
pub fn focused(ui: &mut Ui, state: &Shared, identifier: &str) {
    let mut s = state.borrow_mut();
    s.focus.retain(|w| w != identifier);
    s.focus.push(identifier.to_string());
    let app = s
        .inputs
        .windows
        .iter()
        .find(|w| w.id == identifier)
        .map(|w| w.app.clone());
    if let Some(a) = app
        && let Some(l) = s.launcher.get_mut(&a)
    {
        l.urgent_since = None;
    }
    drop(s);
    repaint(ui, state);
}

/// A window was minimised or restored (D-08): its thumbnail is asked for.
pub fn minimized(ui: &mut Ui, state: &Shared, identifier: &str, minimized: bool) {
    let now = ui.now;
    {
        let mut s = state.borrow_mut();
        s.inputs.minimized.retain(|m| m != identifier);
        if minimized {
            s.inputs.minimized.push(identifier.to_string());
        } else {
            s.thumbnails.remove(identifier);
        }
        s.recompose(now, true);
    }
    if minimized {
        ui.shell(ShellRequest::Thumbnail(identifier.to_string()));
    }
    animate(ui, state);
}

/// A thumbnail arrived (premultiplied RGBA).
pub fn thumbnail(
    ui: &mut Ui,
    state: &Shared,
    identifier: &str,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) {
    if width == 0 || height == 0 {
        return;
    }
    let info = skia_safe::ImageInfo::new(
        (width as i32, height as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let image = skia_safe::images::raster_from_data(
        &info,
        skia_safe::Data::new_copy(&rgba),
        width as usize * 4,
    );
    if let Some(img) = image {
        let mut s = state.borrow_mut();
        if s.inputs.minimized.iter().any(|m| m == identifier) {
            s.thumbnails.insert(identifier.to_string(), img);
        }
    }
    repaint(ui, state);
}

/// Tells Ossido where the app icons are, for windows that minimise into them (`dock.icons`).
pub fn send_icons(ui: &mut Ui, state: &Shared) {
    let now = ui.now;
    let s = state.borrow();
    if !s.options.minimize_to_app {
        return;
    }
    let out_h = ui
        .outputs()
        .first()
        .map_or(s.size.height, |o| o.size.height);
    let top = out_h - s.size.height;
    let (l, refs) = s.frame(now);
    let m = s.metrics();
    let mut icons = Vec::new();
    for (i, t) in s.tiles.iter().enumerate() {
        let (Some(app), Some(c)) = (t.kind.app_id(), DockState::cell_of(&l, &refs, i)) else {
            continue;
        };
        for w in s.windows_of(app) {
            icons.push(serde_json::json!({
                "app_id": w.app,
                "x": f64::from(c.rect.x + c.rect.width / 2.0),
                "y": f64::from(top + l.rest.y + m.icon_top() + m.canvas / 2.0),
            }));
        }
    }
    drop(s);
    let params = serde_json::json!({ "icons": icons }).to_string();
    ui.shell(ShellRequest::ExecuteWith("dock.icons".into(), params));
}

/// Repaints the Dock.
pub fn repaint(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
    }
}

/// Repaints and ticks the Dock until it settles.
pub fn animate(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
        ui.start_animating(v);
    }
}

/// Saves the pinned list.
pub fn save_pinned(ids: &[String]) {
    save_json("pinned", serde_json::json!(ids));
}

/// Saves the folders.
pub fn save_folders(folders: &[PathBuf]) {
    let list: Vec<String> = folders
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    save_json("folders", serde_json::json!(list));
}

/// Keeps or removes tile `key`'s app in the Dock and saves the list.
pub fn set_pinned(ui: &mut Ui, state: &Shared, pin_id: &str, pinned: bool) {
    let now = ui.now;
    {
        let mut s = state.borrow_mut();
        if pinned {
            if !s.inputs.pinned.iter().any(|p| p == pin_id) {
                // A running app kept in the Dock stays where it is: after the pinned ones.
                s.inputs.pinned.push(pin_id.to_string());
            }
        } else {
            s.inputs.pinned.retain(|p| p != pin_id);
        }
        save_pinned(&s.inputs.pinned);
        s.recompose(now, true);
    }
    animate(ui, state);
}

/// Removes `folder` from the Dock.
pub fn remove_folder(ui: &mut Ui, state: &Shared, folder: &Path) {
    let now = ui.now;
    {
        let mut s = state.borrow_mut();
        s.inputs.folders.retain(|f| f != folder);
        save_folders(&s.inputs.folders);
        s.recompose(now, true);
    }
    animate(ui, state);
}

/// Opens tile `i`: activates its app, launches it (bouncing until its first window), opens
/// its folder stack, the Trash, the Launcher, or restores its window.
pub fn activate(ui: &mut Ui, state: &Shared, i: usize) {
    let now = ui.now;
    let kind = state.borrow().tiles.get(i).map(|t| t.kind.clone());
    match kind {
        Some(Kind::App { entry, .. }) => {
            let window = state.borrow().window_of(&entry.id);
            if let Some(l) = state.borrow_mut().launcher.get_mut(&entry.id) {
                l.urgent_since = None;
            }
            match window {
                Some(w) => ui.shell(ShellRequest::ActivateApp(w)),
                None => {
                    let params = serde_json::json!({ "app": entry.id }).to_string();
                    ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
                    crate::faro::note_launched(&entry.id);
                    let bounce = state.borrow().options.bounce;
                    if bounce
                        && let Some(t) = state.borrow_mut().tiles.get_mut(i)
                        && t.launched.is_none_or(|l| now - l > geometry::BOUNCE_LIMIT)
                    {
                        t.launched = Some(now + 0.04);
                    }
                    animate(ui, state);
                }
            }
        }
        Some(Kind::Apps) => {
            let f = state.borrow().on_launcher.clone();
            if let Some(f) = f {
                f(ui);
            }
        }
        Some(Kind::Folder(path)) => view::toggle_stack(ui, state, i, &path),
        Some(Kind::Trash) => {
            if let Some(d) = trash_dir() {
                open_path(&d.join("files"));
            }
        }
        Some(Kind::Window { id, .. }) => ui.shell(ShellRequest::Activate(id)),
        _ => {}
    }
}
