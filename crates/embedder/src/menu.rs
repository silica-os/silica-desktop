//! The `silica/menu` channel: the app's menus for the global menu bar (silica-menu-v1,
//! silica-desktop#25), and on the shell's side the frontmost app's menus
//! (`silica_shell_manager_v1.app_menu`).
//!
//! Messages of `silica/menu` (JSON method codec):
//! - from Dart: `setMenus {menus: [...]}` publishes the model (the format of
//!   `silica-menu-v1.xml`) → `true` if sent, `false` without the protocol or with a model
//!   past its limits; `clearMenus` → the app has no menus; `isSupported` → whether Ossido
//!   offers the protocol;
//! - to Dart: `activated id` when the user chooses an item in the bar.
//!
//! The model is checked here with the protocol's own rules before it leaves: Ossido ends the
//! connection of an app that sends a model it refuses.
//!
//! On `silica/shell` (the shell components) the frontmost app's menus arrive as `appMenu
//! {app_id, serial, model}` (model null without menus); `activateMenuItem {serial, id}` sends
//! the choice back.

use std::os::fd::{AsFd, OwnedFd};

use serde_json::{Value, json};
use silica_protocols::menu::MAX_MODEL_SIZE;
use silica_protocols::menu::client::silica_menu_manager_v1::SilicaMenuManagerV1;
use silica_protocols::menu::client::silica_menu_v1::SilicaMenuV1;
use silica_protocols::menu::client::silica_popup_menu_v1::SilicaPopupMenuV1;
use silica_protocols::menu::model::parse_model;
use tracing::warn;
use wayland_client::{Dispatch, QueueHandle};

use crate::text_input::success;

pub const CHANNEL: &str = "silica/menu";

/// The app's side of silica-menu-v1.
pub struct MenuLink {
    manager: Option<SilicaMenuManagerV1>,
    menu: Option<SilicaMenuV1>,
    /// The last model sent (normal form): the same model is not sent twice.
    sent: Option<Vec<u8>>,
    /// The popup menus shown through the service (`popup_menu.rs`), by Dart's token.
    pub popups: std::collections::HashMap<u64, SilicaPopupMenuV1>,
}

impl MenuLink {
    pub fn new(manager: Option<SilicaMenuManagerV1>) -> Self {
        Self {
            manager,
            menu: None,
            sent: None,
            popups: std::collections::HashMap::new(),
        }
    }

    /// Whether Ossido offers the popup menu service (silica-menu-v1 version 2).
    pub fn popup_menus_available(&self) -> bool {
        self.manager
            .as_ref()
            .is_some_and(|manager| wayland_client::Proxy::version(manager) >= 2)
    }

    /// Sends a popup menu to Ossido on `surface`; false if the service is not offered.
    pub fn show_popup_menu<D>(
        &mut self,
        surface: &wayland_client::protocol::wl_surface::WlSurface,
        menu: &crate::popup_menu::Menu,
        qh: &QueueHandle<D>,
    ) -> bool
    where
        D: Dispatch<SilicaPopupMenuV1, u64> + 'static,
    {
        use crate::popup_menu::Row;
        use silica_protocols::menu::client::silica_menu_manager_v1::PopupKind;
        use silica_protocols::menu::client::silica_popup_menu_v1::{Flags, Modifiers};
        let Some(manager) = self
            .manager
            .as_ref()
            .filter(|_| self.popup_menus_available())
        else {
            return false;
        };
        let [x, y, width, height] = menu.anchor;
        let object = manager.get_popup_menu(
            surface,
            if menu.popup {
                PopupKind::PopUp
            } else {
                PopupKind::Context
            },
            x,
            y,
            width.max(0),
            height.max(0),
            menu.selected,
            qh,
            menu.token,
        );
        for row in &menu.rows {
            match row {
                Row::Item {
                    id,
                    label,
                    flags,
                    icon,
                    key,
                    modifiers,
                } => object.add_item(
                    *id,
                    label.clone(),
                    Flags::from_bits_truncate(*flags),
                    icon.clone(),
                    key.clone(),
                    Modifiers::from_bits_truncate(*modifiers),
                ),
                Row::Separator => object.add_separator(),
                Row::Section(label) => object.add_section(label.clone()),
                Row::SubmenuBegin { label, flags, icon } => object.begin_submenu(
                    label.clone(),
                    Flags::from_bits_truncate(*flags),
                    icon.clone(),
                ),
                Row::SubmenuEnd => object.end_submenu(),
            }
        }
        object.show();
        if let Some(old) = self.popups.insert(menu.token, object) {
            old.destroy();
        }
        true
    }

    /// A call from Dart; returns the reply.
    pub fn handle_call<D>(&mut self, message: &[u8], qh: &QueueHandle<D>) -> Vec<u8>
    where
        D: Dispatch<SilicaMenuV1, ()> + 'static,
    {
        let Ok(call) = serde_json::from_slice::<Value>(message) else {
            return Vec::new();
        };
        match call.get("method").and_then(Value::as_str) {
            Some("isSupported") => success(json!(self.manager.is_some())),
            Some("setMenus") => {
                let args = call.get("args").cloned().unwrap_or(Value::Null);
                success(json!(self.set_menus(&args, qh)))
            }
            Some("clearMenus") => {
                if let Some(menu) = self.menu.take() {
                    menu.destroy();
                }
                self.sent = None;
                success(json!(true))
            }
            _ => Vec::new(),
        }
    }

    fn set_menus<D>(&mut self, args: &Value, qh: &QueueHandle<D>) -> bool
    where
        D: Dispatch<SilicaMenuV1, ()> + 'static,
    {
        let Some(manager) = &self.manager else {
            return false;
        };
        let bytes = match normalize(args) {
            Ok(bytes) => bytes,
            Err(err) => {
                warn!("silica/menu: model refused: {err}");
                return false;
            }
        };
        if self.sent.as_ref() == Some(&bytes) {
            return true;
        }
        let file = match model_file(&bytes) {
            Ok(file) => file,
            Err(err) => {
                warn!("silica/menu: {err}");
                return false;
            }
        };
        let Ok(size) = u32::try_from(bytes.len()) else {
            return false;
        };
        let menu = self.menu.get_or_insert_with(|| manager.get_menu(qh, ()));
        menu.set_model(file.as_fd(), size);
        self.sent = Some(bytes);
        true
    }
}

/// The model from Dart in normal form, within the protocol's limits.
fn normalize(args: &Value) -> Result<Vec<u8>, String> {
    let raw = serde_json::to_vec(args).map_err(|err| err.to_string())?;
    let model = parse_model(&raw)?;
    let bytes = serde_json::to_vec(&model).map_err(|err| err.to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_MODEL_SIZE {
        return Err(format!("{} bytes", bytes.len()));
    }
    Ok(bytes)
}

/// A memfd with the model, sealed against changes.
fn model_file(bytes: &[u8]) -> std::io::Result<OwnedFd> {
    use std::io::Write;
    let fd = rustix::fs::memfd_create(
        "silica-menu",
        rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING,
    )?;
    let mut file = std::fs::File::from(fd);
    file.write_all(bytes)?;
    file.flush()?;
    let fd = OwnedFd::from(file);
    rustix::fs::fcntl_add_seals(
        &fd,
        rustix::fs::SealFlags::SHRINK
            | rustix::fs::SealFlags::GROW
            | rustix::fs::SealFlags::WRITE
            | rustix::fs::SealFlags::SEAL,
    )?;
    Ok(fd)
}

/// The `activated` message for Dart.
pub fn activated_message(id: u32) -> Vec<u8> {
    serde_json::to_vec(&json!({ "method": "activated", "args": id })).unwrap_or_default()
}

/// The frontmost app's menus for the shell (`appMenu` on `silica/shell`): the model read
/// from Ossido's file, null without menus or if the file cannot be read.
pub fn shell_message(app_id: &str, serial: u32, fd: OwnedFd, size: u32) -> Vec<u8> {
    let model = if serial == 0 || size == 0 {
        Value::Null
    } else {
        read_shell_model(fd, size).unwrap_or_else(|err| {
            warn!("app menu from Ossido: {err}");
            Value::Null
        })
    };
    let serial = if model.is_null() { 0 } else { serial };
    serde_json::to_vec(&json!({
        "method": "appMenu",
        "args": { "app_id": app_id, "serial": serial, "model": model },
    }))
    .unwrap_or_default()
}

fn read_shell_model(fd: OwnedFd, size: u32) -> Result<Value, String> {
    use std::os::unix::fs::FileExt;
    let size = usize::try_from(size).map_err(|err| err.to_string())?;
    if size > MAX_MODEL_SIZE {
        return Err(format!("{size} bytes"));
    }
    let file = std::fs::File::from(fd);
    let mut bytes = vec![0; size];
    // At offset 0 whatever the shared offset says.
    file.read_exact_at(&mut bytes, 0)
        .map_err(|err| err.to_string())?;
    serde_json::from_slice(&bytes).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    fn decode(message: &[u8]) -> Value {
        serde_json::from_slice(message).unwrap()
    }

    #[test]
    fn the_model_from_dart_is_put_in_normal_form() {
        let bytes = normalize(&json!({"menus": [
            {"label": "View", "items": [
                {"id": 3, "label": "Basic", "checked": true, "future": 1},
                {"separator": true, "id": 9},
            ]}
        ]}))
        .unwrap();
        assert_eq!(
            decode(&bytes),
            json!({"menus": [{"label": "View", "items": [
                {"id": 3, "label": "Basic", "enabled": true, "checked": true},
                {"separator": true, "enabled": true},
            ]}]})
        );
        // Past the limits it never reaches Ossido.
        let long = "x".repeat(300);
        assert!(normalize(&json!({"menus": [{"label": long}]})).is_err());
        assert!(normalize(&json!([1])).is_err());
    }

    #[test]
    fn the_shell_reads_the_model_at_offset_zero() {
        let file = model_file(br#"{"menus":[]}"#).unwrap();
        // The writer left the offset at the end: the reader does not care.
        let message = decode(&shell_message("org.silica.Calc", 4, file, 12));
        assert_eq!(message["method"], "appMenu");
        assert_eq!(message["args"]["app_id"], "org.silica.Calc");
        assert_eq!(message["args"]["serial"], 4);
        assert_eq!(message["args"]["model"], json!({"menus": []}));
        // Without a model, or with an unreadable one: null and serial 0.
        let file = model_file(b"").unwrap();
        let message = decode(&shell_message("org.silica.Calc", 0, file, 0));
        assert!(message["args"]["model"].is_null());
        let file = model_file(b"{").unwrap();
        let message = decode(&shell_message("org.silica.Calc", 5, file, 1));
        assert!(message["args"]["model"].is_null());
        assert_eq!(message["args"]["serial"], 0);
    }

    #[test]
    fn activations_go_to_dart() {
        assert_eq!(
            decode(&activated_message(12)),
            json!({"method": "activated", "args": 12})
        );
    }
}
