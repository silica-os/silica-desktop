//! The Dock's geometry (docs/spec/dock.md, the kit's `Dock` masters): sizes at any icon
//! canvas, the resting and magnified layout, the measured curves (magnification profile and
//! entrance, launch bounce, attention hops, the drag gap), badges and labels.
//!
//! Everything is in points in the Dock surface: as wide as the screen, its bottom edge on the
//! screen's bottom edge.

use vetro_lib::geom::Rect;

use crate::motion::{Bezier, sample};

/// The canvas of an app icon at rest (kit tile 64 × 80, canvas 60 at (2, 10)).
pub const DEFAULT_CANVAS: f32 = 60.0;
/// The smallest and largest canvas (`dock.tileSize`, the separator drag).
pub const MIN_CANVAS: f32 = 16.0;
/// See [`MIN_CANVAS`].
pub const MAX_CANVAS: f32 = 128.0;
/// The shelf's distance from the screen's bottom edge.
pub const BOTTOM_GAP: f32 = 4.0;
/// Magnification starts this far above the shelf (D-00 "Active zone").
pub const ZONE_ABOVE: f32 = 5.0;
/// Leaving upwards, a magnified cell holds the zone up to its icon's top minus this.
pub const ZONE_HOLD: f32 = 6.0;
/// No labels in the end strips of the shelf.
pub const END_STRIP: f32 = 8.0;
/// The label: a body 28 tall, radius 8, 16 of padding around the text on the 4 pt grid, an
/// arrow 6 deep under it (kit `Dock/Label`).
pub const LABEL_HEIGHT: f32 = 28.0;
/// See [`LABEL_HEIGHT`].
pub const LABEL_ARROW: f32 = 6.0;
/// See [`LABEL_HEIGHT`].
pub const LABEL_PADDING: f32 = 16.0;
/// See [`LABEL_HEIGHT`].
pub const LABEL_RADIUS: f32 = 8.0;
/// The widest label.
pub const LABEL_MAX: f32 = 224.0;
/// A press becomes a drag beyond this (D-05).
pub const DRAG_THRESHOLD: f32 = 5.0;
/// "Remove" shows once the pointer is this far above the grab point (D-05).
pub const REMOVE_ABOVE: f32 = 130.0;
/// The pointer counts as in the Dock up to this far above the shelf while dragging.
pub const DRAG_INSIDE_ABOVE: f32 = 20.0;
/// A minimised window's thumbnail in its cell, and its app icon (D-08).
pub const THUMB: (f32, f32) = (57.0, 47.0);
/// See [`THUMB`].
pub const THUMB_BADGE: f32 = 18.0;
/// See [`THUMB`].
pub const THUMB_BADGE_BELOW: f32 = 4.0;
/// Badges (SH-DK-29): a red capsule 23 tall, its right edge 3 inside the cell.
pub const BADGE_HEIGHT: f32 = 23.0;
/// See [`BADGE_HEIGHT`].
pub const BADGE_RIGHT: f32 = 3.0;
/// Download progress (SH-DK-30): a 59 × 4 capsule in place of the dot.
pub const PROGRESS: (f32, f32) = (59.0, 4.0);

/// The sizes of a Dock whose icon canvas is `canvas` (16–128): the kit's values scaled by
/// k = canvas / 60, each margin rounded once to whole points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// The icon canvas.
    pub canvas: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            canvas: DEFAULT_CANVAS,
        }
    }
}

impl Metrics {
    /// The metrics at `canvas`, clamped and in whole points.
    pub fn new(canvas: f32) -> Metrics {
        let c = if canvas.is_finite() {
            canvas.clamp(MIN_CANVAS, MAX_CANVAS).round()
        } else {
            DEFAULT_CANVAS
        };
        Metrics { canvas: c }
    }

    /// The scale against the kit.
    pub fn k(&self) -> f32 {
        self.canvas / DEFAULT_CANVAS
    }

    fn margin_x(&self) -> f32 {
        (2.0 * self.k()).round()
    }

    fn margin_y(&self) -> f32 {
        (10.0 * self.k()).round()
    }

    /// A tile's width (the pitch: tiles have no gap).
    pub fn tile(&self) -> f32 {
        self.canvas + 2.0 * self.margin_x()
    }

    /// A tile's (and the shelf's) height.
    pub fn tile_height(&self) -> f32 {
        self.canvas + 2.0 * self.margin_y()
    }

    /// The icon canvas's left edge in its tile.
    #[cfg(test)]
    pub fn icon_left(&self) -> f32 {
        self.margin_x()
    }

    /// The icon canvas's top below the shelf's top.
    pub fn icon_top(&self) -> f32 {
        self.margin_y()
    }

    /// The shelf's padding at both ends.
    pub fn padding(&self) -> f32 {
        (8.0 * self.k()).round()
    }

    /// The shelf's corner radius.
    pub fn radius(&self) -> f32 {
        (24.0 * self.k()).round()
    }

    /// The separator cell's width.
    pub fn separator(&self) -> f32 {
        (28.0 * self.k()).round()
    }

    /// The separator line's distance from the shelf's top and bottom.
    pub fn separator_inset(&self) -> f32 {
        (10.0 * self.k()).round()
    }

    /// The running dot's side.
    pub fn dot(&self) -> f32 {
        (4.0 * self.k()).round().max(3.0)
    }

    /// The dot's centre above the shelf's bottom.
    pub fn dot_above(&self) -> f32 {
        (6.0 * self.k()).round()
    }

    /// How far above an icon slot of `side` the label's (or menu's) arrow tip is: the label
    /// ends 8k above where the icon's drawing starts (824 of the 1024 grid, centred).
    pub fn label_gap(&self, side: f32) -> f32 {
        8.0 * self.k() - side * (1.0 - 824.0 / 1024.0) / 2.0
    }
}

/// The width of a label whose text is `text_width` wide: on the 4 pt grid plus the padding.
pub fn label_width(text_width: f32) -> f32 {
    ((text_width / 4.0).ceil() * 4.0 + 2.0 * LABEL_PADDING).min(LABEL_MAX)
}

/// The badge width for `text` (SH-DK-29: 23 for one or two characters, then 26, 33, 39).
pub fn badge_width(text: &str) -> f32 {
    match text.chars().count() {
        0..=2 => 23.0,
        3 => 26.0,
        4 => 33.0,
        5 => 39.0,
        n => 39.0 + (n - 5) as f32 * 6.5,
    }
}

/// The badge text of a count (no truncation was seen up to five characters).
pub fn badge_text(count: i64) -> String {
    if count > 99_999 {
        "99999+".into()
    } else {
        count.to_string()
    }
}

/// The magnification profile f(d), every 16 pt from 0, measured on the 63 pt Dock (dock.md
/// "Profile"); distances scale with the pitch.
const PROFILE: [f32; 15] = [
    1.000, 0.991, 0.965, 0.921, 0.862, 0.787, 0.699, 0.597, 0.485, 0.365, 0.240, 0.131, 0.055,
    0.011, 0.000,
];

/// The magnification at distance `d` from the pointer, for tiles `pitch` apart.
pub fn falloff(d: f32, pitch: f32) -> f32 {
    let k = 63.0 / pitch.max(1.0);
    sample(&PROFILE, 16.0, d.abs() * k)
}

/// Progress of the peak cell entering and leaving the zone, every 10 ms of 160 (dock.md).
const ENTERING: [f32; 17] = [
    0.000, 0.002, 0.014, 0.060, 0.102, 0.202, 0.294, 0.386, 0.480, 0.577, 0.671, 0.759, 0.837,
    0.902, 0.952, 0.985, 1.000,
];

/// The entrance's progress at a fraction `t` of its duration.
pub fn enter_progress(t: f32) -> f32 {
    sample(&ENTERING, 1.0 / 16.0, t)
}

/// The entrance's duration (seconds): ~60 ms for a small growth, ~160 ms for a large one.
pub fn enter_duration(ratio: f32) -> f64 {
    let span = ((ratio - 1.08) / (2.1 - 1.08)).clamp(0.0, 1.0);
    f64::from(0.060 + 0.100 * span)
}

/// The launch growth and quit shrink of a slot (DOCK-ANIM).
pub const GROW: Bezier = Bezier(0.38, 0.0, 0.64, 1.0);
/// See [`GROW`].
pub const SHRINK: Bezier = Bezier(0.38, 0.05, 0.61, 1.0);
/// See [`GROW`] (seconds).
pub const GROW_TIME: f64 = 0.35;
/// See [`GROW`] (seconds).
pub const SHRINK_TIME: f64 = 0.48;
/// The size a launching slot starts at.
pub const GROW_FROM: f32 = 0.1;

/// The launch bounce on the 63 pt Dock (ms, pt), the measured parabola: one hop ~0.7 s.
const BOUNCE: [(f32, f32); 16] = [
    (0.0, 0.0),
    (27.0, 3.0),
    (40.0, 5.0),
    (53.0, 6.0),
    (80.0, 8.0),
    (93.0, 9.0),
    (133.0, 11.0),
    (160.0, 13.0),
    (213.0, 14.0),
    (293.0, 17.0),
    (440.0, 14.0),
    (533.0, 11.0),
    (587.0, 7.0),
    (613.0, 6.0),
    (653.0, 3.0),
    (693.0, 0.0),
];
/// One hop (seconds).
pub const BOUNCE_PERIOD: f64 = 0.693;
/// A launch that never shows a window stops bouncing after this (seconds).
pub const BOUNCE_LIMIT: f64 = 10.0;
/// The measured heights are on a 63 pt pitch; Amalfi's is 56 on that scale.
pub const AMALFI_SCALE: f32 = 56.0 / 63.0;

/// The bounce height `t` seconds after the start (repeating).
pub fn bounce_height(t: f64) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    let ms = ((t % BOUNCE_PERIOD) * 1000.0) as f32;
    for w in BOUNCE.windows(2) {
        if let [(t0, h0), (t1, h1)] = w
            && ms <= *t1
        {
            return AMALFI_SCALE * (h0 + (h1 - h0) * (ms - t0) / (t1 - t0).max(1.0));
        }
    }
    0.0
}

/// Attention (SH-DK-attn): three decaying hops (56, 23, 8 pt in ~466, 320, 214 ms), then still,
/// every 2 s.
const ATTENTION: [(f32, f32); 3] = [(466.0, 56.0), (320.0, 23.0), (214.0, 8.0)];

/// The attention height `t` seconds after the request.
pub fn attention_height(t: f64) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    let mut ms = ((t % 2.0) * 1000.0) as f32;
    for (d, h) in ATTENTION {
        if ms < d {
            let x = ms / d;
            return 4.0 * h * AMALFI_SCALE * x * (1.0 - x);
        }
        ms -= d;
    }
    0.0
}

/// A neighbour sliding over by one cell when the drag gap moves (D-05), per 13 ms frame of
/// 67 pt: the gap's presence follows it (~265 ms, an S-curve).
const GAP_SLIDE: [f32; 14] = [
    0.0, 1.0, 3.0, 7.0, 11.0, 16.0, 23.0, 29.0, 36.0, 43.0, 55.0, 60.0, 66.0, 67.0,
];
/// See [`GAP_SLIDE`] (seconds).
pub const GAP_TIME: f64 = 0.265;

/// The drag gap's progress at fraction `t` of [`GAP_TIME`].
pub fn gap_progress(t: f32) -> f32 {
    sample(&GAP_SLIDE, 1.0 / 13.0, t) / 67.0
}

/// A folder dragged in from another app opens its gap in 250 ms along this curve.
pub const FOLDER_GAP: Bezier = Bezier(0.3, 0.0, 0.7, 1.0);
/// See [`FOLDER_GAP`] (seconds).
pub const FOLDER_GAP_TIME: f64 = 0.25;
/// The removed item fades in place (seconds).
pub const REMOVE_FADE: f64 = 0.25;

/// Auto-hide (SH-DK-auto): the delay before revealing, the slide up (decelerating), the slide
/// down (accelerating), seconds.
pub const AUTOHIDE_DELAY: f64 = 0.19;
/// See [`AUTOHIDE_DELAY`].
pub const AUTOHIDE_REVEAL: f64 = 0.19;
/// See [`AUTOHIDE_DELAY`].
pub const AUTOHIDE_HIDE: f64 = 0.175;

/// The long press that opens a menu (D-04), seconds.
pub const LONG_PRESS: f64 = 0.53;
/// A file held over an app springs it open (MU-40), seconds.
pub const SPRING_LOAD: f64 = 0.83;

/// What the layout needs of a slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    /// Its width at rest.
    pub base: f32,
    /// How open it is (0–1: growing in, shrinking out, a gap) and its icon's opacity.
    pub presence: f32,
    /// The icon's opacity.
    pub opacity: f32,
    /// The bounce or attention hop.
    pub lift: f32,
    /// It does not magnify (the separator, a gap).
    pub flat: bool,
}

impl Slot {
    /// A tile of width `base`, fully there.
    pub fn tile(base: f32) -> Slot {
        Slot {
            base,
            presence: 1.0,
            opacity: 1.0,
            lift: 0.0,
            flat: false,
        }
    }
}

/// One laid-out slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// The cell: its x and width; its y and height are the shelf's.
    pub rect: Rect,
    /// The icon canvas.
    pub icon: Rect,
    /// The icon's opacity.
    pub opacity: f32,
    /// The magnification scale of the slot.
    pub scale: f32,
}

/// Where everything is.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// The shelf now.
    pub shelf: Rect,
    /// The shelf at rest (the zone's horizontal span).
    pub rest: Rect,
    /// The cells, as the slots.
    pub cells: Vec<Cell>,
}

/// Magnification input: the pointer's x, the peak scale, the progress (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Magnify {
    /// The pointer's x.
    pub x: f32,
    /// The peak canvas over the resting one.
    pub peak: f32,
    /// How far magnification has come in.
    pub progress: f32,
}

/// Lays `slots` out in a surface `width` wide whose shelf top is at `shelf_y`.
pub fn layout(
    m: &Metrics,
    slots: &[Slot],
    width: f32,
    shelf_y: f32,
    mag: Option<Magnify>,
) -> Layout {
    let widths: Vec<f32> = slots.iter().map(|s| s.base * s.presence).collect();
    let total: f32 = widths.iter().sum();
    let pad = m.padding();
    let rest_left = (width - total) / 2.0;
    let rest = Rect::new(rest_left - pad, shelf_y, total + 2.0 * pad, m.tile_height());
    let mut x = rest_left;
    let mut centres = Vec::with_capacity(slots.len());
    for w in &widths {
        centres.push(x + w / 2.0);
        x += w;
    }
    let mag = mag.filter(|g| g.peak > 1.0 && g.progress > 0.0);
    let scales: Vec<f32> = slots
        .iter()
        .zip(&centres)
        .map(|(s, c)| match mag {
            Some(g) if !s.flat => 1.0 + (g.peak - 1.0) * falloff(g.x - c, m.tile()) * g.progress,
            _ => 1.0,
        })
        .collect();
    let grown: Vec<f32> = widths.iter().zip(&scales).map(|(w, s)| w * s).collect();
    let growth: f32 = grown.iter().zip(&widths).map(|(g, w)| g - w).sum();
    // The point under the pointer stays still: what grows left of it moves left; near an end
    // that side grows less (dock.md "Spreading sideways").
    let mut left_growth = 0.0;
    if let Some(g) = mag {
        let mut x = rest_left;
        for (w, gw) in widths.iter().zip(&grown) {
            if g.x >= x + w {
                left_growth += gw - w;
            } else if g.x > x {
                left_growth += (gw - w) * (g.x - x) / w.max(1.0);
            }
            x += w;
        }
        let near_left = ((g.x - rest_left) / m.tile()).clamp(0.0, 1.0);
        let near_right = ((rest_left + total - g.x) / m.tile()).clamp(0.0, 1.0);
        left_growth *= near_left;
        left_growth = growth - (growth - left_growth) * near_right;
    }
    let mut x = rest_left - left_growth;
    let mut cells = Vec::with_capacity(slots.len());
    for ((s, gw), scale) in slots.iter().zip(&grown).zip(&scales) {
        let side = m.canvas * scale * s.presence;
        // The bottom edge rises as the icon grows: 1 pt at ×1.079, 7 at ×2.095 (dock.md).
        let rise = (scale - 1.0) * 6.4 * m.k();
        let bottom = shelf_y + m.icon_top() + m.canvas * s.presence - rise - s.lift;
        cells.push(Cell {
            rect: Rect::new(x, shelf_y, *gw, m.tile_height()),
            icon: Rect::new(x + (gw - side) / 2.0, bottom - side, side, side),
            opacity: s.opacity,
            scale: *scale,
        });
        x += gw;
    }
    let shelf = Rect::new(
        rest_left - left_growth - pad,
        shelf_y,
        total + growth + 2.0 * pad,
        m.tile_height(),
    );
    Layout { shelf, rest, cells }
}

/// The pointer at (`x`, `y`) enters the magnification zone: inside the resting shelf
/// horizontally, from 5 pt above it downwards.
pub fn in_zone(l: &Layout, x: f32, y: f32) -> bool {
    x >= l.rest.x && x <= l.rest.right() && y >= l.rest.y - ZONE_ABOVE
}

/// The pointer stays in the zone: as on entering, or inside a magnified cell up to its icon's
/// top minus [`ZONE_HOLD`] (hysteresis, D-01).
pub fn stays_in_zone(l: &Layout, x: f32, y: f32) -> bool {
    if x < l.rest.x || x > l.rest.right() {
        return false;
    }
    if in_zone(l, x, y) {
        return true;
    }
    l.cells
        .iter()
        .find(|c| x >= c.rect.x && x < c.rect.right())
        .is_some_and(|c| y >= c.icon.y - ZONE_HOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiles(n: usize, m: &Metrics) -> Vec<Slot> {
        (0..n).map(|_| Slot::tile(m.tile())).collect()
    }

    fn kit(m: &Metrics) -> Vec<Slot> {
        // Seven apps, the separator, the stack, the Trash (the kit's Dock).
        let mut s = tiles(7, m);
        s.push(Slot {
            flat: true,
            ..Slot::tile(m.separator())
        });
        s.extend(tiles(2, m));
        s
    }

    #[test]
    fn at_rest_cells_shelf_and_icons_follow_the_kit() {
        let m = Metrics::default();
        let l = layout(&m, &kit(&m), 620.0, 20.0, None);
        assert_eq!(
            (l.shelf.x, l.shelf.width, l.shelf.height),
            (0.0, 620.0, 80.0)
        );
        for (i, c) in l.cells.iter().take(7).enumerate() {
            assert_eq!(c.rect.x + c.rect.width / 2.0, 8.0 + 64.0 * i as f32 + 32.0);
            assert_eq!(c.icon.width, 60.0);
            assert_eq!(c.icon.y - l.shelf.y, 10.0);
        }
        // The separator's 28 pt cell at 456 (its line at 470), then the stack and the Trash.
        assert_eq!(l.cells[7].rect.x, 456.0);
        assert_eq!(l.cells[8].rect.x + 32.0, 484.0 + 32.0);
        assert_eq!(l.cells[9].rect.x, 548.0);
    }

    #[test]
    fn the_default_canvas_is_the_kits_60_and_sizes_scale_with_k() {
        let m = Metrics::default();
        assert_eq!(
            (m.tile(), m.tile_height(), m.padding(), m.radius()),
            (64.0, 80.0, 8.0, 24.0)
        );
        assert_eq!(
            (m.separator(), m.separator_inset(), m.dot(), m.dot_above()),
            (28.0, 10.0, 4.0, 6.0)
        );
        for c in 16..=128 {
            let m = Metrics::new(c as f32);
            // The canvas is centred in its tile: the icon and the dot stay on one axis.
            assert_eq!(m.icon_left() + m.canvas / 2.0, m.tile() / 2.0, "canvas {c}");
            assert_eq!(
                m.icon_top() + m.canvas / 2.0,
                m.tile_height() / 2.0,
                "canvas {c}"
            );
        }
        assert_eq!(Metrics::new(4.0).canvas, 16.0);
        assert_eq!(Metrics::new(400.0).canvas, 128.0);
        assert_eq!(Metrics::new(f32::NAN).canvas, 60.0);
        let big = Metrics::new(128.0);
        assert_eq!((big.tile(), big.tile_height()), (128.0 + 8.0, 128.0 + 42.0));
    }

    #[test]
    fn magnification_follows_the_measured_profile_and_keeps_the_point_still() {
        let m = Metrics::default();
        let slots = tiles(14, &m);
        let rest = layout(&m, &slots, 1280.0, 100.0, None);
        let c = rest.cells[4].rect;
        let px = c.x + c.width / 2.0;
        let g = Magnify {
            x: px,
            peak: 96.0 / 60.0,
            progress: 1.0,
        };
        let l = layout(&m, &slots, 1280.0, 100.0, Some(g));
        assert!((l.cells[4].icon.width - 96.0).abs() < 0.01);
        assert!((l.cells[4].rect.x + l.cells[4].rect.width / 2.0 - px).abs() < 0.01);
        // The shelf keeps its top and height; its width follows the grown tiles.
        assert_eq!(
            (l.shelf.y, l.shelf.height),
            (rest.shelf.y, rest.shelf.height)
        );
        let grown: f32 = l
            .cells
            .iter()
            .zip(&rest.cells)
            .map(|(a, b)| a.rect.width - b.rect.width)
            .sum();
        assert!((l.shelf.width - rest.shelf.width - grown).abs() < 0.01);
        // Far away (beyond ~3.5 tiles) nothing grows.
        assert_eq!(l.cells[9].scale, 1.0);
        // Icons rise as they grow (dock.md "Magnification").
        assert!(l.cells[4].icon.bottom() < rest.cells[4].icon.bottom());
        // Off or not larger: nothing moves.
        for peak in [1.0, 0.5] {
            let same = layout(&m, &slots, 1280.0, 100.0, Some(Magnify { peak, ..g }));
            assert_eq!(same.cells, rest.cells);
        }
        assert_eq!(falloff(0.0, 64.0), 1.0);
        assert_eq!(falloff(300.0, 64.0), 0.0);
    }

    #[test]
    fn near_one_end_only_the_other_side_grows() {
        let m = Metrics::default();
        let slots = tiles(14, &m);
        let rest = layout(&m, &slots, 1280.0, 100.0, None);
        let edge = rest.shelf.x + 3.5;
        let g = Magnify {
            x: edge,
            peak: 2.0,
            progress: 1.0,
        };
        let l = layout(&m, &slots, 1280.0, 100.0, Some(g));
        assert!((l.cells[0].rect.x - rest.cells[0].rect.x).abs() < 0.3);
    }

    #[test]
    fn entrance_is_60_ms_small_160_ms_large() {
        assert!((enter_duration(65.0 / 60.0) - 0.060).abs() < 0.001);
        assert!((enter_duration(128.0 / 60.0) - 0.160).abs() < 0.001);
        assert_eq!(enter_progress(0.0), 0.0);
        assert_eq!(enter_progress(1.0), 1.0);
    }

    #[test]
    fn active_zone_starts_5_pt_above_the_shelf_inside_the_cells() {
        let m = Metrics::default();
        let l = layout(&m, &tiles(5, &m), 1000.0, 100.0, None);
        assert!(in_zone(&l, 500.0, 95.0));
        assert!(!in_zone(&l, 500.0, 94.0));
        assert!(!in_zone(&l, l.rest.x - 1.0, 120.0));
    }

    #[test]
    fn label_badge_bounce_and_attention_follow_the_measures() {
        // The label: text on the 4 pt grid plus 32 (kit `Dock/Label`, 64 for "Mail").
        assert_eq!(label_width(28.6), 64.0);
        assert_eq!(label_width(1000.0), LABEL_MAX);
        // The label ends 8 pt above the icon's drawing (#221).
        let m = Metrics::default();
        assert!((m.label_gap(60.0) - (8.0 - 60.0 * 200.0 / 2048.0)).abs() < 1e-4);
        assert_eq!(badge_width("1"), 23.0);
        assert_eq!(badge_width("12"), 23.0);
        assert_eq!(badge_width("123"), 26.0);
        assert_eq!(badge_width("1234"), 33.0);
        assert_eq!(badge_width("Nuovo"), 39.0);
        assert_eq!(badge_text(100_000), "99999+");
        assert!((bounce_height(0.293) - 17.0 * AMALFI_SCALE).abs() < 0.01);
        assert_eq!(bounce_height(BOUNCE_PERIOD), 0.0);
        assert!((attention_height(0.233) - 56.0 * AMALFI_SCALE).abs() < 0.01);
        assert!((attention_height(0.466 + 0.160) - 23.0 * AMALFI_SCALE).abs() < 0.01);
        assert!((attention_height(0.466 + 0.320 + 0.107) - 8.0 * AMALFI_SCALE).abs() < 0.01);
        assert_eq!(attention_height(1.5), 0.0);
        assert!((attention_height(2.233) - 56.0 * AMALFI_SCALE).abs() < 0.01);
        assert_eq!(gap_progress(0.0), 0.0);
        assert_eq!(gap_progress(1.0), 1.0);
    }
}
