//! The share service: `dev.frascella.Silica.Share1` on `/dev/frascella/Silica/Share1`.
//!
//! An app's Share button asks for the targets of a list of files, shows them in its Share menu
//! and sends the chosen one back. The targets are:
//! - `copy`: the files on the clipboard (`text/uri-list`, through the clipboard service);
//! - every application whose desktop entry opts in with `X-Silica-Share=true` (Mail, for
//!   example). It is started with the files, following its `Exec` field codes.
//!
//! The menu itself (its look and order) belongs to the app's toolkit (silica_ui).

use silica_apps::registry::{Dirs, Registry};
use zbus::interface;

use crate::clipboard;

pub const SHARE_PATH: &str = "/dev/frascella/Silica/Share1";

/// The desktop entry key an application sets to appear among the share targets.
pub const SHARE_KEY: &str = "X-Silica-Share";

/// A target of the Share menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// `copy`, or the desktop entry id of an application (`dev.frascella.Mail`).
    pub id: String,
    /// The name to show (an application's `Name`; the toolkit translates `copy`).
    pub name: String,
    /// The application's `Icon` (an id or a path), empty for `copy`.
    pub icon: String,
}

/// An application that takes shared files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareApp {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub exec: String,
}

/// The applications that opt in, by name, from the session's app registry (the live one of the
/// `Apps1` service, else a scan of the same folders).
pub fn share_apps(env: &dyn Fn(&str) -> Option<String>) -> Vec<ShareApp> {
    let dirs = Dirs::from_env(env);
    let registry =
        crate::apps::live(&dirs).unwrap_or_else(|| std::sync::Arc::new(Registry::scan(dirs)));
    registry
        .share_targets()
        .into_iter()
        .map(|a| ShareApp {
            id: a.id.clone(),
            name: a.name.clone(),
            icon: a.icon.clone(),
            exec: a.exec.clone(),
        })
        .collect()
}

/// The local path of a `file://` URI (percent-decoded), or the argument itself.
pub(crate) fn local_path(uri: &str) -> String {
    let Some(rest) = uri.strip_prefix("file://") else {
        return uri.to_owned();
    };
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut out = Vec::with_capacity(path.len());
    let mut bytes = path.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let mut ahead = bytes.clone();
            let hex = [ahead.next(), ahead.next()];
            if let [Some(hi), Some(lo)] = hex
                && let (Some(hi), Some(lo)) = ((hi as char).to_digit(16), (lo as char).to_digit(16))
            {
                out.push((hi * 16 + lo) as u8);
                bytes = ahead;
                continue;
            }
        }
        out.push(byte);
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_owned())
}

/// The command lines that hand `uris` to `exec`, following the field codes: `%F`/`%U` take
/// every file in one run, `%f`/`%u` one run per file, and an Exec without them gets the files
/// at the end. `%%` is a `%`; the other codes are dropped.
pub fn command_lines(exec: &str, uris: &[String]) -> Vec<Vec<String>> {
    silica_apps::exec::launch_lines(
        exec,
        &silica_apps::exec::Context {
            targets: uris,
            ..silica_apps::exec::Context::default()
        },
    )
}

/// The Share menu's targets: `copy`, then the applications.
pub fn targets_for(apps: Vec<ShareApp>) -> Vec<Target> {
    let mut targets = vec![Target {
        id: "copy".to_owned(),
        name: "Copy".to_owned(),
        icon: String::new(),
    }];
    targets.extend(apps.into_iter().map(|a| Target {
        id: a.id,
        name: a.name,
        icon: a.icon,
    }));
    targets
}

pub struct Share;

#[interface(name = "dev.frascella.Silica.Share1")]
impl Share {
    /// The targets for `uris` as `a(sss)` (id, name, icon), `copy` first. Empty when there is
    /// nothing to share.
    fn targets(&self, uris: Vec<String>) -> Vec<(String, String, String)> {
        if uris.is_empty() {
            return Vec::new();
        }
        let env = |k: &str| std::env::var(k).ok();
        targets_for(share_apps(&env))
            .into_iter()
            .map(|t| (t.id, t.name, t.icon))
            .collect()
    }

    /// Shares `uris` with `target` (an id from `Targets`). False if the target is unknown or
    /// could not start.
    fn share(&self, target: String, uris: Vec<String>) -> bool {
        if uris.is_empty() {
            return false;
        }
        if target == "copy" {
            return clipboard::copy_files(&uris);
        }
        let env = |k: &str| std::env::var(k).ok();
        let Some(app) = share_apps(&env).into_iter().find(|a| a.id == target) else {
            return false;
        };
        let mut started = false;
        for line in command_lines(&app.exec, &uris) {
            let Some((program, args)) = line.split_first() else {
                continue;
            };
            match std::process::Command::new(program).args(args).spawn() {
                Ok(child) => {
                    started = true;
                    // Reap it in the background so it does not stay a zombie.
                    let _ = std::thread::Builder::new()
                        .name("share-child".to_owned())
                        .spawn(move || {
                            let mut child = child;
                            let _ = child.wait();
                        });
                }
                Err(err) => eprintln!("silica-daemon: share with {}: {err}", app.id),
            }
        }
        started
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const MAIL: &str = "[Desktop Entry]\nType=Application\nName=Mail\nIcon=dev.frascella.Mail\nExec=mail --compose %F\nX-Silica-Share=true\n";

    #[test]
    fn copy_comes_first_then_the_apps() {
        let app = ShareApp {
            id: "dev.frascella.Mail".to_owned(),
            name: "Mail".to_owned(),
            icon: "dev.frascella.Mail".to_owned(),
            exec: "mail --compose %F".to_owned(),
        };
        let targets = targets_for(vec![app]);
        let ids: Vec<&str> = targets.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["copy", "dev.frascella.Mail"]);
        assert_eq!(targets[1].icon, "dev.frascella.Mail");
    }

    #[test]
    fn field_codes_decide_how_files_are_passed() {
        let uris = vec![
            "file:///home/a/My%20File.png".to_owned(),
            "file:///home/a/b.txt".to_owned(),
        ];
        assert_eq!(
            command_lines("mail --compose %F", &uris),
            vec![vec![
                "mail",
                "--compose",
                "/home/a/My File.png",
                "/home/a/b.txt"
            ]]
        );
        assert_eq!(
            command_lines("viewer %u", &uris),
            vec![
                vec!["viewer", "file:///home/a/My%20File.png"],
                vec!["viewer", "file:///home/a/b.txt"],
            ]
        );
        assert_eq!(
            command_lines("\"/opt/my app/run\" --x", &uris[1..]),
            vec![vec!["/opt/my app/run", "--x", "/home/a/b.txt"]]
        );
        assert_eq!(
            command_lines("tool %i %c 100%% %F", &uris[1..]),
            vec![vec!["tool", "100%", "/home/a/b.txt"]]
        );
    }

    #[test]
    fn earlier_folders_win_and_apps_are_sorted() {
        let root = std::env::temp_dir().join(format!("share-apps-{}", std::process::id()));
        let (home, sys) = (root.join("home"), root.join("sys"));
        for dir in [&home, &sys] {
            std::fs::create_dir_all(dir.join("applications")).unwrap();
        }
        std::fs::write(home.join("applications/dev.frascella.Mail.desktop"), MAIL).unwrap();
        std::fs::write(
            sys.join("applications/dev.frascella.Mail.desktop"),
            MAIL.replace("Name=Mail", "Name=Old"),
        )
        .unwrap();
        std::fs::write(
            sys.join("applications/a.desktop"),
            MAIL.replace("Name=Mail", "Name=Archive"),
        )
        .unwrap();
        // Hidden ones and those that do not opt in are not targets.
        std::fs::write(
            sys.join("applications/b.desktop"),
            MAIL.replace("X-Silica-Share=true\n", ""),
        )
        .unwrap();
        std::fs::write(
            sys.join("applications/c.desktop"),
            format!("{MAIL}NoDisplay=true\n"),
        )
        .unwrap();
        let (h, s) = (home.display().to_string(), sys.display().to_string());
        let env = move |k: &str| match k {
            "XDG_DATA_HOME" => Some(h.clone()),
            "XDG_DATA_DIRS" => Some(s.clone()),
            "HOME" => Some("/nonexistent".to_owned()),
            _ => None,
        };
        let names: Vec<String> = share_apps(&env).into_iter().map(|a| a.name).collect();
        assert_eq!(names, vec!["Archive", "Mail"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
