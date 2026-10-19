//! `silica-bundle`: builds, signs, verifies and packages Silica app bundles (silica-os
//! `docs/app-bundles.md`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use silica_apps::registry::{Location, app_of_info};
use silica_bundle::assemble::{Parts, assemble};
use silica_bundle::minisign::{KeyId, PublicKey, SecretKey};
use silica_bundle::trust::{self, DeveloperStatus, Integrity, Roots};
use silica_bundle::{sxv, zip};

const USAGE: &str = "usage: silica-bundle <command> [options]

  keygen -p KEY.pub -s KEY.key            a new developer key pair (minisign format)
  build --info Info.toml[.in] [-o DIR] [--key KEY.key] [--cert FILE]
        [--set NAME=VALUE]... [--flutter RELEASE_DIR] [--bin FILE]... [--lib DIR]
        [--icon DIR] [--documents DIR] [--l10n DIR]
                                          puts a Name.app together (signed with --key;
                                          --key also fills @DEVELOPER_KEY@)
  sign --key KEY.key [--cert FILE] Name.app
  verify [--ca-dir DIR] Name.app          exit 0 only for a valid signature
  certify --ca-key CA.key --name NAME developer.pub
                                          prints a developer certificate
  revoke --ca-key CA.key --serial N -o revoked.list [KEY_ID...]
  dmg Name.app -o Name.sxv [--key KEY.key] [--cert FILE] [--name VOLUME]
      [--window FILE] [--background PNG] [--license FILE] [--readme FILE]
      [--compression lz4hc|zstd] [--mkfs PATH]
  inspect Name.sxv                        header and signature of a disk image
  pack Name.app -o Name.silicaapp         a zip for developers and CI
  unpack Name.silicaapp -o DIR
  desktop Name.app [--location system|local|user]
                                          the desktop entry of a bundle (for packages)
  feed --key KEY.key --archive Name.silicaapp --url URL -o feed.toml [--zsync URL]
       [--notes TEXT] Name.app            a signed update feed (feed.toml + .minisig)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("silica-bundle: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Options and positional arguments.
struct Args {
    options: Vec<(String, String)>,
    flags: Vec<String>,
    positional: Vec<String>,
}

impl Args {
    fn parse(args: &[String], flags: &[&str]) -> Result<Args, String> {
        let mut out = Args {
            options: Vec::new(),
            flags: Vec::new(),
            positional: Vec::new(),
        };
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            if flags.contains(&arg.as_str()) {
                out.flags.push(arg.clone());
            } else if arg.starts_with('-') && arg.len() > 1 {
                let value = it.next().ok_or_else(|| format!("{arg} needs a value"))?;
                out.options.push((arg.clone(), value.clone()));
            } else {
                out.positional.push(arg.clone());
            }
        }
        Ok(out)
    }

    fn get(&self, names: &[&str]) -> Option<&str> {
        self.options
            .iter()
            .rev()
            .find(|(k, _)| names.contains(&k.as_str()))
            .map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.options
            .iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    fn need(&self, names: &[&str]) -> Result<&str, String> {
        self.get(names)
            .ok_or_else(|| format!("{} is required\n{USAGE}", names.join("/")))
    }

    fn one(&self) -> Result<&str, String> {
        match self.positional.as_slice() {
            [one] => Ok(one),
            _ => Err(format!("one path expected\n{USAGE}")),
        }
    }
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

fn write(path: &str, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))
}

fn secret(path: &str) -> Result<SecretKey, String> {
    SecretKey::parse(&read(path)?).map_err(|e| format!("{path}: {e}"))
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(USAGE.to_owned());
    };
    match command.as_str() {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        "keygen" => {
            let a = Args::parse(rest, &[])?;
            let key = SecretKey::generate()?;
            let secret_path = a.need(&["-s"])?;
            if Path::new(secret_path).exists() {
                return Err(format!("{secret_path} exists: not overwritten"));
            }
            write(secret_path, &key.to_file())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(secret_path, std::fs::Permissions::from_mode(0o600));
            }
            write(a.need(&["-p"])?, &key.public().to_file())?;
            println!("{}", key.public().to_base64());
            Ok(ExitCode::SUCCESS)
        }
        "build" => {
            let a = Args::parse(rest, &[])?;
            let key = a.get(&["--key"]).map(secret).transpose()?;
            let mut substitutions = Vec::new();
            for item in a.all("--set") {
                let (name, value) = item
                    .split_once('=')
                    .ok_or_else(|| format!("--set {item}: NAME=VALUE expected"))?;
                substitutions.push((name.to_owned(), value.to_owned()));
            }
            if let Some(key) = &key {
                substitutions.push(("DEVELOPER_KEY".to_owned(), key.public().to_base64()));
            }
            let parts = Parts {
                info: PathBuf::from(a.need(&["--info"])?),
                substitutions,
                flutter: a.get(&["--flutter"]).map(PathBuf::from),
                bins: a.all("--bin").into_iter().map(PathBuf::from).collect(),
                lib: a.get(&["--lib"]).map(PathBuf::from),
                icon: a.get(&["--icon"]).map(PathBuf::from),
                documents: a.get(&["--documents"]).map(PathBuf::from),
                l10n: a.get(&["--l10n"]).map(PathBuf::from),
            };
            let out = PathBuf::from(a.get(&["-o"]).unwrap_or("."));
            std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            let bundle = assemble(&parts, &out)?;
            if let Some(key) = &key {
                let cert = a.get(&["--cert"]).map(read).transpose()?;
                trust::sign_bundle(&bundle, key, cert.as_deref())?;
            }
            println!("{}", bundle.display());
            Ok(ExitCode::SUCCESS)
        }
        "sign" => {
            let a = Args::parse(rest, &[])?;
            let key = secret(a.need(&["--key"])?)?;
            let cert = a.get(&["--cert"]).map(read).transpose()?;
            trust::sign_bundle(Path::new(a.one()?), &key, cert.as_deref())?;
            Ok(ExitCode::SUCCESS)
        }
        "verify" => {
            let a = Args::parse(rest, &[])?;
            let roots = match a.get(&["--ca-dir"]) {
                Some(dir) => Roots::load(Path::new(dir), &[]),
                None => Roots::system(None),
            };
            let v = trust::verify_bundle(Path::new(a.one()?), &roots);
            let (ok, text) = match &v.integrity {
                Integrity::Valid => (true, "valid".to_owned()),
                Integrity::Unsigned => (false, "unsigned".to_owned()),
                Integrity::Tampered(why) => (false, format!("tampered: {why}")),
                Integrity::Invalid(why) => (false, format!("invalid: {why}")),
            };
            println!("signature: {text}");
            println!("developer: {}", v.developer.as_str());
            if let Some(info) = &v.info {
                println!(
                    "id: {}\nversion: {} ({})",
                    info.id, info.version, info.build
                );
                println!("developer name: {}", info.developer.name);
            }
            if let Some(id) = v.key_id {
                println!("key: {id}");
            }
            Ok(if ok && v.developer != DeveloperStatus::Revoked {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        "certify" => {
            let a = Args::parse(rest, &[])?;
            let ca = secret(a.need(&["--ca-key"])?)?;
            let developer = PublicKey::parse(&read(a.one()?)?).map_err(|e| e.to_string())?;
            print!("{}", trust::certify(&ca, &developer, a.need(&["--name"])?));
            Ok(ExitCode::SUCCESS)
        }
        "revoke" => {
            let a = Args::parse(rest, &[])?;
            let ca = secret(a.need(&["--ca-key"])?)?;
            let serial: u64 = a
                .need(&["--serial"])?
                .parse()
                .map_err(|_| "--serial: a number".to_owned())?;
            let ids = a
                .positional
                .iter()
                .map(|id| KeyId::parse(id).ok_or_else(|| format!("not a key id: {id}")))
                .collect::<Result<Vec<_>, _>>()?;
            let text = trust::render_revoked(serial, &ids);
            let out = a.need(&["-o"])?;
            write(out, &text)?;
            write(
                &format!("{out}.minisig"),
                &ca.sign(text.as_bytes(), &format!("silica-revoked serial={serial}")),
            )?;
            Ok(ExitCode::SUCCESS)
        }
        "dmg" => {
            let a = Args::parse(rest, &[])?;
            let key = a.get(&["--key"]).map(secret).transpose()?;
            let cert = a.get(&["--cert"]).map(read).transpose()?;
            let options = sxv::BuildOptions {
                name: a.get(&["--name"]).map(str::to_owned),
                window: a.get(&["--window"]).map(PathBuf::from),
                background: a.get(&["--background"]).map(PathBuf::from),
                license: a.get(&["--license"]).map(PathBuf::from),
                readme: a.get(&["--readme"]).map(PathBuf::from),
                compression: a.get(&["--compression"]).map(str::to_owned),
                mkfs: a.get(&["--mkfs"]).map(PathBuf::from),
            };
            let out = PathBuf::from(a.need(&["-o"])?);
            let header = sxv::build(
                Path::new(a.one()?),
                &options,
                key.as_ref(),
                cert.as_deref(),
                &out,
            )?;
            println!(
                "{}: volume \"{}\", {} bytes, {}",
                out.display(),
                header.name,
                header.payload_len,
                if key.is_some() { "signed" } else { "unsigned" }
            );
            Ok(ExitCode::SUCCESS)
        }
        "inspect" => {
            let a = Args::parse(rest, &[])?;
            let image = sxv::inspect(Path::new(a.one()?))?;
            println!("volume: {}", image.header.name);
            println!("filesystem: {}", image.header.filesystem.as_str());
            println!("payload: {} bytes", image.header.payload_len);
            println!("license: {}", image.header.has_license);
            let ok = match &image.signature {
                sxv::Signature::Valid { key, cert } => {
                    println!(
                        "signature: valid, key {}{}",
                        key.id,
                        if cert.is_some() { ", certified" } else { "" }
                    );
                    true
                }
                sxv::Signature::Invalid(why) => {
                    println!("signature: INVALID: {why}");
                    false
                }
                sxv::Signature::Unsigned => {
                    println!("signature: none");
                    false
                }
            };
            Ok(if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        "pack" => {
            let a = Args::parse(rest, &[])?;
            zip::pack(Path::new(a.one()?), Path::new(a.need(&["-o"])?))?;
            Ok(ExitCode::SUCCESS)
        }
        "unpack" => {
            let a = Args::parse(rest, &[])?;
            let bundle = zip::unpack(
                Path::new(a.one()?),
                Path::new(a.need(&["-o"])?),
                8 * 1024 * 1024 * 1024,
            )?;
            println!("{}", bundle.display());
            Ok(ExitCode::SUCCESS)
        }
        "desktop" => {
            let a = Args::parse(rest, &[])?;
            let path = Path::new(a.one()?);
            let location = match a.get(&["--location"]).unwrap_or("system") {
                "system" => Location::System,
                "local" => Location::Local,
                "user" => Location::User,
                other => return Err(format!("unknown location {other}")),
            };
            let info = silica_apps::info::Info::read(path).map_err(|e| e.to_string())?;
            print!("{}", app_of_info(&info, path, location).compat_entry());
            Ok(ExitCode::SUCCESS)
        }
        "feed" => {
            let a = Args::parse(rest, &[])?;
            let key = secret(a.need(&["--key"])?)?;
            let bundle = Path::new(a.one()?);
            let info = silica_apps::info::Info::read(bundle).map_err(|e| e.to_string())?;
            let archive = a.need(&["--archive"])?;
            let sha256 = silica_bundle::manifest::sha256_file(Path::new(archive))
                .map_err(|e| format!("{archive}: {e}"))?;
            let size = std::fs::metadata(archive)
                .map_err(|e| format!("{archive}: {e}"))?
                .len();
            let url = a.need(&["--url"])?;
            if !url.starts_with("https://") {
                return Err("--url must be https".to_owned());
            }
            let mut text = format!(
                "format = 1\nid = {:?}\nversion = {:?}\nbuild = {}\nurl = {url:?}\nsha256 = {sha256:?}\nsize = {size}\n",
                info.id, info.version, info.build
            );
            if let Some(min) = &info.minimum_silica {
                text.push_str(&format!("minimum_silica = {min:?}\n"));
            }
            if let Some(zsync) = a.get(&["--zsync"]) {
                text.push_str(&format!("zsync = {zsync:?}\n"));
            }
            if let Some(notes) = a.get(&["--notes"]) {
                text.push_str(&format!("notes = {notes:?}\n"));
            }
            let out = a.need(&["-o"])?;
            write(out, &text)?;
            write(
                &format!("{out}.minisig"),
                &key.sign(
                    text.as_bytes(),
                    &format!("silica-feed id={} build={}", info.id, info.build),
                ),
            )?;
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command {other}\n{USAGE}")),
    }
}
