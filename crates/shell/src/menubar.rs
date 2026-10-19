//! The menu bar (docs/spec/menubar.md for behaviour, the kit's `Menu Bar` and the Amalfi
//! [status bar](../../docs/design/amalfi/status-bar.md) for layout): a 32 pt layer surface
//! along the top of each output (exclusive zone), the Silica mark with the system menu, the
//! focused app's menus from its model (`silica_shell_manager_v1.app_menu`), and the status
//! cluster at the trailing end (the clock for now; the other items come with their panels).

use std::cell::RefCell;
use std::rc::Rc;

use silica_protocols::menu::model::{Item, Model, parse_model};
use vetro_lib::amalfi::foundation::{Label, whole};
use vetro_lib::amalfi::menus::{self, Placement, Row, RowKind};
use vetro_lib::amalfi::tokens::typography;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::shell::ShellRequest;
use vetro_lib::core::window::{Edges, KeyboardMode, Layer};
use vetro_lib::core::{
    Button, Event, EventCx, Handled, LayoutCx, PaintCx, Prop, Proposal, Signal, Ui, Value,
    WindowId, WindowKind,
};
use vetro_lib::geom::{Point, Rect, Size};
use vetro_lib::shape::Shape;
use vetro_lib::style::Style;
use vetro_lib::text::Align;

use crate::time;

/// Measures (kit `Menu Bar/Bar/Closed`).
pub mod metrics {
    /// The bar's height.
    pub const HEIGHT: f32 = 32.0;
    /// Padding at both ends.
    pub const PADDING: f32 = 8.0;
    /// Items: 28 tall at y 2.
    pub const ITEM_HEIGHT: f32 = 28.0;
    /// An item's padding around its label.
    pub const ITEM_PADDING: f32 = 8.0;
    /// The mark's item.
    pub const MARK_WIDTH: f32 = 36.0;
    /// The open title's capsule radius.
    pub const CAPSULE_RADIUS: f32 = 14.0;
}

/// What a title opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleKind {
    /// The Silica mark: the system menu.
    Mark,
    /// Menu `i` of the app's model (the app's own menu is bold).
    App(usize),
}

/// A title in the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Title {
    /// What it opens.
    pub kind: TitleKind,
    /// The label (empty for the mark).
    pub label: String,
    /// Bold (the app's name).
    pub bold: bool,
}

/// The menu bar's state, shared by its surfaces.
#[derive(Default)]
pub struct BarState {
    /// The focused app's menus and the serial to answer with.
    pub model: Option<(String, u32, Model)>,
    /// The titles shown.
    pub titles: Vec<Title>,
    /// The open title and its popup.
    pub open: Option<(usize, WindowId)>,
    /// The bars (one per output) and their status clusters.
    pub bars: Vec<(WindowId, crate::status::Cluster)>,
    /// What a click on a status item does.
    pub on_status: Option<crate::status::OnClick>,
    /// The system's state the clusters show.
    pub system: crate::system::SystemState,
    /// 24-hour clock.
    pub hour24: bool,
    /// The user's name (the Log Out item).
    pub user: String,
}

/// A shared bar state.
pub type Shared = Rc<RefCell<BarState>>;

/// The clock item's text: `Mon 19 Oct  18:02` (two spaces before the time).
pub fn clock_text(t: &time::LocalTime, hour24: bool) -> String {
    let wd = time::WEEKDAYS
        .get(t.weekday as usize)
        .map_or("", |w| w.get(..3).unwrap_or(w));
    let mo = time::MONTHS
        .get(t.month.saturating_sub(1) as usize)
        .map_or("", |m| m.get(..3).unwrap_or(m));
    let time = if hour24 {
        format!("{:02}:{:02}", t.hour, t.minute)
    } else {
        let h = match t.hour % 12 {
            0 => 12,
            h => h,
        };
        format!(
            "{h}:{:02} {}",
            t.minute,
            if t.hour < 12 { "AM" } else { "PM" }
        )
    };
    format!("{wd} {} {mo}  {time}", t.day)
}

impl BarState {
    /// The titles of the current model.
    fn rebuild_titles(&mut self) {
        let mut titles = vec![Title {
            kind: TitleKind::Mark,
            label: String::new(),
            bold: false,
        }];
        if let Some((app, _, model)) = &self.model {
            for (i, m) in model.menus.iter().enumerate() {
                let bold = m.role == "app";
                let label = if bold && m.label.is_empty() {
                    app.rsplit('.').next().unwrap_or(app).to_string()
                } else {
                    m.label.clone()
                };
                titles.push(Title {
                    kind: TitleKind::App(i),
                    label,
                    bold,
                });
            }
        }
        self.titles = titles;
    }
}

/// Opens the bars (one per output).
pub fn open(ui: &mut Ui, state: &Shared) {
    let outputs: Vec<(Option<String>, f32)> = if ui.outputs().is_empty() {
        vec![(None, 1920.0)]
    } else {
        ui.outputs()
            .iter()
            .map(|o| (Some(o.name.clone()), o.size.width))
            .collect()
    };
    state.borrow_mut().rebuild_titles();
    let text = clock_text(&time::now(), state.borrow().hour24);
    let mut bars = Vec::new();
    for (name, width) in outputs {
        let w = ui.create_window(
            WindowKind::Shell {
                layer: Layer::Top,
                anchor: Edges::TOP | Edges::LEFT | Edges::RIGHT,
                exclusive: metrics::HEIGHT as i32,
                margin: [0; 4],
                keyboard: KeyboardMode::None,
                namespace: "menubar".into(),
                output: name,
            },
            Size::new(width, metrics::HEIGHT),
        );
        let on_click = state.borrow().on_status.clone();
        let cluster = crate::status::cluster(ui, crate::status::MENU_BAR, on_click);
        let system = state.borrow().system.clone();
        crate::status::update(ui, &cluster, &system, &text);
        let view = ui.create(MenuBarView::new(state.clone()));
        ui.add_child(view, cluster.view);
        let _ = ui.set(view, Prop::MaxWidth, Value::from(f32::INFINITY));
        let _ = ui.set(view, Prop::MaxHeight, Value::from(f32::INFINITY));
        ui.set_content(w, view);
        ui.show_window(w);
        bars.push((w, cluster));
    }
    state.borrow_mut().bars = bars;
}

/// The focused app's menus changed.
pub fn set_app_menu(ui: &mut Ui, state: &Shared, app_id: String, serial: u32, json: &str) {
    let model = parse_model(json.as_bytes()).ok();
    close_menu(ui, state);
    {
        let mut s = state.borrow_mut();
        s.model = model.map(|m| (app_id, serial, m));
        s.rebuild_titles();
    }
    repaint(ui, state);
}

/// Shows the clock (each minute) and the system's state in the status clusters.
pub fn update_status(ui: &mut Ui, state: &Shared) {
    let (bars, hour24, system) = {
        let s = state.borrow();
        (s.bars.clone(), s.hour24, s.system.clone())
    };
    let text = clock_text(&time::now(), hour24);
    for (_, cluster) in bars {
        crate::status::update(ui, &cluster, &system, &text);
    }
}

/// Marks a status item active in every bar (its panel is open).
pub fn set_status_active(ui: &mut Ui, state: &Shared, kind: crate::status::Kind, active: bool) {
    let bars = state.borrow().bars.clone();
    for (_, cluster) in bars {
        crate::status::set_active(ui, &cluster, kind, active);
    }
}

fn repaint(ui: &mut Ui, state: &Shared) {
    let bars = state.borrow().bars.clone();
    for (w, _) in bars {
        if let Some(root) = ui.window(w).map(|w| w.root) {
            for v in ui.tree.descendants(root) {
                ui.request_paint(v);
                ui.request_layout(v);
            }
        }
    }
}

/// Closes the open menu.
pub fn close_menu(ui: &mut Ui, state: &Shared) {
    let open = state.borrow_mut().open.take();
    if let Some((_, popup)) = open {
        ui.close_window(popup);
    }
    repaint(ui, state);
}

/// An entry of a menu the bar shows.
#[derive(Clone)]
enum Entry {
    /// An item of the app's model (its id).
    App(Option<u32>, Option<Vec<Item>>),
    /// A system action.
    System(Rc<dyn Fn(&mut Ui)>),
    /// Nothing to do (separators, headings, disabled items).
    None,
}

fn shortcut_text(item: &Item) -> Option<String> {
    let s = item.shortcut.as_ref()?;
    let mut out = String::new();
    if s.ctrl {
        out.push('⌃');
    }
    if s.alt {
        out.push('⌥');
    }
    if s.shift {
        out.push('⇧');
    }
    if s.meta {
        out.push('⌘');
    }
    out.push_str(&s.key.to_uppercase());
    Some(out)
}

fn rows_of(items: &[Item]) -> (Vec<Row>, Vec<Entry>) {
    let mut rows = Vec::new();
    let mut entries = Vec::new();
    for it in items {
        if it.separator {
            rows.push(Row {
                kind: RowKind::Separator,
                ..Row::default()
            });
            entries.push(Entry::None);
            continue;
        }
        rows.push(Row {
            kind: RowKind::Item,
            label: it.label.clone(),
            shortcut: shortcut_text(it),
            icon: None,
            checked: it.checked,
            submenu: it.items.is_some(),
            destructive: false,
            disabled: !it.enabled,
        });
        entries.push(Entry::App(it.id, it.items.clone()));
    }
    (rows, entries)
}

/// Rows and what they do, built in order.
#[derive(Default)]
struct Build {
    rows: Vec<Row>,
    entries: Vec<Entry>,
}

impl Build {
    fn item(&mut self, label: &str, shortcut: Option<&str>, entry: Entry) {
        self.rows.push(Row {
            kind: RowKind::Item,
            label: label.into(),
            shortcut: shortcut.map(str::to_string),
            disabled: matches!(entry, Entry::None),
            ..Row::default()
        });
        self.entries.push(entry);
    }

    fn separator(&mut self) {
        self.rows.push(Row {
            kind: RowKind::Separator,
            ..Row::default()
        });
        self.entries.push(Entry::None);
    }
}

/// An Ossido catalog command.
fn run(id: &'static str) -> Entry {
    Entry::System(Rc::new(move |ui: &mut Ui| {
        ui.shell(ShellRequest::Execute(id.into()));
    }))
}

/// The system menu under the Silica mark (the shell's logo menu, logo_menu.dart).
fn system_menu(state: &Shared) -> (Vec<Row>, Vec<Entry>) {
    let user = state.borrow().user.clone();
    let mut b = Build::default();
    b.item("About Silica", None, Entry::None);
    b.separator();
    b.item(
        "System Settings…",
        None,
        Entry::System(Rc::new(|ui: &mut Ui| {
            ui.shell(ShellRequest::ExecuteWith(
                "app.launch".into(),
                r#"{"app":"dev.frascella.Settings"}"#.into(),
            ));
        })),
    );
    b.separator();
    b.item("Force Quit…", Some("⌥⌘⎋"), run("app.force_quit_panel"));
    b.separator();
    b.item(
        "Sleep",
        None,
        Entry::System(Rc::new(|_ui: &mut Ui| {
            vetro_lib::platform::spawn(async {
                if let Some(s) = vetro_lib::platform::services()
                    && let Ok(p) = s.system().await
                {
                    let _ = p.suspend().await;
                }
            });
        })),
    );
    b.separator();
    b.item("Lock Screen", Some("⌃⌘Q"), run("session.lock"));
    let log_out = if user.is_empty() {
        "Log Out…".to_string()
    } else {
        format!("Log Out {user}…")
    };
    b.item(&log_out, Some("⇧⌘Q"), run("session.quit"));
    (b.rows, b.entries)
}

/// Opens title `i`'s menu under `anchor` (window coordinates of `bar`).
fn open_menu(ui: &mut Ui, state: &Shared, bar: WindowId, i: usize, anchor: Rect) {
    let same = state.borrow().open.is_some_and(|(o, _)| o == i);
    close_menu(ui, state);
    if same {
        return;
    }
    let kind = state.borrow().titles.get(i).map(|t| t.kind);
    let (rows, entries) = match kind {
        Some(TitleKind::Mark) => system_menu(state),
        Some(TitleKind::App(m)) => {
            let items = state
                .borrow()
                .model
                .as_ref()
                .and_then(|(_, _, model)| model.menus.get(m).map(|mm| mm.items.clone()))
                .unwrap_or_default();
            rows_of(&items)
        }
        None => return,
    };
    let popup = present(ui, state, bar, anchor, rows, entries, Placement::PullDown);
    if let Some(p) = popup {
        state.borrow_mut().open = Some((i, p));
    }
    repaint(ui, state);
}

/// Shows a menu and wires its choices.
fn present(
    ui: &mut Ui,
    state: &Shared,
    parent: WindowId,
    anchor: Rect,
    rows: Vec<Row>,
    entries: Vec<Entry>,
    placement: Placement,
) -> Option<WindowId> {
    let (menu, row_views) = menus::menu(ui, &rows);
    let popup = menus::present(ui, parent, anchor, menu, placement);
    let st = state.clone();
    ui.connect(menu, Signal::Activated, move |ui, _, v| {
        let Some(i) = v.as_int().and_then(|i| usize::try_from(i).ok()) else {
            return;
        };
        match entries.get(i).cloned() {
            Some(Entry::App(_, Some(sub))) => {
                // A submenu opens beside its row.
                let at = row_views
                    .get(i)
                    .map(|r| ui.tree.window_rect(*r))
                    .unwrap_or_default();
                let (r, e) = rows_of(&sub);
                let a = Rect::new(at.right() - 4.0, at.y - 4.0, 0.0, 0.0);
                let _ = present(ui, &st, popup, a, r, e, Placement::At);
            }
            Some(Entry::App(Some(id), None)) => {
                let serial = st.borrow().model.as_ref().map(|m| m.1);
                close_menu(ui, &st);
                if let Some(serial) = serial {
                    ui.shell(ShellRequest::ActivateMenuItem(serial, id));
                }
            }
            Some(Entry::System(f)) => {
                close_menu(ui, &st);
                f(ui);
            }
            _ => {}
        }
    });
    if let Some(root) = ui.window(popup).map(|w| w.root) {
        let st = state.clone();
        ui.connect(root, Signal::Closed, move |ui, _, _| {
            let mine = st.borrow().open.is_some_and(|(_, p)| p == popup);
            if mine {
                st.borrow_mut().open = None;
                repaint(ui, &st);
            }
        });
    }
    Some(popup)
}

/// A bar's content: the titles, and the status cluster (its child) at the trailing end.
pub struct MenuBarView {
    state: Shared,
    labels: Vec<Label>,
    rects: Vec<Rect>,
}

impl MenuBarView {
    /// A bar over `state`.
    pub fn new(state: Shared) -> MenuBarView {
        MenuBarView {
            state,
            labels: Vec::new(),
            rects: Vec::new(),
        }
    }
}

impl vetro_lib::core::Widget for MenuBarView {
    fn kind(&self) -> &'static str {
        "MenuBar"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(1920.0, metrics::HEIGHT))
    }

    fn place(&mut self, cx: &mut LayoutCx<'_>, size: Size) {
        if let Some(cluster) = cx.children().first().copied() {
            let s = cx.measure(cluster, Proposal::new(None, None));
            cx.place(
                cluster,
                Rect::new(
                    size.width - metrics::PADDING - s.width,
                    0.0,
                    s.width,
                    s.height,
                ),
            );
        }
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let theme = *cx.theme();
        let c = theme.colors;
        let r = cx.bounds();
        let st = self.state.borrow();
        if self.labels.len() != st.titles.len() {
            self.labels = st.titles.iter().map(|_| Label::default()).collect();
        }
        self.rects.clear();
        let mut x = r.x + metrics::PADDING;
        let y = r.y + 2.0;
        for (i, t) in st.titles.iter().enumerate() {
            let (w, layout) = match t.kind {
                TitleKind::Mark => (metrics::MARK_WIDTH, None),
                TitleKind::App(_) => {
                    let token = if t.bold {
                        &typography::HEADLINE
                    } else {
                        &typography::BODY
                    };
                    let style = theme.text(token, c.text_primary);
                    let Some(label) = self.labels.get_mut(i) else {
                        continue;
                    };
                    let tw = whole(label.width(&t.label, &style), 4.0);
                    let l = label.layout(&t.label, &style, Align::Center, tw, Some(1));
                    (tw + 2.0 * metrics::ITEM_PADDING, Some((l, token.name, tw)))
                }
            };
            let item = Rect::new(x, y, w, metrics::ITEM_HEIGHT);
            if st.open.is_some_and(|(o, _)| o == i) {
                cx.rec.layer(
                    "Selection",
                    item,
                    Shape::rounded(metrics::CAPSULE_RADIUS),
                    Style::fill(c.menu_bar_selection),
                );
            }
            match layout {
                None => cx.rec.glyph(
                    "Silica mark",
                    Rect::new(item.x + 8.0, item.y + 4.0, 20.0, 20.0),
                    "diamond-fill",
                    400,
                    c.text_primary,
                ),
                Some((l, token, tw)) => cx.rec.text(
                    "Label",
                    Rect::new(item.x + metrics::ITEM_PADDING, item.y + 5.0, tw, 18.0),
                    l,
                    Some(token),
                    c.text_primary,
                ),
            }
            self.rects.push(item.translate(Point::new(-r.x, -r.y)));
            x += w;
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        if let Event::PointerDown {
            pos,
            button: Button::Left,
            ..
        } = event
        {
            let b = cx.bounds();
            let p = Point::new(pos.x - b.x, pos.y - b.y);
            // The hit area of a title runs the bar's full height.
            if let Some(i) = self
                .rects
                .iter()
                .position(|r| p.x >= r.x && p.x < r.right())
                && let Some(rect) = self.rects.get(i).copied()
                && let Some(bar) = cx.window()
            {
                let state = self.state.clone();
                let anchor = rect.translate(b.origin());
                open_menu(cx.ui(), &state, bar, i, anchor);
                return Handled::Yes;
            }
        }
        Handled::No
    }

    fn hit(&self, p: Point, _size: Size) -> bool {
        self.rects.iter().any(|r| p.x >= r.x && p.x < r.right())
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Toolbar;
        a.label = Some("Menu Bar".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_as_the_kit() {
        let t = time::at(1_792_422_800, 7200);
        assert_eq!(clock_text(&t, true), "Mon 19 Oct  17:13");
        assert_eq!(clock_text(&t, false), "Mon 19 Oct  5:13 PM");
    }

    #[test]
    fn titles_follow_the_model() {
        let json = r#"{"menus":[{"label":"Gallery","role":"app","items":[{"id":1,"label":"Quit"}]},
            {"label":"View","items":[{"id":3,"label":"Toggle","shortcut":{"key":"d","meta":true,"shift":true}}]}]}"#;
        let mut s = BarState {
            model: parse_model(json.as_bytes())
                .ok()
                .map(|m| ("dev.silica.VetroGallery".to_string(), 7, m)),
            ..BarState::default()
        };
        s.rebuild_titles();
        let labels: Vec<(&str, bool)> = s
            .titles
            .iter()
            .map(|t| (t.label.as_str(), t.bold))
            .collect();
        assert_eq!(
            labels,
            vec![("", false), ("Gallery", true), ("View", false)]
        );
        let items = &s
            .model
            .as_ref()
            .map(|m| m.2.menus[1].items.clone())
            .unwrap();
        let (rows, _) = rows_of(items);
        assert_eq!(rows[0].shortcut.as_deref(), Some("⇧⌘D"));
    }
}
