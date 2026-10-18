//! Draws a [`Plan`] at its exact pixel size: the SVG layers of `silica.assets` (resvg), the
//! engraved symbol, an app icon, and the label (ab_glyph with Inter).

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use resvg::tiny_skia::{self, Pixmap, PixmapPaint, Transform};
use resvg::usvg;
use silica_assets::{Assets, Encoding, EntryId, KeySpace, Layer, LayerRole, Query, Theme};

use super::rules::{AppDraw, DocDraw, LabelDraw, LayerDraw, Plan, SymbolDraw, tint_color};

/// An icon bigger than this is refused.
pub const MAX_SIDE: u32 = 4096;

fn layer_of(role: LayerRole) -> Option<Layer> {
    match role {
        LayerRole::Back => Some(Layer::Background),
        LayerRole::Middle => Some(Layer::Middle),
        LayerRole::Front => Some(Layer::Foreground),
        _ => None,
    }
}

/// The SVG bytes of a stored layer of an entry.
fn layer_svg(assets: &Assets, entry: EntryId, layer: Layer) -> Option<&[u8]> {
    assets
        .entry(entry)?
        .variants()
        .find(|v| v.layer == layer && v.encoding == Encoding::Svg)
        .map(|v| v.data)
}

/// The SVG of the foreground of the system type `name` (`file.system.<name>`), if the build had
/// its artwork.
pub fn system_svg(assets: &Assets, name: &str) -> Option<Vec<u8>> {
    let entry = assets.lookup(KeySpace::Role, &format!("file.system.{name}"))?;
    layer_svg(assets, entry, Layer::Foreground).map(<[u8]>::to_vec)
}

/// Renders an SVG scaled so its width is `width` pixels, with its top-left at `(x, y)`, on a
/// transparent pixmap `side` x `side`.
fn render_svg(data: &[u8], side: u32, x: f32, y: f32, width: f32) -> Option<Pixmap> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let k = width / tree.size().width().max(f32::EPSILON);
    let mut pixmap = Pixmap::new(side, side)?;
    resvg::render(
        &tree,
        Transform::from_scale(k, k).post_translate(x, y),
        &mut pixmap.as_mut(),
    );
    Some(pixmap)
}

/// Applies the tint ramp to every pixel (colors only; the alpha stays).
fn tint_pixmap(pixmap: &mut Pixmap, tint: [u8; 3], ramp: super::rules::Ramp) {
    let target = tint.map(|v| f32::from(v) / 255.0);
    for p in pixmap.pixels_mut() {
        let c = p.demultiply();
        if c.alpha() == 0 {
            continue;
        }
        let src = [c.red(), c.green(), c.blue()].map(|v| f32::from(v) / 255.0);
        let out =
            tint_color(src, target, ramp).map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8);
        *p = tiny_skia::ColorU8::from_rgba(out[0], out[1], out[2], c.alpha()).premultiply();
    }
}

fn draw_layer(
    canvas: &mut Pixmap,
    assets: &Assets,
    entry: EntryId,
    layer: &LayerDraw,
    side: u32,
) -> Option<()> {
    let svg = layer_svg(assets, entry, layer_of(layer.role)?)?;
    let mut pixmap = render_svg(svg, side, 0.0, 0.0, side as f32)?;
    if let Some((tint, ramp)) = layer.tint {
        tint_pixmap(&mut pixmap, tint, ramp);
    }
    let paint = PixmapPaint {
        opacity: layer.opacity.clamp(0.0, 1.0),
        ..PixmapPaint::default()
    };
    canvas.draw_pixmap(0, 0, pixmap.as_ref(), &paint, Transform::identity(), None);
    Some(())
}

fn symbol_svg(assets: &Assets, weight: &str, name: &str) -> Option<String> {
    let id = assets.lookup(KeySpace::Symbol, &format!("{weight}/{name}"))?;
    let variant = assets
        .entry(id)?
        .variants()
        .find(|v| v.encoding == Encoding::Svg)?;
    String::from_utf8(variant.data.to_vec()).ok()
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn draw_symbol(canvas: &mut Pixmap, assets: &Assets, symbol: &SymbolDraw, side: u32) -> Option<()> {
    let template = symbol_svg(assets, &symbol.weight, &symbol.name)?;
    let mut pass = |color: [u8; 3], opacity: f32, dy: f32| -> Option<()> {
        let svg = template.replace("currentColor", &hex(color));
        let pixmap = render_svg(
            svg.as_bytes(),
            side,
            symbol.rect[0],
            symbol.rect[1] + dy,
            symbol.rect[2],
        )?;
        let paint = PixmapPaint {
            opacity,
            ..PixmapPaint::default()
        };
        canvas.draw_pixmap(0, 0, pixmap.as_ref(), &paint, Transform::identity(), None);
        Some(())
    };
    // The reflection first, shifted down, then the engraving over it.
    if let Some((color, opacity, dy)) = symbol.highlight {
        pass(color, opacity, dy)?;
    }
    pass(symbol.color, symbol.opacity, 0.0)
}

/// An image in straight RGBA8.
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decodes the best stored rendition of an app icon of `silica.assets` for `side` pixels.
pub fn assets_app_icon(assets: &Assets, name: &str, side: u32) -> Option<Bitmap> {
    let id = assets.lookup(KeySpace::App, name)?;
    let v = assets.pick(id, Query::composite(Theme::Default, side))?;
    let (width, height) = (u32::from(v.width), u32::from(v.height));
    let mut rgba = match v.encoding {
        Encoding::Rgba8Premul => v.data.to_vec(),
        Encoding::Qoi => qoi::decode_to_vec(v.data).ok()?.1,
        _ => return None,
    };
    if rgba.len()
        != (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?
    {
        return None;
    }
    // Stored premultiplied: back to straight for the resampling.
    for p in rgba.as_chunks_mut::<4>().0 {
        if let [r, g, b, a] = p
            && *a != 0
            && *a != 255
        {
            let a16 = u16::from(*a);
            for c in [r, g, b] {
                *c = ((u16::from(*c) * 255 + a16 / 2) / a16).min(255) as u8;
            }
        }
    }
    Some(Bitmap {
        width,
        height,
        rgba,
    })
}

/// Decodes a PNG file (a plain `Icon=` of a desktop entry, or the `icon.png` of an icon folder).
pub fn file_app_icon(path: &std::path::Path) -> Option<Bitmap> {
    let img = image::open(path).ok()?.to_rgba8();
    Some(Bitmap {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    })
}

fn draw_app(canvas: &mut Pixmap, app: &AppDraw, bitmap: &Bitmap) -> Option<()> {
    let target = app.rect[2].round().max(1.0) as u32;
    let source = image::RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.rgba.clone())?;
    let scaled = if source.width() == target && source.height() == target {
        source
    } else {
        image::imageops::resize(
            &source,
            target,
            target,
            image::imageops::FilterType::Lanczos3,
        )
    };
    let mut pixmap = Pixmap::new(scaled.width(), scaled.height())?;
    for (dst, src) in pixmap.pixels_mut().iter_mut().zip(scaled.pixels()) {
        let [r, g, b, a] = src.0;
        *dst = tiny_skia::ColorU8::from_rgba(r, g, b, a).premultiply();
    }
    canvas.draw_pixmap(
        app.rect[0].round() as i32,
        app.rect[1].round() as i32,
        pixmap.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Some(())
}

/// Draws the SVG of a document foreground fitted (aspect kept, centered) in its box.
fn draw_document(canvas: &mut Pixmap, draw: &DocDraw, svg: &[u8]) -> Option<()> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let (w, h) = (tree.size().width(), tree.size().height());
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let [x, y, bw, bh] = draw.rect;
    let k = (bw / w).min(bh / h);
    let (dx, dy) = (x + (bw - w * k) / 2.0, y + (bh - h * k) / 2.0);
    let mut pixmap = Pixmap::new(canvas.width(), canvas.height())?;
    resvg::render(
        &tree,
        Transform::from_scale(k, k).post_translate(dx, dy),
        &mut pixmap.as_mut(),
    );
    canvas.draw_pixmap(
        0,
        0,
        pixmap.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Some(())
}

// ----------------------------------------------------------------------------------- text

const WEIGHTS: [(&str, u16); 5] = [
    ("Inter-Regular", 400),
    ("Inter-Medium", 500),
    ("Inter-SemiBold", 600),
    ("Inter-Bold", 700),
    ("Inter-Black", 900),
];

/// The Inter files of `silica.assets`.
pub struct Fonts<'a> {
    faces: Vec<(u16, FontRef<'a>)>,
}

impl<'a> Fonts<'a> {
    pub fn from_assets(assets: &'a Assets) -> Fonts<'a> {
        let faces = WEIGHTS
            .iter()
            .filter_map(|&(name, weight)| {
                let id = assets.lookup(KeySpace::Font, name)?;
                let data = assets.entry(id)?.variants().next()?.data;
                Some((weight, FontRef::try_from_slice(data).ok()?))
            })
            .collect();
        Fonts { faces }
    }

    /// The face closest to `weight` (a weight that is not shipped falls to the nearest one).
    pub fn face(&self, weight: u16) -> Option<&FontRef<'a>> {
        self.faces
            .iter()
            .min_by_key(|(w, _)| w.abs_diff(weight))
            .map(|(_, f)| f)
    }

    fn scale(font: &FontRef<'_>, size: f32) -> PxScale {
        let em = font.units_per_em().unwrap_or(1000.0).max(1.0);
        PxScale::from(size * font.height_unscaled() / em)
    }

    /// The width of `text` at `size` (any unit) with kerning, 0 without a face.
    pub fn advance(&self, text: &str, size: f32, weight: u16) -> f32 {
        let Some(font) = self.face(weight) else {
            return 0.0;
        };
        let scaled = font.as_scaled(Self::scale(font, size));
        let mut width = 0.0;
        let mut previous = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = previous {
                width += scaled.kern(p, id);
            }
            width += scaled.h_advance(id);
            previous = Some(id);
        }
        width
    }

    fn draw(&self, canvas: &mut Pixmap, label: &LabelDraw, side: u32) -> Option<()> {
        let font = self.face(label.plan.weight)?;
        let k = side as f32 / super::rules::CANVAS;
        let size = label.plan.size * k;
        let scaled = font.as_scaled(Self::scale(font, size));
        let tracking = label.plan.tracking * k;
        let letters = label.plan.text.chars().count().saturating_sub(1) as f32;
        let width = self.advance(&label.plan.text, size, label.plan.weight) + tracking * letters;
        let mut x = label.plan.center_x * k - width / 2.0;
        let baseline = label.plan.baseline * k;
        let [r, g, b] = label.color.map(f32::from);
        let (w, h) = (canvas.width() as i32, canvas.height() as i32);
        let stride = canvas.width() as usize;
        let mut previous = None;
        for c in label.plan.text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = previous {
                x += scaled.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scaled.scale(), point(x, baseline));
            if let Some(outline) = font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                let (ox, oy) = (bounds.min.x as i32, bounds.min.y as i32);
                let pixels = canvas.pixels_mut();
                outline.draw(|px, py, coverage| {
                    let (cx, cy) = (ox + px as i32, oy + py as i32);
                    if cx < 0 || cy < 0 || cx >= w || cy >= h {
                        return;
                    }
                    let Some(dst) = pixels.get_mut(cy as usize * stride + cx as usize) else {
                        return;
                    };
                    let a = coverage.clamp(0.0, 1.0);
                    let over = |s: f32, d: u8| {
                        (s * a + f32::from(d) * (1.0 - a)).round().clamp(0.0, 255.0) as u8
                    };
                    let alpha = (255.0 * a + f32::from(dst.alpha()) * (1.0 - a))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                    if let Some(px) = tiny_skia::PremultipliedColorU8::from_rgba(
                        over(r, dst.red()),
                        over(g, dst.green()),
                        over(b, dst.blue()),
                        alpha,
                    ) {
                        *dst = px;
                    }
                });
            }
            x += scaled.h_advance(id) + tracking;
            previous = Some(id);
        }
        Some(())
    }
}

/// Renders a plan. `entry` holds the layer SVGs; `app` is the decoded app icon and `document`
/// the SVG of the document foreground and `full` those of the layers of a full icon, when the
/// plan has them. None when a layer cannot be drawn
/// or the size is out of range.
pub fn render(
    assets: &Assets,
    entry: EntryId,
    plan: &Plan,
    app: Option<&Bitmap>,
    document: Option<&[u8]>,
    full: &[Vec<u8>],
    fonts: &Fonts<'_>,
) -> Option<Pixmap> {
    if plan.side == 0 || plan.side > MAX_SIDE {
        return None;
    }
    let mut canvas = Pixmap::new(plan.side, plan.side)?;
    // A symbol, app icon or document foreground that cannot be drawn leaves the sheet without it.
    let glyph = |canvas: &mut Pixmap| {
        if let Some(symbol) = &plan.symbol {
            let _ = draw_symbol(canvas, assets, symbol, plan.side);
        }
        if let (Some(draw), Some(bitmap)) = (&plan.app, app) {
            let _ = draw_app(canvas, draw, bitmap);
        }
        if let (Some(draw), Some(svg)) = (&plan.document, document) {
            let _ = draw_document(canvas, draw, svg);
        }
        for (draw, svg) in plan.full.iter().zip(full) {
            let _ = draw_document(canvas, draw, svg);
        }
    };
    for (i, layer) in plan.layers.iter().enumerate() {
        if i == plan.glyph_at {
            glyph(&mut canvas);
        }
        draw_layer(&mut canvas, assets, entry, layer, plan.side)?;
    }
    if plan.glyph_at >= plan.layers.len() {
        glyph(&mut canvas);
    }
    if let Some(label) = &plan.label {
        // Without Inter the icon has no label rather than a wrong one.
        let _ = fonts.draw(&mut canvas, label, plan.side);
    }
    Some(canvas)
}

/// The straight RGBA bytes of a pixmap.
pub fn straight_rgba(pixmap: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixmap.pixels().len() * 4);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}
