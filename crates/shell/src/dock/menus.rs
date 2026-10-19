//! The Dock's context menus (dock.md D-04): an app's windows, Options (Keep in Dock or Remove
//! from Dock, Show in Mustache), Show All Windows, Hide, Quit (Force Quit with Option); a
//! closed app's Options and Open; the file manager's windows, New Window and places; a
//! folder's stack settings, Options and Open; the Trash's Open and Empty Trash; a minimised
//! window's Open; the separator's hiding, magnification, position, minimise effect and Dock
//! settings. The menu's frame starts 40 pt left of the icon's centre, above the Dock.

use std::path::PathBuf;
use std::rc::Rc;

use skia_safe::Image;
use vetro_lib::amalfi::alerts::{AlertButton, AlertSpec, ButtonKind, alert};
use vetro_lib::core::shell::ShellRequest;
use vetro_lib::core::{LayoutCx, PaintCx, Proposal, Signal, Ui, WindowKind};
use vetro_lib::geom::{Point, Size};

use super::model::{FILE_MANAGER, Kind};
use super::stack::{self, Content, Display, Sort};
use super::{DockState, Shared};
use crate::popmenu::{self, Item};

fn command(id: &'static str, app: &str) -> impl Fn(&mut Ui) + 'static {
    let params = serde_json::json!({ "app_id": app }).to_string();
    move |ui: &mut Ui| ui.shell(ShellRequest::ExecuteWith(id.into(), params.clone()))
}

/// The stack settings as menu items: "Sort by", "Show as", "View content as".
pub fn stack_options(ui: &mut Ui, state: &Shared, folder: &std::path::Path) -> Vec<Item> {
    let s = stack::settings(folder);
    let _ = ui;
    let pick = |label: &str, checked: bool, next: stack::Settings| {
        let (f, st) = (folder.to_path_buf(), state.clone());
        Item::check(label, checked, move |ui| {
            stack::save_settings(&f, next);
            super::repaint(ui, &st);
        })
    };
    let mut items = vec![Item::Header("Sort by".into())];
    for (sort, label) in [
        (Sort::Name, "Name"),
        (Sort::Added, "Date added"),
        (Sort::Modified, "Date modified"),
        (Sort::Created, "Date created"),
        (Sort::Kind, "Kind"),
    ] {
        items.push(pick(label, s.sort == sort, stack::Settings { sort, ..s }));
    }
    items.push(Item::Separator);
    items.push(Item::Header("Show as".into()));
    for (display, label) in [(Display::Folder, "Folder"), (Display::Stack, "Stack")] {
        items.push(pick(
            label,
            s.display == display,
            stack::Settings { display, ..s },
        ));
    }
    items.push(Item::Separator);
    items.push(Item::Header("View content as".into()));
    for (content, label) in [
        (Content::Fan, "Fan"),
        (Content::Grid, "Grid"),
        (Content::List, "List"),
        (Content::Automatic, "Automatic"),
    ] {
        items.push(pick(
            label,
            s.content == content,
            stack::Settings { content, ..s },
        ));
    }
    items
}

fn open_named(name: &str) -> String {
    format!("Open \u{201c}{name}\u{201d}")
}

/// The List view of a stack: the files, then Options (the stack settings) and Open.
pub fn stack_list(ui: &mut Ui, state: &Shared, folder: &std::path::Path) -> Vec<Item> {
    let s = stack::settings(folder);
    let mut items: Vec<Item> = stack::entries(folder, s.sort)
        .into_iter()
        .map(|e| {
            let p = e.path.clone();
            Item::action(&e.name(), move |_| super::open_path(&p))
        })
        .collect();
    if !items.is_empty() {
        items.push(Item::Separator);
    }
    items.push(Item::submenu("Options", stack_options(ui, state, folder)));
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let f = folder.to_path_buf();
    items.push(Item::action(&open_named(&name), move |_| {
        super::open_path(&f)
    }));
    items
}

/// The places of the file manager's menu.
fn places() -> Vec<(&'static str, Option<PathBuf>)> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    vec![
        (
            "Applications",
            Some(PathBuf::from("/usr/Applications")).filter(|p| p.is_dir()),
        ),
        (
            "Documents",
            super::user_dir("XDG_DOCUMENTS_DIR", "Documents"),
        ),
        ("Downloads", super::downloads_dir()),
        ("Home", home),
        ("Desktop", super::user_dir("XDG_DESKTOP_DIR", "Desktop")),
        ("Recents", None),
    ]
}

/// The menu of tile `i`.
pub fn items(ui: &mut Ui, state: &Shared, i: usize) -> Vec<Item> {
    let s = state.borrow();
    let Some(tile) = s.tiles.get(i) else {
        return Vec::new();
    };
    let st = state.clone();
    match &tile.kind {
        Kind::Window { id, title, .. } => {
            let id = id.clone();
            vec![Item::action(&open_named(title), move |ui| {
                ui.shell(ShellRequest::Activate(id.clone()))
            })]
        }
        Kind::Separator => separator_menu(&s, &st),
        Kind::Folder(p) => {
            let p = p.clone();
            drop(s);
            let mut items = stack_options(ui, state, &p);
            items.push(Item::Separator);
            let (f1, f2, st2) = (p.clone(), p.clone(), state.clone());
            items.push(Item::submenu(
                "Options",
                vec![
                    Item::action("Remove from Dock", move |ui| {
                        super::remove_folder(ui, &st2, &f1)
                    }),
                    Item::action("Show in Mustache", move |_| {
                        if let Some(parent) = f2.parent() {
                            super::open_path(parent);
                        }
                    }),
                ],
            ));
            items.push(Item::Separator);
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            items.push(Item::action(&open_named(&name), move |_| {
                super::open_path(&p)
            }));
            items
        }
        Kind::Apps => vec![
            Item::action("Remove from Dock", move |ui| {
                super::set_pinned(ui, &st, super::model::APPS_ID, false)
            }),
            Item::Separator,
            {
                let st = state.clone();
                Item::action("Show Apps", move |ui| {
                    let f = st.borrow().on_launcher.clone();
                    if let Some(f) = f {
                        f(ui);
                    }
                })
            },
        ],
        Kind::Missing(id) => {
            let id = id.clone();
            vec![Item::action("Remove from Dock", move |ui| {
                super::set_pinned(ui, &st, &id, false)
            })]
        }
        Kind::Trash => {
            let mut items = vec![Item::action("Open", move |ui| super::activate(ui, &st, i))];
            if s.trash_full {
                items.push(Item::Separator);
                let st = state.clone();
                items.push(Item::action("Empty Trash…", move |ui| {
                    confirm_empty_trash(ui, &st)
                }));
            }
            items
        }
        Kind::App {
            entry,
            pinned,
            fixed,
        } => {
            let id = entry.id.clone();
            let front = s.focus.last().cloned();
            let windows: Vec<(String, String)> = s
                .windows_of(&id)
                .iter()
                .map(|w| {
                    (
                        w.id.clone(),
                        if w.title.is_empty() {
                            entry.name.clone()
                        } else {
                            w.title.clone()
                        },
                    )
                })
                .collect();
            let mut items: Vec<Item> = windows
                .iter()
                .map(|(w, title)| {
                    let w2 = w.clone();
                    Item::check(title, front.as_deref() == Some(w), move |ui| {
                        ui.shell(ShellRequest::Activate(w2.clone()))
                    })
                })
                .collect();
            if !items.is_empty() {
                items.push(Item::Separator);
            }
            let latest = s.window_of(&id);
            let show_all = {
                let latest = latest.clone();
                move |ui: &mut Ui| {
                    if let Some(w) = &latest {
                        ui.shell(ShellRequest::ActivateApp(w.clone()));
                    }
                    ui.shell(ShellRequest::Execute("panorama.app".into()));
                }
            };
            if *fixed {
                // The file manager (#141): New Window, the places, Show All Windows, Hide.
                let fm = id.clone();
                items.push(Item::action("New Window", move |ui| {
                    let params = serde_json::json!({ "app": fm }).to_string();
                    ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
                }));
                items.push(Item::Separator);
                for (label, path) in places() {
                    items.push(match path {
                        Some(p) => Item::action(label, move |_| super::open_path(&p)),
                        None => Item::disabled(label),
                    });
                }
                items.push(Item::Separator);
                if latest.is_some() {
                    items.push(Item::action("Show All Windows", show_all));
                    items.push(Item::action("Hide", command("app.hide", FILE_MANAGER)));
                } else {
                    items.push(Item::disabled("Show All Windows"));
                }
                return items;
            }
            let keep = {
                let (st, id) = (state.clone(), id.clone());
                if *pinned {
                    Item::action("Remove from Dock", move |ui| {
                        super::set_pinned(ui, &st, &id, false)
                    })
                } else {
                    Item::action("Keep in Dock", move |ui| {
                        super::set_pinned(ui, &st, &id, true)
                    })
                }
            };
            let mut options = vec![keep];
            if let Some(dir) = desktop_file_dir(&id) {
                options.push(Item::action("Show in Mustache", move |_| {
                    super::open_path(&dir)
                }));
            }
            items.push(Item::submenu("Options", options));
            items.push(Item::Separator);
            if s.running(&id) {
                items.push(Item::action("Show All Windows", show_all));
                items.push(Item::action("Hide", command("app.hide", &id)));
                items.push(Item::action("Quit", command("app.quit", &id)));
            } else {
                items.push(Item::action("Open", move |ui| super::activate(ui, &st, i)));
            }
            items
        }
    }
}

/// The folder holding app `id`'s desktop entry (or its bundle).
fn desktop_file_dir(id: &str) -> Option<PathBuf> {
    crate::apps::data_dirs()
        .into_iter()
        .map(|d| d.join("applications"))
        .find(|d| d.join(format!("{id}.desktop")).is_file())
}

fn separator_menu(s: &DockState, state: &Shared) -> Vec<Item> {
    let o = s.options;
    let flag = |key: &'static str, v: bool| {
        move |_: &mut Ui| super::set_setting(key, zbus::zvariant::OwnedValue::from(v))
    };
    let text = |key: &'static str, v: &'static str| {
        move |_: &mut Ui| {
            if let Ok(owned) = zbus::zvariant::Value::from(v).try_to_owned() {
                super::set_setting(key, owned);
            }
        }
    };
    let _ = state;
    let mut items = vec![
        Item::action(
            if o.autohide {
                "Turn hiding off"
            } else {
                "Turn hiding on"
            },
            flag("dock.autohide", !o.autohide),
        ),
        Item::action(
            if o.magnify {
                "Turn magnification off"
            } else {
                "Turn magnification on"
            },
            flag("dock.magnify", !o.magnify),
        ),
        // The side positions are not implemented yet (#72, #73): shown disabled.
        Item::submenu(
            "Position on screen",
            vec![
                Item::disabled("Left"),
                Item::check("Bottom", true, |_| {}),
                Item::disabled("Right"),
            ],
        ),
        Item::submenu(
            "Minimize effect",
            vec![
                Item::check(
                    "Genie effect",
                    o.effect == super::Effect::Genie,
                    text("dock.minimizeEffect", "genie"),
                ),
                Item::check(
                    "Scale effect",
                    o.effect == super::Effect::Scale,
                    text("dock.minimizeEffect", "scale"),
                ),
            ],
        ),
    ];
    items.push(Item::Separator);
    items.push(Item::action("Dock settings…", |ui| {
        let params = serde_json::json!({ "app": "dev.frascella.Settings" }).to_string();
        ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
    }));
    items
}

/// Opens tile `i`'s menu: its frame starts 40 pt left of the icon's centre, above the Dock.
pub fn open(ui: &mut Ui, state: &Shared, i: usize) {
    let old = state.borrow_mut().menu.take();
    if let Some((_, w)) = old {
        ui.close_window(w);
    }
    let Some((surface, _)) = state.borrow().surface else {
        return;
    };
    let items = items(ui, state, i);
    if items.is_empty() {
        return;
    }
    show_at_tile(ui, state, surface, i, items, true);
}

/// Shows `items` above tile `i` (the List view of a stack too).
pub fn show_at_tile(
    ui: &mut Ui,
    state: &Shared,
    surface: vetro_lib::core::WindowId,
    i: usize,
    items: Vec<Item>,
    track: bool,
) {
    let now = ui.now;
    let size = popmenu::size(ui, &items);
    let (x, y) = {
        let s = state.borrow();
        let (l, refs) = s.frame(now);
        let Some(cell) = DockState::cell_of(&l, &refs, i) else {
            return;
        };
        let m = s.metrics();
        let centre = cell.rect.x + cell.rect.width / 2.0;
        let x = (centre - 27.0).clamp(4.0, (s.size.width - size.width - 4.0).max(4.0));
        let tip = cell.icon.y - m.label_gap(cell.icon.width);
        (x, tip - geometry_arrow() - size.height)
    };
    let st = state.clone();
    let w = popmenu::show(
        ui,
        surface,
        Point::new(x, y),
        items,
        Some(Rc::new(move |ui: &mut Ui| {
            st.borrow_mut().menu = None;
            super::view::sync_region(ui, &st);
            super::repaint(ui, &st);
        })),
    );
    if track {
        state.borrow_mut().menu = Some((i, w));
    }
    super::view::sync_region(ui, state);
    super::repaint(ui, state);
}

fn geometry_arrow() -> f32 {
    super::geometry::LABEL_ARROW
}

/// "Empty Trash…": the alert in a window of its own (D-07).
pub fn confirm_empty_trash(ui: &mut Ui, state: &Shared) {
    let dark = ui.appearance.dark;
    let icon = crate::apps::shared_icon("files", "trash-full", "icon.png", dark).map(|img| {
        ui.create(IconView {
            image: img,
            size: 56.0,
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
        ..AlertSpec::default()
    };
    let a = alert(ui, spec);
    let size = ui.measure(a, Proposal::new(None, None));
    let w = ui.create_window(WindowKind::Panel, size);
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
        win.title = "Empty Trash".into();
        win.decorated = false;
    }
    ui.set_content(w, a);
    ui.show_window(w);
    ui.set_focus(a, false);
    let st = state.clone();
    ui.connect(a, Signal::Activated, move |ui, _, v| {
        if v.as_int() == Some(0) {
            super::empty_trash();
            st.borrow_mut().trash_full = false;
            super::repaint(ui, &st);
            // ~0.2 s after the Trash turns empty (FILE-SOUNDS).
            ui.after(0.2, |_| {
                vetro_lib::platform::spawn(async {
                    if let Some(s) = vetro_lib::platform::services()
                        && let Ok(p) = s.sound().await
                    {
                        let _ = p.play_sound("trash-empty").await;
                    }
                });
            });
        }
        ui.close_window(w);
    });
    ui.connect(a, Signal::CloseRequested, move |ui, _, _| {
        ui.close_window(w)
    });
}

/// An image at a fixed size (an alert's icon).
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
