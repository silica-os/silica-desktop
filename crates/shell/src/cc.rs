//! Control Center (docs/spec/controlcenter.md for behaviour, the kit's `Control Center`
//! masters for layout): a stack of glass modules under the menu bar at the trailing edge —
//! Wi-Fi and Bluetooth toggles, Do Not Disturb, Dark Mode and Screen Capture tiles, the
//! Display and Sound sliders — over the system's state. It is a popup of the menu bar, so a
//! click outside closes it.

use std::cell::RefCell;
use std::rc::Rc;

use vetro_lib::amalfi::foundation::Label;
use vetro_lib::amalfi::tokens::typography;
use vetro_lib::core::a11y::{A11y, Role};
use vetro_lib::core::display::GlassProfile;
use vetro_lib::core::shell::ShellRequest;
use vetro_lib::core::{
    Button, Event, EventCx, Handled, LayoutCx, PaintCx, Prop, Proposal, Signal, Ui, Value, ViewId,
    WindowId, WindowKind,
};
use vetro_lib::geom::{Point, Rect, Size};
use vetro_lib::shape::Shape;
use vetro_lib::style::Style;
use vetro_lib::text::Align;

use crate::system::{self, SystemState};

/// Measures (kit `Control Center/Panel`, `Module/*`; controlcenter.md "Layout").
pub mod metrics {
    /// The stack's width.
    pub const WIDTH: f32 = 292.0;
    /// A column.
    pub const COLUMN: f32 = 140.0;
    /// Gap between modules.
    pub const GAP: f32 = 12.0;
    /// A module's height (and a tile's side).
    pub const MODULE: f32 = 64.0;
    /// The stack's distance from the right edge.
    pub const RIGHT: f32 = 14.0;
    /// The stack's top from the screen's top.
    pub const TOP: f32 = 42.0;
    /// Sliders' corner radius.
    pub const SLIDER_RADIUS: f32 = 28.0;
    /// Room around the stack in its surface for the modules' shadows.
    pub const MARGIN: f32 = 32.0;
    /// The room above it, which stays clear of the menu bar.
    pub const MARGIN_TOP: f32 = 8.0;
}

/// The stack's origin in the panel's surface.
fn stack_origin(b: Rect) -> Point {
    Point::new(b.x + metrics::MARGIN, b.y + metrics::MARGIN_TOP)
}

/// A module of the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Module {
    /// Wi-Fi toggle.
    Wifi,
    /// Bluetooth toggle.
    Bluetooth,
    /// Do Not Disturb toggle.
    Focus,
    /// Dark Mode tile.
    DarkMode,
    /// Screen Capture tile.
    ScreenCapture,
    /// Display brightness.
    Display,
    /// Sound volume.
    Sound,
}

/// The modules shown for `state`, with their frames in the stack.
pub fn layout(state: &SystemState) -> Vec<(Module, Rect)> {
    let c = metrics::COLUMN + metrics::GAP;
    let pitch = metrics::MODULE + metrics::GAP;
    let m = metrics::MODULE;
    let mut out = vec![
        (Module::Wifi, Rect::new(0.0, 0.0, metrics::COLUMN, m)),
        (Module::Bluetooth, Rect::new(c, 0.0, metrics::COLUMN, m)),
        (Module::Focus, Rect::new(0.0, pitch, metrics::COLUMN, m)),
        (Module::DarkMode, Rect::new(c, pitch, m, m)),
        (Module::ScreenCapture, Rect::new(c + pitch, pitch, m, m)),
    ];
    let mut y = 2.0 * pitch;
    if state.brightness_available || !state.known {
        out.push((Module::Display, Rect::new(0.0, y, metrics::WIDTH, m)));
        y += pitch;
    }
    if state.audio_available || !state.known {
        out.push((Module::Sound, Rect::new(0.0, y, metrics::WIDTH, m)));
    }
    out
}

/// The stack's height for `state`.
pub fn height(state: &SystemState) -> f32 {
    layout(state)
        .iter()
        .map(|(_, r)| r.bottom())
        .fold(0.0, f32::max)
}

/// The open panel.
#[derive(Default)]
pub struct CcState {
    /// Its popup.
    pub popup: Option<WindowId>,
    /// Its view.
    pub view: Option<ViewId>,
}

/// Redraws the open panel (the system's state changed).
pub fn repaint(ui: &mut Ui, cc: &Shared) {
    if let Some(v) = cc.borrow().view {
        ui.request_paint(v);
    }
}

/// Shared.
pub type Shared = Rc<RefCell<CcState>>;

/// Opens (or closes) Control Center as a popup of `bar` (window width `bar_width`).
pub fn toggle(
    ui: &mut Ui,
    cc: &Shared,
    system: &system::Shared,
    bar: WindowId,
    bar_width: f32,
    on_close: Rc<dyn Fn(&mut Ui)>,
) {
    let open = cc.borrow_mut().popup.take();
    if let Some(p) = open {
        cc.borrow_mut().view = None;
        ui.close_window(p);
        on_close(ui);
        return;
    }
    let state = system.borrow().state.clone();
    let m = metrics::MARGIN;
    let size = Size::new(
        metrics::WIDTH + 2.0 * m,
        height(&state) + m + metrics::MARGIN_TOP,
    );
    let x = bar_width - metrics::RIGHT - metrics::WIDTH - m;
    let popup = ui.create_window(
        WindowKind::Popup {
            parent: bar,
            anchor: Rect::new(
                x,
                metrics::TOP - metrics::MARGIN_TOP - size.height,
                size.width,
                size.height,
            ),
            grab: true,
        },
        size,
    );
    let view = ui.create(CcView {
        system: system.clone(),
        labels: Default::default(),
        dragging: None,
    });
    let _ = ui.set(view, Prop::MaxWidth, Value::from(f32::INFINITY));
    let _ = ui.set(view, Prop::MaxHeight, Value::from(f32::INFINITY));
    ui.set_content(popup, view);
    ui.show_window(popup);
    if let Some(root) = ui.window(popup).map(|w| w.root) {
        let c = cc.clone();
        ui.connect(root, Signal::Closed, move |ui, _, _| {
            if c.borrow().popup == Some(popup) {
                c.borrow_mut().popup = None;
                c.borrow_mut().view = None;
                on_close(ui);
            }
        });
    }
    {
        let mut c = cc.borrow_mut();
        c.popup = Some(popup);
        c.view = Some(view);
    }
    system::refresh(system);
}

/// The panel's modules.
struct CcView {
    system: system::Shared,
    labels: [Label; 12],
    /// The slider being dragged.
    dragging: Option<Module>,
}

/// The slider's track in a 292 × 64 module (kit `Module/Slider`: 40, 44, 172 × 4).
fn track(r: Rect) -> Rect {
    Rect::new(r.x + 40.0, r.y + 44.0, 172.0, 4.0)
}

fn slider_value(r: Rect, x: f32) -> f64 {
    let t = track(r);
    f64::from(((x - t.x) / t.width).clamp(0.0, 1.0))
}

impl CcView {
    fn press(&mut self, cx: &mut EventCx<'_>, m: Module, r: Rect, p: Point) {
        let s = self.system.borrow().state.clone();
        let sys = self.system.clone();
        match m {
            Module::Wifi => system::call(&sys, move |p| async move {
                let _ = p.set_wifi_enabled(!s.wifi_enabled).await;
            }),
            Module::Bluetooth => system::call(&sys, move |p| async move {
                let _ = p.set_bluetooth_powered(!s.bluetooth_powered).await;
            }),
            Module::Focus => system::call(&sys, move |p| async move {
                let _ = p.set_do_not_disturb(!s.dnd).await;
            }),
            Module::DarkMode => system::call(&sys, move |p| async move {
                let _ = p.set_dark(!s.dark).await;
            }),
            Module::ScreenCapture => {
                cx.ui()
                    .shell(ShellRequest::Execute("screenshot.toolbar".into()));
            }
            Module::Display | Module::Sound => {
                self.dragging = Some(m);
                cx.capture_pointer();
                self.drag(cx, r, p.x);
            }
        }
    }

    fn drag(&mut self, cx: &mut EventCx<'_>, r: Rect, x: f32) {
        let v = slider_value(r, x);
        let sys = self.system.clone();
        match self.dragging {
            Some(Module::Display) => {
                sys.borrow_mut().state.brightness = v;
                system::call(&sys, move |p| async move {
                    let _ = p.set_brightness(v).await;
                });
            }
            Some(Module::Sound) => {
                sys.borrow_mut().state.volume = v;
                system::call(&sys, move |p| async move {
                    let _ = p.set_volume(v).await;
                });
            }
            _ => {}
        }
        cx.request_paint();
    }
}

impl vetro_lib::core::Widget for CcView {
    fn kind(&self) -> &'static str {
        "ControlCenter"
    }

    fn measure(&mut self, _cx: &mut LayoutCx<'_>, p: Proposal) -> Size {
        let s = self.system.borrow().state.clone();
        let m = metrics::MARGIN;
        p.replacing_unspecified(Size::new(
            metrics::WIDTH + 2.0 * m,
            height(&s) + m + metrics::MARGIN_TOP,
        ))
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let theme = *cx.theme();
        let c = theme.colors;
        let b = cx.bounds();
        let o = stack_origin(b);
        let s = self.system.borrow().state.clone();
        let on_ink = c.text_on_accent;
        let off_disc = c.text_tertiary;
        let title_style = theme.text(&typography::HEADLINE, c.text_primary);
        let sub_style = theme.text(&typography::CAPTION, c.text_secondary);
        let mut li = 0usize;
        for (m, r) in layout(&s) {
            let r = r.translate(o);
            let radius = match m {
                Module::Display | Module::Sound => metrics::SLIDER_RADIUS,
                _ => r.height / 2.0,
            };
            cx.glass(
                "Module",
                r,
                Shape::rounded(radius),
                "Glass/Thick",
                GlassProfile::Module,
            );
            match m {
                Module::Wifi | Module::Bluetooth | Module::Focus => {
                    let (on, glyph, title, sub) = match m {
                        Module::Wifi => (
                            s.wifi_enabled,
                            "wifi",
                            "Wi-Fi",
                            if !s.wifi_enabled {
                                "Off".to_string()
                            } else if s.wired {
                                "Wired".to_string()
                            } else if s.ssid.is_empty() {
                                "Not Connected".to_string()
                            } else {
                                s.ssid.clone()
                            },
                        ),
                        Module::Bluetooth => (
                            s.bluetooth_powered,
                            "bluetooth",
                            "Bluetooth",
                            if s.bluetooth_powered { "On" } else { "Off" }.to_string(),
                        ),
                        _ => (
                            s.dnd,
                            "moon",
                            "Do Not Disturb",
                            if s.dnd { "On" } else { "Off" }.to_string(),
                        ),
                    };
                    cx.rec.layer(
                        "Disc",
                        Rect::new(r.x + 16.0, r.y + 14.0, 36.0, 36.0),
                        Shape::Ellipse,
                        Style::fill(if on { c.accent_amalfi } else { off_disc }),
                    );
                    cx.rec.glyph(
                        "Glyph",
                        Rect::new(r.x + 22.0, r.y + 20.0, 24.0, 24.0),
                        glyph,
                        400,
                        on_ink,
                    );
                    for (text, style, y, h, token) in [
                        (
                            title.to_string(),
                            &title_style,
                            16.0,
                            18.0,
                            typography::HEADLINE.name,
                        ),
                        (sub, &sub_style, 34.0, 14.0, typography::CAPTION.name),
                    ] {
                        if let Some(l) = self.labels.get_mut(li) {
                            let layout = l.layout(&text, style, Align::Start, 72.0, Some(1));
                            cx.rec.text(
                                "Label",
                                Rect::new(r.x + 60.0, r.y + y, 72.0, h),
                                layout,
                                Some(token),
                                style.color,
                            );
                        }
                        li += 1;
                    }
                }
                Module::DarkMode | Module::ScreenCapture => {
                    let (on, glyph) = match m {
                        Module::DarkMode => (s.dark, "moon"),
                        _ => (false, "camera"),
                    };
                    if on {
                        cx.rec.layer(
                            "Active fill",
                            r,
                            Shape::Ellipse,
                            Style::fill(c.accent_amalfi),
                        );
                    }
                    cx.rec.glyph(
                        "Glyph",
                        Rect::new(r.x + 20.0, r.y + 20.0, 24.0, 24.0),
                        glyph,
                        400,
                        if on { on_ink } else { c.text_primary },
                    );
                }
                Module::Display | Module::Sound => {
                    let (title, value, small, large) = match m {
                        Module::Display => ("Display", s.brightness, "sun-min", "sun"),
                        _ => (
                            "Sound",
                            if s.muted { 0.0 } else { s.volume },
                            "volume-0",
                            "volume-2",
                        ),
                    };
                    if let Some(l) = self.labels.get_mut(li) {
                        let layout = l.layout(title, &title_style, Align::Start, 200.0, Some(1));
                        cx.rec.text(
                            "Title",
                            Rect::new(r.x + 16.0, r.y + 8.0, 200.0, 18.0),
                            layout,
                            Some(typography::HEADLINE.name),
                            c.text_primary,
                        );
                    }
                    li += 1;
                    let t = track(r);
                    cx.rec.layer(
                        "Track",
                        t,
                        Shape::rounded(2.0),
                        Style::fill(c.surface_sunken),
                    );
                    cx.rec.layer(
                        "Fill",
                        Rect::new(t.x, t.y, t.width * value.clamp(0.0, 1.0) as f32, t.height),
                        Shape::rounded(2.0),
                        Style::fill(c.accent_amalfi),
                    );
                    cx.rec.glyph(
                        "Icon small",
                        Rect::new(r.x + 16.0, r.y + 34.0, 20.0, 20.0),
                        small,
                        400,
                        c.text_secondary,
                    );
                    cx.rec.glyph(
                        "Icon large",
                        Rect::new(r.x + 216.0, r.y + 34.0, 20.0, 20.0),
                        large,
                        400,
                        c.text_secondary,
                    );
                }
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_>, event: &Event) -> Handled {
        let b = cx.bounds();
        let o = stack_origin(b);
        let s = self.system.borrow().state.clone();
        match event {
            Event::PointerDown {
                pos,
                button: Button::Left,
                ..
            } => {
                let p = Point::new(pos.x - o.x, pos.y - o.y);
                if let Some((m, r)) = layout(&s).into_iter().find(|(_, r)| r.contains(p)) {
                    self.press(cx, m, r.translate(o), *pos);
                } else if let Some(w) = cx.window() {
                    // The shadows' margin is outside the panel: a click there closes it.
                    let ui = cx.ui();
                    if let Some(root) = ui.window(w).map(|w| w.root) {
                        ui.emit(root, Signal::Closed, Value::None);
                    }
                    ui.close_window(w);
                }
                Handled::Yes
            }
            Event::PointerMove { pos, .. } => {
                if let Some(m) = self.dragging
                    && let Some((_, r)) = layout(&s).into_iter().find(|(k, _)| *k == m)
                {
                    self.drag(cx, r.translate(o), pos.x);
                    return Handled::Yes;
                }
                Handled::No
            }
            Event::PointerUp { .. } => {
                self.dragging = None;
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn accessibility(&self, a: &mut A11y) {
        a.role = Role::Group;
        a.label = Some("Control Center".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_follow_the_kit_grid() {
        let s = SystemState {
            known: true,
            brightness_available: true,
            audio_available: true,
            ..SystemState::default()
        };
        let l = layout(&s);
        assert_eq!(l[1].1, Rect::new(152.0, 0.0, 140.0, 64.0));
        assert_eq!(l[4].1, Rect::new(228.0, 76.0, 64.0, 64.0));
        assert_eq!(height(&s), 4.0 * 76.0 - 12.0);
        let no_display = SystemState {
            known: true,
            audio_available: true,
            ..SystemState::default()
        };
        assert_eq!(height(&no_display), 3.0 * 76.0 - 12.0);
        assert_eq!(slider_value(Rect::new(0.0, 0.0, 292.0, 64.0), 126.0), 0.5);
    }
}
