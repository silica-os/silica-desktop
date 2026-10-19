//! Flutter for the embedder: engine, bindings, app bundles.
//!
//! - `engine`: downloads `libflutter_engine.so` and `icudtl.dat` for `crates/embedder/ENGINE_VERSION`.
//! - `engine --release`: builds the release engine from source (`docs/engine-release.md`):
//!   Google publishes the engine for custom embedders only in debug. The patches of
//!   `crates/embedder/engine-patches` are applied first and listed in `silica-patches` next to
//!   the library, where the embedder looks for them.
//! - `bindings`: regenerates `crates/embedder/src/ffi/bindings.rs` with bindgen.
//! - `bundle <shell | demo | path> [--release]`: the bundle of the shell, of the test app
//!   or of an external app (its own repository); in release also `libapp.so`.
//! - `projects`/`check`: used by `cargo xtask ci` for `flutter analyze` and `flutter test`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{Result, root, status};

const STORAGE: &str = "https://storage.googleapis.com/flutter_infra_release/flutter";

pub fn engine_version() -> Result<String> {
    let path = root()?.join("crates/embedder/ENGINE_VERSION");
    std::fs::read_to_string(&path)
        .map(|v| v.trim().to_owned())
        .map_err(|e| format!("{} unreadable: {e}", path.display()))
}

pub fn engine_dir() -> Result<PathBuf> {
    Ok(root()?
        .join("target/flutter-engine")
        .join(engine_version()?))
}

/// The release engine built by `engine --release`.
pub fn release_engine_dir() -> Result<PathBuf> {
    Ok(root()?
        .join("target/flutter-engine")
        .join(format!("{}-release", engine_version()?)))
}

/// The Flutter SDK root: `FLUTTER_ROOT`, or from `flutter` in PATH.
fn sdk_root() -> Result<PathBuf> {
    if let Ok(root) = std::env::var("FLUTTER_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let output = Command::new("sh")
        .args(["-c", "command -v flutter"])
        .output()
        .map_err(|e| format!("cannot start sh: {e}"))?;
    let flutter = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if flutter.is_empty() {
        return Err("Flutter SDK not found: install it or set FLUTTER_ROOT".to_owned());
    }
    std::fs::canonicalize(&flutter)
        .ok()
        .and_then(|bin| bin.parent()?.parent().map(Path::to_path_buf))
        .ok_or_else(|| format!("cannot derive the SDK root from {flutter}"))
}

/// `PATH` for the Flutter commands: adds the `unzip` shim if the real one is missing.
fn tool_path() -> Result<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let has_unzip = Command::new("sh")
        .args(["-c", "command -v unzip"])
        .output()
        .is_ok_and(|o| o.status.success());
    if has_unzip {
        return Ok(path);
    }
    let tools = root()?.join("xtask/tools");
    Ok(format!("{}:{path}", tools.display()))
}

fn flutter_command(args: &[&str]) -> Result<Command> {
    let mut command = Command::new(sdk_root()?.join("bin/flutter"));
    command.args(args).env("PATH", tool_path()?);
    Ok(command)
}

pub fn engine() -> Result<()> {
    let version = engine_version()?;
    let sdk = sdk_root()?;
    let sdk_version = std::fs::read_to_string(sdk.join("bin/internal/engine.version"))
        .map(|v| v.trim().to_owned())
        .map_err(|e| format!("unreadable engine version of the SDK: {e}"))?;
    if sdk_version != version {
        return Err(format!(
            "the Flutter SDK uses engine {sdk_version}, the repo {version}: bundles compiled \
             by the SDK only run on the engine of the same SDK. Update \
             crates/embedder/ENGINE_VERSION and regenerate with `cargo xtask bindings`."
        ));
    }

    let dir = engine_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if !dir.join("libflutter_engine.so").exists() {
        let zip = dir.join("embedder.zip");
        let mut curl = Command::new("curl");
        curl.args(["-sSfL", "-o"]).arg(&zip).arg(format!(
            "{STORAGE}/{version}/linux-x64/linux-x64-embedder.zip"
        ));
        status(curl)?;
        let mut unzip = Command::new("unzip");
        unzip
            .args(["-o", "-q"])
            .arg(&zip)
            .arg("-d")
            .arg(&dir)
            .env("PATH", tool_path()?);
        status(unzip)?;
        let _ = std::fs::remove_file(&zip);
    }

    let icu = sdk.join("bin/cache/artifacts/engine/linux-x64/icudtl.dat");
    if !icu.exists() {
        status(flutter_command(&[
            "precache",
            "--linux",
            "--no-android",
            "--no-ios",
            "--no-web",
        ])?)?;
    }
    std::fs::copy(&icu, dir.join("icudtl.dat")).map_err(|e| format!("icudtl.dat: {e}"))?;
    eprintln!("xtask: engine {version} in {}", dir.display());
    Ok(())
}

/// Builds the release `libflutter_engine.so` from the checkout in `SILICA_ENGINE_SRC`, already
/// prepared with gclient at the `ENGINE_VERSION` revision (`docs/engine-release.md`).
pub fn engine_release() -> Result<()> {
    let version = engine_version()?;
    let source = std::env::var_os("SILICA_ENGINE_SRC")
        .map(PathBuf::from)
        .ok_or(
            "SILICA_ENGINE_SRC not set: the engine checkout is prepared as in \
         docs/engine-release.md",
        )?;
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source)
        .output()
        .map_err(|e| format!("cannot start git: {e}"))?;
    let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    if head != version {
        return Err(format!(
            "{} is at revision {head}, the repo wants {version}",
            source.display()
        ));
    }
    let patches = apply_engine_patches(&source)?;
    let src = source.join("engine/src");
    let ninja = source.join("third_party/ninja/ninja");
    if !ninja.exists() {
        return Err(format!(
            "{} missing: `gclient sync` is needed",
            ninja.display()
        ));
    }

    // The build scripts call depot_tools' vpython3: python3 is enough.
    let shims = root()?.join("target/xtask-shims");
    std::fs::create_dir_all(&shims).map_err(|e| format!("{}: {e}", shims.display()))?;
    let vpython = shims.join("vpython3");
    if !vpython.exists() {
        std::os::unix::fs::symlink("/usr/bin/python3", &vpython)
            .map_err(|e| format!("{}: {e}", vpython.display()))?;
    }
    let path = format!(
        "{}:{}",
        shims.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let mut gn = Command::new("python3");
    gn.arg("flutter/tools/gn")
        .args([
            "--runtime-mode",
            "release",
            "--no-rbe",
            "--no-goma",
            "--no-enable-unittests",
        ])
        .current_dir(&src)
        .env("PATH", &path);
    status(gn)?;
    let mut build = Command::new(&ninja);
    build
        .args([
            "-C",
            "out/host_release",
            "flutter/shell/platform/embedder:flutter_engine",
        ])
        .current_dir(&src)
        .env("PATH", &path);
    status(build)?;

    let dir = release_engine_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::copy(
        src.join("out/host_release/libflutter_engine.so"),
        dir.join("libflutter_engine.so"),
    )
    .map_err(|e| format!("libflutter_engine.so: {e}"))?;
    std::fs::copy(engine_dir()?.join("icudtl.dat"), dir.join("icudtl.dat"))
        .map_err(|e| format!("icudtl.dat (run `cargo xtask engine` first): {e}"))?;
    let listing = dir.join(PATCH_LIST);
    std::fs::write(&listing, patch_list(&patches))
        .map_err(|e| format!("{}: {e}", listing.display()))?;
    let applied = if patches.is_empty() {
        "none".to_owned()
    } else {
        patches.join(", ")
    };
    eprintln!(
        "xtask: engine release {version} in {} (patches: {applied})",
        dir.display()
    );
    Ok(())
}

/// The list of the patches a release engine was built with, next to `libflutter_engine.so`
/// (`silica_embedder::engine_has_patch`).
pub const PATCH_LIST: &str = "silica-patches";

/// The contents of [`PATCH_LIST`]: one patch name per line.
fn patch_list(patches: &[String]) -> String {
    patches.iter().map(|name| format!("{name}\n")).collect()
}

/// The patches in `dir` (`crates/embedder/engine-patches`), sorted by file name:
/// (name, path), the name being the file name without `.patch`.
fn engine_patches(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut patches: Vec<(String, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "patch"))
        .filter_map(|path| {
            let name = path.file_stem()?.to_str()?.to_owned();
            Some((name, path))
        })
        .collect();
    patches.sort();
    patches
}

/// Applies the engine patches to the checkout `source`, each one once (a patch that applies
/// in reverse is already there). Returns their names.
fn apply_engine_patches(source: &Path) -> Result<Vec<String>> {
    let apply = |patch: &Path, extra: &[&str]| {
        Command::new("git")
            .arg("apply")
            .args(extra)
            .arg(patch)
            .current_dir(source)
            .output()
            .map(|output| output.status.success())
            .map_err(|e| format!("cannot start git: {e}"))
    };
    let mut names = Vec::new();
    for (name, patch) in engine_patches(&root()?.join("crates/embedder/engine-patches")) {
        if apply(&patch, &["--reverse", "--check"])? {
            eprintln!("xtask: engine patch {name} already applied");
        } else if apply(&patch, &[])? {
            eprintln!("xtask: engine patch {name} applied");
        } else {
            return Err(format!(
                "engine patch {} does not apply to {}: update or drop it",
                patch.display(),
                source.display()
            ));
        }
        names.push(name);
    }
    Ok(names)
}

pub fn bindings() -> Result<()> {
    let header = engine_dir()?.join("flutter_embedder.h");
    if !header.exists() {
        return Err("header missing: run `cargo xtask engine` first".to_owned());
    }
    let out = root()?.join("crates/embedder/src/ffi/bindings.rs");
    let mut bindgen = Command::new("bindgen");
    bindgen
        .arg(&header)
        .args([
            "--allowlist-function",
            "Flutter.*",
            "--allowlist-type",
            "Flutter.*",
            "--allowlist-var",
            "FLUTTER_.*|kFlutter.*",
            "--dynamic-loading",
            "FlutterEngineLib",
            "--default-enum-style",
            "newtype",
            "--with-derive-default",
            "--no-layout-tests",
            "--no-doc-comments",
            "--raw-line",
            "// Generated by `cargo xtask bindings` from flutter_embedder.h (BSD, The Flutter Authors).",
            "--raw-line",
            &format!("// Engine: {}. Do not edit by hand.", engine_version()?),
            "-o",
        ])
        .arg(&out);
    status(bindgen)?;
    status({
        let mut fmt = Command::new("rustfmt");
        fmt.args(["--edition", "2024"]).arg(&out);
        fmt
    })
}

/// The folder of a Flutter project to build: `shell` is the shell, `demo` the test app of
/// the embedder tests, otherwise the path of an external app (Silica's apps live in their
/// own repositories).
fn app_dir(app: &str) -> Result<PathBuf> {
    let root = root()?;
    Ok(match app {
        "shell" => root.join("shell"),
        "demo" => root.join("testapps/demo"),
        path => std::path::absolute(path).map_err(|e| format!("{path}: {e}"))?,
    })
}

pub fn bundle(app: &str) -> Result<()> {
    let dir = app_dir(app)?;
    if !dir.join("pubspec.yaml").exists() {
        return Err(format!("{} is not a Flutter app", dir.display()));
    }
    let mut command = flutter_command(&["build", "bundle", "--debug"])?;
    command.current_dir(&dir);
    status(command)?;
    crate::app_icon::bundle_icon(&dir, &dir.join("build"))?;
    eprintln!(
        "xtask: bundle in {}",
        dir.join("build/flutter_assets").display()
    );
    Ok(())
}

/// Release bundle in `build/release/`: assets and `libapp.so`, the Dart compiled AOT with
/// the SDK's frontend and gen_snapshot (the same steps as `flutter build linux`).
pub fn bundle_release(app: &str) -> Result<()> {
    let dir = app_dir(app)?;
    if !dir.join("pubspec.yaml").exists() {
        return Err(format!("{} is not a Flutter app", dir.display()));
    }
    let out = dir.join("build/release");
    let mut assets = flutter_command(&["build", "bundle", "--release", "--asset-dir"])?;
    assets.arg(out.join("flutter_assets")).current_dir(&dir);
    status(assets)?;

    let cache = sdk_root()?.join("bin/cache");
    let package = app_package(&dir)?;
    let dill = out.join("app.dill");
    let mut kernel = Command::new(cache.join("dart-sdk/bin/dartaotruntime"));
    kernel
        .arg(cache.join("dart-sdk/bin/snapshots/frontend_server_aot.dart.snapshot"))
        .arg("--sdk-root")
        .arg(cache.join("artifacts/engine/common/flutter_patched_sdk_product/"))
        .args([
            "--target=flutter",
            "--aot",
            "--tfa",
            "--no-print-incremental-dependencies",
            "-Ddart.vm.product=true",
            "-Ddart.vm.profile=false",
            "--packages",
            ".dart_tool/package_config.json",
            "--output-dill",
        ])
        .arg(&dill)
        .arg(format!("package:{package}/main.dart"))
        .current_dir(&dir);
    status(kernel)?;

    let mut snapshot = Command::new(cache.join("artifacts/engine/linux-x64-release/gen_snapshot"));
    snapshot
        .args(["--deterministic", "--snapshot_kind=app-aot-elf", "--strip"])
        .arg(format!("--elf={}", out.join("libapp.so").display()))
        .arg(&dill);
    status(snapshot)?;
    let _ = std::fs::remove_file(&dill);
    crate::app_icon::bundle_icon(&dir, &out)?;
    eprintln!("xtask: bundle release in {}", out.display());
    Ok(())
}

/// The Dart package name from `pubspec.yaml`.
fn app_package(dir: &Path) -> Result<String> {
    let pubspec = std::fs::read_to_string(dir.join("pubspec.yaml"))
        .map_err(|e| format!("pubspec.yaml: {e}"))?;
    pubspec
        .lines()
        .find_map(|line| line.strip_prefix("name:"))
        .map(|name| name.trim().to_owned())
        .ok_or_else(|| "pubspec.yaml without name".to_owned())
}

/// The `version:` of a Dart project's `pubspec.yaml`, without the build number (`0.3.0+4` →
/// `0.3.0`).
pub fn pubspec_version(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project.join("pubspec.yaml")).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("version:"))
        .map(|v| v.trim().split('+').next().unwrap_or("").trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// The Dart projects of the repo: the shell and the test apps in `testapps/`, as relative
/// paths (`shell`, `testapps/demo`). `silica_ui` and the apps have their own repositories.
pub fn projects() -> Result<Vec<String>> {
    let root = root()?;
    let mut projects = Vec::new();
    if root.join("shell/pubspec.yaml").exists() {
        projects.push("shell".to_owned());
    }
    let dir = root.join("testapps");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    projects.extend(
        entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().join("pubspec.yaml").exists())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .map(|name| format!("testapps/{name}")),
    );
    projects.sort();
    Ok(projects)
}

/// `flutter analyze` and `flutter test` for a project of [`projects`].
pub fn check(project: &str) -> Result<()> {
    let dir = root()?.join(project);
    for args in [["analyze", "--fatal-infos"].as_slice(), ["test"].as_slice()] {
        let mut command = flutter_command(args)?;
        command.current_dir(&dir);
        status(command)?;
    }
    Ok(())
}

/// Extracts a `key: Type.name,` map from `keyboard_maps.g.dart` and resolves the names.
fn dart_map(
    maps: &str,
    map_name: &str,
    values: &std::collections::HashMap<String, u64>,
) -> Result<Vec<(u32, u64)>> {
    let start = maps
        .find(&format!("{map_name} = "))
        .ok_or_else(|| format!("{map_name} missing"))?;
    let body = maps
        .get(start..)
        .and_then(|rest| rest.split_once("};"))
        .map(|(body, _)| body)
        .ok_or_else(|| format!("{map_name} not closed"))?;
    let mut entries = Vec::new();
    for line in body.lines().skip(1) {
        let Some((key, value)) = line.trim().trim_end_matches(',').split_once(':') else {
            continue;
        };
        let key = key.trim();
        let key = match key.strip_prefix("0x") {
            Some(hex) => u32::from_str_radix(hex, 16),
            None => key.parse(),
        }
        .map_err(|e| format!("{map_name}: key {key}: {e}"))?;
        let name = value
            .trim()
            .rsplit('.')
            .next()
            .ok_or_else(|| format!("{map_name}: value {value}"))?;
        let code = values
            .get(name)
            .ok_or_else(|| format!("{map_name}: {name} not defined"))?;
        entries.push((key, *code));
    }
    entries.sort_unstable();
    entries.dedup_by_key(|(key, _)| *key);
    Ok(entries)
}

/// Regenerates `crates/embedder/src/keymap.rs` from the Flutter SDK tables.
pub fn keymap() -> Result<()> {
    let services = sdk_root()?.join("packages/flutter/lib/src/services");
    let read = |name: &str| {
        std::fs::read_to_string(services.join(name)).map_err(|e| format!("{name}: {e}"))
    };
    let keys = read("keyboard_key.g.dart")?;
    let maps = read("keyboard_maps.g.dart")?;

    // `static const LogicalKeyboardKey name = LogicalKeyboardKey(0x...);`
    let mut logical = std::collections::HashMap::new();
    let mut physical = std::collections::HashMap::new();
    for line in keys.lines() {
        let Some(rest) = line.trim().strip_prefix("static const ") else {
            continue;
        };
        let Some((kind, rest)) = rest.split_once(' ') else {
            continue;
        };
        let Some((name, value)) = rest.split_once(" = ") else {
            continue;
        };
        let Some(hex) = value.split_once("(0x").and_then(|(_, v)| v.split_once(')')) else {
            continue;
        };
        let Ok(code) = u64::from_str_radix(hex.0, 16) else {
            continue;
        };
        match kind {
            "LogicalKeyboardKey" => logical.insert(name.to_owned(), code),
            "PhysicalKeyboardKey" => physical.insert(name.to_owned(), code),
            _ => None,
        };
    }

    let table = |name: &str, doc: &str, entries: &[(u32, u64)]| {
        let rows: String = entries
            .iter()
            .map(|(key, value)| format!("    (0x{key:08x}, 0x{value:011x}),\n"))
            .collect();
        format!("/// {doc}\npub const {name}: &[(u32, u64)] = &[\n{rows}];\n")
    };
    let out = format!(
        "//! Generated by `cargo xtask keymap` from the Flutter SDK's keyboard_maps.g.dart\n\
         //! (BSD, The Flutter Authors). Do not edit by hand.\n\n{}\n{}\n{}",
        table(
            "LINUX_TO_PHYSICAL",
            "xkb keycode (evdev + 8) → Flutter physical key. Sorted by key.",
            &dart_map(&maps, "kLinuxToPhysicalKey", &physical)?,
        ),
        table(
            "GTK_TO_LOGICAL",
            "xkb keysym → Flutter logical key. Sorted by key.",
            &dart_map(&maps, "kGtkToLogicalKey", &logical)?,
        ),
        table(
            "GTK_NUMPAD",
            "Numeric keypad keysym → Flutter logical key. Sorted by key.",
            &dart_map(&maps, "kGtkNumpadMap", &logical)?,
        ),
    );
    let path = root()?.join("crates/embedder/src/keymap.rs");
    std::fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    eprintln!("xtask: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{engine_patches, patch_list};

    #[test]
    fn engine_patches_are_listed_by_name_in_order() {
        let dir = std::env::temp_dir().join(format!("xtask-patches-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for file in ["b-second.patch", "a-first.patch", "notes.txt"] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let names: Vec<String> = engine_patches(&dir).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["a-first", "b-second"]);
        assert_eq!(patch_list(&names), "a-first\nb-second\n");
        assert_eq!(patch_list(&[]), "");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(engine_patches(&dir).is_empty());
    }

    #[test]
    fn the_embedder_reads_the_list_xtask_writes() {
        // Both sides name the file the same way, and the patch the embedder looks for exists.
        assert_eq!(super::PATCH_LIST, "silica-patches");
        let dir = crate::root()
            .unwrap()
            .join("crates/embedder/engine-patches");
        let names: Vec<String> = engine_patches(&dir).into_iter().map(|(n, _)| n).collect();
        assert!(names.iter().any(|n| n == "impeller-vulkan-backing-stores"));
    }
}
