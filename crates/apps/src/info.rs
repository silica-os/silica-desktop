//! `Contents/Info.toml`, version 1: the description of an app bundle (silica-os
//! `docs/app-bundles.md` § 2).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

/// The largest `Info.toml` read.
pub const MAX_INFO: u64 = 64 * 1024;
/// The format this code reads.
pub const FORMAT: u32 = 1;

/// How the app runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    /// A Flutter bundle run by the platform's embedder.
    Flutter,
    /// An executable linked against the runtime (libvetro after silica-desktop#254).
    Native,
}

impl AppKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AppKind::Flutter => "flutter",
            AppKind::Native => "native",
        }
    }
}

/// The runtime an app runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Runtime {
    /// `none`: the bundle carries everything.
    None,
    /// A runtime reference `name//branch` (`dev.silica.Platform//1`).
    Ref { name: String, branch: String },
}

/// Silica's platform runtime.
pub const PLATFORM: &str = "dev.silica.Platform";

impl Runtime {
    pub fn parse(text: &str) -> Option<Runtime> {
        if text == "none" {
            return Some(Runtime::None);
        }
        let (name, branch) = text.split_once("//")?;
        let ok = |s: &str| {
            !s.is_empty()
                && !s.starts_with('.')
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        };
        (ok(name) && ok(branch) && name.contains('.')).then(|| Runtime::Ref {
            name: name.to_owned(),
            branch: branch.to_owned(),
        })
    }

    pub fn is_platform(&self) -> bool {
        matches!(self, Runtime::Ref { name, .. } if name == PLATFORM)
    }
}

impl std::fmt::Display for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Runtime::None => f.write_str("none"),
            Runtime::Ref { name, branch } => write!(f, "{name}//{branch}"),
        }
    }
}

/// The role of an app for a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Editor,
    Viewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Editor => "editor",
            Role::Viewer => "viewer",
        }
    }
}

/// Device access.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Devices {
    None,
    #[default]
    Gpu,
    All,
}

impl Devices {
    pub fn as_str(self) -> &'static str {
        match self {
            Devices::None => "none",
            Devices::Gpu => "gpu",
            Devices::All => "all",
        }
    }
}

/// `[permissions]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Permissions {
    pub network: bool,
    pub devices: Devices,
    pub filesystem: Vec<String>,
    pub talk: Vec<String>,
    pub background: bool,
}

/// `[developer]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Developer {
    pub name: String,
    /// The minisign public key, as the base64 line of `developer.pub`.
    pub key: String,
}

/// `[update]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Update {
    #[serde(default)]
    pub feed: Option<String>,
}

#[derive(Deserialize)]
struct Raw {
    format: u32,
    id: String,
    name: String,
    version: String,
    build: u64,
    #[serde(default)]
    minimum_silica: Option<String>,
    kind: AppKind,
    runtime: String,
    #[serde(default)]
    exec: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    url_schemes: Vec<String>,
    #[serde(default)]
    generic_name: Option<String>,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    share: bool,
    #[serde(default)]
    types: BTreeMap<String, Role>,
    #[serde(default)]
    documents: BTreeMap<String, String>,
    #[serde(default)]
    permissions: Permissions,
    #[serde(default)]
    update: Update,
    developer: Developer,
    #[serde(flatten)]
    extra: BTreeMap<String, toml::Value>,
}

/// A parsed and checked `Info.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub id: String,
    pub name: String,
    /// `"name.<lang>"` keys.
    pub names: BTreeMap<String, String>,
    pub version: String,
    pub build: u64,
    pub minimum_silica: Option<String>,
    pub kind: AppKind,
    pub runtime: Runtime,
    /// Relative to `Contents/`: the executable (native) or the Flutter bundle folder.
    pub exec: String,
    pub args: Vec<String>,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub url_schemes: Vec<String>,
    /// What kind of app it is ("Calculator", "Mail Client"), as `GenericName` of a desktop entry.
    pub generic_name: String,
    /// One line about it, as `Comment`.
    pub comment: String,
    /// A target of the Share menu (`X-Silica-Share=true`).
    pub share: bool,
    pub types: BTreeMap<String, Role>,
    pub documents: BTreeMap<String, String>,
    pub permissions: Permissions,
    pub update: Update,
    pub developer: Developer,
}

/// Why an `Info.toml` was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Io(String),
    TooLarge,
    Syntax(String),
    Format(u32),
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "cannot read Info.toml: {e}"),
            Error::TooLarge => write!(f, "Info.toml is larger than {MAX_INFO} bytes"),
            Error::Syntax(e) => write!(f, "Info.toml: {e}"),
            Error::Format(n) => write!(
                f,
                "Info.toml: unknown format {n} (this Silica reads {FORMAT})"
            ),
            Error::Invalid(e) => write!(f, "Info.toml: {e}"),
        }
    }
}

/// Whether `id` is a bundle id: reverse DNS, at least two dot-separated parts of letters, digits,
/// `_` and `-`.
pub fn valid_bundle_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    id.len() <= 255
        && parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        })
        && !id.starts_with(|c: char| c.is_ascii_digit())
}

/// Whether `path` is a plain relative path inside the bundle (no `..`, not absolute).
pub fn inside(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn valid_mime(mime: &str) -> bool {
    let Some((major, minor)) = mime.split_once('/') else {
        return false;
    };
    let ok = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
    };
    ok(major) && ok(minor)
}

fn valid_scheme(scheme: &str) -> bool {
    scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn valid_bus_name(name: &str) -> bool {
    valid_bundle_id(name) || (name.ends_with(".*") && valid_bundle_id(name.trim_end_matches(".*")))
}

/// Whether a `filesystem` item is one the sandbox understands.
pub fn valid_filesystem(item: &str) -> bool {
    let item = item.strip_suffix(":ro").unwrap_or(item);
    const NAMED: [&str; 8] = [
        "home",
        "xdg-documents",
        "xdg-download",
        "xdg-music",
        "xdg-pictures",
        "xdg-videos",
        "xdg-desktop",
        "xdg-templates",
    ];
    if NAMED.contains(&item) {
        return true;
    }
    for prefix in ["xdg-config/", "xdg-data/"] {
        if let Some(sub) = item.strip_prefix(prefix) {
            return inside(sub);
        }
    }
    if item.starts_with('/') {
        let path = Path::new(item);
        let normal = path
            .components()
            .skip(1)
            .all(|c| matches!(c, Component::Normal(_)));
        let first = path.components().nth(1);
        let forbidden = ["usr", "etc", "proc", "sys", "dev", "run", "boot", "app"];
        return normal
            && first.is_some_and(|c| {
                !forbidden
                    .iter()
                    .any(|f| c.as_os_str() == std::ffi::OsStr::new(f))
            });
    }
    false
}

impl Info {
    /// Parses and checks the text of an `Info.toml`.
    pub fn parse(text: &str) -> Result<Info, Error> {
        if text.len() as u64 > MAX_INFO {
            return Err(Error::TooLarge);
        }
        // The format first, so that a newer file is reported as such and not as a syntax error.
        if let Ok(value) = text.parse::<toml::Table>()
            && let Some(format) = value.get("format").and_then(toml::Value::as_integer)
            && format != i64::from(FORMAT)
        {
            return Err(Error::Format(u32::try_from(format).unwrap_or(u32::MAX)));
        }
        let raw: Raw = toml::from_str(text).map_err(|e| Error::Syntax(e.message().to_owned()))?;
        if raw.format != FORMAT {
            return Err(Error::Format(raw.format));
        }
        let invalid = |m: String| Err(Error::Invalid(m));
        if !valid_bundle_id(&raw.id) {
            return invalid(format!("not a reverse-DNS id: {}", raw.id));
        }
        if raw.name.trim().is_empty() || raw.name.contains(['\n', '\r', '/']) {
            return invalid("the name is empty or has a newline or a slash".to_owned());
        }
        if raw.version.trim().is_empty() || raw.version.contains(['\n', '\r']) {
            return invalid("the version is empty".to_owned());
        }
        // One line each: they become desktop entry values.
        let line = |v: Option<String>| v.filter(|t| !t.contains(['\n', '\r'])).unwrap_or_default();
        let generic_name = line(raw.generic_name);
        let comment = line(raw.comment);
        let Some(runtime) = Runtime::parse(&raw.runtime) else {
            return invalid(format!("not a runtime: {}", raw.runtime));
        };
        if raw.kind == AppKind::Flutter && !runtime.is_platform() {
            return invalid(format!(
                "a flutter app runs on {PLATFORM}, not on {runtime}"
            ));
        }
        let exec = match (raw.kind, raw.exec) {
            (_, Some(exec)) => exec,
            (AppKind::Flutter, None) => "app".to_owned(),
            (AppKind::Native, None) => return invalid("a native app needs exec".to_owned()),
        };
        if !inside(&exec) {
            return invalid(format!("exec is not a path inside the bundle: {exec}"));
        }
        if raw.developer.name.trim().is_empty() || raw.developer.key.trim().is_empty() {
            return invalid("developer.name and developer.key are required".to_owned());
        }
        if let Some(bad) = raw.types.keys().find(|m| !valid_mime(m)) {
            return invalid(format!("not a MIME type: {bad}"));
        }
        if let Some(bad) = raw.url_schemes.iter().find(|s| !valid_scheme(s)) {
            return invalid(format!("not a URL scheme: {bad}"));
        }
        if let Some(bad) = raw
            .permissions
            .filesystem
            .iter()
            .find(|f| !valid_filesystem(f))
        {
            return invalid(format!("not a filesystem permission: {bad}"));
        }
        if let Some(bad) = raw.permissions.talk.iter().find(|n| !valid_bus_name(n)) {
            return invalid(format!("not a bus name: {bad}"));
        }
        let documents = raw
            .documents
            .into_iter()
            .filter(|(m, n)| {
                (valid_mime(m) || m.ends_with("/*")) && crate::desktop::valid_document_name(n)
            })
            .collect();
        if let Some(feed) = &raw.update.feed
            && !feed.starts_with("https://")
        {
            return invalid("update.feed must be an https URL".to_owned());
        }
        let names = raw
            .extra
            .iter()
            .filter_map(|(key, value)| {
                let lang = key.strip_prefix("name.")?;
                let valid_lang = !lang.is_empty()
                    && lang
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '@'));
                let name = value.as_str()?;
                (valid_lang && !name.trim().is_empty() && !name.contains(['\n', '\r']))
                    .then(|| (lang.to_owned(), name.to_owned()))
            })
            .collect();
        Ok(Info {
            id: raw.id,
            name: raw.name,
            names,
            version: raw.version,
            build: raw.build,
            minimum_silica: raw.minimum_silica,
            kind: raw.kind,
            runtime,
            exec,
            args: raw.args,
            categories: raw.categories,
            keywords: raw.keywords,
            url_schemes: raw.url_schemes,
            generic_name,
            comment,
            share: raw.share,
            types: raw.types,
            documents,
            permissions: raw.permissions,
            update: raw.update,
            developer: raw.developer,
        })
    }

    /// Reads `<bundle>/Contents/Info.toml`.
    pub fn read(bundle: &Path) -> Result<Info, Error> {
        let path = bundle.join("Contents/Info.toml");
        let meta = std::fs::metadata(&path).map_err(|e| Error::Io(e.to_string()))?;
        if !meta.is_file() {
            return Err(Error::Io("not a file".to_owned()));
        }
        if meta.len() > MAX_INFO {
            return Err(Error::TooLarge);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| Error::Io(e.to_string()))?;
        Info::parse(&text)
    }

    /// The MIME types it opens, editors first.
    pub fn mime_types(&self) -> Vec<String> {
        let mut types: Vec<(&Role, &String)> = self.types.iter().map(|(m, r)| (r, m)).collect();
        types.sort();
        types
            .into_iter()
            .map(|(_, m)| m.clone())
            .chain(
                self.url_schemes
                    .iter()
                    .map(|s| format!("x-scheme-handler/{s}")),
            )
            .collect()
    }

    /// The bundle's icon folder.
    pub fn icon_dir(bundle: &Path) -> PathBuf {
        bundle.join("Contents/Resources/icon")
    }

    /// The name in `lang` (`it_IT` falls back to `it`).
    pub fn name_in(&self, lang: &str) -> &str {
        crate::desktop::localized(&self.names, lang).map_or(self.name.as_str(), String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const HELLO: &str = r#"
format = 1
id = "dev.example.Hello"
name = "Hello"
"name.it" = "Ciao"
version = "1.2.0"
build = 42
minimum_silica = "0.2.0"
kind = "native"
runtime = "org.freedesktop.Platform//24.08"
exec = "bin/hello"
args = ["--fast"]
categories = ["Utility"]
url_schemes = ["hello"]
future_key = "ignored"

[types]
"text/x-hello" = "editor"
"image/png" = "viewer"

[documents]
"text/x-hello" = "hello"
"text/*" = "../bad"

[permissions]
network = true
filesystem = ["xdg-documents:ro", "/media/data"]
talk = ["org.example.Service"]

[update]
feed = "https://example.org/feed.toml"

[developer]
name = "Example"
key = "RWQabc"
"#;

    #[test]
    fn parses_version_1() {
        let info = Info::parse(HELLO).unwrap();
        assert_eq!(info.id, "dev.example.Hello");
        assert_eq!(info.name_in("it_IT"), "Ciao");
        assert_eq!(info.name_in("de"), "Hello");
        assert_eq!(info.kind, AppKind::Native);
        assert_eq!(info.runtime.to_string(), "org.freedesktop.Platform//24.08");
        assert_eq!(info.exec, "bin/hello");
        assert!(info.permissions.network);
        assert_eq!(info.permissions.devices, Devices::Gpu);
        assert_eq!(
            info.mime_types(),
            ["text/x-hello", "image/png", "x-scheme-handler/hello"]
        );
        assert_eq!(info.documents.len(), 1);
        assert_eq!(
            info.update.feed.as_deref(),
            Some("https://example.org/feed.toml")
        );
    }

    #[test]
    fn the_desktop_keys_of_silica_apps() {
        let info = Info::parse(&HELLO.replace(
            "future_key = \"ignored\"\n",
            "generic_name = \"Greeter\"\ncomment = \"Says hello\"\nshare = true\n",
        ))
        .unwrap();
        assert_eq!(
            (
                info.generic_name.as_str(),
                info.comment.as_str(),
                info.share
            ),
            ("Greeter", "Says hello", true)
        );
        // Optional, and one line each.
        let plain = Info::parse(HELLO).unwrap();
        assert_eq!((plain.generic_name.as_str(), plain.share), ("", false));
        let two =
            Info::parse(&HELLO.replace("future_key = \"ignored\"\n", "comment = \"one\\ntwo\"\n"))
                .unwrap();
        assert_eq!(two.comment, "");
    }

    #[test]
    fn flutter_defaults_and_rules() {
        let flutter = HELLO
            .replace("kind = \"native\"", "kind = \"flutter\"")
            .replace("exec = \"bin/hello\"\n", "");
        // A Flutter app needs the Silica platform.
        assert!(matches!(Info::parse(&flutter), Err(Error::Invalid(_))));
        let flutter = flutter.replace("org.freedesktop.Platform//24.08", "dev.silica.Platform//1");
        let info = Info::parse(&flutter).unwrap();
        assert_eq!(info.exec, "app");
        assert!(info.runtime.is_platform());
    }

    #[test]
    fn refuses_what_breaks_the_rules() {
        let bad = |from: &str, to: &str| Info::parse(&HELLO.replace(from, to));
        assert_eq!(bad("format = 1", "format = 2"), Err(Error::Format(2)));
        assert!(matches!(
            bad("dev.example.Hello", "hello"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            bad("bin/hello", "../hello"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            bad("bin/hello", "/bin/sh"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            bad("\"/media/data\"", "\"/etc/shadow\""),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            bad("\"/media/data\"", "\"/media/../etc\""),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            bad("https://example", "http://example"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            bad("runtime = \"org", "runtime = \"//org"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            bad("build = 42", "build = \"x\""),
            Err(Error::Syntax(_))
        ));
        assert!(matches!(
            bad("key = \"RWQabc\"", "key = \"\""),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            Info::parse(&"x".repeat(70_000)),
            Err(Error::TooLarge)
        ));
    }

    #[test]
    fn filesystem_items() {
        assert!(valid_filesystem("home"));
        assert!(valid_filesystem("xdg-download:ro"));
        assert!(valid_filesystem("xdg-config/app"));
        assert!(!valid_filesystem("xdg-config/../x"));
        assert!(!valid_filesystem("/"));
        assert!(!valid_filesystem("/run/user"));
        assert!(!valid_filesystem("everything"));
    }
}
