//! Screenshots (docs/spec/screenshot-ui.md): Ossido captures, the shell does the rest.
//!
//! - **Thumbnail:** each capture Ossido announces (`screenshot` broadcast) shows framed at the
//!   bottom right, fades in (380 ms), stays 5.4 s (while the pointer is on it too), slides off
//!   to the right (250 ms), and then the file goes to its place (the Desktop by default) with
//!   the shutter sound. A click opens it at once; its menu saves elsewhere, shows it, deletes
//!   it. Clipboard captures go to the clipboard with no thumbnail.
//! - **Selection** (`screenshot.selection`, ⇧⌘4): a crosshair over the screen; a drag captures
//!   the region, Space switches to window mode (Ossido tints the window under the pointer),
//!   Esc cancels.
//! - **The bar** (`screenshot.toolbar`, ⇧⌘5): close, the three capture modes, the Options menu
//!   (save to, timer, thumbnail) and Capture. Recording is not available yet (#153): its
//!   buttons are shown dimmed and do nothing.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use skia_safe::Image;
use vetro_lib::amalfi::foundation::Label;
use vetro_lib::amalfi::tokens::typography;
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
use vetro_lib::geom::{Insets, Point, Rect, Size};
use vetro_lib::shape::Shape;
use vetro_lib::style::{Border, Style};
use vetro_lib::text::Align;

use crate::popmenu::{self, Item};

/// Measures.
pub mod metrics {
    /// The thumbnail's image width (the outer frame is 8 more).
    pub const THUMB: f32 = 256.0;
    /// Its frame.
    pub const FRAME: f32 = 4.0;
    /// Its margin from the screen's right and bottom edges.
    pub const EDGE: f32 = 12.0;
    /// Room for its shadow in the surface.
    pub const MARGIN: f32 = 24.0;
    /// The bar.
    pub const BAR: (f32, f32) = (545.0, 52.0);
    /// The bar's distance above the Dock zone.
    pub const BAR_GAP: f32 = 4.0;
    /// The Dock zone's height.
    pub const DOCK: f32 = 84.0;
}

const FADE_IN: f64 = 0.38;
const STAY: f64 = 5.4;
const SLIDE_OUT: f64 = 0.25;

/// Where captures go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaveTo {
    /// The Desktop.
    #[default]
    Desktop,
    /// Documents.
    Documents,
    /// The clipboard.
    Clipboard,
}

/// Capture modes of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The whole screen.
    Screen,
    /// A window.
    Window,
    /// A selection.
    #[default]
    Selection,
}

/// A thumbnail on screen.
#[derive(Clone)]
struct Thumb {
    path: PathBuf,
    image: Option<Image>,
    since: f64,
    leaving: Option<f64>,
    hovered: bool,
    /// The timer stops while the menu is open.
    paused: f64,
    menu_since: Option<f64>,
    /// Saved, deleted or opened already: nothing to do when it leaves.
    handled: bool,
}

/// The screenshot UI's state.
#[derive(Default)]
pub struct ShotState {
    /// The options.
    pub save_to: SaveTo,
    /// Seconds before capturing (0, 5, 10).
    pub timer: u32,
    /// Show the floating thumbnail.
    pub thumbnail: bool,
    /// The bar's mode.
    pub mode: Mode,
    thumb: Option<Thumb>,
    thumb_surface: Option<(WindowId, ViewId)>,
    /// The selection overlay and its mode (window when true).
    select: Option<(WindowId, ViewId)>,
    clipboard_selection: bool,
    window_mode: bool,
    drag: Option<(Point, Point)>,
    pointer: Point,
    /// The bar.
    bar: Option<(WindowId, ViewId)>,
    /// The bar's selection (screen coordinates), kept between uses.
    last_selection: Option<Rect>,
    output: Size,
    initialized: bool,
}

/// Shared.
pub type Shared = Rc<RefCell<ShotState>>;

fn init(ui: &Ui, state: &Shared) {
    let mut s = state.borrow_mut();
    s.output = ui
        .outputs()
        .first()
        .map_or(Size::new(1280.0, 800.0), |o| o.size);
    if !s.initialized {
        s.initialized = true;
        s.thumbnail = true;
    }
}

/// The capture's file name: "Screenshot 2026-10-14 at 04.12.33.png".
pub fn file_name() -> String {
    let t = crate::time::now();
    format!(
        "Screenshot {:04}-{:02}-{:02} at {:02}.{:02}.{:02}.png",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

fn destination(save_to: SaveTo) -> Option<PathBuf> {
    let dir = match save_to {
        SaveTo::Desktop => crate::dock::user_dir("XDG_DESKTOP_DIR", "Desktop"),
        SaveTo::Documents => crate::dock::user_dir("XDG_DOCUMENTS_DIR", "Documents"),
        SaveTo::Clipboard => None,
    }
    .or_else(|| std::env::var_os("HOME").map(PathBuf::from))?;
    let mut p = dir.join(file_name());
    let mut n = 2;
    while p.exists() {
        p = dir.join(file_name().replace(".png", &format!(" ({n}).png")));
        n += 1;
    }
    Some(p)
}

fn play_shutter() {
    vetro_lib::platform::spawn(async {
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.sound().await
        {
            let _ = p.play_sound("screen-capture").await;
        }
    });
}

fn copy_image(path: &Path) {
    let path = path.to_string_lossy().into_owned();
    vetro_lib::platform::spawn(async move {
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.clipboard().await
        {
            let _ = p.copy_image(&path).await;
        }
    });
}

/// Moves the capture to `save_to` (a copy across file systems); returns where it went.
fn place(path: &Path, save_to: SaveTo) -> Option<PathBuf> {
    if save_to == SaveTo::Clipboard {
        copy_image(path);
        return None;
    }
    let to = destination(save_to)?;
    if std::fs::rename(path, &to).is_err() {
        std::fs::copy(path, &to).ok()?;
        let _ = std::fs::remove_file(path);
    }
    Some(to)
}

/// Ossido announced a capture.
pub fn captured(ui: &mut Ui, state: &Shared, json: &str) {
    init(ui, state);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return;
    };
    let Some(path) = v.get("path").and_then(|p| p.as_str()).map(PathBuf::from) else {
        return;
    };
    play_shutter();
    let clipboard = v.get("clipboard").and_then(serde_json::Value::as_bool) == Some(true);
    let (thumbnail, save_to) = {
        let s = state.borrow();
        (s.thumbnail, s.save_to)
    };
    if clipboard || save_to == SaveTo::Clipboard {
        copy_image(&path);
        return;
    }
    if !thumbnail {
        let _ = place(&path, save_to);
        return;
    }
    // A thumbnail on screen goes to its place at once.
    if let Some(old) = state.borrow_mut().thumb.take()
        && !old.handled
    {
        let _ = place(&old.path, save_to);
    }
    let image = std::fs::read(&path)
        .ok()
        .and_then(|b| Image::from_encoded(skia_safe::Data::new_copy(&b)));
    let now = ui.now;
    state.borrow_mut().thumb = Some(Thumb {
        path,
        image,
        since: now,
        leaving: None,
        hovered: false,
        paused: 0.0,
        menu_since: None,
        handled: false,
    });
    open_thumb(ui, state);
}

fn thumb_size(t: &Thumb) -> Size {
    let (w, h) = t.image.as_ref().map_or((16.0, 9.0), |i| {
        (i.width() as f32, i.height().max(1) as f32)
    });
    let iw = metrics::THUMB;
    let ih = (iw * h / w.max(1.0)).round().clamp(40.0, 256.0);
    Size::new(iw + 2.0 * metrics::FRAME, ih + 2.0 * metrics::FRAME)
}

fn open_thumb(ui: &mut Ui, state: &Shared) {
    let Some(t) = state.borrow().thumb.clone() else {
        return;
    };
    let card = thumb_size(&t);
    let m = metrics::MARGIN;
    let size = Size::new(
        card.width + m + metrics::EDGE,
        card.height + m + metrics::EDGE,
    );
    let existing = state.borrow().thumb_surface;
    let (w, v) = match existing {
        Some((w, v)) => {
            ui.request(vetro_lib::core::Request::Resize(w, size));
            (w, v)
        }
        None => {
            let output = ui.outputs().first().map(|o| o.name.clone());
            let w = ui.create_window(
                WindowKind::Shell {
                    layer: Layer::Overlay,
                    anchor: Edges::BOTTOM | Edges::RIGHT,
                    exclusive: -1,
                    margin: [0; 4],
                    keyboard: KeyboardMode::None,
                    namespace: "screenshot-thumbnail".into(),
                    output,
                },
                size,
            );
            if let Some(win) = ui.window_mut(w) {
                win.opaque = false;
            }
            let v = ui.create(ThumbView {
                state: state.clone(),
            });
            let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
            let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
            ui.set_content(w, v);
            ui.show_window(w);
            state.borrow_mut().thumb_surface = Some((w, v));
            (w, v)
        }
    };
    ui.set_input_region(w, Some(vec![Rect::new(m, m, card.width, card.height)]));
    ui.request_paint(v);
    ui.start_animating(v);
}

/// The thumbnail.
struct ThumbView {
    state: Shared,
}

impl ThumbView {
    fn card(&self, b: Rect, t: &Thumb, now: f64) -> Rect {
        let s = thumb_size(t);
        let dx = t.leaving.map_or(0.0, |l| {
            let k = ((now - l) / SLIDE_OUT).clamp(0.0, 1.0) as f32;
            // It speeds up, then holds a near-constant speed.
            (k * k * 0.4 + k * 0.6) * (s.width + metrics::EDGE + metrics::MARGIN)
        });
        Rect::new(
            b.x + metrics::MARGIN + dx,
            b.y + metrics::MARGIN,
            s.width,
            s.height,
        )
    }

    fn menu(&self, cx: &mut EventCx<'_>, at: Point) {
        let Some(w) = cx.window() else { return };
        let now = cx.now();
        if let Some(t) = self.state.borrow_mut().thumb.as_mut() {
            t.menu_since = Some(now);
        }
        let st = self.state.clone();
        let save = |label: &str, to: SaveTo| {
            let st = st.clone();
            Item::action(label, move |ui| {
                let _ = finish(ui, &st, Some(to));
            })
        };
        let st2 = self.state.clone();
        let st3 = self.state.clone();
        let st4 = self.state.clone();
        let items = vec![
            save("Save to Desktop", SaveTo::Desktop),
            save("Save to Documents", SaveTo::Documents),
            save("Save to Clipboard", SaveTo::Clipboard),
            Item::Separator,
            Item::action("Open", move |ui| open_now(ui, &st2)),
            Item::action("Show in Mustache", move |ui| {
                let to = st3.borrow().save_to;
                if let Some(p) = finish(ui, &st3, Some(to))
                    && let Some(d) = p.parent()
                {
                    crate::dock::open_path(d);
                }
            }),
            Item::action("Delete", move |ui| {
                let t = st4.borrow_mut().thumb.as_mut().map(|t| {
                    t.handled = true;
                    t.path.clone()
                });
                if let Some(p) = t {
                    let _ = std::fs::remove_file(p);
                }
                leave(ui, &st4);
            }),
            Item::Separator,
            {
                let st = self.state.clone();
                Item::action("Close", move |ui| {
                    let to = st.borrow().save_to;
                    let _ = finish(ui, &st, Some(to));
                })
            },
        ];
        let st = self.state.clone();
        popmenu::show(
            cx.ui(),
            w,
            at,
            items,
            Some(Rc::new(move |ui: &mut Ui| {
                // The timer goes on from where it stopped.
                let now = ui.now;
                if let Some(t) = st.borrow_mut().thumb.as_mut()
                    && let Some(m) = t.menu_since.take()
                {
                    t.paused += now - m;
                }
            })),
        );
    }
}

/// Puts the capture in its place (or `to`), marks it done and sends the thumbnail away.
fn finish(ui: &mut Ui, state: &Shared, to: Option<SaveTo>) -> Option<PathBuf> {
    let target = {
        let mut s = state.borrow_mut();
        let save_to = to.unwrap_or(s.save_to);
        s.thumb.as_mut().filter(|t| !t.handled).map(|t| {
            t.handled = true;
            (t.path.clone(), save_to)
        })
    };
    let placed = target.and_then(|(p, to)| place(&p, to));
    leave(ui, state);
    placed
}

/// Opens the capture with its default app now.
fn open_now(ui: &mut Ui, state: &Shared) {
    let to = state.borrow().save_to;
    let to = if to == SaveTo::Clipboard {
        SaveTo::Desktop
    } else {
        to
    };
    if let Some(p) = finish(ui, state, Some(to)) {
        crate::dock::open_path(&p);
    }
}

fn leave(ui: &mut Ui, state: &Shared) {
    let now = ui.now;
    if let Some(t) = state.borrow_mut().thumb.as_mut()
        && t.leaving.is_none()
    {
        t.leaving = Some(now);
    }
    if let Some((_, v)) = state.borrow().thumb_surface {
        ui.start_animating(v);
    }
}

impl vetro_lib::core::Widget for ThumbView {
    fn kind(&self) -> &'static str {
        "ScreenshotThumbnail"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(300.0, 200.0))
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        cx.request_paint();
        let (expired, gone) = {
            let s = self.state.borrow();
            match &s.thumb {
                None => (false, true),
                Some(t) => {
                    let paused = t.paused + t.menu_since.map_or(0.0, |m| now - m);
                    let expired = t.leaving.is_none()
                        && !t.hovered
                        && t.menu_since.is_none()
                        && now - t.since - paused >= STAY;
                    (expired, t.leaving.is_some_and(|l| now - l >= SLIDE_OUT))
                }
            }
        };
        if expired {
            let state = self.state.clone();
            let _ = finish(cx.ui(), &state, None);
        }
        if gone {
            let w = {
                let mut s = self.state.borrow_mut();
                s.thumb = None;
                s.thumb_surface.take()
            };
            if let Some((w, _)) = w {
                cx.ui().close_window(w);
            }
            return false;
        }
        true
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let now = cx.now();
        let dark = cx.theme().dark();
        let b = cx.bounds();
        let s = self.state.borrow();
        let Some(t) = &s.thumb else { return };
        let r = self.card(b, t, now);
        let k = ((now - t.since) / FADE_IN).clamp(0.0, 1.0) as f32;
        let alpha = 1.0 - (1.0 - k) * (1.0 - k);
        cx.rec.push_opacity(alpha);
        let mut frame = Style::fill(if dark { Color::BLACK } else { Color::WHITE });
        if let Some(sh) = cx.theme().shadow(2) {
            frame.shadows.push(sh);
        }
        cx.rec.layer("Frame", r, Shape::rounded(4.0), frame);
        let img_r = r.inset(Insets::all(metrics::FRAME));
        if let Some(img) = &t.image {
            cx.rec.image("Image", img_r, img.clone(), 1.0);
        }
        if dark {
            cx.rec.layer(
                "Inner line",
                img_r.inset(Insets::all(-1.0)),
                Shape::rounded(1.0),
                Style::fill(Color::TRANSPARENT)
                    .with_border(Border::inside(Color::from_u32(0x949494ff), 1.0)),
            );
        }
        cx.rec.pop();
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        match event {
            Event::PointerEnter | Event::PointerMove { .. } => {
                if let Some(t) = self.state.borrow_mut().thumb.as_mut() {
                    t.hovered = true;
                }
                Handled::Yes
            }
            Event::PointerLeave => {
                let now = cx.now();
                if let Some(t) = self.state.borrow_mut().thumb.as_mut() {
                    // It stays another 5.4 s after the pointer leaves.
                    t.hovered = false;
                    t.since = now;
                    t.paused = 0.0;
                }
                Handled::Yes
            }
            Event::PointerUp {
                button: Button::Left,
                ..
            } => {
                let state = self.state.clone();
                open_now(cx.ui(), &state);
                Handled::Yes
            }
            Event::PointerDown {
                pos,
                button: Button::Right,
                ..
            } => {
                self.menu(cx, *pos);
                Handled::Yes
            }
            Event::PointerDown { .. } => Handled::Yes,
            _ => Handled::No,
        }
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Image;
        a.label = Some("Screenshot".into());
    }
}

// ----- Selection -----------------------------------------------------------------------------

/// Starts a selection (⇧⌘4); `window` starts in window mode.
pub fn select(ui: &mut Ui, state: &Shared, clipboard: bool, window: bool) {
    init(ui, state);
    if state.borrow().select.is_some() {
        return;
    }
    let out = state.borrow().output;
    let output = ui.outputs().first().map(|o| o.name.clone());
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Overlay,
            anchor: Edges::TOP | Edges::BOTTOM | Edges::LEFT | Edges::RIGHT,
            exclusive: -1,
            margin: [0; 4],
            keyboard: KeyboardMode::Exclusive,
            namespace: "screenshot-selection".into(),
            output,
        },
        out,
    );
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
    }
    let v = ui.create(SelectView {
        state: state.clone(),
        label: Label::default(),
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, v);
    ui.show_window(w);
    ui.set_focus(v, false);
    let mut s = state.borrow_mut();
    s.select = Some((w, v));
    s.clipboard_selection = clipboard;
    s.window_mode = window;
    s.drag = None;
    drop(s);
    if window {
        veil(ui, true);
    }
}

fn veil(ui: &mut Ui, on: bool) {
    let params = serde_json::json!({ "enabled": on }).to_string();
    ui.shell(ShellRequest::ExecuteWith(
        "screenshot.window_veil".into(),
        params,
    ));
}

fn end_selection(ui: &mut Ui, state: &Shared) {
    let (w, window_mode) = {
        let mut s = state.borrow_mut();
        (s.select.take(), s.window_mode)
    };
    if window_mode {
        veil(ui, false);
    }
    if let Some((w, _)) = w {
        ui.close_window(w);
    }
}

/// Captures after the overlays are gone (and after the timer).
fn capture_later(ui: &mut Ui, delay: f64, id: &'static str, params: serde_json::Value) {
    let text = params.to_string();
    ui.after(delay.max(0.12), move |ui| {
        ui.shell(ShellRequest::ExecuteWith(id.into(), text.clone()));
    });
}

/// The selection overlay.
struct SelectView {
    state: Shared,
    label: Label,
}

impl vetro_lib::core::Widget for SelectView {
    fn kind(&self) -> &'static str {
        "ScreenshotSelection"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(self.state.borrow().output)
    }

    fn focus(&self) -> FocusKind {
        FocusKind::Always
    }

    fn attached(&mut self, cx: &mut EventCx<'_>) {
        cx.set_cursor(vetro_lib::core::Cursor::Crosshair);
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let theme = *cx.theme();
        let b = cx.bounds();
        let s = self.state.borrow();
        if s.window_mode {
            return;
        }
        if let Some((a, c)) = s.drag {
            let r = Rect::new(
                a.x.min(c.x),
                a.y.min(c.y),
                (a.x - c.x).abs(),
                (a.y - c.y).abs(),
            )
            .translate(b.origin());
            cx.rec.layer(
                "Selection",
                r,
                Shape::rounded(0.0),
                Style::fill(Color::rgba(128, 128, 128, 60))
                    .with_border(Border::inside(Color::rgba(255, 255, 255, 200), 1.0)),
            );
            let text = format!("{} × {}", r.width.round(), r.height.round());
            let style = theme.text(&typography::CAPTION, Color::BLACK);
            let l = self
                .label
                .layout(&text, &style, Align::Start, 120.0, Some(1));
            cx.rec.text(
                "Size",
                Rect::new(c.x + b.x + 12.0, c.y + b.y + 8.0, 120.0, 16.0),
                l,
                None,
                Color::BLACK,
            );
        } else {
            // The point's coordinates next to the crosshair.
            let p = s.pointer;
            let text = format!("{}\n{}", p.x.round(), p.y.round());
            let style = theme.text(&typography::CAPTION, Color::BLACK);
            let l = self
                .label
                .layout(&text, &style, Align::Start, 60.0, Some(2));
            cx.rec.text(
                "Coordinates",
                Rect::new(p.x + b.x + 12.0, p.y + b.y + 8.0, 60.0, 32.0),
                l,
                None,
                Color::BLACK,
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        let state = self.state.clone();
        match event {
            Event::Key(k) if k.pressed && k.key == Key::Escape => {
                end_selection(cx.ui(), &state);
                Handled::Yes
            }
            Event::Key(k) if k.pressed && k.key == Key::Space => {
                let on = {
                    let mut s = state.borrow_mut();
                    s.window_mode = !s.window_mode;
                    s.drag = None;
                    s.window_mode
                };
                veil(cx.ui(), on);
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerMove { pos, .. } => {
                let p = Point::new(pos.x - b.x, pos.y - b.y);
                let mut s = state.borrow_mut();
                s.pointer = p;
                if let Some((a, _)) = s.drag {
                    s.drag = Some((a, p));
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                let p = Point::new(pos.x - b.x, pos.y - b.y);
                if !state.borrow().window_mode {
                    state.borrow_mut().drag = Some((p, p));
                }
                Handled::Yes
            }
            Event::PointerUp {
                pos,
                button: Button::Left,
                ..
            } => {
                let p = Point::new(pos.x - b.x, pos.y - b.y);
                let (window_mode, drag, clipboard) = {
                    let s = state.borrow();
                    (s.window_mode, s.drag, s.clipboard_selection)
                };
                end_selection(cx.ui(), &state);
                if window_mode {
                    capture_later(
                        cx.ui(),
                        0.0,
                        "screenshot.window_at",
                        serde_json::json!({ "x": p.x, "y": p.y, "or_screen": true, "clipboard": clipboard }),
                    );
                } else if let Some((a, c)) = drag {
                    let r = Rect::new(
                        a.x.min(c.x),
                        a.y.min(c.y),
                        (a.x - c.x).abs(),
                        (a.y - c.y).abs(),
                    );
                    if r.width >= 2.0 && r.height >= 2.0 {
                        capture_later(
                            cx.ui(),
                            0.0,
                            "screenshot.region",
                            serde_json::json!({ "x": r.x, "y": r.y, "width": r.width, "height": r.height, "clipboard": clipboard }),
                        );
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
        a.label = Some("Screenshot selection".into());
    }
}

// ----- The bar -------------------------------------------------------------------------------

/// The bar's buttons, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Close,
    Capture(Mode),
    Record(Mode),
    Options,
    Action,
}

/// The buttons' frames in the bar (screenshot-ui.md "⇧⌘5").
fn controls() -> Vec<(Control, Rect)> {
    let mut v = vec![(Control::Close, Rect::new(8.0, 1.0, 18.0, 50.0))];
    for (i, m) in [Mode::Screen, Mode::Window, Mode::Selection]
        .into_iter()
        .enumerate()
    {
        v.push((
            Control::Capture(m),
            Rect::new(42.0 + i as f32 * 48.0, 1.0, 44.0, 50.0),
        ));
    }
    for (i, m) in [Mode::Screen, Mode::Window, Mode::Selection]
        .into_iter()
        .enumerate()
    {
        v.push((
            Control::Record(m),
            Rect::new(200.0 + i as f32 * 57.0, 1.0, 50.0, 50.0),
        ));
    }
    v.push((Control::Options, Rect::new(383.0, 1.0, 76.0, 50.0)));
    v.push((Control::Action, Rect::new(463.0, 1.0, 80.0, 50.0)));
    v
}

/// Opens or closes the bar (⇧⌘5).
pub fn toggle_bar(ui: &mut Ui, state: &Shared) {
    init(ui, state);
    let open = state.borrow_mut().bar.take();
    if let Some((w, _)) = open {
        ui.close_window(w);
        return;
    }
    let out = state.borrow().output;
    let output = ui.outputs().first().map(|o| o.name.clone());
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Overlay,
            anchor: Edges::TOP | Edges::BOTTOM | Edges::LEFT | Edges::RIGHT,
            exclusive: -1,
            margin: [0; 4],
            keyboard: KeyboardMode::Exclusive,
            namespace: "screenshot-bar".into(),
            output,
        },
        out,
    );
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
    }
    let v = ui.create(BarView {
        state: state.clone(),
        label: Label::default(),
        moving: None,
        sel_drag: None,
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, v);
    ui.show_window(w);
    ui.set_focus(v, false);
    {
        let mut s = state.borrow_mut();
        s.bar = Some((w, v));
        if s.last_selection.is_none() {
            // The first time, half the screen, centred.
            s.last_selection = Some(Rect::new(
                out.width / 4.0,
                out.height / 4.0,
                out.width / 2.0,
                out.height / 2.0,
            ));
        }
    }
    if state.borrow().mode == Mode::Window {
        veil(ui, true);
    }
}

fn close_bar(ui: &mut Ui, state: &Shared) {
    let (w, mode) = {
        let mut s = state.borrow_mut();
        (s.bar.take(), s.mode)
    };
    if mode == Mode::Window {
        veil(ui, false);
    }
    if let Some((w, _)) = w {
        ui.close_window(w);
    }
}

/// Captures as the bar is set, after its timer.
fn bar_capture(ui: &mut Ui, state: &Shared, at: Point) {
    let (mode, timer, sel, clipboard) = {
        let s = state.borrow();
        (
            s.mode,
            s.timer,
            s.last_selection,
            s.save_to == SaveTo::Clipboard,
        )
    };
    close_bar(ui, state);
    let delay = f64::from(timer);
    match mode {
        Mode::Screen => capture_later(
            ui,
            delay,
            "screenshot.screen",
            serde_json::json!({ "clipboard": clipboard }),
        ),
        Mode::Window => capture_later(
            ui,
            delay,
            "screenshot.window_at",
            serde_json::json!({ "x": at.x, "y": at.y, "or_screen": true, "clipboard": clipboard }),
        ),
        Mode::Selection => {
            if let Some(r) = sel {
                capture_later(
                    ui,
                    delay,
                    "screenshot.region",
                    serde_json::json!({ "x": r.x, "y": r.y, "width": r.width, "height": r.height, "clipboard": clipboard }),
                );
            }
        }
    }
}

/// The bar, over a transparent screen-wide surface (the selection shows on it).
struct BarView {
    state: Shared,
    label: Label,
    /// The bar being dragged by its background: (grab offset).
    moving: Option<Point>,
    /// The selection being moved or drawn: (start pointer, rect at start, new rect mode).
    sel_drag: Option<(Point, Rect, bool)>,
}

thread_local! {
    static BAR_AT: std::cell::Cell<Option<Point>> = const { std::cell::Cell::new(None) };
}

impl BarView {
    fn bar_rect(&self, b: Rect) -> Rect {
        let (w, h) = metrics::BAR;
        let at = BAR_AT.with(std::cell::Cell::get).unwrap_or(Point::new(
            ((b.width - w) / 2.0).round(),
            b.height - metrics::DOCK - metrics::BAR_GAP - h,
        ));
        Rect::new(b.x + at.x, b.y + at.y, w, h)
    }

    fn options_menu(&self, cx: &mut EventCx<'_>, at: Point) {
        let Some(w) = cx.window() else { return };
        let s = self.state.borrow();
        let header = |label: &str| Item::Action {
            label: label.into(),
            action: None,
            checked: false,
        };
        let choice = |label: &str, checked: bool, f: Rc<dyn Fn(&mut ShotState)>| {
            let st = self.state.clone();
            Item::Action {
                label: label.into(),
                action: Some(Rc::new(move |_ui: &mut Ui| f(&mut st.borrow_mut()))),
                checked,
            }
        };
        let items = vec![
            header("Save to"),
            choice(
                "Desktop",
                s.save_to == SaveTo::Desktop,
                Rc::new(|s| s.save_to = SaveTo::Desktop),
            ),
            choice(
                "Documents",
                s.save_to == SaveTo::Documents,
                Rc::new(|s| s.save_to = SaveTo::Documents),
            ),
            choice(
                "Clipboard",
                s.save_to == SaveTo::Clipboard,
                Rc::new(|s| s.save_to = SaveTo::Clipboard),
            ),
            Item::Separator,
            header("Timer"),
            choice("None", s.timer == 0, Rc::new(|s| s.timer = 0)),
            choice("5 Seconds", s.timer == 5, Rc::new(|s| s.timer = 5)),
            choice("10 Seconds", s.timer == 10, Rc::new(|s| s.timer = 10)),
            Item::Separator,
            header("Options"),
            choice(
                "Show Floating Thumbnail",
                s.thumbnail,
                Rc::new(|s| s.thumbnail = !s.thumbnail),
            ),
        ];
        drop(s);
        let size = popmenu::size(cx.ui(), &items);
        popmenu::show(
            cx.ui(),
            w,
            Point::new(at.x, at.y - size.height - 6.0),
            items,
            None,
        );
    }
}

impl vetro_lib::core::Widget for BarView {
    fn kind(&self) -> &'static str {
        "ScreenshotBar"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(self.state.borrow().output)
    }

    fn focus(&self) -> FocusKind {
        FocusKind::Always
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let theme = *cx.theme();
        let dark = theme.dark();
        let b = cx.bounds();
        let s = self.state.borrow();
        if s.mode == Mode::Selection
            && let Some(sel) = s.last_selection
        {
            // A 35 % dim outside the selection, a white border and eight handles.
            let r = sel.translate(b.origin());
            let dim = Style::fill(Color::rgba(0, 0, 0, 89));
            for d in [
                Rect::new(b.x, b.y, b.width, r.y - b.y),
                Rect::new(b.x, r.bottom(), b.width, b.bottom() - r.bottom()),
                Rect::new(b.x, r.y, r.x - b.x, r.height),
                Rect::new(r.right(), r.y, b.right() - r.right(), r.height),
            ] {
                cx.rec.layer("Dim", d, Shape::rounded(0.0), dim.clone());
            }
            cx.rec.layer(
                "Selection",
                r,
                Shape::rounded(0.0),
                Style::fill(Color::TRANSPARENT).with_border(Border::inside(Color::WHITE, 1.0)),
            );
            for (hx, hy) in [
                (0.0, 0.0),
                (0.5, 0.0),
                (1.0, 0.0),
                (0.0, 0.5),
                (1.0, 0.5),
                (0.0, 1.0),
                (0.5, 1.0),
                (1.0, 1.0),
            ] {
                let c = Point::new(r.x + r.width * hx, r.y + r.height * hy);
                cx.rec.layer(
                    "Handle",
                    Rect::new(c.x - 4.5, c.y - 4.5, 9.0, 9.0),
                    Shape::Ellipse,
                    Style::fill(Color::WHITE),
                );
            }
        }
        let bar = self.bar_rect(b);
        cx.glass(
            "Bar",
            bar,
            Shape::rounded(14.0),
            "Glass/Regular",
            GlassProfile::Panel,
        );
        let (glyph, sep, pill) = if dark {
            (
                Color::from_u32(0xc7c8cdff),
                Color::from_u32(0x8f9095ff),
                Color::from_u32(0x8f9095ff),
            )
        } else {
            (
                Color::from_u32(0x4f617bff),
                Color::from_u32(0xa1b3cdff),
                Color::from_u32(0xa1b3cdff),
            )
        };
        for x in [192.0, 372.0] {
            cx.rec.layer(
                "Separator",
                Rect::new(bar.x + x, bar.y + 12.0, 1.0, 28.0),
                Shape::rounded(0.0),
                Style::fill(sep),
            );
        }
        for (c, r) in controls() {
            let r = r.translate(bar.origin());
            match c {
                Control::Close => {
                    let d = Rect::new(r.x, r.y + 16.0, 18.0, 18.0);
                    cx.rec.layer("Close", d, Shape::Ellipse, Style::fill(sep));
                    cx.rec.glyph(
                        "Close glyph",
                        d.inset(Insets::all(4.0)),
                        "xmark",
                        600,
                        glyph,
                    );
                }
                Control::Capture(m) | Control::Record(m) => {
                    let record = matches!(c, Control::Record(_));
                    if !record && s.mode == m {
                        cx.rec.layer(
                            "Chosen",
                            Rect::new(r.x, r.y + 9.0, r.width, 30.0),
                            Shape::rounded(8.0),
                            Style::fill(pill),
                        );
                    }
                    let sym = match (m, record) {
                        (Mode::Screen, false) => "display",
                        (Mode::Window, false) => "window",
                        (Mode::Selection, false) => "selection",
                        (Mode::Screen, true) => "record",
                        (Mode::Window, true) => "record-circle",
                        (Mode::Selection, true) => "video",
                    };
                    let ink = if record { glyph.with_alpha(90) } else { glyph };
                    cx.rec.glyph(
                        "Mode",
                        Rect::new(r.x + (r.width - 22.0) / 2.0, r.y + 13.0, 22.0, 22.0),
                        sym,
                        400,
                        ink,
                    );
                }
                Control::Options => {
                    let style = theme.text(&typography::BODY, glyph);
                    let l = self
                        .label
                        .layout("Options ⌄", &style, Align::Center, r.width, Some(1));
                    let h = l.size().height;
                    cx.rec.text(
                        "Options",
                        Rect::new(r.x, r.y + (r.height - h) / 2.0, r.width, h),
                        l,
                        None,
                        glyph,
                    );
                }
                Control::Action => {
                    let pill = Rect::new(r.x + 2.0, r.y + 10.0, 76.0, 30.0);
                    cx.rec.layer(
                        "Capture",
                        pill,
                        Shape::rounded(8.0),
                        Style::fill(Color::from_u32(0x006fdcff)),
                    );
                    let style = theme.text(&typography::BODY, Color::WHITE);
                    let l =
                        self.label
                            .layout("Capture", &style, Align::Center, pill.width, Some(1));
                    let h = l.size().height;
                    cx.rec.text(
                        "Capture text",
                        Rect::new(pill.x, pill.y + (pill.height - h) / 2.0, pill.width, h),
                        l,
                        None,
                        Color::WHITE,
                    );
                }
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        let state = self.state.clone();
        let bar = self.bar_rect(b);
        match event {
            Event::Key(k) if k.pressed && k.key == Key::Escape => {
                close_bar(cx.ui(), &state);
                Handled::Yes
            }
            Event::Key(k) if k.pressed && k.key == Key::Enter => {
                bar_capture(cx.ui(), &state, Point::new(b.width / 2.0, b.height / 2.0));
                Handled::Yes
            }
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                let p = *pos;
                if bar.contains(p) {
                    let local = Point::new(p.x - bar.x, p.y - bar.y);
                    match controls()
                        .into_iter()
                        .find(|(_, r)| r.contains(local))
                        .map(|c| c.0)
                    {
                        Some(Control::Close) => close_bar(cx.ui(), &state),
                        Some(Control::Capture(m)) => {
                            let before = state.borrow().mode;
                            state.borrow_mut().mode = m;
                            if (before == Mode::Window) != (m == Mode::Window) {
                                veil(cx.ui(), m == Mode::Window);
                            }
                            cx.request_paint();
                        }
                        Some(Control::Record(_)) => {}
                        Some(Control::Options) => {
                            let r = controls()
                                .into_iter()
                                .find(|c| c.0 == Control::Options)
                                .map_or(bar, |c| c.1.translate(bar.origin()));
                            self.options_menu(cx, Point::new(r.x, r.y));
                        }
                        Some(Control::Action) => {
                            bar_capture(cx.ui(), &state, Point::new(b.width / 2.0, b.height / 2.0));
                        }
                        None => self.moving = Some(Point::new(p.x - bar.x, p.y - bar.y)),
                    }
                    return Handled::Yes;
                }
                let mode = state.borrow().mode;
                match mode {
                    Mode::Screen | Mode::Window => {
                        bar_capture(cx.ui(), &state, Point::new(p.x - b.x, p.y - b.y))
                    }
                    Mode::Selection => {
                        let sel = state.borrow().last_selection.unwrap_or(Rect::ZERO);
                        let local = Point::new(p.x - b.x, p.y - b.y);
                        self.sel_drag = Some((local, sel, !sel.contains(local)));
                    }
                }
                Handled::Yes
            }
            Event::PointerMove { pos, .. } => {
                if let Some(off) = self.moving {
                    let at = Point::new(pos.x - b.x - off.x, pos.y - b.y - off.y);
                    BAR_AT.with(|c| c.set(Some(at)));
                    cx.request_paint();
                } else if let Some((start, rect, new)) = self.sel_drag {
                    let p = Point::new(pos.x - b.x, pos.y - b.y);
                    let r = if new {
                        Rect::new(
                            start.x.min(p.x),
                            start.y.min(p.y),
                            (start.x - p.x).abs(),
                            (start.y - p.y).abs(),
                        )
                    } else {
                        rect.translate(Point::new(p.x - start.x, p.y - start.y))
                    };
                    state.borrow_mut().last_selection = Some(r);
                    cx.request_paint();
                }
                Handled::Yes
            }
            Event::PointerUp { .. } => {
                self.moving = None;
                self.sel_drag = None;
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
        a.label = Some("Screenshot".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_carry_the_time() {
        let n = file_name();
        assert!(n.starts_with("Screenshot ") && n.ends_with(".png"));
        assert!(n.contains(" at "));
    }

    #[test]
    fn the_bar_buttons_follow_the_measures() {
        let c = controls();
        assert_eq!(c.len(), 9);
        assert!(c.iter().all(|(_, r)| r.right() <= metrics::BAR.0));
    }
}
