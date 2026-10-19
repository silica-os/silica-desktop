//! Banners and the Notification Centre on the Amalfi kit (`Notifications/*`), with the behaviour
//! measured in docs/spec/notifications.md, as the Flutter shell does.
//!
//! - A banner is a Glass/Regular card 360 wide (radius 18) 16 pt from the right edge and below
//!   the menu bar: the 36 pt app icon, the title (Headline) and two lines of body (Callout,
//!   secondary). It slides in from the right (time constant 71 ms) fading in, stays 5 s
//!   (forever while the pointer is on it, then 1.1 s; alerts and critical ones until closed)
//!   and leaves with the measured exit. On hover: the close button and "Details", which (as a
//!   long press) opens its actions as small push buttons and the reply field. A click runs the
//!   default action or opens the app; a drag to the right sends it away. A new banner replaces
//!   the one on screen; Do Not Disturb or the app's "none" style keep it silent.
//! - The Centre (the clock, `notifications.toggle`) is a 380 × 416 Glass/Thick panel (radius 24):
//!   "Notifications" with "Clear all", the sections Today and Earlier, opaque cards (radius 16)
//!   with the time, the actions and the reply field; an app's notifications fold into a stack
//!   of plates, a click expands it (with Collapse and Clear). It opens and closes with the
//!   measured motion; Esc or a click outside closes it.
//! - Per-app settings (`notifications.apps.<app>.style|showInCenter|sound|previews|grouping`)
//!   apply.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use futures_lite::StreamExt;
use vetro_lib::amalfi::fields::{FieldKind, TextField};
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
    Button, Event, EventCx, Handled, LayoutCx, PaintCx, Prop, Proposal, Signal, Ui, Value, ViewId,
    WindowId, WindowKind,
};
use vetro_lib::geom::{Insets, Point, Rect, Size};
use vetro_lib::services::settings::{SettingValue, SettingsMap};
use vetro_lib::services::types::Notification;
use vetro_lib::shape::Shape;
use vetro_lib::style::{Border, Style};
use vetro_lib::text::Align;

use crate::apps;
use crate::text::lay;

/// Measures (kit `Notifications/*`).
pub mod metrics {
    /// The banner column.
    pub const COLUMN: f32 = 360.0;
    /// Its margin from the right edge and below the menu bar.
    pub const MARGIN: f32 = 16.0;
    /// The menu bar.
    pub const MENU_BAR: f32 = 32.0;
    /// A banner's corner radius.
    pub const RADIUS: f32 = 18.0;
    /// The app icon.
    pub const ICON: f32 = 36.0;
    /// The card's padding.
    pub const PADDING: f32 = 12.0;
    /// The text's left edge.
    pub const TEXT_X: f32 = 56.0;
    /// The body ends this far from the banner's right edge (room for the time).
    pub const BODY_RIGHT: f32 = 40.0;
    /// The content: a title and two lines of body.
    pub const CONTENT: f32 = 52.0;
    /// Actions: small push buttons 24 tall, 8 under the content, 8 apart.
    pub const ACTION_HEIGHT: f32 = 24.0;
    /// See [`ACTION_HEIGHT`].
    pub const ACTION_GAP: f32 = 8.0;
    /// The Centre: 380 wide, at most 416 tall, radius 24, 16 inset; cards radius 16.
    pub const CENTRE: f32 = 380.0;
    /// See [`CENTRE`].
    pub const CENTRE_MAX: f32 = 416.0;
    /// See [`CENTRE`].
    pub const CENTRE_RADIUS: f32 = 24.0;
    /// See [`CENTRE`].
    pub const INSET: f32 = 16.0;
    /// See [`CENTRE`].
    pub const CARD_RADIUS: f32 = 16.0;
    /// Gap between groups.
    pub const CARD_GAP: f32 = 8.0;
    /// Gap between the cards of an expanded group.
    pub const EXPANDED_GAP: f32 = 6.0;
    /// A section label's height.
    pub const SECTION: f32 = 16.0;
    /// The plates of a collapsed group: insets and step.
    pub const PLATES: [f32; 2] = [16.0, 32.0];
    /// See [`PLATES`].
    pub const PLATE_STEP: f32 = 10.0;
    /// The close button on hover: 20 pt, centred at (4, 6) of the card.
    pub const CLOSE: f32 = 20.0;
    /// The "Details" capsule.
    pub const DETAILS: (f32, f32) = (64.0, 24.0);
    /// The reply field.
    pub const REPLY: f32 = 28.0;
}

const REPLY_ACTION: &str = "inline-reply";

/// Banner motion (notifications.md "Banner behaviour", scaled to the Amalfi banner).
const ENTER_TAU: f64 = 0.071;
const ENTER_DISTANCE: f32 = 360.0 + 16.0;
const FADE_IN: f64 = 0.21;
const STAY: f64 = 5.0;
const LINGER: f64 = 1.1;
const EXIT: f64 = 0.19;
const EXIT_STEPS: [f32; 15] = [
    0.0, 37.0, 66.0, 104.0, 141.0, 175.0, 207.0, 234.0, 264.0, 287.0, 305.0, 321.0, 335.0, 346.0,
    356.0,
];
/// The Centre's motion.
const CENTRE_DISTANCE: f32 = 380.0 + 16.0;
const CENTRE_OPEN: f64 = 0.235;
const CENTRE_CLOSE: f64 = 0.300;
const CENTRE_CLOSE_STEPS: [(f32, f32); 7] = [
    (0.0, 0.0),
    (60.0, 10.0),
    (120.0, 37.0),
    (135.0, 104.0),
    (150.0, 357.0),
    (200.0, 380.0),
    (300.0, 387.0),
];

/// The banner's offset to the right `t` seconds into its entrance.
pub fn enter_offset(t: f64) -> f32 {
    if t <= 0.0 {
        return ENTER_DISTANCE;
    }
    let x = ENTER_DISTANCE * (-t / ENTER_TAU).exp() as f32;
    if x < 0.5 { 0.0 } else { x }
}

/// The banner's offset `t` seconds into its exit.
pub fn exit_offset(t: f64) -> f32 {
    let step = (t * 75.0) as f32;
    if step <= 0.0 {
        return 0.0;
    }
    let i = step.floor() as usize;
    let k = ENTER_DISTANCE / (344.0 + 16.0);
    match (EXIT_STEPS.get(i), EXIT_STEPS.get(i + 1)) {
        (Some(a), Some(b)) => k * (a + (b - a) * (step - i as f32)),
        _ => ENTER_DISTANCE,
    }
}

/// The Centre's offset `t` seconds into its opening.
pub fn centre_open_offset(t: f64) -> f32 {
    if t <= 0.0 {
        return CENTRE_DISTANCE;
    }
    let ms = (t * 1000.0) as f32;
    let k = CENTRE_DISTANCE / 387.0;
    let x = k * (347.0 * (-ms / 12.0).exp() + 40.0 * (-ms / 70.0).exp());
    if t >= CENTRE_OPEN || x < 0.5 { 0.0 } else { x }
}

/// The Centre's offset `t` seconds into its closing.
pub fn centre_close_offset(t: f64) -> f32 {
    let ms = (t * 1000.0) as f32;
    if ms <= 0.0 {
        return 0.0;
    }
    let k = CENTRE_DISTANCE / 387.0;
    for w in CENTRE_CLOSE_STEPS.windows(2) {
        if let [(t0, x0), (t1, x1)] = w
            && ms <= *t1
        {
            return k * (x0 + (x1 - x0) * (ms - t0) / (t1 - t0));
        }
    }
    CENTRE_DISTANCE
}

/// An app's banner style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerStyle {
    /// The banner leaves by itself.
    Banners,
    /// It stays until acted on.
    Alerts,
    /// No banner.
    None,
}

fn group_of(n: &Notification) -> String {
    if n.desktop_entry.is_empty() {
        n.app_name.clone()
    } else {
        n.desktop_entry.clone()
    }
}

fn app_setting<'a>(s: &'a SettingsMap, n: &Notification, field: &str) -> Option<&'a SettingValue> {
    s.get(&format!("notifications.apps.{}.{field}", group_of(n)))
        .or_else(|| s.get(&format!("notifications.apps.{}.{field}", n.app_name)))
}

/// The style of `n`'s app.
pub fn style_of(s: &SettingsMap, n: &Notification) -> BannerStyle {
    match app_setting(s, n, "style").and_then(SettingValue::as_text) {
        Some("alerts") => BannerStyle::Alerts,
        Some("none") => BannerStyle::None,
        _ => BannerStyle::Banners,
    }
}

fn in_centre(s: &SettingsMap, n: &Notification) -> bool {
    !n.transient && app_setting(s, n, "showInCenter").and_then(SettingValue::as_bool) != Some(false)
}

fn sound_of(s: &SettingsMap, n: &Notification) -> bool {
    app_setting(s, n, "sound").and_then(SettingValue::as_bool) != Some(false)
}

fn by_default(s: &SettingsMap, n: &Notification, field: &str, global: &str, value: &str) -> bool {
    let app = app_setting(s, n, field).and_then(SettingValue::as_text);
    let all = s.get(global).and_then(SettingValue::as_text);
    match app {
        Some(v) if v != "default" => v == value,
        _ => all == Some(value),
    }
}

/// The buttons of `n`: its actions but the default and the reply.
pub fn buttons(n: &Notification) -> Vec<(String, String)> {
    n.actions
        .chunks(2)
        .filter_map(|c| match c {
            [k, l] if k != "default" && k != REPLY_ACTION => Some((k.clone(), l.clone())),
            _ => None,
        })
        .collect()
}

fn can_reply(n: &Notification) -> bool {
    n.actions
        .chunks(2)
        .any(|c| c.first().is_some_and(|k| k == REPLY_ACTION))
}

fn reply_label(n: &Notification) -> String {
    n.actions
        .chunks(2)
        .find_map(|c| match c {
            [k, l] if k == REPLY_ACTION && !l.is_empty() => Some(l.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Reply".into())
}

/// The Centre's groups: by app, the most recent first (an app that does not group: one each).
pub fn groups(list: &[Notification], settings: &SettingsMap) -> Vec<(String, Vec<Notification>)> {
    let mut sorted: Vec<&Notification> = list.iter().filter(|n| in_centre(settings, n)).collect();
    sorted.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms).then(b.id.cmp(&a.id)));
    let mut out: Vec<(String, Vec<Notification>)> = Vec::new();
    for n in sorted {
        let key = if by_default(settings, n, "grouping", "notifications.grouping", "off") {
            format!("{}#{}", group_of(n), n.id)
        } else {
            group_of(n)
        };
        match out.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(n.clone()),
            None => out.push((key, vec![n.clone()])),
        }
    }
    out
}

/// `now` within the last minute, otherwise `HH:MM`.
fn time_label(ms: u64) -> String {
    let now = crate::time::unix_now();
    let secs = (ms / 1000) as i64;
    if now - secs < 60 {
        return "now".into();
    }
    let t = crate::time::local_at(secs);
    format!("{:02}:{:02}", t.hour, t.minute)
}

/// Runs with the core.
pub type Callback = Rc<dyn Fn(&mut Ui)>;

/// A banner on screen.
#[derive(Debug, Clone)]
struct Banner {
    n: Notification,
    since: f64,
    leaving: Option<f64>,
    hovered: bool,
    left_at: Option<f64>,
    drag: f32,
    expanded: bool,
    persistent: bool,
}

/// The notifications' state.
#[derive(Default)]
pub struct NotesState {
    /// The notifications.
    pub list: Vec<Notification>,
    banners: Vec<Banner>,
    surface: Option<(WindowId, ViewId)>,
    /// Do Not Disturb.
    pub dnd: bool,
    /// The settings.
    pub settings: SettingsMap,
    centre: Option<(WindowId, ViewId)>,
    centre_since: f64,
    centre_closing: Option<f64>,
    expanded: Vec<String>,
    output: Size,
    on_centre_closed: Option<Callback>,
}

/// Shared.
pub type Shared = Rc<RefCell<NotesState>>;

fn with_proxy<F, Fut>(f: F)
where
    F: FnOnce(vetro_lib::services::Notifications1Proxy<'static>) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    vetro_lib::platform::spawn(async move {
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.notifications().await
        {
            f(p).await;
        }
    });
}

fn dismiss(id: u32) {
    with_proxy(move |p| async move {
        let _ = p.dismiss(id).await;
    });
}

fn invoke(id: u32, key: String) {
    with_proxy(move |p| async move {
        let _ = p.invoke_action(id, &key).await;
    });
}

fn reply(id: u32, text: String) {
    with_proxy(move |p| async move {
        let _ = p.reply(id, &text).await;
    });
}

fn clear_all() {
    with_proxy(|p| async move {
        let _ = p.clear().await;
    });
}

/// A click: the default action, otherwise the sender app opens.
fn activate(ui: &mut Ui, n: &Notification) {
    if n.actions
        .chunks(2)
        .any(|c| c.first().is_some_and(|k| k == "default"))
    {
        invoke(n.id, "default".into());
        return;
    }
    let id = if !n.desktop_entry.is_empty() {
        Some(n.desktop_entry.clone())
    } else {
        apps::all()
            .into_iter()
            .find(|e| e.name == n.app_name)
            .map(|e| e.id)
    };
    if let Some(id) = id {
        let params = serde_json::json!({ "app": id }).to_string();
        ui.shell(ShellRequest::ExecuteWith("app.launch".into(), params));
    }
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
                repaint_centre(ui, &s);
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
                    let (id, s) = (a.id, s2.clone());
                    vetro_lib::platform::post(move |ui| on_removed(ui, &s, id));
                }
            }
        };
        let additions = async move {
            while let Some(sig) = added.next().await {
                if let Ok(a) = sig.args() {
                    let (n, s) = (a.notification, st.clone());
                    vetro_lib::platform::post(move |ui| on_added(ui, &s, n));
                }
            }
        };
        futures_lite::future::zip(additions, removals).await;
    });
}

/// A notification arrived.
pub fn on_added(ui: &mut Ui, state: &Shared, mut n: Notification) {
    let now = ui.now;
    {
        let mut s = state.borrow_mut();
        s.list.retain(|x| x.id != n.id);
        s.list.push(n.clone());
        let style = style_of(&s.settings, &n);
        // With Do Not Disturb it waits in the Centre, silently (SH-NC-07).
        if s.dnd || s.centre.is_some() || style == BannerStyle::None {
            drop(s);
            repaint_centre(ui, state);
            return;
        }
        if sound_of(&s.settings, &n) {
            vetro_lib::platform::spawn(async {
                if let Some(sv) = vetro_lib::platform::services()
                    && let Ok(p) = sv.sound().await
                {
                    let _ = p.play_alert("").await;
                }
            });
        }
        if by_default(
            &s.settings,
            &n,
            "previews",
            "notifications.previews",
            "never",
        ) {
            n.body.clear();
        }
        // The same notification again: its content changes in place.
        if let Some(b) = s
            .banners
            .iter_mut()
            .find(|b| b.leaving.is_none() && b.n.id == n.id)
        {
            b.n = n;
        } else {
            for b in s.banners.iter_mut().filter(|b| b.leaving.is_none()) {
                b.leaving = Some(now);
            }
            let persistent = style == BannerStyle::Alerts || n.urgency >= 2;
            s.banners.push(Banner {
                n,
                since: now,
                leaving: None,
                hovered: false,
                left_at: None,
                drag: 0.0,
                expanded: false,
                persistent,
            });
        }
    }
    show_banner_surface(ui, state);
    repaint_centre(ui, state);
}

/// A notification went (closed elsewhere: its banner leaves).
pub fn on_removed(ui: &mut Ui, state: &Shared, id: u32) {
    let now = ui.now;
    {
        let mut s = state.borrow_mut();
        s.list.retain(|x| x.id != id);
        for b in s
            .banners
            .iter_mut()
            .filter(|b| b.n.id == id && b.leaving.is_none())
        {
            b.leaving = Some(now);
        }
    }
    animate_banner(ui, state);
    repaint_centre(ui, state);
}

/// The settings changed.
pub fn settings_changed(state: &Shared, settings: &SettingsMap) {
    state.borrow_mut().settings = settings.clone();
}

fn show_banner_surface(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
        ui.start_animating(v);
        return;
    }
    let size = Size::new(metrics::COLUMN + 2.0 * metrics::MARGIN, 440.0);
    let output = ui.outputs().first().map(|o| o.name.clone());
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Overlay,
            anchor: Edges::TOP | Edges::RIGHT,
            exclusive: -1,
            margin: [0; 4],
            keyboard: KeyboardMode::OnDemand,
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
        hits: Vec::new(),
        press: None,
        reply: None,
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, v);
    ui.show_window(w);
    ui.start_animating(v);
    ui.set_input_region(w, Some(Vec::new()));
    state.borrow_mut().surface = Some((w, v));
}

fn animate_banner(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
        ui.start_animating(v);
    }
}

// ----- Cards ---------------------------------------------------------------------------------

/// What can be pressed.
#[derive(Debug, Clone, PartialEq)]
enum Hit {
    /// A card.
    Card(u32),
    /// Its close button.
    Close(u32),
    /// "Details".
    Details(u32),
    /// An action.
    Action(u32, String),
    /// "Clear all".
    ClearAll,
    /// A collapsed group (expands).
    Group(String),
    /// "Collapse".
    Collapse(String),
    /// "Clear" of a group.
    ClearGroup(Vec<u32>),
}

/// How a card is drawn.
#[derive(Debug, Clone, Copy)]
struct CardLook {
    /// In the Centre: opaque, radius 16, with the time.
    panel: bool,
    actions: bool,
    body_lines: usize,
    close: bool,
    details: bool,
}

fn icon_of(n: &Notification, dark: bool) -> Option<skia_safe::Image> {
    if !n.image_path.is_empty()
        && let Some(i) = apps::icon_named(n.image_path.trim_start_matches("file://"))
    {
        return Some(i);
    }
    if !n.desktop_entry.is_empty()
        && let Some(e) = apps::entry(&n.desktop_entry)
        && let Some(i) = apps::icon(&e, dark)
    {
        return Some(i);
    }
    apps::icon_named(&n.app_icon)
}

/// Where and how a card is drawn, and what it records.
struct CardCx<'a> {
    labels: &'a mut Vec<Label>,
    /// The next label slot.
    li: usize,
    hits: Vec<(Rect, Hit)>,
}

/// Paints (or with `dry`, only measures) the card of `n` at `at`, `width` wide; returns its
/// height and the rect of its reply field (when it takes one and the actions show).
fn paint_card(
    cx: &mut PaintCx<'_>,
    cc: &mut CardCx<'_>,
    at: Point,
    width: f32,
    n: &Notification,
    look: CardLook,
    dry: bool,
) -> (f32, Option<Rect>) {
    let theme = *cx.theme();
    let c = theme.colors;
    let dark = theme.dark();
    let pad = metrics::PADDING;
    let title_style = theme.text(&typography::HEADLINE, c.text_primary);
    let body_style = theme.text(&typography::CALLOUT, c.text_secondary);
    let time_style = theme.text(&typography::CAPTION, c.text_tertiary);
    let details_room = if look.details {
        12.0 + metrics::DETAILS.0 + 8.0
    } else {
        pad
    };
    let text_w = width - metrics::TEXT_X - details_room;
    let title = if n.summary.is_empty() {
        n.app_name.clone()
    } else {
        n.summary.clone()
    };
    let base = cc.li;
    let time = look.panel.then(|| time_label(n.timestamp_ms));
    let time_w = match &time {
        Some(t) => {
            if cc.labels.len() <= base + 2 {
                cc.labels.resize_with(base + 3, Label::default);
            }
            cc.labels
                .get_mut(base + 2)
                .map_or(0.0, |l| l.width(t, &time_style))
                + 8.0
        }
        None => 0.0,
    };
    let body_w = if look.panel {
        text_w
    } else {
        text_w.min(width - metrics::TEXT_X - metrics::BODY_RIGHT)
    };
    let tl = lay(
        cc.labels,
        base,
        &title,
        &title_style,
        Align::Start,
        text_w - time_w,
        Some(1),
    );
    let bl = (!n.body.is_empty()).then(|| {
        lay(
            cc.labels,
            base + 1,
            &n.body,
            &body_style,
            Align::Start,
            body_w,
            Some(look.body_lines),
        )
    });
    let content = (18.0 + bl.as_ref().map_or(0.0, |b| b.size().height)).max(metrics::CONTENT);
    let buttons = if look.actions { buttons(n) } else { Vec::new() };
    let reply = look.actions && can_reply(n);
    let with_actions = reply || !buttons.is_empty();
    let mut h = pad + content + if with_actions { 0.0 } else { pad };
    let reply_rect = reply.then(|| {
        let r = Rect::new(
            at.x + metrics::TEXT_X,
            at.y + h + 8.0,
            width - metrics::TEXT_X - pad,
            metrics::REPLY,
        );
        h += 8.0 + metrics::REPLY;
        r
    });
    let actions_y = h + 8.0;
    if !buttons.is_empty() {
        h += 8.0 + metrics::ACTION_HEIGHT + pad;
    } else if reply {
        h += pad;
    }
    cc.li = base + 4 + buttons.len();
    if dry {
        return (h, reply_rect);
    }
    let card = Rect::new(at.x, at.y, width, h);
    if look.panel {
        cx.rec.layer(
            "Card",
            card,
            Shape::rounded(metrics::CARD_RADIUS),
            Style::fill(c.surface_raised),
        );
    } else {
        cx.glass(
            "Card",
            card,
            Shape::rounded(metrics::RADIUS),
            "Glass/Regular",
            GlassProfile::Banner,
        );
    }
    cc.hits.push((card, Hit::Card(n.id)));
    let ir = Rect::new(at.x + pad, at.y + pad, metrics::ICON, metrics::ICON);
    match icon_of(n, dark) {
        Some(img) => cx.rec.image("App icon", ir, img, 1.0),
        None => {
            // A tile with the initial.
            cx.rec.layer(
                "App icon",
                ir,
                Shape::rounded(8.0),
                Style::fill(c.accent_amalfi),
            );
            let initial: String = n
                .app_name
                .chars()
                .take(1)
                .collect::<String>()
                .to_uppercase();
            let st = theme.text(&typography::HEADLINE, Color::WHITE);
            let l = lay(
                cc.labels,
                base + 3,
                &initial,
                &st,
                Align::Center,
                metrics::ICON,
                Some(1),
            );
            let lh = l.size().height;
            cx.rec.text(
                "Initial",
                Rect::new(ir.x, ir.y + (metrics::ICON - lh) / 2.0, metrics::ICON, lh),
                l,
                None,
                Color::WHITE,
            );
        }
    }
    cx.rec.text(
        "Title",
        Rect::new(at.x + metrics::TEXT_X, at.y + pad, text_w - time_w, 18.0),
        tl,
        Some(typography::HEADLINE.name),
        c.text_primary,
    );
    if let Some(t) = &time {
        let l = lay(
            cc.labels,
            base + 2,
            t,
            &time_style,
            Align::End,
            time_w,
            Some(1),
        );
        cx.rec.text(
            "Time",
            Rect::new(
                at.x + metrics::TEXT_X + text_w - time_w,
                at.y + pad + 2.0,
                time_w,
                14.0,
            ),
            l,
            None,
            c.text_tertiary,
        );
    }
    if let Some(b) = bl {
        let bh = b.size().height;
        cx.rec.text(
            "Body",
            Rect::new(at.x + metrics::TEXT_X, at.y + pad + 18.0, body_w, bh),
            b,
            Some(typography::CALLOUT.name),
            c.text_secondary,
        );
    }
    // The actions: small secondary push buttons.
    let small = theme.text(&typography::CALLOUT, c.text_primary);
    let mut x = at.x + metrics::TEXT_X;
    for (k, (key, label)) in buttons.iter().enumerate() {
        let slot = base + 4 + k;
        if cc.labels.len() <= slot {
            cc.labels.resize_with(slot + 1, Label::default);
        }
        let w = cc
            .labels
            .get_mut(slot)
            .map_or(40.0, |l| l.width(label, &small))
            .max(32.0)
            + 24.0;
        let r = Rect::new(x, at.y + actions_y, w, metrics::ACTION_HEIGHT);
        cx.rec.layer(
            "Action",
            r,
            Shape::rounded(6.0),
            Style::fill(c.surface_control).with_border(Border::inside(c.line_border, 1.0)),
        );
        let l = lay(cc.labels, slot, label, &small, Align::Center, w, Some(1));
        let lh = l.size().height;
        cx.rec.text(
            "Action label",
            Rect::new(r.x, r.y + (r.height - lh) / 2.0, w, lh),
            l,
            None,
            c.text_primary,
        );
        cc.hits.push((r, Hit::Action(n.id, key.clone())));
        x += w + metrics::ACTION_GAP;
    }
    if look.details {
        let (dw, dh) = metrics::DETAILS;
        let r = Rect::new(
            card.right() - 12.0 - dw,
            card.y + (card.height - dh) / 2.0,
            dw,
            dh,
        );
        cx.rec.layer(
            "Details",
            r,
            Shape::rounded(dh / 2.0),
            Style::fill(c.surface_control),
        );
        let l = lay(
            cc.labels,
            990,
            "Details",
            &small,
            Align::Center,
            dw,
            Some(1),
        );
        let lh = l.size().height;
        cx.rec.text(
            "Details label",
            Rect::new(r.x, r.y + (dh - lh) / 2.0, dw, lh),
            l,
            None,
            c.text_primary,
        );
        cc.hits.push((r, Hit::Details(n.id)));
    }
    if look.close {
        let s = metrics::CLOSE;
        let r = Rect::new(card.x + 4.0 - s / 2.0, card.y + 6.0 - s / 2.0, s, s);
        cx.rec.layer(
            "Close",
            r,
            Shape::Ellipse,
            Style::fill(c.surface_control).with_border(Border::inside(c.line_border, 1.0)),
        );
        cx.rec.glyph(
            "Close glyph",
            r.inset(Insets::all(5.0)),
            "xmark",
            600,
            c.text_secondary,
        );
        cc.hits.push((r, Hit::Close(n.id)));
    }
    (h, reply_rect)
}

/// Makes or keeps the reply field of `n` in view `parent`; `on_sent` runs after a reply.
fn reply_field(
    ui: &mut Ui,
    parent: ViewId,
    existing: Option<ViewId>,
    n: &Notification,
    on_sent: Callback,
) -> ViewId {
    if let Some(v) = existing
        && ui.tree.contains(v)
    {
        return v;
    }
    let f = ui.create(TextField::new(FieldKind::Text));
    let _ = ui.set(f, Prop::Placeholder, Value::from(reply_label(n).as_str()));
    ui.add_child(parent, f);
    let id = n.id;
    ui.connect(f, Signal::Submitted, move |ui, v, _| {
        let text = ui
            .get(v, Prop::Text)
            .and_then(|t| t.as_str().map(str::to_string))
            .unwrap_or_default();
        if !text.trim().is_empty() {
            reply(id, text);
            let _ = ui.set(v, Prop::Text, Value::Str(String::new()));
            on_sent(ui);
        }
    });
    f
}

/// Keeps the reply fields of a view in step with the cards that show one.
fn sync_replies(
    ui: &mut Ui,
    view: ViewId,
    old: HashMap<u32, (ViewId, Rect)>,
    wanted: Vec<(Notification, Rect)>,
    on_sent: impl Fn(u32) -> Callback,
) -> HashMap<u32, (ViewId, Rect)> {
    let mut keep = HashMap::new();
    for (n, r) in wanted {
        let v = reply_field(ui, view, old.get(&n.id).map(|x| x.0), &n, on_sent(n.id));
        keep.insert(n.id, (v, r));
    }
    for (id, (v, _)) in old {
        if !keep.contains_key(&id) {
            ui.destroy(v);
        }
    }
    ui.request_layout(view);
    keep
}

// ----- Banners -------------------------------------------------------------------------------

/// The banner column.
struct BannerView {
    state: Shared,
    labels: Vec<Label>,
    hits: Vec<(Rect, Hit)>,
    press: Option<(Point, u32)>,
    reply: Option<HashMap<u32, (ViewId, Rect)>>,
}

fn motion(b: &Banner, now: f64) -> (f32, f32) {
    let t = now - b.since;
    let (exit_x, exit_a) = match b.leaving {
        Some(l) => (
            exit_offset(now - l),
            1.0 - 0.5 * ((now - l) / EXIT).clamp(0.0, 1.0) as f32,
        ),
        None => (0.0, 1.0),
    };
    (
        enter_offset(t) + b.drag + exit_x,
        (t / FADE_IN).clamp(0.0, 1.0) as f32 * exit_a,
    )
}

fn leave(s: &mut NotesState, id: u32, now: f64) {
    for b in s
        .banners
        .iter_mut()
        .filter(|b| b.n.id == id && b.leaving.is_none())
    {
        b.leaving = Some(now);
    }
}

impl vetro_lib::core::Widget for BannerView {
    fn kind(&self) -> &'static str {
        "Banner"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(392.0, 440.0))
    }

    fn place(&mut self, cx: &mut LayoutCx<'_>, _size: Size) {
        for (v, r) in self.reply.iter().flat_map(|m| m.values()) {
            cx.place(*v, *r);
        }
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        let gone = {
            let mut s = self.state.borrow_mut();
            for b in s.banners.iter_mut().filter(|b| b.leaving.is_none()) {
                let expired = if b.hovered || b.expanded || b.persistent {
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
            s.banners
                .retain(|b| !b.leaving.is_some_and(|l| now - l >= EXIT));
            s.banners.is_empty()
        };
        cx.request_paint();
        if gone {
            let w = self.state.borrow_mut().surface.take();
            if let Some((w, _)) = w {
                cx.ui().close_window(w);
            }
            return false;
        }
        true
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let now = cx.now();
        let b = cx.bounds();
        let st = self.state.clone();
        let s = st.borrow();
        let mut cc = CardCx {
            labels: &mut self.labels,
            li: 0,
            hits: Vec::new(),
        };
        let mut region = Vec::new();
        let mut wanted = Vec::new();
        for banner in &s.banners {
            let (dx, alpha) = motion(banner, now);
            let at = Point::new(
                b.x + metrics::MARGIN + dx,
                b.y + metrics::MENU_BAR + metrics::MARGIN,
            );
            let exiting = banner.leaving.is_some();
            let look = CardLook {
                panel: false,
                actions: banner.expanded,
                body_lines: if banner.expanded { 6 } else { 2 },
                close: (banner.hovered || banner.persistent) && !exiting,
                details: banner.hovered && !exiting && !banner.expanded,
            };
            cx.rec.push_opacity(alpha);
            let (h, reply) = paint_card(cx, &mut cc, at, metrics::COLUMN, &banner.n, look, false);
            cx.rec.pop();
            if !exiting {
                region.push(Rect::new(
                    at.x - b.x - 8.0,
                    at.y - b.y - 8.0,
                    metrics::COLUMN + 16.0,
                    h + 16.0,
                ));
                if let Some(r) = reply {
                    wanted.push((banner.n.clone(), r.translate(Point::new(-b.x, -b.y))));
                }
            }
        }
        self.hits = cc.hits;
        let surface = s.surface;
        drop(s);
        let Some((w, _)) = surface else { return };
        let view = cx.id();
        let old = self.reply.take().unwrap_or_default();
        let st2 = st.clone();
        vetro_lib::platform::post(move |ui| {
            ui.set_input_region(w, Some(region));
            let keep = sync_replies(ui, view, old, wanted, |id| {
                let st3 = st2.clone();
                Rc::new(move |ui: &mut Ui| {
                    let now = ui.now;
                    leave(&mut st3.borrow_mut(), id, now);
                    animate_banner(ui, &st3);
                })
            });
            ui.with_widget::<BannerView, _>(view, |bv| bv.reply = Some(keep));
        });
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let now = cx.now();
        let hit = |hits: &[(Rect, Hit)], p: Point| {
            hits.iter()
                .rev()
                .find(|(r, _)| r.contains(p))
                .map(|x| x.1.clone())
        };
        match event {
            Event::PointerMove { pos, .. } => {
                let over = hit(&self.hits, *pos);
                let press = self.press;
                let mut s = self.state.borrow_mut();
                for b in s.banners.iter_mut().filter(|b| b.leaving.is_none()) {
                    let on = matches!(&over, Some(Hit::Card(id) | Hit::Close(id) | Hit::Details(id) | Hit::Action(id, _)) if *id == b.n.id);
                    if on != b.hovered {
                        b.hovered = on;
                        b.left_at = (!on).then_some(now);
                    }
                    if let Some((p0, id)) = press
                        && id == b.n.id
                    {
                        b.drag = (pos.x - p0.x).max(0.0);
                    }
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerLeave => {
                for b in self
                    .state
                    .borrow_mut()
                    .banners
                    .iter_mut()
                    .filter(|b| b.hovered)
                {
                    b.hovered = false;
                    b.left_at = Some(now);
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                if let Some(Hit::Card(id)) = hit(&self.hits, *pos) {
                    self.press = Some((*pos, id));
                    // A long press opens the actions, like "Details".
                    let st = self.state.clone();
                    let view = cx.id();
                    cx.ui().after(0.5, move |ui| {
                        let held = ui
                            .with_widget::<BannerView, _>(view, |v| {
                                v.press.is_some_and(|p| p.1 == id)
                            })
                            .unwrap_or(false);
                        if !held {
                            return;
                        }
                        if let Some(b) = st
                            .borrow_mut()
                            .banners
                            .iter_mut()
                            .find(|b| b.n.id == id && b.drag < 5.0)
                        {
                            b.expanded = true;
                        }
                        ui.with_widget::<BannerView, _>(view, |v| v.press = None);
                        animate_banner(ui, &st);
                    });
                }
                Handled::Yes
            }
            Event::PointerUp {
                pos,
                button: Button::Left,
                ..
            } => {
                let press = self.press.take();
                let target = hit(&self.hits, *pos);
                let st = self.state.clone();
                // A drag to the right sends it away (it stays in the Centre).
                if let Some((p0, id)) = press
                    && pos.x - p0.x >= 15.0
                {
                    leave(&mut st.borrow_mut(), id, now);
                    cx.animate();
                    return Handled::Yes;
                }
                match target {
                    Some(Hit::Close(id)) => {
                        dismiss(id);
                        leave(&mut st.borrow_mut(), id, now);
                    }
                    Some(Hit::Details(id)) => {
                        if let Some(b) = st.borrow_mut().banners.iter_mut().find(|b| b.n.id == id) {
                            b.expanded = true;
                        }
                    }
                    Some(Hit::Action(id, key)) => {
                        invoke(id, key);
                        leave(&mut st.borrow_mut(), id, now);
                    }
                    Some(Hit::Card(id)) if press.is_some_and(|p| p.1 == id) => {
                        let n = st
                            .borrow()
                            .banners
                            .iter()
                            .find(|b| b.n.id == id && !b.expanded)
                            .map(|b| b.n.clone());
                        if let Some(n) = n {
                            activate(cx.ui(), &n);
                            leave(&mut st.borrow_mut(), id, now);
                        }
                    }
                    _ => {}
                }
                for b in st
                    .borrow_mut()
                    .banners
                    .iter_mut()
                    .filter(|b| b.leaving.is_none())
                {
                    b.drag = 0.0;
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
            .banners
            .last()
            .map(|b| b.n.summary.clone());
    }
}

// ----- The Centre ----------------------------------------------------------------------------

/// Opens or closes the Centre.
pub fn toggle_center(ui: &mut Ui, state: &Shared, closed: Option<Callback>) {
    let now = ui.now;
    if state.borrow().centre.is_some() {
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
    let v = ui.create(CentreView {
        state: state.clone(),
        labels: Vec::new(),
        hits: Vec::new(),
        scroll: 0.0,
        content: 0.0,
        hovered: None,
        replies: HashMap::new(),
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, v);
    ui.show_window(w);
    ui.set_focus(v, false);
    ui.start_animating(v);
    {
        let mut s = state.borrow_mut();
        s.centre = Some((w, v));
        s.centre_since = now;
        s.centre_closing = None;
        s.output = out;
        s.on_centre_closed = closed;
        for b in s.banners.iter_mut().filter(|b| b.leaving.is_none()) {
            b.leaving = Some(now);
        }
    }
    animate_banner(ui, state);
}

/// Closes the Centre (it slides out first).
pub fn close_center(ui: &mut Ui, state: &Shared) {
    let now = ui.now;
    let v = {
        let mut s = state.borrow_mut();
        if s.centre_closing.is_none() {
            s.centre_closing = Some(now);
        }
        s.centre.map(|c| c.1)
    };
    if let Some(v) = v {
        ui.start_animating(v);
    }
}

fn repaint_centre(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().centre {
        ui.request_paint(v);
    }
}

/// The Centre.
struct CentreView {
    state: Shared,
    labels: Vec<Label>,
    hits: Vec<(Rect, Hit)>,
    scroll: f32,
    content: f32,
    hovered: Option<u32>,
    replies: HashMap<u32, (ViewId, Rect)>,
}

const CARD_LOOK: CardLook = CardLook {
    panel: true,
    actions: true,
    body_lines: 4,
    close: false,
    details: false,
};

impl CentreView {
    fn pose(s: &NotesState, now: f64) -> (f32, f32) {
        let x = match s.centre_closing {
            Some(c) => centre_close_offset(now - c),
            None => centre_open_offset(now - s.centre_since),
        };
        (x, (1.0 - x / CENTRE_DISTANCE).clamp(0.0, 1.0))
    }

    /// The groups split into Today and Earlier (by the latest notification).
    fn sections(gs: &[(String, Vec<Notification>)]) -> [(&'static str, Vec<usize>); 2] {
        let midnight = crate::time::midnight_unix() * 1000;
        let today = |i: &usize| {
            gs.get(*i)
                .and_then(|g| g.1.first())
                .is_some_and(|n| n.timestamp_ms as i64 >= midnight)
        };
        let all: Vec<usize> = (0..gs.len()).collect();
        [
            ("Today", all.iter().copied().filter(today).collect()),
            (
                "Earlier",
                all.iter().copied().filter(|i| !today(i)).collect(),
            ),
        ]
    }
}

impl vetro_lib::core::Widget for CentreView {
    fn kind(&self) -> &'static str {
        "NotificationCentre"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(self.state.borrow().output)
    }

    fn place(&mut self, cx: &mut LayoutCx<'_>, _size: Size) {
        for (v, r) in self.replies.values() {
            cx.place(*v, *r);
        }
    }

    fn focus(&self) -> FocusKind {
        FocusKind::Always
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        cx.request_paint();
        let (closing, since) = {
            let s = self.state.borrow();
            (s.centre_closing, s.centre_since)
        };
        if let Some(c) = closing
            && now - c >= CENTRE_CLOSE
        {
            let (w, closed) = {
                let mut s = self.state.borrow_mut();
                s.centre_closing = None;
                (s.centre.take().map(|c| c.0), s.on_centre_closed.take())
            };
            if let Some(w) = w {
                cx.ui().close_window(w);
            }
            if let Some(f) = closed {
                f(cx.ui());
            }
            return false;
        }
        closing.is_some() || now - since < CENTRE_OPEN
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let now = cx.now();
        let b = cx.bounds();
        let theme = *cx.theme();
        let c = theme.colors;
        let st = self.state.clone();
        let s = st.borrow();
        let (dx, alpha) = Self::pose(&s, now);
        let left = b.right() - metrics::MARGIN - metrics::CENTRE + dx;
        let top = b.y + metrics::MENU_BAR + metrics::MARGIN;
        let gs = groups(&s.list, &s.settings);
        let inner_w = metrics::CENTRE - 2.0 * metrics::INSET;
        let header_h = metrics::INSET + 24.0 + 8.0;
        let sections = Self::sections(&gs);
        // The content's height: a dry run of the layout.
        let mut cc = CardCx {
            labels: &mut self.labels,
            li: 0,
            hits: Vec::new(),
        };
        let mut body_h = 0.0;
        if gs.is_empty() {
            body_h = 24.0 + 18.0 + 48.0;
        } else {
            for (_, idx) in &sections {
                if idx.is_empty() {
                    continue;
                }
                body_h += metrics::SECTION + metrics::CARD_GAP;
                for i in idx {
                    let Some((key, items)) = gs.get(*i) else {
                        continue;
                    };
                    if items.len() == 1 || s.expanded.contains(key) {
                        if items.len() > 1 {
                            body_h += 24.0;
                        }
                        for n in items {
                            body_h += paint_card(
                                cx,
                                &mut cc,
                                Point::new(0.0, 0.0),
                                inner_w,
                                n,
                                CARD_LOOK,
                                true,
                            )
                            .0 + metrics::EXPANDED_GAP;
                        }
                        body_h += metrics::CARD_GAP - metrics::EXPANDED_GAP;
                    } else if let Some(n) = items.first() {
                        let plates = (items.len() - 1).min(2) as f32;
                        body_h += paint_card(
                            cx,
                            &mut cc,
                            Point::new(0.0, 0.0),
                            inner_w,
                            n,
                            CARD_LOOK,
                            true,
                        )
                        .0 + plates * metrics::PLATE_STEP
                            + metrics::CARD_GAP;
                    }
                }
            }
            body_h += metrics::INSET;
        }
        self.content = body_h;
        let panel_h = (header_h + body_h).min(metrics::CENTRE_MAX);
        let panel = Rect::new(left, top, metrics::CENTRE, panel_h);
        cx.rec.push_opacity(alpha);
        cx.glass(
            "Panel",
            panel,
            Shape::rounded(metrics::CENTRE_RADIUS),
            "Glass/Thick",
            GlassProfile::Panel,
        );
        let mut cc = CardCx {
            labels: &mut self.labels,
            li: 0,
            hits: Vec::new(),
        };
        let title = theme.text(&typography::TITLE_3, c.text_primary);
        let l = lay(
            cc.labels,
            1000,
            "Notifications",
            &title,
            Align::Start,
            240.0,
            Some(1),
        );
        cx.rec.text(
            "Title",
            Rect::new(
                panel.x + metrics::INSET + 4.0,
                panel.y + metrics::INSET,
                240.0,
                24.0,
            ),
            l,
            None,
            c.text_primary,
        );
        let link = theme.text(&typography::BODY, c.accent_amalfi);
        let mut extra: Vec<(Rect, Hit)> = Vec::new();
        if !gs.is_empty() {
            let r = Rect::new(
                panel.right() - metrics::INSET - 80.0,
                panel.y + metrics::INSET,
                80.0,
                24.0,
            );
            let l = lay(
                cc.labels,
                1001,
                "Clear all",
                &link,
                Align::End,
                80.0,
                Some(1),
            );
            cx.rec.text(
                "Clear all",
                Rect::new(r.x, r.y + 3.0, r.width, 18.0),
                l,
                None,
                c.accent_amalfi,
            );
            extra.push((r, Hit::ClearAll));
        }
        let mut wanted: Vec<(Notification, Rect)> = Vec::new();
        if gs.is_empty() {
            let es = theme.text(&typography::BODY, c.text_secondary);
            let l = lay(
                cc.labels,
                1002,
                "No Recent Notifications",
                &es,
                Align::Center,
                inner_w,
                Some(1),
            );
            cx.rec.text(
                "Empty",
                Rect::new(
                    panel.x + metrics::INSET,
                    panel.y + header_h + 24.0,
                    inner_w,
                    18.0,
                ),
                l,
                None,
                c.text_secondary,
            );
        } else {
            let clip = Rect::new(
                panel.x,
                panel.y + header_h,
                panel.width,
                panel.height - header_h,
            );
            cx.rec.push_clip(clip, Shape::RECT);
            let x = panel.x + metrics::INSET;
            let mut y = clip.y - self.scroll;
            let section_style = theme.text(&typography::LABEL, c.text_tertiary);
            let head = theme.text(&typography::HEADLINE, c.text_primary);
            let mut li_extra = 1010;
            for (label, idx) in &sections {
                if idx.is_empty() {
                    continue;
                }
                let l = lay(
                    cc.labels,
                    li_extra,
                    label,
                    &section_style,
                    Align::Start,
                    inner_w,
                    Some(1),
                );
                li_extra += 1;
                cx.rec.text(
                    "Section",
                    Rect::new(x, y, inner_w, metrics::SECTION),
                    l,
                    None,
                    c.text_tertiary,
                );
                y += metrics::SECTION + metrics::CARD_GAP;
                for i in idx {
                    let Some((key, items)) = gs.get(*i) else {
                        continue;
                    };
                    if items.len() == 1 || s.expanded.contains(key) {
                        if items.len() > 1 {
                            let name = items
                                .first()
                                .map(|n| n.app_name.clone())
                                .unwrap_or_default();
                            let l = lay(
                                cc.labels,
                                li_extra,
                                &name,
                                &head,
                                Align::Start,
                                inner_w - 140.0,
                                Some(1),
                            );
                            cx.rec.text(
                                "Group",
                                Rect::new(x + 4.0, y + 3.0, inner_w - 140.0, 18.0),
                                l,
                                None,
                                c.text_primary,
                            );
                            let cr = Rect::new(x + inner_w - 130.0, y, 80.0, 24.0);
                            let l = lay(
                                cc.labels,
                                li_extra + 1,
                                "Collapse",
                                &link,
                                Align::End,
                                80.0,
                                Some(1),
                            );
                            cx.rec.text(
                                "Collapse",
                                Rect::new(cr.x, cr.y + 3.0, 80.0, 18.0),
                                l,
                                None,
                                c.accent_amalfi,
                            );
                            extra.push((cr, Hit::Collapse(key.clone())));
                            let kr = Rect::new(x + inner_w - 45.0, y, 45.0, 24.0);
                            let l = lay(
                                cc.labels,
                                li_extra + 2,
                                "Clear",
                                &link,
                                Align::End,
                                45.0,
                                Some(1),
                            );
                            cx.rec.text(
                                "Clear",
                                Rect::new(kr.x, kr.y + 3.0, 45.0, 18.0),
                                l,
                                None,
                                c.accent_amalfi,
                            );
                            extra.push((kr, Hit::ClearGroup(items.iter().map(|n| n.id).collect())));
                            li_extra += 3;
                            y += 24.0;
                        }
                        for n in items {
                            let look = CardLook {
                                close: self.hovered == Some(n.id),
                                ..CARD_LOOK
                            };
                            let (ch, reply) =
                                paint_card(cx, &mut cc, Point::new(x, y), inner_w, n, look, false);
                            if let Some(r) = reply
                                && r.y >= clip.y
                                && r.bottom() <= clip.bottom()
                            {
                                wanted.push((n.clone(), r.translate(Point::new(-b.x, -b.y))));
                            }
                            y += ch + metrics::EXPANDED_GAP;
                        }
                        y += metrics::CARD_GAP - metrics::EXPANDED_GAP;
                    } else if let Some(n) = items.first() {
                        let plates = (items.len() - 1).min(2);
                        let look = CardLook {
                            close: self.hovered == Some(n.id),
                            ..CARD_LOOK
                        };
                        let start = cc.li;
                        let (ch, _) =
                            paint_card(cx, &mut cc, Point::new(x, y), inner_w, n, look, true);
                        cc.li = start;
                        for p in (0..plates).rev() {
                            let inset = metrics::PLATES.get(p).copied().unwrap_or(32.0);
                            let pr = Rect::new(
                                x + inset,
                                y + ch + (p as f32 + 1.0) * metrics::PLATE_STEP - 76.0,
                                inner_w - 2.0 * inset,
                                76.0,
                            );
                            cx.rec.layer(
                                "Plate",
                                pr,
                                Shape::rounded(metrics::CARD_RADIUS),
                                Style::fill(c.surface_raised)
                                    .with_border(Border::inside(c.line_separator, 1.0)),
                            );
                        }
                        let before = cc.hits.len();
                        let _ = paint_card(cx, &mut cc, Point::new(x, y), inner_w, n, look, false);
                        for h in cc.hits.iter_mut().skip(before) {
                            if matches!(h.1, Hit::Card(_)) {
                                h.1 = Hit::Group(key.clone());
                            }
                        }
                        y += ch + plates as f32 * metrics::PLATE_STEP + metrics::CARD_GAP;
                    }
                }
            }
            cx.rec.pop();
        }
        cx.rec.pop();
        let mut hits = cc.hits;
        hits.extend(extra);
        self.hits = hits;
        drop(s);
        // The reply fields of the cards that take one.
        let view = cx.id();
        let old = std::mem::take(&mut self.replies);
        vetro_lib::platform::post(move |ui| {
            let keep = sync_replies(ui, view, old, wanted, |_| Rc::new(|_| {}));
            ui.with_widget::<CentreView, _>(view, |cv| cv.replies = keep);
        });
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        let st = self.state.clone();
        let hit = |hits: &[(Rect, Hit)], p: Point| {
            hits.iter()
                .rev()
                .find(|(r, _)| r.contains(p))
                .map(|x| x.1.clone())
        };
        match event {
            Event::Key(k) if k.pressed && k.key == Key::Escape => {
                close_center(cx.ui(), &st);
                Handled::Yes
            }
            Event::PointerMove { pos, .. } => {
                let over = match hit(&self.hits, *pos) {
                    Some(Hit::Card(id) | Hit::Close(id) | Hit::Action(id, _)) => Some(id),
                    _ => None,
                };
                if over != self.hovered {
                    self.hovered = over;
                    cx.request_paint();
                }
                Handled::Yes
            }
            Event::Scroll { dy, .. } => {
                let visible = metrics::CENTRE_MAX - (metrics::INSET + 32.0);
                self.scroll = (self.scroll + dy).clamp(0.0, (self.content - visible).max(0.0));
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerUp {
                pos,
                button: Button::Left,
                ..
            } => {
                let panel_x = b.right() - metrics::MARGIN - metrics::CENTRE;
                let panel_top = b.y + metrics::MENU_BAR + metrics::MARGIN;
                let panel_bottom =
                    panel_top + (metrics::INSET + 32.0 + self.content).min(metrics::CENTRE_MAX);
                match hit(&self.hits, *pos) {
                    None if pos.x < panel_x || pos.y < panel_top || pos.y > panel_bottom => {
                        close_center(cx.ui(), &st)
                    }
                    None | Some(Hit::Details(_)) => {}
                    Some(Hit::ClearAll) => clear_all(),
                    Some(Hit::Group(key)) => st.borrow_mut().expanded.push(key),
                    Some(Hit::Collapse(key)) => st.borrow_mut().expanded.retain(|k| *k != key),
                    Some(Hit::ClearGroup(ids)) => {
                        for id in ids {
                            dismiss(id);
                        }
                    }
                    Some(Hit::Close(id)) => dismiss(id),
                    Some(Hit::Action(id, key)) => invoke(id, key),
                    Some(Hit::Card(id)) => {
                        let n = st.borrow().list.iter().find(|n| n.id == id).cloned();
                        if let Some(n) = n {
                            activate(cx.ui(), &n);
                            close_center(cx.ui(), &st);
                        }
                    }
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
        a.label = Some("Notification Centre".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: u32, app: &str, ts: u64, actions: &[&str]) -> Notification {
        Notification {
            id,
            app_name: app.into(),
            app_icon: String::new(),
            summary: "Lemon cake".into(),
            body: "The oven is ready.".into(),
            actions: actions.iter().map(|s| s.to_string()).collect(),
            urgency: 1,
            category: String::new(),
            desktop_entry: String::new(),
            image_path: String::new(),
            transient: false,
            resident: false,
            timestamp_ms: ts,
            expire_timeout: -1,
        }
    }

    #[test]
    fn banner_and_centre_motion_follow_the_measures() {
        assert_eq!(enter_offset(0.0), ENTER_DISTANCE);
        assert_eq!(enter_offset(0.5), 0.0);
        assert!(exit_offset(0.1) > 0.0);
        assert_eq!(exit_offset(0.3), ENTER_DISTANCE);
        assert_eq!(centre_open_offset(CENTRE_OPEN), 0.0);
        assert!(centre_open_offset(0.035) < 80.0);
        assert_eq!(centre_close_offset(0.0), 0.0);
        assert!((centre_close_offset(CENTRE_CLOSE) - CENTRE_DISTANCE).abs() < 0.01);
    }

    #[test]
    fn groups_actions_and_settings() {
        let settings = SettingsMap::default();
        let list = vec![
            note(1, "Mail", 1, &[]),
            note(2, "Clock", 3, &[]),
            note(
                3,
                "Mail",
                2,
                &["default", "Open", "inline-reply", "Reply", "mute", "Mute"],
            ),
        ];
        let g = groups(&list, &settings);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, "Clock");
        assert_eq!(g[1].1.iter().map(|n| n.id).collect::<Vec<_>>(), [3, 1]);
        assert_eq!(
            buttons(&list[2]),
            [("mute".to_string(), "Mute".to_string())]
        );
        assert!(can_reply(&list[2]));
        assert_eq!(reply_label(&list[2]), "Reply");
        assert_eq!(style_of(&settings, &list[0]), BannerStyle::Banners);
    }
}
