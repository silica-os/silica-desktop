//! The Dock (docs/spec/dock.md for behaviour, the kit's `Dock` masters for layout): a glass
//! shelf on a Top layer at the bottom of the first output, the pinned apps
//! (`$XDG_CONFIG_HOME/silica/dock.json` `pinned`, else the first installed of the default
//! list), then the running apps that are not pinned. A click activates a running app (all
//! its windows, `activate_app`) or launches it (`app.launch`); a disc marks the running
//! apps; the app's name shows above the hovered icon.
//!
//! Not yet: magnification (off by default, `dock.magnify`), the launch bounce, stacks, the
//! Trash, context menus and dragging.

use std::cell::RefCell;
use std::rc::Rc;

use vetro_lib::amalfi::foundation::{Label, whole};
use vetro_lib::amalfi::tokens::typography;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::display::GlassProfile;
use vetro_lib::core::shell::ShellRequest;
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

/// Measures (kit `Dock/Shelf/Default`, `Dock/Tile/*`).
pub mod metrics {
    /// The shelf's height.
    pub const SHELF_HEIGHT: f32 = 80.0;
    /// The shelf's padding at both ends.
    pub const PADDING: f32 = 8.0;
    /// A tile's width.
    pub const TILE: f32 = 64.0;
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
}

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
    &["org.gnome.TextEditor", "org.gnome.gedit"],
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

/// The Dock's state.
#[derive(Default)]
pub struct DockState {
    /// The apps, in order.
    pub apps: Vec<DockApp>,
    /// Open windows: (identifier, app id), the most recently focused last.
    pub windows: Vec<(String, String)>,
    /// The hovered tile.
    pub hovered: Option<usize>,
    /// The Dock's surface and view.
    pub surface: Option<(WindowId, ViewId)>,
    /// The hover label's popup.
    pub label: Option<WindowId>,
}

/// A shared Dock state.
pub type Shared = Rc<RefCell<DockState>>;

/// The pinned ids from dock.json, or `None` without a list there.
fn configured_pinned() -> Option<Vec<String>> {
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    let text = std::fs::read_to_string(dir.join("silica/dock.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let list = v.get("pinned")?.as_array()?;
    Some(
        list.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
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

impl DockState {
    /// Puts the running apps that are not pinned after the pinned ones.
    fn sync_running(&mut self) {
        self.apps.retain(|a| a.pinned);
        let running: Vec<String> = self.windows.iter().map(|(_, a)| a.clone()).collect();
        for app in running {
            if app.is_empty() || self.apps.iter().any(|a| a.entry.id == app) {
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
            self.apps.push(DockApp {
                entry,
                pinned: false,
            });
        }
    }

    fn running(&self, app: &str) -> bool {
        self.windows.iter().any(|(_, a)| a == app)
    }

    /// The most recently focused window of `app`.
    fn window_of(&self, app: &str) -> Option<String> {
        self.windows
            .iter()
            .rev()
            .find(|(_, a)| a == app)
            .map(|(w, _)| w.clone())
    }

    fn shelf_width(&self) -> f32 {
        2.0 * metrics::PADDING + self.apps.len() as f32 * metrics::TILE
    }
}

/// Opens the Dock on the first output.
pub fn open(ui: &mut Ui, state: &Shared) {
    {
        let mut s = state.borrow_mut();
        s.apps = pinned()
            .into_iter()
            .map(|entry| DockApp {
                entry,
                pinned: true,
            })
            .collect();
        s.sync_running();
    }
    let output = ui.outputs().first().map(|o| o.name.clone());
    let width = state.borrow().shelf_width();
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Top,
            anchor: Edges::BOTTOM,
            exclusive: (metrics::SHELF_HEIGHT + metrics::BOTTOM) as i32,
            margin: [0, 0, metrics::BOTTOM as i32, 0],
            keyboard: KeyboardMode::None,
            namespace: "dock".into(),
            output,
        },
        Size::new(width, metrics::SHELF_HEIGHT),
    );
    let view = ui.create(DockView::new(state.clone()));
    let _ = ui.set(view, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(view, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, view);
    ui.show_window(w);
    state.borrow_mut().surface = Some((w, view));
}

/// The windows changed: the running apps follow.
pub fn window_changed(ui: &mut Ui, state: &Shared, identifier: &str, app_id: &str, open: bool) {
    let resize = {
        let mut s = state.borrow_mut();
        let before = s.apps.len();
        s.windows.retain(|(w, _)| w != identifier);
        if open {
            s.windows.push((identifier.to_string(), app_id.to_string()));
        }
        s.sync_running();
        s.hovered = s.hovered.filter(|h| *h < s.apps.len());
        (s.apps.len() != before).then(|| (s.surface, s.shelf_width()))
    };
    if let Some((Some((w, _)), width)) = resize {
        ui.request(vetro_lib::core::Request::Resize(
            w,
            Size::new(width, metrics::SHELF_HEIGHT),
        ));
    }
    repaint(ui, state);
}

/// A window took the focus: it is its app's most recent.
pub fn focused(state: &Shared, identifier: &str) {
    let mut s = state.borrow_mut();
    if let Some(i) = s.windows.iter().position(|(w, _)| w == identifier) {
        let w = s.windows.remove(i);
        s.windows.push(w);
    }
}

fn repaint(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
        ui.request_layout(v);
    }
}

fn activate(ui: &mut Ui, state: &Shared, i: usize) {
    let (app, window) = {
        let s = state.borrow();
        let Some(app) = s.apps.get(i) else { return };
        (app.entry.id.clone(), s.window_of(&app.entry.id))
    };
    match window {
        Some(w) => ui.shell(ShellRequest::ActivateApp(w)),
        None => {
            let params = serde_json::json!({ "app": app }).to_string();
            ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
        }
    }
}

/// The app's name above the hovered icon: a capsule popup of the Dock's surface.
fn show_label(ui: &mut Ui, state: &Shared, i: Option<usize>) {
    let old = state.borrow_mut().label.take();
    if let Some(p) = old {
        ui.close_window(p);
    }
    let (Some(i), Some((dock, view))) = (i, state.borrow().surface) else {
        return;
    };
    let Some(name) = state.borrow().apps.get(i).map(|a| a.entry.name.clone()) else {
        return;
    };
    let theme = ui.theme_of(view);
    let style = theme.text(&typography::BODY, theme.colors.text_primary);
    let tw = whole(Label::default().width(&name, &style), 1.0);
    let size = Size::new(tw + 2.0 * metrics::LABEL_PADDING, metrics::LABEL_HEIGHT);
    let x = metrics::PADDING + i as f32 * metrics::TILE + metrics::TILE / 2.0 - size.width / 2.0;
    let y = metrics::ICON_TOP - metrics::LABEL_GAP - size.height;
    let popup = ui.create_window(
        WindowKind::Popup {
            parent: dock,
            anchor: Rect::new(x, y - size.height, size.width, size.height),
            grab: false,
        },
        size,
    );
    let v = ui.create(DockLabel {
        text: name,
        label: Label::default(),
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(popup, v);
    ui.show_window(popup);
    state.borrow_mut().label = Some(popup);
}

/// The hover label.
struct DockLabel {
    text: String,
    label: Label,
}

impl vetro_lib::core::Widget for DockLabel {
    fn kind(&self) -> &'static str {
        "DockLabel"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(80.0, metrics::LABEL_HEIGHT))
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let r = cx.bounds();
        let theme = *cx.theme();
        cx.glass(
            "Label",
            r,
            Shape::rounded(r.height / 2.0),
            "Glass/Regular",
            GlassProfile::Tooltip,
        );
        let style = theme.text(&typography::BODY, theme.colors.text_primary);
        let l = self
            .label
            .layout(&self.text, &style, Align::Center, r.width, Some(1));
        let h = l.size().height;
        cx.rec.text(
            "Name",
            Rect::new(r.x, r.y + (r.height - h) / 2.0, r.width, h),
            l,
            Some(typography::BODY.name),
            theme.colors.text_primary,
        );
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Label;
        a.label = Some(self.text.clone());
    }
}

/// The Dock's shelf and tiles.
pub struct DockView {
    state: Shared,
}

impl DockView {
    /// A Dock over `state`.
    pub fn new(state: Shared) -> DockView {
        DockView { state }
    }

    fn tile_at(&self, x: f32) -> Option<usize> {
        let i = ((x - metrics::PADDING) / metrics::TILE).floor();
        (i >= 0.0 && (i as usize) < self.state.borrow().apps.len()).then_some(i as usize)
    }
}

impl vetro_lib::core::Widget for DockView {
    fn kind(&self) -> &'static str {
        "Dock"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, _p: Proposal) -> Size {
        Size::new(self.state.borrow().shelf_width(), metrics::SHELF_HEIGHT)
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let r = cx.bounds();
        let theme = *cx.theme();
        let c = theme.colors;
        let dark = theme.dark();
        let st = self.state.borrow();
        let shelf = Rect::new(r.x, r.y, st.shelf_width(), metrics::SHELF_HEIGHT);
        cx.glass(
            "Shelf",
            shelf,
            Shape::rounded(metrics::RADIUS),
            "Glass/Regular",
            GlassProfile::Dock,
        );
        for (i, app) in st.apps.iter().enumerate() {
            let x = r.x + metrics::PADDING + i as f32 * metrics::TILE;
            let icon = Rect::new(
                x + (metrics::TILE - metrics::ICON) / 2.0,
                r.y + metrics::ICON_TOP,
                metrics::ICON,
                metrics::ICON,
            );
            match apps::icon(&app.entry, dark) {
                Some(img) => cx.rec.image("Icon", icon, img, 1.0),
                None => {
                    // An app without an icon: a plain tile with its initial.
                    cx.rec.layer(
                        "Icon",
                        icon.inset(vetro_lib::geom::Insets::all(4.0)),
                        Shape::rounded(12.0),
                        Style::fill(c.surface_control)
                            .with_border(Border::inside(c.line_border, 1.0)),
                    );
                }
            }
            if st.running(&app.entry.id) {
                cx.rec.layer(
                    "Running dot",
                    Rect::new(
                        x + (metrics::TILE - metrics::DOT) / 2.0,
                        r.y + metrics::DOT_TOP,
                        metrics::DOT,
                        metrics::DOT,
                    ),
                    Shape::Ellipse,
                    Style::fill(c.text_primary),
                );
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        match event {
            Event::PointerMove { pos, .. } => {
                let i = self.tile_at(pos.x - b.x);
                if i != self.state.borrow().hovered {
                    self.state.borrow_mut().hovered = i;
                    let state = self.state.clone();
                    show_label(cx.ui(), &state, i);
                }
                Handled::No
            }
            Event::PointerLeave => {
                self.state.borrow_mut().hovered = None;
                let state = self.state.clone();
                show_label(cx.ui(), &state, None);
                Handled::No
            }
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                if let Some(i) = self.tile_at(pos.x - b.x) {
                    let state = self.state.clone();
                    activate(cx.ui(), &state, i);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_apps_follow_the_pinned_ones() {
        let mut s = DockState {
            apps: vec![DockApp {
                entry: DesktopEntry {
                    id: "a.Pinned".into(),
                    name: "Pinned".into(),
                    ..DesktopEntry::default()
                },
                pinned: true,
            }],
            ..DockState::default()
        };
        s.windows = vec![
            ("w1".into(), "x.Other".into()),
            ("w2".into(), "a.Pinned".into()),
            ("w3".into(), "x.Other".into()),
        ];
        s.sync_running();
        let ids: Vec<&str> = s.apps.iter().map(|a| a.entry.id.as_str()).collect();
        assert_eq!(ids, vec!["a.Pinned", "x.Other"]);
        assert!(s.running("a.Pinned"));
        assert_eq!(s.window_of("x.Other").as_deref(), Some("w3"));
        assert_eq!(s.shelf_width(), 16.0 + 128.0);
    }
}
