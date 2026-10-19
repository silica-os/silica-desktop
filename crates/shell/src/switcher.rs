//! The app switcher (docs/spec/app-switcher.md): Ossido holds the keys and sends
//! `switcher.next` / `previous` / `commit` / `cancel` / `expose`; the shell keeps the apps in
//! most-recently-used order and shows the row of icons, centred, ~160 ms after the first Tab
//! (a quick ⌘Tab never shows it). Releasing ⌘ switches and the panel fades out; Esc closes at
//! once; ↓ opens the selected app's windows.

use std::cell::RefCell;
use std::rc::Rc;

use vetro_lib::amalfi::foundation::Label;
use vetro_lib::amalfi::tokens::{TypeStyle, typography};
use vetro_lib::color::Color;
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
use vetro_lib::style::Style;
use vetro_lib::text::Align;

use crate::apps::{self, DesktopEntry};

/// Measures (app-switcher.md "Panel", "Icons and selection").
pub mod metrics {
    /// Icon pitch.
    pub const PITCH: f32 = 134.0;
    /// Icon size.
    pub const ICON: f32 = 104.0;
    /// Inset of the first icon.
    pub const INSET: f32 = 35.0;
    /// The panel's height.
    pub const HEIGHT: f32 = 176.0;
    /// The panel's corner radius.
    pub const RADIUS: f32 = 56.0;
    /// The selection box around an icon.
    pub const SELECTION: f32 = 8.0;
    /// The selection's radius.
    pub const SELECTION_RADIUS: f32 = 30.0;
    /// Margin from the screen's sides.
    pub const SIDE: f32 = 33.0;
    /// Delay before the panel shows.
    pub const DELAY: f64 = 0.16;
    /// The fade-out.
    pub const FADE: f64 = 0.19;
    /// Room around the panel in its surface.
    pub const MARGIN: f32 = 24.0;
}

/// The switcher's name label: semibold, ~14 pt.
const NAME: TypeStyle = TypeStyle {
    name: "Switcher Name",
    size: 14.0,
    line_height: Some(18.0),
    weight: 600,
    ..typography::BODY
};

/// The switcher's state.
#[derive(Default)]
pub struct SwitcherState {
    /// The apps (id, a window), most recent first, while switching.
    pub apps: Vec<(DesktopEntry, String)>,
    /// The selected one.
    pub selected: usize,
    /// Switching (between the first Tab and the commit or cancel).
    pub active: bool,
    /// When it started.
    since: f64,
    /// When it began fading out.
    fading: Option<f64>,
    /// The surface and view.
    surface: Option<(WindowId, ViewId)>,
    /// Pitch, icon size and panel height as opened.
    sizes: (f32, f32, f32),
}

/// Shared.
pub type Shared = Rc<RefCell<SwitcherState>>;

/// The geometry: icon pitch and size for `n` apps on a screen `width` wide.
pub fn sizing(n: usize, width: f32) -> (f32, f32, f32) {
    let natural = metrics::PITCH * n as f32 + 44.0;
    let max = width - 2.0 * metrics::SIDE;
    let k = if natural > max && n > 0 {
        ((max - 44.0) / n as f32 / metrics::PITCH).clamp(0.3, 1.0)
    } else {
        1.0
    };
    let pitch = metrics::PITCH * k;
    let icon = metrics::ICON * k;
    (pitch, icon, metrics::HEIGHT - (metrics::ICON - icon))
}

/// Handles a `switcher.*` command; `windows` are (window, app) with the most recent last.
pub fn command(ui: &mut Ui, state: &Shared, id: &str, windows: &[(String, String)]) {
    let now = ui.now;
    match id {
        "switcher.next" | "switcher.previous" => {
            let forward = id == "switcher.next";
            let starting = !state.borrow().active;
            if starting {
                let mut apps: Vec<(DesktopEntry, String)> = Vec::new();
                for (w, app) in windows.iter().rev() {
                    if app.is_empty() || apps.iter().any(|(e, _)| e.id == *app) {
                        continue;
                    }
                    let e = apps::entry(app).unwrap_or_else(|| DesktopEntry {
                        id: app.clone(),
                        name: app.rsplit('.').next().unwrap_or(app).to_string(),
                        ..DesktopEntry::default()
                    });
                    apps.push((e, w.clone()));
                }
                if apps.is_empty() {
                    return;
                }
                let n = apps.len();
                let mut s = state.borrow_mut();
                s.apps = apps;
                // ⌘Tab starts on the second app.
                s.selected = if forward { 1 % n } else { n - 1 };
                s.active = true;
                s.since = now;
                s.fading = None;
                drop(s);
                let st = state.clone();
                ui.after(metrics::DELAY, move |ui| {
                    let show = st.borrow().active && st.borrow().surface.is_none();
                    if show {
                        open(ui, &st);
                    }
                });
            } else {
                let mut s = state.borrow_mut();
                let n = s.apps.len().max(1);
                s.selected = if forward {
                    (s.selected + 1) % n
                } else {
                    (s.selected + n - 1) % n
                };
                drop(s);
                repaint(ui, state);
            }
        }
        "switcher.commit" | "switcher.expose" => {
            let target = {
                let mut s = state.borrow_mut();
                if !s.active {
                    return;
                }
                s.active = false;
                s.fading = Some(now);
                s.apps.get(s.selected).map(|(_, w)| w.clone())
            };
            if let Some(w) = target {
                ui.shell(ShellRequest::ActivateApp(w));
                if id == "switcher.expose" {
                    ui.shell(ShellRequest::Execute("panorama.app".into()));
                }
            }
            if let Some((_, v)) = state.borrow().surface {
                ui.start_animating(v);
            }
        }
        "switcher.cancel" => {
            let w = {
                let mut s = state.borrow_mut();
                s.active = false;
                s.fading = None;
                s.surface.take()
            };
            if let Some((w, _)) = w {
                ui.close_window(w);
            }
        }
        _ => {}
    }
}

fn open(ui: &mut Ui, state: &Shared) {
    let out = ui
        .outputs()
        .first()
        .map_or(Size::new(1280.0, 800.0), |o| o.size);
    let n = state.borrow().apps.len();
    let (pitch, icon, height) = sizing(n, out.width);
    state.borrow_mut().sizes = (pitch, icon, height);
    let m = metrics::MARGIN;
    let size = Size::new(pitch * n as f32 + 44.0 + 2.0 * m, height + 40.0 + 2.0 * m);
    let output = ui.outputs().first().map(|o| o.name.clone());
    let w = ui.create_window(
        WindowKind::Shell {
            layer: Layer::Overlay,
            anchor: Edges::empty(),
            exclusive: -1,
            margin: [0; 4],
            keyboard: KeyboardMode::None,
            namespace: "switcher".into(),
            output,
        },
        size,
    );
    if let Some(win) = ui.window_mut(w) {
        win.opaque = false;
    }
    let v = ui.create(SwitcherView {
        state: state.clone(),
        label: Label::default(),
    });
    let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(w, v);
    ui.show_window(w);
    ui.set_input_region(w, Some(vec![Rect::new(m, m, size.width - 2.0 * m, height)]));
    state.borrow_mut().surface = Some((w, v));
}

fn repaint(ui: &mut Ui, state: &Shared) {
    if let Some((_, v)) = state.borrow().surface {
        ui.request_paint(v);
    }
}

/// The panel.
struct SwitcherView {
    state: Shared,
    label: Label,
}

impl SwitcherView {
    fn panel(&self, b: Rect) -> (Rect, f32, f32) {
        let n = self.state.borrow().apps.len();
        let (pitch, icon, height) = self.state.borrow().sizes;
        let m = metrics::MARGIN;
        (
            Rect::new(b.x + m, b.y + m, pitch * n as f32 + 44.0, height),
            pitch,
            icon,
        )
    }

    fn icon_rect(panel: Rect, pitch: f32, icon: f32, i: usize) -> Rect {
        let k = icon / metrics::ICON;
        Rect::new(
            panel.x + metrics::INSET * k + i as f32 * pitch,
            panel.y + metrics::INSET * k,
            icon,
            icon,
        )
    }
}

impl vetro_lib::core::Widget for SwitcherView {
    fn kind(&self) -> &'static str {
        "Switcher"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        p.replacing_unspecified(Size::new(600.0, 260.0))
    }

    fn tick(&mut self, cx: &mut EventCx<'_>, _dt: f32) -> bool {
        let now = cx.now();
        cx.request_paint();
        let done = self
            .state
            .borrow()
            .fading
            .is_some_and(|f| now - f >= metrics::FADE);
        if done {
            let w = {
                let mut s = self.state.borrow_mut();
                s.fading = None;
                s.surface.take()
            };
            if let Some((w, _)) = w {
                cx.ui().close_window(w);
            }
            return false;
        }
        self.state.borrow().fading.is_some()
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let theme = *cx.theme();
        let dark = theme.dark();
        let now = cx.now();
        let b = cx.bounds();
        let (panel, pitch, icon) = self.panel(b);
        let s = self.state.borrow();
        let alpha = s.fading.map_or(1.0, |f| {
            1.0 - ((now - f) / metrics::FADE).clamp(0.0, 1.0) as f32
        });
        cx.rec.push_opacity(alpha);
        cx.glass(
            "Panel",
            panel,
            Shape::rounded(metrics::RADIUS * panel.height / metrics::HEIGHT),
            "Glass/Regular",
            GlassProfile::Dock,
        );
        let (sel, ink) = if dark {
            (Color::from_u32(0xa9aaafff), Color::from_u32(0xf7f7f8ff))
        } else {
            (Color::from_u32(0x484950ff), Color::from_u32(0x131314ff))
        };
        for (i, (e, _)) in s.apps.iter().enumerate() {
            let r = Self::icon_rect(panel, pitch, icon, i);
            if i == s.selected {
                let k = icon / metrics::ICON;
                let pad = metrics::SELECTION * k;
                let boxr = Rect::new(
                    r.x - pad,
                    r.y - pad,
                    r.width + 2.0 * pad,
                    r.height + 2.0 * pad,
                );
                cx.rec.layer(
                    "Selection",
                    boxr,
                    Shape::rounded(metrics::SELECTION_RADIUS * k),
                    Style::fill(sel),
                );
                let style = theme.text(&NAME, ink);
                let l = self
                    .label
                    .layout(&e.name, &style, Align::Center, pitch + 40.0, Some(1));
                cx.rec.text(
                    "Name",
                    Rect::new(
                        r.x + r.width / 2.0 - (pitch + 40.0) / 2.0,
                        boxr.bottom() + 7.0,
                        pitch + 40.0,
                        18.0,
                    ),
                    l,
                    None,
                    ink,
                );
            }
            match apps::icon(e, dark) {
                Some(img) => cx.rec.image("Icon", r, img, 1.0),
                None => cx.rec.glyph(
                    "Icon",
                    r.inset(vetro_lib::geom::Insets::all(r.width * 0.25)),
                    "question",
                    400,
                    ink,
                ),
            }
        }
        cx.rec.pop();
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        let (panel, pitch, icon) = self.panel(b);
        let n = self.state.borrow().apps.len();
        let at = |p: Point| {
            (0..n).find(|i| {
                Self::icon_rect(panel, pitch, icon, *i)
                    .inset(vetro_lib::geom::Insets::all(-15.0))
                    .contains(p)
            })
        };
        match event {
            // Moving the pointer over an icon selects it.
            Event::PointerMove { pos, .. } => {
                if let Some(i) = at(*pos)
                    && self.state.borrow().selected != i
                {
                    self.state.borrow_mut().selected = i;
                    cx.request_paint();
                }
                Handled::Yes
            }
            Event::PointerUp {
                pos,
                button: Button::Left,
                ..
            } => {
                if let Some(i) = at(*pos) {
                    self.state.borrow_mut().selected = i;
                    let state = self.state.clone();
                    command(cx.ui(), &state, "switcher.commit", &[]);
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::List;
        a.label = Some("App switcher".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_row_shrinks_past_the_screen() {
        assert_eq!(sizing(4, 2560.0).0 * 4.0 + 44.0, 580.0);
        let (pitch, icon, h) = sizing(25, 2560.0);
        assert!(pitch < 100.0 && icon < 80.0 && h < 150.0);
    }
}
