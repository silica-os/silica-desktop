//! Context menus of shell surfaces (the Dock's, the stacks'): Amalfi menus in grabbing
//! popups, with submenus beside their rows. A chosen item closes the whole chain first, then
//! runs.

use std::cell::RefCell;
use std::rc::Rc;

use vetro_lib::amalfi::menus::{self, Row, RowKind};
use vetro_lib::core::{Prop, Proposal, Signal, Ui, Value, WindowId, WindowKind};
use vetro_lib::geom::{Point, Rect, Size};

/// What an item does.
pub type Action = Rc<dyn Fn(&mut Ui)>;

/// An entry of a menu.
#[derive(Clone)]
pub enum Item {
    /// A plain item; without an action it shows disabled.
    Action {
        /// Its title.
        label: String,
        /// What it does.
        action: Option<Action>,
        /// A check mark.
        checked: bool,
    },
    /// A title opening a submenu.
    Submenu {
        /// Its title.
        label: String,
        /// The submenu's entries.
        items: Vec<Item>,
    },
    /// A separator.
    Separator,
    /// A grey section header that cannot be chosen.
    Header(String),
}

impl Item {
    /// An item running `f`.
    pub fn action(label: &str, f: impl Fn(&mut Ui) + 'static) -> Item {
        Item::Action {
            label: label.into(),
            action: Some(Rc::new(f)),
            checked: false,
        }
    }

    /// An item with a check mark when `checked`.
    pub fn check(label: &str, checked: bool, f: impl Fn(&mut Ui) + 'static) -> Item {
        Item::Action {
            label: label.into(),
            action: Some(Rc::new(f)),
            checked,
        }
    }

    /// A disabled item.
    pub fn disabled(label: &str) -> Item {
        Item::Action {
            label: label.into(),
            action: None,
            checked: false,
        }
    }

    /// A submenu.
    pub fn submenu(label: &str, items: Vec<Item>) -> Item {
        Item::Submenu {
            label: label.into(),
            items,
        }
    }
}

fn row(item: &Item) -> Row {
    match item {
        Item::Action {
            label,
            action,
            checked,
        } => Row {
            kind: RowKind::Item,
            label: label.clone(),
            checked: *checked,
            disabled: action.is_none(),
            ..Row::default()
        },
        Item::Submenu { label, .. } => Row {
            kind: RowKind::Item,
            label: label.clone(),
            submenu: true,
            ..Row::default()
        },
        Item::Separator => Row {
            kind: RowKind::Separator,
            ..Row::default()
        },
        Item::Header(label) => Row {
            kind: RowKind::Header,
            label: label.clone(),
            ..Row::default()
        },
    }
}

/// The popups of an open menu, the root first.
type Chain = Rc<RefCell<Vec<WindowId>>>;

fn close_chain(ui: &mut Ui, chain: &Chain) {
    let windows = std::mem::take(&mut *chain.borrow_mut());
    for w in windows.into_iter().rev() {
        ui.close_window(w);
    }
}

/// The size a menu of `items` takes.
pub fn size(ui: &mut Ui, items: &[Item]) -> Size {
    let rows: Vec<Row> = items.iter().map(row).collect();
    let (menu, _) = menus::menu(ui, &rows);
    let _ = ui.set(menu, Prop::MinWidth, Value::from(menus::metrics::MIN_WIDTH));
    let s = ui.measure(menu, Proposal::new(None, None));
    ui.destroy(menu);
    s
}

/// Shows a menu of `items` with its top-left corner at `at` (coordinates of `parent`).
/// `closed` runs once when it goes, chosen or dismissed.
pub fn show(
    ui: &mut Ui,
    parent: WindowId,
    at: Point,
    items: Vec<Item>,
    closed: Option<Action>,
) -> WindowId {
    let chain: Chain = Rc::default();
    let w = open(ui, parent, at, items, &chain);
    if let Some(root) = ui.window(w).map(|w| w.root) {
        let mut once = closed;
        let c = chain.clone();
        ui.connect(root, Signal::Closed, move |ui, _, _| {
            // A dismissed root takes its submenus along.
            let rest: Vec<WindowId> = c.borrow_mut().drain(1..).collect();
            for s in rest.into_iter().rev() {
                ui.close_window(s);
            }
            if let Some(f) = once.take() {
                f(ui);
            }
        });
    }
    w
}

fn open(ui: &mut Ui, parent: WindowId, at: Point, items: Vec<Item>, chain: &Chain) -> WindowId {
    let rows: Vec<Row> = items.iter().map(row).collect();
    let (menu, row_views) = menus::menu(ui, &rows);
    let _ = ui.set(menu, Prop::MinWidth, Value::from(menus::metrics::MIN_WIDTH));
    let size = ui.measure(menu, Proposal::new(None, None));
    // The popup's top-left corner is the anchor's bottom-left one.
    let window = ui.create_window(
        WindowKind::Popup {
            parent,
            anchor: Rect::new(at.x, at.y - size.height, size.width, size.height),
            grab: true,
        },
        size,
    );
    ui.set_content(window, menu);
    ui.show_window(window);
    chain.borrow_mut().push(window);
    let c = chain.clone();
    ui.connect(menu, Signal::Activated, move |ui, _, v| {
        let Some(i) = v.as_int().and_then(|i| usize::try_from(i).ok()) else {
            return;
        };
        match items.get(i).cloned() {
            Some(Item::Submenu { items: sub, .. }) => {
                // Only one submenu at a time below this level.
                let level = c.borrow().iter().position(|w| *w == window);
                if let Some(level) = level {
                    let deeper: Vec<WindowId> = c.borrow_mut().drain(level + 1..).collect();
                    for s in deeper.into_iter().rev() {
                        ui.close_window(s);
                    }
                }
                let r = row_views
                    .get(i)
                    .map(|r| ui.tree.window_rect(*r))
                    .unwrap_or_default();
                let at = Point::new(r.right() - 4.0, r.y - 4.0);
                open(ui, window, at, sub, &c);
            }
            Some(Item::Action {
                action: Some(f), ..
            }) => {
                // The root's Closed handler runs `closed`: emit it before closing.
                let root = c.borrow().first().copied();
                if let Some(r) = root.and_then(|w| ui.window(w).map(|w| w.root)) {
                    ui.emit(r, Signal::Closed, Value::None);
                }
                close_chain(ui, &c);
                f(ui);
            }
            _ => {}
        }
    });
    window
}
