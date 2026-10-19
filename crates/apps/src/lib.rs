//! Silica's app registry, shared by the daemon (`Apps1`), Ossido (`app.launch`) and
//! `silica-launch`: desktop entries, app bundles (`Info.toml`), AppImages, the `mimeapps.list`
//! associations and the launch command lines. See `docs/spec/launch-services.md` and silica-os
//! `docs/app-bundles.md`.

pub mod appimage;
pub mod db;
pub mod desktop;
pub mod exec;
pub mod info;
pub mod mimeapps;
pub mod registry;

pub use desktop::DesktopEntry;
pub use info::Info;
pub use registry::{App, Dirs, Kind, Location, Registry};

/// Whether `id` is a valid app id: the characters of a desktop file id (letters, digits, `.`,
/// `_`, `-`, `+`), not empty, not starting with a dot, at most 255 bytes.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 255
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids() {
        assert!(super::valid_id("dev.frascella.Calculator"));
        assert!(super::valid_id("org.gnome.Nautilus"));
        assert!(!super::valid_id(""));
        assert!(!super::valid_id("../etc/passwd"));
        assert!(!super::valid_id(".hidden"));
        assert!(!super::valid_id("a b"));
    }
}
