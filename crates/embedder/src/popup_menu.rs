//! The `silica/popupmenu` channel: the app's context menus and pop-up buttons through the
//! system menu service (silica-menu-v1 version 2, silica-desktop#241).
//!
//! Messages (JSON method codec, in-process):
//! - from Dart: `available` → whether Ossido offers the service; `show {token, viewId, kind,
//!   anchor: [x, y, w, h], selected, rows}` → `true` once the menu is sent, `false` when it
//!   cannot be (no service, the view is gone, rows past the limits: Ossido would end the
//!   connection of a client that sent them); `cancel {token}` → the menu ends silently;
//! - to Dart: `result {token, id}` when the menu ends: the id of the chosen row, null when
//!   it was dismissed (a press outside, a change of context, Esc).
//!
//! Rows: `{kind, id, label, flags, icon, key, modifiers}` with kind 0 item, 1 separator,
//! 2 section, 3 submenu begin, 4 submenu end.

use serde_json::{Value, json};

/// The protocol's limits (`silica_popup_menu_v1`).
const MAX_ROWS: usize = 1024;
const MAX_DEPTH: usize = 4;
const MAX_TEXT: usize = 256;

pub const CHANNEL: &str = "silica/popupmenu";

/// A row, as the requests need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Item {
        id: u32,
        label: String,
        flags: u32,
        icon: String,
        key: String,
        modifiers: u32,
    },
    Separator,
    Section(String),
    SubmenuBegin {
        label: String,
        flags: u32,
        icon: String,
    },
    SubmenuEnd,
}

/// A menu from Dart, checked.
#[derive(Debug, PartialEq, Eq)]
pub struct Menu {
    pub token: u64,
    pub popup: bool,
    pub anchor: [i32; 4],
    pub selected: u32,
    pub rows: Vec<Row>,
}

/// The arguments of `show`, within the protocol's limits.
pub fn parse(args: &Value) -> Result<Menu, String> {
    let token = args
        .get("token")
        .and_then(Value::as_u64)
        .ok_or("no token")?;
    let anchor = args
        .get("anchor")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 4)
        .ok_or("no anchor")?;
    let mut rect = [0i32; 4];
    for (slot, value) in rect.iter_mut().zip(anchor) {
        let number = value
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("bad anchor")?;
        *slot = number.round().clamp(-100_000.0, 100_000.0) as i32;
    }
    let selected = args
        .get("selected")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0);
    let list = args
        .get("rows")
        .and_then(Value::as_array)
        .ok_or("no rows")?;
    if list.len() > MAX_ROWS {
        return Err("too many rows".to_owned());
    }
    let mut rows = Vec::with_capacity(list.len());
    let mut depth = 0usize;
    let mut ids = std::collections::HashSet::new();
    for row in list {
        let number = |name: &str| {
            row.get(name)
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0)
        };
        let text = |name: &str| -> Result<String, String> {
            let text = row.get(name).and_then(Value::as_str).unwrap_or_default();
            if text.len() > MAX_TEXT {
                return Err(format!("{name} longer than {MAX_TEXT} bytes"));
            }
            Ok(text.to_owned())
        };
        rows.push(match number("kind") {
            0 => {
                let id = number("id");
                if id == 0 || !ids.insert(id) {
                    return Err("an item id is zero or repeated".to_owned());
                }
                Row::Item {
                    id,
                    label: text("label")?,
                    flags: number("flags") & 0xf,
                    icon: text("icon")?,
                    key: text("key")?,
                    modifiers: number("modifiers") & 0xf,
                }
            }
            1 => Row::Separator,
            2 => Row::Section(text("label")?),
            3 => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err("too many levels of submenu".to_owned());
                }
                Row::SubmenuBegin {
                    label: text("label")?,
                    flags: number("flags") & 0xf,
                    icon: text("icon")?,
                }
            }
            4 => {
                depth = depth
                    .checked_sub(1)
                    .ok_or("a submenu ends that never began")?;
                Row::SubmenuEnd
            }
            other => return Err(format!("row kind {other}")),
        });
    }
    if depth != 0 {
        return Err("a submenu is left open".to_owned());
    }
    if ids.is_empty() {
        return Err("no item".to_owned());
    }
    Ok(Menu {
        token,
        popup: args.get("kind").and_then(Value::as_u64) == Some(1),
        anchor: rect,
        selected,
        rows,
    })
}

/// The `result` message for Dart.
pub fn result_message(token: u64, id: Option<u32>) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "method": "result",
        "args": { "token": token, "id": id },
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    fn item(id: u64) -> Value {
        json!({"kind": 0, "id": id, "label": "x", "flags": 18})
    }

    fn menu(rows: Value) -> Value {
        json!({"token": 3, "kind": 1, "anchor": [1.4, 2.6, 80, 22], "selected": 2, "rows": rows})
    }

    #[test]
    fn a_menu_within_the_limits_is_read() {
        let parsed = parse(&menu(json!([
            item(1),
            {"kind": 1},
            {"kind": 3, "label": "More"},
            item(2),
            {"kind": 4},
        ])))
        .unwrap();
        assert_eq!(parsed.token, 3);
        assert!(parsed.popup);
        assert_eq!(parsed.anchor, [1, 3, 80, 22]);
        assert_eq!(parsed.rows.len(), 5);
        // Flags past the defined bits are dropped.
        assert!(matches!(&parsed.rows[0], Row::Item { flags: 2, .. }));
    }

    #[test]
    fn what_ossido_would_refuse_never_leaves() {
        assert!(parse(&menu(json!([item(1), item(1)]))).is_err());
        assert!(parse(&menu(json!([item(0)]))).is_err());
        assert!(parse(&menu(json!([{"kind": 1}]))).is_err());
        assert!(parse(&menu(json!([item(1), {"kind": 4}]))).is_err());
        assert!(parse(&menu(json!([item(1), {"kind": 3, "label": "a"}]))).is_err());
        let long = "x".repeat(300);
        assert!(parse(&menu(json!([{"kind": 0, "id": 1, "label": long}]))).is_err());
        let deep: Vec<Value> = (0..5)
            .map(|_| json!({"kind": 3}))
            .chain([item(1)])
            .chain((0..5).map(|_| json!({"kind": 4})))
            .collect();
        assert!(parse(&menu(Value::Array(deep))).is_err());
        let many: Vec<Value> = (1..=1025).map(item).collect();
        assert!(parse(&menu(Value::Array(many))).is_err());
        assert!(parse(&json!({"token": 1})).is_err());
    }

    #[test]
    fn the_result_carries_the_id_or_null() {
        let m: Value = serde_json::from_slice(&result_message(5, Some(2))).unwrap();
        assert_eq!(m["args"], json!({"token": 5, "id": 2}));
        let m: Value = serde_json::from_slice(&result_message(5, None)).unwrap();
        assert!(m["args"]["id"].is_null());
    }
}
