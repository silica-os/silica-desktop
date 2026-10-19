//! Icons of files and folders, composed by silica-daemon (`Thumbnails1.Icon`: the kind or the
//! handling app, folder colours and symbols, special folders) and cached here. A request
//! returns at once with what is cached; the first one for a file asks the daemon, and the
//! views that asked are repainted when the icon arrives. Without the daemon the generic
//! silica-icons file and folder icons stand in.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use skia_safe::{Data, Image};
use vetro_lib::core::{Ui, ViewId};

/// The pixel sizes asked for (a 64 pt fan icon or a 92 pt grid icon at 2×, a pile item).
fn bucket(px: u32) -> u32 {
    match px {
        0..=64 => 64,
        65..=128 => 128,
        _ => 256,
    }
}

#[derive(Default)]
struct Cache {
    icons: HashMap<(PathBuf, u32, bool), Option<Image>>,
    waiting: HashMap<(PathBuf, u32, bool), Vec<ViewId>>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

fn fallback(dir: bool, dark: bool) -> Option<Image> {
    let name = if dir {
        "folder-generic"
    } else {
        "file-generic"
    };
    crate::apps::shared_icon("files", name, "icon.png", dark)
}

/// The icon of `path` at about `px` pixels; `view` is repainted when a better one arrives.
pub fn icon(path: &Path, px: u32, dark: bool, view: Option<ViewId>) -> Option<Image> {
    let key = (path.to_path_buf(), bucket(px), dark);
    let dir = path.is_dir();
    let (cached, ask) = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(i) = c.icons.get(&key) {
            return (Some(i.clone()), false);
        }
        let waiting = c.waiting.entry(key.clone()).or_default();
        let first = waiting.is_empty();
        if let Some(v) = view
            && !waiting.contains(&v)
        {
            waiting.push(v);
        }
        (None, first)
    });
    if let Some(i) = cached {
        return i.or_else(|| fallback(dir, dark));
    }
    if ask {
        request(key);
    }
    fallback(dir, dark)
}

fn request(key: (PathBuf, u32, bool)) {
    let uri = format!("file://{}", key.0.display());
    let size = key.1;
    // Bits 1-3: the icon style (dark is 4).
    let flags: u32 = if key.2 { 4 << 1 } else { 0 };
    let k2 = key.clone();
    let started = vetro_lib::platform::spawn(async move {
        let mut image = None;
        if let Some(s) = vetro_lib::platform::services()
            && let Ok(p) = s.thumbnails().await
            && let Ok((path, _)) = p.icon(&uri, size, 1.0, flags | 1).await
            && !path.is_empty()
            && let Ok(bytes) = std::fs::read(&path)
        {
            image = Image::from_encoded(Data::new_copy(&bytes));
        }
        vetro_lib::platform::post(move |ui| arrived(ui, k2, image));
    });
    if !started {
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            c.waiting.remove(&key);
            c.icons.insert(key, None);
        });
    }
}

fn arrived(ui: &mut Ui, key: (PathBuf, u32, bool), image: Option<Image>) {
    let views = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.icons.insert(key.clone(), image);
        c.waiting.remove(&key).unwrap_or_default()
    });
    for v in views {
        if ui.tree.contains(v) {
            ui.request_paint(v);
        }
    }
}

/// Forgets the icons of `folder`'s items (its contents changed).
pub fn forget_under(folder: &Path) {
    CACHE.with(|c| {
        c.borrow_mut()
            .icons
            .retain(|(p, _, _), _| p.parent() != Some(folder));
    });
}
