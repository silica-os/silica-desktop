//! Folder stacks in the Dock (docs/spec/dock-stacks.md): the tile's pile, the stack settings
//! (order, folder or stack, fan, grid, list or automatic; `$XDG_CONFIG_HOME/silica/
//! dock-stacks.json` by path), the fan and the grid opening above the tile, and the list view's
//! menu items.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vetro_lib::amalfi::foundation::Label;
use vetro_lib::amalfi::tokens::{TypeStyle, typography};
use vetro_lib::color::Color;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::display::GlassProfile;
use vetro_lib::core::event::Key;
use vetro_lib::core::{
    Button, Event, EventCx, Handled, LayoutCx, PaintCx, Prop, Proposal, Signal, Ui, Value,
    WindowId, WindowKind,
};
use vetro_lib::geom::{Insets, Point, Rect, Size, Transform};
use vetro_lib::shape::Shape;
use vetro_lib::style::{Border, Style};
use vetro_lib::text::Align;

use crate::motion::Bezier;

/// The order of the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// By name (natural order).
    #[default]
    Name,
    /// By date added, newest first.
    Added,
    /// By date modified, newest first.
    Modified,
    /// By date created, newest first.
    Created,
    /// By kind (extension), then name.
    Kind,
}

/// The tile shows the folder or the stacked documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    /// The folder's icon.
    Folder,
    /// The pile of the first items.
    #[default]
    Stack,
}

/// How the content opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Content {
    /// The fan.
    Fan,
    /// The grid.
    Grid,
    /// A menu.
    List,
    /// Fan up to 15 files, grid beyond.
    #[default]
    Automatic,
}

/// A folder's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    /// Order.
    pub sort: Sort,
    /// Tile.
    pub display: Display,
    /// View.
    pub content: Content,
}

const SORTS: [(Sort, &str); 5] = [
    (Sort::Name, "name"),
    (Sort::Added, "added"),
    (Sort::Modified, "modified"),
    (Sort::Created, "created"),
    (Sort::Kind, "kind"),
];
const DISPLAYS: [(Display, &str); 2] = [(Display::Folder, "folder"), (Display::Stack, "stack")];
const CONTENTS: [(Content, &str); 4] = [
    (Content::Fan, "fan"),
    (Content::Grid, "grid"),
    (Content::List, "list"),
    (Content::Automatic, "automatic"),
];

fn settings_file() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(dir.join("silica/dock-stacks.json"))
}

fn read_all(file: Option<&Path>) -> serde_json::Map<String, serde_json::Value> {
    file.and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// The settings of `folder`: saved ones, else by name (the Downloads folder by date added).
pub fn settings(folder: &Path) -> Settings {
    settings_in(settings_file().as_deref(), folder)
}

fn settings_in(file: Option<&Path>, folder: &Path) -> Settings {
    let all = read_all(file);
    let key = folder.to_string_lossy();
    let Some(v) = all.get(key.as_ref()) else {
        return default_settings(folder);
    };
    let pick = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("");
    Settings {
        sort: SORTS
            .iter()
            .find(|(_, n)| *n == pick("sort"))
            .map_or(Sort::Name, |x| x.0),
        display: DISPLAYS
            .iter()
            .find(|(_, n)| *n == pick("display"))
            .map_or(Display::Stack, |x| x.0),
        content: CONTENTS
            .iter()
            .find(|(_, n)| *n == pick("content"))
            .map_or(Content::Automatic, |x| x.0),
    }
}

/// The defaults: by name, except the Downloads folder (date added, like a download stack).
pub fn default_settings(folder: &Path) -> Settings {
    if super::downloads_dir().as_deref() == Some(folder) {
        Settings {
            sort: Sort::Added,
            ..Settings::default()
        }
    } else {
        Settings::default()
    }
}

/// Saves `folder`'s settings.
pub fn save_settings(folder: &Path, s: Settings) {
    if let Some(file) = settings_file() {
        save_in(&file, folder, s);
    }
}

fn save_in(file: &Path, folder: &Path, s: Settings) {
    let mut all = read_all(Some(file));
    let name =
        |list: &[(_, &'static str)], v| list.iter().find(|(x, _)| *x == v).map_or("", |x| x.1);
    all.insert(
        folder.to_string_lossy().into_owned(),
        serde_json::json!({
            "sort": SORTS.iter().find(|(x, _)| *x == s.sort).map_or("name", |x| x.1),
            "display": name(&DISPLAYS, s.display),
            "content": CONTENTS.iter().find(|(x, _)| *x == s.content).map_or("automatic", |x| x.1),
        }),
    );
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(t) = serde_json::to_string_pretty(&serde_json::Value::Object(all)) {
        let _ = std::fs::write(file, t);
    }
}

/// "Natural" comparison: digits count as numbers (document-2 before document-10),
/// case-insensitive.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn parts(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for ch in s.to_lowercase().chars() {
            let digit = ch.is_ascii_digit();
            match out.last_mut() {
                Some((d, part)) if *d == digit => part.push(ch),
                _ => out.push((digit, ch.to_string())),
            }
        }
        out
    }
    let (pa, pb) = (parts(a), parts(b));
    for ((da, a), (db, b)) in pa.iter().zip(&pb) {
        let c = match (da, db, a.parse::<u128>(), b.parse::<u128>()) {
            (true, true, Ok(x), Ok(y)) => x.cmp(&y),
            _ => a.cmp(b),
        };
        if c != Ordering::Equal {
            return c;
        }
    }
    pa.len().cmp(&pb.len())
}

/// An item of a folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Its path.
    pub path: PathBuf,
    /// A folder.
    pub dir: bool,
    modified: SystemTime,
    changed: SystemTime,
}

impl Entry {
    /// The file's name.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn extension(&self) -> String {
        if self.dir {
            return String::new();
        }
        let n = self.name();
        match n.rfind('.') {
            Some(i) if i > 0 => n[i + 1..].to_lowercase(),
            _ => String::new(),
        }
    }
}

/// The folder's visible items in `sort` order. On Linux "date added" is the status change
/// (moving or copying a file into the folder updates it); there is no reliable creation
/// date, so the modification stands in for it.
pub fn entries(folder: &Path, sort: Sort) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut list: Vec<Entry> = rd
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let meta = e.metadata().ok();
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let changed = meta
                .as_ref()
                .and_then(|m| {
                    use std::os::unix::fs::MetadataExt;
                    let secs = u64::try_from(m.ctime()).ok()?;
                    SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(secs))
                })
                .unwrap_or(modified);
            Entry {
                path: e.path(),
                dir: meta.is_some_and(|m| m.is_dir()),
                modified,
                changed,
            }
        })
        .collect();
    sort_entries(&mut list, sort);
    list
}

fn sort_entries(list: &mut [Entry], sort: Sort) {
    list.sort_by(|a, b| {
        let by_name = || natural_cmp(&a.name(), &b.name());
        match sort {
            Sort::Name => by_name(),
            Sort::Kind => a.extension().cmp(&b.extension()).then_with(by_name),
            Sort::Added => b.changed.cmp(&a.changed).then_with(by_name),
            Sort::Modified | Sort::Created => b.modified.cmp(&a.modified).then_with(by_name),
        }
    });
}

/// The fan holds at most this many files; the next place opens the folder.
pub const FAN_MAX: usize = 15;
/// The fan's icons and their vertical pitch.
pub const FAN_ICON: f32 = 64.0;
/// See [`FAN_ICON`].
pub const FAN_PITCH: f32 = 64.0;
const FAN_FIRST_ANGLE: f32 = 1.63;
const FAN_ANGLE_STEP: f32 = 1.23;
/// The first item's frame ends this far above the Dock area.
pub const FAN_ABOVE: f32 = 7.0;
const FAN_LABEL_HEIGHT: f32 = 24.0;
const FAN_LABEL_PADDING: f32 = 8.0;
const FAN_LABEL_GAP: f32 = 5.5;
const FAN_FOLDER: f32 = 58.0;

/// The grid's cell, margins and tip.
pub const GRID_CELL: f32 = 128.0;
const GRID_LEFT: f32 = 17.0;
const GRID_RIGHT: f32 = 17.0;
const GRID_TOP: f32 = 32.0;
const GRID_BOTTOM: f32 = 11.0;
const GRID_RADIUS: f32 = 16.0;
const GRID_TIP: (f32, f32) = (20.0, 8.0);
const GRID_TIP_ABOVE: f32 = 8.0;
const GRID_CANVAS: f32 = 100.0;
const GRID_CANVAS_TOP: f32 = 6.0;
const GRID_LABEL_WIDTH: f32 = 104.0;

const FAN_OPEN: (Bezier, f64, f64) = (Bezier(0.37, 0.24, 0.60, 1.0), 0.205, 0.036);
const GRID_OPEN: (Bezier, f64, f64) = (Bezier(0.21, 0.22, 0.27, 0.99), 0.230, 0.008);
const FAN_CLOSE: Bezier = Bezier(0.5, 0.3, 0.65, 0.9);
const GRID_CLOSE: (Bezier, f64) = (Bezier(0.3, 0.3, 0.6, 0.85), 0.160);

/// What closes a stack (it sets the fan's duration).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseBy {
    /// Esc: 180 ms from the key-down.
    Esc,
    /// The stack's own tile: 187 ms from the release.
    Button,
    /// A press outside: 211 ms from the press.
    Outside,
    /// A click on a file: ~200 ms.
    File,
}

fn fan_close_time(by: CloseBy) -> f64 {
    match by {
        CloseBy::Esc => 0.180,
        CloseBy::Button => 0.187,
        CloseBy::Outside => 0.211,
        CloseBy::File => 0.200,
    }
}

/// The two ways a stack opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// The fan.
    Fan,
    /// The grid.
    Grid,
}

/// The automatic view for `count` files.
pub fn automatic_view(count: usize) -> View {
    if count <= FAN_MAX {
        View::Fan
    } else {
        View::Grid
    }
}

/// The fan's place `i` (from the bottom) at progress `p`: the icon's centre relative to the
/// stack tile's centre in x and to the top of the Dock area in y, its rotation (radians,
/// clockwise) and its frame (the rotated icon's bounding box side).
pub fn fan_slot(i: usize, p: f32) -> (Point, f32, f32) {
    let angle = |k: usize| (FAN_FIRST_ANGLE + FAN_ANGLE_STEP * k as f32).to_radians() * p;
    let frame = |k: usize| FAN_ICON * (angle(k).cos() + angle(k).sin());
    let mut x = FAN_PITCH * angle(0).tan();
    for k in 1..=i {
        x += FAN_PITCH * ((angle(k - 1) + angle(k)) / 2.0).tan();
    }
    let x = x * p;
    let y = -FAN_ABOVE - frame(0) / 2.0 - FAN_PITCH * p * i as f32;
    (Point::new(x, y), angle(i), frame(i))
}

/// The grid's columns and rows for `count` files (+1 to open the folder).
pub fn grid_shape(count: usize) -> (usize, usize) {
    let cells = count + 1;
    let columns = ((cells as f32).sqrt().ceil() as usize).clamp(1, 12);
    (columns, cells.div_ceil(columns))
}

/// The grid panel's size (its height at most `max_height`, at least one row).
pub fn grid_size(count: usize, max_height: f32) -> Size {
    let (c, r) = grid_shape(count);
    let h = GRID_TOP + r as f32 * GRID_CELL + GRID_BOTTOM;
    let min = GRID_TOP + GRID_CELL + GRID_BOTTOM;
    Size::new(
        GRID_LEFT + c as f32 * GRID_CELL + GRID_RIGHT,
        h.clamp(min, max_height.max(min)),
    )
}

/// Paints a stack tile in `icon` (the canvas): the folder's icon, or a pile of up to three
/// items on a 50 pt canvas (at 17, 15, 13 from the tile's top, each sheet behind 4 % smaller,
/// issue #244); with the stack open, the Stack Close badge on the folder.
pub fn paint_tile(
    cx: &mut PaintCx<'_>,
    icon: Rect,
    folder: &Path,
    dark: bool,
    open: bool,
    opacity: f32,
) {
    let k = icon.width / 60.0;
    let s = settings(folder);
    cx.rec.push_opacity(opacity);
    let folder_icon = |full: bool| {
        crate::apps::shared_icon(
            "files",
            "folder-generic",
            if full { "icon-full.png" } else { "icon.png" },
            dark,
        )
    };
    let items = entries(folder, s.sort);
    if open {
        if let Some(img) = folder_icon(!items.is_empty()) {
            cx.rec.image("Folder", icon, img, 1.0);
        }
        let badge = if dark {
            "stack-close-dark.png"
        } else {
            "stack-close.png"
        };
        if let Some(img) = crate::apps::shared_icon("badges", "", badge, false) {
            cx.rec.image("Close", icon, img, 1.0);
        }
    } else if items.is_empty() || s.display == Display::Folder {
        if let Some(img) = folder_icon(!items.is_empty()) {
            cx.rec.image("Folder", icon, img, 1.0);
        }
    } else {
        let view = cx.id();
        for (depth, e) in items.iter().take(3).enumerate().rev() {
            let scale = 0.96f32.powi(depth as i32);
            let side = (50.0 * scale * k).round();
            let top = [17.0, 15.0, 13.0].get(depth).copied().unwrap_or(13.0) - 10.0;
            let r = Rect::new(
                icon.x + (icon.width - side) / 2.0,
                icon.y + top * k,
                side,
                side,
            );
            let px = (side * 2.0) as u32;
            if let Some(img) = crate::file_icons::icon(&e.path, px, dark, Some(view)) {
                cx.rec.image("Item", r, img, 1.0);
            }
        }
    }
    cx.rec.pop();
}

/// Runs once when a stack is gone.
pub type Gone = Box<dyn FnOnce(&mut Ui)>;

/// Where a stack opens: the Dock surface and its width, the tile's centre, the Dock area's top.
#[derive(Debug, Clone, Copy)]
pub struct Place {
    /// The Dock surface.
    pub surface: WindowId,
    /// Its width.
    pub width: f32,
    /// The tile's centre x.
    pub centre_x: f32,
    /// The Dock area's top.
    pub zone_top: f32,
}

/// The open stack: the fan or the grid, in a grabbing popup of the Dock.
pub struct StackView {
    folder: PathBuf,
    entries: Vec<Entry>,
    view: View,
    /// Where the tile's centre (x) and the Dock area's top (y) are in the popup.
    anchor: Point,
    /// When the opening starts (after its delay), and the closing.
    opened: f64,
    closing: Option<(f64, f64, f32)>,
    labels: Vec<Label>,
    pressed: Option<usize>,
    scroll: f32,
    dark: bool,
    /// Runs once when the stack is gone.
    on_gone: Option<Gone>,
}

const FAN_LABEL: TypeStyle = TypeStyle {
    name: "Fan Label",
    ..typography::BODY
};

impl StackView {
    /// The progress (0 closed, 1 open) at `now`.
    fn progress(&self, now: f64) -> f32 {
        if let Some((start, dur, from)) = self.closing {
            let t = (((now - start) / dur) as f32).clamp(0.0, 1.0);
            let curve = match self.view {
                View::Fan => FAN_CLOSE,
                View::Grid => GRID_CLOSE.0,
            };
            return from * (1.0 - curve.at(t));
        }
        let (curve, dur, _) = match self.view {
            View::Fan => FAN_OPEN,
            View::Grid => GRID_OPEN,
        };
        curve.at((((now - self.opened) / dur) as f32).clamp(0.0, 1.0))
    }

    /// Starts closing (from where an opening was).
    pub fn close(&mut self, now: f64, by: CloseBy) {
        if self.closing.is_some() {
            return;
        }
        let from = self.progress(now);
        let dur = match self.view {
            View::Fan => fan_close_time(by),
            View::Grid => GRID_CLOSE.1,
        };
        self.closing = Some((now, dur, from));
    }

    fn shown(&self) -> usize {
        self.entries.len().min(FAN_MAX)
    }

    /// The grid panel in the popup (its bottom is the tip's base).
    fn panel(&self, b: Rect) -> Rect {
        let max = (self.anchor.y - GRID_TIP.1 - GRID_TIP_ABOVE - 8.0).max(GRID_CELL);
        let s = grid_size(self.entries.len(), max);
        let bottom = self.anchor.y - GRID_TIP_ABOVE - GRID_TIP.1;
        let x = (self.anchor.x - s.width / 2.0).clamp(4.0, (b.width - s.width - 4.0).max(4.0));
        Rect::new(b.x + x, b.y + bottom - s.height, s.width, s.height)
    }

    fn grid_cell(&self, panel: Rect, k: usize) -> Rect {
        let (c, _) = grid_shape(self.entries.len());
        let (col, row) = (k % c, k / c);
        Rect::new(
            panel.x + GRID_LEFT + col as f32 * GRID_CELL,
            panel.y + GRID_TOP + row as f32 * GRID_CELL - self.scroll,
            GRID_CELL,
            GRID_CELL,
        )
    }

    /// The fan item under `p` (popup coordinates), the folder item being `shown()`.
    fn fan_hit(&self, b: Rect, p: Point) -> Option<usize> {
        (0..=self.shown()).find(|i| {
            let (c, _, frame) = fan_slot(*i, 1.0);
            let centre = Point::new(b.x + self.anchor.x + c.x, b.y + self.anchor.y + c.y);
            (p.x - centre.x).abs() <= frame / 2.0 && (p.y - centre.y).abs() <= frame / 2.0
        })
    }

    fn open_item(&mut self, cx: &mut EventCx<'_>, k: usize) {
        let path = self
            .entries
            .get(k)
            .filter(|_| self.view == View::Grid || k < self.shown())
            .map_or_else(|| self.folder.clone(), |e| e.path.clone());
        let now = cx.now();
        self.close(now, CloseBy::File);
        cx.animate();
        super::open_path(&path);
    }
}

impl vetro_lib::core::Widget for StackView {
    fn kind(&self) -> &'static str {
        "Stack"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(500.0, 600.0))
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        cx.request_paint();
        if let Some((start, dur, _)) = self.closing
            && now - start >= dur
        {
            if let Some(f) = self.on_gone.take() {
                f(cx.ui());
            }
            return false;
        }
        self.closing.is_some() || self.progress(now) < 1.0
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let now = cx.now();
        let b = cx.bounds();
        let p = self.progress(now);
        let dark = self.dark;
        let view = cx.id();
        let theme = *cx.theme();
        match self.view {
            View::Fan => {
                let (fill, border, ink) = if dark {
                    (
                        Color::from_u32(0x828388f2),
                        Color::from_u32(0x434347ff),
                        Color::from_u32(0xf4f4f5ff),
                    )
                } else {
                    (
                        Color::from_u32(0xbdbec2f2),
                        Color::from_u32(0x424347ff),
                        Color::from_u32(0x212122ff),
                    )
                };
                let style = theme.text(&FAN_LABEL, ink);
                let n = self.shown();
                let more = self.entries.len().saturating_sub(n);
                for i in (0..=n).rev() {
                    let (c, angle, frame) = fan_slot(i, p);
                    let centre = Point::new(b.x + self.anchor.x + c.x, b.y + self.anchor.y + c.y);
                    let rot = Transform::translate(-centre.x, -centre.y)
                        .then(Transform::rotate(angle))
                        .then(Transform::translate(centre.x, centre.y));
                    cx.rec.push_transform(rot);
                    let icon = Rect::new(
                        centre.x - FAN_ICON / 2.0,
                        centre.y - FAN_ICON / 2.0,
                        FAN_ICON,
                        FAN_ICON,
                    );
                    let name = match self.entries.get(i).filter(|_| i < n) {
                        Some(e) => {
                            if let Some(img) =
                                crate::file_icons::icon(&e.path, 128, dark, Some(view))
                            {
                                let shade = if self.pressed == Some(i) { 0.8 } else { 1.0 };
                                cx.rec.image("Icon", icon, img, shade);
                            }
                            e.name()
                        }
                        None => {
                            let d = Rect::new(
                                centre.x - FAN_FOLDER / 2.0,
                                centre.y - FAN_FOLDER / 2.0,
                                FAN_FOLDER,
                                FAN_FOLDER,
                            );
                            let (f, o, g) = if dark {
                                (0x595a5dff, 0x000000ff, 0xd6d6d6ff)
                            } else {
                                (0xbcbdc0ff, 0x666666ff, 0x3e3e3eff)
                            };
                            cx.rec.layer(
                                "Open folder",
                                d,
                                Shape::Ellipse,
                                Style::fill(Color::from_u32(f))
                                    .with_border(Border::inside(Color::from_u32(o), 1.0)),
                            );
                            cx.rec.glyph(
                                "Glyph",
                                d.inset(Insets::all(17.0)),
                                "arrow-up-right",
                                600,
                                Color::from_u32(g),
                            );
                            if more > 0 {
                                format!("{more} more in the folder")
                            } else {
                                "Open folder".to_string()
                            }
                        }
                    };
                    // The label capsule, left of the icon's frame, rotated with it.
                    if self.labels.len() <= i {
                        self.labels.resize_with(i + 1, Label::default);
                    }
                    let tw = self
                        .labels
                        .get_mut(i)
                        .map_or(0.0, |l| l.width(&name, &style))
                        .min(260.0)
                        .ceil();
                    let w = tw + 2.0 * FAN_LABEL_PADDING;
                    let l = crate::text::lay(
                        &mut self.labels,
                        i,
                        &name,
                        &style,
                        Align::Center,
                        w,
                        Some(1),
                    );
                    let rect = Rect::new(
                        centre.x - frame / 2.0 - FAN_LABEL_GAP - w,
                        centre.y - FAN_LABEL_HEIGHT / 2.0,
                        w,
                        FAN_LABEL_HEIGHT,
                    );
                    cx.rec.push_opacity(p);
                    cx.rec.layer(
                        "Label",
                        rect,
                        Shape::rounded(FAN_LABEL_HEIGHT / 2.0),
                        Style::fill(fill).with_border(Border::inside(border, 1.0)),
                    );
                    let h = l.size().height;
                    cx.rec.text(
                        "Name",
                        Rect::new(rect.x, rect.y + (rect.height - h) / 2.0, w, h),
                        l,
                        None,
                        ink,
                    );
                    cx.rec.pop();
                    cx.rec.pop();
                }
            }
            View::Grid => {
                let panel = self.panel(b);
                let tip = Point::new(
                    b.x + self
                        .anchor
                        .x
                        .clamp(panel.x - b.x + 20.0, panel.right() - b.x - 20.0),
                    panel.bottom() + GRID_TIP.1,
                );
                let scale = Transform::translate(-tip.x, -tip.y)
                    .then(Transform::scale(p.max(0.001), p.max(0.001)))
                    .then(Transform::translate(tip.x, tip.y));
                let at_rest = p >= 0.999 && self.closing.is_none();
                if !at_rest {
                    cx.rec.push_transform(scale);
                    cx.rec.push_opacity(p);
                }
                let shape = Shape::rounded(GRID_RADIUS);
                if at_rest {
                    cx.glass("Panel", panel, shape, "Glass/Thick", GlassProfile::Popover);
                } else {
                    cx.rec
                        .layer("Panel", panel, shape, theme.material_style("Glass/Thick"));
                }
                // The tip towards the tile.
                let tip_rect = Rect::new(
                    tip.x - GRID_TIP.0 / 2.0,
                    panel.bottom() - 1.0,
                    GRID_TIP.0,
                    GRID_TIP.1 + 1.0,
                );
                cx.rec.custom(
                    "Tip",
                    tip_rect,
                    std::rc::Rc::new(move |canvas: &skia_safe::Canvas, r: Rect| {
                        let mut path = skia_safe::PathBuilder::new();
                        path.move_to((r.x, r.y));
                        path.line_to((r.right(), r.y));
                        path.line_to((r.x + r.width / 2.0, r.bottom()));
                        path.close();
                        let mut paint = skia_safe::Paint::default();
                        paint.set_anti_alias(true);
                        paint.set_color(if dark {
                            skia_safe::Color::from_argb(235, 0x70, 0x71, 0x76)
                        } else {
                            skia_safe::Color::from_argb(240, 0xf8, 0xf8, 0xf8)
                        });
                        canvas.draw_path(&path.detach(), &paint);
                    }),
                );
                let ink = if dark {
                    Color::from_u32(0xf8f8f8ff)
                } else {
                    Color::from_u32(0x252525ff)
                };
                let title = theme.text(&typography::HEADLINE, ink);
                let name = self
                    .folder
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let l = crate::text::lay(
                    &mut self.labels,
                    0,
                    &name,
                    &title,
                    Align::Center,
                    panel.width - 40.0,
                    Some(1),
                );
                cx.rec.text(
                    "Title",
                    Rect::new(panel.x + 20.0, panel.y + 8.0, panel.width - 40.0, 18.0),
                    l,
                    None,
                    ink,
                );
                cx.rec.push_clip(
                    Rect::new(
                        panel.x,
                        panel.y + GRID_TOP,
                        panel.width,
                        panel.height - GRID_TOP - GRID_BOTTOM,
                    ),
                    Shape::RECT,
                );
                let caption = theme.text(&typography::CAPTION, ink);
                let count = self.entries.len();
                for k in 0..=count {
                    let r = self.grid_cell(panel, k);
                    if r.bottom() < panel.y || r.y > panel.bottom() {
                        continue;
                    }
                    let canvas = Rect::new(
                        r.x + (r.width - GRID_CANVAS) / 2.0,
                        r.y + GRID_CANVAS_TOP,
                        GRID_CANVAS,
                        GRID_CANVAS,
                    );
                    let name = match self.entries.get(k) {
                        Some(e) => {
                            if let Some(img) =
                                crate::file_icons::icon(&e.path, 200, dark, Some(view))
                            {
                                cx.rec.image(
                                    "Icon",
                                    canvas,
                                    img,
                                    if self.pressed == Some(k) { 0.8 } else { 1.0 },
                                );
                            }
                            e.name()
                        }
                        None => {
                            let d = Rect::new(canvas.x + 21.0, canvas.y + 21.0, 58.0, 58.0);
                            cx.rec.layer(
                                "Open folder",
                                d,
                                Shape::Ellipse,
                                Style::fill(Color::TRANSPARENT)
                                    .with_border(Border::inside(ink, 1.5)),
                            );
                            cx.rec.glyph(
                                "Glyph",
                                d.inset(Insets::all(17.0)),
                                "arrow-up-right",
                                600,
                                ink,
                            );
                            "Open folder".to_string()
                        }
                    };
                    let l = crate::text::lay(
                        &mut self.labels,
                        k + 1,
                        &name,
                        &caption,
                        Align::Center,
                        GRID_LABEL_WIDTH,
                        Some(1),
                    );
                    cx.rec.text(
                        "Name",
                        Rect::new(
                            r.x + (r.width - GRID_LABEL_WIDTH) / 2.0,
                            r.y + 106.0,
                            GRID_LABEL_WIDTH,
                            16.0,
                        ),
                        l,
                        None,
                        ink,
                    );
                }
                cx.rec.pop();
                if !at_rest {
                    cx.rec.pop();
                    cx.rec.pop();
                }
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        let now = cx.now();
        let hit = |s: &Self, p: Point| -> Option<usize> {
            match s.view {
                View::Fan => s.fan_hit(b, p),
                View::Grid => {
                    let panel = s.panel(b);
                    (0..=s.entries.len())
                        .find(|k| s.grid_cell(panel, *k).contains(p) && panel.contains(p))
                }
            }
        };
        match event {
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                let inside = match self.view {
                    View::Fan => self.fan_hit(b, *pos).is_some(),
                    View::Grid => self.panel(b).contains(*pos),
                };
                if !inside {
                    // A press outside closes on the press.
                    self.close(now, CloseBy::Outside);
                    cx.animate();
                } else {
                    self.pressed = hit(self, *pos);
                    cx.request_paint();
                }
                Handled::Yes
            }
            Event::PointerUp {
                pos,
                button: Button::Left,
                ..
            } => {
                let pressed = self.pressed.take();
                if let Some(k) = pressed
                    && hit(self, *pos) == Some(k)
                {
                    self.open_item(cx, k);
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::Scroll { dy, .. } if self.view == View::Grid => {
                let panel = self.panel(b);
                let (_, rows) = grid_shape(self.entries.len());
                let content = rows as f32 * GRID_CELL;
                let visible = panel.height - GRID_TOP - GRID_BOTTOM;
                self.scroll = (self.scroll + dy).clamp(0.0, (content - visible).max(0.0));
                cx.request_paint();
                Handled::Yes
            }
            Event::Key(k) if k.pressed && k.key == Key::Escape => {
                self.close(now, CloseBy::Esc);
                cx.animate();
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn hit(&self, _p: Point, _size: Size) -> bool {
        true
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Group;
        a.label = self
            .folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
    }
}

/// Opens the stack of `folder` as `view` above the tile whose centre is `centre_x` in the Dock
/// surface `surface`, the Dock area's top at `zone_top`. `on_gone` runs when it has closed.
pub fn open(
    ui: &mut Ui,
    at: Place,
    folder: &Path,
    view: View,
    dark: bool,
    on_gone: Gone,
) -> Option<(WindowId, vetro_lib::core::ViewId)> {
    let Place {
        surface,
        width: surface_width,
        centre_x,
        zone_top,
    } = at;
    let s = settings(folder);
    let list = entries(folder, s.sort);
    let screen_h = ui.outputs().first().map_or(800.0, |o| o.size.height);
    let room =
        (zone_top + screen_h - (ui.window(surface).map_or(0.0, |w| w.size.height))).max(200.0);
    let (width, height, ax) = match view {
        View::Fan => {
            let n = list.len().min(FAN_MAX) + 1;
            let h = (FAN_ABOVE + 80.0 + FAN_PITCH * n as f32 + 24.0).min(room);
            (620.0, h, 360.0)
        }
        View::Grid => {
            let s = grid_size(list.len(), room - GRID_TIP.1 - GRID_TIP_ABOVE - 16.0);
            let w = (s.width + 40.0).min(surface_width);
            (w, s.height + GRID_TIP.1 + GRID_TIP_ABOVE + 24.0, w / 2.0)
        }
    };
    // The popup's left edge in the Dock surface, kept on screen; the anchor follows the tile.
    let left = (centre_x - ax).clamp(0.0, (surface_width - width).max(0.0));
    let top = zone_top - height;
    let size = Size::new(width, height);
    let w = ui.create_window(
        WindowKind::Popup {
            parent: surface,
            anchor: Rect::new(left, top - height, width, height),
            grab: true,
        },
        size,
    );
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
    }
    let now = ui.now;
    let delay = match view {
        View::Fan => FAN_OPEN.2,
        View::Grid => GRID_OPEN.2,
    };
    let v = ui.create(StackView {
        folder: folder.to_path_buf(),
        entries: list,
        view,
        anchor: Point::new(centre_x - left, height),
        opened: now + delay,
        closing: None,
        labels: Vec::new(),
        pressed: None,
        scroll: 0.0,
        dark,
        on_gone: Some(on_gone),
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, v);
    ui.show_window(w);
    ui.set_focus(v, false);
    ui.start_animating(v);
    // A dismissed popup (a click outside the grab) closes at once.
    if let Some(root) = ui.window(w).map(|x| x.root) {
        ui.connect(root, Signal::Closed, move |ui, _, _| {
            let gone = ui
                .with_widget::<StackView, _>(v, |s| s.on_gone.take())
                .flatten();
            if let Some(f) = gone {
                f(ui);
            }
        });
    }
    Some((w, v))
}

/// Starts closing the open stack view `v`.
pub fn close(ui: &mut Ui, v: vetro_lib::core::ViewId, by: CloseBy) {
    let now = ui.now;
    ui.with_widget::<StackView, _>(v, |s| s.close(now, by));
    ui.start_animating(v);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fan_follows_the_measurements() {
        // Rotation ~0.4° + 1.23° k (k from 1); centres 64 apart; each shifted right by 64 tan θ.
        let (c0, a0, f0) = fan_slot(0, 1.0);
        assert!((a0.to_degrees() - 1.63).abs() < 1e-3);
        assert!((f0 - 64.0 * (a0.cos() + a0.sin())).abs() < 1e-3);
        assert!((c0.y + FAN_ABOVE + f0 / 2.0).abs() < 1e-3);
        let (c9, a9, _) = fan_slot(9, 1.0);
        assert!((a9.to_degrees() - (1.63 + 1.23 * 9.0)).abs() < 1e-3);
        assert!((c0.y - c9.y - 9.0 * 64.0).abs() < 1e-3);
        // The column bends more and more to the right: steps of 2 to 15 pt.
        let steps: Vec<f32> = (1..10)
            .map(|i| fan_slot(i, 1.0).0.x - fan_slot(i - 1, 1.0).0.x)
            .collect();
        assert!(steps.windows(2).all(|w| w[1] > w[0]));
        assert!(steps[0] > 1.5 && steps[0] < 5.0 && steps[8] > 12.0 && steps[8] < 17.0);
        // Closed: an upright stack over the icon.
        let (c, a, _) = fan_slot(5, 0.0);
        assert_eq!((c.x, a), (0.0, 0.0));
    }

    #[test]
    fn automatic_view_and_grid_shape() {
        assert_eq!(automatic_view(15), View::Fan);
        assert_eq!(automatic_view(16), View::Grid);
        assert_eq!(grid_shape(3), (2, 2));
        assert_eq!(grid_shape(10), (4, 3));
        assert_eq!(grid_shape(14), (4, 4));
        assert_eq!(grid_shape(40), (7, 6));
        // 2 columns make 290 pt, 4 make 546 (dock-stacks.md "Grid").
        assert_eq!(grid_size(3, 2000.0).width, 290.0);
        assert_eq!(grid_size(10, 2000.0).width, 546.0);
        assert_eq!(grid_size(40, 300.0).height, 300.0);
        assert_eq!(
            grid_size(40, 10.0).height,
            GRID_TOP + GRID_CELL + GRID_BOTTOM
        );
    }

    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("documento-2", "documento-10"), Ordering::Less);
        assert_eq!(natural_cmp("B", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
    }

    #[test]
    fn stack_order_by_date_and_kind() {
        let t = |s: u64| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(s);
        let e = |n: &str, m: u64| Entry {
            path: PathBuf::from(format!("/x/{n}")),
            dir: false,
            modified: t(m),
            changed: t(m),
        };
        let mut list = vec![e("b.txt", 1), e("a.pdf", 3), e("c.txt", 2)];
        sort_entries(&mut list, Sort::Modified);
        let names: Vec<String> = list.iter().map(Entry::name).collect();
        assert_eq!(names, ["a.pdf", "c.txt", "b.txt"]);
        sort_entries(&mut list, Sort::Kind);
        let names: Vec<String> = list.iter().map(Entry::name).collect();
        assert_eq!(names, ["a.pdf", "b.txt", "c.txt"]);
    }

    #[test]
    fn settings_round_trip_by_path() {
        let dir = std::env::temp_dir().join(format!("silica-stack-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("dock-stacks.json");
        let folder = Path::new("/srv/folder");
        assert_eq!(settings_in(Some(&file), folder), Settings::default());
        let s = Settings {
            sort: Sort::Kind,
            display: Display::Folder,
            content: Content::Grid,
        };
        save_in(&file, folder, s);
        assert_eq!(settings_in(Some(&file), folder), s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
