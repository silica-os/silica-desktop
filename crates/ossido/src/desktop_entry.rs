//! Apps for `app.launch {app}`: the registry of `silica-apps` (docs/spec/launch-services.md),
//! read in process so that the compositor's thread never waits on the bus. The command is the
//! app's `Exec` with the field codes stripped, as the Dock and Faro run it (so `--app-id` and
//! the like are kept); bundles and AppImages start through `silica-launch`.

use silica_apps::registry::{Dirs, Registry};

/// The command that runs the app `id` (`org.mozilla.firefox`, with or without `.desktop`).
pub fn command_for(id: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    if !silica_apps::valid_id(id) {
        return Err(format!("not a desktop entry id: {id}"));
    }
    let registry = Registry::scan(Dirs::from_env(env));
    let app = registry
        .get(id)
        .ok_or_else(|| format!("no app with the id {id}"))?;
    let command = app.command();
    if command.is_empty() {
        return Err(format!("{id}: not a launchable application"));
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_for_searches_the_registry() {
        let root = std::env::temp_dir().join(format!("silica-entry-{}", std::process::id()));
        let apps = root.join("share/applications");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(
            apps.join("dev.x.Calc.desktop"),
            "[Desktop Entry]\nType=Application\nName=C\nExec=calc --app-id dev.x.Calc %U\n",
        )
        .unwrap();
        let share = root.join("share").to_string_lossy().into_owned();
        let env = move |k: &str| match k {
            "XDG_DATA_DIRS" => Some(share.clone()),
            "HOME" => Some("/nonexistent".to_owned()),
            _ => None,
        };
        assert_eq!(
            command_for("dev.x.Calc", &env).unwrap(),
            "calc --app-id dev.x.Calc"
        );
        assert_eq!(
            command_for("dev.x.Calc.desktop", &env).unwrap(),
            "calc --app-id dev.x.Calc"
        );
        assert!(command_for("missing", &env).is_err());
        assert!(command_for("../etc/passwd", &env).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
