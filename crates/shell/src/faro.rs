//! Faro and the Launcher (docs/spec/faro.md, launcher.md): one panel that changes shape. Faro
//! is the search bar (640 pt, top at 30 % of the screen) that grows a list of results — apps,
//! files in the home folder, a calculation, a web search when nothing matches; the Launcher is
//! the 844 × 577 app grid with its category capsules and a row of recent apps. Esc turns the
//! Launcher back into the Faro bar; a second Esc, or a click outside, closes.
//!
//! The panel lives in an Overlay surface over the whole output that takes the keyboard while
//! open, so a click anywhere else closes it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use vetro_lib::amalfi::foundation::Label;
use vetro_lib::amalfi::tokens::{TypeStyle, typography};
use vetro_lib::color::Color;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::display::GlassProfile;
use vetro_lib::core::event::Key;
use vetro_lib::core::shell::ShellRequest;
use vetro_lib::core::widget::FocusKind;
use vetro_lib::core::window::{Edges, KeyboardMode, Layer};
use vetro_lib::core::{
    Button, Event, EventCx, Handled, LayoutCx, PaintCx, Prop, Proposal, Ui, Value, ViewId,
    WindowId, WindowKind,
};
use vetro_lib::geom::{Point, Rect, Size};
use vetro_lib::shape::Shape;
use vetro_lib::style::{Border, Style};
use vetro_lib::text::Align;

use crate::apps::{self, DesktopEntry};
use crate::motion::Bezier;

/// Measures (faro.md, launcher.md).
pub mod metrics {
    /// Faro's width.
    pub const FARO_WIDTH: f32 = 640.0;
    /// The empty bar's height (and a result row's).
    pub const BAR: f32 = 57.0;
    /// The header's height.
    pub const HEADER: f32 = 56.0;
    /// A result row.
    pub const ROW: f32 = 57.0;
    /// Faro's tallest panel.
    pub const FARO_MAX: f32 = 467.0;
    /// Faro's corner radius with results.
    pub const FARO_RADIUS: f32 = 28.5;
    /// The Launcher's size.
    pub const LAUNCHER: (f32, f32) = (844.0, 577.0);
    /// The Launcher's corner radius.
    pub const LAUNCHER_RADIUS: f32 = 25.0;
    /// The capsule zone.
    pub const CAPSULES: f32 = 47.0;
    /// Grid columns.
    pub const COLUMNS: usize = 7;
    /// Grid cell pitch.
    pub const PITCH: f32 = 112.5;
    /// Grid cell.
    pub const CELL: (f32, f32) = (112.0, 92.0);
    /// Grid row pitch.
    pub const ROW_PITCH: f32 = 102.0;
    /// The grid's icon.
    pub const GRID_ICON: f32 = 61.0;
    /// The text's left edge (the caret's).
    pub const TEXT_X: f32 = 61.0;
}

/// The field's type: 26 pt regular.
const FIELD: TypeStyle = TypeStyle {
    name: "Faro Field",
    weight: 400,
    ..typography::TITLE_1
};

/// Results' titles.
const ROW_TITLE: TypeStyle = TypeStyle {
    name: "Faro Row",
    size: 17.0,
    line_height: Some(22.0),
    weight: 400,
    ..typography::BODY
};

/// The panel's two shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The search bar and its list.
    Faro,
    /// The app grid.
    Launcher,
}

/// A result.
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    /// An app.
    App(DesktopEntry),
    /// A file or folder.
    File(PathBuf, bool),
    /// A calculation and its value.
    Calc(String, f64),
    /// A web search.
    Web(String),
}

/// The Launcher's categories (desktop entry categories in each).
const CATEGORIES: &[(&str, &[&str])] = &[
    (
        "Productivity",
        &[
            "Office",
            "Calendar",
            "ContactManagement",
            "Finance",
            "Presentation",
            "Spreadsheet",
            "WordProcessor",
        ],
    ),
    (
        "Creativity",
        &[
            "Graphics",
            "2DGraphics",
            "3DGraphics",
            "Photography",
            "AudioVideoEditing",
            "Music",
        ],
    ),
    (
        "Utilities",
        &[
            "Utility",
            "System",
            "Settings",
            "Accessibility",
            "FileTools",
            "FileManager",
            "TerminalEmulator",
        ],
    ),
    (
        "Social",
        &[
            "Network",
            "Chat",
            "Email",
            "InstantMessaging",
            "VideoConference",
        ],
    ),
    (
        "Entertainment",
        &["Game", "AudioVideo", "Player", "Video", "Audio"],
    ),
    (
        "Information",
        &[
            "Education",
            "Science",
            "Dictionary",
            "Viewer",
            "News",
            "Maps",
        ],
    ),
    (
        "Developer Tools",
        &["Development", "IDE", "Debugger", "RevisionControl"],
    ),
];

/// The home folder's files (path, folder), filled by a thread.
type FileIndex = Arc<Mutex<Option<Vec<(PathBuf, bool)>>>>;

/// Faro's state.
#[derive(Default)]
pub struct FaroState {
    /// The surface and its view.
    pub window: Option<(WindowId, ViewId)>,
    /// The shape.
    pub mode: Option<Mode>,
    /// The typed text.
    pub query: String,
    /// The results.
    pub hits: Vec<Hit>,
    /// The selected result (Faro) or grid cell (Launcher).
    pub selected: Option<usize>,
    /// Every app.
    apps: Vec<DesktopEntry>,
    /// The chosen category.
    pub category: Option<usize>,
    /// The grid's scroll.
    scroll: f32,
    /// The home folder's files, indexed on a thread.
    files: FileIndex,
    indexing: bool,
    /// The output's size.
    output: Size,
    /// Shape animation: from where, since when, how long.
    anim: Option<(Rect, f64, f64)>,
    /// When it opened.
    opened: f64,
    /// The pressed hit.
    pressed: Option<usize>,
}

/// Shared.
pub type Shared = Rc<RefCell<FaroState>>;

fn state_file() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(dir.join("silica/recent-apps.json"))
}

/// The recently opened apps, the newest first (the Flutter shell's file: a JSON list).
pub fn recents() -> Vec<String> {
    state_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .map(|mut v| {
            v.truncate(metrics::COLUMNS);
            v
        })
        .unwrap_or_default()
}

/// Notes that app `id` was opened.
pub fn note_launched(id: &str) {
    let mut list = recents();
    list.retain(|l| l != id);
    list.insert(0, id.to_string());
    list.truncate(metrics::COLUMNS);
    if let Some(p) = state_file() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(t) = serde_json::to_string(&list) {
            let _ = std::fs::write(p, t);
        }
    }
}

/// Evaluates an arithmetic expression (+ − × ÷ ^ %, parentheses, decimals).
pub fn calculate(text: &str) -> Option<f64> {
    let t: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '×' | 'x' => '*',
            '÷' => '/',
            ',' => '.',
            '−' => '-',
            c => c,
        })
        .collect();
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_op = t.chars().skip(1).any(|c| "+-*/^%".contains(c));
    if !has_digit || !has_op || !t.chars().all(|c| "0123456789.+-*/^%()".contains(c)) {
        return None;
    }
    let chars: Vec<char> = t.chars().collect();
    let mut p = Parser { s: &chars, i: 0 };
    let v = p.sum()?;
    (p.i == chars.len() && v.is_finite()).then_some(v)
}

struct Parser<'a> {
    s: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product()?;
        while let Some(c) = self.peek() {
            match c {
                '+' => {
                    self.i += 1;
                    v += self.product()?;
                }
                '-' => {
                    self.i += 1;
                    v -= self.product()?;
                }
                _ => break,
            }
        }
        Some(v)
    }

    fn product(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        while let Some(c) = self.peek() {
            match c {
                '*' => {
                    self.i += 1;
                    v *= self.power()?;
                }
                '/' => {
                    self.i += 1;
                    v /= self.power()?;
                }
                _ => break,
            }
        }
        Some(v)
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.peek() == Some('^') {
            self.i += 1;
            let e = self.power()?;
            return Some(base.powf(e));
        }
        Some(base)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some('-') => {
                self.i += 1;
                Some(-self.unary()?)
            }
            Some('+') => {
                self.i += 1;
                self.unary()
            }
            _ => {
                let v = self.atom()?;
                if self.peek() == Some('%') {
                    self.i += 1;
                    return Some(v / 100.0);
                }
                Some(v)
            }
        }
    }

    fn atom(&mut self) -> Option<f64> {
        if self.peek() == Some('(') {
            self.i += 1;
            let v = self.sum()?;
            if self.peek() != Some(')') {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.i += 1;
        }
        let n: String = self.s.get(start..self.i)?.iter().collect();
        n.parse().ok()
    }
}

/// A value as the field shows it.
pub fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Indexes the home folder's files (four levels, no hidden ones) on a thread.
fn index_files(out: FileIndex) {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    std::thread::spawn(move || {
        let mut found = Vec::new();
        let mut stack = vec![(home, 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                let p = e.path();
                if is_dir && depth < 4 {
                    stack.push((p.clone(), depth + 1));
                }
                found.push((p, is_dir));
                if found.len() >= 50_000 {
                    break;
                }
            }
        }
        if let Ok(mut g) = out.lock() {
            *g = Some(found);
        }
    });
}

fn matches(hay: &str, needle: &str) -> Option<usize> {
    let h = hay.to_lowercase();
    let pos = h.find(needle)?;
    // Word starts rank first.
    let word = pos == 0 || h[..pos].ends_with([' ', '-', '_', '.']);
    Some(if pos == 0 {
        0
    } else if word {
        1
    } else {
        2
    })
}

impl FaroState {
    /// The results for the query.
    fn search(&mut self) {
        let q = self.query.trim().to_lowercase();
        let mut hits = Vec::new();
        if q.is_empty() {
            self.hits = hits;
            self.selected = None;
            return;
        }
        if let Some(v) = calculate(&q) {
            hits.push(Hit::Calc(self.query.trim().to_string(), v));
        }
        let mut apps: Vec<(usize, &DesktopEntry)> = self
            .apps
            .iter()
            .filter_map(|e| {
                let by_name = matches(&e.name, &q);
                let by_word = e
                    .keywords
                    .iter()
                    .filter_map(|k| matches(k, &q))
                    .min()
                    .map(|r| r + 3);
                by_name.or(by_word).map(|r| (r, e))
            })
            .collect();
        apps.sort_by_key(|(r, e)| (*r, e.name.to_lowercase()));
        let limit = if self.mode == Some(Mode::Launcher) {
            70
        } else {
            8
        };
        hits.extend(
            apps.into_iter()
                .take(limit)
                .map(|(_, e)| Hit::App(e.clone())),
        );
        if self.mode == Some(Mode::Faro)
            && let Ok(g) = self.files.lock()
            && let Some(files) = g.as_ref()
        {
            let mut f: Vec<(usize, &(PathBuf, bool))> = files
                .iter()
                .filter_map(|e| {
                    let name = e.0.file_name()?.to_string_lossy();
                    matches(&name, &q).map(|r| (r, e))
                })
                .take(400)
                .collect();
            f.sort_by_key(|(r, e)| (*r, e.0.as_os_str().len()));
            hits.extend(
                f.into_iter()
                    .take(6)
                    .map(|(_, e)| Hit::File(e.0.clone(), e.1)),
            );
        }
        if self.mode == Some(Mode::Faro) && !hits.iter().any(|h| !matches!(h, Hit::Calc(..))) {
            hits.push(Hit::Web(self.query.trim().to_string()));
        }
        self.hits = hits;
        self.selected = (!self.hits.is_empty()).then_some(0);
    }

    /// The apps the grid shows, and how many of them are the recents row.
    fn grid(&self) -> (Vec<DesktopEntry>, usize) {
        if !self.query.trim().is_empty() {
            let apps = self
                .hits
                .iter()
                .filter_map(|h| match h {
                    Hit::App(e) => Some(e.clone()),
                    _ => None,
                })
                .collect();
            return (apps, 0);
        }
        if let Some(c) = self.category.and_then(|c| CATEGORIES.get(c)) {
            let apps = self
                .apps
                .iter()
                .filter(|e| e.categories.iter().any(|x| c.1.contains(&x.as_str())))
                .cloned()
                .collect();
            return (apps, 0);
        }
        let recent: Vec<DesktopEntry> = recents()
            .iter()
            .filter_map(|id| self.apps.iter().find(|e| &e.id == id).cloned())
            .take(metrics::COLUMNS)
            .collect();
        let n = recent.len();
        let mut all = recent;
        all.extend(self.apps.iter().cloned());
        (all, n)
    }

    /// The panel's frame for the current state.
    fn target(&self) -> Rect {
        let out = self.output;
        let (w, h, top) = match self.mode {
            Some(Mode::Launcher) => (metrics::LAUNCHER.0, metrics::LAUNCHER.1, 0.3),
            _ => {
                let list = self.list_height();
                let h = if list > 0.0 {
                    (metrics::HEADER + 1.0 + list).min(metrics::FARO_MAX)
                } else {
                    metrics::BAR
                };
                (metrics::FARO_WIDTH, h, 0.3)
            }
        };
        let y = (out.height * top).min(out.height - h - 24.0).max(8.0);
        Rect::new(((out.width - w) / 2.0).round(), y.round(), w, h)
    }

    fn list_height(&self) -> f32 {
        self.hits
            .iter()
            .map(|h| match h {
                Hit::Calc(..) => 88.0,
                _ => metrics::ROW,
            })
            .sum::<f32>()
            + if self.hits.is_empty() { 0.0 } else { 8.0 }
    }

    /// The panel's frame now.
    fn frame(&self, now: f64) -> Rect {
        let to = self.target();
        let Some((from, start, dur)) = self.anim else {
            return to;
        };
        let t = (((now - start) / dur) as f32).clamp(0.0, 1.0);
        let e = Bezier(0.25, 0.8, 0.25, 1.0).at(t);
        Rect::new(
            from.x + (to.x - from.x) * e,
            from.y + (to.y - from.y) * e,
            from.width + (to.width - from.width) * e,
            from.height + (to.height - from.height) * e,
        )
    }

    fn radius(&self, r: Rect) -> f32 {
        match self.mode {
            Some(Mode::Launcher) => metrics::LAUNCHER_RADIUS,
            _ if r.height <= metrics::BAR + 0.5 => r.height / 2.0,
            _ => metrics::FARO_RADIUS,
        }
    }

    /// Changes the shape with an animation from where it is now.
    fn reshape(&mut self, now: f64, f: impl FnOnce(&mut FaroState)) {
        let from = self.frame(now);
        f(self);
        if self.target() != from {
            self.anim = Some((from, now, 0.22));
        }
    }
}

/// Opens Faro or the Launcher; the same command again closes it.
pub fn toggle(ui: &mut Ui, state: &Shared, mode: Mode) {
    let open = state.borrow().mode;
    if open == Some(mode) {
        close(ui, state);
        return;
    }
    let now = ui.now;
    if open.is_some() {
        state.borrow_mut().reshape(now, |s| {
            s.mode = Some(mode);
            s.category = None;
            s.scroll = 0.0;
            s.search();
        });
        animate(ui, state);
        return;
    }
    let out = ui
        .outputs()
        .first()
        .map_or(Size::new(1280.0, 800.0), |o| o.size);
    {
        let mut s = state.borrow_mut();
        if s.apps.is_empty() {
            s.apps = apps::all();
        }
        if !s.indexing {
            s.indexing = true;
            index_files(s.files.clone());
        }
        s.mode = Some(mode);
        s.output = out;
        s.category = None;
        s.scroll = 0.0;
        s.search();
        // Opening: the whole panel ~7 % larger, settling to size.
        let to = s.target();
        let k = if mode == Mode::Launcher { 0.07 } else { 0.0 };
        let dh = if mode == Mode::Faro {
            to.height * 0.034
        } else {
            0.0
        };
        let from = Rect::new(
            to.x - to.width * k / 2.0,
            to.y - to.height * k / 2.0 + dh / 2.0,
            to.width * (1.0 + k),
            to.height * (1.0 + k) - dh,
        );
        s.anim = Some((from, now, 0.19));
        s.opened = now;
    }
    let output = ui.outputs().first().map(|o| o.name.clone());
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Overlay,
            anchor: Edges::TOP | Edges::BOTTOM | Edges::LEFT | Edges::RIGHT,
            exclusive: -1,
            margin: [0; 4],
            keyboard: KeyboardMode::Exclusive,
            namespace: "faro".into(),
            output,
        },
        out,
    );
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
    }
    let view = ui.create(FaroView {
        state: state.clone(),
        labels: Vec::new(),
        field: Label::default(),
        blink: true,
    });
    let _ = ui.set(view, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(view, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, view);
    ui.show_window(w);
    ui.set_focus(view, false);
    state.borrow_mut().window = Some((w, view));
    animate(ui, state);
}

/// Closes the panel.
pub fn close(ui: &mut Ui, state: &Shared) {
    let w = {
        let mut s = state.borrow_mut();
        s.mode = None;
        s.anim = None;
        s.pressed = None;
        s.window.take()
    };
    if let Some((w, _)) = w {
        ui.close_window(w);
    }
}

fn animate(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().window {
        ui.request_paint(v);
        ui.start_animating(v);
    }
}

/// Opens hit `h` and closes.
fn open_hit(ui: &mut Ui, state: &Shared, h: &Hit) {
    match h {
        Hit::App(e) => {
            note_launched(&e.id);
            let params = serde_json::json!({ "app": e.id }).to_string();
            ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
        }
        Hit::File(p, _) => crate::dock::open_path(p),
        Hit::Calc(_, v) => ui.set_clipboard(format_number(*v)),
        Hit::Web(q) => {
            let mut url = String::from("https://duckduckgo.com/?q=");
            for b in q.bytes() {
                if b.is_ascii_alphanumeric() {
                    url.push(char::from(b));
                } else {
                    url.push_str(&format!("%{b:02X}"));
                }
            }
            vetro_lib::platform::spawn(async move {
                if let Some(s) = vetro_lib::platform::services()
                    && let Ok(p) = s.apps().await
                {
                    let _ = p.open_uris(&[url.as_str()]).await;
                }
            });
        }
    }
    close(ui, state);
}

/// The panel.
struct FaroView {
    state: Shared,
    labels: Vec<Label>,
    field: Label,
    blink: bool,
}

/// Colours of the panel's text (faro.md).
struct Inks {
    text: Color,
    placeholder: Color,
    secondary: Color,
    selection: Color,
    separator: Color,
    capsule: Color,
    capsule_text: Color,
}

fn inks(dark: bool) -> Inks {
    let c = |v: u32| Color::from_u32(v);
    if dark {
        Inks {
            text: c(0xffffffff),
            placeholder: c(0xedeef2ff),
            secondary: c(0xb2b3b7ff),
            selection: c(0x94959980),
            separator: c(0x94959966),
            capsule: c(0x82838799),
            capsule_text: c(0xfeffffff),
        }
    } else {
        Inks {
            text: c(0x16171cff),
            placeholder: c(0x494a4fff),
            secondary: c(0x88898eff),
            selection: c(0xafb0b580),
            separator: c(0xafb0b599),
            capsule: c(0xd9dadfcc),
            capsule_text: c(0x5a5b60ff),
        }
    }
}

/// Lays text out with label `i` (made when missing).
fn lay(
    labels: &mut Vec<Label>,
    i: usize,
    text: &str,
    style: &vetro_lib::text::TextStyle,
    align: Align,
    width: f32,
    lines: Option<usize>,
) -> Rc<vetro_lib::text::TextLayout> {
    if labels.len() <= i {
        labels.resize_with(i + 1, Label::default);
    }
    match labels.get_mut(i) {
        Some(l) => l.layout(text, style, align, width, lines),
        None => Label::default().layout(text, style, align, width, lines),
    }
}

impl FaroView {
    /// Where the list's rows (Faro) are, panel-relative.
    fn rows(s: &FaroState, panel: Rect) -> Vec<(usize, Rect)> {
        let mut y = panel.y + metrics::HEADER + 1.0 + 4.0;
        let mut out = Vec::new();
        for (i, h) in s.hits.iter().enumerate() {
            let hgt = if matches!(h, Hit::Calc(..)) {
                88.0
            } else {
                metrics::ROW
            };
            out.push((i, Rect::new(panel.x + 2.0, y, panel.width - 4.0, hgt)));
            y += hgt;
        }
        out
    }

    /// The Launcher's capsules.
    fn capsules(&mut self, theme: &vetro_lib::amalfi::theme::Theme, panel: Rect) -> Vec<Rect> {
        let style = theme.text(&typography::BODY, Color::BLACK);
        let mut x = panel.x + 20.0;
        let y = panel.y + metrics::HEADER + 1.0 + 10.0;
        let mut out = Vec::new();
        for (name, _) in CATEGORIES {
            let w = (self.field.width(name, &style) + 18.0).max(85.0).ceil();
            out.push(Rect::new(x, y, w, 26.0));
            x += w + 8.0;
        }
        out
    }

    /// The grid's cells, panel-relative, with the recents' separator's y.
    fn cells(s: &FaroState, panel: Rect, n: usize, recent: usize) -> (Vec<Rect>, Option<f32>) {
        let top = panel.y
            + metrics::HEADER
            + 1.0
            + if s.category.is_none() && s.query.trim().is_empty() {
                metrics::CAPSULES
            } else {
                12.0
            }
            - s.scroll;
        let mut out = Vec::with_capacity(n);
        let mut sep = None;
        for k in 0..n {
            let (row, col, extra) = if recent > 0 && k >= recent {
                let j = k - recent;
                (1 + j / metrics::COLUMNS, j % metrics::COLUMNS, 15.0)
            } else {
                (k / metrics::COLUMNS, k % metrics::COLUMNS, 0.0)
            };
            let y = top + row as f32 * metrics::ROW_PITCH + extra;
            out.push(Rect::new(
                panel.x + 20.0 + col as f32 * metrics::PITCH,
                y,
                metrics::CELL.0,
                metrics::CELL.1,
            ));
        }
        if recent > 0 {
            sep = Some(top + metrics::CELL.1 + 13.0);
        }
        (out, sep)
    }

    fn key(&mut self, cx: &mut EventCx<'_>, k: &Key) -> bool {
        let now = cx.now();
        let state = self.state.clone();
        let mode = state.borrow().mode;
        match k {
            Key::Escape => {
                let mut s = state.borrow_mut();
                if mode == Some(Mode::Launcher) && s.category.is_some() {
                    s.reshape(now, |s| s.category = None);
                } else if mode == Some(Mode::Launcher) {
                    s.reshape(now, |s| {
                        s.mode = Some(Mode::Faro);
                        s.search();
                    });
                } else if !s.query.is_empty() {
                    s.reshape(now, |s| {
                        s.query.clear();
                        s.search();
                    });
                } else {
                    drop(s);
                    close(cx.ui(), &state);
                    return true;
                }
            }
            Key::Backspace => {
                let mut s = state.borrow_mut();
                if s.query.is_empty() && s.category.is_some() {
                    s.reshape(now, |s| s.category = None);
                } else {
                    s.reshape(now, |s| {
                        s.query.pop();
                        s.scroll = 0.0;
                        s.search();
                    });
                }
            }
            Key::Down | Key::Up | Key::Left | Key::Right => {
                let mut s = state.borrow_mut();
                let n = if mode == Some(Mode::Launcher) {
                    s.grid().0.len()
                } else {
                    s.hits.len()
                };
                if n == 0 {
                    return true;
                }
                let step: isize = match (mode, k) {
                    (Some(Mode::Launcher), Key::Down) => metrics::COLUMNS as isize,
                    (Some(Mode::Launcher), Key::Up) => -(metrics::COLUMNS as isize),
                    (_, Key::Down | Key::Right) => 1,
                    _ => -1,
                };
                let cur = s.selected.map_or(-1, |c| c as isize);
                let next = if cur < 0 {
                    0
                } else {
                    (cur + step).clamp(0, n as isize - 1)
                };
                s.selected = usize::try_from(next).ok();
            }
            Key::Enter => {
                let s = state.borrow();
                let hit = if mode == Some(Mode::Launcher) {
                    let (apps, _) = s.grid();
                    s.selected.and_then(|i| apps.get(i).cloned()).map(Hit::App)
                } else {
                    s.selected.and_then(|i| s.hits.get(i).cloned())
                };
                drop(s);
                if let Some(h) = hit {
                    open_hit(cx.ui(), &state, &h);
                }
                return true;
            }
            Key::Tab => {}
            _ => return false,
        }
        cx.request_paint();
        cx.animate();
        true
    }
}

impl vetro_lib::core::Widget for FaroView {
    fn kind(&self) -> &'static str {
        "Faro"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(self.state.borrow().output)
    }

    fn focus(&self) -> FocusKind {
        FocusKind::Always
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        let mut s = self.state.borrow_mut();
        // The caret fades rather than blinks; its period is about 1 s.
        let on = now.rem_euclid(1.0) < 0.62;
        if on != self.blink {
            self.blink = on;
        }
        if let Some((_, start, dur)) = s.anim
            && now - start >= dur
        {
            s.anim = None;
        }
        // Files arrive from the indexer.
        let ready = s.files.lock().is_ok_and(|g| g.is_some());
        if ready && s.indexing && s.mode == Some(Mode::Faro) && !s.query.is_empty() {
            s.indexing = false;
            s.search();
        }
        cx.request_paint();
        s.mode.is_some()
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let theme = *cx.theme();
        let dark = theme.dark();
        let ink = inks(dark);
        let now = cx.now();
        let b = cx.bounds();
        let st = self.state.clone();
        let s = st.borrow();
        let Some(mode) = s.mode else { return };
        let panel = s.frame(now).translate(b.origin());
        let fade = (((now - s.opened) / 0.06) as f32).clamp(0.0, 1.0) * 0.78 + 0.22;
        cx.rec.push_opacity(fade);
        cx.glass(
            "Panel",
            panel,
            Shape::rounded(s.radius(panel)),
            "Glass/Thick",
            GlassProfile::Panel,
        );
        cx.rec.push_clip(panel, Shape::rounded(s.radius(panel)));
        // Header: glyph, field, caret.
        let hy = panel.y + (metrics::BAR.min(panel.height) - 25.0) / 2.0;
        let glyph = if mode == Mode::Launcher {
            "square-grid-2x2"
        } else {
            "search"
        };
        cx.rec.glyph_sized(
            "Glyph",
            Rect::new(panel.x + 22.0, hy, 25.0, 25.0),
            23.0,
            glyph,
            400,
            ink.placeholder,
        );
        let mut tx = panel.x + metrics::TEXT_X;
        if let Some(c) = s.category.and_then(|c| CATEGORIES.get(c)) {
            let style = theme.text(&FIELD, ink.text);
            let w = self.field.width(c.0, &style) + 12.0;
            let tok = Rect::new(tx - 6.0, panel.y + 13.0, w, 31.0);
            cx.rec.layer(
                "Token",
                tok,
                Shape::rounded(10.0),
                Style::fill(ink.selection),
            );
            let l = self.field.layout(c.0, &style, Align::Start, w, Some(1));
            cx.rec.text(
                "Token text",
                Rect::new(tx, panel.y + 12.0, w, 32.0),
                l,
                None,
                ink.text,
            );
            tx += w + 2.0;
        }
        let (text, color) = if s.query.is_empty() {
            let p = if mode == Mode::Launcher {
                "Search apps"
            } else {
                "Faro Search"
            };
            (p.to_string(), ink.placeholder)
        } else {
            (s.query.clone(), ink.text)
        };
        let style = theme.text(&FIELD, color);
        let fw = (panel.right() - tx - 60.0).max(10.0);
        let l = self.field.layout(&text, &style, Align::Start, fw, Some(1));
        let typed_w = if s.query.is_empty() {
            0.0
        } else {
            l.size().width
        };
        cx.rec.text(
            "Field",
            Rect::new(tx, panel.y + 12.0, fw, 32.0),
            l,
            None,
            color,
        );
        if self.blink {
            cx.rec.layer(
                "Caret",
                Rect::new(tx + typed_w, panel.y + 13.0, 2.0, 31.0),
                Shape::rounded(1.0),
                Style::fill(ink.text),
            );
        }
        if panel.height > metrics::BAR + 1.0 {
            cx.rec.layer(
                "Separator",
                Rect::new(
                    panel.x + 20.0,
                    panel.y + metrics::HEADER,
                    panel.width - 40.0,
                    1.0,
                ),
                Shape::rounded(0.0),
                Style::fill(ink.separator),
            );
        }
        match mode {
            Mode::Faro => {
                let title = theme.text(&ROW_TITLE, ink.text);
                let sub = theme.text(&typography::CALLOUT, ink.secondary);
                let mut li = 0usize;
                for (i, r) in Self::rows(&s, panel) {
                    if r.y > panel.bottom() {
                        break;
                    }
                    let Some(h) = s.hits.get(i) else { continue };
                    if s.selected == Some(i) {
                        cx.rec.layer(
                            "Selection",
                            Rect::new(r.x + 8.0, r.y, r.width - 16.0, r.height),
                            Shape::rounded(16.5),
                            Style::fill(ink.selection),
                        );
                    }
                    let (name, detail, icon) = match h {
                        Hit::App(e) => (e.name.clone(), None, apps::icon(e, dark)),
                        Hit::File(p, dir) => (
                            p.file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            p.parent().map(|d| d.display().to_string()),
                            apps::shared_icon(
                                "files",
                                if *dir {
                                    "folder-generic"
                                } else {
                                    "file-generic"
                                },
                                "icon.png",
                                dark,
                            ),
                        ),
                        Hit::Calc(expr, v) => {
                            let card =
                                Rect::new(r.x + 10.0, r.y + 6.0, r.width - 20.0, r.height - 12.0);
                            cx.rec.layer(
                                "Card",
                                card,
                                Shape::rounded(14.0),
                                Style::fill(ink.capsule),
                            );
                            let small = theme.text(&typography::CALLOUT, ink.secondary);
                            let l = lay(
                                &mut self.labels,
                                li,
                                expr,
                                &small,
                                Align::Start,
                                card.width - 70.0,
                                Some(1),
                            );
                            li += 1;
                            cx.rec.text(
                                "Expression",
                                Rect::new(card.x + 16.0, card.y + 10.0, card.width - 70.0, 18.0),
                                l,
                                None,
                                ink.secondary,
                            );
                            let big = theme.text(&typography::TITLE_1, ink.text);
                            let l = lay(
                                &mut self.labels,
                                li,
                                &format!("= {}", format_number(*v)),
                                &big,
                                Align::Start,
                                card.width - 70.0,
                                Some(1),
                            );
                            li += 1;
                            cx.rec.text(
                                "Result",
                                Rect::new(card.x + 16.0, card.y + 32.0, card.width - 70.0, 32.0),
                                l,
                                None,
                                ink.text,
                            );
                            cx.rec.layer(
                                "Copy",
                                Rect::new(
                                    card.right() - 42.0,
                                    card.y + (card.height - 26.0) / 2.0,
                                    26.0,
                                    26.0,
                                ),
                                Shape::Ellipse,
                                Style::fill(ink.selection),
                            );
                            cx.rec.glyph(
                                "Copy glyph",
                                Rect::new(
                                    card.right() - 37.0,
                                    card.y + (card.height - 16.0) / 2.0,
                                    16.0,
                                    16.0,
                                ),
                                "copy",
                                400,
                                ink.text,
                            );
                            continue;
                        }
                        Hit::Web(q) => (
                            format!("Search the web for \u{201c}{q}\u{201d}"),
                            None,
                            None,
                        ),
                    };
                    let ir = Rect::new(r.x + 16.0, r.y + (r.height - 40.0) / 2.0, 40.0, 40.0);
                    match icon {
                        Some(img) => cx.rec.image("Icon", ir, img, 1.0),
                        None => cx.rec.glyph(
                            "Icon",
                            ir.inset(vetro_lib::geom::Insets::all(6.0)),
                            "globe",
                            400,
                            ink.placeholder,
                        ),
                    }
                    let tw = r.width - 68.0 - 16.0;
                    let ty = if detail.is_some() {
                        r.y + 8.0
                    } else {
                        r.y + (r.height - 22.0) / 2.0
                    };
                    let l = lay(
                        &mut self.labels,
                        li,
                        &name,
                        &title,
                        Align::Start,
                        tw,
                        Some(1),
                    );
                    li += 1;
                    cx.rec.text(
                        "Title",
                        Rect::new(r.x + 68.0, ty, tw, 22.0),
                        l,
                        None,
                        ink.text,
                    );
                    if let Some(d) = detail {
                        let l = lay(&mut self.labels, li, &d, &sub, Align::Start, tw, Some(1));
                        li += 1;
                        cx.rec.text(
                            "Subtitle",
                            Rect::new(r.x + 68.0, r.y + 30.0, tw, 18.0),
                            l,
                            None,
                            ink.secondary,
                        );
                    }
                }
            }
            Mode::Launcher => {
                let mut li = 0usize;
                let browsing = s.category.is_none() && s.query.trim().is_empty();
                if browsing {
                    let caps = self.capsules(&theme, panel);
                    let style = theme.text(&typography::BODY, ink.capsule_text);
                    for (k, r) in caps.iter().enumerate() {
                        let Some((name, _)) = CATEGORIES.get(k) else {
                            continue;
                        };
                        cx.rec
                            .layer("Capsule", *r, Shape::rounded(9.0), Style::fill(ink.capsule));
                        let l = lay(
                            &mut self.labels,
                            li,
                            name,
                            &style,
                            Align::Center,
                            r.width,
                            Some(1),
                        );
                        li += 1;
                        let h = l.size().height;
                        cx.rec.text(
                            "Capsule text",
                            Rect::new(r.x, r.y + (r.height - h) / 2.0, r.width, h),
                            l,
                            None,
                            ink.capsule_text,
                        );
                    }
                }
                let clip_top = panel.y
                    + metrics::HEADER
                    + 1.0
                    + if browsing { metrics::CAPSULES } else { 0.0 };
                let grid_clip =
                    Rect::new(panel.x, clip_top, panel.width, panel.bottom() - clip_top);
                cx.rec.push_clip(grid_clip, Shape::rounded(0.0));
                let (apps_list, recent) = s.grid();
                let (cells, sep) = Self::cells(&s, panel, apps_list.len(), recent);
                if let Some(y) = sep {
                    cx.rec.layer(
                        "Recents separator",
                        Rect::new(panel.x + 20.0, y, metrics::PITCH * 7.0 - 0.5, 1.0),
                        Shape::rounded(0.0),
                        Style::fill(ink.separator),
                    );
                }
                let style = theme.text(&typography::BODY, ink.text);
                for (k, (e, r)) in apps_list.iter().zip(&cells).enumerate() {
                    if r.bottom() < clip_top || r.y > panel.bottom() {
                        continue;
                    }
                    let ir = Rect::new(
                        r.x + (r.width - metrics::GRID_ICON) / 2.0,
                        r.y + 7.0,
                        metrics::GRID_ICON,
                        metrics::GRID_ICON,
                    );
                    if s.selected == Some(k) || s.pressed == Some(k) {
                        cx.rec.layer(
                            "Selection ring",
                            ir.inset(vetro_lib::geom::Insets::all(-4.0)),
                            Shape::rounded(17.0),
                            Style::fill(Color::TRANSPARENT)
                                .with_border(Border::inside(theme.colors.accent_amalfi, 4.0)),
                        );
                    }
                    match apps::icon(e, dark) {
                        Some(img) => cx.rec.image("Icon", ir, img, 1.0),
                        None => cx.rec.glyph(
                            "Icon",
                            ir.inset(vetro_lib::geom::Insets::all(12.0)),
                            "question",
                            400,
                            ink.placeholder,
                        ),
                    }
                    let l = lay(
                        &mut self.labels,
                        li,
                        &e.name,
                        &style,
                        Align::Center,
                        110.0,
                        Some(1),
                    );
                    li += 1;
                    cx.rec.text(
                        "Name",
                        Rect::new(r.x + 1.0, ir.bottom() + 6.0, 110.0, 18.0),
                        l,
                        None,
                        ink.text,
                    );
                }
                cx.rec.pop();
                // The scroll bar is always there (launcher.md "Scroll bar").
                let track = Rect::new(
                    panel.right() - 14.0,
                    clip_top + 40.0,
                    11.0,
                    (panel.bottom() - 49.0 - clip_top - 40.0).max(0.0),
                );
                let (track_c, thumb_c) = if dark {
                    (Color::from_u32(0x141414ff), Color::from_u32(0x7c7c7cff))
                } else {
                    (Color::from_u32(0xf2f2f2ff), Color::from_u32(0x808080ff))
                };
                cx.rec.layer(
                    "Scroll track",
                    track,
                    Shape::rounded(5.5),
                    Style::fill(track_c),
                );
                let content = cells
                    .last()
                    .map_or(0.0, |r| r.bottom() + s.scroll - clip_top + 20.0);
                let visible = panel.bottom() - clip_top;
                if content > visible + 1.0 {
                    let k = (visible / content).clamp(0.05, 1.0);
                    let h = (track.height * k).max(22.0);
                    let t = (s.scroll / (content - visible)).clamp(0.0, 1.0);
                    let thumb = Rect::new(track.x, track.y + (track.height - h) * t, 11.0, h);
                    cx.rec.layer(
                        "Scroll thumb",
                        thumb,
                        Shape::rounded(5.5),
                        Style::fill(thumb_c),
                    );
                }
            }
        }
        cx.rec.pop();
        cx.rec.pop();
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let now = cx.now();
        let b = cx.bounds();
        match event {
            Event::Key(k) if k.pressed => {
                if self.key(cx, &k.key) {
                    return Handled::Yes;
                }
                let Some(t) = k.text.as_deref().filter(|t| !t.is_empty()) else {
                    return Handled::No;
                };
                let state = self.state.clone();
                state.borrow_mut().reshape(now, |s| {
                    s.query.push_str(t);
                    s.scroll = 0.0;
                    s.search();
                });
                cx.request_paint();
                cx.animate();
                Handled::Yes
            }
            Event::Text(t) => {
                let state = self.state.clone();
                state.borrow_mut().reshape(now, |s| {
                    s.query.push_str(t);
                    s.scroll = 0.0;
                    s.search();
                });
                cx.request_paint();
                cx.animate();
                Handled::Yes
            }
            Event::Scroll { dy, .. } => {
                let mut s = self.state.borrow_mut();
                if s.mode == Some(Mode::Launcher) {
                    let (apps_list, recent) = s.grid();
                    let rows = (apps_list.len().saturating_sub(recent)).div_ceil(metrics::COLUMNS)
                        + usize::from(recent > 0);
                    let content = rows as f32 * metrics::ROW_PITCH + 40.0;
                    let view = metrics::LAUNCHER.1 - metrics::HEADER - metrics::CAPSULES;
                    s.scroll = (s.scroll + dy).clamp(0.0, (content - view).max(0.0));
                    cx.request_paint();
                }
                Handled::Yes
            }
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                let p = Point::new(pos.x - b.x, pos.y - b.y);
                let state = self.state.clone();
                let s = state.borrow();
                let panel = s.frame(now);
                if !panel.contains(p) {
                    drop(s);
                    close(cx.ui(), &state);
                    return Handled::Yes;
                }
                let mode = s.mode;
                drop(s);
                if mode == Some(Mode::Launcher) {
                    let theme = cx.theme();
                    let browsing = {
                        let s = state.borrow();
                        s.category.is_none() && s.query.trim().is_empty()
                    };
                    if browsing {
                        let caps = self.capsules(&theme, panel);
                        if let Some(k) = caps.iter().position(|r| r.contains(p)) {
                            state.borrow_mut().reshape(now, |s| {
                                s.category = Some(k);
                                s.scroll = 0.0;
                                s.selected = None;
                            });
                            cx.request_paint();
                            cx.animate();
                            return Handled::Yes;
                        }
                    }
                    let s = state.borrow();
                    let (apps_list, recent) = s.grid();
                    let (cells, _) = Self::cells(&s, panel, apps_list.len(), recent);
                    let clip_top = panel.y
                        + metrics::HEADER
                        + 1.0
                        + if browsing { metrics::CAPSULES } else { 0.0 };
                    let hit = (p.y >= clip_top)
                        .then(|| cells.iter().position(|r| r.contains(p)))
                        .flatten();
                    drop(s);
                    state.borrow_mut().pressed = hit;
                } else {
                    let s = state.borrow();
                    let hit = Self::rows(&s, panel)
                        .into_iter()
                        .find(|(_, r)| r.contains(p))
                        .map(|(i, _)| i);
                    drop(s);
                    state.borrow_mut().pressed = hit;
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerUp {
                button: Button::Left,
                ..
            } => {
                let state = self.state.clone();
                let pressed = state.borrow_mut().pressed.take();
                if let Some(i) = pressed {
                    let s = state.borrow();
                    let hit = if s.mode == Some(Mode::Launcher) {
                        s.grid().0.get(i).cloned().map(Hit::App)
                    } else {
                        s.hits.get(i).cloned()
                    };
                    drop(s);
                    if let Some(h) = hit {
                        open_hit(cx.ui(), &state, &h);
                    }
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
        a.role = Role::Dialog;
        a.label = Some("Faro".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculations() {
        assert_eq!(calculate("2+3*4"), Some(14.0));
        assert_eq!(calculate("(2+3)×4"), Some(20.0));
        assert_eq!(calculate("2^10"), Some(1024.0));
        assert_eq!(calculate("50%*8"), Some(4.0));
        assert_eq!(calculate("-3+1"), Some(-2.0));
        assert_eq!(calculate("terminal"), None);
        assert_eq!(calculate("42"), None);
        assert_eq!(calculate("1/0"), None);
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(format_number(14.0), "14");
    }

    #[test]
    fn results_rank_name_starts_first() {
        let mut s = FaroState {
            mode: Some(Mode::Faro),
            output: Size::new(1280.0, 800.0),
            ..FaroState::default()
        };
        s.apps = vec![
            DesktopEntry {
                id: "a".into(),
                name: "Text Editor".into(),
                ..DesktopEntry::default()
            },
            DesktopEntry {
                id: "b".into(),
                name: "Terminal".into(),
                ..DesktopEntry::default()
            },
        ];
        s.query = "te".into();
        s.search();
        assert!(matches!(s.hits.first(), Some(Hit::App(e)) if e.name == "Terminal"));
        assert_eq!(s.selected, Some(0));
        s.query = "zzz".into();
        s.search();
        assert!(matches!(s.hits.first(), Some(Hit::Web(_))));
        assert_eq!(s.target().width, 640.0);
        assert_eq!(s.target().y, 240.0);
        s.query.clear();
        s.search();
        assert_eq!(s.target().height, metrics::BAR);
    }
}
