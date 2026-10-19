//! The app that opens a type, and its Silica icon.
//!
//! The default handler is the registry's (`silica_apps`, docs/spec/launch-services.md):
//! `[Default Applications]` of the `mimeapps.list` files, then `[Added Associations]`, then the
//! apps that declare the type. Only an app with a Silica icon gives
//! one: `X-Silica-Icon=` (an absolute icon folder, whose `icon.assets` is composed, or a plain
//! name of `silica.assets`), else an absolute `Icon=` PNG. Any other handler gives none and the
//! file is an unknown extension. An app may also draw its own documents: `X-Silica-Document-Icons=`
//! maps MIME types to `documents/<name>.svg` in its icon folder (docs/thumbnails.md).

use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use silica_apps::desktop::valid_document_name;
pub use silica_apps::registry::Dirs;
use silica_apps::registry::{App, Kind, Registry};

/// Where an app icon comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppIcon {
    /// An icon folder (`icon.assets`, else `icon.png`).
    Folder(PathBuf),
    /// A PNG file.
    File(PathBuf),
    /// An app icon of `silica.assets`.
    Assets(String),
}

/// The Silica icon an app declares, if any.
pub fn icon_of(app: &App) -> Option<AppIcon> {
    let value = &app.silica_icon;
    if !value.is_empty() {
        return Some(if value.starts_with('/') {
            AppIcon::Folder(PathBuf::from(value))
        } else {
            AppIcon::Assets(value.clone())
        });
    }
    Some(&app.icon)
        .filter(|i| i.starts_with('/') && i.ends_with(".png"))
        .map(|i| AppIcon::File(PathBuf::from(i)))
}

/// The apps of the session, for the icons: the live registry of the `Apps1` service when it
/// runs in this process, else a scan of the same folders.
pub struct Apps {
    registry: Arc<Registry>,
}

impl Apps {
    pub fn load(dirs: Dirs) -> Apps {
        let registry = crate::apps::live(&dirs).unwrap_or_else(|| Arc::new(Registry::scan(dirs)));
        Apps { registry }
    }

    pub fn dirs(&self) -> &Dirs {
        self.registry.dirs()
    }

    fn app(&self, id: &str) -> Option<&App> {
        self.registry.get(id)
    }

    /// The absolute icon folder (`X-Silica-Icon=`, a bundle's `Resources/icon`) of the app
    /// `id` (with or without `.desktop`).
    pub fn silica_icon_dir(&self, id: &str) -> Option<PathBuf> {
        Some(&self.app(id)?.silica_icon)
            .filter(|v| v.starts_with('/'))
            .map(PathBuf::from)
    }

    /// The id of the app that opens `mime`.
    pub fn handler(&self, mime: &str) -> Option<&str> {
        self.handler_for(&[mime.to_owned()])
    }

    /// The id of the app that opens the first type of `chain` (the type and its parents) that
    /// has a handler.
    pub fn handler_for(&self, chain: &[String]) -> Option<&str> {
        self.registry.default_for(chain).map(|a| a.id.as_str())
    }

    /// The Silica icon of the app that opens `mime`: the first type of `chain` (the type and
    /// its parents) that has a handler decides, and the handler needs a Silica icon.
    pub fn icon_for(&self, chain: &[String]) -> Option<AppIcon> {
        icon_of(self.registry.default_for(chain)?)
    }

    /// The document icon the app `id` declares for the first type of `chain` it lists (exact
    /// type, then `major/*`), if what it names is there: see [`DocumentIcon`]. A bundle keeps
    /// its document icons in `Contents/Resources/documents`, a desktop entry in the
    /// `documents/` of its icon folder.
    pub fn document_icon(&self, id: &str, chain: &[String]) -> Option<DocumentIcon> {
        let app = self.app(id)?;
        let documents = match app.kind {
            Kind::Bundle | Kind::System if app.desktop_file.is_none() => {
                app.path.join("Contents/Resources/documents")
            }
            _ => self.silica_icon_dir(id)?.join("documents"),
        };
        let find = |key: &str| {
            app.document_icons
                .iter()
                .find(|(m, _)| m == key)
                .map(|(_, n)| n.as_str())
        };
        let name = chain.iter().find_map(|mime| {
            find(mime).or_else(|| {
                let (major, _) = mime.split_once('/')?;
                find(&format!("{major}/*"))
            })
        })?;
        document_icon_at(&documents, name)
    }
}

/// What an app ships under `<icon folder>/documents/` for a document icon name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentIcon {
    /// A foreground drawn in the glyph box of the system sheet: `<name>.svg`, or
    /// `<name>/foreground.svg`.
    Foreground(PathBuf),
    /// A whole icon that replaces the sheet: `<name>/icon.svg`, or `<name>/back.svg` under
    /// `<name>/front.svg` (bottom first). `label`: the folder holds a file named `label`, which
    /// asks for the extension label to be drawn on the icon (it is not otherwise).
    Full { layers: Vec<PathBuf>, label: bool },
}

/// Resolves `name` in `documents`: a folder wins over a plain SVG, and in a folder a full icon
/// (`icon.svg`, then `back.svg` + `front.svg`) over `foreground.svg`.
fn document_icon_at(documents: &Path, name: &str) -> Option<DocumentIcon> {
    let folder = documents.join(name);
    if folder.is_dir() {
        let file = |n: &str| Some(folder.join(n)).filter(|p| p.is_file());
        let full = file("icon.svg").map(|icon| vec![icon]).or_else(|| {
            let (back, front) = (file("back.svg")?, file("front.svg")?);
            Some(vec![back, front])
        });
        return match full {
            Some(layers) => Some(DocumentIcon::Full {
                layers,
                label: folder.join("label").is_file(),
            }),
            None => file("foreground.svg").map(DocumentIcon::Foreground),
        };
    }
    let path = documents.join(format!("{name}.svg"));
    path.is_file().then_some(DocumentIcon::Foreground(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry(text: &str) -> App {
        silica_apps::registry::entry_app(
            "p",
            &silica_apps::desktop::parse(text),
            Path::new("/p.desktop"),
            false,
        )
    }

    #[test]
    fn entries_declare_a_silica_icon_or_not() {
        let e = entry(
            "[Desktop Entry]\nType=Application\nName=N\nExec=x\nName=P\nX-Silica-Icon=/usr/lib64/silica/apps/p/icon\nIcon=/x.png\nMimeType=a/b;c/d;\n[Desktop Action x]\nIcon=no\n",
        );
        assert_eq!(e.mime_types, ["a/b", "c/d"]);
        assert_eq!(
            icon_of(&e),
            Some(AppIcon::Folder(PathBuf::from(
                "/usr/lib64/silica/apps/p/icon"
            )))
        );
        let named = entry(
            "[Desktop Entry]\nType=Application\nName=N\nExec=x\nX-Silica-Icon=mission-control\n",
        );
        assert_eq!(
            icon_of(&named),
            Some(AppIcon::Assets("mission-control".into()))
        );
        let plain = entry("[Desktop Entry]\nType=Application\nName=N\nExec=x\nIcon=/opt/app.png\n");
        assert_eq!(
            icon_of(&plain),
            Some(AppIcon::File(PathBuf::from("/opt/app.png")))
        );
        // A theme icon name is not a Silica icon.
        assert_eq!(
            icon_of(&entry(
                "[Desktop Entry]\nType=Application\nName=N\nExec=x\nIcon=firefox\n"
            )),
            None
        );
        assert_eq!(
            icon_of(&entry(
                "[Desktop Entry]\nType=Application\nName=N\nExec=x\nX-Silica-Icon=\n"
            )),
            None
        );
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apps-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_default_handler_decides_and_needs_a_silica_icon() {
        let root = tmp("handler");
        let apps = root.join("data/applications");
        fs::create_dir_all(&apps).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        let write = |name: &str, text: &str| fs::write(apps.join(name), text).unwrap();
        write(
            "preview.desktop",
            "[Desktop Entry]\nType=Application\nName=N\nExec=x\nX-Silica-Icon=/apps/preview/icon\nMimeType=application/pdf;\n",
        );
        write(
            "evince.desktop",
            "[Desktop Entry]\nType=Application\nName=N\nExec=x\nIcon=evince\nMimeType=application/pdf;\n",
        );
        write(
            "editor.desktop",
            "[Desktop Entry]\nType=Application\nName=N\nExec=x\nX-Silica-Icon=/apps/editor/icon\nMimeType=text/plain;\n",
        );
        write(
            "firefox.desktop",
            "[Desktop Entry]\nType=Application\nName=N\nExec=x\nIcon=firefox\nMimeType=text/html;\n",
        );
        write(
            "mpv.desktop",
            "[Desktop Entry]\nType=Application\nName=N\nExec=x\nX-Silica-Icon=/apps/mpv/icon\nMimeType=video/mp4;\n",
        );
        fs::write(
            root.join("config/mimeapps.list"),
            "[Default Applications]\napplication/pdf=evince.desktop;preview.desktop;\n\
             text/html=gone.desktop;firefox.desktop;\n",
        )
        .unwrap();
        let dirs = Dirs {
            config_home: root.join("config"),
            config_dirs: vec![],
            data_home: root.join("data"),
            data_dirs: vec![],
            ..Dirs::default()
        };
        let apps = Apps::load(dirs);
        let chain = |m: &str| vec![m.to_owned()];
        // The listed default is evince, which has no Silica icon: the file keeps its symbol.
        assert_eq!(apps.handler("application/pdf"), Some("evince"));
        assert_eq!(apps.icon_for(&chain("application/pdf")), None);
        // A missing first id is skipped; Firefox has no Silica icon either.
        assert_eq!(apps.handler("text/html"), Some("firefox"));
        assert_eq!(apps.icon_for(&chain("text/html")), None);
        // Without mimeapps.list the desktop entries decide.
        assert_eq!(
            apps.icon_for(&chain("text/plain")),
            Some(AppIcon::Folder(PathBuf::from("/apps/editor/icon")))
        );
        assert_eq!(
            apps.icon_for(&chain("video/mp4")),
            Some(AppIcon::Folder(PathBuf::from("/apps/mpv/icon")))
        );
        // The parents are tried in order.
        let chain = vec!["text/x-notes".to_owned(), "text/plain".to_owned()];
        assert!(apps.icon_for(&chain).is_some());
        assert_eq!(apps.icon_for(&["image/png".to_owned()]), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_document_icon_folder_is_a_foreground_or_a_full_icon() {
        let root = tmp("docfolders");
        let docs = root.join("documents");
        let touch = |rel: &str| {
            let path = docs.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "<svg/>").unwrap();
        };
        touch("plain.svg");
        touch("fg/foreground.svg");
        touch("one/icon.svg");
        touch("one/foreground.svg");
        touch("two/back.svg");
        touch("two/front.svg");
        touch("lab/icon.svg");
        touch("lab/label");
        touch("half/back.svg");
        touch("both.svg");
        touch("both/foreground.svg");
        let at = |n: &str| document_icon_at(&docs, n);
        assert_eq!(
            at("plain"),
            Some(DocumentIcon::Foreground(docs.join("plain.svg")))
        );
        assert_eq!(
            at("fg"),
            Some(DocumentIcon::Foreground(docs.join("fg/foreground.svg")))
        );
        // A full icon wins over the foreground of the same folder.
        assert_eq!(
            at("one"),
            Some(DocumentIcon::Full {
                layers: vec![docs.join("one/icon.svg")],
                label: false
            })
        );
        assert_eq!(
            at("two"),
            Some(DocumentIcon::Full {
                layers: vec![docs.join("two/back.svg"), docs.join("two/front.svg")],
                label: false
            })
        );
        assert!(matches!(
            at("lab"),
            Some(DocumentIcon::Full { label: true, .. })
        ));
        // Only one of back/front: nothing usable. A folder wins over a plain file.
        assert_eq!(at("half"), None);
        assert_eq!(
            at("both"),
            Some(DocumentIcon::Foreground(docs.join("both/foreground.svg")))
        );
        assert_eq!(at("none"), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn document_icons_are_parsed_and_matched_by_type_then_wildcard() {
        let e = entry(
            "[Desktop Entry]\nType=Application\nName=N\nExec=x\nX-Silica-Document-Icons=application/pdf=pdf;text/*=text;bad=x;text/y=../up;image/png=;\n",
        );
        assert_eq!(
            e.document_icons,
            [
                ("application/pdf".to_owned(), "pdf".to_owned()),
                ("text/*".to_owned(), "text".to_owned())
            ]
        );
        let root = tmp("docicons");
        let apps_dir = root.join("data/applications");
        let icon = root.join("app/icon");
        fs::create_dir_all(&apps_dir).unwrap();
        fs::create_dir_all(icon.join("documents")).unwrap();
        fs::write(icon.join("documents/pdf.svg"), "<svg/>").unwrap();
        fs::write(icon.join("documents/text.svg"), "<svg/>").unwrap();
        fs::write(
            apps_dir.join("p.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=N\nExec=x\nX-Silica-Icon={}\nMimeType=application/pdf;text/plain;\n\
                 X-Silica-Document-Icons=application/pdf=pdf;text/*=text;image/png=missing;\n",
                icon.display()
            ),
        )
        .unwrap();
        let apps = Apps::load(Dirs {
            config_home: root.join("config"),
            config_dirs: vec![],
            data_home: root.join("data"),
            data_dirs: vec![],
            ..Dirs::default()
        });
        let chain = |m: &str| vec![m.to_owned()];
        assert_eq!(
            apps.document_icon("p.desktop", &chain("application/pdf")),
            Some(DocumentIcon::Foreground(icon.join("documents/pdf.svg")))
        );
        // The wildcard matches the types of its group.
        assert_eq!(
            apps.document_icon("p.desktop", &chain("text/markdown")),
            Some(DocumentIcon::Foreground(icon.join("documents/text.svg")))
        );
        // A parent in the chain counts; a name whose file is missing, or no entry, gives none.
        let md = vec!["x/y".to_owned(), "application/pdf".to_owned()];
        assert!(apps.document_icon("p.desktop", &md).is_some());
        assert_eq!(apps.document_icon("p.desktop", &chain("image/png")), None);
        assert_eq!(apps.document_icon("p.desktop", &chain("audio/mp3")), None);
        assert_eq!(
            apps.document_icon("none.desktop", &chain("text/plain")),
            None
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_xdg_dirs_have_defaults() {
        let env = |k: &str| (k == "HOME").then(|| "/h".to_owned());
        let d = Dirs::from_env(&env);
        assert_eq!(d.config_home, PathBuf::from("/h/.config"));
        assert_eq!(d.data_home, PathBuf::from("/h/.local/share"));
        assert_eq!(
            d.data_dirs,
            [
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
                PathBuf::from("/h/.local/share/flatpak/exports/share"),
                PathBuf::from("/var/lib/flatpak/exports/share"),
            ]
        );
        assert_eq!(d.config_dirs, [PathBuf::from("/etc/xdg")]);
    }
}
