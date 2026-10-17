//! `cargo xtask install-app <path>`: installs the `.desktop` entry of a Silica Flutter app
//! (one repository per app, next to this one) in `~/.local/share/applications`, so the
//! shell finds it with its icon: Dock, switcher, Faro, launcher.
//!
//! The app repository has a `linux/<id>.desktop.in` template with two placeholders:
//! `@EMBEDDER@` (the release embedder with the shell's engine and renderer) and `@BUNDLE@`
//! (its release bundle). If they are missing, the bundle and the embedder are built first.

use std::path::{Path, PathBuf};

use crate::{Result, cargo_command, flutter, root, status};

pub fn install_app(app: &str) -> Result<()> {
    let dir = std::path::absolute(app).map_err(|e| format!("{app}: {e}"))?;
    let templates: Vec<PathBuf> = std::fs::read_dir(dir.join("linux"))
        .map_err(|e| format!("{}/linux: {e}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.to_string_lossy().ends_with(".desktop.in"))
        .collect();
    if templates.is_empty() {
        return Err(format!("{}/linux: no .desktop.in template", dir.display()));
    }
    let bundle = dir.join("build/release/flutter_assets");
    if !dir.join("build/release/libapp.so").exists() {
        flutter::bundle_release(app)?;
    }
    let root = root()?;
    let embedder = root.join("target/release/silica-embedder");
    if !embedder.exists() {
        status(cargo_command(&[
            "build",
            "--release",
            "--package",
            "silica-embedder",
        ]))?;
    }
    let engine = flutter::release_engine_dir()?;
    if !engine.join("libflutter_engine.so").exists() {
        return Err(format!(
            "{} missing: cargo xtask engine --release (docs/engine-release.md)",
            engine.display()
        ));
    }
    let command = format!(
        "{} --engine {} --renderer vulkan",
        embedder.display(),
        engine.display()
    );
    // The app's own icon (`icon/` in its repository), copied next to the bundle.
    let icon = crate::app_icon::bundle_icon(&dir, &dir.join("build/release"))?.map(|folder| {
        let png = crate::app_icon::png_name(&folder);
        (folder.display().to_string(), png)
    });
    let target = applications_dir()?;
    std::fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    for template in templates {
        let text = std::fs::read_to_string(&template)
            .map_err(|e| format!("{}: {e}", template.display()))?;
        let entry = render_entry(
            &text,
            &command,
            &bundle.display().to_string(),
            icon.as_ref()
                .map(|(folder, png)| (folder.as_str(), png.as_str())),
        );
        let name = template
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".in"))
            .ok_or_else(|| format!("{}: invalid name", template.display()))?;
        let out = target.join(name);
        std::fs::write(&out, entry).map_err(|e| format!("{}: {e}", out.display()))?;
        eprintln!("xtask: installed {}", out.display());
    }
    Ok(())
}

/// A `.desktop.in` template with its placeholders filled. With `icon` (the installed icon
/// folder of the app and the PNG in it), the entry gets `X-Silica-Icon=<folder>` and
/// `Icon=<folder>/<png>`; without it the template keeps the icon names it has (the shell looks
/// them up in the icons folder).
pub(crate) fn render_entry(
    template: &str,
    embedder: &str,
    bundle: &str,
    icon: Option<(&str, &str)>,
) -> String {
    let entry = template
        .replace("@EMBEDDER@", embedder)
        .replace("@BUNDLE@", bundle);
    match icon {
        Some((folder, png)) => crate::app_icon::with_icon(&entry, folder, png),
        None => entry,
    }
}

/// The shell's own applications (`shell/linux/*.desktop.in`: Apps, Mission Control, Screenshot), their
/// `@ACTION@` replaced with `action` (the `silica-action` binary): `(file name, entry)`.
pub fn shell_entries(root: &Path, action: &str) -> Result<Vec<(String, String)>> {
    let dir = root.join("shell/linux");
    let mut entries = Vec::new();
    for item in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = item.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".in"))
            .filter(|n| n.ends_with(".desktop"))
        else {
            continue;
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        entries.push((name.to_owned(), text.replace("@ACTION@", action)));
    }
    entries.sort();
    Ok(entries)
}

/// Installs the shell's applications in `~/.local/share/applications` for sessions on this
/// machine (`xtask run --shell`).
pub fn install_shell_entries(root: &Path) -> Result<()> {
    let action = root.join("target/release/silica-action");
    let target = applications_dir()?;
    std::fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    for (name, entry) in shell_entries(root, &action.display().to_string())? {
        let out = target.join(&name);
        std::fs::write(&out, entry).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(())
}

/// `$XDG_DATA_HOME/applications`, or `~/.local/share/applications`.
fn applications_dir() -> Result<PathBuf> {
    Ok(data_home()?.join("applications"))
}

/// `$XDG_DATA_HOME`, or `~/.local/share`.
fn data_home() -> Result<PathBuf> {
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(data));
    }
    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    Ok(Path::new(&home).join(".local/share"))
}

/// `$XDG_CONFIG_HOME`, or `~/.config`.
fn config_home() -> Result<PathBuf> {
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(config));
    }
    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    Ok(Path::new(&home).join(".config"))
}

/// `cargo xtask install-portal`: Silica's portal backend for the user. The `.portal` file
/// tells xdg-desktop-portal there is a FileChooser backend (`UseIn=Silica`), the
/// `silica-portals.conf` configuration picks it when `XDG_CURRENT_DESKTOP=Silica`, the
/// D-Bus service starts `silica-portal` on the first request. The dialogs are Mustache's
/// (installed with `install-app`).
pub fn install_portal() -> Result<()> {
    let root = root()?;
    let binary = root.join("target/release/silica-portal");
    if !binary.exists() {
        status(cargo_command(&[
            "build",
            "--release",
            "--package",
            "silica-portal",
        ]))?;
    }
    let data = root.join("crates/portal/data");
    let read =
        |name: &str| std::fs::read_to_string(data.join(name)).map_err(|e| format!("{name}: {e}"));
    let files = [
        (
            data_home()?.join("xdg-desktop-portal/portals/silica.portal"),
            read("silica.portal")?,
        ),
        (
            config_home()?.join("xdg-desktop-portal/silica-portals.conf"),
            read("silica-portals.conf")?,
        ),
        (
            data_home()?.join("dbus-1/services/org.freedesktop.impl.portal.desktop.silica.service"),
            read("org.freedesktop.impl.portal.desktop.silica.service.in")?
                .replace("@PORTAL@", &binary.display().to_string()),
        ),
    ];
    for (path, text) in files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("xtask: installed {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn an_app_with_an_icon_folder_gets_its_path_in_the_entry() {
        let template = "[Desktop Entry]\nName=Clock\nIcon=clock\nX-Silica-Icon=clock\nExec=@EMBEDDER@ @BUNDLE@\n";
        let entry = render_entry(
            template,
            "/bin/emb --engine e",
            "/b/flutter_assets",
            Some(("/b/icon", "icon.png")),
        );
        assert!(
            entry.contains("Exec=/bin/emb --engine e /b/flutter_assets\n"),
            "{entry}"
        );
        assert!(entry.contains("X-Silica-Icon=/b/icon\n"), "{entry}");
        assert!(entry.contains("Icon=/b/icon/icon.png\n"), "{entry}");
        assert!(!entry.contains("=clock"), "{entry}");
        // Without an icon folder the template is untouched: the name is looked up as before.
        let plain = render_entry(template, "/bin/emb", "/b", None);
        assert!(plain.contains("X-Silica-Icon=clock\n"), "{plain}");
        assert!(plain.contains("Icon=clock\n"), "{plain}");
    }

    #[test]
    fn shell_entries_run_their_action() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let entries = shell_entries(root, "/opt/silica-action").unwrap();
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "dev.frascella.Apps.desktop",
                "dev.frascella.MissionControl.desktop",
                "dev.frascella.Screenshot.desktop"
            ]
        );
        let apps = &entries[0].1;
        assert!(
            apps.contains("Exec=/opt/silica-action launcher.toggle"),
            "{apps}"
        );
        assert!(apps.contains("X-Silica-Icon=apps"), "{apps}");
        let mission = &entries[1].1;
        assert!(
            mission.contains("Exec=/opt/silica-action panorama.toggle"),
            "{mission}"
        );
        assert!(
            mission.contains("X-Silica-Icon=mission-control"),
            "{mission}"
        );
        assert!(mission.contains("Categories=Utility;"), "{mission}");
        let shot = &entries[2].1;
        assert!(
            shot.contains("Exec=/opt/silica-action screenshot.toolbar"),
            "{shot}"
        );
        assert!(shot.contains("Categories=Utility;"), "{shot}");
        assert!(
            !apps.contains("@ACTION@")
                && !mission.contains("@ACTION@")
                && !shot.contains("@ACTION@")
        );
    }
}
