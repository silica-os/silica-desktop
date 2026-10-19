//! Desktop entries (the freedesktop Desktop Entry specification): the `[Desktop Entry]` group
//! and its `[Desktop Action …]` groups, with the localized keys (`Name[it]`).

use std::collections::BTreeMap;

/// An action of an entry (`Actions=` and its `[Desktop Action <id>]` group).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    pub name: String,
    /// `Name[lang]` by language.
    pub names: BTreeMap<String, String>,
    /// The command line with its field codes.
    pub exec: String,
    pub icon: Option<String>,
}

/// The keys of a desktop entry Silica uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    /// `Type=`.
    pub kind: Option<String>,
    pub name: Option<String>,
    /// `Name[lang]` by language (`it`, `pt_BR`).
    pub names: BTreeMap<String, String>,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    /// `Exec=` as written, with its field codes.
    pub exec: Option<String>,
    pub try_exec: Option<String>,
    pub icon: Option<String>,
    /// `X-Silica-Icon=`: an absolute silica-icons folder or a `silica.assets` name.
    pub silica_icon: Option<String>,
    /// `X-Silica-Document-Icons=`: (MIME type or `major/*`, document icon name).
    pub document_icons: Vec<(String, String)>,
    pub wm_class: Option<String>,
    pub keywords: Vec<String>,
    /// `Keywords[lang]` by language.
    pub keywords_l10n: BTreeMap<String, Vec<String>>,
    pub categories: Vec<String>,
    pub mime_types: Vec<String>,
    pub actions: Vec<Action>,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
    /// `X-Silica-Share=true`: a target of the Share menu.
    pub share: bool,
    /// `X-Flatpak=<app id>`: exported by Flatpak.
    pub flatpak: Option<String>,
    /// `X-Silica-Bundle=<path>`: the compatibility entry of a bundle.
    pub bundle: Option<String>,
    /// `OnlyShowIn=` / `NotShowIn=`.
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
}

impl DesktopEntry {
    /// Whether this is an application that may be listed or launched (`Type=Application`, not
    /// `Hidden`, has an `Exec`).
    pub fn is_application(&self) -> bool {
        self.kind.as_deref() == Some("Application")
            && !self.hidden
            && self.exec.as_deref().is_some_and(|e| !e.trim().is_empty())
    }

    /// Whether the entry is meant for the desktop `desktop` (`OnlyShowIn`, `NotShowIn`).
    pub fn shown_in(&self, desktop: &str) -> bool {
        if self.not_show_in.iter().any(|d| d == desktop) {
            return false;
        }
        self.only_show_in.is_empty() || self.only_show_in.iter().any(|d| d == desktop)
    }

    /// The name in `lang` (`it_IT` falls back to `it`), else the plain name.
    pub fn name_in(&self, lang: &str) -> Option<&str> {
        localized(&self.names, lang)
            .map(String::as_str)
            .or(self.name.as_deref())
    }
}

/// The value for `lang` in a localized map: `ll_CC`, then `ll`.
pub fn localized<'a, V>(map: &'a BTreeMap<String, V>, lang: &str) -> Option<&'a V> {
    let lang = lang.split(['.', '@']).next().unwrap_or(lang);
    map.get(lang)
        .or_else(|| map.get(lang.split('_').next().unwrap_or(lang)))
}

/// A list value (`a;b;c;`), with `\;` kept as a `;` inside an item.
pub fn list(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut item = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(';') => item.push(';'),
                Some(other) => {
                    item.push('\\');
                    item.push(other);
                }
                None => item.push('\\'),
            },
            ';' => {
                let trimmed = item.trim();
                if !trimmed.is_empty() {
                    items.push(trimmed.to_owned());
                }
                item.clear();
            }
            c => item.push(c),
        }
    }
    let trimmed = item.trim();
    if !trimmed.is_empty() {
        items.push(trimmed.to_owned());
    }
    items
}

/// Unescapes a string value (`\s`, `\n`, `\t`, `\r`, `\\`).
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The names of a document icon: letters, digits, `.`, `_`, `-`, not starting with a dot.
pub fn valid_document_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Parses `mime/type=name;major/*=name;`. Invalid items are dropped.
pub fn document_icons(value: &str) -> Vec<(String, String)> {
    value
        .split(';')
        .filter_map(|item| {
            let (mime, name) = item.split_once('=')?;
            let (mime, name) = (mime.trim(), name.trim());
            (mime.contains('/') && valid_document_name(name))
                .then(|| (mime.to_owned(), name.to_owned()))
        })
        .collect()
}

/// Splits `Key[lang]` into the key and the language.
fn split_key(key: &str) -> (&str, Option<&str>) {
    match key.split_once('[') {
        Some((base, rest)) => match rest.strip_suffix(']') {
            Some(lang) if !lang.is_empty() => (base, Some(lang)),
            _ => (key, None),
        },
        None => (key, None),
    }
}

/// Parses a desktop entry. The first value of a key wins (a repeated key is an error in the
/// specification; this is what GLib does too).
pub fn parse(text: &str) -> DesktopEntry {
    let mut entry = DesktopEntry::default();
    // The action groups, by id, in file order.
    let mut groups: BTreeMap<String, Action> = BTreeMap::new();
    let mut action_ids: Vec<String> = Vec::new();
    enum Group {
        None,
        Entry,
        Action(String),
    }
    let mut group = Group::None;
    let mut seen_entry = false;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            group = if name == "Desktop Entry" && !seen_entry {
                seen_entry = true;
                Group::Entry
            } else if let Some(id) = name.strip_prefix("Desktop Action ") {
                Group::Action(id.to_owned())
            } else {
                Group::None
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match &group {
            Group::None => {}
            Group::Action(id) => {
                let action = groups.entry(id.clone()).or_insert_with(|| Action {
                    id: id.clone(),
                    ..Action::default()
                });
                match split_key(key) {
                    ("Name", None) if action.name.is_empty() => action.name = unescape(value),
                    ("Name", Some(lang)) => {
                        action
                            .names
                            .entry(lang.to_owned())
                            .or_insert_with(|| unescape(value));
                    }
                    ("Exec", None) if action.exec.is_empty() => action.exec = value.to_owned(),
                    ("Icon", None) if action.icon.is_none() => {
                        action.icon = Some(unescape(value));
                    }
                    _ => {}
                }
            }
            Group::Entry => {
                if !seen.insert(key.to_owned()) {
                    continue;
                }
                match split_key(key) {
                    ("Type", None) => entry.kind = Some(value.to_owned()),
                    ("Name", None) => entry.name = Some(unescape(value)),
                    ("Name", Some(lang)) => {
                        entry.names.insert(lang.to_owned(), unescape(value));
                    }
                    ("GenericName", None) => entry.generic_name = Some(unescape(value)),
                    ("Comment", None) => entry.comment = Some(unescape(value)),
                    ("Exec", None) => entry.exec = Some(value.to_owned()),
                    ("TryExec", None) => entry.try_exec = Some(unescape(value)),
                    ("Icon", None) => entry.icon = Some(unescape(value)),
                    ("X-Silica-Icon", None) => entry.silica_icon = Some(unescape(value)),
                    ("X-Silica-Document-Icons", None) => {
                        entry.document_icons = document_icons(value);
                    }
                    ("StartupWMClass", None) => entry.wm_class = Some(unescape(value)),
                    ("Keywords", None) => entry.keywords = list(value),
                    ("Keywords", Some(lang)) => {
                        entry.keywords_l10n.insert(lang.to_owned(), list(value));
                    }
                    ("Categories", None) => entry.categories = list(value),
                    ("MimeType", None) => entry.mime_types = list(value),
                    ("Actions", None) => action_ids = list(value),
                    ("NoDisplay", None) => entry.no_display = value == "true",
                    ("Hidden", None) => entry.hidden = value == "true",
                    ("Terminal", None) => entry.terminal = value == "true",
                    ("X-Silica-Share", None) => entry.share = value == "true",
                    ("X-Flatpak", None) => {
                        entry.flatpak = Some(value.to_owned()).filter(|v| !v.is_empty());
                    }
                    ("X-Silica-Bundle", None) => {
                        entry.bundle = Some(unescape(value)).filter(|v| !v.is_empty());
                    }
                    ("OnlyShowIn", None) => entry.only_show_in = list(value),
                    ("NotShowIn", None) => entry.not_show_in = list(value),
                    _ => {}
                }
            }
        }
    }
    entry.actions = action_ids
        .iter()
        .filter_map(|id| groups.remove(id))
        .filter(|a| !a.name.is_empty() && !a.exec.is_empty())
        .collect();
    entry
}

/// Writes a desktop entry for an app (the compatibility entries of bundles). Values are escaped
/// so that a newline in a name cannot add a key.
pub fn render(entry: &DesktopEntry) -> String {
    fn esc(value: &str) -> String {
        value
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }
    fn esc_list(items: &[String]) -> String {
        items
            .iter()
            .map(|i| format!("{};", esc(i).replace(';', "\\;")))
            .collect()
    }
    let mut out = String::from("[Desktop Entry]\nType=Application\n");
    let mut put = |key: &str, value: &str| {
        if !value.is_empty() {
            out.push_str(&format!("{key}={value}\n"));
        }
    };
    put("Name", &esc(entry.name.as_deref().unwrap_or_default()));
    for (lang, name) in &entry.names {
        put(&format!("Name[{lang}]"), &esc(name));
    }
    put(
        "GenericName",
        &esc(entry.generic_name.as_deref().unwrap_or_default()),
    );
    put(
        "Comment",
        &esc(entry.comment.as_deref().unwrap_or_default()),
    );
    put("Exec", entry.exec.as_deref().unwrap_or_default());
    put("Icon", &esc(entry.icon.as_deref().unwrap_or_default()));
    put(
        "X-Silica-Icon",
        &esc(entry.silica_icon.as_deref().unwrap_or_default()),
    );
    put(
        "StartupWMClass",
        &esc(entry.wm_class.as_deref().unwrap_or_default()),
    );
    put("Categories", &esc_list(&entry.categories));
    put("Keywords", &esc_list(&entry.keywords));
    put("MimeType", &esc_list(&entry.mime_types));
    if entry.no_display {
        put("NoDisplay", "true");
    }
    if entry.share {
        put("X-Silica-Share", "true");
    }
    put(
        "X-Silica-Bundle",
        &esc(entry.bundle.as_deref().unwrap_or_default()),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "# comment\n\
        [Desktop Entry]\n\
        Type=Application\n\
        Name=Calculator\n\
        Name[it]=Calcolatrice\n\
        Name=Ignored\n\
        GenericName=Calc\n\
        Exec=silica-app --app-id dev.x.Calc %U\n\
        Icon=accessories-calculator\n\
        X-Silica-Icon=/apps/calc/icon\n\
        X-Silica-Document-Icons=application/pdf=pdf;bad=x;\n\
        StartupWMClass=dev.x.Calc\n\
        Keywords=math;sum\\;mation;\n\
        Keywords[it]=conti;\n\
        Categories=Utility;Calculator;\n\
        MimeType=text/x-calc;\n\
        Actions=new;broken;\n\
        X-Silica-Share=true\n\
        OnlyShowIn=Silica;\n\
        \n\
        [Desktop Action new]\n\
        Name=New Window\n\
        Name[it]=Nuova finestra\n\
        Exec=calc --new\n\
        [Desktop Action broken]\n\
        Name=No exec\n\
        [Other Group]\n\
        Name=Other\n";

    #[test]
    fn parses_the_main_group_and_actions() {
        let e = parse(ENTRY);
        assert!(e.is_application());
        assert_eq!(e.name.as_deref(), Some("Calculator"));
        assert_eq!(e.name_in("it_IT.UTF-8"), Some("Calcolatrice"));
        assert_eq!(e.name_in("fr"), Some("Calculator"));
        assert_eq!(e.exec.as_deref(), Some("silica-app --app-id dev.x.Calc %U"));
        assert_eq!(e.silica_icon.as_deref(), Some("/apps/calc/icon"));
        assert_eq!(
            e.document_icons,
            [("application/pdf".to_owned(), "pdf".to_owned())]
        );
        assert_eq!(e.keywords, ["math", "sum;mation"]);
        assert_eq!(e.keywords_l10n.get("it").map(Vec::len), Some(1));
        assert_eq!(e.categories, ["Utility", "Calculator"]);
        assert!(e.share);
        assert!(e.shown_in("Silica"));
        assert!(!e.shown_in("GNOME"));
        assert_eq!(e.actions.len(), 1);
        assert_eq!(e.actions[0].id, "new");
        assert_eq!(e.actions[0].names.get("it").unwrap(), "Nuova finestra");
    }

    #[test]
    fn hidden_and_non_applications_are_not_applications() {
        assert!(
            !parse("[Desktop Entry]\nType=Application\nHidden=true\nExec=a\n").is_application()
        );
        assert!(!parse("[Desktop Entry]\nType=Link\nExec=a\n").is_application());
        assert!(!parse("[Desktop Entry]\nType=Application\nExec=\n").is_application());
        let nd = parse("[Desktop Entry]\nType=Application\nNoDisplay=true\nExec=a\n");
        assert!(nd.is_application() && nd.no_display);
    }

    #[test]
    fn rendering_round_trips_and_escapes() {
        let entry = DesktopEntry {
            name: Some("Evil\nExec=rm -rf /".to_owned()),
            exec: Some("silica-launch '/Applications/Hello.app' %U".to_owned()),
            categories: vec!["Utility".to_owned()],
            mime_types: vec!["text/x-a;b".to_owned()],
            bundle: Some("/Applications/Hello.app".to_owned()),
            ..DesktopEntry::default()
        };
        let text = render(&entry);
        let back = parse(&text);
        assert_eq!(back.exec, entry.exec);
        assert_eq!(back.name.as_deref(), Some("Evil\nExec=rm -rf /"));
        assert_eq!(back.mime_types, ["text/x-a;b"]);
        assert_eq!(back.bundle.as_deref(), Some("/Applications/Hello.app"));
        assert_eq!(text.matches("\nExec=").count(), 1);
    }
}
