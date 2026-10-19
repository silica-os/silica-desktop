//! The desktop wallpaper: from the Settings service (`wallpaper.image` a path,
//! `wallpaper.color` `#rrggbb`, `wallpaper.id` an image of the Amalfi set; the first one
//! set wins in that order), the Amalfi set from `SILICA_WALLPAPERS`, the data folders'
//! `silica/wallpapers` or a silica-wallpapers checkout in `~/silica` (`amalfi/<id>/light.png`
//! and `dark.png`), Ravello by default. Images fill the screen and are centred (the excess
//! is cropped).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use skia_safe::{Data, Image};
use vetro_lib::color::Color;
use vetro_lib::geom::Rect;
use vetro_lib::services::settings::{SettingValue, SettingsMap};

/// The default wallpaper of the set (user decision 2026-10-16).
pub const DEFAULT_ID: &str = "ravello";

/// What the wallpaper is.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    /// An image file.
    Image(PathBuf),
    /// A plain colour.
    Color(Color),
}

/// The folders that may hold the Amalfi set.
fn roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(d) = std::env::var_os("SILICA_WALLPAPERS").filter(|d| !d.is_empty()) {
        out.push(PathBuf::from(d));
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    out.extend(data_home.map(|d| d.join("silica/wallpapers")));
    let dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    out.extend(
        dirs.split(':')
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join("silica/wallpapers")),
    );
    if let Some(h) = std::env::var_os("HOME") {
        out.push(PathBuf::from(h).join("silica/silica-wallpapers"));
    }
    out
}

/// The image of `id` in the Amalfi set for the appearance.
pub fn set_image(id: &str, dark: bool) -> Option<PathBuf> {
    if id.is_empty() || id.contains('/') || id.contains("..") {
        return None;
    }
    let file = if dark { "dark.png" } else { "light.png" };
    roots()
        .into_iter()
        .map(|r| r.join("amalfi").join(id).join(file))
        .find(|p| p.is_file())
}

fn text<'a>(settings: &'a SettingsMap, key: &str) -> Option<&'a str> {
    settings
        .get(key)
        .and_then(SettingValue::as_text)
        .filter(|s| !s.is_empty())
}

fn parse_color(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// The wallpaper the settings choose.
pub fn choice(settings: &SettingsMap, dark: bool) -> Option<Choice> {
    if let Some(p) = text(settings, "wallpaper.image") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(Choice::Image(path));
        }
    }
    if let Some(c) = text(settings, "wallpaper.color").and_then(parse_color) {
        return Some(Choice::Color(c));
    }
    let id = text(settings, "wallpaper.id").unwrap_or(DEFAULT_ID);
    set_image(id, dark)
        .or_else(|| set_image(DEFAULT_ID, dark))
        .map(Choice::Image)
}

thread_local! {
    static IMAGES: RefCell<HashMap<PathBuf, Option<Image>>> = RefCell::new(HashMap::new());
}

/// The decoded image of a file (decoded once per process).
pub fn image(path: &PathBuf) -> Option<Image> {
    IMAGES.with(|cache| {
        cache
            .borrow_mut()
            .entry(path.clone())
            .or_insert_with(|| {
                let bytes = std::fs::read(path).ok()?;
                Image::from_encoded(Data::new_copy(&bytes))
            })
            .clone()
    })
}

/// Where an image of `w` × `h` goes to fill `area`, centred (aspect fill).
pub fn fill_rect(w: f32, h: f32, area: Rect) -> Rect {
    if w <= 0.0 || h <= 0.0 {
        return area;
    }
    let scale = (area.width / w).max(area.height / h);
    let (dw, dh) = (w * scale, h * scale);
    Rect::new(
        area.x + (area.width - dw) / 2.0,
        area.y + (area.height - dh) / 2.0,
        dw,
        dh,
    )
}

/// The desktop's background surface (one per output): the wallpaper, filling the screen.
pub struct BackgroundView {
    /// The choice to draw (shared with the shell's settings).
    pub choice: std::rc::Rc<std::cell::RefCell<Option<Choice>>>,
}

impl vetro_lib::core::Widget for BackgroundView {
    fn kind(&self) -> &'static str {
        "Wallpaper"
    }

    fn measure(
        &mut self,
        _cx: &mut vetro_lib::core::LayoutCx<'_>,
        p: vetro_lib::core::Proposal,
    ) -> vetro_lib::geom::Size {
        p.replacing_unspecified(vetro_lib::geom::Size::new(1920.0, 1200.0))
    }

    fn paint(&mut self, cx: &mut vetro_lib::core::PaintCx<'_>) {
        use vetro_lib::shape::Shape;
        use vetro_lib::style::Style;
        let r = cx.bounds();
        let fallback = Color::rgb(0x1b, 0x1c, 0x1f);
        match self.choice.borrow().as_ref() {
            Some(Choice::Image(path)) => match image(path) {
                Some(img) => {
                    let to = fill_rect(img.width() as f32, img.height() as f32, r);
                    cx.rec.push_clip(r, Shape::RECT);
                    cx.rec.image("Wallpaper", to, img, 1.0);
                    cx.rec.pop();
                }
                None => cx
                    .rec
                    .layer("Wallpaper", r, Shape::RECT, Style::fill(fallback)),
            },
            Some(Choice::Color(c)) => cx.rec.layer("Wallpaper", r, Shape::RECT, Style::fill(*c)),
            None => cx
                .rec
                .layer("Wallpaper", r, Shape::RECT, Style::fill(fallback)),
        }
    }
}

/// Opens the background surfaces; returns their views (repainted when the choice changes).
pub fn open_background(
    ui: &mut vetro_lib::core::Ui,
    choice: &std::rc::Rc<std::cell::RefCell<Option<Choice>>>,
) -> Vec<vetro_lib::core::ViewId> {
    use vetro_lib::core::window::{Edges, KeyboardMode, Layer};
    use vetro_lib::core::{Prop, Value, WindowKind};
    let outputs: Vec<(Option<String>, vetro_lib::geom::Size)> = if ui.outputs().is_empty() {
        vec![(None, vetro_lib::geom::Size::new(1920.0, 1200.0))]
    } else {
        ui.outputs()
            .iter()
            .map(|o| (Some(o.name.clone()), o.size))
            .collect()
    };
    let mut views = Vec::new();
    for (name, size) in outputs {
        let w = ui.create_window(
            WindowKind::Shell {
                layer: Layer::Background,
                anchor: Edges::TOP | Edges::BOTTOM | Edges::LEFT | Edges::RIGHT,
                exclusive: -1,
                margin: [0; 4],
                keyboard: KeyboardMode::None,
                namespace: "wallpaper".into(),
                output: name,
            },
            size,
        );
        let v = ui.create(BackgroundView {
            choice: choice.clone(),
        });
        let _ = ui.set(v, Prop::MaxWidth, Value::from(f32::INFINITY));
        let _ = ui.set(v, Prop::MaxHeight, Value::from(f32::INFINITY));
        ui.set_content(w, v);
        ui.show_window(w);
        views.push(v);
    }
    views
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_fill_and_centre() {
        let r = fill_rect(100.0, 100.0, Rect::new(0.0, 0.0, 200.0, 100.0));
        assert_eq!(r, Rect::new(0.0, -50.0, 200.0, 200.0));
        assert_eq!(parse_color("#7a7b80"), Some(Color::rgb(0x7a, 0x7b, 0x80)));
        assert_eq!(parse_color("7a7b80"), None);
        assert_eq!(set_image("../x", true), None);
    }
}
