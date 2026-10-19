//! File and folder icons composed at the exact pixel size (docs/thumbnails.md).
//!
//! The layers come from `silica.assets` (silica-icons `file-generic` and `folder-generic`),
//! the rules from the same file (`crates/silica-assets/data/compose.toml`). [`Icons::icon`]
//! resolves what a path is, builds the [`rules::Plan`] and renders it, or finds it already
//! rendered: the cache file is named after the MD5 of the plan, so every icon with the same
//! look at the same size is one file and nothing needs invalidating.
//!
//! A file has no symbol of its own type: its glyph is the icon of the app that opens it (or that
//! app's document foreground for the type), else the system type's foreground, else the
//! `unknown` glyph (a circled question mark), always with the extension label when it has one. The previews of the apps' thumbnailers are
//! [`Icons::thumbnailer_preview`].

pub mod appicon;
pub mod apps;
pub mod compose;
pub mod folder;
pub mod mime;
pub mod rules;
pub mod thumbnailer;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use silica_assets::{Assets, EntryId, KeySpace};

use self::appicon::{Appearance, Look};
use self::apps::{AppIcon, Apps, Dirs, DocumentIcon};
use self::folder::Tint;
use self::mime::MimeDb;
use self::rules::{FileLook, FileRules, FolderLook, FolderRules, Glyph};
use self::thumbnailer::Thumbnailers;
use silica_assets::appicon::AppIcon as AppIconAssets;

/// A document foreground bigger than this is not read.
const MAX_DOCUMENT_SVG: u64 = 2 * 1024 * 1024;

/// An `icon.assets` bigger than this is not read.
const MAX_ICON_ASSETS: u64 = 64 * 1024 * 1024;

/// How long the desktop entries are kept before they are read again.
const APPS_TTL: Duration = Duration::from_secs(60);

/// The path of the compiled assets: `SILICA_ASSETS`, else the system file.
pub fn assets_path(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    env("SILICA_ASSETS")
        .filter(|v| v.starts_with('/'))
        .map_or_else(
            || PathBuf::from("/usr/share/silica/silica.assets"),
            PathBuf::from,
        )
}

/// `$XDG_CACHE_HOME/silica/icons`, or `$HOME/.cache/silica/icons`.
pub fn cache_dir(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    let base = env("XDG_CACHE_HOME")
        .filter(|v| v.starts_with('/'))
        .unwrap_or_else(|| format!("{}/.cache", env("HOME").unwrap_or_default()));
    Path::new(&base).join("silica/icons")
}

/// Everything the composition needs, loaded once.
pub struct Icons {
    assets: Assets,
    file_entry: EntryId,
    folder_entry: EntryId,
    file_rules: FileRules,
    folder_rules: FolderRules,
    mime: MimeDb,
    apps: Mutex<(Instant, Arc<Apps>)>,
    thumbnailers: Mutex<(Instant, Arc<Thumbnailers>)>,
    /// Whether the `icon.assets` named by a source string can be composed.
    usable: Mutex<HashMap<String, bool>>,
    dirs: Dirs,
    home: PathBuf,
    user_dirs: Vec<(String, PathBuf)>,
    cache: PathBuf,
}

impl Icons {
    /// Loads the assets and the system databases; None without a usable `silica.assets`.
    pub fn load(env: &dyn Fn(&str) -> Option<String>) -> Option<Icons> {
        let assets = Assets::open(assets_path(env)).ok()?;
        Self::with_assets(assets, env, cache_dir(env))
    }

    pub fn with_assets(
        assets: Assets,
        env: &dyn Fn(&str) -> Option<String>,
        cache: PathBuf,
    ) -> Option<Icons> {
        let file_entry = assets.lookup(KeySpace::Type, "file-generic")?;
        let folder_entry = assets.lookup(KeySpace::Role, "folder.generic")?;
        let file_rules = FileRules::from_assets(&assets)?;
        let folder_rules = FolderRules::from_assets(&assets)?;
        let dirs = Dirs::from_env(env);
        let home = env("HOME").unwrap_or_default();
        let user_dirs = fs::read_to_string(dirs.config_home.join("user-dirs.dirs"))
            .map(|t| folder::user_dirs(&t, &home))
            .unwrap_or_default();
        let mime = MimeDb::load(&mime::data_dirs(env));
        let apps = Arc::new(Apps::load(dirs.clone()));
        Some(Icons {
            assets,
            file_entry,
            folder_entry,
            file_rules,
            folder_rules,
            mime,
            apps: Mutex::new((Instant::now(), apps)),
            thumbnailers: Mutex::new((Instant::now(), Arc::new(Thumbnailers::load(&dirs)))),
            usable: Mutex::new(HashMap::new()),
            dirs,
            home: PathBuf::from(home),
            user_dirs,
            cache,
        })
    }

    fn apps(&self) -> Arc<Apps> {
        // The live registry of the Apps1 service is always current: no time-to-live.
        if crate::apps::live(&self.dirs).is_some() {
            return Arc::new(Apps::load(self.dirs.clone()));
        }
        let Ok(mut guard) = self.apps.lock() else {
            return Arc::new(Apps::load(self.dirs.clone()));
        };
        if guard.0.elapsed() > APPS_TTL {
            *guard = (Instant::now(), Arc::new(Apps::load(self.dirs.clone())));
        }
        Arc::clone(&guard.1)
    }

    fn thumbnailers(&self) -> Arc<Thumbnailers> {
        let Ok(mut guard) = self.thumbnailers.lock() else {
            return Arc::new(Thumbnailers::load(&self.dirs));
        };
        if guard.0.elapsed() > APPS_TTL {
            *guard = (Instant::now(), Arc::new(Thumbnailers::load(&self.dirs)));
        }
        Arc::clone(&guard.1)
    }

    /// The MIME type of a file: by name, else by its first bytes.
    fn mime_of(&self, path: &Path) -> Option<String> {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        self.mime
            .for_name(name)
            .map(str::to_owned)
            .or_else(|| mime::sniff(&mime::read_head(path)).map(str::to_owned))
    }

    /// The preview a freedesktop thumbnailer of an app draws for `path`, `edge` pixels at most
    /// (bounded in time, see [`thumbnailer`]). None when no installed thumbnailer takes its type.
    pub fn thumbnailer_preview(&self, path: &Path, edge: u32) -> Option<image::DynamicImage> {
        let chain = self.mime.chain(&self.mime_of(path)?);
        let all = self.thumbnailers();
        let path_var = std::env::var("PATH").unwrap_or_default();
        all.find(&chain)
            .into_iter()
            .find_map(|t| thumbnailer::run(t, path, edge, thumbnailer::TIMEOUT, &path_var))
    }

    // ------------------------------------------------------------------ what a path looks like

    /// The look of the folder at `path`.
    pub fn folder_look(&self, path: &Path) -> FolderLook {
        let rules = &self.folder_rules;
        let labels = |name: &str| rules.label_color(name).is_some();
        let style = folder::read_style(path, &labels);
        let tint = style.tint.and_then(|t| match t {
            Tint::Hex(c) => Some(c),
            Tint::Label(name) => rules.label_color(&name),
        });
        let exists = |name: &str| {
            self.assets
                .lookup(KeySpace::Symbol, &format!("regular/{name}"))
                .is_some()
        };
        let special = folder::special_key(
            path,
            &self.home,
            &self.user_dirs,
            &self.dirs.data_home,
            &self.dirs.data_dirs,
        )
        .and_then(|key| rules.special(&key).map(str::to_owned));
        // A custom symbol replaces the special one, unless the set does not have it.
        let symbol = style.symbol.filter(|s| exists(s)).or(special);
        FolderLook {
            non_empty: folder::has_items(path),
            symbol,
            tint,
            style: None,
        }
    }

    /// The look of the file at `path`. The glyph is, in order of precedence (after a real
    /// preview or a thumbnailer, which are not icons): the full icon the default app ships for
    /// its type, the document foreground of that app, the icon of that app, the foreground of
    /// the system type (executable, archive...), or the `unknown` glyph.
    pub fn file_look(&self, path: &Path, _pt: f32) -> FileLook {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = mime::extension_of(name);
        let chain = self
            .mime_of(path)
            .map(|m| self.mime.chain(&m))
            .unwrap_or_default();
        let apps = self.apps();
        let glyph = apps
            .handler_for(&chain)
            .and_then(|id| match apps.document_icon(id, &chain)? {
                DocumentIcon::Full { layers, label } => self.full_glyph(&layers, label),
                DocumentIcon::Foreground(svg) => self.document_source(&svg).map(Glyph::Document),
            })
            .or_else(|| {
                apps.icon_for(&chain)
                    .and_then(|icon| self.app_source(&icon))
                    .map(Glyph::App)
            })
            .or_else(|| {
                let name = self.file_rules.system_type(extension.as_deref(), &chain)?;
                compose::system_svg(&self.assets, name).map(|_| Glyph::System(name.to_owned()))
            })
            .or_else(|| {
                compose::system_svg(&self.assets, "unknown")
                    .map(|_| Glyph::System("unknown".to_owned()))
            })
            .unwrap_or(Glyph::None);
        FileLook { glyph, extension }
    }

    /// A full icon: every layer within the size limit and drawable, else none.
    fn full_glyph(&self, layers: &[PathBuf], label: bool) -> Option<Glyph> {
        let sources = layers
            .iter()
            .map(|svg| {
                let source = self.document_source(svg)?;
                self.drawable(&source, svg).then_some(source)
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Glyph::Full { sources, label })
    }

    /// Whether the SVG at `path` parses (remembered by its `source` name).
    fn drawable(&self, source: &str, path: &Path) -> bool {
        let key = format!("svg:{source}");
        if let Some(known) = self.usable.lock().ok().and_then(|m| m.get(&key).copied()) {
            return known;
        }
        let ok = fs::read(path).ok().is_some_and(|bytes| {
            resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default()).is_ok()
        });
        if let Ok(mut memo) = self.usable.lock() {
            if memo.len() >= 512 {
                memo.clear();
            }
            memo.insert(key, ok);
        }
        ok
    }

    /// A stable name for a document foreground: where it is and when it changed.
    fn document_source(&self, svg: &Path) -> Option<String> {
        let meta = fs::metadata(svg).ok()?;
        if meta.len() > MAX_DOCUMENT_SVG {
            return None;
        }
        Some(format!("doc:{}", stamp(svg, &meta)?))
    }

    /// A stable name for an app icon: where it is and when it changed. An icon folder counts
    /// with its `icon.assets` when that can be composed, else with its `icon.png`.
    fn app_source(&self, icon: &AppIcon) -> Option<String> {
        match icon {
            AppIcon::Folder(dir) => self
                .composable_source(&dir.join("icon.assets"))
                .or_else(|| png_source(&dir.join("icon.png"))),
            AppIcon::File(path) => png_source(path),
            AppIcon::Assets(name) => {
                self.assets.lookup(KeySpace::App, name)?;
                Some(format!("assets:{name}"))
            }
        }
    }

    /// `icon:<path>@<mtime>:<size>` when `file` is an `icon.assets` that decodes.
    fn composable_source(&self, file: &Path) -> Option<String> {
        let meta = fs::metadata(file).ok()?;
        if !meta.is_file() || meta.len() > MAX_ICON_ASSETS {
            return None;
        }
        let key = format!("icon:{}", stamp(file, &meta)?);
        let known = self.usable.lock().ok().and_then(|m| m.get(&key).copied());
        let ok = known.unwrap_or_else(|| {
            let ok = fs::read(file)
                .ok()
                .is_some_and(|bytes| AppIconAssets::decode(&bytes).is_some());
            if let Ok(mut memo) = self.usable.lock() {
                if memo.len() >= 512 {
                    memo.clear();
                }
                memo.insert(key.clone(), ok);
            }
            ok
        });
        ok.then_some(key)
    }

    // ------------------------------------------------------------------------------- render

    /// The path of the PNG of the icon of `path`, rendered at `size_px` pixels (`pt` points
    /// wide: the symbol weight and the label depend on it), or found in the cache.
    pub fn icon(&self, path: &Path, size_px: u32, pt: f32) -> Option<PathBuf> {
        self.icon_styled(path, size_px, pt, None)
    }

    /// [`Icons::icon`] with the icon style of the system (Settings > Appearance): folders are
    /// drawn in it (dark, clear, tinted; docs/thumbnails.md), files do not change.
    pub fn icon_styled(
        &self,
        path: &Path,
        size_px: u32,
        pt: f32,
        style: Option<rules::StyleSel>,
    ) -> Option<PathBuf> {
        let meta = fs::metadata(path).ok()?;
        let fonts = compose::Fonts::from_assets(&self.assets);
        let (entry, plan) = if meta.is_dir() {
            let mut look = self.folder_look(path);
            look.style = style;
            (
                self.folder_entry,
                rules::plan_folder(&self.folder_rules, size_px, pt, &look),
            )
        } else {
            let look = self.file_look(path, pt);
            let plan = rules::plan_file(&self.file_rules, size_px, pt, &look, |t, s, w| {
                fonts.advance(t, s, w)
            });
            (self.file_entry, plan)
        };
        let key = format!("{}|{plan:?}", self.assets.content_version());
        let target = self
            .cache
            .join(size_px.to_string())
            .join(format!("{:x}.png", md5::compute(key.as_bytes())));
        if target.is_file() {
            return Some(target);
        }
        let app = plan.app.as_ref().and_then(|draw| {
            let app_pt = pt * draw.rect[2] / size_px as f32;
            self.load_app(&draw.source, draw.rect[2], app_pt)
        });
        let document = plan
            .document
            .as_ref()
            .and_then(|draw| self.load_document(&draw.source));
        let full: Vec<Vec<u8>> = plan
            .full
            .iter()
            .filter_map(|draw| load_document(&draw.source))
            .collect();
        let pixmap = compose::render(
            &self.assets,
            entry,
            &plan,
            app.as_ref(),
            document.as_deref(),
            &full,
            &fonts,
        )?;
        write_icon(
            &target,
            pixmap.width(),
            pixmap.height(),
            &compose::straight_rgba(&pixmap),
        )?;
        Some(target)
    }

    /// The SVG a document source names: an app's file (`doc:`) or a system foreground
    /// (`system:`).
    fn load_document(&self, source: &str) -> Option<Vec<u8>> {
        match source.strip_prefix("system:") {
            Some(name) => compose::system_svg(&self.assets, name),
            None => load_document(source),
        }
    }

    /// Decodes the app icon a plan names, `side` pixels wide (`pt` points).
    fn load_app(&self, source: &str, side: f32, pt: f32) -> Option<compose::Bitmap> {
        if let Some(rest) = source.strip_prefix("assets:") {
            return compose::assets_app_icon(&self.assets, rest, side.ceil() as u32);
        }
        if let Some(rest) = source.strip_prefix("icon:") {
            // The same composition as `AppIcon` (docs/thumbnails.md), light appearance.
            let (file, _) = rest.rsplit_once('@')?;
            let bytes = fs::read(file).ok()?;
            let icon = AppIconAssets::decode(&bytes)?;
            let px = side.round().max(1.0) as u32;
            let appearance = Appearance {
                look: Look::Light,
                dark: false,
                bare: false,
            };
            let rgba = appicon::compose(&icon, px, pt, appearance, appicon::DEFAULT_ACCENT)?;
            return Some(compose::Bitmap {
                width: px,
                height: px,
                rgba,
            });
        }
        let path = source.strip_prefix("file:")?;
        let (path, _) = path.rsplit_once('@')?;
        compose::file_app_icon(Path::new(path))
    }
}

/// The SVG a `doc:` source names.
fn load_document(source: &str) -> Option<Vec<u8>> {
    let (path, _) = source.strip_prefix("doc:")?.rsplit_once('@')?;
    let meta = fs::metadata(path).ok()?;
    (meta.len() <= MAX_DOCUMENT_SVG).then_some(())?;
    fs::read(path).ok()
}

/// `<path>@<mtime>:<size>`.
fn stamp(path: &Path, meta: &fs::Metadata) -> Option<String> {
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("{}@{mtime}:{}", path.display(), meta.len()))
}

fn png_source(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    meta.is_file().then_some(())?;
    Some(format!("file:{}", stamp(path, &meta)?))
}

pub(crate) fn write_icon(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Option<()> {
    use std::os::unix::fs::DirBuilderExt;
    let dir = path.parent()?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .ok()?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        std::process::id(),
        path.file_name()?.to_string_lossy()
    ));
    let result = (|| {
        let mut encoder =
            png::Encoder::new(BufWriter::new(File::create(&tmp).ok()?), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
        writer.finish().ok()?;
        fs::rename(&tmp, path).ok()
    })();
    if result.is_none() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests;
