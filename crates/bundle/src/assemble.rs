//! Putting a bundle together from its parts (`silica-bundle build`).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use silica_apps::info::{AppKind, Info};

/// The parts of a bundle.
#[derive(Clone, Debug, Default)]
pub struct Parts {
    /// `Info.toml`, or a template with `@NAME@` placeholders (see `substitutions`).
    pub info: PathBuf,
    /// Placeholder values for the template (`VERSION` → `@VERSION@`).
    pub substitutions: Vec<(String, String)>,
    /// A Flutter release bundle (`flutter_assets/`, `libapp.so`, and maybe `icon/`): becomes
    /// `Contents/<exec>` (`app/` by default); its `icon/` becomes the bundle's icon.
    pub flutter: Option<PathBuf>,
    /// Executables for `Contents/bin/`.
    pub bins: Vec<PathBuf>,
    /// A folder copied as `Contents/lib/`.
    pub lib: Option<PathBuf>,
    /// A silica-icons app icon folder for `Contents/Resources/icon/`.
    pub icon: Option<PathBuf>,
    /// Document icons for `Contents/Resources/documents/`.
    pub documents: Option<PathBuf>,
    /// Translations for `Contents/Resources/l10n/`.
    pub l10n: Option<PathBuf>,
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut items: Vec<_> = fs::read_dir(from)
        .map_err(|e| format!("{}: {e}", from.display()))?
        .flatten()
        .collect();
    items.sort_by_key(|e| e.file_name());
    for item in items {
        let path = item.path();
        let target = to.join(item.file_name());
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.file_type().is_symlink() {
            let link = fs::read_link(&path).map_err(|e| e.to_string())?;
            std::os::unix::fs::symlink(link, &target).map_err(|e| e.to_string())?;
        } else if meta.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target)
                .map_err(|e| format!("{} → {}: {e}", path.display(), target.display()))?;
        }
    }
    Ok(())
}

/// Fills the `@NAME@` placeholders of a template.
pub fn substitute(template: &str, values: &[(String, String)]) -> String {
    let mut text = template.to_owned();
    for (name, value) in values {
        text = text.replace(&format!("@{name}@"), value);
    }
    text
}

/// Builds `<out_dir>/<name>.app` (replacing an older one) and returns its path.
pub fn assemble(parts: &Parts, out_dir: &Path) -> Result<PathBuf, String> {
    let template =
        fs::read_to_string(&parts.info).map_err(|e| format!("{}: {e}", parts.info.display()))?;
    let text = substitute(&template, &parts.substitutions);
    if let Some(at) = text.find('@')
        && text
            .get(at + 1..)
            .and_then(|rest| rest.split_once('@'))
            .is_some_and(|(name, _)| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
            })
    {
        return Err("Info.toml still has an @PLACEHOLDER@: pass it with --set".to_owned());
    }
    let info = Info::parse(&text).map_err(|e| e.to_string())?;
    let bundle = out_dir.join(format!("{}.app", info.name));
    let staging = out_dir.join(format!(".{}.app.building", info.name));
    let _ = fs::remove_dir_all(&staging);
    let contents = staging.join("Contents");
    let result = (|| {
        fs::create_dir_all(&contents).map_err(|e| e.to_string())?;
        fs::write(contents.join("Info.toml"), &text).map_err(|e| e.to_string())?;
        let mut icon = parts.icon.clone();
        if let Some(flutter) = &parts.flutter {
            if info.kind != AppKind::Flutter {
                return Err("--flutter is for kind = \"flutter\" apps".to_owned());
            }
            let app = contents.join(&info.exec);
            fs::create_dir_all(&app).map_err(|e| e.to_string())?;
            copy_dir(&flutter.join("flutter_assets"), &app.join("flutter_assets"))?;
            fs::copy(flutter.join("libapp.so"), app.join("libapp.so"))
                .map_err(|e| format!("{}/libapp.so: {e}", flutter.display()))?;
            if icon.is_none() && flutter.join("icon").is_dir() {
                icon = Some(flutter.join("icon"));
            }
        } else if info.kind == AppKind::Flutter {
            return Err("a flutter app needs --flutter <release bundle>".to_owned());
        }
        if !parts.bins.is_empty() {
            let bin = contents.join("bin");
            fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
            for file in &parts.bins {
                let name = file.file_name().ok_or("an executable without a name")?;
                let target = bin.join(name);
                fs::copy(file, &target).map_err(|e| format!("{}: {e}", file.display()))?;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())?;
            }
        }
        if let Some(lib) = &parts.lib {
            copy_dir(lib, &contents.join("lib"))?;
        }
        let resources = contents.join("Resources");
        if let Some(icon) = &icon {
            copy_dir(icon, &resources.join("icon"))?;
        }
        if let Some(documents) = &parts.documents {
            copy_dir(documents, &resources.join("documents"))?;
        }
        if let Some(l10n) = &parts.l10n {
            copy_dir(l10n, &resources.join("l10n"))?;
        }
        if !contents.join(&info.exec).exists() {
            return Err(format!("exec {} is not in the bundle", info.exec));
        }
        if bundle.exists() {
            fs::remove_dir_all(&bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
        }
        fs::rename(&staging, &bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
        Ok(bundle.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flutter_bundle_from_a_release_folder() {
        let root = std::env::temp_dir().join(format!("assemble-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let release = root.join("build/release");
        fs::create_dir_all(release.join("flutter_assets/fonts")).unwrap();
        fs::create_dir_all(release.join("icon")).unwrap();
        fs::write(release.join("flutter_assets/AssetManifest.bin"), "m").unwrap();
        fs::write(release.join("libapp.so"), "elf").unwrap();
        fs::write(release.join("icon/icon.png"), "png").unwrap();
        fs::write(
            root.join("Info.toml.in"),
            "format = 1\nid = \"dev.frascella.Calculator\"\nname = \"Calculator\"\n\
             version = \"@VERSION@\"\nbuild = @BUILD@\nkind = \"flutter\"\n\
             runtime = \"dev.silica.Platform//1\"\nargs = [\"--size\", \"248x412\"]\n\
             [developer]\nname = \"Silica\"\nkey = \"@DEVELOPER_KEY@\"\n",
        )
        .unwrap();
        let mut parts = Parts {
            info: root.join("Info.toml.in"),
            flutter: Some(release.clone()),
            ..Parts::default()
        };
        // A placeholder left: refused.
        assert!(assemble(&parts, &root).is_err());
        parts.substitutions = vec![
            ("VERSION".into(), "0.2.0".into()),
            ("BUILD".into(), "7".into()),
            ("DEVELOPER_KEY".into(), "RWQkey".into()),
        ];
        let bundle = assemble(&parts, &root).unwrap();
        assert_eq!(bundle, root.join("Calculator.app"));
        let c = bundle.join("Contents");
        assert!(c.join("app/flutter_assets/AssetManifest.bin").is_file());
        assert!(c.join("app/libapp.so").is_file());
        assert!(c.join("Resources/icon/icon.png").is_file());
        let info = Info::read(&bundle).unwrap();
        assert_eq!((info.version.as_str(), info.build), ("0.2.0", 7));
        // Built again: replaced, no staging left.
        assemble(&parts, &root).unwrap();
        assert!(!root.join(".Calculator.app.building").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
