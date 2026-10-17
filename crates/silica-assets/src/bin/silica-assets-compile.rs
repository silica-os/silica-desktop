//! `silica-assets-compile --icons DIR [--brand DIR] [--wallpapers DIR] [--fonts DIR] [--sounds DIR] [--types FILE] [--compose FILE]
//! [--version TEXT] --out FILE`: compiles the sources into a `silica.assets`.
//!
//! `silica-assets-compile --app-icon DIR --out FILE` compiles one app icon folder (silica-icons
//! format 1 or 2) into an `icon.assets` (docs/assets-format.md, "App icon file").
//!
//! The file is written to a temporary name and renamed over `--out`, so a running session that
//! has the old file mapped keeps reading the old bytes.

use std::path::PathBuf;
use std::process::ExitCode;

use silica_assets::compile::{Options, compile};

const USAGE: &str = "usage: silica-assets-compile --icons DIR [--brand DIR] [--wallpapers DIR] [--fonts DIR] \
[--sounds DIR] [--types FILE] [--compose FILE] [--version TEXT] --out FILE\n       silica-assets-compile --app-icon DIR --out FILE";

fn write_atomic(out: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let mut temporary = out.to_path_buf().into_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, bytes).map_err(|e| format!("{}: {e}", temporary.display()))?;
    std::fs::rename(&temporary, out).map_err(|e| format!("{}: {e}", out.display()))
}

fn run() -> Result<(), String> {
    let mut options = Options::default();
    let mut out: Option<PathBuf> = None;
    let mut app_icon: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--icons" => options.icons = PathBuf::from(value("--icons")?),
            "--brand" => options.brand = Some(PathBuf::from(value("--brand")?)),
            "--wallpapers" => options.wallpapers = Some(PathBuf::from(value("--wallpapers")?)),
            "--fonts" => options.fonts = Some(PathBuf::from(value("--fonts")?)),
            "--sounds" => options.sounds = Some(PathBuf::from(value("--sounds")?)),
            "--types" => options.types = Some(PathBuf::from(value("--types")?)),
            "--compose" => options.compose = Some(PathBuf::from(value("--compose")?)),
            "--version" => options.content_version = Some(value("--version")?),
            "--app-icon" => app_icon = Some(PathBuf::from(value("--app-icon")?)),
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    if let Some(dir) = app_icon {
        let out = out.ok_or(USAGE)?;
        let bytes = silica_assets::compile::app_icon(&dir).map_err(|e| e.to_string())?;
        return write_atomic(&out, &bytes);
    }
    if options.icons.as_os_str().is_empty() {
        return Err(USAGE.to_owned());
    }
    let out = out.ok_or(USAGE)?;
    let (bytes, report) = compile(&options).map_err(|e| e.to_string())?;
    for warning in &report.warnings {
        eprintln!("silica-assets-compile: warning: {warning}");
    }
    write_atomic(&out, &bytes)?;
    eprintln!(
        "silica-assets-compile: {} ({} bytes): {} apps, {} files, {} badges, {} symbols, {} fonts, {} sounds, {} wallpapers, {} aliases",
        out.display(),
        bytes.len(),
        report.apps,
        report.files,
        report.badges,
        report.symbols,
        report.fonts,
        report.sounds,
        report.wallpapers,
        report.aliases
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("silica-assets-compile: {message}");
            ExitCode::FAILURE
        }
    }
}
