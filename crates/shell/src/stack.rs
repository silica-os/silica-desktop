//! Folder stacks in the Dock (docs/spec/dock-stacks.md): the tile's pile of the newest items,
//! and the open stack — a glass grid of the folder's items above its tile, each opening with
//! its default app, and a last cell that opens the folder itself.
//!
//! Not yet: the fan and list views, sorting and display options.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vetro_lib::amalfi::foundation::Label;
use vetro_lib::amalfi::tokens::typography;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::display::GlassProfile;
use vetro_lib::core::{
    Button, Event, EventCx, Handled, LayoutCx, PaintCx, Prop, Proposal, Signal, Ui, Value,
    WindowKind,
};
use vetro_lib::geom::{Point, Rect, Size};
use vetro_lib::shape::Shape;
use vetro_lib::style::Style;
use vetro_lib::text::Align;

use crate::apps;
use crate::dock::{self, Shared};

/// Grid measures.
mod metrics {
    /// A cell.
    pub const CELL: f32 = 96.0;
    /// A cell's height (icon and two lines).
    pub const CELL_HEIGHT: f32 = 104.0;
    /// The icon.
    pub const ICON: f32 = 64.0;
    /// The panel's padding.
    pub const PADDING: f32 = 16.0;
    /// Columns at most.
    pub const COLUMNS: usize = 5;
    /// Items shown at most (the rest opens in the folder).
    pub const MAX: usize = 19;
    /// The panel's bottom above the shelf.
    pub const GAP: f32 = 12.0;
    /// Room for the panel's shadow.
    pub const MARGIN: f32 = 24.0;
    /// The panel's corner radius.
    pub const RADIUS: f32 = 24.0;
}

/// An item of a folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Its path.
    pub path: PathBuf,
    /// A folder.
    pub dir: bool,
    /// When it was changed (added, for downloads).
    pub time: SystemTime,
}

/// The folder's visible items: the downloads folder newest first, others by name.
pub fn entries(folder: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = rd
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let meta = e.metadata().ok();
            Entry {
                path: e.path(),
                dir: meta.as_ref().is_some_and(|m| m.is_dir()),
                time: meta
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH),
            }
        })
        .collect();
    if dock::downloads_dir().as_deref() == Some(folder) {
        out.sort_by_key(|e| std::cmp::Reverse(e.time));
    } else {
        out.sort_by_key(|e| {
            e.path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
        });
    }
    out
}

fn entry_icon(e: &Entry, dark: bool) -> Option<skia_safe::Image> {
    if e.dir {
        apps::shared_icon("files", "folder-generic", "icon.png", dark)
    } else {
        apps::shared_icon("files", "file-generic", "icon.png", dark)
    }
}

/// Paints the tile: a pile of up to three items on a 50 pt canvas, or the folder's icon when
/// empty; an open stack shows the close badge.
pub fn paint_tile(cx: &mut PaintCx<'_>, icon: Rect, folder: &Path, dark: bool, open: bool) {
    let k = icon.width / 60.0;
    let items = entries(folder);
    if open {
        if let Some(img) = apps::shared_icon("files", "folder-generic", "icon-full.png", dark) {
            cx.rec.image("Folder", icon, img, 1.0);
        }
        let badge = if dark {
            "stack-close-dark.png"
        } else {
            "stack-close.png"
        };
        if let Some(img) = apps::shared_icon("badges", "", badge, false) {
            cx.rec.image("Close", icon, img, 1.0);
        }
        return;
    }
    if items.is_empty() {
        if let Some(img) = apps::shared_icon("files", "folder-generic", "icon.png", dark) {
            cx.rec.image("Folder", icon, img, 1.0);
        }
        return;
    }
    // Back to front: offsets 13, 15, 17 from the tile's top, each 4 % smaller behind.
    let pile: Vec<&Entry> = items.iter().take(3).collect();
    for (depth, e) in pile.iter().enumerate().rev() {
        let scale = 0.96f32.powi(depth as i32);
        let side = (50.0 * scale * k).round();
        let top = [17.0, 15.0, 13.0].get(depth).copied().unwrap_or(13.0) - 10.0;
        let x = icon.x + (icon.width - side) / 2.0;
        let y = icon.y + top * k;
        if let Some(img) = entry_icon(e, dark) {
            cx.rec.image("Item", Rect::new(x, y, side, side), img, 1.0);
        }
    }
}

/// Whether tile `i`'s stack is the open one.
pub fn open_for(state: &Shared, i: usize) -> bool {
    STACK_TILE.with(|t| t.get() == Some(i)) && state.borrow().stack.is_some()
}

thread_local! {
    static STACK_TILE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Opens tile `i`'s stack, or closes it when open.
pub fn toggle(ui: &mut Ui, state: &Shared, i: usize, folder: &Path) {
    let open = state.borrow_mut().stack.take();
    if let Some(w) = open {
        ui.close_window(w);
        let was = STACK_TILE.with(|t| t.replace(None));
        if was == Some(i) {
            return;
        }
    }
    let Some((surface, view)) = state.borrow().surface else {
        return;
    };
    let mut items = entries(folder);
    let more = items.len().saturating_sub(metrics::MAX);
    items.truncate(metrics::MAX);
    let cells = items.len() + 1;
    let columns = cells.clamp(1, metrics::COLUMNS);
    let rows = cells.div_ceil(columns);
    let m = metrics::MARGIN;
    let panel = Size::new(
        columns as f32 * metrics::CELL + 2.0 * metrics::PADDING,
        rows as f32 * metrics::CELL_HEIGHT + 2.0 * metrics::PADDING,
    );
    let size = Size::new(panel.width + 2.0 * m, panel.height + 2.0 * m);
    let g = state.borrow().geometry(ui.now);
    let Some(cell) = g.cells.get(i) else { return };
    let centre = cell.rect.x + cell.rect.width / 2.0;
    let surface_w = state.borrow().size.width;
    let x = (centre - size.width / 2.0).clamp(0.0, (surface_w - size.width).max(0.0));
    let y = g.shelf.y - metrics::GAP - panel.height - m;
    let w = ui.create_window(
        WindowKind::Popup {
            parent: surface,
            anchor: Rect::new(x, y - size.height, size.width, size.height),
            grab: true,
        },
        size,
    );
    let v = ui.create(StackView {
        folder: folder.to_path_buf(),
        items,
        more,
        columns,
        labels: Vec::new(),
        dark: ui.theme_of(view).dark(),
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, v);
    ui.show_window(w);
    ui.set_input_region(w, Some(vec![Rect::new(m, m, panel.width, panel.height)]));
    STACK_TILE.with(|t| t.set(Some(i)));
    state.borrow_mut().stack = Some(w);
    if let Some(root) = ui.window(w).map(|w| w.root) {
        let st = state.clone();
        ui.connect(root, Signal::Closed, move |ui, _, _| {
            if st.borrow().stack == Some(w) {
                st.borrow_mut().stack = None;
                STACK_TILE.with(|t| t.set(None));
                if let Some((_, v)) = st.borrow().surface {
                    ui.request_paint(v);
                }
            }
        });
    }
    ui.request_paint(view);
}

/// The open stack's grid.
struct StackView {
    folder: PathBuf,
    items: Vec<Entry>,
    more: usize,
    columns: usize,
    labels: Vec<Label>,
    dark: bool,
}

impl StackView {
    fn cell(&self, b: Rect, k: usize) -> Rect {
        let o = metrics::MARGIN + metrics::PADDING;
        let (c, r) = (k % self.columns, k / self.columns);
        Rect::new(
            b.x + o + c as f32 * metrics::CELL,
            b.y + o + r as f32 * metrics::CELL_HEIGHT,
            metrics::CELL,
            metrics::CELL_HEIGHT,
        )
    }

    fn close(cx: &mut EventCx<'_>) {
        if let Some(w) = cx.window() {
            let ui = cx.ui();
            if let Some(root) = ui.window(w).map(|w| w.root) {
                ui.emit(root, Signal::Closed, Value::None);
            }
            ui.close_window(w);
        }
    }
}

impl vetro_lib::core::Widget for StackView {
    fn kind(&self) -> &'static str {
        "Stack"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(400.0, 300.0))
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let b = cx.bounds();
        let theme = *cx.theme();
        let c = theme.colors;
        let m = metrics::MARGIN;
        let panel = Rect::new(b.x + m, b.y + m, b.width - 2.0 * m, b.height - 2.0 * m);
        cx.glass(
            "Panel",
            panel,
            Shape::rounded(metrics::RADIUS),
            "Glass/Regular",
            GlassProfile::Popover,
        );
        let style = theme.text(&typography::CAPTION, c.text_primary);
        let n = self.items.len() + 1;
        if self.labels.len() < n {
            self.labels.resize_with(n, Label::default);
        }
        for k in 0..n {
            let r = self.cell(b, k);
            let icon = Rect::new(
                r.x + (r.width - metrics::ICON) / 2.0,
                r.y + 4.0,
                metrics::ICON,
                metrics::ICON,
            );
            let (name, img) = match self.items.get(k) {
                Some(e) => (
                    e.path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    entry_icon(e, self.dark),
                ),
                None => (
                    if self.more > 0 {
                        format!("{} more in Mustache", self.more)
                    } else {
                        "Open in Mustache".to_string()
                    },
                    apps::shared_icon("files", "folder-generic", "icon-full.png", self.dark),
                ),
            };
            if let Some(img) = img {
                cx.rec.image("Icon", icon, img, 1.0);
            } else {
                cx.rec.layer(
                    "Icon",
                    icon,
                    Shape::rounded(12.0),
                    Style::fill(c.surface_control),
                );
            }
            if let Some(l) = self.labels.get_mut(k) {
                let layout = l.layout(&name, &style, Align::Center, r.width - 8.0, Some(2));
                cx.rec.text(
                    "Name",
                    Rect::new(r.x + 4.0, icon.bottom() + 4.0, r.width - 8.0, 30.0),
                    layout,
                    Some(typography::CAPTION.name),
                    c.text_primary,
                );
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        if let Event::PointerUp {
            pos,
            button: Button::Left,
            ..
        } = event
        {
            let b = cx.bounds();
            let p = Point::new(pos.x, pos.y);
            let hit = (0..=self.items.len()).find(|k| self.cell(b, *k).contains(p));
            if let Some(k) = hit {
                let path = self
                    .items
                    .get(k)
                    .map_or_else(|| self.folder.clone(), |e| e.path.clone());
                Self::close(cx);
                dock::open_path(&path);
            }
            return Handled::Yes;
        }
        match event {
            Event::PointerDown { .. } => Handled::Yes,
            _ => Handled::No,
        }
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Group;
        a.label = self
            .folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
    }
}
