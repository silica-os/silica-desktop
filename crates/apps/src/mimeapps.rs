//! The `mimeapps.list` files ("Association between MIME types and applications"): the default
//! application of a type, the added and the removed associations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const DEFAULTS: &str = "Default Applications";
const ADDED: &str = "Added Associations";
const REMOVED: &str = "Removed Associations";

/// The associations of several `mimeapps.list` files, merged in precedence order: the ids of
/// the most specific file come first. Ids carry `.desktop`, as in the files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MimeApps {
    pub defaults: BTreeMap<String, Vec<String>>,
    pub added: BTreeMap<String, Vec<String>>,
    pub removed: BTreeMap<String, BTreeSet<String>>,
}

impl MimeApps {
    /// Adds a file; files are added most specific first.
    pub fn add(&mut self, text: &str) {
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                name.clone_into(&mut section);
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let ids = value
                .split(';')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned);
            let key = key.trim().to_owned();
            match section.as_str() {
                DEFAULTS => self.defaults.entry(key).or_default().extend(ids),
                ADDED => self.added.entry(key).or_default().extend(ids),
                REMOVED => self.removed.entry(key).or_default().extend(ids),
                _ => {}
            }
        }
    }

    /// Reads the files that exist, most specific first.
    pub fn load(files: &[impl AsRef<Path>]) -> MimeApps {
        let mut apps = MimeApps::default();
        for file in files {
            if let Ok(text) = std::fs::read_to_string(file) {
                apps.add(&text);
            }
        }
        apps
    }

    /// Whether `id` is removed for `mime`.
    pub fn is_removed(&self, mime: &str, id: &str) -> bool {
        self.removed.get(mime).is_some_and(|ids| ids.contains(id))
    }
}

/// `text` (a `mimeapps.list`) with `[Default Applications]` saying `mime=id;`: the line is
/// replaced in place, or added to the section, or the section is added. Other lines are kept
/// as they are. A removed association of the same app for the type is dropped (choosing an app
/// undoes removing it).
pub fn with_default(text: &str, mime: &str, id: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut written = false;
    let mut in_defaults_end: Option<usize> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if section == DEFAULTS && !written {
                in_defaults_end = Some(out.len());
            }
            name.clone_into(&mut section);
            out.push(line.to_owned());
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=')
            && key.trim() == mime
        {
            if section == DEFAULTS {
                if !written {
                    out.push(format!("{mime}={id};"));
                    written = true;
                }
                continue;
            }
            if section == REMOVED {
                let rest: Vec<&str> = value
                    .split(';')
                    .map(str::trim)
                    .filter(|v| !v.is_empty() && *v != id)
                    .collect();
                if !rest.is_empty() {
                    out.push(format!("{mime}={};", rest.join(";")));
                }
                continue;
            }
        }
        out.push(line.to_owned());
    }
    if !written {
        if section == DEFAULTS {
            // The file ends inside the section: append there, before trailing blank lines.
            let mut at = out.len();
            while at > 0 && out.get(at - 1).is_some_and(|l| l.trim().is_empty()) {
                at -= 1;
            }
            out.insert(at, format!("{mime}={id};"));
        } else if let Some(at) = in_defaults_end {
            let mut at = at;
            while at > 0 && out.get(at - 1).is_some_and(|l| l.trim().is_empty()) {
                at -= 1;
            }
            out.insert(at, format!("{mime}={id};"));
        } else {
            if out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(format!("[{DEFAULTS}]"));
            out.push(format!("{mime}={id};"));
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_merge_most_specific_first() {
        let mut apps = MimeApps::default();
        apps.add("[Default Applications]\ntext/plain=user.desktop;\n[Removed Associations]\ntext/plain=gedit.desktop;\n");
        apps.add("[Default Applications]\ntext/plain=system.desktop;\n[Added Associations]\ntext/plain=added.desktop;other.desktop\n");
        assert_eq!(
            apps.defaults.get("text/plain").unwrap(),
            &["user.desktop", "system.desktop"]
        );
        assert_eq!(
            apps.added.get("text/plain").unwrap(),
            &["added.desktop", "other.desktop"]
        );
        assert!(apps.is_removed("text/plain", "gedit.desktop"));
        assert!(!apps.is_removed("text/plain", "user.desktop"));
    }

    #[test]
    fn setting_a_default_keeps_the_rest() {
        let text = "# mine\n[Added Associations]\ntext/plain=a.desktop;\n\n[Default Applications]\ntext/plain=old.desktop;\nimage/png=p.desktop;\n\n[Removed Associations]\ntext/plain=new.desktop;x.desktop;\n";
        let out = with_default(text, "text/plain", "new.desktop");
        assert!(out.contains("# mine\n"));
        assert!(out.contains("[Added Associations]\ntext/plain=a.desktop;\n"));
        assert!(
            out.contains("[Default Applications]\ntext/plain=new.desktop;\nimage/png=p.desktop;\n")
        );
        assert!(out.contains("[Removed Associations]\ntext/plain=x.desktop;\n"));
        let mut apps = MimeApps::default();
        apps.add(&out);
        assert_eq!(apps.defaults.get("text/plain").unwrap(), &["new.desktop"]);
        assert!(!apps.is_removed("text/plain", "new.desktop"));
    }

    #[test]
    fn a_missing_line_or_section_is_added() {
        let out = with_default(
            "[Default Applications]\nimage/png=p.desktop;\n\n[Added Associations]\nx/y=z.desktop;\n",
            "text/html",
            "web.desktop",
        );
        let mut apps = MimeApps::default();
        apps.add(&out);
        assert_eq!(apps.defaults.get("text/html").unwrap(), &["web.desktop"]);
        assert_eq!(apps.defaults.get("image/png").unwrap(), &["p.desktop"]);
        assert_eq!(apps.added.get("x/y").unwrap(), &["z.desktop"]);
        let fresh = with_default("", "text/html", "web.desktop");
        assert_eq!(fresh, "[Default Applications]\ntext/html=web.desktop;\n");
        let tail = with_default(
            "[Default Applications]\na/b=c.desktop;\n",
            "text/html",
            "w.desktop",
        );
        assert_eq!(
            tail,
            "[Default Applications]\na/b=c.desktop;\ntext/html=w.desktop;\n"
        );
    }
}
