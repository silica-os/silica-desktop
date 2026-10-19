//! The Dock's view: drawing and every pointer interaction.
//!
//! - Magnification and the hover label (D-00, D-12).
//! - A click activates, a secondary click or a long press opens the menu (D-02 to D-04).
//! - Dragging (D-05): past 5 pt an icon follows the pointer at its magnified size; its place
//!   closes at once and a gap opens where it would land, the neighbours sliding over (~265 ms);
//!   dropped inside it takes the gap (an open app dropped among the apps is kept in the Dock);
//!   held 130 pt above the grab point "Remove" shows and dropping removes it (an app that is
//!   not running fades in place); dropped on the Trash it is removed from the Dock (#235).
//!   Folders reorder among the folders the same way.
//! - Dragging the separator up or down resizes the Dock (1 pt per point, 16–128).
//! - Files dragged in from another app (D-05, MU-40): over an app it darkens and springs open
//!   after ~830 ms, dropping opens them with it; over the Trash they go to the Trash; over the
//!   middle of a folder they move into it; between folders a gap opens and folders dropped
//!   there are added to the Dock.
//! - Auto-hide (SH-DK-auto): hidden below the screen, the last row reveals it after ~190 ms.

use std::path::{Path, PathBuf};

use vetro_lib::amalfi::foundation::Label;
use vetro_lib::amalfi::tokens::typography;
use vetro_lib::color::Color;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::display::GlassProfile;
use vetro_lib::core::{Button, Event, EventCx, Handled, LayoutCx, PaintCx, Proposal, Ui};
use vetro_lib::geom::{Insets, Point, Rect, Size};
use vetro_lib::shape::Shape;
use vetro_lib::style::{Border, Style};
use vetro_lib::text::Align;

use super::geometry::{
    self, BADGE_HEIGHT, BADGE_RIGHT, DRAG_INSIDE_ABOVE, DRAG_THRESHOLD, END_STRIP, LABEL_ARROW,
    LABEL_HEIGHT, LABEL_RADIUS, Layout, Metrics, PROGRESS, REMOVE_ABOVE, THUMB, THUMB_BADGE,
    THUMB_BADGE_BELOW,
};
use super::model::{FILE_MANAGER, Kind};
use super::stack;
use super::{DockState, Gap, Shared, SlotRef};

/// A press on a tile.
#[derive(Debug, Clone, Copy)]
struct Press {
    tile: usize,
    at: Point,
    time: f64,
}

/// An icon being dragged.
#[derive(Debug, Clone)]
struct Drag {
    tile: usize,
    key: String,
    /// The grab point in the icon, and the icon's side.
    grab: Point,
    side: f32,
    down: Point,
    pointer: Point,
    inside: bool,
    remove: bool,
    over_trash: bool,
    folder: bool,
}

/// The separator being dragged: the pointer's y and the canvas at the start.
#[derive(Debug, Clone, Copy)]
struct Resize {
    y: f32,
    canvas: f32,
}

/// A removed icon fading in place.
#[derive(Debug, Clone)]
struct Ghost {
    kind: Kind,
    rect: Rect,
    since: f64,
}

/// What a drag of files from another app is over.
#[derive(Debug, Clone, PartialEq)]
enum FileTarget {
    /// An app (tile index), since when.
    App(usize, f64),
    /// The Trash.
    Trash(usize),
    /// The middle of a folder.
    Into(usize),
    /// A gap among the folders, after this key.
    Gap(Option<String>),
}

/// The Dock's shelf, tiles, label and drags.
pub struct DockView {
    state: Shared,
    labels: Vec<Label>,
    pressed: Option<Press>,
    drag: Option<Drag>,
    resize: Option<Resize>,
    ghost: Option<Ghost>,
    file: Option<FileTarget>,
    hovered: Option<usize>,
    pointer: Option<Point>,
    in_zone: bool,
    /// The icon whose label was hidden for its menu (until the pointer leaves it).
    label_hidden: Option<usize>,
}

impl DockView {
    /// A Dock over `state`.
    pub fn new(state: Shared) -> DockView {
        DockView {
            state,
            labels: Vec::new(),
            pressed: None,
            drag: None,
            resize: None,
            ghost: None,
            file: None,
            hovered: None,
            pointer: None,
            in_zone: false,
            label_hidden: None,
        }
    }

    /// The tile under `p` in a frame (above the shelf, only a raised icon counts).
    fn tile_at(l: &Layout, refs: &[SlotRef], p: Point) -> Option<usize> {
        let slot = l.cells.iter().position(|c| {
            p.x >= c.rect.x
                && p.x < c.rect.right()
                && (p.y >= l.shelf.y - geometry::ZONE_ABOVE || p.y >= c.icon.y)
        })?;
        match refs.get(slot) {
            Some(SlotRef::Tile(i)) => Some(*i),
            _ => None,
        }
    }

    /// Follows the pointer: the zone, the peak, the hovered tile, auto-hide.
    fn track(&mut self, cx: &mut EventCx<'_>, p: Option<Point>) {
        let now = cx.now();
        let state = self.state.clone();
        let (in_zone, hovered) = {
            let mut s = state.borrow_mut();
            let (l, refs) = s.frame(now);
            let in_zone = p.is_some_and(|p| {
                if self.in_zone {
                    geometry::stays_in_zone(&l, p.x, p.y)
                } else {
                    geometry::in_zone(&l, p.x, p.y)
                }
            });
            let target = if in_zone && s.options.magnify {
                1.0
            } else {
                0.0
            };
            if s.mag.1 != target {
                let m = s.mag_at(now);
                s.mag = (m, target, now);
            }
            if in_zone && let Some(p) = p {
                s.peak_x = p.x;
            }
            let strip = p
                .is_some_and(|p| p.x < l.shelf.x + END_STRIP || p.x >= l.shelf.right() - END_STRIP);
            let hovered = p
                .and_then(|p| Self::tile_at(&l, &refs, p))
                .filter(|_| !strip && in_zone);
            (in_zone, hovered)
        };
        if hovered != self.label_hidden {
            self.label_hidden = None;
        }
        self.in_zone = in_zone;
        self.hovered = hovered;
        self.pointer = p;
        self.autohide(cx, p);
        cx.request_paint();
        cx.animate();
    }

    /// Auto-hide: the last row of the screen reveals the Dock after ~190 ms; leaving it hides it.
    fn autohide(&mut self, cx: &mut EventCx<'_>, p: Option<Point>) {
        let now = cx.now();
        let state = self.state.clone();
        let (autohide, target, height, busy) = {
            let s = state.borrow();
            (
                s.options.autohide,
                s.hide.1,
                s.size.height,
                s.menu.is_some() || s.stack.is_some(),
            )
        };
        if !autohide {
            return;
        }
        let bottom = p.is_some_and(|p| p.y >= height - 1.0);
        if target < 1.0 && bottom {
            let st = state.clone();
            cx.ui().after(geometry::AUTOHIDE_DELAY, move |ui| {
                let mut s = st.borrow_mut();
                if s.hide.1 < 1.0 {
                    let now = ui.now;
                    let from = s.shown(now);
                    s.hide = (from, 1.0, now, geometry::AUTOHIDE_REVEAL);
                    drop(s);
                    sync_region(ui, &st);
                    super::animate(ui, &st);
                }
            });
        } else if target > 0.0
            && !self.in_zone
            && !bottom
            && !busy
            && self.drag.is_none()
            && self.resize.is_none()
        {
            let mut s = state.borrow_mut();
            let from = s.shown(now);
            s.hide = (from, 0.0, now, geometry::AUTOHIDE_HIDE);
        }
    }

    // ----- Dragging icons --------------------------------------------------------------------

    /// The tiles a dragged item can go among: apps between the file manager and the
    /// separator, folders between the separator and the windows or the Trash.
    fn region(s: &DockState, folder: bool) -> (Option<String>, Vec<usize>) {
        let sep = s
            .tiles
            .iter()
            .rposition(|t| matches!(t.kind, Kind::Separator))
            .unwrap_or(s.tiles.len());
        if folder {
            let end = s
                .tiles
                .iter()
                .position(|t| matches!(t.kind, Kind::Window { .. } | Kind::Trash))
                .unwrap_or(s.tiles.len());
            let before = s.tiles.get(sep).map(|t| t.kind.key());
            (before, ((sep + 1)..end).collect())
        } else {
            let first_sep = s
                .tiles
                .iter()
                .position(|t| matches!(t.kind, Kind::Separator))
                .unwrap_or(s.tiles.len());
            let start = usize::from(
                s.tiles
                    .first()
                    .is_some_and(|t| matches!(t.kind, Kind::App { fixed: true, .. })),
            );
            let before = (start > 0)
                .then(|| s.tiles.first().map(|t| t.kind.key()))
                .flatten();
            (before, (start..first_sep).collect())
        }
    }

    /// Where a dragged item at `x` would land: the key of the tile before the gap.
    fn landing(
        s: &DockState,
        l: &Layout,
        refs: &[SlotRef],
        dragged: &str,
        folder: bool,
        x: f32,
    ) -> Option<String> {
        let (before, region) = Self::region(s, folder);
        let mut after = before;
        for i in region {
            let Some(t) = s.tiles.get(i) else { continue };
            let key = t.kind.key();
            if key == dragged || t.dying.is_some() {
                continue;
            }
            let Some(c) = DockState::cell_of(l, refs, i) else {
                continue;
            };
            if c.rect.x + c.rect.width / 2.0 < x {
                after = Some(key);
            }
        }
        after
    }

    /// Moves the gap to after `key` (closing the old one, opening the new one).
    fn move_gap(s: &mut DockState, after: Option<String>, now: f64, folder: bool) {
        if let Some(g) = s.gaps.iter().find(|g| g.opening)
            && g.after == after
        {
            return;
        }
        for g in s.gaps.iter_mut().filter(|g| g.opening) {
            g.from = g.presence(now);
            g.opening = false;
            g.since = now;
        }
        // A closing gap at that place opens again from where it is.
        if let Some(g) = s.gaps.iter_mut().find(|g| !g.opening && g.after == after) {
            g.from = g.presence(now);
            g.opening = true;
            g.since = now;
            g.folder = folder;
            return;
        }
        s.gaps.push(Gap {
            after,
            opening: true,
            since: now,
            from: 0.0,
            folder,
        });
    }

    /// Closes every gap (`at_once`: no animation).
    fn close_gaps(s: &mut DockState, now: f64, at_once: bool) {
        if at_once {
            s.gaps.clear();
            return;
        }
        for g in s.gaps.iter_mut().filter(|g| g.opening) {
            g.from = g.presence(now);
            g.opening = false;
            g.since = now;
        }
    }

    fn start_drag(&mut self, cx: &mut EventCx<'_>, press: Press, p: Point) {
        let now = cx.now();
        let state = self.state.clone();
        let mut s = state.borrow_mut();
        let Some(t) = s.tiles.get(press.tile) else {
            return;
        };
        if !t.kind.draggable() || t.dying.is_some() {
            return;
        }
        let key = t.kind.key();
        let folder = matches!(t.kind, Kind::Folder(_));
        let (l, refs) = s.frame(now);
        let Some(cell) = DockState::cell_of(&l, &refs, press.tile) else {
            return;
        };
        let before = s
            .tiles
            .iter()
            .take(press.tile)
            .rev()
            .find(|t| t.dying.is_none())
            .map(|t| t.kind.key());
        s.dragging = Some(key.clone());
        // Its place closes at once and the gap stands there, full.
        s.gaps.push(Gap {
            after: before,
            opening: true,
            since: now - 1.0,
            from: 1.0,
            folder: false,
        });
        drop(s);
        self.drag = Some(Drag {
            tile: press.tile,
            key,
            grab: Point::new(press.at.x - cell.icon.x, press.at.y - cell.icon.y),
            side: cell.icon.width,
            down: press.at,
            pointer: p,
            inside: true,
            remove: false,
            over_trash: false,
            folder,
        });
        self.update_drag(cx, p);
    }

    fn update_drag(&mut self, cx: &mut EventCx<'_>, p: Point) {
        let now = cx.now();
        let Some(mut d) = self.drag.clone() else {
            return;
        };
        let state = self.state.clone();
        let mut s = state.borrow_mut();
        let (l, refs) = s.frame(now);
        d.pointer = p;
        d.inside = p.y >= l.shelf.y - DRAG_INSIDE_ABOVE;
        d.remove = !d.inside && p.y < d.down.y - REMOVE_ABOVE;
        let trash = s.tiles.iter().position(|t| matches!(t.kind, Kind::Trash));
        d.over_trash = trash
            .and_then(|i| DockState::cell_of(&l, &refs, i))
            .is_some_and(|c| {
                c.rect.contains(p) || (p.x >= c.rect.x && p.x < c.rect.right() && d.inside)
            });
        if d.inside && !d.over_trash {
            let after = Self::landing(&s, &l, &refs, &d.key, d.folder, p.x);
            Self::move_gap(&mut s, after, now, false);
        } else {
            // Dragging above the Dock (or onto the Trash) closes the gap at once.
            Self::close_gaps(&mut s, now, true);
        }
        drop(s);
        self.drag = Some(d);
        cx.request_paint();
        cx.animate();
    }

    fn end_drag(&mut self, cx: &mut EventCx<'_>) {
        let now = cx.now();
        let Some(d) = self.drag.take() else { return };
        let state = self.state.clone();
        let kind = state
            .borrow()
            .tiles
            .iter()
            .find(|t| t.kind.key() == d.key)
            .map(|t| t.kind.clone());
        let Some(kind) = kind else {
            let mut s = state.borrow_mut();
            s.dragging = None;
            s.gaps.clear();
            return;
        };
        if d.remove || d.over_trash {
            let running = kind.app_id().is_some_and(|a| state.borrow().running(a));
            {
                let mut s = state.borrow_mut();
                s.dragging = None;
                s.gaps.clear();
            }
            if !running {
                // It fades in place where it was dropped.
                self.ghost = Some(Ghost {
                    kind: kind.clone(),
                    rect: Rect::new(
                        d.pointer.x - d.grab.x,
                        d.pointer.y - d.grab.y,
                        d.side,
                        d.side,
                    ),
                    since: now,
                });
            }
            match &kind {
                Kind::Folder(p) => super::remove_folder(cx.ui(), &state, p),
                k => {
                    if let Some(id) = k.pin_id() {
                        let id = id.to_string();
                        super::set_pinned(cx.ui(), &state, &id, false);
                    }
                }
            }
            if !running {
                // No shrink: the ghost fades instead.
                let mut s = state.borrow_mut();
                s.tiles.retain(|t| t.kind.key() != d.key);
            }
        } else if d.inside {
            let mut s = state.borrow_mut();
            let after = s
                .gaps
                .iter()
                .find(|g| g.opening)
                .and_then(|g| g.after.clone());
            let (before, region) = Self::region(&s, d.folder);
            let mut order: Vec<Kind> = region
                .iter()
                .filter_map(|i| s.tiles.get(*i))
                .filter(|t| t.kind.key() != d.key && t.dying.is_none())
                .map(|t| t.kind.clone())
                .collect();
            let at = match &after {
                a if *a == before => 0,
                Some(a) => order
                    .iter()
                    .position(|k| k.key() == *a)
                    .map_or(order.len(), |p| p + 1),
                None => 0,
            };
            order.insert(at.min(order.len()), kind.clone());
            if d.folder {
                s.inputs.folders = order
                    .iter()
                    .filter_map(|k| match k {
                        Kind::Folder(p) => Some(p.clone()),
                        _ => None,
                    })
                    .collect();
                super::save_folders(&s.inputs.folders);
            } else {
                let mut pinned: Vec<String> = s
                    .inputs
                    .pinned
                    .iter()
                    .filter(|p| *p == FILE_MANAGER)
                    .cloned()
                    .collect();
                for k in &order {
                    if (k.pinned() || k.key() == d.key)
                        && let Some(id) = k.pin_id()
                    {
                        pinned.push(id.to_string());
                    }
                }
                s.inputs.pinned = pinned;
                super::save_pinned(&s.inputs.pinned);
            }
            s.dragging = None;
            s.gaps.clear();
            s.recompose(now, false);
        } else {
            // Released above the Dock but not far enough: it goes back.
            let mut s = state.borrow_mut();
            s.dragging = None;
            s.gaps.clear();
        }
        let _ = d.tile;
        self.track(cx, Some(d.pointer));
        sync_region(cx.ui(), &state);
        cx.request_paint();
        cx.animate();
    }

    // ----- Files from other apps -------------------------------------------------------------

    fn file_target(&self, p: Point, now: f64) -> Option<FileTarget> {
        let s = self.state.borrow();
        let (l, refs) = s.frame(now);
        if p.y < l.shelf.y - geometry::ZONE_ABOVE {
            return None;
        }
        let sep = s
            .tiles
            .iter()
            .rposition(|t| matches!(t.kind, Kind::Separator))?;
        let sep_x =
            DockState::cell_of(&l, &refs, sep).map_or(0.0, |c| c.rect.x + c.rect.width / 2.0);
        if let Some(i) = Self::tile_at(&l, &refs, p)
            && let Some(t) = s.tiles.get(i)
        {
            let c = DockState::cell_of(&l, &refs, i)?;
            match &t.kind {
                Kind::App { .. } => {
                    let since = match &self.file {
                        Some(FileTarget::App(j, since)) if *j == i => *since,
                        _ => now,
                    };
                    return Some(FileTarget::App(i, since));
                }
                Kind::Trash => return Some(FileTarget::Trash(i)),
                Kind::Folder(_)
                    if (p.x - (c.rect.x + c.rect.width / 2.0)).abs() < c.rect.width / 4.0 =>
                {
                    return Some(FileTarget::Into(i));
                }
                _ => {}
            }
        }
        if p.x > sep_x {
            let after = Self::landing(&s, &l, &refs, "", true, p.x);
            return Some(FileTarget::Gap(after));
        }
        None
    }

    fn file_over(&mut self, cx: &mut EventCx<'_>, p: Point) {
        let now = cx.now();
        let target = self.file_target(p, now);
        let state = self.state.clone();
        {
            let mut s = state.borrow_mut();
            match &target {
                Some(FileTarget::Gap(after)) => Self::move_gap(&mut s, after.clone(), now, true),
                _ => Self::close_gaps(&mut s, now, false),
            }
        }
        // Spring-loading: a file held over an app opens its windows (MU-40).
        let newly = match (&target, &self.file) {
            (Some(FileTarget::App(i, _)), Some(FileTarget::App(j, _))) => i != j,
            (Some(FileTarget::App(..)), _) => true,
            _ => false,
        };
        if newly && let Some(FileTarget::App(i, since)) = target.clone() {
            let (id, st) = (cx.id(), state.clone());
            cx.ui().after(geometry::SPRING_LOAD, move |ui| {
                let still = ui
                    .with_widget::<DockView, _>(id, |v| v.file == Some(FileTarget::App(i, since)))
                    .unwrap_or(false);
                if !still {
                    return;
                }
                let window = {
                    let s = st.borrow();
                    s.tiles
                        .get(i)
                        .and_then(|t| t.kind.app_id().map(str::to_string))
                        .and_then(|a| s.window_of(&a))
                };
                if let Some(w) = window {
                    ui.shell(vetro_lib::core::shell::ShellRequest::ActivateApp(w));
                    ui.shell(vetro_lib::core::shell::ShellRequest::Execute(
                        "panorama.app".into(),
                    ));
                }
            });
        }
        self.file = target;
        self.track(cx, Some(p));
    }

    fn file_leave(&mut self, cx: &mut EventCx<'_>) {
        let now = cx.now();
        self.file = None;
        {
            let mut s = self.state.borrow_mut();
            Self::close_gaps(&mut s, now, false);
        }
        self.track(cx, None);
    }

    fn file_drop(&mut self, cx: &mut EventCx<'_>, p: Point, items: &[String]) {
        let now = cx.now();
        let paths = super::uri_paths(items);
        let target = self.file.take().or_else(|| self.file_target(p, now));
        let state = self.state.clone();
        match target {
            Some(FileTarget::App(i, _)) => {
                let id = state
                    .borrow()
                    .tiles
                    .get(i)
                    .and_then(|t| t.kind.app_id().map(str::to_string));
                if let Some(id) = id {
                    if id == FILE_MANAGER {
                        for p in &paths {
                            super::open_path(p);
                        }
                    } else {
                        super::open_with(&id, &paths);
                    }
                }
            }
            Some(FileTarget::Trash(_)) => {
                super::move_to_trash(&paths);
                state.borrow_mut().trash_full = true;
                vetro_lib::platform::spawn(async {
                    if let Some(s) = vetro_lib::platform::services()
                        && let Ok(p) = s.sound().await
                    {
                        let _ = p.play_sound("trash-move").await;
                    }
                });
            }
            Some(FileTarget::Into(i)) => {
                let folder = state.borrow().tiles.get(i).and_then(|t| match &t.kind {
                    Kind::Folder(f) => Some(f.clone()),
                    _ => None,
                });
                if let Some(f) = folder {
                    super::move_into(&paths, &f);
                    crate::file_icons::forget_under(&f);
                }
            }
            Some(FileTarget::Gap(after)) => {
                let dirs: Vec<PathBuf> = paths.into_iter().filter(|p| p.is_dir()).collect();
                let mut s = state.borrow_mut();
                let mut folders: Vec<PathBuf> = s
                    .inputs
                    .folders
                    .iter()
                    .filter(|f| !dirs.contains(f))
                    .cloned()
                    .collect();
                let at = match &after {
                    Some(k) => folders
                        .iter()
                        .position(|f| format!("folder:{}", f.display()) == *k)
                        .map_or(0, |p| p + 1),
                    None => 0,
                };
                for (n, d) in dirs.into_iter().enumerate() {
                    folders.insert((at + n).min(folders.len()), d);
                }
                s.inputs.folders = folders;
                super::save_folders(&s.inputs.folders);
                s.gaps.clear();
                s.recompose(now, true);
            }
            None => {}
        }
        {
            let mut s = state.borrow_mut();
            Self::close_gaps(&mut s, now, false);
        }
        sync_region(cx.ui(), &state);
        cx.request_paint();
        cx.animate();
    }
}

/// The open stack opens or closes (a click on its tile).
pub fn toggle_stack(ui: &mut Ui, state: &Shared, i: usize, folder: &Path) {
    let now = ui.now;
    let key = state
        .borrow()
        .tiles
        .get(i)
        .map(|t| t.kind.key())
        .unwrap_or_default();
    let open = state.borrow().stack.clone();
    if let Some((k, _, v)) = open {
        stack::close(ui, v, stack::CloseBy::Button);
        if k == key {
            return;
        }
    }
    let settings = stack::settings(folder);
    let count = stack::entries(folder, settings.sort).len();
    let Some((surface, view)) = state.borrow().surface else {
        return;
    };
    let view_kind = match settings.content {
        stack::Content::Fan => stack::View::Fan,
        stack::Content::Grid => stack::View::Grid,
        stack::Content::Automatic => stack::automatic_view(count),
        stack::Content::List => {
            let items = super::menus::stack_list(ui, state, folder);
            super::menus::show_at_tile(ui, state, surface, i, items, true);
            return;
        }
    };
    let (centre, zone_top, width) = {
        let s = state.borrow();
        let (l, refs) = s.frame(now);
        let Some(c) = DockState::cell_of(&l, &refs, i) else {
            return;
        };
        (c.rect.x + c.rect.width / 2.0, l.rest.y - 4.0, s.size.width)
    };
    let dark = ui.theme_of(view).dark();
    let st = state.clone();
    let k2 = key.clone();
    let opened = stack::open(
        ui,
        stack::Place {
            surface,
            width,
            centre_x: centre,
            zone_top,
        },
        folder,
        view_kind,
        dark,
        Box::new(move |ui: &mut Ui| {
            let w = {
                let mut s = st.borrow_mut();
                match s.stack.take() {
                    Some((k, w, _)) if k == k2 => Some(w),
                    other => {
                        s.stack = other;
                        None
                    }
                }
            };
            if let Some(w) = w {
                ui.close_window(w);
            }
            sync_region(ui, &st);
            super::repaint(ui, &st);
        }),
    );
    if let Some((w, v)) = opened {
        state.borrow_mut().stack = Some((key, w, v));
    }
    sync_region(ui, state);
    super::repaint(ui, state);
}

/// Limits the surface's input to the shelf and what rises above it; with auto-hide, the
/// screen's last row too (hidden: only that row).
pub fn sync_region(ui: &mut Ui, state: &Shared) {
    let now = ui.now;
    let (surface, rects) = {
        let s = state.borrow();
        let (l, _) = s.frame(now);
        let mut rects = Vec::new();
        let hidden = s.options.autohide && s.shown(now) <= 0.0 && s.hide.1 <= 0.0;
        if !hidden {
            let top = l
                .cells
                .iter()
                .map(|c| c.icon.y)
                .fold(l.shelf.y - geometry::ZONE_ABOVE, f32::min)
                .max(0.0);
            rects.push(Rect::new(
                l.shelf.x.min(l.rest.x).floor(),
                top.floor(),
                l.shelf.width.max(l.rest.width).ceil() + 1.0,
                (s.size.height - top).ceil(),
            ));
        }
        if s.options.autohide {
            rects.push(Rect::new(0.0, s.size.height - 1.0, s.size.width, 1.0));
        }
        (s.surface, rects)
    };
    if let Some((w, _)) = surface {
        ui.set_input_region(w, Some(rects));
    }
}

/// Colours of the dot and the separator (kit `Dock/Indicator/Dot`, `Dock/Separator/Line`).
fn dot_color(dark: bool) -> Color {
    if dark {
        Color::from_u32(0xb0b4bcff)
    } else {
        Color::from_u32(0x5f636bff)
    }
}

fn separator_color(dark: bool) -> Color {
    if dark {
        Color::rgba(0xf4, 0xf5, 0xf7, 0x4d)
    } else {
        Color::rgba(0x1d, 0x1e, 0x21, 0x59)
    }
}

/// An icon image, darkened when pressed (or under a dragged file).
fn paint_image(
    cx: &mut PaintCx<'_>,
    rect: Rect,
    img: skia_safe::Image,
    opacity: f32,
    dark_shade: bool,
) {
    if !dark_shade {
        cx.rec.image("Icon", rect, img, opacity);
        return;
    }
    cx.rec.custom(
        "Icon",
        rect,
        std::rc::Rc::new(move |canvas: &skia_safe::Canvas, r: Rect| {
            let mut paint = skia_safe::Paint::default();
            paint.set_alpha_f(opacity);
            paint.set_color_filter(skia_safe::color_filters::blend(
                skia_safe::Color::from_argb(0x40, 0, 0, 0),
                skia_safe::BlendMode::SrcATop,
            ));
            canvas.draw_image_rect_with_sampling_options(
                &img,
                None,
                skia_safe::Rect::from_xywh(r.x, r.y, r.width, r.height),
                skia_safe::SamplingOptions::from(skia_safe::CubicResampler::mitchell()),
                &paint,
            );
        }),
    );
}

/// An app without an icon (kit `Dock/Tile/Missing`): a 48 pt tile with a question mark.
fn missing_tile(cx: &mut PaintCx<'_>, icon: Rect, opacity: f32, dark: bool) {
    let k = icon.width / 60.0;
    let (fill, border, ink) = if dark {
        (
            Color::from_u32(0x3a3c42ff),
            Color::from_u32(0xffffff14),
            Color::from_u32(0x8e929aff),
        )
    } else {
        (
            Color::from_u32(0xe6e8ecff),
            Color::from_u32(0x00000014),
            Color::from_u32(0x9a9ea6ff),
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

/// How an icon is drawn.
#[derive(Debug, Clone, Copy)]
struct Look {
    opacity: f32,
    dark: bool,
    /// Darkened (pressed, under a dragged file).
    shade: bool,
    /// Its stack is open.
    open: bool,
}

/// Paints the icon of `kind` in `icon`.
fn paint_kind(cx: &mut PaintCx<'_>, kind: &Kind, icon: Rect, look: Look, s: &DockState) {
    let Look {
        opacity,
        dark,
        shade,
        open: stack_open,
    } = look;
    match kind {
        Kind::App { entry, .. } => match crate::apps::icon(entry, dark) {
            Some(img) => paint_image(cx, icon, img, opacity, shade),
            None => missing_tile(cx, icon, opacity, dark),
        },
        Kind::Apps => {
            let img = crate::apps::silica_icon_file("apps", dark)
                .and_then(|p| crate::apps::icon_named(&p.to_string_lossy()));
            match img {
                Some(img) => paint_image(cx, icon, img, opacity, shade),
                None => {
                    cx.rec.push_opacity(opacity);
                    cx.rec.glyph(
                        "Apps",
                        icon.inset(Insets::all(icon.width * 0.2)),
                        "square-grid-2x2",
                        400,
                        dot_color(dark),
                    );
                    cx.rec.pop();
                }
            }
        }
        Kind::Missing(_) => missing_tile(cx, icon, opacity, dark),
        Kind::Folder(p) => stack::paint_tile(cx, icon, p, dark, stack_open, opacity),
        Kind::Trash => {
            let name = if s.trash_full {
                "trash-full"
            } else {
                "trash-empty"
            };
            match crate::apps::shared_icon("files", name, "icon.png", dark) {
                Some(img) => paint_image(cx, icon, img, opacity, shade),
                None => missing_tile(cx, icon, opacity, dark),
            }
        }
        Kind::Window { id, app, .. } => {
            let k = icon.width / 60.0;
            let (tw, th) = (THUMB.0 * k, THUMB.1 * k);
            let thumb = Rect::new(
                icon.x + (icon.width - tw) / 2.0,
                icon.y + (icon.height - th) / 2.0,
                tw,
                th,
            );
            cx.rec.push_opacity(opacity);
            match s.thumbnails.get(id) {
                Some(img) => {
                    let (iw, ih) = (img.width() as f32, img.height().max(1) as f32);
                    let fit = (tw / iw).min(th / ih);
                    let r = Rect::new(
                        thumb.x + (tw - iw * fit) / 2.0,
                        thumb.y + (th - ih * fit) / 2.0,
                        iw * fit,
                        ih * fit,
                    );
                    cx.rec.image("Thumbnail", r, img.clone(), 1.0);
                }
                None => cx.rec.layer(
                    "Thumbnail",
                    thumb,
                    Shape::rounded(4.0 * k),
                    Style::fill(Color::from_u32(0x8e929a80)),
                ),
            }
            let b = THUMB_BADGE * k;
            let badge = Rect::new(
                thumb.right() - b * 0.75,
                thumb.bottom() + THUMB_BADGE_BELOW * k - b,
                b,
                b,
            );
            if let Some(img) = crate::apps::entry(app).and_then(|e| crate::apps::icon(&e, dark)) {
                cx.rec.image("App", badge, img, 1.0);
            }
            cx.rec.pop();
        }
        Kind::Separator => {}
    }
}

/// Paints a label (kit `Dock/Label`): Glass/Thick body 28 tall, radius 8, an arrow 6 deep whose
/// tip is at (`centre`, `tip_y`).
fn paint_label(
    cx: &mut PaintCx<'_>,
    labels: &mut Vec<Label>,
    slot: usize,
    text: &str,
    centre: f32,
    tip_y: f32,
    opacity: f32,
) {
    let theme = *cx.theme();
    let style = theme.text(&typography::BODY, theme.colors.text_primary);
    if labels.len() <= slot {
        labels.resize_with(slot + 1, Label::default);
    }
    let tw = labels.get_mut(slot).map_or(0.0, |l| l.width(text, &style));
    let w = geometry::label_width(tw);
    let body = Rect::new(
        (centre - w / 2.0).round(),
        tip_y - LABEL_ARROW - LABEL_HEIGHT,
        w,
        LABEL_HEIGHT,
    );
    cx.rec.push_opacity(opacity);
    cx.glass(
        "Label",
        body,
        Shape::rounded(LABEL_RADIUS),
        "Glass/Thick",
        GlassProfile::Tooltip,
    );
    let tint = theme.material_style("Glass/Thick");
    let arrow = Rect::new(centre - 6.0, body.bottom() - 0.5, 12.0, LABEL_ARROW + 0.5);
    let color = tint
        .fills
        .iter()
        .rev()
        .find_map(|f| match f {
            vetro_lib::style::Paint::Solid(c) => Some(*c),
            _ => None,
        })
        .unwrap_or(Color::WHITE);
    cx.rec.custom(
        "Arrow",
        arrow,
        std::rc::Rc::new(move |canvas: &skia_safe::Canvas, r: Rect| {
            let mut path = skia_safe::PathBuilder::new();
            path.move_to((r.x, r.y));
            path.line_to((r.right(), r.y));
            path.line_to((r.x + r.width / 2.0, r.bottom()));
            path.close();
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(skia_safe::Color::from_argb(
                color.a, color.r, color.g, color.b,
            ));
            canvas.draw_path(&path.detach(), &paint);
        }),
    );
    let l = crate::text::lay(labels, slot, text, &style, Align::Center, w, Some(1));
    let h = l.size().height;
    cx.rec.text(
        "Label text",
        Rect::new(body.x, body.y + (LABEL_HEIGHT - h) / 2.0, w, h),
        l,
        Some(typography::BODY.name),
        theme.colors.text_primary,
    );
    cx.rec.pop();
}

impl vetro_lib::core::Widget for DockView {
    fn kind(&self) -> &'static str {
        "Dock"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        let s = self.state.borrow().size;
        p.replacing_unspecified(s)
    }

    /// The width the compositor gave the layer (the output's): the Dock centres on it. The
    /// window can be created before the outputs are known, at a guessed width.
    fn place(&mut self, _cx: &mut LayoutCx<'_>, size: Size) {
        let changed = {
            let mut s = self.state.borrow_mut();
            let changed = size.width >= 1.0 && (s.size.width - size.width).abs() > 0.5;
            if changed {
                s.size.width = size.width;
            }
            changed
        };
        if changed {
            let state = self.state.clone();
            vetro_lib::platform::post(move |ui| sync_region(ui, &state));
        }
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        let state = self.state.clone();
        let more = {
            let mut s = state.borrow_mut();
            let (from, to, _) = s.mag;
            if from != to && s.mag_at(now) == to {
                s.mag = (to, to, now);
            }
            let running: Vec<String> = s.inputs.windows.iter().map(|w| w.app.clone()).collect();
            for t in &mut s.tiles {
                if t.born.is_some_and(|b| now - b >= geometry::GROW_TIME) {
                    t.born = None;
                }
                if let Some(l) = t.launched {
                    let up = t
                        .kind
                        .app_id()
                        .is_some_and(|a| running.iter().any(|r| r == a));
                    let elapsed = now - l;
                    let landed = elapsed >= geometry::BOUNCE_PERIOD
                        && (elapsed % geometry::BOUNCE_PERIOD) < 0.02;
                    if (up || elapsed > geometry::BOUNCE_LIMIT)
                        && (landed || elapsed > geometry::BOUNCE_LIMIT + 1.0)
                    {
                        t.launched = None;
                    }
                }
            }
            s.tiles
                .retain(|t| !t.dying.is_some_and(|d| now - d >= geometry::SHRINK_TIME));
            s.gaps.retain(|g| !g.gone(now));
            let (from, to, since, dur) = s.hide;
            if from != to && now - since >= dur {
                s.hide = (to, to, now, 0.0);
            }
            s.animating(now)
        };
        if self
            .ghost
            .as_ref()
            .is_some_and(|g| now - g.since >= geometry::REMOVE_FADE)
        {
            self.ghost = None;
        }
        sync_region(cx.ui(), &state);
        cx.request_paint();
        if !more {
            super::send_icons(cx.ui(), &state);
        }
        more || self.ghost.is_some() || self.drag.is_some()
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let r = cx.bounds();
        let theme = *cx.theme();
        let dark = theme.dark();
        let now = cx.now();
        let st = self.state.clone();
        let s = st.borrow();
        let m: Metrics = s.metrics();
        let (l, refs) = s.frame(now);
        let o = r.origin();
        cx.glass(
            "Shelf",
            l.shelf.translate(o),
            Shape::rounded(m.radius()),
            "Glass/Regular",
            GlassProfile::Dock,
        );
        let stack_key = s.stack.as_ref().map(|x| x.0.clone());
        let menu_tile = s.menu.map(|x| x.0);
        for (slot, cell) in l.cells.iter().enumerate() {
            let Some(SlotRef::Tile(i)) = refs.get(slot).copied() else {
                continue;
            };
            let Some(tile) = s.tiles.get(i) else { continue };
            let icon = cell.icon.translate(o);
            if let Kind::Separator = tile.kind {
                cx.rec.layer(
                    "Separator",
                    Rect::new(
                        o.x + cell.rect.x + (cell.rect.width / 2.0).floor(),
                        o.y + cell.rect.y + m.separator_inset(),
                        1.0,
                        m.tile_height() - 2.0 * m.separator_inset(),
                    ),
                    Shape::RECT,
                    Style::fill(separator_color(dark)),
                );
                continue;
            }
            let pressed = self.pressed.is_some_and(|p| p.tile == i) && self.drag.is_none()
                || menu_tile == Some(i)
                || matches!(&self.file, Some(FileTarget::App(j, _) | FileTarget::Trash(j) | FileTarget::Into(j)) if *j == i)
                || (matches!(tile.kind, Kind::Trash)
                    && self.drag.as_ref().is_some_and(|d| d.over_trash));
            let open = stack_key.as_deref() == Some(&tile.kind.key());
            paint_kind(
                cx,
                &tile.kind,
                icon,
                Look {
                    opacity: cell.opacity,
                    dark,
                    shade: pressed,
                    open,
                },
                &s,
            );
            let app = tile.kind.app_id();
            let launcher = app.and_then(|a| s.launcher.get(a));
            // The dot, or the progress bar in its place (SH-DK-30).
            let dot_y = o.y + cell.rect.y + m.tile_height() - m.dot_above();
            if let Some(p) = launcher.and_then(|x| x.progress) {
                let (pw, ph) = (PROGRESS.0 * m.k(), PROGRESS.1 * m.k());
                let bar = Rect::new(
                    o.x + cell.rect.x + (cell.rect.width - pw) / 2.0,
                    dot_y - ph / 2.0,
                    pw,
                    ph,
                );
                cx.rec.layer(
                    "Progress",
                    bar,
                    Shape::rounded(ph / 2.0),
                    Style::fill(Color::TRANSPARENT)
                        .with_border(Border::inside(dot_color(dark), 1.0)),
                );
                cx.rec.layer(
                    "Progress fill",
                    Rect::new(bar.x, bar.y, bar.width * p, bar.height),
                    Shape::rounded(ph / 2.0),
                    Style::fill(dot_color(dark)),
                );
            } else if s.options.indicators && app.is_some_and(|a| s.running(a)) {
                let d = m.dot();
                cx.rec.layer(
                    "Running dot",
                    Rect::new(
                        o.x + cell.rect.x + (cell.rect.width - d) / 2.0,
                        dot_y - d / 2.0,
                        d,
                        d,
                    ),
                    Shape::Ellipse,
                    Style::fill(dot_color(dark)),
                );
            }
            // The badge (SH-DK-29): its right edge 3 inside the cell, about 5 above the icon.
            if let Some(text) = launcher.and_then(|x| x.badge.as_deref()) {
                let k = cell.icon.width / 60.0;
                let (bw, bh) = (geometry::badge_width(text) * k, BADGE_HEIGHT * k);
                let right = o.x + cell.rect.right() - BADGE_RIGHT * m.k();
                let badge = Rect::new(right - bw, icon.y - 2.0 * k, bw, bh);
                cx.rec.layer(
                    "Badge",
                    badge,
                    Shape::rounded(bh / 2.0),
                    Style::fill(Color::from_u32(0xf23f34ff)),
                );
                let style = theme.text(&typography::CALLOUT, Color::WHITE);
                let lt = crate::text::lay(
                    &mut self.labels,
                    1000 + slot,
                    text,
                    &style,
                    Align::Center,
                    bw,
                    Some(1),
                );
                let h = lt.size().height;
                cx.rec.text(
                    "Badge text",
                    Rect::new(badge.x, badge.y + (bh - h) / 2.0, bw, h),
                    lt,
                    None,
                    Color::WHITE,
                );
            }
        }
        // The removed icon fading in place.
        if let Some(g) = &self.ghost {
            let a = 1.0 - ((now - g.since) / geometry::REMOVE_FADE).clamp(0.0, 1.0) as f32;
            paint_kind(
                cx,
                &g.kind,
                g.rect.translate(o),
                Look {
                    opacity: a,
                    dark,
                    shade: false,
                    open: false,
                },
                &s,
            );
        }
        // The dragged icon at the pointer, with "Remove" above it, or the Trash's label.
        if let Some(d) = &self.drag {
            let kind = s
                .tiles
                .iter()
                .find(|t| t.kind.key() == d.key)
                .map(|t| t.kind.clone());
            if let Some(kind) = kind {
                let rect = Rect::new(
                    o.x + d.pointer.x - d.grab.x,
                    o.y + d.pointer.y - d.grab.y,
                    d.side,
                    d.side,
                );
                paint_kind(
                    cx,
                    &kind,
                    rect,
                    Look {
                        opacity: 1.0,
                        dark,
                        shade: false,
                        open: false,
                    },
                    &s,
                );
                if d.remove {
                    let tip = rect.y - m.label_gap(d.side);
                    paint_label(
                        cx,
                        &mut self.labels,
                        2000,
                        "Remove",
                        rect.x + d.side / 2.0,
                        tip,
                        1.0,
                    );
                }
            }
            if d.over_trash
                && let Some(i) = s.tiles.iter().position(|t| matches!(t.kind, Kind::Trash))
                && let Some(c) = DockState::cell_of(&l, &refs, i)
            {
                let tip = o.y + c.icon.y - m.label_gap(c.icon.width);
                paint_label(
                    cx,
                    &mut self.labels,
                    2001,
                    "Remove from Dock",
                    o.x + c.rect.x + c.rect.width / 2.0,
                    tip,
                    1.0,
                );
            }
        }
        // The hover label (not while its menu or stack is open, nor during a drag).
        if self.drag.is_none()
            && self.resize.is_none()
            && let Some(h) = self
                .hovered
                .filter(|h| Some(*h) != self.label_hidden && Some(*h) != menu_tile)
            && let Some(t) = s.tiles.get(h)
            && stack_key.as_deref() != Some(&t.kind.key())
            && let (Some(name), Some(c)) = (t.kind.label(), DockState::cell_of(&l, &refs, h))
        {
            let tip = o.y + c.icon.y - m.label_gap(c.icon.width);
            paint_label(
                cx,
                &mut self.labels,
                0,
                &name,
                o.x + c.rect.x + c.rect.width / 2.0,
                tip,
                1.0,
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        let local = |p: Point| Point::new(p.x - b.x, p.y - b.y);
        let now = cx.now();
        match event {
            Event::PointerMove { pos, .. } => {
                let p = local(*pos);
                if let Some(rz) = self.resize {
                    let canvas = Metrics::new(rz.canvas + (rz.y - p.y)).canvas;
                    let state = self.state.clone();
                    state.borrow_mut().options.canvas = canvas;
                    sync_region(cx.ui(), &state);
                    cx.request_paint();
                    return Handled::Yes;
                }
                if self.drag.is_some() {
                    self.update_drag(cx, p);
                    return Handled::Yes;
                }
                if let Some(press) = self.pressed
                    && (press.at.x - p.x).hypot(press.at.y - p.y) > DRAG_THRESHOLD
                {
                    self.pressed = None;
                    let kind = self
                        .state
                        .borrow()
                        .tiles
                        .get(press.tile)
                        .map(|t| t.kind.clone());
                    match kind {
                        Some(Kind::Separator) => {
                            let canvas = self.state.borrow().options.canvas;
                            self.resize = Some(Resize {
                                y: press.at.y,
                                canvas,
                            });
                        }
                        Some(k) if k.draggable() => self.start_drag(cx, press, p),
                        _ => {}
                    }
                    return Handled::Yes;
                }
                self.track(cx, Some(p));
                let state = self.state.clone();
                sync_region(cx.ui(), &state);
                Handled::No
            }
            Event::PointerLeave => {
                if self.drag.is_none() && self.resize.is_none() {
                    self.track(cx, None);
                }
                Handled::No
            }
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                let p = local(*pos);
                let (l, refs) = self.state.borrow().frame(now);
                if let Some(i) = Self::tile_at(&l, &refs, p) {
                    self.pressed = Some(Press {
                        tile: i,
                        at: p,
                        time: now,
                    });
                    let state = self.state.clone();
                    let separator = matches!(
                        state.borrow().tiles.get(i).map(|t| &t.kind),
                        Some(Kind::Separator)
                    );
                    if !separator {
                        let id = cx.id();
                        cx.ui().after(geometry::LONG_PRESS, move |ui| {
                            let held = ui
                                .with_widget::<DockView, _>(id, |v| {
                                    let held =
                                        v.pressed.is_some_and(|p| p.tile == i && p.time == now)
                                            && v.drag.is_none();
                                    if held {
                                        v.pressed = None;
                                        v.label_hidden = Some(i);
                                    }
                                    held
                                })
                                .unwrap_or(false);
                            if held {
                                super::menus::open(ui, &state, i);
                            }
                        });
                    }
                    cx.request_paint();
                }
                Handled::Yes
            }
            Event::PointerUp {
                button: Button::Left,
                ..
            } => {
                if let Some(rz) = self.resize.take() {
                    let canvas = self.state.borrow().options.canvas;
                    if canvas != rz.canvas {
                        // Kept on release: the key Settings writes; magnification never below it.
                        super::set_setting(
                            "dock.tileSize",
                            zbus::zvariant::OwnedValue::from(canvas as i32),
                        );
                        let magnified = self.state.borrow().options.magnified;
                        if magnified < canvas {
                            super::set_setting(
                                "dock.magnifiedSize",
                                zbus::zvariant::OwnedValue::from(canvas as i32),
                            );
                        }
                    }
                    return Handled::Yes;
                }
                if self.drag.is_some() {
                    self.end_drag(cx);
                    return Handled::Yes;
                }
                if let Some(p) = self.pressed.take() {
                    let state = self.state.clone();
                    let separator = matches!(
                        state.borrow().tiles.get(p.tile).map(|t| &t.kind),
                        Some(Kind::Separator)
                    );
                    if !separator {
                        super::activate(cx.ui(), &state, p.tile);
                    }
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerDown {
                pos,
                button: Button::Right,
                ..
            } => {
                let p = local(*pos);
                let (l, refs) = self.state.borrow().frame(now);
                if let Some(i) = Self::tile_at(&l, &refs, p) {
                    self.pressed = None;
                    self.label_hidden = Some(i);
                    let state = self.state.clone();
                    super::menus::open(cx.ui(), &state, i);
                }
                Handled::Yes
            }
            Event::DragOver { pos } => {
                self.file_over(cx, *pos);
                Handled::Yes
            }
            Event::DragLeave => {
                self.file_leave(cx);
                Handled::Yes
            }
            Event::Drop { pos, items } => {
                self.file_drop(cx, *pos, items);
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

/// The cell of the file manager in a frame (tests).
#[cfg(test)]
fn _fm_cell(s: &DockState, l: &Layout, refs: &[SlotRef]) -> Option<geometry::Cell> {
    s.tiles
        .iter()
        .position(|t| matches!(t.kind, Kind::App { fixed: true, .. }))
        .and_then(|i| DockState::cell_of(l, refs, i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::DesktopEntry;
    use crate::dock::model::{self, Inputs};

    fn e(id: &str) -> DesktopEntry {
        DesktopEntry {
            id: id.into(),
            name: id.into(),
            ..DesktopEntry::default()
        }
    }

    fn state() -> DockState {
        let mut s = DockState {
            inputs: Inputs {
                entries: vec![e(FILE_MANAGER), e("a"), e("b"), e("c")],
                pinned: vec![FILE_MANAGER.into(), "a".into(), "b".into(), "c".into()],
                folders: vec![PathBuf::from("/f1"), PathBuf::from("/f2")],
                ..Inputs::default()
            },
            size: Size::new(1280.0, 404.0),
            ..DockState::default()
        };
        s.hide = (1.0, 1.0, 0.0, 0.0);
        s.recompose(0.0, false);
        s
    }

    #[test]
    fn regions_bound_apps_and_folders() {
        let s = state();
        let (before, apps) = DockView::region(&s, false);
        assert_eq!(before.as_deref(), Some("app:dev.frascella.Mustache"));
        assert_eq!(apps, [1, 2, 3]);
        let (before, folders) = DockView::region(&s, true);
        assert_eq!(before.as_deref(), Some("sep"));
        assert_eq!(folders, [5, 6]);
    }

    #[test]
    fn the_gap_lands_between_cells_and_never_before_the_file_manager() {
        let s = state();
        let (l, refs) = s.frame(1.0);
        // Far left: right after the file manager.
        let after = DockView::landing(&s, &l, &refs, "app:c", false, 0.0);
        assert_eq!(after.as_deref(), Some("app:dev.frascella.Mustache"));
        let c = DockState::cell_of(&l, &refs, 2)
            .map(|c| c.rect.right())
            .unwrap_or(0.0);
        let after = DockView::landing(&s, &l, &refs, "app:c", false, c + 1.0);
        assert_eq!(after.as_deref(), Some("app:b"));
        assert!(_fm_cell(&s, &l, &refs).is_some());
    }

    #[test]
    fn a_moving_gap_closes_where_it_was_and_opens_where_it_goes() {
        let mut s = state();
        DockView::move_gap(&mut s, Some("app:a".into()), 0.0, false);
        DockView::move_gap(&mut s, Some("app:b".into()), 0.1, false);
        assert_eq!(s.gaps.len(), 2);
        assert!(!s.gaps[0].opening && s.gaps[1].opening);
        // The slot width moves from one to the other: the total stays one cell after the slide.
        let total = |s: &DockState, t: f64| s.gaps.iter().map(|g| g.presence(t)).sum::<f32>();
        assert!((total(&s, 0.1 + geometry::GAP_TIME) - 1.0).abs() < 0.3);
        DockView::close_gaps(&mut s, 1.0, true);
        assert!(s.gaps.is_empty());
        let _ = model::APPS_ID;
    }
}

#[cfg(test)]
mod drag_tests {
    use super::*;
    use crate::apps::DesktopEntry;
    use crate::dock::model::Inputs;

    #[test]
    fn dragging_moves_the_gap_and_keeps_every_other_tile() {
        let e = |id: &str| DesktopEntry {
            id: id.into(),
            name: id.into(),
            ..DesktopEntry::default()
        };
        let mut s = DockState {
            inputs: Inputs {
                entries: vec![e("a"), e("b"), e("c"), e("d"), e("e")],
                pinned: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
                ..Inputs::default()
            },
            size: Size::new(1280.0, 404.0),
            ..DockState::default()
        };
        s.hide = (1.0, 1.0, 0.0, 0.0);
        s.recompose(0.0, false);
        // Drag "b" (tile 1): its place closes, the gap stands there.
        s.dragging = Some("app:b".into());
        s.gaps.push(Gap {
            after: Some("app:a".into()),
            opening: true,
            since: -1.0,
            from: 1.0,
            folder: false,
        });
        let (l, refs) = s.frame(0.0);
        assert_eq!(refs.len(), s.tiles.len());
        let d = DockState::cell_of(&l, &refs, 3)
            .map(|c| c.rect.x + c.rect.width)
            .unwrap_or(0.0);
        let after = DockView::landing(&s, &l, &refs, "app:b", false, d + 1.0);
        assert_eq!(after.as_deref(), Some("app:d"));
        DockView::move_gap(&mut s, after, 0.0, false);
        let (l, refs) = s.frame(1.0);
        s.gaps.retain(|g| !g.gone(1.0));
        assert_eq!(s.gaps.len(), 1);
        // Every tile but the dragged one is laid out.
        for i in [0, 2, 3, 4] {
            assert!(DockState::cell_of(&l, &refs, i).is_some(), "tile {i}");
        }
    }
}
