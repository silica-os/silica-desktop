//! The system's state from silica-daemon (`System1.State`): Wi-Fi, Bluetooth, battery,
//! display brightness, sound, Do Not Disturb, the appearance. Read when the shell starts,
//! again every few seconds and right after the shell changes something; the status items and
//! Control Center draw from it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use zbus::zvariant::OwnedValue;

/// What the status items and Control Center show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemState {
    /// The daemon answered at least once.
    pub known: bool,
    /// Wi-Fi hardware.
    pub wifi_available: bool,
    /// Wi-Fi on.
    pub wifi_enabled: bool,
    /// The joined network.
    pub ssid: String,
    /// Signal 0–100.
    pub strength: u8,
    /// A wired connection.
    pub wired: bool,
    /// Bluetooth hardware.
    pub bluetooth_available: bool,
    /// Bluetooth on.
    pub bluetooth_powered: bool,
    /// A battery.
    pub battery_present: bool,
    /// Its charge, 0–100.
    pub battery_percent: f64,
    /// Charging.
    pub charging: bool,
    /// A display with adjustable brightness.
    pub brightness_available: bool,
    /// Its brightness, 0–1.
    pub brightness: f64,
    /// Sound output.
    pub audio_available: bool,
    /// Volume, 0–1.
    pub volume: f64,
    /// Muted.
    pub muted: bool,
    /// Do Not Disturb.
    pub dnd: bool,
    /// Dark appearance.
    pub dark: bool,
}

fn get<T: TryFrom<OwnedValue>>(m: &HashMap<String, OwnedValue>, k: &str) -> Option<T> {
    m.get(k)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| T::try_from(v).ok())
}

impl SystemState {
    /// From the daemon's dictionary.
    pub fn from_dict(m: &HashMap<String, OwnedValue>) -> SystemState {
        SystemState {
            known: true,
            wifi_available: get(m, "wifi.available").unwrap_or(false),
            wifi_enabled: get(m, "wifi.enabled").unwrap_or(false),
            ssid: get(m, "wifi.ssid").unwrap_or_default(),
            strength: get(m, "wifi.strength").unwrap_or(0),
            wired: get(m, "wifi.wired").unwrap_or(false),
            bluetooth_available: get(m, "bluetooth.available").unwrap_or(false),
            bluetooth_powered: get(m, "bluetooth.powered").unwrap_or(false),
            battery_present: get(m, "battery.present").unwrap_or(false),
            battery_percent: get(m, "battery.percent").unwrap_or(0.0),
            charging: get(m, "battery.charging").unwrap_or(false),
            brightness_available: get(m, "brightness.available").unwrap_or(false),
            brightness: get(m, "brightness").unwrap_or(0.0),
            audio_available: get(m, "audio.available").unwrap_or(false),
            volume: get(m, "audio.volume").unwrap_or(0.0),
            muted: get(m, "audio.muted").unwrap_or(false),
            dnd: get(m, "dnd").unwrap_or(false),
            dark: get(m, "appearance.dark").unwrap_or(false),
        }
    }

    /// The Wi-Fi glyph: off, or by signal (`wifi-0/1/2`, full `wifi`).
    pub fn wifi_glyph(&self) -> &'static str {
        if !self.wifi_enabled {
            "wifi-slash"
        } else if self.strength >= 75 {
            "wifi"
        } else if self.strength >= 50 {
            "wifi-2"
        } else if self.strength >= 25 {
            "wifi-1"
        } else {
            "wifi-0"
        }
    }

    /// The sound glyph by level.
    pub fn volume_glyph(&self) -> &'static str {
        if self.muted || self.volume <= 0.0 {
            "volume-slash"
        } else if self.volume < 0.34 {
            "volume-0"
        } else if self.volume < 0.67 {
            "volume-1"
        } else {
            "volume-2"
        }
    }

    /// The battery glyph by charge (status-bar.md).
    pub fn battery_glyph(&self) -> &'static str {
        if self.charging {
            "battery-charging"
        } else if self.battery_percent >= 75.0 {
            "battery"
        } else if self.battery_percent >= 30.0 {
            "battery-50"
        } else if self.battery_percent >= 8.0 {
            "battery-10"
        } else {
            "battery-0"
        }
    }
}

/// Called with the core when the state changes.
pub type Listener = Rc<dyn Fn(&mut vetro_lib::core::Ui)>;

/// A shared state with the callbacks that redraw what shows it.
#[derive(Default)]
pub struct System {
    /// The state.
    pub state: SystemState,
    /// Called with the core when the state changes.
    pub listeners: Vec<Listener>,
}

/// Shared.
pub type Shared = Rc<RefCell<System>>;

/// Reads the state now; the listeners run when it changed.
pub fn refresh(system: &Shared) {
    let s = system.clone();
    vetro_lib::platform::spawn(async move {
        let Some(services) = vetro_lib::platform::services() else {
            return;
        };
        let Ok(proxy) = services.system().await else {
            return;
        };
        let Ok(dict) = proxy.state().await else {
            return;
        };
        let state = SystemState::from_dict(&dict);
        vetro_lib::platform::post(move |ui| {
            let changed = s.borrow().state != state;
            if changed {
                s.borrow_mut().state = state;
                let listeners = s.borrow().listeners.clone();
                for l in listeners {
                    l(ui);
                }
            }
        });
    });
}

/// Polls the state every two seconds.
pub fn watch(ui: &mut vetro_lib::core::Ui, system: Shared) {
    refresh(&system);
    ui.after(2.0, move |ui| watch(ui, system));
}

/// Runs a call on System1, then reads the state again.
pub fn call<F, Fut>(system: &Shared, f: F)
where
    F: FnOnce(vetro_lib::services::System1Proxy<'static>) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    let s = system.clone();
    vetro_lib::platform::spawn(async move {
        if let Some(services) = vetro_lib::platform::services()
            && let Ok(proxy) = services.system().await
        {
            f(proxy).await;
        }
        refresh(&s);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_follow_the_state() {
        let mut s = SystemState {
            wifi_enabled: true,
            strength: 60,
            volume: 0.8,
            battery_percent: 40.0,
            ..SystemState::default()
        };
        assert_eq!(s.wifi_glyph(), "wifi-2");
        assert_eq!(s.volume_glyph(), "volume-2");
        assert_eq!(s.battery_glyph(), "battery-50");
        s.muted = true;
        s.wifi_enabled = false;
        assert_eq!(s.volume_glyph(), "volume-slash");
        assert_eq!(s.wifi_glyph(), "wifi-slash");
    }
}
