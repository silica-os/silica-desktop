//! The icon of a Silica app that lives in the app's own repository (docs/assets-format.md,
//! "App icons"): `icon/` is the silica-icons app icon folder as it is. `bundle` copies it next
//! to `flutter_assets`, `install-app` and `os deploy` write the desktop entry that points at it.

use std::path::{Path, PathBuf};

use crate::Result;

/// Name of the icon folder, in the app repository and in the bundle.
pub const ICON_DIR: &str = "icon";

/// The `icon/` folder of the app repository `app`, when it has one (an `icon.json` inside).
pub fn source(app: &Path) -> Option<PathBuf> {
    let dir = app.join(ICON_DIR);
    dir.join("icon.json").is_file().then_some(dir)
}

/// Copies `<app>/icon/` to `<bundle>/icon/` (replacing an older copy). Nothing happens, and
/// `None` comes back, for an app without `icon/`: it keeps its icon name looked up in the
/// icons folder.
pub fn copy_to_bundle(app: &Path, bundle: &Path) -> Result<Option<PathBuf>> {
    let Some(from) = source(app) else {
        return Ok(None);
    };
    let to = bundle.join(ICON_DIR);
    if to.exists() {
        std::fs::remove_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    copy_dir(&from, &to)?;
    eprintln!("xtask: icon in {}", to.display());
    Ok(Some(to))
}

/// Name of the compiled icon in `icon/` (docs/assets-format.md, "App icon file").
pub const COMPILED: &str = "icon.assets";

/// [`copy_to_bundle`], then the compiled `icon/icon.assets` next to the copied sources (the
/// thumbnail service composes light, dark, clear and tinted from it). The fallback PNGs and
/// `icon.json` stay. A compile error is a warning: the app keeps its flat icon.
pub fn bundle_icon(app: &Path, bundle: &Path) -> Result<Option<PathBuf>> {
    let Some(copied) = copy_to_bundle(app, bundle)? else {
        return Ok(None);
    };
    if let Err(e) = compile(&copied, &copied.join(COMPILED)) {
        eprintln!("xtask: warning: {}: no {COMPILED}: {e}", copied.display());
    }
    Ok(Some(copied))
}

/// Compiles the icon folder `from` into `to` with `silica-assets-compile --app-icon`.
pub fn compile(from: &Path, to: &Path) -> Result<()> {
    let mut command = crate::cargo_command(&[
        "run",
        "--release",
        "--locked",
        "--quiet",
        "--package",
        "silica-assets",
        "--bin",
        "silica-assets-compile",
        "--",
        "--app-icon",
    ]);
    command.arg(from).arg("--out").arg(to);
    crate::status(command)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for item in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let path = item.map_err(|e| format!("{}: {e}", from.display()))?.path();
        let Some(name) = path.file_name() else {
            continue;
        };
        let target = to.join(name);
        // `metadata` follows links: a linked file is copied, a linked folder is walked.
        let meta = std::fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)
                .map_err(|e| format!("{} → {}: {e}", path.display(), target.display()))?;
        }
    }
    Ok(())
}

/// The PNG for `Icon=`: the `rendered.default` file of `icon.json` (1024 px), else `icon.png`.
pub fn png_name(icon_dir: &Path) -> String {
    std::fs::read_to_string(icon_dir.join("icon.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| {
            json.get("rendered")?
                .get("default")?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|name| !name.contains('/') && !name.contains(".."))
        .unwrap_or_else(|| "icon.png".to_owned())
}

/// Points a desktop entry at an app icon folder: `X-Silica-Icon=<icon_dir>` (the shell draws
/// the folder's rendered icon) and `Icon=<icon_dir>/<png>` (any other launcher). Both keys
/// are replaced in the `[Desktop Entry]` group (or added to it); the other groups are left
/// as they are. The `@ICON@` and `@ICON_PNG@` placeholders are replaced too.
pub fn with_icon(entry: &str, icon_dir: &str, png: &str) -> String {
    let png_path = format!("{icon_dir}/{png}");
    let entry = entry
        .replace("@ICON_PNG@", &png_path)
        .replace("@ICON@", icon_dir);
    let mut out = Vec::new();
    let mut in_main = false;
    let mut inserted = false;
    for line in entry.lines() {
        if line.starts_with('[') {
            if in_main && !inserted {
                out.push(format!("X-Silica-Icon={icon_dir}"));
                out.push(format!("Icon={png_path}"));
                inserted = true;
            }
            in_main = line.trim() == "[Desktop Entry]";
        } else if in_main && (line.starts_with("X-Silica-Icon=") || line.starts_with("Icon=")) {
            if !inserted {
                out.push(format!("X-Silica-Icon={icon_dir}"));
                out.push(format!("Icon={png_path}"));
                inserted = true;
            }
            continue;
        }
        out.push(line.to_owned());
    }
    if in_main && !inserted {
        out.push(format!("X-Silica-Icon={icon_dir}"));
        out.push(format!("Icon={png_path}"));
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            PathBuf::from("/dev/shm").join(format!("xtask-app-icon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn app_with_icon(dir: &Path) {
        let icon = dir.join("icon");
        std::fs::create_dir_all(&icon).unwrap();
        std::fs::write(
            icon.join("icon.json"),
            r#"{"id":"clock","rendered":{"default":"icon.png","dark":"icon-dark.png"}}"#,
        )
        .unwrap();
        std::fs::write(icon.join("icon.png"), b"png").unwrap();
        std::fs::write(icon.join("icon-dark.png"), b"dark").unwrap();
        std::fs::write(icon.join("background.svg"), b"<svg/>").unwrap();
    }

    #[test]
    fn the_bundle_gets_a_copy_of_the_icon_folder() {
        let dir = scratch("copy");
        let app = dir.join("app");
        let bundle = app.join("build/release");
        app_with_icon(&app);
        std::fs::create_dir_all(bundle.join("icon")).unwrap();
        std::fs::write(bundle.join("icon/stale.png"), b"old").unwrap();
        let copied = copy_to_bundle(&app, &bundle).unwrap().unwrap();
        assert_eq!(copied, bundle.join("icon"));
        for file in ["icon.json", "icon.png", "icon-dark.png", "background.svg"] {
            assert!(copied.join(file).is_file(), "{file}");
        }
        assert!(!copied.join("stale.png").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_app_without_an_icon_folder_copies_nothing() {
        let dir = scratch("none");
        std::fs::create_dir_all(dir.join("app")).unwrap();
        assert!(
            copy_to_bundle(&dir.join("app"), &dir.join("out"))
                .unwrap()
                .is_none()
        );
        assert!(!dir.join("out").exists());
        // A folder without a manifest is not an icon either.
        std::fs::create_dir_all(dir.join("app/icon")).unwrap();
        assert!(source(&dir.join("app")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_png_comes_from_the_manifest() {
        let dir = scratch("png");
        app_with_icon(&dir);
        assert_eq!(png_name(&dir.join("icon")), "icon.png");
        std::fs::write(
            dir.join("icon/icon.json"),
            r#"{"rendered":{"default":"../etc/passwd"}}"#,
        )
        .unwrap();
        assert_eq!(png_name(&dir.join("icon")), "icon.png");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_desktop_entry_points_at_the_installed_icon() {
        let template = "[Desktop Entry]\nType=Application\nName=Clock\nIcon=clock\n\
                        X-Silica-Icon=clock\nExec=@EMBEDDER@ @BUNDLE@\n\n\
                        [Desktop Action new]\nName=New\nIcon=list-add\n";
        let text = with_icon(template, "/usr/lib64/silica/apps/Clock/icon", "icon.png");
        assert!(
            text.contains("X-Silica-Icon=/usr/lib64/silica/apps/Clock/icon\n"),
            "{text}"
        );
        assert!(
            text.contains("Icon=/usr/lib64/silica/apps/Clock/icon/icon.png\n"),
            "{text}"
        );
        assert!(!text.contains("Icon=clock"), "{text}");
        // The action group keeps its own icon.
        assert!(text.contains("Icon=list-add"), "{text}");
        assert_eq!(text.matches("X-Silica-Icon=").count(), 1);
        // Keys that were missing are added to the main group.
        let bare = with_icon("[Desktop Entry]\nName=A\n", "/i", "icon.png");
        assert_eq!(
            bare,
            "[Desktop Entry]\nName=A\nX-Silica-Icon=/i\nIcon=/i/icon.png\n"
        );
        // Placeholders work too.
        let placeholders = with_icon(
            "[Desktop Entry]\nIcon=@ICON_PNG@\nX-Silica-Icon=@ICON@\n",
            "/i",
            "p.png",
        );
        assert!(placeholders.contains("Icon=/i/p.png\n"), "{placeholders}");
    }
}
