//! The `silica/shell` channel, for the shell components: the open windows
//! (`ext-foreign-toplevel-list-v1`) to Dart, and `activate` and `launch` to Ossido
//! (`silica-shell-v1`). Outside the shell Ossido does not show these protocols: the
//! calls answer `false` and the list stays empty.
//!
//! Messages (JSON method codec):
//! - from Dart: `list` → the list; `focused` → the focused identifier;
//!   `activate {id}` (that window), `activateApp {id}` (all its app's windows);
//!   `launch {command}`; `execute {id}` (catalog command) → `true` if sent;
//! - to Dart: `toplevels [{id, title, app_id, minimized}]` and `focused id` on every
//!   change, `command id` when Ossido forwards a command to the shell (e.g.
//!   `faro.toggle`), `broadcast {topic, value}` (a state of another shell component),
//!   `minimized [id…]` (the minimized ones, from the first minimized); `windowMenu {id, x,
//!   y}` (long press on the green button: the arrangement popover with its tip at x, y);
//!   `transientDismissed` (a press outside the open menu or panel ended it, #149);
//! - `activateMenuItem {serial, id}` from Dart: an item of the frontmost app's menus, which
//!   arrive as `appMenu {app_id, serial, model}` (`menu.rs`, version 8); `appMenu` from
//!   Dart → the last ones;
//! - `popupMenu {serial, app_id, kind, anchor: [x, y, w, h], selected, appearance, rows}` to
//!   Dart when an app shows a context menu or pop-up button's menu (version 9, #241); rows are
//!   `{kind, id, label, flags, icon, key, modifiers}`; `popupMenuClosed serial` when Ossido
//!   ended it; `popupMenuChoose {serial, id}` and `popupMenuClose {serial}` from Dart;
//! - `thumbnail {id}` from Dart asks for the thumbnail of a minimized window: it arrives as
//!   `thumbnail {id, width, height, path}`, a premultiplied RGBA file that Dart reads and
//!   deletes (width 0 if there is none).

use serde_json::{Value, json};
use silica_protocols::shell::client::silica_shell_manager_v1::SilicaShellManagerV1;
use wayland_client::{Proxy, QueueHandle};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::{
    self, ExtForeignToplevelHandleV1,
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1;

use crate::text_input::success;

pub const CHANNEL: &str = "silica/shell";

/// Maximum side of the thumbnails (silica-shell-v1 version 2).
const THUMBNAIL_MAX: u32 = 256;

/// A window announced by Ossido.
struct Toplevel {
    handle: ExtForeignToplevelHandleV1,
    identifier: String,
    title: String,
    app_id: String,
}

pub struct ShellLink {
    manager: Option<SilicaShellManagerV1>,
    /// The window list: kept alive so that Ossido keeps announcing them.
    _list: Option<ExtForeignToplevelListV1>,
    toplevels: Vec<Toplevel>,
    /// The focused window (identifier), empty if none.
    focused: String,
    /// The minimized windows, from the first minimized.
    minimized: Vec<String>,
    /// The thumbnails asked for and not yet arrived: identifier and file.
    thumbnails: std::cell::RefCell<Vec<(String, std::path::PathBuf)>>,
    serial: std::cell::Cell<u64>,
    /// The frontmost app's menus, as last sent to Dart (`appMenu`).
    app_menu: Value,
    /// The states Ossido announced (`broadcast`), kept for `states`: the ones sent before Dart
    /// listened (a shell started again while the session is locked) would be lost.
    states: std::collections::BTreeMap<String, String>,
    /// The popup menu whose rows are arriving.
    popup: Option<PopupBuild>,
}

/// A popup menu (`silica_shell_manager_v1.popup_menu`) while its rows arrive.
struct PopupBuild {
    serial: u32,
    head: Value,
    rows: Vec<Value>,
}

/// The most rows kept for one menu (the protocol's limit plus the submenu ends).
const MAX_POPUP_ROWS: usize = 2048;

impl ShellLink {
    /// The shell globals, if Ossido shows them to this process.
    pub fn new(
        manager: Option<SilicaShellManagerV1>,
        list: Option<ExtForeignToplevelListV1>,
    ) -> Self {
        Self {
            manager,
            _list: list,
            toplevels: Vec::new(),
            focused: String::new(),
            minimized: Vec::new(),
            thumbnails: std::cell::RefCell::new(Vec::new()),
            serial: std::cell::Cell::new(0),
            app_menu: Value::Null,
            states: std::collections::BTreeMap::new(),
            popup: None,
        }
    }

    /// `popup_menu`: a new menu starts (silica-desktop#241).
    pub fn begin_popup_menu(
        &mut self,
        serial: u32,
        app_id: String,
        kind: u32,
        anchor: [i32; 4],
        selected: u32,
        appearance: u32,
    ) {
        self.popup = Some(PopupBuild {
            serial,
            head: json!({
                "serial": serial,
                "app_id": app_id,
                "kind": kind,
                "anchor": anchor,
                "selected": selected,
                "appearance": crate::window_appearance::name(appearance),
            }),
            rows: Vec::new(),
        });
    }

    /// `popup_menu_row`.
    #[allow(clippy::too_many_arguments)]
    pub fn popup_menu_row(
        &mut self,
        serial: u32,
        kind: u32,
        id: u32,
        label: String,
        flags: u32,
        icon: String,
        key: String,
        modifiers: u32,
    ) {
        if let Some(build) = self.popup.as_mut().filter(|b| b.serial == serial)
            && build.rows.len() < MAX_POPUP_ROWS
        {
            build.rows.push(json!({
                "kind": kind, "id": id, "label": label, "flags": flags,
                "icon": icon, "key": key, "modifiers": modifiers,
            }));
        }
    }

    /// `popup_menu_end`: the `popupMenu` message for Dart.
    pub fn end_popup_menu(&mut self, serial: u32) -> Option<Vec<u8>> {
        if self.popup.as_ref().is_none_or(|b| b.serial != serial) {
            return None;
        }
        let build = self.popup.take()?;
        let mut args = build.head;
        if let Value::Object(map) = &mut args {
            map.insert("rows".to_owned(), Value::Array(build.rows));
        }
        serde_json::to_vec(&json!({ "method": "popupMenu", "args": args })).ok()
    }

    /// The shell manager, if Ossido shows it to this process.
    pub fn manager(&self) -> Option<&SilicaShellManagerV1> {
        self.manager.as_ref()
    }

    /// The focus changed: the `focused` message for Dart.
    pub fn set_focused(&mut self, identifier: String) -> Vec<u8> {
        self.focused = identifier;
        serde_json::to_vec(&json!({ "method": "focused", "args": self.focused }))
            .unwrap_or_default()
    }

    /// Ossido announced a state (at most 64 topics, like Ossido keeps).
    pub fn remember_state(&mut self, topic: &str, value: &str) {
        if self.states.contains_key(topic) || self.states.len() < 64 {
            self.states.insert(topic.to_owned(), value.to_owned());
        }
    }

    /// The frontmost app's menus changed (`menu::shell_message`): kept for `appMenu`, the
    /// call Dart makes at startup, after messages it may have missed.
    pub fn remember_app_menu(&mut self, message: &[u8]) {
        if let Ok(message) = serde_json::from_slice::<Value>(message) {
            self.app_menu = message.get("args").cloned().unwrap_or(Value::Null);
        }
    }

    /// A new window: the details arrive with the events of its handle.
    pub fn add(&mut self, handle: ExtForeignToplevelHandleV1) {
        self.toplevels.push(Toplevel {
            handle,
            identifier: String::new(),
            title: String::new(),
            app_id: String::new(),
        });
    }

    /// An event of a handle. Returns true when the list must be sent to Dart
    /// (`done` or `closed`).
    pub fn event(
        &mut self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
    ) -> bool {
        let Some(index) = self.toplevels.iter().position(|t| &t.handle == handle) else {
            return false;
        };
        match event {
            ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                if let Some(t) = self.toplevels.get_mut(index) {
                    t.identifier = identifier;
                }
                false
            }
            ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                if let Some(t) = self.toplevels.get_mut(index) {
                    t.title = title;
                }
                false
            }
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                if let Some(t) = self.toplevels.get_mut(index) {
                    t.app_id = app_id;
                }
                false
            }
            ext_foreign_toplevel_handle_v1::Event::Done => true,
            ext_foreign_toplevel_handle_v1::Event::Closed => {
                let closed = self.toplevels.remove(index);
                closed.handle.destroy();
                true
            }
            _ => false,
        }
    }

    /// A window minimized or restored: the `minimized` message for Dart.
    pub fn set_minimized(&mut self, identifier: String, minimized: bool) -> Vec<u8> {
        self.minimized.retain(|id| *id != identifier);
        if minimized {
            self.minimized.push(identifier);
        }
        serde_json::to_vec(&json!({ "method": "minimized", "args": self.minimized }))
            .unwrap_or_default()
    }

    /// Asks Ossido for the thumbnail into a new file in `$XDG_RUNTIME_DIR`.
    fn request_thumbnail(&self, identifier: &str) -> bool {
        let Some(manager) = self.manager.as_ref().filter(|m| m.version() >= 2) else {
            return false;
        };
        if identifier.is_empty() {
            return false;
        }
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map_or_else(std::env::temp_dir, std::path::PathBuf::from)
            .join(format!("silica-thumbnails-{}", std::process::id()));
        let serial = self.serial.get().wrapping_add(1);
        self.serial.set(serial);
        let path = dir.join(format!("{serial}.rgba"));
        let size = u64::from(THUMBNAIL_MAX) * u64::from(THUMBNAIL_MAX) * 4;
        let file = std::fs::create_dir_all(&dir).and_then(|()| {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)?;
            file.set_len(size)?;
            Ok(file)
        });
        match file {
            Ok(file) => {
                manager.thumbnail(identifier.to_owned(), std::os::fd::AsFd::as_fd(&file));
                self.thumbnails
                    .borrow_mut()
                    .push((identifier.to_owned(), path));
                true
            }
            Err(err) => {
                tracing::warn!("thumbnail: {err}");
                false
            }
        }
    }

    /// The thumbnail is in the file: the `thumbnail` message for Dart.
    pub fn thumbnail_ready(&self, identifier: &str, width: u32, height: u32) -> Vec<u8> {
        let mut pending = self.thumbnails.borrow_mut();
        let Some(index) = pending.iter().position(|(id, _)| id == identifier) else {
            return Vec::new();
        };
        let (_, path) = pending.remove(index);
        if width == 0 || height == 0 || width > THUMBNAIL_MAX || height > THUMBNAIL_MAX {
            let _ = std::fs::remove_file(&path);
            return serde_json::to_vec(&json!({
                "method": "thumbnail",
                "args": {"id": identifier, "width": 0, "height": 0, "path": ""},
            }))
            .unwrap_or_default();
        }
        serde_json::to_vec(&json!({
            "method": "thumbnail",
            "args": {"id": identifier, "width": width, "height": height, "path": path},
        }))
        .unwrap_or_default()
    }

    /// The window list, as Dart sees it.
    pub fn list(&self) -> Value {
        Value::Array(
            self.toplevels
                .iter()
                .filter(|t| !t.identifier.is_empty())
                .map(|t| {
                    json!({
                        "id": t.identifier,
                        "title": t.title,
                        "app_id": t.app_id,
                        "minimized": self.minimized.contains(&t.identifier),
                    })
                })
                .collect(),
        )
    }

    /// The `toplevels` message for Dart.
    pub fn toplevels_message(&self) -> Vec<u8> {
        serde_json::to_vec(&json!({ "method": "toplevels", "args": self.list() }))
            .unwrap_or_default()
    }

    /// A call from Dart; returns the reply.
    pub fn handle_call<D: 'static>(&self, message: &[u8], _qh: &QueueHandle<D>) -> Vec<u8> {
        let Ok(call) = serde_json::from_slice::<Value>(message) else {
            return Vec::new();
        };
        let args = call.get("args").cloned().unwrap_or(Value::Null);
        let text = |name: &str| {
            args.get(name)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_default()
        };
        match call.get("method").and_then(Value::as_str) {
            Some("list") => success(self.list()),
            Some("minimized") => success(json!(self.minimized)),
            Some("thumbnail") => success(json!(self.request_thumbnail(&text("id")))),
            Some("focused") => success(json!(self.focused)),
            Some("appMenu") => success(self.app_menu.clone()),
            Some("states") => success(json!(self.states)),
            Some("activate") => {
                let sent = self.manager.as_ref().is_some_and(|manager| {
                    manager.activate(text("id"));
                    true
                });
                success(json!(sent))
            }
            // The whole app (version 10); an older Ossido's `activate` already brings it.
            Some("activateApp") => {
                let sent = self.manager.as_ref().is_some_and(|manager| {
                    if manager.version() >= 10 {
                        manager.activate_app(text("id"));
                    } else {
                        manager.activate(text("id"));
                    }
                    true
                });
                success(json!(sent))
            }
            Some("execute") => {
                let id = text("id");
                // With parameters (a JSON object) version 5 is needed.
                let params = call
                    .pointer("/args/params")
                    .filter(|p| p.is_object())
                    .map(ToString::to_string);
                let sent = !id.is_empty()
                    && self.manager.as_ref().is_some_and(|manager| match params {
                        Some(params) if manager.version() >= 5 => {
                            manager.execute_with(id, params);
                            true
                        }
                        Some(_) => false,
                        None => {
                            manager.execute(id);
                            true
                        }
                    });
                success(json!(sent))
            }
            Some("broadcast") => {
                let topic = text("topic");
                let value = text("value");
                let sent = !topic.is_empty()
                    && self
                        .manager
                        .as_ref()
                        .is_some_and(|manager| manager.version() >= 4)
                    && self.manager.as_ref().is_some_and(|manager| {
                        manager.broadcast(topic, value);
                        true
                    });
                success(json!(sent))
            }
            Some("activateMenuItem") => {
                let number = |name: &str| {
                    args.get(name)
                        .and_then(Value::as_u64)
                        .and_then(|n| u32::try_from(n).ok())
                };
                let sent = match (self.manager.as_ref(), number("serial"), number("id")) {
                    (Some(manager), Some(serial), Some(id))
                        if manager.version() >= 8 && serial != 0 =>
                    {
                        manager.activate_menu_item(serial, id);
                        true
                    }
                    _ => false,
                };
                success(json!(sent))
            }
            Some("popupMenuChoose") => {
                let number = |name: &str| {
                    args.get(name)
                        .and_then(Value::as_u64)
                        .and_then(|n| u32::try_from(n).ok())
                };
                let sent = match (self.manager.as_ref(), number("serial"), number("id")) {
                    (Some(manager), Some(serial), Some(id)) if manager.version() >= 9 => {
                        manager.popup_menu_choose(serial, id);
                        true
                    }
                    _ => false,
                };
                success(json!(sent))
            }
            Some("popupMenuClose") => {
                let serial = args
                    .get("serial")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok());
                let sent = match (self.manager.as_ref(), serial) {
                    (Some(manager), Some(serial)) if manager.version() >= 9 => {
                        manager.popup_menu_close(serial);
                        true
                    }
                    _ => false,
                };
                success(json!(sent))
            }
            Some("launch") => {
                let command = text("command");
                let sent = !command.is_empty()
                    && self.manager.as_ref().is_some_and(|manager| {
                        manager.launch(command);
                        true
                    });
                success(json!(sent))
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn a_popup_menu_reaches_dart_when_its_rows_are_complete() {
        let mut link = ShellLink::new(None, None);
        link.begin_popup_menu(4, "org.silica.A".into(), 1, [10, 20, 80, 22], 2, 2);
        // Rows of another menu are dropped.
        link.popup_menu_row(9, 0, 7, "Other".into(), 0, String::new(), String::new(), 0);
        link.popup_menu_row(4, 0, 2, "Copy".into(), 2, "doc".into(), "C".into(), 1);
        link.popup_menu_row(4, 1, 0, String::new(), 0, String::new(), String::new(), 0);
        assert!(link.end_popup_menu(9).is_none());
        let message = link.end_popup_menu(4).unwrap();
        let message: Value = serde_json::from_slice(&message).unwrap();
        assert_eq!(message["method"], "popupMenu");
        let args = &message["args"];
        assert_eq!(args["anchor"], json!([10, 20, 80, 22]));
        assert_eq!(args["appearance"], "dark");
        assert_eq!(args["rows"].as_array().unwrap().len(), 2);
        assert_eq!(args["rows"][0]["label"], "Copy");
        // Taken: a second end finds nothing.
        assert!(link.end_popup_menu(4).is_none());
    }

    #[test]
    fn the_states_announced_before_dart_listened_are_kept_for_it() {
        let mut link = ShellLink::new(None, None);
        link.remember_state("session.locked", "1");
        link.remember_state("session.locked", "0");
        link.remember_state("screensaver.active", "");
        assert_eq!(
            link.states.get("session.locked").map(String::as_str),
            Some("0")
        );
        assert_eq!(link.states.len(), 2);
    }
}
