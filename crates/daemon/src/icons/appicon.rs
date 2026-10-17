//! App icons composed from `icon.assets` ([`silica_assets::appicon`]) in light, dark, clear (light
//! only) and tinted (docs/thumbnails.md, "App icons"). A port of `tools/compose.py` of silica-icons
//! (docs/icon-format.md, Format 2): SVG layers rasterized with resvg, the per-appearance colour
//! transforms in OKLab, glass, blends, the dark forms, `min_size`, the clip to the body.
//! Dynamic layers are not drawn: the host draws them live on top. Cast shadows and the rim
//! belong to the host, as in the reference.
//!
//! Images are straight RGBA, `f32` in 0..1.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use silica_assets::appicon::{AppIcon, Background, Blend, Form, Mode};

use super::apps::{Apps, Dirs};
use super::svgfix;

/// A request bigger than this is refused.
pub const MAX_SIDE: u32 = 2048;
/// `Accent/Amalfi`.
pub const DEFAULT_ACCENT: u32 = 0x0e_77_e6;
/// An `icon.assets` bigger than this is not read.
const MAX_FILE: u64 = 64 * 1024 * 1024;
const APPS_TTL: Duration = Duration::from_secs(60);

type Px = [f32; 4];
type Rgb = [f32; 3];

/// The appearance word of the D-Bus method: the low two bits pick the look, bit 2 the dark form.
/// Clear exists only in light: clear with the dark bit is clear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Light,
    Clear,
    Tinted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub look: Look,
    pub dark: bool,
}

impl Appearance {
    /// 0 light, 1 clear, 2 tinted; add 4 for the dark form (4 dark, 6 tinted dark; 5 is clear).
    pub fn from_u32(v: u32) -> Option<Appearance> {
        let look = match v & 3 {
            0 => Look::Light,
            1 => Look::Clear,
            2 => Look::Tinted,
            _ => return None,
        };
        (v < 8).then_some(Appearance {
            look,
            dark: v & 4 != 0 && look != Look::Clear,
        })
    }

    fn name(self) -> &'static str {
        match (self.look, self.dark) {
            (Look::Light, false) => "light",
            (Look::Light, true) => "dark",
            (Look::Clear, _) => "clear",
            (Look::Tinted, false) => "tinted",
            (Look::Tinted, true) => "tinted-dark",
        }
    }
}

// ------------------------------------------------------------------------------ colour math

fn hex_rgb(v: u32) -> Rgb {
    [16, 8, 0].map(|s| ((v >> s) & 255) as f32 / 255.0)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [0, 1, 2].map(|i| channel(a, i) * (1.0 - t) + channel(b, i) * t)
}

fn channel(c: Rgb, i: usize) -> f32 {
    c.get(i).copied().unwrap_or(0.0)
}

const WHITE: Rgb = [1.0; 3];
const BLACK: Rgb = [0.0; 3];

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn oklab([r, g, b]: Rgb) -> [f32; 3] {
    let (r, g, b) = (to_linear(r), to_linear(g), to_linear(b));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn oklab_to_linear([lum, a, b]: [f32; 3]) -> Rgb {
    let l = (lum + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lum - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lum - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_93 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
        -0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

/// OKLCH(L, chroma and hue of the accent), the chroma reduced until the colour fits sRGB.
/// A table over L (the result depends on L only): 4096 entries, nearest.
struct AccentMap(Vec<Rgb>);

const ACCENT_STEPS: usize = 4096;

impl AccentMap {
    fn new(accent: Rgb) -> AccentMap {
        let [_, a, b] = oklab(accent);
        let entry = |i: usize| {
            let lum = i as f32 / (ACCENT_STEPS - 1) as f32;
            let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
            for _ in 0..16 {
                let mid = (lo + hi) / 2.0;
                let lin = oklab_to_linear([lum, a * mid, b * mid]);
                if lin.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v)) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            oklab_to_linear([lum, a * lo, b * lo]).map(|v| to_srgb(v.clamp(0.0, 1.0)))
        };
        AccentMap((0..ACCENT_STEPS).map(entry).collect())
    }

    fn get(&self, lum: f32) -> Rgb {
        let i = (lum.clamp(0.0, 1.0) * (ACCENT_STEPS - 1) as f32).round() as usize;
        self.0.get(i).copied().unwrap_or(WHITE)
    }
}

/// Piecewise-linear through the stops (positions ascending), clamped at both ends.
fn ramp(x: f32, stops: &[(f32, Rgb)]) -> Rgb {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return BLACK;
    };
    if x <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        if let [(p0, c0), (p1, c1)] = pair
            && x <= *p1
        {
            return mix(*c0, *c1, (x - p0) / (p1 - p0).max(f32::EPSILON));
        }
    }
    last.1
}

// ----------------------------------------------------------------------------------- images

struct Img {
    side: usize,
    px: Vec<Px>,
}

impl Img {
    fn blank(side: usize) -> Img {
        Img {
            side,
            px: vec![[0.0; 4]; side * side],
        }
    }

    /// A body-high vertical gradient (y 100..924 of the canvas) filling the canvas.
    fn vertical(side: usize, top: Rgb, bottom: Rgb, a_top: f32, a_bottom: f32) -> Img {
        let k = side as f32 / 1024.0;
        let mut img = Img::blank(side);
        for (y, row) in img.px.chunks_exact_mut(side).enumerate() {
            let t = (((y as f32 + 0.5) - 100.0 * k) / (824.0 * k)).clamp(0.0, 1.0);
            let c = mix(top, bottom, t);
            let a = a_top * (1.0 - t) + a_bottom * t;
            row.fill([c[0], c[1], c[2], a]);
        }
        img
    }

    /// The pixmap (premultiplied) box-averaged by `k` per side, as straight alpha.
    fn averaged(pixmap: &Pixmap, k: usize) -> Img {
        let big = pixmap.width() as usize;
        let side = big / k.max(1);
        let n = (k * k) as f32 * 255.0;
        let src = pixmap.pixels();
        let mut img = Img::blank(side);
        for (y, row) in img.px.chunks_exact_mut(side.max(1)).enumerate() {
            for (x, dst) in row.iter_mut().enumerate() {
                let mut sum = [0.0_f32; 4];
                for dy in 0..k {
                    let start = (y * k + dy) * big + x * k;
                    for p in src.get(start..start + k).unwrap_or(&[]) {
                        for (s, v) in sum
                            .iter_mut()
                            .zip([p.red(), p.green(), p.blue(), p.alpha()])
                        {
                            *s += f32::from(v);
                        }
                    }
                }
                let [r, g, b, a] = sum.map(|v| v / n);
                *dst = if a > 0.0 {
                    [r / a, g / a, b / a, a]
                } else {
                    [0.0; 4]
                };
            }
        }
        img
    }

    fn rgba8(&self) -> Vec<u8> {
        self.px
            .iter()
            .flatten()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect()
    }
}

/// Samples per pixel side of the layer renders; the render is capped at 4096 px.
const SUPERSAMPLE: u32 = 4;

/// An SVG of the 1024 canvas rendered at `side`: drawn at `SUPERSAMPLE` times the side (with the
/// filter regions widened, see `svgfix`) and box-averaged in premultiplied float, which is plain
/// anti-aliasing and removes the hairline where Sketch's shadows are knocked out under a shape.
fn render_svg(data: &[u8], side: u32) -> Option<Img> {
    let k = SUPERSAMPLE.min(4096 / side.max(1)).max(1);
    let data = svgfix::widen_filters(data);
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
    let big = side.checked_mul(k)?;
    let scale = big as f32 / tree.size().width().max(f32::EPSILON);
    let mut pixmap = Pixmap::new(big, big)?;
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(Img::averaged(&pixmap, k as usize))
}

/// `src` over `dst` (straight alpha), with the blend applied against the destination colour.
fn over(dst: Px, src: Px, blend: Blend) -> Px {
    let (sa, da) = (src[3], dst[3]);
    let sc = [src[0], src[1], src[2]];
    let dc = [dst[0], dst[1], dst[2]];
    let sc = match blend {
        Blend::Normal => sc,
        Blend::Multiply => mix(sc, [0, 1, 2].map(|i| channel(sc, i) * channel(dc, i)), da),
        Blend::Screen => mix(
            sc,
            [0, 1, 2].map(|i| 1.0 - (1.0 - channel(sc, i)) * (1.0 - channel(dc, i))),
            da,
        ),
    };
    let a = sa + da * (1.0 - sa);
    let k = a.max(1e-9);
    let c = [0, 1, 2].map(|i| (channel(sc, i) * sa + channel(dc, i) * da * (1.0 - sa)) / k);
    [c[0], c[1], c[2], a]
}

// ------------------------------------------------------------------------------- the layers

fn background(icon: &AppIcon, side: u32, ap: Appearance, accent: Rgb) -> Option<Img> {
    let s = side as usize;
    let g = |c: [u8; 3]| c.map(|v| f32::from(v) / 255.0);
    Some(match (ap.look, ap.dark) {
        (Look::Light, dark) => {
            let b = icon.background.get(usize::from(dark))?;
            match b {
                Background::Gradient(top, bottom) => {
                    Img::vertical(s, g(*top), g(*bottom), 1.0, 1.0)
                }
                Background::Svg(i) => render_svg(icon.blob(*i)?, side)?,
            }
        }
        (Look::Clear, _) => Img::vertical(s, WHITE, WHITE, 0.50, 0.25),
        (Look::Tinted, true) => Img::vertical(
            s,
            mix(accent, BLACK, 0.62),
            mix(accent, BLACK, 0.84),
            1.0,
            1.0,
        ),
        (Look::Tinted, false) => Img::vertical(
            s,
            mix(accent, WHITE, 0.90),
            mix(accent, WHITE, 0.76),
            1.0,
            1.0,
        ),
    })
}

/// The per-appearance colour transform of one layer (alpha unchanged).
fn transform(
    img: &mut Img,
    mode: Mode,
    ap: Appearance,
    accent: Rgb,
    map: &AccentMap,
    luma: Option<[f32; 2]>,
) {
    if matches!(mode, Mode::Keep | Mode::Glass) || ap.look == Look::Light {
        return;
    }
    let dark = ap.dark;
    let tinted_stops = if dark {
        vec![
            (0.0, mix(accent, BLACK, 0.70)),
            (1.0, mix(accent, WHITE, 0.60)),
        ]
    } else {
        vec![(0.0, mix(accent, BLACK, 0.70)), (0.5, accent), (1.0, WHITE)]
    };
    let ink = if dark {
        mix(accent, WHITE, 0.70)
    } else {
        WHITE
    };
    for p in &mut img.px {
        let lum = oklab([p[0], p[1], p[2]])[0].clamp(0.0, 1.0);
        let rgb: Rgb = if ap.look == Look::Clear {
            let (lo, hi) = if mode == Mode::Light {
                (200.0, 255.0)
            } else {
                luma.map_or((105.0, 255.0), |l| (l[0], l[1]))
            };
            [(lo + (hi - lo) * lum) / 255.0; 3]
        } else if mode == Mode::Accent {
            map.get(lum)
        } else if mode == Mode::Light {
            let k = (200.0 + 55.0 * lum) / 255.0;
            ink.map(|v| v * k)
        } else {
            let x = match luma {
                Some([lo, hi]) => lo + (hi - lo) * lum,
                None => lum,
            };
            ramp(x, &tinted_stops)
        };
        p[0] = rgb[0];
        p[1] = rgb[1];
        p[2] = rgb[2];
    }
}

/// A Gaussian blur of every channel (edge pixels repeat), one separable pass.
fn blur_pass(src: &[Px], side: usize, kernel: &[f32], horizontal: bool) -> Vec<Px> {
    let r = (kernel.len() / 2) as isize;
    let mut out = vec![[0.0; 4]; src.len()];
    for (i, o) in out.iter_mut().enumerate() {
        let (x, y) = ((i % side) as isize, (i / side) as isize);
        let mut acc = [0.0_f32; 4];
        for (k, w) in kernel.iter().enumerate() {
            let d = k as isize - r;
            let (sx, sy) = if horizontal {
                ((x + d).clamp(0, side as isize - 1), y)
            } else {
                (x, (y + d).clamp(0, side as isize - 1))
            };
            if let Some(p) = src.get(sy as usize * side + sx as usize) {
                for (a, v) in acc.iter_mut().zip(p) {
                    *a += v * w;
                }
            }
        }
        *o = acc;
    }
    out
}

fn gaussian(img: &Img, sigma: f32) -> Vec<Px> {
    if sigma <= 0.0 {
        return img.px.clone();
    }
    let r = (sigma * 3.0).ceil() as isize;
    let mut kernel: Vec<f32> = (-r..=r)
        .map(|d| (-((d * d) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = kernel.iter().sum();
    kernel.iter_mut().for_each(|w| *w /= sum);
    let h = blur_pass(&img.px, img.side, &kernel, true);
    blur_pass(&h, img.side, &kernel, false)
}

/// The layer as a translucent white glass piece over the blurred stack below it.
fn glass(below: &mut Img, layer: &Img, opacity: f32, tune: [f32; 3]) {
    let side = layer.side;
    let alpha = |p: &Px| p[3];
    let row_has = |row: &[Px]| row.iter().any(|p| alpha(p) > 0.0);
    let rows: Vec<usize> = layer
        .px
        .chunks_exact(side)
        .enumerate()
        .filter(|(_, r)| row_has(r))
        .map(|(y, _)| y)
        .collect();
    let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else {
        return;
    };
    let sigma = tune[2] * side as f32 / 1024.0;
    let blurred = gaussian(below, sigma);
    let lip = ((4.0 * side as f32 / 1024.0).round() as usize).max(1);
    for (y, (row, lrow)) in below
        .px
        .chunks_exact_mut(side)
        .zip(layer.px.chunks_exact(side))
        .enumerate()
    {
        let t = ((y as f32 - first as f32) / (last - first).max(1) as f32).clamp(0.0, 1.0);
        let g = tune[0] * (1.0 - t) + tune[1] * t;
        for (x, (dst, l)) in row.iter_mut().zip(lrow).enumerate() {
            let a = l[3];
            let bl = blurred.get(y * side + x).copied().unwrap_or(*dst);
            let k = a * opacity;
            let mut out: Px = [0.0; 4];
            for ((o, d), b) in out.iter_mut().zip(dst.iter()).zip(bl.iter()) {
                *o = d * (1.0 - k) + b * k;
            }
            out = over(out, [1.0, 1.0, 1.0, g * a * opacity], Blend::Normal);
            let above = y
                .checked_sub(lip)
                .and_then(|yy| layer.px.get(yy * side + x))
                .map_or(0.0, |p| p[3]);
            let edge = (a - above).clamp(0.0, 1.0) * opacity;
            *dst = over(out, [1.0, 1.0, 1.0, edge], Blend::Normal);
        }
    }
}

/// The body: the Sketch `Icon / Background` Body path (824 square at (100, 100), radius 214,
/// smoothing 0.6; `tools/body.svg` of silica-icons), as an alpha mask at `side`.
fn body_mask(side: u32) -> Option<Vec<f32>> {
    let tree = usvg::Tree::from_data(BODY_SVG.as_bytes(), &usvg::Options::default()).ok()?;
    let k = side as f32 / tree.size().width().max(f32::EPSILON);
    let mut pixmap = Pixmap::new(side, side)?;
    resvg::render(&tree, Transform::from_scale(k, k), &mut pixmap.as_mut());
    Some(
        pixmap
            .pixels()
            .iter()
            .map(|p| f32::from(p.alpha()) / 255.0)
            .collect(),
    )
}

const BODY_SVG: &str = include_str!("body.svg");

/// Composes `icon` at `side` pixels (`pt` points wide) in `ap`, `accent` as 0xRRGGBB for the
/// tinted looks. None when an SVG does not parse or the size is not usable.
pub fn compose(icon: &AppIcon, side: u32, pt: f32, ap: Appearance, accent: u32) -> Option<Vec<u8>> {
    if side == 0 || side > MAX_SIDE {
        return None;
    }
    let accent = hex_rgb(accent);
    let map = AccentMap::new(accent);
    let mut out = background(icon, side, ap, accent)?;
    let form_ix = usize::from(ap.dark);
    let tune = icon.glass;
    for layer in &icon.layers {
        // The host draws these live.
        if layer.dynamic.is_some() {
            continue;
        }
        let form: &Form = layer.forms.get(form_ix)?;
        if form.min_size > pt {
            continue;
        }
        let (mode, factor, luma) = match ap.look {
            Look::Light => (Mode::Keep, 1.0, None),
            Look::Clear => (form.clear.mode, form.clear.opacity, form.clear.luma),
            Look::Tinted => (form.tinted.mode, form.tinted.opacity, form.tinted.luma),
        };
        if mode == Mode::Hide {
            continue;
        }
        let opacity = form.opacity * factor;
        let mut img = render_svg(icon.blob(form.svg)?, side)?;
        transform(&mut img, mode, ap, accent, &map, luma);
        if mode == Mode::Glass {
            glass(&mut out, &img, opacity, tune);
            continue;
        }
        for (dst, src) in out.px.iter_mut().zip(&img.px) {
            *dst = over(*dst, [src[0], src[1], src[2], src[3] * opacity], form.blend);
        }
    }
    let mask = body_mask(side)?;
    for (p, m) in out.px.iter_mut().zip(&mask) {
        p[3] *= m;
    }
    Some(out.rgba8())
}

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
