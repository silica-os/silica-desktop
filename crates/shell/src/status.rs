//! The status cluster (docs/design/amalfi/status-bar.md): the same items in the menu bar and
//! on the lock screen, drawn from the system's state. Menu bar: Bluetooth, Wi-Fi, Sound,
//! Battery, Search, Clock, Control Center (the unavailable ones hidden); lock screen: Wi-Fi
//! and Battery.

use std::rc::Rc;

use vetro_lib::amalfi::statusbar::{ItemContent, StatusItem, status_bar};
use vetro_lib::core::{Prop, Signal, Ui, Value, ViewId};

use crate::system::SystemState;

/// An item of the cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Bluetooth.
    Bluetooth,
    /// Wi-Fi.
    Wifi,
    /// Sound.
    Sound,
    /// Battery, with its percentage.
    Battery,
    /// Faro.
    Search,
    /// The date and time.
    Clock,
    /// Control Center.
    ControlCenter,
}

/// The menu bar's items, leading to trailing.
pub const MENU_BAR: &[Kind] = &[
    Kind::Bluetooth,
    Kind::Wifi,
    Kind::Sound,
    Kind::Battery,
    Kind::Search,
    Kind::Clock,
    Kind::ControlCenter,
];

/// The lock screen's items.
pub const LOCK_SCREEN: &[Kind] = &[Kind::Wifi, Kind::Battery];

/// What a click on an item does.
pub type OnClick = Rc<dyn Fn(&mut Ui, Kind, ViewId)>;

/// A cluster: its view and its items.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// The status bar view.
    pub view: ViewId,
    /// The items.
    pub items: Vec<(Kind, ViewId)>,
}

fn title(k: Kind) -> &'static str {
    match k {
        Kind::Bluetooth => "Bluetooth",
        Kind::Wifi => "Wi-Fi",
        Kind::Sound => "Sound",
        Kind::Battery => "Battery",
        Kind::Search => "Search",
        Kind::Clock => "Clock",
        Kind::ControlCenter => "Control Center",
    }
}

/// Makes a cluster of `kinds`; `on_click` runs when an item is clicked (none: not clickable).
pub fn cluster(ui: &mut Ui, kinds: &[Kind], on_click: Option<OnClick>) -> Cluster {
    let mut items = Vec::new();
    for k in kinds {
        let content = match k {
            Kind::Clock => ItemContent::Clock(String::new()),
            Kind::Battery => ItemContent::Battery {
                symbol: "battery".into(),
                percentage: String::new(),
            },
            Kind::Bluetooth => ItemContent::Glyph("bluetooth".into()),
            Kind::Wifi => ItemContent::Glyph("wifi".into()),
            Kind::Sound => ItemContent::Glyph("volume-2".into()),
            Kind::Search => ItemContent::Glyph("search".into()),
            Kind::ControlCenter => ItemContent::Glyph("control-center".into()),
        };
        let v = ui.create(StatusItem::new(content).titled(title(*k)));
        if let Some(f) = &on_click {
            let f = f.clone();
            let kind = *k;
            ui.connect(v, Signal::Activated, move |ui, item, _| f(ui, kind, item));
        }
        items.push((*k, v));
    }
    let ids: Vec<ViewId> = items.iter().map(|i| i.1).collect();
    let view = status_bar(ui, &ids);
    Cluster { view, items }
}

fn set_if_changed(ui: &mut Ui, v: ViewId, p: Prop, value: Value) {
    if ui.get(v, p).as_ref() != Some(&value) {
        let _ = ui.set(v, p, value);
    }
}

/// Shows `state` and the clock's text in the cluster.
pub fn update(ui: &mut Ui, c: &Cluster, state: &SystemState, clock: &str) {
    for (k, v) in &c.items {
        let (shown, symbol, text) = match k {
            Kind::Bluetooth => (
                state.bluetooth_available,
                Some(if state.bluetooth_powered {
                    "bluetooth"
                } else {
                    "bluetooth-slash"
                }),
                None,
            ),
            Kind::Wifi => (
                state.wifi_available || !state.known,
                Some(state.wifi_glyph()),
                None,
            ),
            Kind::Sound => (state.audio_available, Some(state.volume_glyph()), None),
            Kind::Battery => (
                state.battery_present,
                Some(state.battery_glyph()),
                Some(format!("{:.0}%", state.battery_percent)),
            ),
            Kind::Clock => (true, None, Some(clock.to_string())),
            Kind::Search | Kind::ControlCenter => (true, None, None),
        };
        set_if_changed(ui, *v, Prop::Hidden, Value::Bool(!shown));
        if let Some(s) = symbol {
            set_if_changed(ui, *v, Prop::Symbol, Value::Str(s.into()));
        }
        if let Some(t) = text {
            set_if_changed(ui, *v, Prop::Text, Value::Str(t));
        }
    }
}

/// Marks an item active (its menu or panel is open).
pub fn set_active(ui: &mut Ui, c: &Cluster, kind: Kind, active: bool) {
    for (k, v) in &c.items {
        if *k == kind {
            set_if_changed(ui, *v, Prop::Expanded, Value::Bool(active));
        }
    }
}
