//! The xdg-desktop-portal backend of Silica (`org.freedesktop.impl.portal.FileChooser`): the
//! Open and Save dialogs of apps are Mustache windows in dialog mode
//! (`--dialog=open|save`), started from its .desktop entry; the choice comes back on stdout, one
//! URI per line (empty with a non-zero exit: cancelled). Specification: xdg-desktop-portal
//! documentation, interface `org.freedesktop.impl.portal.FileChooser`.

pub mod settings;

use std::collections::HashMap;
use std::path::Path;

/// The backend name on the session bus and the portal object.
pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.silica";
pub const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

/// The .desktop entry of Mustache.
pub const MUSTACHE_ID: &str = "dev.frascella.Mustache.desktop";

/// Portal responses: 0 done, 1 cancelled by the user, 2 other error.
pub const RESPONSE_OK: u32 = 0;
pub const RESPONSE_CANCELLED: u32 = 1;
pub const RESPONSE_OTHER: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Open,
    Save,
}

/// What the app asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogRequest {
    pub title: String,
    pub accept_label: Option<String>,
    pub multiple: bool,
    pub directory: bool,
    pub current_name: Option<String>,
    pub current_folder: Option<String>,
    /// The xdg-foreign v2 handle of the app's window ([`parent_handle`]): the dialog is
    /// attached to it.
    pub parent: Option<String>,
}

/// The Wayland handle in the portal's `parent_window` (`wayland:<handle>`, an xdg-foreign v2
/// export of the app's window); none for an X11 window (`x11:<xid>`) or no window (empty).
pub fn parent_handle(parent_window: &str) -> Option<String> {
    let handle = parent_window.strip_prefix("wayland:")?.trim();
    (!handle.is_empty()).then(|| handle.to_owned())
}

/// The words of an `Exec` line (Desktop Entry specification quoting), without the field
/// codes: the dialog files have nothing to do with them.
pub fn exec_words(exec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                '"' => quoted = false,
                _ => current.push(c),
            }
        } else if c == '"' {
            quoted = true;
            started = true;
        } else if c == ' ' || c == '\t' {
            if started || !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            started = false;
        } else {
            current.push(c);
            started = true;
        }
    }
    if started || !current.is_empty() {
        out.push(current);
    }
    out.into_iter()
        .filter(|w| !(w.len() == 2 && w.starts_with('%')))
        .collect()
}

/// The value of `Exec` in the `[Desktop Entry]` section.
pub fn desktop_exec(text: &str) -> Option<String> {
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_entry && let Some(value) = line.strip_prefix("Exec=") {
            return Some(value.trim().to_owned());
        }
    }
    None
}

/// Mustache's command line for the dialog: `Exec` plus the arguments for Dart. The dialog is
/// a helper process of the embedder (`--helper`): not the running Mustache, which a click in
/// the Dock must reach, and it ends with its window, so that a closed dialog still answers.
/// With the app's window (`--parent`) the dialog opens over it and gives it the focus back.
pub fn dialog_command(exec: &str, kind: DialogKind, request: &DialogRequest) -> Vec<String> {
    let mut words = exec_words(exec);
    words.push("--helper".to_owned());
    if let Some(parent) = &request.parent {
        words.push("--parent".to_owned());
        words.push(parent.clone());
    }
    let mut arg = |value: String| {
        words.push("--dart-arg".to_owned());
        words.push(value);
    };
    arg(match kind {
        DialogKind::Open => "--dialog=open".to_owned(),
        DialogKind::Save => "--dialog=save".to_owned(),
    });
    if !request.title.is_empty() {
        arg(format!("--title={}", request.title));
    }
    if let Some(label) = &request.accept_label {
        arg(format!("--accept={label}"));
    }
    if request.multiple {
        arg("--multiple".to_owned());
    }
    if request.directory {
        arg("--directory".to_owned());
    }
    if let Some(name) = &request.current_name {
        arg(format!("--name={name}"));
    }
    if let Some(folder) = &request.current_folder {
        arg(format!("--folder={folder}"));
    }
    words
}

/// The URIs chosen on Mustache's stdout: only `file://` lines.
pub fn parse_uris(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("file://"))
        .map(str::to_owned)
        .collect()
}

/// A path given as a zero-terminated byte array (the portal's `current_folder`).
pub fn path_from_bytes(bytes: &[u8]) -> Option<String> {
    let trimmed = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    let text = std::str::from_utf8(trimmed).ok()?;
    (!text.is_empty() && Path::new(text).is_absolute()).then(|| text.to_owned())
}

/// Mustache's entry in the XDG data directories (the first one wins).
pub fn find_mustache_exec(env: &HashMap<String, String>) -> Option<String> {
    let home = env.get("HOME").cloned().unwrap_or_default();
    let data_home = env
        .get("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("{home}/.local/share"));
    let data_dirs = env
        .get("XDG_DATA_DIRS")
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    std::iter::once(data_home)
        .chain(data_dirs.split(':').map(str::to_owned))
        .filter(|d| !d.is_empty())
        .find_map(|dir| {
            let text = std::fs::read_to_string(format!("{dir}/applications/{MUSTACHE_ID}")).ok()?;
            desktop_exec(&text)
        })
}

/// Marks a file an app saves through the dialog with the quarantine attribute (silica-os
/// `docs/app-bundles.md` § 6): the file is created empty when it does not exist yet, so that the
/// attribute is there before the app writes it (writing keeps it; a save that replaces the
/// file with a renamed temporary one drops it, a documented limit), and `app_id` is the agent.
pub fn mark_saved(path: &std::path::Path, app_id: &str) -> std::io::Result<()> {
    use silica_bundle::quarantine::{FROM_PORTAL, Quarantine, write};
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    write(path, &Quarantine::now(FROM_PORTAL, app_id, ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_files_are_marked() {
        let dir = std::env::temp_dir().join(format!("portal-mark-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let new = dir.join("download.AppImage");
        let _ = std::fs::remove_file(&new);
        match mark_saved(&new, "org.example.Browser") {
            Ok(()) => {
                assert_eq!(std::fs::read(&new).unwrap(), b"");
                let q = silica_bundle::quarantine::read(&new).unwrap();
                assert_eq!(q.agent, "org.example.Browser");
                assert_eq!(q.flags, silica_bundle::quarantine::FROM_PORTAL);
                // An existing file keeps its contents and gets the mark.
                std::fs::write(&new, "data").unwrap();
                silica_bundle::quarantine::clear(&new).unwrap();
                mark_saved(&new, "org.example.Other").unwrap();
                assert_eq!(std::fs::read(&new).unwrap(), b"data");
                assert_eq!(
                    silica_bundle::quarantine::read(&new).unwrap().agent,
                    "org.example.Other"
                );
            }
            // A file system without user attributes: the file is still created.
            Err(e) => assert_eq!(e.raw_os_error(), Some(95), "{e}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exec_words_respect_quotes_and_drop_field_codes() {
        assert_eq!(
            exec_words(r#"/opt/silica/embedder --title "The Mustache" /b/flutter_assets %F"#),
            [
                "/opt/silica/embedder",
                "--title",
                "The Mustache",
                "/b/flutter_assets"
            ]
        );
        assert_eq!(exec_words(r#"a "b \"c\"""#), ["a", r#"b "c""#]);
    }

    #[test]
    fn the_dialog_command_adds_dart_arguments() {
        let request = DialogRequest {
            title: "Choose".to_owned(),
            multiple: true,
            current_folder: Some("/home/io".to_owned()),
            ..DialogRequest::default()
        };
        let words = dialog_command("emb --app-id x /b", DialogKind::Open, &request);
        assert_eq!(
            words,
            [
                "emb",
                "--app-id",
                "x",
                "/b",
                "--helper",
                "--dart-arg",
                "--dialog=open",
                "--dart-arg",
                "--title=Choose",
                "--dart-arg",
                "--multiple",
                "--dart-arg",
                "--folder=/home/io",
            ]
        );
    }

    #[test]
    fn the_dialog_is_attached_to_the_wayland_parent_window() {
        assert_eq!(parent_handle("wayland:Zx81").as_deref(), Some("Zx81"));
        assert_eq!(parent_handle("x11:0x3a00004"), None);
        assert_eq!(parent_handle(""), None);
        assert_eq!(parent_handle("wayland:"), None);
        assert_eq!(parent_handle("Zx81"), None);
        let request = DialogRequest {
            parent: parent_handle("wayland:Zx81"),
            ..DialogRequest::default()
        };
        let words = dialog_command("emb /b", DialogKind::Save, &request);
        assert_eq!(
            words,
            [
                "emb",
                "/b",
                "--helper",
                "--parent",
                "Zx81",
                "--dart-arg",
                "--dialog=save",
            ]
        );
    }

    #[test]
    fn results_and_paths() {
        assert_eq!(
            parse_uris("file:///a.txt\nrumore\n file:///b%20c.txt \n"),
            ["file:///a.txt", "file:///b%20c.txt"]
        );
        assert_eq!(path_from_bytes(b"/home/io\0").as_deref(), Some("/home/io"));
        assert_eq!(path_from_bytes(b"relative\0"), None);
        assert_eq!(path_from_bytes(&[0xff, 0]), None);
        assert_eq!(
            desktop_exec("[Desktop Action x]\nExec=no\n[Desktop Entry]\nName=M\nExec=emb /b\n")
                .as_deref(),
            Some("emb /b")
        );
    }

    #[test]
    fn the_desktop_entry_is_found_in_the_data_dirs() {
        let dir = std::env::temp_dir().join(format!("silica-portal-{}", std::process::id()));
        let apps = dir.join("applications");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(
            apps.join(MUSTACHE_ID),
            "[Desktop Entry]\nType=Application\nExec=emb /b\n",
        )
        .unwrap();
        let env = HashMap::from([
            ("HOME".to_owned(), "/nowhere".to_owned()),
            ("XDG_DATA_DIRS".to_owned(), dir.display().to_string()),
        ]);
        assert_eq!(find_mustache_exec(&env).as_deref(), Some("emb /b"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
