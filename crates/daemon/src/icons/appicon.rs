//! The app icon service: finds the `icon.assets` of an app and composes (and caches) its icons
//! with [`silica_assets::compose`] (docs/thumbnails.md, "App icons").

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use silica_assets::appicon::AppIcon;
pub use silica_assets::compose::{Appearance, DEFAULT_ACCENT, Look, MAX_SIDE, compose};

use super::apps::{Apps, Dirs};

/// An `icon.assets` bigger than this is not read.
const MAX_FILE: u64 = 64 * 1024 * 1024;
const APPS_TTL: Duration = Duration::from_secs(60);

// ------------------------------------------------------------------------------- the service

/// Finds the `icon.assets` of an app and composes (and caches) its icons.
pub struct AppIcons {
    cache: PathBuf,
    dirs: Dirs,
    apps: Mutex<Option<(Instant, Arc<Apps>)>>,
}

impl AppIcons {
    pub fn new(cache: PathBuf, dirs: Dirs) -> AppIcons {
        AppIcons {
            cache,
            dirs,
            apps: Mutex::new(None),
        }
    }

    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> AppIcons {
        AppIcons::new(super::cache_dir(env).join("app"), Dirs::from_env(env))
    }

    fn apps(&self) -> Option<Arc<Apps>> {
        // The live registry of the Apps1 service is always current: no time-to-live.
        if crate::apps::live(&self.dirs).is_some() {
            return Some(Arc::new(Apps::load(self.dirs.clone())));
        }
        let mut guard = self.apps.lock().ok()?;
        if let Some((at, apps)) = guard.as_ref()
            && at.elapsed() < APPS_TTL
        {
            return Some(Arc::clone(apps));
        }
        let apps = Arc::new(Apps::load(self.dirs.clone()));
        *guard = Some((Instant::now(), Arc::clone(&apps)));
        Some(apps)
    }

    /// The icon folder of `app`: an absolute folder as it is, else the `X-Silica-Icon` of the
    /// desktop entry with that id (with or without `.desktop`).
    pub fn folder(&self, app: &str) -> Option<PathBuf> {
        if app.starts_with('/') {
            return Some(PathBuf::from(app));
        }
        let id = if app.ends_with(".desktop") {
            app.to_owned()
        } else {
            format!("{app}.desktop")
        };
        self.apps()?.silica_icon_dir(&id)
    }

    /// The path of the PNG of the icon of `app` at `size_px` pixels on a display of `scale`,
    /// composed on a miss. None when the app has no `icon.assets` or it cannot be composed (the
    /// client keeps the bundled `icon.png`).
    pub fn icon(
        &self,
        app: &str,
        size_px: u32,
        scale: f64,
        appearance: u32,
        accent_rgb: u32,
    ) -> Option<PathBuf> {
        let ap = Appearance::from_u32(appearance)?;
        if size_px == 0 || size_px > MAX_SIDE {
            return None;
        }
        let file = self.folder(app)?.join("icon.assets");
        if fs::metadata(&file).ok()?.len() > MAX_FILE {
            return None;
        }
        let bytes = fs::read(&file).ok()?;
        let digest = AppIcon::digest(&bytes)?;
        let pt = if scale.is_finite() && scale >= 0.25 {
            size_px as f32 / scale as f32
        } else {
            size_px as f32
        };
        let accent = if accent_rgb == 0 {
            DEFAULT_ACCENT
        } else {
            accent_rgb & 0xff_ff_ff
        };
        let accent_key = if ap.look == Look::Tinted {
            format!("-{accent:06x}")
        } else {
            String::new()
        };
        // min_size depends on the size in points.
        let target = self.cache.join(format!(
            "{digest:016x}-{}-{size_px}-{}{accent_key}.png",
            ap.name(),
            (pt * 10.0).round() as u32
        ));
        if target.is_file() {
            return Some(target);
        }
        let icon = AppIcon::decode(&bytes)?;
        let rgba = compose(&icon, size_px, pt, ap, accent)?;
        super::write_icon(&target, size_px, size_px, &rgba)?;
        Some(target)
    }
}
