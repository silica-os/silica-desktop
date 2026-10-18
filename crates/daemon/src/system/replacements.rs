//! Text replacements (Settings › Keyboard › Text Replacements): short texts that become
//! phrases as the user types (`omw` → `On my way!`). The daemon keeps the list in
//! `$XDG_CONFIG_HOME/silica/text-replacements.json` and hands it to the apps with the rest of
//! its state (`text.replacements`, `a(ss)`), where silica_ui's text fields apply it; Settings
//! changes it with `System1.SetTextReplacements(a(ss))`.

use std::path::Path;

use serde_json::{Value, json};

/// How many replacements are kept.
pub const MAX_ENTRIES: usize = 500;
/// The longest shortcut and phrase, in characters.
pub const MAX_SHORTCUT: usize = 40;
pub const MAX_PHRASE: usize = 1000;

/// A list of (shortcut, phrase) as the apps get it.
pub type Replacements = Vec<(String, String)>;

/// Whether the list is acceptable: shortcuts are non-empty, without spaces and not repeated,
/// phrases are non-empty, the sizes are within the limits.
pub fn valid(list: &[(String, String)]) -> bool {
    list.len() <= MAX_ENTRIES
        && list.iter().enumerate().all(|(i, (shortcut, phrase))| {
            let n = shortcut.chars().count();
            (1..=MAX_SHORTCUT).contains(&n)
                && !shortcut.chars().any(char::is_whitespace)
                && (1..=MAX_PHRASE).contains(&phrase.chars().count())
                && list.iter().skip(i + 1).all(|(other, _)| other != shortcut)
        })
}

/// The saved list; empty without the file or when it is unreadable, and a bad entry is skipped.
pub fn read(file: &Path) -> Replacements {
    let Some(Value::Object(root)) = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return Vec::new();
    };
    let mut list: Replacements = Vec::new();
    for item in root
        .get("replacements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let pair = (
            item.get("shortcut").and_then(Value::as_str),
            item.get("phrase").and_then(Value::as_str),
        );
        if let (Some(shortcut), Some(phrase)) = pair {
            let entry = (shortcut.to_owned(), phrase.to_owned());
            list.push(entry);
            if !valid(&list) {
                list.pop();
            }
        }
    }
    list
}

/// Saves the list.
pub fn write(file: &Path, list: &[(String, String)]) -> Result<(), String> {
    let items: Vec<Value> = list
        .iter()
        .map(|(shortcut, phrase)| json!({"shortcut": shortcut, "phrase": phrase}))
        .collect();
    super::display::write_json(file, &json!({ "replacements": items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_owned(), b.to_owned())
    }

    #[test]
    fn what_is_acceptable() {
        assert!(valid(&[pair("omw", "On my way!")]));
        assert!(valid(&[]));
        // Empty, with a space, repeated, too long.
        assert!(!valid(&[pair("", "x")]));
        assert!(!valid(&[pair("a b", "x")]));
        assert!(!valid(&[pair("a", "")]));
        assert!(!valid(&[pair("a", "x"), pair("a", "y")]));
        assert!(!valid(&[pair(&"a".repeat(41), "x")]));
        assert!(!valid(&[pair("a", &"x".repeat(1001))]));
        let many: Vec<_> = (0..=MAX_ENTRIES)
            .map(|i| pair(&format!("s{i}"), "x"))
            .collect();
        assert!(!valid(&many));
    }

    #[test]
    fn the_list_survives_a_restart_and_bad_entries_are_dropped() {
        let dir = std::env::temp_dir().join(format!("silica-repl-{}", std::process::id()));
        let file = dir.join("text-replacements.json");
        assert!(read(&file).is_empty());
        let list = vec![pair("omw", "On my way!"), pair("addr", "1 Main St\nTown")];
        write(&file, &list).unwrap();
        assert_eq!(read(&file), list);
        std::fs::write(
            &file,
            r#"{"replacements": [{"shortcut": "ok", "phrase": "fine"}, {"shortcut": "bad one", "phrase": "x"},
                {"shortcut": "ok", "phrase": "again"}, {"shortcut": 3}]}"#,
        )
        .unwrap();
        assert_eq!(read(&file), vec![pair("ok", "fine")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
