//! The Dock (docs/spec/dock.md for behaviour, the kit's `Dock` masters for layout): a glass
//! shelf at the bottom of the first output with the pinned apps
//! (`$XDG_CONFIG_HOME/silica/dock.json` `pinned`, else the first installed of the default
//! list), the running apps that are not pinned, a separator, the Downloads stack and the
//! Trash.
//!
//! The surface spans the output's width with room above the shelf for magnified and bouncing
//! icons and the hover label; its input region is only the shelf (and what magnification
//! raises), so the rest passes through.
//!
//! - A click on a running app activates it (all its windows); on a closed one it launches it,
//!   and the icon bounces until its first window (D-02).
//! - Magnification (`dock.magnify`, `dock.magnifiedSize`) follows the measured profile, with
//!   the entering and leaving curve (D-00).
//! - Apps that are not pinned grow in and shrink out (DOCK-ANIM).
//! - A secondary click or a long press opens the context menu (D-04).
//! - The Trash empties after an alert (D-07); the stack opens its files (dock-stacks.md).
//!
//! Not yet: dragging (D-05), auto-hide, badges, minimised windows, the left and right
//! positions.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use skia_safe::Image;
use vetro_lib::amalfi::foundation::{Label, whole};
use vetro_lib::amalfi::tokens::typography;
use vetro_lib::color::Color;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::display::GlassProfile;
use vetro_lib::core::shell::ShellRequest;
use vetro_lib::core::window::{Edges, KeyboardMode, Layer};
use vetro_lib::core::{
    Button, Event, EventCx, Handled, LayoutCx, PaintCx, Prop, Proposal, Ui, Value, ViewId,
    WindowId, WindowKind,
};
use vetro_lib::geom::{Point, Rect, Size};
use vetro_lib::services::settings::{SettingValue, SettingsMap};
use vetro_lib::shape::Shape;
use vetro_lib::style::{Border, Style};
use vetro_lib::text::Align;

use crate::apps::{self, DesktopEntry};
use crate::motion::{Bezier, sample};
use crate::popmenu::{self, Item};

/// Measures (kit `Dock/Shelf/Default`, `Dock/Tile/*`, `Dock/Separator/Line`).
pub mod metrics {
    /// The shelf's height.
    pub const SHELF_HEIGHT: f32 = 80.0;
    /// The shelf's padding at both ends.
    pub const PADDING: f32 = 8.0;
    /// A tile's width.
    pub const TILE: f32 = 64.0;
    /// The separator's width.
    pub const SEPARATOR: f32 = 28.0;
    /// The icon in a tile: 60 at (2, 10).
    pub const ICON: f32 = 60.0;
    /// The icon's top in the tile.
    pub const ICON_TOP: f32 = 10.0;
    /// The running disc: 4 at y 72, centred.
    pub const DOT: f32 = 4.0;
    /// The running disc's top.
    pub const DOT_TOP: f32 = 72.0;
    /// The shelf's corner radius.
    pub const RADIUS: f32 = 24.0;
    /// The shelf's distance from the screen's bottom edge.
    pub const BOTTOM: f32 = 4.0;
    /// The label's body height and padding (dock.md "Hover label").
    pub const LABEL_HEIGHT: f32 = 27.0;
    /// The label's padding at both ends.
    pub const LABEL_PADDING: f32 = 15.0;
    /// The label's bottom edge above the icon's top (components.md).
    pub const LABEL_GAP: f32 = 8.0;
    /// Room above the shelf in the surface: magnified and bouncing icons, the label.
    pub const HEADROOM: f32 = 140.0;
    /// Magnification starts this far above the shelf (D-00 "Active zone").
    pub const ZONE_ABOVE: f32 = 5.0;
    /// The end strips without labels.
    pub const END_STRIP: f32 = 8.0;
    /// The long press that opens the menu (D-04).
    pub const LONG_PRESS: f64 = 0.55;
    /// The canvas of a resting icon, the reference of `dock.magnifiedSize`.
    pub const CANVAS: f32 = 60.0;
    /// The largest magnified canvas.
    pub const MAX_CANVAS: f32 = 128.0;
}

/// The magnification profile f(d), every 16 pt from 0 (dock.md "Profile").
const PROFILE: [f32; 15] = [
    1.000, 0.991, 0.965, 0.921, 0.862, 0.787, 0.699, 0.597, 0.485, 0.365, 0.240, 0.131, 0.055,
    0.011, 0.000,
];

/// Progress of the peak cell entering and leaving, every 10 ms of 160 (dock.md "Entering and
/// leaving", the entering row with its ends).
const ENTERING: [f32; 17] = [
    0.000, 0.002, 0.014, 0.060, 0.102, 0.202, 0.294, 0.386, 0.480, 0.577, 0.671, 0.759, 0.837,
    0.902, 0.952, 0.985, 1.000,
];

/// The launch growth (DOCK-ANIM).
const GROW: Bezier = Bezier(0.38, 0.0, 0.64, 1.0);
/// The quit shrink (DOCK-ANIM).
const SHRINK: Bezier = Bezier(0.38, 0.05, 0.61, 1.0);
/// A cell's growth when its app appears.
const GROW_TIME: f64 = 0.35;
/// A cell's shrink when its app quits.
const SHRINK_TIME: f64 = 0.48;
/// The bounce's rise (same curve as the growth) and fall (quadratic ease-in).
const RISE_TIME: f64 = 0.355;
/// One whole hop.
const HOP_TIME: f64 = 0.705;
/// The bounce's height.
const HOP_HEIGHT: f32 = 17.0;
/// A launch that never shows a window stops bouncing after this.
const LAUNCH_TIMEOUT: f64 = 20.0;

/// Apps pinned when the configuration has none: the first installed of each group.
pub const DEFAULT_PINNED: &[&[&str]] = &[
    &["dev.frascella.Mustache", "org.gnome.Nautilus"],
    &[
        "firefox_firefox",
        "firefox",
        "org.mozilla.firefox",
        "chromium_chromium",
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

/// An app in the Dock.
#[derive(Debug, Clone)]
pub struct DockApp {
    /// Its entry.
    pub entry: DesktopEntry,
    /// Pinned (it stays when it quits).
    pub pinned: bool,
}

/// What a tile holds.
#[derive(Debug, Clone)]
pub enum Kind {
    /// An app.
    App(DockApp),
    /// The line between the apps and the folders.
    Separator,
    /// A folder stack.
    Folder(PathBuf),
    /// The Trash.
    Trash,
}

/// A tile with its transient animations.
#[derive(Debug, Clone)]
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
    fn new(kind: Kind) -> Tile {
        Tile {
            kind,
            born: None,
            dying: None,
            launched: None,
        }
    }

    fn app_id(&self) -> Option<&str> {
        match &self.kind {
            Kind::App(a) => Some(&a.entry.id),
            _ => None,
        }
    }

    /// Its size factor from growing in or shrinking out, and its icon's opacity.
    fn presence(&self, now: f64) -> (f32, f32) {
        if let Some(d) = self.dying {
            let s = 1.0 - SHRINK.at(((now - d) / SHRINK_TIME) as f32);
            return (s, s * s);
        }
        if let Some(b) = self.born {
            let s = 0.1 + 0.9 * GROW.at(((now - b) / GROW_TIME) as f32);
            return (s, ((s - 0.2) / 0.8).clamp(0.0, 1.0));
        }
        (1.0, 1.0)
    }

    /// The bounce's height now; `still_launching` lets a new hop start.
    fn lift(&self, now: f64, still_launching: bool) -> f32 {
        let Some(start) = self.launched else {
            return 0.0;
        };
        let t = now - start;
        if t < 0.0 {
            return 0.0;
        }
        let hops = (t / HOP_TIME).floor();
        // A hop always lands; another starts only while the app is still launching.
        if hops >= 1.0 && !still_launching {
            return 0.0;
        }
        let h = t - hops * HOP_TIME;
        if h < RISE_TIME {
            HOP_HEIGHT * GROW.at((h / RISE_TIME) as f32)
        } else {
            let f = ((h - RISE_TIME) / (HOP_TIME - RISE_TIME)) as f32;
            HOP_HEIGHT * (1.0 - f * f)
        }
    }

    fn animating(&self, now: f64) -> bool {
        self.born.is_some_and(|b| now - b < GROW_TIME)
            || self.dying.is_some()
            || self.launched.is_some()
    }
}

/// The Dock's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Magnification on.
    pub magnify: bool,
    /// The peak canvas with magnification (`dock.magnifiedSize`).
    pub magnified: f32,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            magnify: false,
            magnified: 65.0,
        }
    }
}

impl Options {
    /// From the settings.
    pub fn from_settings(s: &SettingsMap) -> Options {
        let d = Options::default();
        Options {
            magnify: s
                .get("dock.magnify")
                .and_then(SettingValue::as_bool)
                .unwrap_or(d.magnify),
            magnified: s
                .get("dock.magnifiedSize")
                .and_then(SettingValue::as_int)
                .map_or(d.magnified, |v| v as f32)
                .clamp(metrics::CANVAS, metrics::MAX_CANVAS),
        }
    }

    /// The peak scale.
    fn peak(&self) -> f32 {
        self.magnified / metrics::CANVAS
    }
}

/// The Dock's state.
#[derive(Default)]
pub struct DockState {
    /// The tiles, in order.
    pub tiles: Vec<Tile>,
    /// Open windows: (identifier, app id, title), the most recently focused last.
    pub windows: Vec<(String, String, String)>,
    /// The tile under the pointer.
    pub hovered: Option<usize>,
    /// The pointer in the surface.
    pub pointer: Option<Point>,
    /// The pointer is in the magnification zone.
    pub in_zone: bool,
    /// Magnification: progress of the peak (0–1), where it started, when, and the pointer x
    /// it follows.
    pub mag: f32,
    mag_from: f32,
    mag_since: f64,
    peak_x: f32,
    /// The settings.
    pub options: Options,
    /// The Dock's surface and view.
    pub surface: Option<(WindowId, ViewId)>,
    /// The surface's size.
    pub size: Size,
    /// The pressed tile, when, and where.
    pressed: Option<(usize, f64, Point)>,
    /// The tile whose menu is open.
    pub menu: Option<(usize, WindowId)>,
    /// The Trash has files.
    pub trash_full: bool,
    /// The input region sent last.
    region: Option<Rect>,
    /// The open stack.
    pub stack: Option<WindowId>,
}

/// A shared Dock state.
pub type Shared = Rc<RefCell<DockState>>;

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

/// The pinned ids from dock.json, or `None` without a list there.
fn configured_pinned() -> Option<Vec<String>> {
    let v = dock_json()?;
    let list = v.get("pinned")?.as_array()?;
    Some(
        list.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
}

/// Writes the pinned ids to dock.json, keeping its other keys.
fn save_pinned(ids: &[String]) {
    let Some(dir) = config_dir().map(|d| d.join("silica")) else {
        return;
    };
    let mut v = dock_json().unwrap_or_else(|| serde_json::json!({}));
    if let Some(o) = v.as_object_mut() {
        o.insert("pinned".into(), serde_json::json!(ids));
    }
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(text) = serde_json::to_string_pretty(&v) {
        let tmp = dir.join("dock.json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("dock.json"));
        }
    }
}

/// The pinned apps that are installed.
pub fn pinned() -> Vec<DesktopEntry> {
    match configured_pinned() {
        Some(ids) => ids.iter().filter_map(|id| apps::entry(id)).collect(),
        None => DEFAULT_PINNED
            .iter()
            .filter_map(|group| group.iter().find_map(|id| apps::entry(id)))
            .collect(),
    }
}

/// The folders right of the separator: dock.json `folders`, else the downloads folder.
fn folders() -> Vec<PathBuf> {
    if let Some(list) = dock_json()
        .as_ref()
        .and_then(|v| v.get("folders"))
        .and_then(|f| f.as_array())
    {
        return list
            .iter()
            .filter_map(|x| x.as_str())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_dir())
            .collect();
    }
    downloads_dir().into_iter().collect()
}

/// The downloads folder: xdg-user-dirs' `XDG_DOWNLOAD_DIR`, else `~/Downloads`.
pub fn downloads_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let from_dirs = config_dir()
        .and_then(|d| std::fs::read_to_string(d.join("user-dirs.dirs")).ok())
        .and_then(|text| {
            text.lines().find_map(|l| {
                let v = l.trim().strip_prefix("XDG_DOWNLOAD_DIR=")?;
                let v = v.trim_matches('"');
                let p = match v.strip_prefix("$HOME") {
                    Some(rest) => home.join(rest.trim_start_matches('/')),
                    None => PathBuf::from(v),
                };
                (p.is_absolute() && p != home).then_some(p)
            })
        });
    from_dirs
        .or_else(|| Some(home.join("Downloads")))
        .filter(|p| p.is_dir())
}

/// The home Trash (freedesktop Trash specification).
fn trash_dir() -> Option<PathBuf> {
    Some(data_home()?.join("Trash"))
}

fn trash_has_files() -> bool {
    trash_dir()
        .and_then(|d| std::fs::read_dir(d.join("files")).ok())
        .is_some_and(|mut r| r.next().is_some())
}

/// Empties the home Trash: its `files` and `info` folders.
fn empty_trash() {
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

/// One laid-out tile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// The cell (its x and width; the shelf's height).
    pub rect: Rect,
    /// The icon.
    pub icon: Rect,
    /// The icon's opacity.
    pub opacity: f32,
}

/// Where everything is.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    /// The shelf.
    pub shelf: Rect,
    /// The shelf at rest (the zone's span).
    pub rest: Rect,
    /// The cells, as the tiles.
    pub cells: Vec<Cell>,
}

/// What the layout needs of a tile.
#[derive(Debug, Clone, Copy)]
pub struct Slot {
    /// Its width at rest.
    pub base: f32,
    /// Its growing or shrinking factor and opacity.
    pub presence: (f32, f32),
    /// The bounce.
    pub lift: f32,
    /// It does not magnify (the separator).
    pub flat: bool,
}

/// The magnification profile at distance `d`.
pub fn falloff(d: f32) -> f32 {
    sample(&PROFILE, 16.0, d.abs())
}

/// Lays the tiles out in a surface of `size`; `mag` is (pointer x, peak scale, progress).
pub fn layout(slots: &[Slot], size: Size, mag: Option<(f32, f32, f32)>) -> Geometry {
    let widths: Vec<f32> = slots.iter().map(|s| s.base * s.presence.0).collect();
    let total: f32 = widths.iter().sum();
    let rest_left = ((size.width - total) / 2.0).round();
    let shelf_y = size.height - metrics::SHELF_HEIGHT;
    let rest = Rect::new(
        rest_left - metrics::PADDING,
        shelf_y,
        total + 2.0 * metrics::PADDING,
        metrics::SHELF_HEIGHT,
    );
    // Resting centres and scales.
    let mut x = rest_left;
    let mut centres = Vec::with_capacity(slots.len());
    for w in &widths {
        centres.push(x + w / 2.0);
        x += w;
    }
    let scales: Vec<f32> = slots
        .iter()
        .zip(&centres)
        .map(|(s, c)| match mag {
            Some((px, peak, p)) if !s.flat => 1.0 + (peak - 1.0) * falloff(px - c) * p,
            _ => 1.0,
        })
        .collect();
    let grown: Vec<f32> = widths.iter().zip(&scales).map(|(w, s)| w * s).collect();
    let growth: f32 = grown.iter().zip(&widths).map(|(g, w)| g - w).sum();
    // The point under the pointer stays still: what grows left of it moves left.
    let mut left_growth = 0.0;
    if let Some((px, _, _)) = mag {
        let mut x = rest_left;
        for (w, g) in widths.iter().zip(&grown) {
            if px >= x + w {
                left_growth += g - w;
            } else if px > x {
                left_growth += (g - w) * (px - x) / w.max(1.0);
            }
            x += w;
        }
        // Near an end, that side grows less (dock.md "Spreading sideways").
        let near_left = ((px - rest_left) / metrics::TILE).clamp(0.0, 1.0);
        let near_right = ((rest_left + total - px) / metrics::TILE).clamp(0.0, 1.0);
        left_growth *= near_left;
        left_growth = growth - (growth - left_growth) * near_right;
    }
    let mut x = rest_left - left_growth;
    let mut cells = Vec::with_capacity(slots.len());
    for ((s, g), scale) in slots.iter().zip(&grown).zip(&scales) {
        let icon_size = metrics::ICON * scale * s.presence.0;
        // The bottom edge rises as the icon grows: 1 pt at ×1.079, 7 at ×2.095.
        let rise = (scale - 1.0) * 6.4;
        let bottom = shelf_y + metrics::ICON_TOP + metrics::ICON - rise - s.lift;
        cells.push(Cell {
            rect: Rect::new(x, shelf_y, *g, metrics::SHELF_HEIGHT),
            icon: Rect::new(
                x + (g - icon_size) / 2.0,
                bottom - icon_size,
                icon_size,
                icon_size,
            ),
            opacity: s.presence.1,
        });
        x += g;
    }
    let shelf = Rect::new(
        rest_left - left_growth - metrics::PADDING,
        shelf_y,
        total + growth + 2.0 * metrics::PADDING,
        metrics::SHELF_HEIGHT,
    );
    Geometry { shelf, rest, cells }
}

impl DockState {
    fn app_tiles(&self) -> impl Iterator<Item = (usize, &DockApp)> {
        self.tiles
            .iter()
            .enumerate()
            .filter_map(|(i, t)| match &t.kind {
                Kind::App(a) => Some((i, a)),
                _ => None,
            })
    }

    /// Builds the tiles: the pinned apps, the running ones, the separator, the folders, the
    /// Trash.
    fn compose(&mut self, pinned: Vec<DesktopEntry>) {
        let mut tiles: Vec<Tile> = pinned
            .into_iter()
            .map(|entry| {
                Tile::new(Kind::App(DockApp {
                    entry,
                    pinned: true,
                }))
            })
            .collect();
        tiles.push(Tile::new(Kind::Separator));
        tiles.extend(folders().into_iter().map(|f| Tile::new(Kind::Folder(f))));
        tiles.push(Tile::new(Kind::Trash));
        self.tiles = tiles;
        self.sync_running(None);
    }

    /// The index where apps that are not pinned go: before the separator.
    fn separator(&self) -> usize {
        self.tiles
            .iter()
            .position(|t| matches!(t.kind, Kind::Separator))
            .unwrap_or(self.tiles.len())
    }

    /// Adds the running apps that are not pinned (growing in at `now`), and lets those that
    /// quit shrink out.
    fn sync_running(&mut self, now: Option<f64>) {
        let running: Vec<String> = self.windows.iter().map(|(_, a, _)| a.clone()).collect();
        for t in &mut self.tiles {
            if let Kind::App(a) = &t.kind {
                let up = running.contains(&a.entry.id);
                if !a.pinned && !up && t.dying.is_none() {
                    t.dying = Some(now.unwrap_or(f64::NEG_INFINITY));
                }
                if up {
                    t.dying = None;
                }
            }
        }
        for app in running {
            if app.is_empty() || self.tiles.iter().any(|t| t.app_id() == Some(&app)) {
                continue;
            }
            let entry = apps::entry(&app).unwrap_or_else(|| DesktopEntry {
                id: app.clone(),
                name: app.rsplit('.').next().unwrap_or(&app).to_string(),
                ..DesktopEntry::default()
            });
            if entry.no_display {
                continue;
            }
            let mut t = Tile::new(Kind::App(DockApp {
                entry,
                pinned: false,
            }));
            t.born = now;
            let at = self.separator();
            self.tiles.insert(at, t);
        }
        self.prune(now.unwrap_or(f64::INFINITY));
    }

    /// Drops the tiles that finished shrinking.
    fn prune(&mut self, now: f64) {
        self.tiles
            .retain(|t| !t.dying.is_some_and(|d| now - d >= SHRINK_TIME));
    }

    fn running(&self, app: &str) -> bool {
        self.windows.iter().any(|(_, a, _)| a == app)
    }

    /// The most recently focused window of `app`.
    fn window_of(&self, app: &str) -> Option<String> {
        self.windows
            .iter()
            .rev()
            .find(|(_, a, _)| a == app)
            .map(|(w, _, _)| w.clone())
    }

    /// The magnification progress at `now`.
    fn mag_at(&self, now: f64) -> f32 {
        let target = if self.in_zone && self.options.magnify {
            1.0
        } else {
            0.0
        };
        if self.mag == target {
            return target;
        }
        // The duration grows with the amplitude: 55 ms at ×1.079, 155 at ×2.095.
        let k = ((self.options.peak() - 1.079) / (2.095 - 1.079)).clamp(0.0, 1.0);
        let duration = 0.055 + 0.1 * k;
        let t = (((now - self.mag_since) / f64::from(duration)) as f32).clamp(0.0, 1.0);
        let e = sample(&ENTERING, 1.0 / 16.0, t);
        self.mag_from + (target - self.mag_from) * e
    }

    fn slots(&self, now: f64) -> Vec<Slot> {
        self.tiles
            .iter()
            .map(|t| {
                let still = t.app_id().is_some_and(|a| !self.running(a))
                    && t.launched.is_some_and(|l| now - l < LAUNCH_TIMEOUT);
                Slot {
                    base: if matches!(t.kind, Kind::Separator) {
                        metrics::SEPARATOR
                    } else {
                        metrics::TILE
                    },
                    presence: t.presence(now),
                    lift: t.lift(now, still),
                    flat: matches!(t.kind, Kind::Separator),
                }
            })
            .collect()
    }

    /// Where everything is at `now`.
    pub fn geometry(&self, now: f64) -> Geometry {
        let mag = self.mag_at(now);
        let m = (mag > 0.0).then_some((self.peak_x, self.options.peak(), mag));
        layout(&self.slots(now), self.size, m)
    }

    fn label_of(&self, i: usize) -> Option<String> {
        match &self.tiles.get(i)?.kind {
            Kind::App(a) => Some(a.entry.name.clone()),
            Kind::Folder(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()),
            Kind::Trash => Some("Trash".into()),
            Kind::Separator => None,
        }
    }

    fn animating(&self, now: f64) -> bool {
        let target = if self.in_zone && self.options.magnify {
            1.0
        } else {
            0.0
        };
        self.mag_at(now) != target || self.tiles.iter().any(|t| t.animating(now))
    }
}

/// Opens the Dock on the first output.
pub fn open(ui: &mut Ui, state: &Shared) {
    state.borrow_mut().compose(pinned());
    state.borrow_mut().trash_full = trash_has_files();
    let output = ui.outputs().first().map(|o| (o.name.clone(), o.size.width));
    let width = output.as_ref().map_or(1280.0, |o| o.1);
    let size = Size::new(width, metrics::SHELF_HEIGHT + metrics::HEADROOM);
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Top,
            anchor: Edges::BOTTOM | Edges::LEFT | Edges::RIGHT,
            exclusive: (metrics::SHELF_HEIGHT + metrics::BOTTOM) as i32,
            margin: [0, 0, metrics::BOTTOM as i32, 0],
            keyboard: KeyboardMode::None,
            namespace: "dock".into(),
            output: output.map(|o| o.0),
        },
        size,
    );
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
    }
    let view = ui.create(DockView::new(state.clone()));
    let _ = ui.set(view, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(view, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, view);
    ui.show_window(w);
    {
        let mut s = state.borrow_mut();
        s.surface = Some((w, view));
        s.size = size;
    }
    sync_region(ui, state);
    watch_trash(ui, state.clone());
}

/// Follows the Trash every two seconds.
fn watch_trash(ui: &mut Ui, state: Shared) {
    ui.after(2.0, move |ui| {
        let full = trash_has_files();
        if full != state.borrow().trash_full {
            state.borrow_mut().trash_full = full;
            repaint(ui, &state);
        }
        watch_trash(ui, state);
    });
}

/// The settings changed.
pub fn settings_changed(ui: &mut Ui, state: &Shared, settings: &SettingsMap) {
    let o = Options::from_settings(settings);
    if state.borrow().options != o {
        state.borrow_mut().options = o;
        repaint(ui, state);
    }
}

/// Limits the surface's input to the shelf and what magnification raises above it.
fn sync_region(ui: &mut Ui, state: &Shared) {
    let now = ui.now;
    let (surface, region) = {
        let s = state.borrow();
        let g = s.geometry(now);
        let top = g
            .cells
            .iter()
            .map(|c| c.icon.y)
            .fold(g.shelf.y - metrics::ZONE_ABOVE, f32::min)
            .max(0.0);
        let r = Rect::new(
            g.shelf.x.min(g.rest.x).floor(),
            top.floor(),
            g.shelf.width.max(g.rest.width).ceil() + 1.0,
            (s.size.height - top).ceil(),
        );
        (s.surface, r)
    };
    let Some((w, _)) = surface else { return };
    if state.borrow().region != Some(region) {
        state.borrow_mut().region = Some(region);
        ui.set_input_region(w, Some(vec![region]));
    }
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
    {
        let mut s = state.borrow_mut();
        let pos = s.windows.iter().position(|(w, _, _)| w == identifier);
        match (pos, open) {
            (Some(i), true) => {
                if let Some(w) = s.windows.get_mut(i) {
                    w.1 = app_id.to_string();
                    w.2 = title.to_string();
                }
            }
            (Some(i), false) => {
                s.windows.remove(i);
            }
            (None, true) => s
                .windows
                .push((identifier.into(), app_id.into(), title.into())),
            (None, false) => {}
        }
        s.sync_running(Some(now));
        s.hovered = s.hovered.filter(|h| *h < s.tiles.len());
    }
    animate(ui, state);
    send_icons(ui, state);
}

/// A window took the focus: it is its app's most recent.
pub fn focused(state: &Shared, identifier: &str) {
    let mut s = state.borrow_mut();
    if let Some(i) = s.windows.iter().position(|(w, _, _)| w == identifier) {
        let w = s.windows.remove(i);
        s.windows.push(w);
    }
}

/// Tells Ossido where the app icons are, for the minimise effect (`dock.icons`).
fn send_icons(ui: &mut Ui, state: &Shared) {
    let now = ui.now;
    let s = state.borrow();
    let out_h = ui
        .outputs()
        .first()
        .map_or(s.size.height, |o| o.size.height);
    let top = out_h - metrics::BOTTOM - s.size.height;
    let g = s.geometry(now);
    let icons: Vec<serde_json::Value> = s
        .app_tiles()
        .filter_map(|(i, a)| {
            let c = g.cells.get(i)?;
            Some(serde_json::json!({
                "app_id": a.entry.id,
                "x": f64::from(c.rect.x + c.rect.width / 2.0),
                "y": f64::from(top + c.rect.y + metrics::ICON_TOP + metrics::ICON / 2.0),
            }))
        })
        .collect();
    drop(s);
    let params = serde_json::json!({ "icons": icons }).to_string();
    ui.shell(ShellRequest::ExecuteWith("dock.icons".into(), params));
}

fn repaint(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
    }
}

fn animate(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
        ui.start_animating(v);
    }
}

fn activate(ui: &mut Ui, state: &Shared, i: usize) {
    let now = ui.now;
    let kind = state.borrow().tiles.get(i).map(|t| t.kind.clone());
    match kind {
        Some(Kind::App(app)) => {
            let window = state.borrow().window_of(&app.entry.id);
            match window {
                Some(w) => ui.shell(ShellRequest::ActivateApp(w)),
                None => {
                    let params = serde_json::json!({ "app": app.entry.id }).to_string();
                    ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
                    crate::faro::note_launched(&app.entry.id);
                    // A bounce always lands: a new launch waits for the current hop.
                    if let Some(t) = state.borrow_mut().tiles.get_mut(i)
                        && t.launched.is_none_or(|l| now - l > LAUNCH_TIMEOUT)
                    {
                        t.launched = Some(now + 0.04);
                    }
                    animate(ui, state);
                }
            }
        }
        Some(Kind::Folder(path)) => crate::stack::toggle(ui, state, i, &path),
        Some(Kind::Trash) => {
            if let Some(d) = trash_dir() {
                open_path(&d.join("files"));
            }
        }
        _ => {}
    }
}

/// Keeps or removes `app` in the Dock and saves the list.
fn set_pinned(ui: &mut Ui, state: &Shared, app: &str, pinned: bool) {
    let now = ui.now;
    {
        let mut s = state.borrow_mut();
        for t in &mut s.tiles {
            if let Kind::App(a) = &mut t.kind
                && a.entry.id == app
            {
                a.pinned = pinned;
            }
        }
        if !pinned {
            // An unpinned app moves after the pinned ones while it runs.
            if let Some(i) = s.tiles.iter().position(|t| t.app_id() == Some(app)) {
                let t = s.tiles.remove(i);
                let at = s.separator();
                s.tiles.insert(at, t);
            }
        }
        s.sync_running(Some(now));
        let ids: Vec<String> = s
            .app_tiles()
            .filter(|(_, a)| a.pinned)
            .map(|(_, a)| a.entry.id.clone())
            .collect();
        save_pinned(&ids);
    }
    animate(ui, state);
}

fn command(id: &'static str, app: &str) -> impl Fn(&mut Ui) + 'static {
    let params = serde_json::json!({ "app_id": app }).to_string();
    move |ui: &mut Ui| ui.shell(ShellRequest::ExecuteWith(id.into(), params.clone()))
}

fn set_setting(key: &'static str, value: bool) {
    vetro_lib::platform::spawn(async move {
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.settings_proxy().await
        {
            let _ = p.set(key, &zbus::zvariant::Value::from(value)).await;
        }
    });
}

/// The context menu of tile `i` (D-04).
fn menu_items(ui: &mut Ui, state: &Shared, i: usize) -> Vec<Item> {
    let s = state.borrow();
    let Some(tile) = s.tiles.get(i) else {
        return Vec::new();
    };
    let st = state.clone();
    match &tile.kind {
        Kind::App(a) => {
            let id = a.entry.id.clone();
            let keep = {
                let (st, id) = (st.clone(), id.clone());
                if a.pinned {
                    Item::action("Remove from Dock", move |ui| {
                        set_pinned(ui, &st, &id, false)
                    })
                } else {
                    Item::action("Keep in Dock", move |ui| set_pinned(ui, &st, &id, true))
                }
            };
            let mut items = Vec::new();
            if s.running(&id) {
                // One item per open window.
                for (w, _, title) in s.windows.iter().filter(|(_, app, _)| *app == id) {
                    let w = w.clone();
                    let label = if title.is_empty() {
                        a.entry.name.clone()
                    } else {
                        title.clone()
                    };
                    items.push(Item::action(&label, move |ui| {
                        ui.shell(ShellRequest::Activate(w.clone()))
                    }));
                }
                items.push(Item::Separator);
                items.push(Item::submenu("Options", vec![keep]));
                items.push(Item::Separator);
                let w = s.window_of(&id);
                items.push(Item::action("Show All Windows", move |ui| {
                    if let Some(w) = &w {
                        ui.shell(ShellRequest::ActivateApp(w.clone()));
                    }
                    ui.shell(ShellRequest::Execute("panorama.app".into()));
                }));
                items.push(Item::action("Hide", command("app.hide", &id)));
                items.push(Item::action("Quit", command("app.quit", &id)));
            } else {
                items.push(Item::submenu("Options", vec![keep]));
                items.push(Item::Separator);
                let st = st.clone();
                items.push(Item::action("Open", move |ui| activate(ui, &st, i)));
            }
            items
        }
        Kind::Folder(p) => {
            let p = p.clone();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            vec![Item::action(
                &format!("Open \u{201c}{name}\u{201d}"),
                move |_| open_path(&p),
            )]
        }
        Kind::Trash => {
            let full = s.trash_full;
            let mut items = vec![Item::action("Open", move |ui| activate(ui, &st, i))];
            items.push(Item::Separator);
            if full {
                let st = state.clone();
                items.push(Item::action("Empty Trash…", move |ui| {
                    confirm_empty_trash(ui, &st)
                }));
            } else {
                items.push(Item::Action {
                    label: "Empty Trash…".into(),
                    action: None,
                    checked: false,
                });
            }
            items
        }
        Kind::Separator => {
            let magnify = s.options.magnify;
            let _ = ui;
            vec![
                Item::action(
                    if magnify {
                        "Turn Magnification Off"
                    } else {
                        "Turn Magnification On"
                    },
                    move |_| set_setting("dock.magnify", !magnify),
                ),
                Item::Separator,
                Item::action("Dock Settings…", |ui| {
                    let params = serde_json::json!({ "app": "dev.frascella.Settings" }).to_string();
                    ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
                }),
            ]
        }
    }
}

/// Opens tile `i`'s menu: its frame starts 40 pt left of the icon's centre, above the Dock.
fn open_menu(ui: &mut Ui, state: &Shared, i: usize) {
    let old = state.borrow_mut().menu.take();
    if let Some((_, w)) = old {
        ui.close_window(w);
    }
    let Some((surface, _)) = state.borrow().surface else {
        return;
    };
    let items = menu_items(ui, state, i);
    if items.is_empty() {
        return;
    }
    let size = popmenu::size(ui, &items);
    let g = state.borrow().geometry(ui.now);
    let Some(cell) = g.cells.get(i) else { return };
    let centre = cell.rect.x + cell.rect.width / 2.0;
    let x = (centre - 27.0).clamp(4.0, (state.borrow().size.width - size.width - 4.0).max(4.0));
    let y = g.shelf.y - 14.0 - size.height;
    let st = state.clone();
    let w = popmenu::show(
        ui,
        surface,
        Point::new(x, y),
        items,
        Some(Rc::new(move |ui: &mut Ui| {
            st.borrow_mut().menu = None;
            repaint(ui, &st);
        })),
    );
    state.borrow_mut().menu = Some((i, w));
    repaint(ui, state);
}

/// "Empty Trash…": the alert in a window of its own (D-07).
fn confirm_empty_trash(ui: &mut Ui, state: &Shared) {
    use vetro_lib::amalfi::alerts::{AlertButton, AlertSpec, ButtonKind, alert};
    let dark = state
        .borrow()
        .surface
        .is_some_and(|(_, v)| ui.theme_of(v).dark());
    let icon = apps::shared_icon("files", "trash-full", "icon.png", dark).map(|img| {
        ui.create(crate::dock::IconView {
            image: img,
            size: 66.0,
        })
    });
    let spec = AlertSpec {
        icon,
        title: "Erase the items in the Trash for good?".into(),
        message: Some("You can't undo this action.".into()),
        buttons: vec![
            AlertButton::new("Empty Trash", ButtonKind::Destructive),
            AlertButton::new("Cancel", ButtonKind::Secondary),
        ],
        stacked: true,
        wide_buttons: true,
        glass: true,
        ..AlertSpec::default()
    };
    let a = alert(ui, spec);
    let size = ui.measure(a, Proposal::new(None, None));
    let w = ui.create_window(WindowKind::Panel, size);
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
        win.title = "Empty Trash".into();
    }
    ui.set_content(w, a);
    ui.show_window(w);
    let st = state.clone();
    ui.connect(a, vetro_lib::core::Signal::Activated, move |ui, _, v| {
        if v.as_int() == Some(0) {
            empty_trash();
            st.borrow_mut().trash_full = trash_has_files();
            repaint(ui, &st);
        }
        ui.close_window(w);
    });
    ui.connect(
        a,
        vetro_lib::core::Signal::CloseRequested,
        move |ui, _, _| ui.close_window(w),
    );
}

/// An image at a fixed size (the alert's Trash).
pub struct IconView {
    /// The image.
    pub image: Image,
    /// Its side.
    pub size: f32,
}

impl vetro_lib::core::Widget for IconView {
    fn kind(&self) -> &'static str {
        "Image"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, _p: Proposal) -> Size {
        Size::new(self.size, self.size)
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let r = cx.bounds();
        cx.rec.image("Icon", r, self.image.clone(), 1.0);
    }
}

/// The Dock's shelf, tiles and label.
pub struct DockView {
    state: Shared,
    label: Label,
}

impl DockView {
    /// A Dock over `state`.
    pub fn new(state: Shared) -> DockView {
        DockView {
            state,
            label: Label::default(),
        }
    }

    /// The tile under `p` (surface coordinates) in geometry `g`.
    fn tile_at(g: &Geometry, p: Point) -> Option<usize> {
        if p.y < g.shelf.y - metrics::ZONE_ABOVE {
            // Above the shelf only a raised icon counts.
            return g
                .cells
                .iter()
                .position(|c| p.x >= c.rect.x && p.x < c.rect.right() && p.y >= c.icon.y);
        }
        g.cells
            .iter()
            .position(|c| p.x >= c.rect.x && p.x < c.rect.right())
    }

    /// Follows the pointer: the zone, the peak, the hovered tile.
    fn track(&mut self, cx: &mut EventCx<'_>, p: Option<Point>) {
        let now = cx.now();
        let state = self.state.clone();
        let mut s = state.borrow_mut();
        let g = s.geometry(now);
        let in_zone = p.is_some_and(|p| {
            let span = p.x >= g.rest.x && p.x < g.rest.right();
            let above = p.y >= g.shelf.y - metrics::ZONE_ABOVE;
            // At a large setting a raised icon holds the zone up to its top.
            let raised = s.options.peak() > 1.5
                && s.hovered
                    .and_then(|h| g.cells.get(h))
                    .is_some_and(|c| p.y >= c.icon.y && p.x >= c.rect.x && p.x < c.rect.right());
            span && (above || raised)
        });
        if in_zone != s.in_zone {
            let m = s.mag_at(now);
            s.mag_from = m;
            s.mag_since = now;
            s.in_zone = in_zone;
            s.mag = if in_zone {
                1.0 - f32::EPSILON
            } else {
                f32::EPSILON
            };
        }
        if in_zone && let Some(p) = p {
            s.peak_x = p.x;
        }
        let hovered = p.and_then(|p| Self::tile_at(&g, p));
        // No label in the end strips.
        let strip = p.is_some_and(|p| {
            p.x < g.shelf.x + metrics::END_STRIP || p.x >= g.shelf.right() - metrics::END_STRIP
        });
        s.hovered = if strip { None } else { hovered };
        s.pointer = p;
        drop(s);
        cx.request_paint();
        cx.animate();
    }
}

impl vetro_lib::core::Widget for DockView {
    fn kind(&self) -> &'static str {
        "Dock"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        let s = self.state.borrow().size;
        p.replacing_unspecified(s)
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        let more = {
            let mut s = self.state.borrow_mut();
            let target = if s.in_zone && s.options.magnify {
                1.0
            } else {
                0.0
            };
            if s.mag_at(now) == target {
                s.mag = target;
            }
            // Finished animations end.
            let running: Vec<String> = s.windows.iter().map(|(_, a, _)| a.clone()).collect();
            for t in &mut s.tiles {
                if t.born.is_some_and(|b| now - b >= GROW_TIME) {
                    t.born = None;
                }
                if let Some(l) = t.launched {
                    let up = t.app_id().is_some_and(|a| running.iter().any(|r| r == a));
                    let hop_done = (now - l) >= HOP_TIME * ((now - l) / HOP_TIME).ceil() - 0.001;
                    if (up || now - l > LAUNCH_TIMEOUT) && (now - l) >= HOP_TIME && hop_done {
                        t.launched = None;
                    }
                }
            }
            let before = s.tiles.len();
            s.prune(now);
            if s.tiles.len() != before {
                s.hovered = None;
            }
            s.animating(now)
        };
        let state = self.state.clone();
        sync_region(cx.ui(), &state);
        cx.request_paint();
        if !more {
            send_icons(cx.ui(), &state);
        }
        more
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let r = cx.bounds();
        let theme = *cx.theme();
        let c = theme.colors;
        let dark = theme.dark();
        let now = cx.now();
        let st = self.state.borrow();
        let g = st.geometry(now);
        let o = r.origin();
        cx.glass(
            "Shelf",
            g.shelf.translate(o),
            Shape::rounded(metrics::RADIUS),
            "Glass/Regular",
            GlassProfile::Dock,
        );
        for (i, (tile, cell)) in st.tiles.iter().zip(&g.cells).enumerate() {
            let icon = cell.icon.translate(o);
            match &tile.kind {
                Kind::Separator => {
                    let line = if dark {
                        Color::rgba(0xf4, 0xf5, 0xf7, 0x4d)
                    } else {
                        Color::rgba(0x1d, 0x1e, 0x21, 0x59)
                    };
                    cx.rec.layer(
                        "Separator",
                        Rect::new(
                            o.x + cell.rect.x + (cell.rect.width / 2.0).floor(),
                            o.y + cell.rect.y + 10.0,
                            1.0,
                            60.0,
                        ),
                        Shape::rounded(0.0),
                        Style::fill(line),
                    );
                    continue;
                }
                Kind::App(app) => match apps::icon(&app.entry, dark) {
                    Some(img) => cx.rec.image("Icon", icon, img, cell.opacity),
                    None => missing_tile(cx, icon, cell.opacity, dark),
                },
                Kind::Folder(p) => {
                    let open = st.stack.is_some()
                        && st.menu.is_none()
                        && crate::stack::open_for(&self.state, i);
                    crate::stack::paint_tile(cx, icon, p, dark, open);
                }
                Kind::Trash => {
                    let name = if st.trash_full {
                        "trash-full"
                    } else {
                        "trash-empty"
                    };
                    match apps::shared_icon("files", name, "icon.png", dark) {
                        Some(img) => cx.rec.image("Icon", icon, img, cell.opacity),
                        None => missing_tile(cx, icon, cell.opacity, dark),
                    }
                }
            }
            if let Some(id) = tile.app_id()
                && st.running(id)
            {
                cx.rec.layer(
                    "Running dot",
                    Rect::new(
                        o.x + cell.rect.x + (cell.rect.width - metrics::DOT) / 2.0,
                        o.y + cell.rect.y + metrics::DOT_TOP,
                        metrics::DOT,
                        metrics::DOT,
                    ),
                    Shape::Ellipse,
                    Style::fill(c.text_primary),
                );
            }
        }
        // The label above the hovered icon (not while its menu is open).
        if let Some(h) = st
            .hovered
            .filter(|_| st.menu.is_none() && st.stack.is_none())
            && let (Some(name), Some(cell)) = (st.label_of(h), g.cells.get(h))
        {
            let style = theme.text(&typography::BODY, c.text_primary);
            let tw = whole(self.label.width(&name, &style), 1.0);
            let w = tw + 2.0 * metrics::LABEL_PADDING;
            let centre = cell.rect.x + cell.rect.width / 2.0;
            let rect = Rect::new(
                o.x + (centre - w / 2.0).round(),
                o.y + cell.icon.y - metrics::LABEL_GAP - metrics::LABEL_HEIGHT,
                w,
                metrics::LABEL_HEIGHT,
            );
            cx.glass(
                "Label",
                rect,
                Shape::rounded(metrics::LABEL_HEIGHT / 2.0),
                "Glass/Regular",
                GlassProfile::Tooltip,
            );
            let l = self.label.layout(&name, &style, Align::Center, w, Some(1));
            let h = l.size().height;
            cx.rec.text(
                "Label text",
                Rect::new(rect.x, rect.y + (rect.height - h) / 2.0, w, h),
                l,
                Some(typography::BODY.name),
                c.text_primary,
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        let local = |p: Point| Point::new(p.x - b.x, p.y - b.y);
        match event {
            Event::PointerMove { pos, .. } => {
                let p = local(*pos);
                // A press that moves is not a long press.
                let moved = self
                    .state
                    .borrow()
                    .pressed
                    .is_some_and(|(_, _, at)| (at.x - p.x).hypot(at.y - p.y) > 5.0);
                if moved {
                    self.state.borrow_mut().pressed = None;
                }
                self.track(cx, Some(p));
                let state = self.state.clone();
                sync_region(cx.ui(), &state);
                Handled::No
            }
            Event::PointerLeave => {
                self.track(cx, None);
                Handled::No
            }
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                let p = local(*pos);
                let now = cx.now();
                let g = self.state.borrow().geometry(now);
                if let Some(i) = Self::tile_at(&g, p) {
                    self.state.borrow_mut().pressed = Some((i, now, p));
                    let state = self.state.clone();
                    cx.ui().after(metrics::LONG_PRESS, move |ui| {
                        let held = state
                            .borrow()
                            .pressed
                            .is_some_and(|(j, t, _)| j == i && t == now);
                        if held {
                            state.borrow_mut().pressed = None;
                            open_menu(ui, &state, i);
                        }
                    });
                }
                Handled::Yes
            }
            Event::PointerUp {
                button: Button::Left,
                ..
            } => {
                let pressed = self.state.borrow_mut().pressed.take();
                if let Some((i, _, _)) = pressed {
                    let state = self.state.clone();
                    activate(cx.ui(), &state, i);
                }
                Handled::Yes
            }
            Event::PointerDown {
                pos,
                button: Button::Right,
                ..
            } => {
                let p = local(*pos);
                let g = self.state.borrow().geometry(cx.now());
                if let Some(i) = Self::tile_at(&g, p) {
                    self.state.borrow_mut().pressed = None;
                    let state = self.state.clone();
                    open_menu(cx.ui(), &state, i);
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn hit(&self, _p: Point, _size: Size) -> bool {
        true
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Toolbar;
        a.label = Some("Dock".into());
    }
}

/// An app without an icon (kit `Dock/Tile/Missing`): a 48 pt tile with a question mark.
fn missing_tile(cx: &mut PaintCx<'_>, icon: Rect, opacity: f32, dark: bool) {
    let k = icon.width / metrics::ICON;
    let (fill, border, ink) = if dark {
        (
            Color::rgba(0x3a, 0x3c, 0x42, 0xff),
            Color::rgba(0xff, 0xff, 0xff, 0x14),
            Color::rgba(0x8e, 0x92, 0x9a, 0xff),
        )
    } else {
        (
            Color::rgba(0xe6, 0xe8, 0xec, 0xff),
            Color::rgba(0, 0, 0, 0x14),
            Color::rgba(0x9a, 0x9e, 0xa6, 0xff),
        )
    };
    cx.rec.push_opacity(opacity);
    cx.rec.layer(
        "Tile",
        Rect::new(icon.x + 6.0 * k, icon.y + 6.0 * k, 48.0 * k, 48.0 * k),
        Shape::rounded(12.0 * k),
        Style::fill(fill).with_border(Border::inside(border, 1.0)),
    );
    cx.rec.glyph(
        "Glyph",
        Rect::new(icon.x + 16.0 * k, icon.y + 16.0 * k, 28.0 * k, 28.0 * k),
        "question",
        700,
        ink,
    );
    cx.rec.pop();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(n: usize) -> Vec<Slot> {
        (0..n)
            .map(|_| Slot {
                base: 64.0,
                presence: (1.0, 1.0),
                lift: 0.0,
                flat: false,
            })
            .collect()
    }

    #[test]
    fn running_apps_go_before_the_separator() {
        let mut s = DockState {
            tiles: vec![
                Tile::new(Kind::App(DockApp {
                    entry: DesktopEntry {
                        id: "a.Pinned".into(),
                        name: "Pinned".into(),
                        ..DesktopEntry::default()
                    },
                    pinned: true,
                })),
                Tile::new(Kind::Separator),
                Tile::new(Kind::Trash),
            ],
            ..DockState::default()
        };
        s.windows = vec![
            ("w1".into(), "x.Other".into(), String::new()),
            ("w2".into(), "a.Pinned".into(), String::new()),
            ("w3".into(), "x.Other".into(), String::new()),
        ];
        s.sync_running(None);
        let ids: Vec<Option<&str>> = s.tiles.iter().map(|t| t.app_id()).collect();
        assert_eq!(ids, vec![Some("a.Pinned"), Some("x.Other"), None, None]);
        assert!(s.running("a.Pinned"));
        assert_eq!(s.window_of("x.Other").as_deref(), Some("w3"));
        // The app quits: it shrinks out, then goes.
        s.windows.retain(|w| w.1 != "x.Other");
        s.sync_running(Some(10.0));
        assert_eq!(s.tiles.len(), 4);
        s.prune(10.0 + SHRINK_TIME);
        assert_eq!(s.tiles.len(), 3);
    }

    #[test]
    fn magnification_follows_the_profile() {
        assert_eq!(falloff(0.0), 1.0);
        assert!((falloff(24.0) - 0.978).abs() < 1e-3);
        assert_eq!(falloff(300.0), 0.0);
        let size = Size::new(1280.0, 220.0);
        let rest = layout(&slots(10), size, None);
        assert_eq!(rest.shelf.width, 640.0 + 16.0);
        assert_eq!(rest.shelf, rest.rest);
        // The pointer on the centre of the fifth cell: it peaks and stays put.
        let c = rest.cells[4].rect;
        let px = c.x + 32.0;
        let m = layout(&slots(10), size, Some((px, 2.0, 1.0)));
        assert_eq!(m.cells[4].rect.width, 128.0);
        assert!((m.cells[4].rect.x + 64.0 - px).abs() < 0.01);
        assert!(m.shelf.width > rest.shelf.width);
        // Icons rise as they grow.
        assert!(m.cells[4].icon.bottom() < rest.cells[4].icon.bottom());
        // On the first cell the left end does not move.
        let first = rest.cells[0].rect.x;
        let m = layout(&slots(10), size, Some((first, 2.0, 1.0)));
        assert_eq!(m.cells[0].rect.x, first);
    }

    #[test]
    fn bounces_land() {
        let mut t = Tile::new(Kind::Trash);
        t.launched = Some(0.0);
        assert!(t.lift(0.0, true) < 0.01);
        assert!((t.lift(RISE_TIME, true) - HOP_HEIGHT).abs() < 0.01);
        assert!(t.lift(HOP_TIME - 0.001, true) < 0.5);
        assert_eq!(t.lift(HOP_TIME + 0.2, false), 0.0);
        assert!(t.lift(HOP_TIME + 0.2, true) > 0.0);
    }
}
