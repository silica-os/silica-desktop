//! File and folder icons composed at the exact pixel size (docs/thumbnails.md).
//!
//! The layers come from `silica.assets` (silica-icons `file-generic` and `folder-generic`),
//! the rules from the same file (`crates/silica-assets/data/compose.toml`). [`Icons::icon`]
//! resolves what a path is, builds the [`rules::Plan`] and renders it, or finds it already
//! rendered: the cache file is named after the MD5 of the plan, so every icon with the same
//! look at the same size is one file and nothing needs invalidating.

pub mod appicon;
pub mod apps;
pub mod compose;
pub mod folder;
pub mod mime;
pub mod rules;

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use silica_assets::{Assets, EntryId, KeySpace, SlotKind};

use self::apps::{AppIcon, Apps, Dirs};
use self::folder::Tint;
use self::mime::MimeDb;
use self::rules::{FileLook, FileRules, FolderLook, FolderRules, Glyph};

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
            dirs,
            home: PathBuf::from(home),
            user_dirs,
            cache,
        })
    }

    fn apps(&self) -> Arc<Apps> {
        let Ok(mut guard) = self.apps.lock() else {
            return Arc::new(Apps::load(self.dirs.clone()));
        };
        if guard.0.elapsed() > APPS_TTL {
            *guard = (Instant::now(), Arc::new(Apps::load(self.dirs.clone())));
        }
        Arc::clone(&guard.1)
    }

    /// The symbol and color of a kind of file.
    fn kind_style(&self, kind: &str) -> Option<(String, [u8; 3])> {
        let id = self
            .assets
            .lookup(KeySpace::Type, &format!("kind.{kind}"))?;
        let slot = self
            .assets
            .entry(id)?
            .slots()
            .find(|s| s.kind == SlotKind::KindStyle)?;
        let c = slot.color[0];
        Some((
            slot.font?.to_owned(),
            [(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8],
        ))
    }

    fn category_color(&self, category: &str) -> [u8; 3] {
        let color = self
            .assets
            .lookup(KeySpace::Type, &format!("category.{category}"))
            .and_then(|id| self.assets.entry(id))
            .and_then(|e| {
                e.slots()
                    .find(|s| s.kind == SlotKind::TypeCategory)
                    .map(|s| s.color[0])
            });
        color.map_or([0x63, 0x63, 0x66], |c| {
            [(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8]
        })
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
        }
    }

    /// The look of the file at `path`.
    pub fn file_look(&self, path: &Path, pt: f32) -> FileLook {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = mime::extension_of(name);
        let mime_type = self
            .mime
            .for_name(name)
            .map(str::to_owned)
            .or_else(|| mime::sniff(&mime::read_head(path)).map(str::to_owned));
        let resolved = mime::resolve(
            &self.assets,
            &self.mime,
            mime_type.as_deref(),
            extension.as_deref(),
        );
        let chain = mime_type
            .as_deref()
            .map(|m| self.mime.chain(m))
            .unwrap_or_default();
        let app = self
            .apps()
            .icon_for(&chain)
            .and_then(|icon| self.app_source(&icon, pt));
        let category = self.category_color(&resolved.category);
        let (glyph, label_color) = match (
            app,
            resolved.kind.as_deref().and_then(|k| self.kind_style(k)),
        ) {
            (Some(source), _) => (Glyph::App(source), category),
            (None, Some((glyph, color))) => (Glyph::Symbol(glyph, color), color),
            (None, None) => (Glyph::None, category),
        };
        FileLook {
            glyph,
            extension,
            label_color,
        }
    }

    /// A stable name for an app icon: where it is and when it changed.
    fn app_source(&self, icon: &AppIcon, _pt: f32) -> Option<String> {
        match icon {
            AppIcon::File(path) => {
                let meta = fs::metadata(path).ok()?;
                let mtime = meta
                    .modified()
                    .ok()?
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_secs();
                Some(format!("file:{}@{mtime}:{}", path.display(), meta.len()))
            }
            AppIcon::Assets(name) => {
                self.assets.lookup(KeySpace::App, name)?;
                Some(format!("assets:{name}"))
            }
        }
    }

    // ------------------------------------------------------------------------------- render

    /// The path of the PNG of the icon of `path`, rendered at `size_px` pixels (`pt` points
    /// wide: the symbol weight and the label depend on it), or found in the cache.
    pub fn icon(&self, path: &Path, size_px: u32, pt: f32) -> Option<PathBuf> {
        let meta = fs::metadata(path).ok()?;
        let fonts = compose::Fonts::from_assets(&self.assets);
        let (entry, plan) = if meta.is_dir() {
            let look = self.folder_look(path);
            (
                self.folder_entry,
                rules::plan_folder(&self.folder_rules, size_px, pt, &look),
            )
        } else {
            let mut look = self.file_look(path, pt);
            let mut plan = rules::plan_file(&self.file_rules, size_px, pt, &look, |t, s, w| {
                fonts.advance(t, s, w)
            });
            // No glyph and no label to say what it is: the fallback kind.
            if plan.symbol.is_none()
                && plan.app.is_none()
                && plan.label.is_none()
                && let Some((glyph, color)) = self.kind_style(&self.file_rules.fallback)
            {
                look.glyph = Glyph::Symbol(glyph, color);
                look.label_color = color;
                plan = rules::plan_file(&self.file_rules, size_px, pt, &look, |t, s, w| {
                    fonts.advance(t, s, w)
                });
            }
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
        let app = plan
            .app
            .as_ref()
            .and_then(|draw| self.load_app(&draw.source, draw.rect[2]));
        let pixmap = compose::render(&self.assets, entry, &plan, app.as_ref(), &fonts)?;
        write_icon(
            &target,
            pixmap.width(),
            pixmap.height(),
            &compose::straight_rgba(&pixmap),
        )?;
        Some(target)
    }

    fn load_app(&self, source: &str, side: f32) -> Option<compose::Bitmap> {
        if let Some(rest) = source.strip_prefix("assets:") {
            return compose::assets_app_icon(&self.assets, rest, side.ceil() as u32);
        }
        let path = source.strip_prefix("file:")?;
        let (path, _) = path.rsplit_once('@')?;
        compose::file_app_icon(Path::new(path))
    }
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
