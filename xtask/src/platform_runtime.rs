//! `cargo xtask platform-runtime [--build]`: the Flatpak runtime `dev.silica.Platform//1`
//! (silica-desktop#258, silica-os docs/app-bundles.md § 2). It writes the offline Cargo sources
//! of the embedder (`data/flatpak/generated-cargo-sources.json`, from `Cargo.lock`, as
//! flatpak-cargo-generator does) next to the manifest `data/flatpak/dev.silica.Platform.yml`
//! and, with `--build`, runs `flatpak-builder` into `target/flatpak/repo`.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use crate::Result;

/// The crates of a `Cargo.lock` from crates.io: (name, version, checksum).
pub(crate) fn registry_crates(lock: &str) -> Result<Vec<(String, String, String)>> {
    let mut out = Vec::new();
    for block in lock.split("[[package]]").skip(1) {
        let field = |key: &str| {
            block.lines().find_map(|l| {
                l.trim()
                    .strip_prefix(key)
                    .and_then(|r| r.trim_start().strip_prefix('='))
                    .map(|v| v.trim().trim_matches('"').to_owned())
            })
        };
        let (Some(name), Some(version)) = (field("name"), field("version")) else {
            return Err("Cargo.lock: a package without name or version".to_owned());
        };
        match field("source") {
            // A crate of this workspace: built from the source tree.
            None => continue,
            Some(source) if source.starts_with("registry+") => {
                let checksum = field("checksum")
                    .ok_or_else(|| format!("Cargo.lock: {name} {version} has no checksum"))?;
                out.push((name, version, checksum));
            }
            Some(source) => {
                return Err(format!(
                    "Cargo.lock: {name} {version} comes from {source}: only crates.io is vendored"
                ));
            }
        }
    }
    Ok(out)
}

/// The flatpak-builder sources that vendor `crates` offline: each `.crate` archive unpacked in
/// `cargo/vendor/<name>-<version>` with its `.cargo-checksum.json`, and the Cargo
/// configuration that replaces crates.io with that folder.
pub(crate) fn cargo_sources(crates: &[(String, String, String)]) -> Value {
    let mut sources: Vec<Value> = Vec::new();
    for (name, version, checksum) in crates {
        let dest = format!("cargo/vendor/{name}-{version}");
        sources.push(json!({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": format!("https://static.crates.io/crates/{name}/{name}-{version}.crate"),
            "sha256": checksum,
            "dest": dest,
        }));
        sources.push(json!({
            "type": "inline",
            "contents": json!({"package": checksum, "files": {}}).to_string(),
            "dest": dest,
            "dest-filename": ".cargo-checksum.json",
        }));
    }
    sources.push(json!({
        "type": "inline",
        "contents": "[source.vendored-sources]\ndirectory = \"cargo/vendor\"\n\n[source.crates-io]\nreplace-with = \"vendored-sources\"\n",
        "dest": "cargo",
        "dest-filename": "config",
    }));
    Value::Array(sources)
}

pub(crate) fn platform_runtime(args: &[String]) -> Result<()> {
    let root = crate::root()?;
    let lock =
        std::fs::read_to_string(root.join("Cargo.lock")).map_err(|e| format!("Cargo.lock: {e}"))?;
    let sources = cargo_sources(&registry_crates(&lock)?);
    let out = root.join("data/flatpak/generated-cargo-sources.json");
    let text = serde_json::to_string_pretty(&sources).map_err(|e| e.to_string())?;
    std::fs::write(&out, text + "\n").map_err(|e| format!("{}: {e}", out.display()))?;
    eprintln!("xtask: {}", out.display());
    if !args.iter().any(|a| a == "--build") {
        return Ok(());
    }
    build(&root)
}

fn build(root: &Path) -> Result<()> {
    let manifest = root.join("data/flatpak/dev.silica.Platform.yml");
    let work = crate::os::target_dir()?.join("flatpak");
    let status = Command::new("flatpak-builder")
        .arg("--user")
        .arg("--install-deps-from=flathub")
        .arg("--force-clean")
        .arg(format!("--repo={}", work.join("repo").display()))
        .arg(work.join("build"))
        .arg(&manifest)
        .status()
        .map_err(|e| format!("flatpak-builder: {e} (install flatpak-builder)"))?;
    if !status.success() {
        return Err(format!("flatpak-builder: {status}"));
    }
    eprintln!(
        "xtask: the runtime is in {}: flatpak --user remote-add --no-gpg-verify silica-local {} && flatpak --user install silica-local dev.silica.Platform//1",
        work.join("repo").display(),
        work.join("repo").display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = r#"
version = 4

[[package]]
name = "ab_glyph"
version = "0.2.32"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "01c0457472c38ea5bd1c3b5ada5e368271cb550be7a4ca4a0b4634e9913f6cc2"
dependencies = [
 "owned_ttf_parser",
]

[[package]]
name = "silica-embedder"
version = "0.0.0"
dependencies = [
 "ab_glyph",
]
"#;

    #[test]
    fn the_lock_becomes_offline_sources() {
        let crates = registry_crates(LOCK).unwrap();
        assert_eq!(
            crates,
            [(
                "ab_glyph".to_owned(),
                "0.2.32".to_owned(),
                "01c0457472c38ea5bd1c3b5ada5e368271cb550be7a4ca4a0b4634e9913f6cc2".to_owned()
            )]
        );
        let sources = cargo_sources(&crates);
        let list = sources.as_array().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(
            list[0]["url"],
            "https://static.crates.io/crates/ab_glyph/ab_glyph-0.2.32.crate"
        );
        assert_eq!(list[0]["dest"], "cargo/vendor/ab_glyph-0.2.32");
        assert_eq!(list[1]["dest-filename"], ".cargo-checksum.json");
        assert!(
            list[2]["contents"]
                .as_str()
                .unwrap()
                .contains("replace-with")
        );
        // A git dependency cannot be vendored this way.
        let git = LOCK.replace(
            "registry+https://github.com/rust-lang/crates.io-index",
            "git+https://example.org/x",
        );
        assert!(registry_crates(&git).is_err());
    }

    #[test]
    fn the_workspace_lock_is_vendorable() {
        let lock =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.lock"))
                .unwrap();
        assert!(registry_crates(&lock).unwrap().len() > 100);
    }
}
