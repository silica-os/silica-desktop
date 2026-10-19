//! Notifications (docs/spec/notifications.md): banners and Notification Center over
//! silica-daemon's `Notifications1`.
//!
//! - A banner slides in from the right 16 pt under the menu bar, stays 5 s (forever while the
//!   pointer is on it, then 1.1 s), and slides out; a newer one from the same app replaces it.
//!   A click opens it (its default action, else its app), a drag to the right sends it away,
//!   the × on hover dismisses it. With Do Not Disturb on no banner shows.
//! - Notification Center (the clock, or `notifications.toggle`) slides the column in from the
//!   right over a scrim: the notifications grouped by app (a collapsed stack shows two strips
//!   under its top card; a click expands it), Clear All, and the empty state.
//!
//! Not yet: action buttons, inline replies, widgets.

use std::cell::RefCell;
use std::rc::Rc;

use futures_lite::StreamExt;
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
use vetro_lib::geom::{Insets, Point, Rect, Size};
use vetro_lib::services::types::Notification;
use vetro_lib::shape::Shape;
use vetro_lib::style::{Border, Style};
use vetro_lib::text::Align;

use crate::apps;

/// Measures (notifications.md "Banner geometry", "Notification Center").
pub mod metrics {
    /// A card's width.
    pub const WIDTH: f32 = 344.0;
    /// Distance from the screen's right edge.
    pub const RIGHT: f32 = 16.0;
    /// Distance below the menu bar.
    pub const TOP: f32 = 16.0;
    /// Corner radius.
    pub const RADIUS: f32 = 20.0;
    /// The app icon.
    pub const ICON: f32 = 32.0;
    /// The text column's start.
    pub const TEXT_X: f32 = 58.0;
    /// Line pitch.
    pub const LINE: f32 = 17.0;
    /// Room around the banner for its shadow and close button.
    pub const MARGIN: f32 = 16.0;
    /// The entrance's start, right of the rest position.
    pub const SLIDE: f32 = 172.0;
    /// The menu bar's height.
    pub const MENU_BAR: f32 = 32.0;
    /// The stack strips: 7 pt at an 8 pt pitch.
    pub const STRIP: f32 = 8.0;
    /// The header's height.
    pub const HEADER: f32 = 21.0;
}

/// How long a banner stays.
const STAY: f64 = 5.0;
/// How long it stays after the pointer leaves it.
const LINGER: f64 = 1.1;
/// The entrance's time constant (each 13 ms frame closes 18 % of the distance).
const TAU: f64 = 0.065;
/// The exit.
const LEAVE: f64 = 0.18;

/// Card text: about 14 pt.
const CARD_TITLE: TypeStyle = TypeStyle {
    name: "Notification Title",
    size: 14.0,
    line_height: Some(17.0),
    weight: 600,
    ..typography::BODY
};
const CARD_BODY: TypeStyle = TypeStyle {
    name: "Notification Body",
    size: 14.0,
    line_height: Some(17.0),
    weight: 400,
    ..typography::BODY
};

/// A banner on screen.
#[derive(Debug, Clone)]
pub struct Banner {
    /// What it shows.
    pub n: Notification,
    /// When it appeared.
    pub since: f64,
    /// When it began leaving.
    pub leaving: Option<f64>,
    /// The pointer is on it.
    pub hovered: bool,
    /// When the pointer left it.
    pub left_at: Option<f64>,
    /// How far it was dragged to the right.
    pub drag: f32,
}

/// The notifications' state.
#[derive(Default)]
pub struct NotesState {
    /// The notifications, oldest first.
    pub list: Vec<Notification>,
    /// The banner and the one leaving under it.
    pub banner: Option<Banner>,
    leaving: Option<Banner>,
    /// The banner's surface and view.
    surface: Option<(WindowId, ViewId)>,
    /// Do Not Disturb.
    pub dnd: bool,
    /// Notification Center: its surface, when it opened, when it began closing.
    center: Option<(WindowId, ViewId)>,
    center_since: f64,
    center_closing: Option<f64>,
    /// The expanded stacks (app names).
    expanded: Vec<String>,
    /// Where a press began on the banner.
    press: Option<Point>,
    /// The output's size.
    output: Size,
    /// Run when the center closes (the clock's highlight).
    on_center_closed: Option<Callback>,
}

/// Run with the core.
pub type Callback = Rc<dyn Fn(&mut Ui)>;

/// Shared.
pub type Shared = Rc<RefCell<NotesState>>;

fn app_of(n: &Notification) -> String {
    if n.app_name.is_empty() {
        n.desktop_entry.clone()
    } else {
        n.app_name.clone()
    }
}

/// The lines a card shows: title (the app), subtitle (the summary), up to two body lines.
fn card_height(n: &Notification) -> f32 {
    let mut lines = 1;
    if !n.summary.is_empty() {
        lines += 1;
    }
    if !n.body.is_empty() {
        lines += if n.body.chars().count() > 40 { 2 } else { 1 };
    }
    (40.0 + metrics::LINE * (lines - 1) as f32).max(58.0)
}

fn icon_of(n: &Notification, dark: bool) -> Option<skia_safe::Image> {
    if !n.desktop_entry.is_empty()
        && let Some(e) = apps::entry(&n.desktop_entry)
        && let Some(i) = apps::icon(&e, dark)
    {
        return Some(i);
    }
    if !n.image_path.is_empty() {
        let p = n.image_path.trim_start_matches("file://");
        if let Some(i) = apps::icon_named(p) {
            return Some(i);
        }
    }
    apps::icon_named(&n.app_icon)
}

/// Paints a card at `r`.
fn paint_card(
    cx: &mut PaintCx<'_>,
    labels: &mut Vec<Label>,
    li: &mut usize,
    r: Rect,
    n: &Notification,
    hovered: bool,
) {
    let theme = *cx.theme();
    let dark = theme.dark();
    let ink = if dark {
        Color::from_u32(0xeaeaebff)
    } else {
        Color::from_u32(0x1d1d1eff)
    };
    cx.glass(
        "Card",
        r,
        Shape::rounded(metrics::RADIUS),
        "Glass/Regular",
        GlassProfile::Banner,
    );
    let ir = Rect::new(
        r.x + 14.0,
        r.y + (r.height - metrics::ICON) / 2.0,
        metrics::ICON,
        metrics::ICON,
    );
    match icon_of(n, dark) {
        Some(img) => cx.rec.image("App icon", ir, img, 1.0),
        None => cx.rec.layer(
            "App icon",
            ir,
            Shape::rounded(7.0),
            Style::fill(theme.colors.surface_control)
                .with_border(Border::inside(theme.colors.line_border, 1.0)),
        ),
    }
    let tw = r.width - metrics::TEXT_X - 14.0;
    let mut y = r.y + 12.0;
    let title = theme.text(&CARD_TITLE, ink);
    let body = theme.text(&CARD_BODY, ink);
    let mut line = |cx: &mut PaintCx<'_>, text: &str, style, lines: usize, h: f32| {
        if labels.len() <= *li {
            labels.resize_with(*li + 1, Label::default);
        }
        if let Some(l) = labels.get_mut(*li) {
            let layout = l.layout(text, style, Align::Start, tw, Some(lines));
            cx.rec.text(
                "Text",
                Rect::new(r.x + metrics::TEXT_X, y, tw, h),
                layout,
                None,
                ink,
            );
        }
        *li += 1;
        y += h;
    };
    line(cx, &app_of(n), &title, 1, metrics::LINE);
    if !n.summary.is_empty() {
        line(cx, &n.summary, &title, 1, metrics::LINE);
    }
    if !n.body.is_empty() {
        line(cx, &n.body, &body, 2, 2.0 * metrics::LINE);
    }
    if hovered {
        // The close button on the top-left corner: a light disc with a dark ×.
        let c = Rect::new(r.x - 6.5, r.y - 4.5, 21.0, 21.0);
        cx.rec.layer(
            "Close",
            c,
            Shape::Ellipse,
            Style::fill(Color::from_u32(0xf2f2f4ff))
                .with_border(Border::inside(Color::from_u32(0x00000059), 1.0)),
        );
        cx.rec.glyph(
            "Close glyph",
            c.inset(Insets::all(5.0)),
            "xmark",
            600,
            Color::from_u32(0x3a3a3cff),
        );
    }
}

/// The close button's rect for a card at `r`.
fn close_rect(r: Rect) -> Rect {
    Rect::new(r.x - 6.5, r.y - 4.5, 21.0, 21.0)
}

/// Starts following the daemon.
pub fn start(ui: &mut Ui, state: &Shared) {
    state.borrow_mut().output = ui
        .outputs()
        .first()
        .map_or(Size::new(1280.0, 800.0), |o| o.size);
    let st = state.clone();
    vetro_lib::platform::spawn(async move {
        let Some(services) = vetro_lib::platform::services() else {
            return;
        };
        let Ok(proxy) = services.notifications().await else {
            return;
        };
        if let Ok(list) = proxy.list().await {
            let s = st.clone();
            vetro_lib::platform::post(move |ui| {
                s.borrow_mut().list = list;
                repaint_center(ui, &s);
            });
        }
        let (Ok(mut added), Ok(mut removed)) =
            (proxy.receive_added().await, proxy.receive_removed().await)
        else {
            return;
        };
        let s2 = st.clone();
        let removals = async move {
            while let Some(sig) = removed.next().await {
                if let Ok(a) = sig.args() {
                    let id = a.id;
                    let s = s2.clone();
                    vetro_lib::platform::post(move |ui| on_removed(ui, &s, id));
                }
            }
        };
        let additions = async move {
            while let Some(sig) = added.next().await {
                if let Ok(a) = sig.args() {
                    let n = a.notification;
                    let s = st.clone();
                    vetro_lib::platform::post(move |ui| on_added(ui, &s, n));
                }
            }
        };
        futures_lite::future::zip(additions, removals).await;
    });
}

/// A notification arrived.
pub fn on_added(ui: &mut Ui, state: &Shared, n: Notification) {
    let now = ui.now;
    {
        let mut s = state.borrow_mut();
        s.list.retain(|x| x.id != n.id);
        s.list.push(n.clone());
        if s.dnd || s.center.is_some() {
            drop(s);
            repaint_center(ui, state);
            return;
        }
        // A banner replaces the one on screen; they overlap while it leaves.
        if let Some(mut old) = s.banner.take() {
            old.leaving = Some(now);
            s.leaving = Some(old);
        }
        s.banner = Some(Banner {
            n,
            since: now,
            leaving: None,
            hovered: false,
            left_at: None,
            drag: 0.0,
        });
    }
    show_banner_surface(ui, state);
    repaint_center(ui, state);
}

/// A notification went.
pub fn on_removed(ui: &mut Ui, state: &Shared, id: u32) {
    let now = ui.now;
    {
        let mut s = state.borrow_mut();
        s.list.retain(|x| x.id != id);
        if let Some(b) = s.banner.as_mut()
            && b.n.id == id
            && b.leaving.is_none()
        {
            b.leaving = Some(now);
        }
    }
    animate_banner(ui, state);
    repaint_center(ui, state);
}

fn show_banner_surface(ui: &mut Ui, state: &Shared) {
    let h = state
        .borrow()
        .banner
        .as_ref()
        .map_or(91.0, |b| card_height(&b.n));
    let size = Size::new(
        metrics::WIDTH + metrics::MARGIN + metrics::RIGHT,
        h + 2.0 * metrics::MARGIN,
    );
    let existing = state.borrow().surface;
    match existing {
        Some((w, v)) => {
            ui.request(vetro_lib::core::Request::Resize(w, size));
            ui.request_paint(v);
            ui.start_animating(v);
        }
        None => {
            let output = ui.outputs().first().map(|o| o.name.clone());
            let w = ui.create_window(
                WindowKind::Shell {
                    layer: Layer::Overlay,
                    anchor: Edges::TOP | Edges::RIGHT,
                    exclusive: 0,
                    margin: [(metrics::TOP - metrics::MARGIN) as i32, 0, 0, 0],
                    keyboard: KeyboardMode::None,
                    namespace: "notification".into(),
                    output,
                },
                size,
            );
            if let Some(win) = ui.window_mut(w) {
                win.opaque = false;
            }
            let v = ui.create(BannerView {
                state: state.clone(),
                labels: Vec::new(),
            });
            let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
            let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
            ui.set_content(w, v);
            ui.show_window(w);
            ui.start_animating(v);
            state.borrow_mut().surface = Some((w, v));
        }
    }
    let r = Rect::new(
        metrics::MARGIN - 8.0,
        metrics::MARGIN - 6.0,
        metrics::WIDTH + 8.0,
        h + 6.0,
    );
    if let Some((w, _)) = state.borrow().surface {
        ui.set_input_region(w, Some(vec![r]));
    }
}

fn animate_banner(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
        ui.start_animating(v);
    }
}

/// The card's x offset right of its rest position, and its opacity, at `now`.
fn banner_motion(b: &Banner, now: f64) -> (f32, f32) {
    let t = now - b.since;
    let enter = metrics::SLIDE * (-t / TAU).exp() as f32;
    let alpha_in = (t / 0.21).clamp(0.0, 1.0) as f32;
    let (out, alpha_out) = match b.leaving {
        Some(l) => {
            let k = ((now - l) / LEAVE).clamp(0.0, 1.0) as f32;
            (k * k * (metrics::WIDTH + metrics::RIGHT), 1.0 - k)
        }
        None => (0.0, 1.0),
    };
    (enter + out + b.drag.max(0.0), alpha_in * alpha_out)
}

/// The banner on screen.
struct BannerView {
    state: Shared,
    labels: Vec<Label>,
}

impl BannerView {
    fn card(&self, b: Rect, banner: &Banner, now: f64) -> Rect {
        let (dx, _) = banner_motion(banner, now);
        Rect::new(
            b.x + metrics::MARGIN + dx,
            b.y + metrics::MARGIN,
            metrics::WIDTH,
            card_height(&banner.n),
        )
    }

    fn dismiss(id: u32) {
        vetro_lib::platform::spawn(async move {
            if let Some(s) = vetro_lib::platform::services()
                && let Ok(p) = s.notifications().await
            {
                let _ = p.dismiss(id).await;
            }
        });
    }
}

/// Opens notification `n`: its default action, else its app.
fn open_notification(ui: &mut Ui, n: &Notification) {
    if n.actions.iter().any(|a| a == "default") {
        let id = n.id;
        vetro_lib::platform::spawn(async move {
            if let Some(s) = vetro_lib::platform::services()
                && let Ok(p) = s.notifications().await
            {
                let _ = p.invoke_action(id, "default").await;
            }
        });
    } else if !n.desktop_entry.is_empty() {
        let params = serde_json::json!({ "app": n.desktop_entry }).to_string();
        ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
    }
}

impl vetro_lib::core::Widget for BannerView {
    fn kind(&self) -> &'static str {
        "Banner"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(376.0, 123.0))
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        let mut s = self.state.borrow_mut();
        if let Some(b) = s.banner.as_mut()
            && b.leaving.is_none()
        {
            let expired = if b.hovered {
                false
            } else if let Some(l) = b.left_at {
                now - l >= LINGER
            } else {
                now - b.since >= STAY
            };
            if expired {
                b.leaving = Some(now);
            }
        }
        if s.leaving
            .as_ref()
            .is_some_and(|b| b.leaving.is_some_and(|l| now - l >= LEAVE))
        {
            s.leaving = None;
        }
        if s.banner
            .as_ref()
            .is_some_and(|b| b.leaving.is_some_and(|l| now - l >= LEAVE))
        {
            s.banner = None;
        }
        cx.request_paint();
        let done = s.banner.is_none() && s.leaving.is_none();
        if done && let Some((w, _)) = s.surface.take() {
            drop(s);
            cx.ui().close_window(w);
            return false;
        }
        true
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let now = cx.now();
        let b = cx.bounds();
        let s = self.state.borrow();
        let mut li = 0;
        for banner in [s.leaving.as_ref(), s.banner.as_ref()]
            .into_iter()
            .flatten()
        {
            let (_, alpha) = banner_motion(banner, now);
            let r = self.card(b, banner, now);
            cx.rec.push_opacity(alpha);
            paint_card(cx, &mut self.labels, &mut li, r, &banner.n, banner.hovered);
            cx.rec.pop();
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let now = cx.now();
        let b = cx.bounds();
        match event {
            Event::PointerEnter | Event::PointerMove { .. } => {
                let mut s = self.state.borrow_mut();
                let press = s.press;
                if let Some(bn) = s.banner.as_mut() {
                    bn.hovered = true;
                    bn.left_at = None;
                    if let (Some(p0), Event::PointerMove { pos, .. }) = (press, event) {
                        bn.drag = (pos.x - p0.x).max(0.0);
                    }
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerLeave => {
                if let Some(bn) = self.state.borrow_mut().banner.as_mut() {
                    bn.hovered = false;
                    bn.left_at = Some(now);
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                self.state.borrow_mut().press = Some(*pos);
                Handled::Yes
            }
            Event::PointerUp {
                pos,
                button: Button::Left,
                ..
            } => {
                let mut s = self.state.borrow_mut();
                s.press = None;
                let Some(bn) = s.banner.clone() else {
                    return Handled::Yes;
                };
                let card = self.card(b, &bn, now);
                let dragged = bn.drag >= 15.0;
                if let Some(x) = s.banner.as_mut() {
                    x.drag = 0.0;
                    x.leaving = Some(now);
                }
                drop(s);
                if dragged {
                    // A drag sends it away; it stays in Notification Center.
                } else if close_rect(card).contains(*pos) {
                    Self::dismiss(bn.n.id);
                } else {
                    open_notification(cx.ui(), &bn.n);
                }
                cx.request_paint();
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
            .state
            .borrow()
            .banner
            .as_ref()
            .map(|b| b.n.summary.clone());
    }
}

// ----- Notification Center -------------------------------------------------------------------

/// The groups the center shows: (app, notifications newest first), the newest group first.
fn groups(list: &[Notification]) -> Vec<(String, Vec<Notification>)> {
    let mut out: Vec<(String, Vec<Notification>)> = Vec::new();
    for n in list.iter().rev().filter(|n| !n.transient) {
        let app = app_of(n);
        match out.iter_mut().find(|g| g.0 == app) {
            Some(g) => g.1.push(n.clone()),
            None => out.push((app, vec![n.clone()])),
        }
    }
    out
}

/// One placed element of the center.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    /// The "Notification Center" header with Clear All.
    Header(Rect),
    /// A card (group index, index in the group).
    Card(Rect, usize, usize),
    /// A stack's strips under its top card (group index, how many).
    Strips(Rect, usize, usize),
    /// An expanded stack's header (group index).
    StackHeader(Rect, usize),
    /// The empty state.
    Empty(Rect),
}

/// Lays the center's column out (x from 0, y from the column's top).
fn layout(groups: &[(String, Vec<Notification>)], expanded: &[String]) -> Vec<Part> {
    let w = metrics::WIDTH;
    let mut parts = Vec::new();
    let mut y = metrics::MENU_BAR + metrics::TOP;
    if groups.is_empty() {
        parts.push(Part::Empty(Rect::new(0.0, y + 40.0, w, 22.0)));
        return parts;
    }
    parts.push(Part::Header(Rect::new(0.0, y, w, metrics::HEADER)));
    y += metrics::HEADER + 7.5;
    for (gi, (app, list)) in groups.iter().enumerate() {
        let open = list.len() > 1 && expanded.iter().any(|e| e == app);
        if open {
            parts.push(Part::StackHeader(Rect::new(0.0, y, w, 24.0), gi));
            y += 30.0;
            for (k, n) in list.iter().enumerate() {
                let _ = n;
                let h = 58.0;
                parts.push(Part::Card(Rect::new(0.0, y, w, h), gi, k));
                y += 64.0;
            }
        } else if let Some(n) = list.first() {
            let h = card_height(n);
            parts.push(Part::Card(Rect::new(0.0, y, w, h), gi, 0));
            y += h;
            let strips = list.len().saturating_sub(1).min(2);
            if strips > 0 {
                parts.push(Part::Strips(
                    Rect::new(0.0, y, w, strips as f32 * metrics::STRIP),
                    gi,
                    strips,
                ));
                y += strips as f32 * metrics::STRIP;
            }
        }
        y += 8.0;
    }
    parts
}

/// Opens or closes Notification Center.
pub fn toggle_center(ui: &mut Ui, state: &Shared, closed: Option<Callback>) {
    let now = ui.now;
    let open = state.borrow().center.is_some();
    if open {
        close_center(ui, state);
        return;
    }
    let out = ui
        .outputs()
        .first()
        .map_or(Size::new(1280.0, 800.0), |o| o.size);
    let output = ui.outputs().first().map(|o| o.name.clone());
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Overlay,
            anchor: Edges::TOP | Edges::BOTTOM | Edges::LEFT | Edges::RIGHT,
            exclusive: -1,
            margin: [0; 4],
            keyboard: KeyboardMode::Exclusive,
            namespace: "notification-center".into(),
            output,
        },
        out,
    );
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
    }
    let v = ui.create(CenterView {
        state: state.clone(),
        labels: Vec::new(),
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, v);
    ui.show_window(w);
    ui.set_focus(v, false);
    ui.start_animating(v);
    {
        let mut s = state.borrow_mut();
        s.center = Some((w, v));
        s.center_since = now;
        s.center_closing = None;
        s.output = out;
        s.on_center_closed = closed;
        // The banner goes when the center opens.
        if let Some(b) = s.banner.as_mut() {
            b.leaving = Some(now);
        }
    }
    animate_banner(ui, state);
}

/// Closes Notification Center (it slides out first).
pub fn close_center(ui: &mut Ui, state: &Shared) {
    let now = ui.now;
    let v = {
        let mut s = state.borrow_mut();
        if s.center_closing.is_none() {
            s.center_closing = Some(now);
        }
        s.center.map(|c| c.1)
    };
    if let Some(v) = v {
        ui.start_animating(v);
    }
}

fn repaint_center(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().center {
        ui.request_paint(v);
    }
}

/// The column's x offset (right of its rest position) at `now`.
fn center_offset(s: &NotesState, now: f64) -> f32 {
    let travel = metrics::WIDTH + metrics::RIGHT + 70.0;
    if let Some(c) = s.center_closing {
        let k = ((now - c) / 0.3).clamp(0.0, 1.0) as f32;
        return travel * k * k;
    }
    let t = now - s.center_since;
    travel * (-t / 0.045).exp() as f32
}

/// Notification Center.
struct CenterView {
    state: Shared,
    labels: Vec<Label>,
}

impl CenterView {
    fn column_x(s: &NotesState, b: Rect, now: f64) -> f32 {
        b.right() - metrics::RIGHT - metrics::WIDTH + center_offset(s, now)
    }

    fn clear_all() {
        vetro_lib::platform::spawn(async move {
            if let Some(s) = vetro_lib::platform::services()
                && let Ok(p) = s.notifications().await
            {
                let _ = p.clear().await;
            }
        });
    }
}

impl vetro_lib::core::Widget for CenterView {
    fn kind(&self) -> &'static str {
        "NotificationCenter"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(self.state.borrow().output)
    }

    fn focus(&self) -> FocusKind {
        FocusKind::Always
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        let s = self.state.borrow();
        cx.request_paint();
        if let Some(c) = s.center_closing
            && now - c >= 0.3
        {
            let closed = s.on_center_closed.clone();
            drop(s);
            let w = {
                let mut s = self.state.borrow_mut();
                s.center_closing = None;
                s.center.take().map(|c| c.0)
            };
            if let Some(w) = w {
                cx.ui().close_window(w);
            }
            if let Some(f) = closed {
                f(cx.ui());
            }
            return false;
        }
        now - s.center_since < 0.3 || s.center_closing.is_some()
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let now = cx.now();
        let b = cx.bounds();
        let theme = *cx.theme();
        let s = self.state.borrow();
        let x = Self::column_x(&s, b, now);
        let travel = metrics::WIDTH + metrics::RIGHT + 70.0;
        let shown = 1.0 - (center_offset(&s, now) / travel).clamp(0.0, 1.0);
        // The scrim: one soft halo around the column.
        // It starts about 70 pt before the column and deepens towards it.
        for i in 0..7 {
            let left = x - 70.0 + i as f32 * 10.0;
            cx.rec.layer(
                "Scrim",
                Rect::new(left, b.y, b.right() - left, b.height),
                Shape::rounded(0.0),
                Style::fill(Color::rgba(0, 0, 0, (shown * 7.0) as u8)),
            );
        }
        let gs = groups(&s.list);
        let parts = layout(&gs, &s.expanded);
        let white = Color::rgba(255, 255, 255, 230);
        let mut li = 0;
        for p in &parts {
            match p {
                Part::Header(r) => {
                    let r = r.translate(Point::new(x, b.y));
                    let style = theme.text(&typography::TITLE_3, white);
                    if self.labels.len() <= li {
                        self.labels.resize_with(li + 1, Label::default);
                    }
                    if let Some(l) = self.labels.get_mut(li) {
                        let lay = l.layout(
                            "Notification Center",
                            &style,
                            Align::Start,
                            r.width - 30.0,
                            Some(1),
                        );
                        cx.rec.text(
                            "Header",
                            Rect::new(r.x, r.y, r.width - 30.0, r.height),
                            lay,
                            None,
                            white,
                        );
                    }
                    li += 1;
                    let c = Rect::new(r.right() - 20.0, r.y + (r.height - 20.0) / 2.0, 20.0, 20.0);
                    cx.rec.layer(
                        "Clear All",
                        c,
                        Shape::Ellipse,
                        Style::fill(Color::from_u32(0xbcbdc0ff))
                            .with_border(Border::inside(Color::from_u32(0x2c2d3080), 1.0)),
                    );
                    cx.rec.glyph(
                        "Clear All glyph",
                        c.inset(Insets::all(5.0)),
                        "xmark",
                        600,
                        Color::from_u32(0x5a5b5cff),
                    );
                }
                Part::Card(r, gi, k) => {
                    let r = r.translate(Point::new(x, b.y));
                    if let Some(n) = gs.get(*gi).and_then(|g| g.1.get(*k)) {
                        paint_card(cx, &mut self.labels, &mut li, r, n, false);
                    }
                }
                Part::Strips(r, _, n) => {
                    let r = r.translate(Point::new(x, b.y));
                    for i in 0..*n {
                        let inset = if i == 0 { 19.0 } else { 29.0 };
                        let strip = Rect::new(
                            r.x + inset,
                            r.y + i as f32 * metrics::STRIP - 6.0,
                            r.width - 2.0 * inset,
                            13.0,
                        );
                        cx.glass(
                            "Strip",
                            strip,
                            Shape::rounded(10.0),
                            "Glass/Regular",
                            GlassProfile::Banner,
                        );
                    }
                }
                Part::StackHeader(r, gi) => {
                    let r = r.translate(Point::new(x, b.y));
                    let name = gs.get(*gi).map(|g| g.0.clone()).unwrap_or_default();
                    let style = theme.text(&typography::TITLE_3, white);
                    if self.labels.len() <= li {
                        self.labels.resize_with(li + 1, Label::default);
                    }
                    if let Some(l) = self.labels.get_mut(li) {
                        let lay = l.layout(&name, &style, Align::Start, r.width - 140.0, Some(1));
                        cx.rec.text(
                            "Stack header",
                            Rect::new(r.x, r.y, r.width - 140.0, r.height),
                            lay,
                            None,
                            white,
                        );
                    }
                    li += 1;
                    let less = Rect::new(r.right() - 130.0, r.y + 2.0, 102.0, 20.0);
                    cx.rec.layer(
                        "Show less",
                        less,
                        Shape::rounded(10.0),
                        Style::fill(Color::from_u32(0xbcbdc0e6)),
                    );
                    let small = theme.text(&typography::CAPTION, Color::from_u32(0x2c2d30ff));
                    if self.labels.len() <= li {
                        self.labels.resize_with(li + 1, Label::default);
                    }
                    if let Some(l) = self.labels.get_mut(li) {
                        let lay = l.layout("Show less", &small, Align::Center, less.width, Some(1));
                        let h = lay.size().height;
                        cx.rec.text(
                            "Show less text",
                            Rect::new(less.x, less.y + (20.0 - h) / 2.0, less.width, h),
                            lay,
                            None,
                            Color::from_u32(0x2c2d30ff),
                        );
                    }
                    li += 1;
                    let c = Rect::new(r.right() - 20.0, r.y + 2.0, 20.0, 20.0);
                    cx.rec.layer(
                        "Clear",
                        c,
                        Shape::Ellipse,
                        Style::fill(Color::from_u32(0xbcbdc0e6)),
                    );
                    cx.rec.glyph(
                        "Clear glyph",
                        c.inset(Insets::all(5.0)),
                        "xmark",
                        600,
                        Color::from_u32(0x5a5b5cff),
                    );
                }
                Part::Empty(r) => {
                    let r = r.translate(Point::new(x, b.y));
                    let style = theme.text(&typography::HEADLINE, white);
                    if self.labels.len() <= li {
                        self.labels.resize_with(li + 1, Label::default);
                    }
                    if let Some(l) = self.labels.get_mut(li) {
                        let lay = l.layout(
                            "No recent notifications",
                            &style,
                            Align::Center,
                            r.width,
                            Some(1),
                        );
                        cx.rec.text("Empty", r, lay, None, white);
                    }
                    li += 1;
                }
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let now = cx.now();
        let b = cx.bounds();
        match event {
            Event::Key(k) if k.pressed && k.key == Key::Escape => {
                let state = self.state.clone();
                close_center(cx.ui(), &state);
                Handled::Yes
            }
            Event::PointerUp {
                pos,
                button: Button::Left,
                ..
            } => {
                let state = self.state.clone();
                let s = state.borrow();
                let x = Self::column_x(&s, b, now);
                let p = Point::new(pos.x - x, pos.y - b.y);
                let gs = groups(&s.list);
                let parts = layout(&gs, &s.expanded);
                drop(s);
                let hit = parts.iter().find(|part| match part {
                    Part::Header(r)
                    | Part::Card(r, ..)
                    | Part::Strips(r, ..)
                    | Part::StackHeader(r, _) => r.contains(p),
                    Part::Empty(_) => false,
                });
                match hit {
                    None => close_center(cx.ui(), &state),
                    Some(Part::Header(r)) => {
                        if p.x >= r.right() - 24.0 {
                            Self::clear_all();
                        }
                    }
                    Some(Part::StackHeader(r, gi)) => {
                        let app = gs.get(*gi).map(|g| g.0.clone()).unwrap_or_default();
                        if p.x >= r.right() - 22.0 {
                            // Clear the group.
                            let ids: Vec<u32> = gs
                                .get(*gi)
                                .map(|g| g.1.iter().map(|n| n.id).collect())
                                .unwrap_or_default();
                            for id in ids {
                                BannerView::dismiss(id);
                            }
                        }
                        state.borrow_mut().expanded.retain(|e| *e != app);
                    }
                    Some(Part::Card(r, gi, k)) => {
                        let Some(g) = gs.get(*gi) else {
                            return Handled::Yes;
                        };
                        let stacked = g.1.len() > 1;
                        let expanded = state.borrow().expanded.contains(&g.0);
                        if stacked && !expanded {
                            state.borrow_mut().expanded.push(g.0.clone());
                        } else if let Some(n) = g.1.get(*k) {
                            let _ = r;
                            open_notification(cx.ui(), n);
                            BannerView::dismiss(n.id);
                            close_center(cx.ui(), &state);
                        }
                    }
                    Some(Part::Strips(_, gi, _)) => {
                        if let Some(g) = gs.get(*gi) {
                            state.borrow_mut().expanded.push(g.0.clone());
                        }
                    }
                    Some(Part::Empty(_)) => {}
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerDown { .. } => Handled::Yes,
            _ => Handled::No,
        }
    }

    fn hit(&self, _p: Point, _size: Size) -> bool {
        true
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Dialog;
        a.label = Some("Notification Center".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: u32, app: &str) -> Notification {
        Notification {
            id,
            app_name: app.into(),
            summary: "Lemon cake".into(),
            body: "The oven is ready; put the cake in for forty minutes.".into(),
            app_icon: String::new(),
            actions: Vec::new(),
            urgency: 1,
            category: String::new(),
            desktop_entry: String::new(),
            image_path: String::new(),
            transient: false,
            resident: false,
            timestamp_ms: 0,
            expire_timeout: -1,
        }
    }

    #[test]
    fn a_full_card_is_91_tall() {
        assert_eq!(card_height(&note(1, "Clock")), 91.0);
    }

    #[test]
    fn the_banner_slides_in_and_out() {
        let b = Banner {
            n: note(1, "Clock"),
            since: 0.0,
            leaving: None,
            hovered: false,
            left_at: None,
            drag: 0.0,
        };
        assert_eq!(banner_motion(&b, 0.0).0, metrics::SLIDE);
        assert!(banner_motion(&b, 0.4).0 < 1.0);
        let gone = Banner {
            leaving: Some(1.0),
            ..b
        };
        assert!(banner_motion(&gone, 1.0 + LEAVE).1 <= 0.0);
    }

    #[test]
    fn notifications_group_by_app() {
        let list = vec![note(1, "Mail"), note(2, "Clock"), note(3, "Mail")];
        let g = groups(&list);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, "Mail");
        assert_eq!(g[0].1.len(), 2);
        let parts = layout(&g, &[]);
        assert!(matches!(parts[0], Part::Header(_)));
        assert!(parts.iter().any(|p| matches!(p, Part::Strips(_, 0, 1))));
        assert!(matches!(layout(&[], &[])[0], Part::Empty(_)));
    }
}
