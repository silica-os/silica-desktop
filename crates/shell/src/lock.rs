//! The lock screen (docs/spec/lock.md L-01 for behaviour and timings,
//! docs/design/amalfi/lock-screen.md for layout and look).
//!
//! Ossido locks (`session.lock`) and announces `session.locked` = `1`; the shell then takes
//! the session lock (ext-session-lock) with one lock surface per output. The first output
//! shows the user block and the password field, the others the clock. The password goes to
//! silica-daemon (`System1.Unlock`), which only answers right or wrong: on right the shell
//! ends the lock itself.

use std::cell::RefCell;
use std::rc::Rc;

use vetro_lib::amalfi::foundation::Label;
use vetro_lib::amalfi::tokens::typography;
use vetro_lib::color::Color;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::{
    Event, EventCx, FocusKind, Handled, Key, LayoutCx, PaintCx, Proposal, Ui, Widget, WindowKind,
};
use vetro_lib::geom::{Point, Rect, Size, Transform};
use vetro_lib::shape::Shape;
use vetro_lib::style::{Border, Style};
use vetro_lib::text::Align;

use crate::time;
use crate::wallpaper::{self, Choice};

/// Measures of the Amalfi lock screen (lock-screen.md "Screen layout").
pub mod metrics {
    use vetro_lib::geom::Size;

    /// The clock block's width.
    pub const CLOCK_WIDTH: f32 = 960.0;
    /// The clock block's top.
    pub const CLOCK_TOP: f32 = 104.0;
    /// The date's box height.
    pub const DATE_HEIGHT: f32 = 40.0;
    /// The time's box top in the block, and height.
    pub const TIME_TOP: f32 = 40.0;
    /// The time's box height.
    pub const TIME_HEIGHT: f32 = 200.0;
    /// The user block: width, top from the bottom (H − 284).
    pub const USER_WIDTH: f32 = 320.0;
    /// User block top from the bottom edge.
    pub const USER_FROM_BOTTOM: f32 = 284.0;
    /// The avatar.
    pub const AVATAR: f32 = 72.0;
    /// The name's box top in the user block.
    pub const NAME_TOP: f32 = 84.0;
    /// The field: 208 × 32, top H − 160.
    pub const FIELD: Size = Size {
        width: 208.0,
        height: 32.0,
    };
    /// Field top from the bottom edge.
    pub const FIELD_FROM_BOTTOM: f32 = 160.0;
    /// The help button, 8 right of the field.
    pub const HELP: f32 = 32.0;
    /// Gap between the field and the help button.
    pub const HELP_GAP: f32 = 8.0;
    /// Dots: first at x 16, pitch 12, 8 pt, y 12.
    pub const DOT: f32 = 8.0;
    /// Dot pitch.
    pub const DOT_PITCH: f32 = 12.0;
    /// First dot's x.
    pub const DOT_X: f32 = 16.0;
    /// Dots shown before the field scrolls.
    pub const MAX_DOTS: usize = 13;
    /// The submit disc: 24 at x 180, y 4.
    pub const SUBMIT: f32 = 24.0;
    /// Submit disc x.
    pub const SUBMIT_X: f32 = 180.0;
    /// Failed attempts before the hint replaces the user block.
    pub const HINT_AFTER: u32 = 3;
}

/// The wrong-password shake (lock.md "Wrong password shake"): a damped oscillation, first
/// to the left, about 8 Hz, peaks 14 decaying to 5, about 400 ms.
pub fn shake_offset(t: f64) -> f32 {
    if !(0.0..0.4).contains(&t) {
        return 0.0;
    }
    let amp = 14.0 * (-t / 0.36).exp();
    (-(amp * (std::f64::consts::TAU * 8.0 * t).sin())) as f32
}

/// Where the password is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Typing (or empty).
    #[default]
    Entry,
    /// silica-daemon is checking it.
    Verifying,
    /// It was wrong: the dots stay, selected.
    Wrong,
}

/// The lock screen's state, shared by its surfaces.
#[derive(Debug, Default)]
pub struct LockState {
    /// The session is locked by this shell.
    pub locked: bool,
    /// The typed password.
    pub password: String,
    /// Where it is.
    pub phase: Phase,
    /// Failed attempts since locking.
    pub failures: u32,
    /// When the last shake began (`Ui::now`).
    pub shake_from: Option<f64>,
    /// The user's display name.
    pub user: String,
    /// 24-hour clock.
    pub hour24: bool,
    /// The wallpaper.
    pub wallpaper: Option<Choice>,
    /// The lock surfaces.
    pub windows: Vec<vetro_lib::core::WindowId>,
    /// The system's state its status items show.
    pub system: crate::system::SystemState,
    /// The status items of each surface (lock.md L-01: Wi-Fi and battery).
    pub clusters: Vec<crate::status::Cluster>,
}

/// Shows the system's state in the lock surfaces' status items.
pub fn update_status(ui: &mut Ui, state: &Shared) {
    let (clusters, system) = {
        let s = state.borrow();
        (s.clusters.clone(), s.system.clone())
    };
    for c in clusters {
        crate::status::update(ui, &c, &system, "");
    }
}

/// A shared lock state.
pub type Shared = Rc<RefCell<LockState>>;

/// The user's display name: the GECOS full name of `$USER`, else the login name.
pub fn user_name() -> String {
    let login = std::env::var("USER").unwrap_or_default();
    let full = std::fs::read_to_string("/etc/passwd").ok().and_then(|p| {
        p.lines().find_map(|l| {
            let mut f = l.split(':');
            (f.next() == Some(login.as_str()))
                .then(|| f.nth(3))
                .flatten()
                .map(|g| g.split(',').next().unwrap_or("").to_string())
        })
    });
    full.filter(|n| !n.is_empty()).unwrap_or(login)
}

/// Takes the lock: one surface per output (the first with the password field).
pub fn lock(ui: &mut Ui, state: &Shared) {
    if state.borrow().locked {
        return;
    }
    {
        let mut s = state.borrow_mut();
        s.locked = true;
        s.password.clear();
        s.phase = Phase::Entry;
        s.failures = 0;
        s.shake_from = None;
    }
    let outputs: Vec<(Option<String>, Size)> = if ui.outputs().is_empty() {
        vec![(None, Size::new(1920.0, 1200.0))]
    } else {
        ui.outputs()
            .iter()
            .map(|o| (Some(o.name.clone()), o.size))
            .collect()
    };
    let mut windows = Vec::new();
    let mut clusters = Vec::new();
    for (i, (name, size)) in outputs.into_iter().enumerate() {
        let w = ui.create_window(WindowKind::Lock { output: name }, size);
        let view = ui.create(LockView::new(state.clone(), i == 0));
        // The status items sit where the menu bar keeps them, white over the wallpaper.
        let cluster = crate::status::cluster(ui, crate::status::LOCK_SCREEN, None);
        let _ = ui.set(
            cluster.view,
            vetro_lib::core::Prop::AppearanceOverride,
            vetro_lib::core::Value::Int(2),
        );
        let system = state.borrow().system.clone();
        crate::status::update(ui, &cluster, &system, "");
        ui.add_child(view, cluster.view);
        clusters.push(cluster);
        let _ = ui.set(
            view,
            vetro_lib::core::Prop::MaxWidth,
            vetro_lib::core::Value::from(f32::INFINITY),
        );
        let _ = ui.set(
            view,
            vetro_lib::core::Prop::MaxHeight,
            vetro_lib::core::Value::from(f32::INFINITY),
        );
        ui.set_content(w, view);
        ui.show_window(w);
        if i == 0 {
            ui.set_focus(view, false);
        }
        windows.push(w);
    }
    let mut s = state.borrow_mut();
    s.windows = windows;
    s.clusters = clusters;
}

/// Ends the lock after the password was right.
pub fn unlock(ui: &mut Ui, state: &Shared) {
    let mut s = state.borrow_mut();
    if !s.locked {
        return;
    }
    s.locked = false;
    s.password.clear();
    s.phase = Phase::Entry;
    s.windows.clear();
    s.clusters.clear();
    drop(s);
    ui.unlock_session();
}

/// Ossido unlocked on its own (the lock ended): the surfaces go.
pub fn forget(ui: &mut Ui, state: &Shared) {
    let windows = std::mem::take(&mut state.borrow_mut().windows);
    state.borrow_mut().clusters.clear();
    state.borrow_mut().locked = false;
    for w in windows {
        ui.close_window(w);
    }
}

/// Sends the typed password to silica-daemon.
fn submit(state: &Shared) {
    let password = {
        let mut s = state.borrow_mut();
        if s.password.is_empty() || s.phase == Phase::Verifying {
            return;
        }
        s.phase = Phase::Verifying;
        s.password.clone()
    };
    let st = state.clone();
    let started = vetro_lib::platform::spawn(async move {
        let answer = match vetro_lib::platform::services() {
            Some(services) => match services.system().await {
                Ok(proxy) => proxy.unlock(&password).await.ok(),
                Err(_) => None,
            },
            None => None,
        };
        let st2 = st.clone();
        vetro_lib::platform::post(move |ui| answered(ui, &st2, answer == Some(true)));
    });
    if !started {
        // Not running in a session (a preview): never right.
        state.borrow_mut().phase = Phase::Wrong;
    }
}

/// silica-daemon answered.
fn answered(ui: &mut Ui, state: &Shared, right: bool) {
    if right {
        unlock(ui, state);
        return;
    }
    let mut s = state.borrow_mut();
    s.phase = Phase::Wrong;
    s.failures += 1;
    s.shake_from = Some(ui.now);
    drop(s);
    for w in state.borrow().windows.clone() {
        if let Some(root) = ui.window(w).map(|w| w.root) {
            for v in ui.tree.descendants(root) {
                ui.request_paint(v);
            }
        }
    }
}

/// One lock surface's content.
pub struct LockView {
    state: Shared,
    /// The surface with the user block and the field.
    primary: bool,
    labels: [Label; 6],
    clock: String,
    date: String,
    minute: u32,
    blink: bool,
    shaking: bool,
}

impl LockView {
    /// A view of `state`.
    pub fn new(state: Shared, primary: bool) -> LockView {
        let t = time::now();
        let hour24 = state.borrow().hour24;
        LockView {
            state,
            primary,
            labels: Default::default(),
            clock: t.clock(hour24),
            date: t.long_date(),
            minute: t.minute,
            blink: true,
            shaking: false,
        }
    }

    fn field_rect(size: Size) -> Rect {
        Rect::new(
            ((size.width - metrics::FIELD.width) / 2.0).round(),
            size.height - metrics::FIELD_FROM_BOTTOM,
            metrics::FIELD.width,
            metrics::FIELD.height,
        )
    }

    fn submit_rect(field: Rect) -> Rect {
        Rect::new(
            field.x + metrics::SUBMIT_X,
            field.y + (field.height - metrics::SUBMIT) / 2.0,
            metrics::SUBMIT,
            metrics::SUBMIT,
        )
    }
}

/// White at `a` (0–1).
fn white(a: f32) -> Color {
    Color::rgba(255, 255, 255, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl Widget for LockView {
    fn kind(&self) -> &'static str {
        "LockScreen"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(1920.0, 1200.0))
    }

    fn place(&mut self, cx: &mut LayoutCx<'_>, size: Size) {
        if let Some(cluster) = cx.children().first().copied() {
            let s = cx.measure(cluster, Proposal::new(None, None));
            let x = size.width - crate::menubar::metrics::PADDING - s.width;
            cx.place(cluster, Rect::new(x, 0.0, s.width, s.height));
        }
    }

    fn attached(&mut self, cx: &mut EventCx<'_>) {
        cx.animate();
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        // The caret blinks at 1 Hz.
        let blink = now.rem_euclid(1.0) < 0.5;
        if blink != self.blink {
            self.blink = blink;
            cx.request_paint();
        }
        let shaking = self
            .state
            .borrow()
            .shake_from
            .is_some_and(|t| now - t < 0.4);
        if shaking || shaking != self.shaking {
            self.shaking = shaking;
            cx.request_paint();
        }
        let t = time::now();
        if t.minute != self.minute {
            self.minute = t.minute;
            self.clock = t.clock(self.state.borrow().hour24);
            self.date = t.long_date();
            cx.request_paint();
        }
        true
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let theme = *cx.theme();
        let c = theme.colors;
        let r = cx.bounds();
        let size = r.size();
        let now = cx.now();
        let st = self.state.borrow();
        // The wallpaper, not blurred, under a black 6 % veil.
        match &st.wallpaper {
            Some(c @ (Choice::Image(_) | Choice::Asset(..))) => match wallpaper::choice_image(c) {
                Some(img) => {
                    let to = wallpaper::fill_rect(img.width() as f32, img.height() as f32, r);
                    cx.rec.push_clip(r, Shape::RECT);
                    cx.rec.image("Wallpaper", to, img, 1.0);
                    cx.rec.pop();
                }
                None => cx.rec.layer(
                    "Wallpaper",
                    r,
                    Shape::RECT,
                    Style::fill(Color::rgb(0x1b, 0x1c, 0x1f)),
                ),
            },
            Some(Choice::Color(col)) => {
                cx.rec.layer("Wallpaper", r, Shape::RECT, Style::fill(*col));
            }
            None => cx.rec.layer(
                "Wallpaper",
                r,
                Shape::RECT,
                Style::fill(Color::rgb(0x1b, 0x1c, 0x1f)),
            ),
        }
        cx.rec.layer(
            "Veil",
            r,
            Shape::RECT,
            Style::fill(Color::rgba(0, 0, 0, 0x0f)),
        );
        // The clock block: the date, then the time.
        let block_x = r.x + ((size.width - metrics::CLOCK_WIDTH) / 2.0).round();
        let date_style = theme.text(&typography::LOCK_DATE, white(0.84));
        let [l_date, l_time, l_name, l_place, l_hint, _] = &mut self.labels;
        let layout = l_date.layout(
            &self.date,
            &date_style,
            Align::Center,
            metrics::CLOCK_WIDTH,
            Some(1),
        );
        cx.rec.text(
            "Date",
            Rect::new(
                block_x,
                r.y + metrics::CLOCK_TOP,
                metrics::CLOCK_WIDTH,
                metrics::DATE_HEIGHT,
            ),
            layout,
            Some(typography::LOCK_DATE.name),
            white(0.84),
        );
        let time_style = theme.text(&typography::LOCK_CLOCK, white(0.76));
        let layout = l_time.layout(
            &self.clock,
            &time_style,
            Align::Center,
            metrics::CLOCK_WIDTH,
            Some(1),
        );
        cx.rec.text(
            "Time",
            Rect::new(
                block_x,
                r.y + metrics::CLOCK_TOP + metrics::TIME_TOP,
                metrics::CLOCK_WIDTH,
                metrics::TIME_HEIGHT,
            ),
            layout,
            Some(typography::LOCK_CLOCK.name),
            white(0.76),
        );
        if !self.primary {
            return;
        }
        let user_top = r.y + size.height - metrics::USER_FROM_BOTTOM;
        let cx_mid = r.x + size.width / 2.0;
        if st.failures >= metrics::HINT_AFTER {
            // The hint replaces the avatar and the name in one step (lock.md "Hint state").
            let style = theme.text(&typography::HEADLINE, c.text_primary);
            let hint = "If you forgot your password, you can reset it with your recovery key. \
                        The computer restarts to do it.";
            let layout = l_hint.layout(hint, &style, Align::Center, metrics::USER_WIDTH, None);
            let h = layout.size().height;
            let field = Self::field_rect(size).translate(r.origin());
            cx.rec.text(
                "Hint",
                Rect::new(
                    cx_mid - metrics::USER_WIDTH / 2.0,
                    field.y - 24.0 - h,
                    metrics::USER_WIDTH,
                    h,
                ),
                layout,
                Some(typography::HEADLINE.name),
                c.text_primary,
            );
        } else {
            // The avatar: an accent disc with a person glyph, a light edge inside.
            let a = Rect::new(
                cx_mid - metrics::AVATAR / 2.0,
                user_top,
                metrics::AVATAR,
                metrics::AVATAR,
            );
            cx.rec.layer(
                "Avatar",
                a,
                Shape::Ellipse,
                Style::fill(c.accent_amalfi).with_border(Border::inside(white(0.24), 1.0)),
            );
            let g = 40.0;
            cx.rec.glyph(
                "Avatar glyph",
                Rect::new(a.center().x - g / 2.0, a.center().y - g / 2.0, g, g),
                "person-fill",
                400,
                c.text_on_accent,
            );
            let style = theme.text(&typography::TITLE_3, c.text_primary);
            let layout = l_name.layout(
                &st.user,
                &style,
                Align::Center,
                metrics::USER_WIDTH,
                Some(1),
            );
            cx.rec.text(
                "Name",
                Rect::new(
                    cx_mid - metrics::USER_WIDTH / 2.0,
                    user_top + metrics::NAME_TOP,
                    metrics::USER_WIDTH,
                    22.0,
                ),
                layout,
                Some(typography::TITLE_3.name),
                c.text_primary,
            );
        }
        // The field and the help button shake together.
        let dx = st.shake_from.map_or(0.0, |t| shake_offset(now - t));
        let field = Self::field_rect(size)
            .translate(r.origin())
            .translate(Point::new(dx, 0.0));
        let glass = theme.material_style("Glass/Lock");
        cx.rec.layer(
            "Password field",
            field,
            Shape::rounded(field.height / 2.0),
            glass.clone(),
        );
        let ink = c.text_primary;
        let shown = st.password.chars().count().min(metrics::MAX_DOTS);
        let dots_end = field.x + metrics::DOT_X + shown as f32 * metrics::DOT_PITCH;
        if st.phase == Phase::Wrong && shown > 0 {
            cx.rec.layer(
                "Selection",
                Rect::new(
                    field.x + 10.0,
                    field.y + 8.0,
                    dots_end - 6.0 - field.x - 4.0,
                    16.0,
                ),
                Shape::rounded(8.0),
                Style::fill(c.line_focus),
            );
        }
        for i in 0..shown {
            let x = field.x + metrics::DOT_X + i as f32 * metrics::DOT_PITCH;
            cx.rec.layer(
                "Dot",
                Rect::new(x, field.y + 12.0, metrics::DOT, metrics::DOT),
                Shape::Ellipse,
                Style::fill(ink),
            );
        }
        let caret_x = if shown == 0 {
            field.x + 16.0
        } else {
            dots_end - metrics::DOT_PITCH + metrics::DOT + 4.0
        };
        if st.phase == Phase::Entry && self.blink && cx.is_focused() {
            cx.rec.layer(
                "Caret",
                Rect::new(caret_x, field.y + 8.0, 2.0, 16.0),
                Shape::RECT,
                Style::fill(ink),
            );
        }
        if shown == 0 {
            let style = theme.text(&typography::BODY, c.text_secondary);
            let layout = l_place.layout("Enter Password", &style, Align::Start, 160.0, Some(1));
            cx.rec.text(
                "Placeholder",
                Rect::new(field.x + 22.0, field.y + 7.0, 160.0, 18.0),
                layout,
                Some(typography::BODY.name),
                c.text_secondary,
            );
        } else if st.phase == Phase::Verifying {
            // The small spinner in place of the submit: twelve ticks, one bright.
            let centre = Point::new(field.x + 192.0, field.y + 16.0);
            let lit = (now * 12.0).floor() as i64;
            for k in 0..12i64 {
                let a = (k - lit).rem_euclid(12) as f32 / 12.0;
                cx.rec.push_transform(
                    Transform::rotate(k as f32 * std::f32::consts::TAU / 12.0)
                        .then(Transform::translate(centre.x, centre.y)),
                );
                cx.rec.layer(
                    "Spinner",
                    Rect::new(-1.0, -8.0, 2.0, 4.5),
                    Shape::rounded(1.0),
                    Style::fill(ink.with_alpha((255.0 * (1.0 - 0.75 * a)) as u8)),
                );
                cx.rec.pop();
            }
        } else {
            let s = Self::submit_rect(field);
            cx.rec
                .layer("Submit", s, Shape::Ellipse, Style::fill(c.accent_amalfi));
            cx.rec.glyph(
                "Submit glyph",
                Rect::new(s.x + 4.0, s.y + 4.0, 16.0, 16.0),
                "arrow-right",
                600,
                c.text_on_accent,
            );
        }
        let help = Rect::new(
            field.right() + metrics::HELP_GAP,
            field.y,
            metrics::HELP,
            metrics::HELP,
        );
        cx.rec.layer("Help", help, Shape::Ellipse, glass);
        cx.rec.glyph(
            "Help glyph",
            Rect::new(help.x + 8.0, help.y + 8.0, 16.0, 16.0),
            "question",
            600,
            c.text_primary,
        );
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        match event {
            Event::Text(t) => {
                let mut s = self.state.borrow_mut();
                if s.phase == Phase::Verifying {
                    return Handled::Yes;
                }
                if s.phase == Phase::Wrong {
                    // Typing after a failure replaces the selected dots.
                    s.password.clear();
                    s.phase = Phase::Entry;
                }
                s.password.push_str(t);
                drop(s);
                cx.request_paint();
                Handled::Yes
            }
            Event::Key(k) if k.pressed => {
                match k.key {
                    Key::Enter => submit(&self.state),
                    Key::Backspace => {
                        let mut s = self.state.borrow_mut();
                        if s.phase == Phase::Wrong {
                            s.password.clear();
                        } else if s.phase == Phase::Entry {
                            s.password.pop();
                        }
                        s.phase = Phase::Entry;
                    }
                    Key::Escape => {
                        let mut s = self.state.borrow_mut();
                        if s.phase != Phase::Verifying {
                            s.password.clear();
                            s.phase = Phase::Entry;
                        }
                    }
                    _ => return Handled::No,
                }
                cx.request_paint();
                Handled::Yes
            }
            Event::PointerDown { pos, .. } => {
                cx.focus_self();
                let b = cx.bounds();
                let field = Self::field_rect(b.size()).translate(b.origin());
                if Self::submit_rect(field).contains(*pos) {
                    submit(&self.state);
                    cx.request_paint();
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focus(&self) -> FocusKind {
        FocusKind::Always
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Window;
        a.label = Some("Lock Screen".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shake_goes_left_first_and_settles() {
        assert!(shake_offset(0.02) < 0.0);
        let peak = (0..40)
            .map(|i| shake_offset(f64::from(i) * 0.01).abs())
            .fold(0.0f32, f32::max);
        assert!(peak > 9.0 && peak < 14.5, "{peak}");
        assert_eq!(shake_offset(0.5), 0.0);
    }
}
