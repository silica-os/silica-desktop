//! The composition rules of the generic file and folder, read from `silica.assets` (slots
//! compiled from `crates/silica-assets/data/compose.toml`), and the geometry that turns them
//! into pixels. Nothing here draws: it produces a [`Plan`], the complete list of decisions for
//! one icon at one size, which `compose` renders and which doubles as the cache key.

use silica_assets::{Assets, KeySpace, LAYER_TINT, LAYER_WHEN_NOT_EMPTY, LayerRole, SlotKind};

/// The canvas all rules are measured on.
pub const CANVAS: f32 = 1024.0;

/// A band of symbol sizes: used from `min_size` pt up.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub min_size: f32,
    pub weight: String,
    pub scale: f32,
    pub highlight: bool,
}

/// An extension label.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelSpec {
    pub max_chars: usize,
    pub min_size: f32,
    pub rect: [f32; 4],
    pub size: f32,
    pub weight: u16,
    pub tracking: f32,
    pub max_width: f32,
    pub upper: bool,
    /// The colour of the text: one grey for every file.
    pub color: [u8; 3],
}

/// One step of a layer stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub role: LayerRole,
    pub tint: bool,
    pub when_not_empty: bool,
}

/// How a folder tint color turns into the colors of the layers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp {
    pub low: f32,
    pub high: f32,
    pub shade: f32,
    pub glow: f32,
}

/// The engraving of a folder symbol.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effect {
    pub color: [u8; 3],
    pub highlight: [u8; 3],
    pub opacity: f32,
    pub highlight_opacity: f32,
    pub offset: f32,
}

#[derive(Clone, Debug, Default)]
pub struct FileRules {
    pub steps: Vec<Step>,
    pub glyph_box: [f32; 4],
    pub app_body: Option<[f32; 4]>,
    pub label: Option<LabelSpec>,
    pub label_alone: Option<LabelSpec>,
}

#[derive(Clone, Debug, Default)]
pub struct FolderRules {
    pub steps: Vec<Step>,
    pub symbol_box: [f32; 4],
    pub bands: Vec<Band>,
    pub effect: Option<Effect>,
    pub ramp: Option<Ramp>,
    pub labels: Vec<(String, [u8; 3])>,
    pub specials: Vec<(String, String)>,
}

fn rgb(color: u32) -> [u8; 3] {
    [(color >> 24) as u8, (color >> 16) as u8, (color >> 8) as u8]
}

fn band_of(slot: &silica_assets::Slot<'_>) -> Band {
    Band {
        min_size: f32::from(slot.min_size),
        weight: slot.format.unwrap_or("regular").to_owned(),
        scale: slot.rect[0],
        highlight: slot.rect[1] > 0.5,
    }
}

fn label_of(slot: &silica_assets::Slot<'_>) -> LabelSpec {
    LabelSpec {
        max_chars: usize::from(slot.unit),
        min_size: f32::from(slot.min_size),
        rect: slot.rect,
        size: f32::from(slot.size) / 64.0,
        weight: slot.weight,
        tracking: slot.tracking,
        max_width: slot.max_width,
        upper: slot.format == Some("upper"),
        color: rgb(slot.color[0]),
    }
}

fn steps_of(entry: &silica_assets::Entry<'_>) -> Vec<Step> {
    entry
        .slots()
        .filter(|s| s.kind == SlotKind::LayerRule)
        .filter_map(|s| {
            Some(Step {
                role: LayerRole::from_u32(u32::from(s.unit))?,
                tint: s.weight & LAYER_TINT != 0,
                when_not_empty: s.weight & LAYER_WHEN_NOT_EMPTY != 0,
            })
        })
        .collect()
}

impl FileRules {
    pub fn from_assets(assets: &Assets) -> Option<FileRules> {
        let entry = assets.entry(assets.lookup(KeySpace::Type, "file-generic")?)?;
        let mut rules = FileRules {
            steps: steps_of(&entry),
            ..FileRules::default()
        };
        for slot in entry.slots() {
            match slot.kind {
                SlotKind::GlyphBox => rules.glyph_box = slot.rect,
                SlotKind::AppIconBody => rules.app_body = Some(slot.rect),
                SlotKind::FileLabel => rules.label = Some(label_of(&slot)),
                SlotKind::FileLabelAlone => rules.label_alone = Some(label_of(&slot)),
                _ => {}
            }
        }
        Some(rules)
    }
}

impl FolderRules {
    pub fn from_assets(assets: &Assets) -> Option<FolderRules> {
        let entry = assets.entry(assets.lookup(KeySpace::Role, "folder.generic")?)?;
        let mut rules = FolderRules {
            steps: steps_of(&entry),
            ..FolderRules::default()
        };
        for slot in entry.slots() {
            match slot.kind {
                SlotKind::FolderEmblem => rules.symbol_box = slot.rect,
                SlotKind::SymbolBand => rules.bands.push(band_of(&slot)),
                SlotKind::FolderEffect => {
                    rules.effect = Some(Effect {
                        color: rgb(slot.color[0]),
                        highlight: rgb(slot.color[1]),
                        opacity: slot.rect[0],
                        highlight_opacity: slot.rect[1],
                        offset: slot.tracking,
                    });
                }
                SlotKind::FolderTint => {
                    rules.ramp = Some(Ramp {
                        low: slot.rect[0],
                        high: slot.rect[1],
                        shade: slot.rect[2],
                        glow: slot.rect[3],
                    });
                }
                SlotKind::TintLabel => {
                    if let Some(name) = slot.format {
                        rules.labels.push((name.to_owned(), rgb(slot.color[0])));
                    }
                }
                SlotKind::SpecialFolder => {
                    if let (Some(key), Some(symbol)) = (slot.format, slot.font) {
                        rules.specials.push((key.to_owned(), symbol.to_owned()));
                    }
                }
                _ => {}
            }
        }
        Some(rules)
    }

    /// The symbol of a special folder key (`DOCUMENTS`).
    pub fn special(&self, key: &str) -> Option<&str> {
        self.specials
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The color of a tint label (`blue`).
    pub fn label_color(&self, name: &str) -> Option<[u8; 3]> {
        self.labels.iter().find(|(k, _)| k == name).map(|(_, c)| *c)
    }
}

// ------------------------------------------------------------------------------- geometry

/// The first band whose `min_size` is at most `pt`: bands are stored from the largest.
pub fn band_for(bands: &[Band], pt: f32) -> Option<&Band> {
    bands.iter().find(|b| pt >= b.min_size)
}

/// A rectangle of the 1024 canvas on an icon `side` pixels wide.
pub fn to_px(rect: [f32; 4], side: f32) -> [f32; 4] {
    let k = side / CANVAS;
    [rect[0] * k, rect[1] * k, rect[2] * k, rect[3] * k]
}

/// `rect` scaled by `scale` around its center.
pub fn scaled_about_center(rect: [f32; 4], scale: f32) -> [f32; 4] {
    let (w, h) = (rect[2] * scale, rect[3] * scale);
    [
        rect[0] + (rect[2] - w) / 2.0,
        rect[1] + (rect[3] - h) / 2.0,
        w,
        h,
    ]
}

/// Where an app icon goes: its 1024 canvas scaled so its 824 body fills `body`, as
/// `(x, y, side)` on the 1024 canvas.
pub fn app_canvas(body: [f32; 4]) -> [f32; 3] {
    let scale = body[2] / 824.0;
    [
        body[0] - 100.0 * scale,
        body[1] - 100.0 * scale,
        CANVAS * scale,
    ]
}

/// What the label draws.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelPlan {
    pub text: String,
    /// Font size, weight and tracking on the 1024 canvas.
    pub size: f32,
    pub weight: u16,
    pub tracking: f32,
    /// Horizontal center and baseline on the 1024 canvas.
    pub center_x: f32,
    pub baseline: f32,
}

/// The text of a label: the extension in the case of the spec, at most `max_chars`
/// characters, never truncated. None for no text, too many characters or a side below
/// `min_size`.
pub fn label_text(spec: &LabelSpec, extension: &str, pt: f32) -> Option<String> {
    if pt < spec.min_size {
        return None;
    }
    let text = if spec.upper {
        extension.to_uppercase()
    } else {
        extension.to_owned()
    };
    let count = text.chars().count();
    if count == 0 || count > spec.max_chars {
        return None;
    }
    Some(text)
}

/// Places a label. `advance(text, size)` is the width of the text at `size` without tracking.
/// The size shrinks proportionally when the text is wider than `max_width`; the natural Inter
/// line (1.2102 x size) is centered vertically in the box, which puts the baseline at
/// `y + h/2 - 0.605 size + 0.96875 size`.
pub fn place_label(
    spec: &LabelSpec,
    text: String,
    advance: impl Fn(&str, f32) -> f32,
) -> LabelPlan {
    let letters = text.chars().count().saturating_sub(1) as f32;
    let width = advance(&text, spec.size) + spec.tracking * letters;
    let size = if width > spec.max_width && width > 0.0 {
        spec.size * spec.max_width / width
    } else {
        spec.size
    };
    LabelPlan {
        text,
        size,
        weight: spec.weight,
        tracking: spec.tracking,
        center_x: spec.rect[0] + spec.rect[2] / 2.0,
        baseline: spec.rect[1] + spec.rect[3] / 2.0 - 0.605 * size + 0.96875 * size,
    }
}

/// The color a tint turns `source` into: along the ramp on the luminance of `source`, from
/// the tint times `shade` to the tint lightened toward white by `glow`.
pub fn tint_color(source: [f32; 3], tint: [f32; 3], ramp: Ramp) -> [f32; 3] {
    let l = 0.2126 * source[0] + 0.7152 * source[1] + 0.0722 * source[2];
    let span = (ramp.high - ramp.low).max(f32::EPSILON);
    let t = ((l - ramp.low) / span).clamp(0.0, 1.0);
    let mut out = [0.0; 3];
    for (o, &c) in out.iter_mut().zip(&tint) {
        let dark = c * ramp.shade;
        let light = c + (1.0 - c) * ramp.glow;
        *o = dark + (light - dark) * t;
    }
    out
}

/// The same on 8-bit colors.
pub fn tint_rgb8(source: [u8; 3], tint: [u8; 3], ramp: Ramp) -> [u8; 3] {
    let f = |c: [u8; 3]| c.map(|v| f32::from(v) / 255.0);
    let out = tint_color(f(source), f(tint), ramp);
    out.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
}

// ----------------------------------------------------------------------------------- plan

/// A layer to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerDraw {
    pub role: LayerRole,
    /// The tint color and its ramp, when the tint applies.
    pub tint: Option<([u8; 3], Ramp)>,
}

/// A symbol to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct SymbolDraw {
    pub name: String,
    pub weight: String,
    /// Box in pixels of the icon.
    pub rect: [f32; 4],
    pub color: [u8; 3],
    pub opacity: f32,
    /// The engraving highlight: color, opacity and the shift down in pixels.
    pub highlight: Option<([u8; 3], f32, f32)>,
}

/// An app icon to draw in place of a symbol: its 1024 canvas as `(x, y, side)` in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct AppDraw {
    pub source: String,
    pub rect: [f32; 3],
}

/// A document foreground (an SVG of the app) fitted in a box, in pixels of the icon.
#[derive(Clone, Debug, PartialEq)]
pub struct DocDraw {
    pub source: String,
    pub rect: [f32; 4],
}

/// The label in pixels is `LabelPlan` on the canvas; the color is final.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelDraw {
    pub plan: LabelPlan,
    pub color: [u8; 3],
}

/// Everything decided about one icon at one size.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub side: u32,
    pub layers: Vec<LayerDraw>,
    /// Where the symbol or app icon goes in the stack: after the layer at this index
    /// (`layers.len()` for after all of them).
    pub glyph_at: usize,
    pub symbol: Option<SymbolDraw>,
    pub app: Option<AppDraw>,
    pub document: Option<DocDraw>,
    pub label: Option<LabelDraw>,
}

/// What a folder looks like.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FolderLook {
    pub non_empty: bool,
    pub symbol: Option<String>,
    pub tint: Option<[u8; 3]>,
}

/// The plan of a folder icon of `side` pixels, `pt` points wide.
pub fn plan_folder(rules: &FolderRules, side: u32, pt: f32, look: &FolderLook) -> Plan {
    let tint = look.tint.zip(rules.ramp);
    let mut layers = Vec::new();
    let mut symbol_tinted = false;
    let mut glyph_at = 0;
    let mut symbol_step = false;
    for step in &rules.steps {
        if step.role == LayerRole::Symbol {
            symbol_tinted = step.tint;
            symbol_step = true;
            glyph_at = layers.len();
            continue;
        }
        if step.when_not_empty && !look.non_empty {
            continue;
        }
        layers.push(LayerDraw {
            role: step.role,
            tint: if step.tint { tint } else { None },
        });
    }
    if !symbol_step {
        glyph_at = layers.len();
    }
    let side_f = side as f32;
    let symbol = look
        .symbol
        .as_ref()
        .zip(band_for(&rules.bands, pt))
        .zip(rules.effect)
        .map(|((name, band), effect)| {
            let apply = |c: [u8; 3]| match (symbol_tinted, tint) {
                (true, Some((t, ramp))) => tint_rgb8(c, t, ramp),
                _ => c,
            };
            let rect = to_px(scaled_about_center(rules.symbol_box, band.scale), side_f);
            // The shift is at least half a pixel.
            let dy = (effect.offset * side_f / CANVAS).max(0.5);
            SymbolDraw {
                name: name.clone(),
                weight: band.weight.clone(),
                rect,
                color: apply(effect.color),
                opacity: effect.opacity,
                highlight: band
                    .highlight
                    .then(|| (apply(effect.highlight), effect.highlight_opacity, dy)),
            }
        });
    Plan {
        side,
        layers,
        glyph_at,
        symbol,
        app: None,
        document: None,
        label: None,
    }
}

/// What the glyph of a file is. Never a symbol of a type: an unknown extension has none.
#[derive(Clone, Debug, PartialEq)]
pub enum Glyph {
    None,
    /// The icon of the app that opens the file, identified by `source` (where it is and when it
    /// changed).
    App(String),
    /// A document foreground the app ships for the file's type, identified by `source`.
    Document(String),
}

/// What a file is, once resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct FileLook {
    pub glyph: Glyph,
    /// The extension without the dot, lowercase, if any.
    pub extension: Option<String>,
}

/// The plan of a file icon.
pub fn plan_file(
    rules: &FileRules,
    side: u32,
    pt: f32,
    look: &FileLook,
    advance: impl Fn(&str, f32, u16) -> f32,
) -> Plan {
    let side_f = side as f32;
    let mut layers = Vec::new();
    let mut glyph_at = 0;
    let mut glyph_step = false;
    for step in &rules.steps {
        match step.role {
            LayerRole::Glyph => {
                glyph_at = layers.len();
                glyph_step = true;
            }
            LayerRole::Label | LayerRole::Symbol => {}
            role => layers.push(LayerDraw { role, tint: None }),
        }
    }
    if !glyph_step {
        glyph_at = layers.len();
    }
    let mut app = None;
    let mut document = None;
    let has_glyph = match &look.glyph {
        Glyph::App(source) => rules.app_body.is_some_and(|body| {
            let [x, y, s] = app_canvas(body);
            let k = side_f / CANVAS;
            app = Some(AppDraw {
                source: source.clone(),
                rect: [x * k, y * k, s * k],
            });
            true
        }),
        Glyph::Document(source) => {
            document = Some(DocDraw {
                source: source.clone(),
                rect: to_px(rules.glyph_box, side_f),
            });
            true
        }
        Glyph::None => false,
    };
    let text = look.extension.as_deref().unwrap_or("");
    let spec = if has_glyph {
        rules.label.as_ref()
    } else {
        rules.label_alone.as_ref()
    };
    let label = spec.and_then(|spec| {
        let text = label_text(spec, text, pt)?;
        let weight = spec.weight;
        let plan = place_label(spec, text, |t, size| advance(t, size, weight));
        Some(LabelDraw {
            plan,
            color: spec.color,
        })
    });
    Plan {
        side,
        layers,
        glyph_at,
        symbol: None,
        app,
        document,
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    /// `Text/Secondary` (light) of the Amalfi tokens.
    const GREY: [u8; 3] = [0x5f, 0x63, 0x6b];

    fn label() -> LabelSpec {
        LabelSpec {
            max_chars: 5,
            min_size: 32.0,
            rect: [219.08, 744.5, 585.84, 170.05],
            size: 139.59,
            weight: 500,
            tracking: 0.0,
            max_width: 585.84,
            upper: true,
            color: GREY,
        }
    }

    fn alone() -> LabelSpec {
        LabelSpec {
            size: 175.53,
            weight: 900,
            rect: [219.08, 362.8, 585.84, 212.83],
            max_width: 585.84,
            ..label()
        }
    }

    fn file_rules() -> FileRules {
        FileRules {
            steps: [LayerRole::Back, LayerRole::Glyph, LayerRole::Front]
                .into_iter()
                .map(|role| Step {
                    role,
                    tint: false,
                    when_not_empty: false,
                })
                .collect(),
            glyph_box: [262.0, 262.0, 500.0, 500.0],
            app_body: Some([303.67, 303.67, 416.67, 416.67]),
            label: Some(label()),
            label_alone: Some(alone()),
        }
    }

    /// Every letter 0.6 em wide.
    fn advance(text: &str, size: f32, _weight: u16) -> f32 {
        text.chars().count() as f32 * size * 0.6
    }

    #[test]
    fn boxes_scale_with_the_side() {
        for side in [16.0, 64.0, 128.0, 333.0, 1024.0, 2048.0] {
            let r = to_px([262.0, 262.0, 500.0, 500.0], side);
            let k = side / 1024.0;
            assert!(close(r[0], 262.0 * k) && close(r[2], 500.0 * k), "{side}");
            // Centered on the canvas whatever the size.
            assert!(close(r[0] + r[2] / 2.0, side / 2.0), "{side}");
        }
        // The symbol of the small folder band is 1.272 times larger around the same center.
        let r = scaled_about_center([335.04, 415.04, 353.92, 353.92], 1.272);
        assert!(close(r[0] + r[2] / 2.0, 512.0) && close(r[1] + r[3] / 2.0, 592.0));
        assert!(close(r[2], 353.92 * 1.272));
    }

    #[test]
    fn the_app_icon_body_fills_the_body_box() {
        let [x, y, side] = app_canvas([303.67, 303.67, 416.67, 416.67]);
        assert!(close(x, 253.10) && close(y, 253.10), "{x} {y}");
        assert!(close(side, 517.8), "{side}");
        // The 824 body of the scaled canvas is the box.
        let k = side / 1024.0;
        assert!(close(x + 100.0 * k, 303.67) && close(824.0 * k, 416.67));
    }

    #[test]
    fn label_text_rules() {
        let l = label();
        assert_eq!(label_text(&l, "pdf", 64.0).as_deref(), Some("PDF"));
        assert_eq!(label_text(&l, "xhtml", 64.0).as_deref(), Some("XHTML"));
        // Six characters: no label, never truncated.
        assert_eq!(label_text(&l, "sqlite", 64.0), None);
        assert_eq!(label_text(&l, "", 64.0), None);
        // Hidden below 32 pt, shown from 32.
        assert_eq!(label_text(&l, "pdf", 31.9), None);
        assert_eq!(label_text(&l, "pdf", 32.0).as_deref(), Some("PDF"));
        // The cases of non-ASCII letters count as characters, not bytes.
        assert_eq!(label_text(&l, "straße", 64.0), None);
        assert_eq!(label_text(&l, "ää", 64.0).as_deref(), Some("ÄÄ"));
    }

    #[test]
    fn labels_shrink_to_fit_and_sit_on_the_baseline() {
        let l = label();
        // The label is 25 % bigger than the former 111.67. Three letters fit at full size.
        assert!(close(l.size, 111.67 * 1.25));
        let p = place_label(&l, "PDF".to_owned(), |t, s| advance(t, s, 500));
        assert!(close(p.size, 139.59));
        assert!(close(p.baseline, 880.30), "{}", p.baseline);
        // It still sits under the glyph box (ends at 762) and inside the sheet.
        assert!(p.baseline - 0.73 * p.size > 762.0 && p.baseline < 914.0);
        assert!(close(p.center_x, 512.0));
        // Five letters at 0.6 em: 5 * 0.6 * 139.59 = 419 < 585.84: still full size. Make the
        // letters wider so that the text overflows and shrinks to exactly max_width.
        let p = place_label(&l, "WOFF2".to_owned(), |t, s| {
            t.chars().count() as f32 * s * 1.0
        });
        assert!(close(p.size, 139.59 * 585.84 / (5.0 * 139.59)));
        assert!(close(5.0 * p.size, 585.84));
        // The label alone: baseline 533.06.
        let p = place_label(&alone(), "ABC".to_owned(), |t, s| advance(t, s, 900));
        assert!(close(p.baseline, 533.06), "{}", p.baseline);
    }

    #[test]
    fn tracking_counts_in_the_width() {
        let mut l = label();
        l.tracking = 100.0;
        let p = place_label(&l, "ABCDE".to_owned(), |t, s| advance(t, s, 500));
        // 419 + 4 * 100 = 819 > 585.84.
        assert!(close(
            p.size,
            139.59 * 585.84 / (5.0 * 139.59 * 0.6 + 400.0)
        ));
    }

    fn look(glyph: Glyph, ext: &str) -> FileLook {
        FileLook {
            glyph,
            extension: (!ext.is_empty()).then(|| ext.to_owned()),
        }
    }

    #[test]
    fn a_file_with_an_app_icon_has_back_icon_front_and_a_label() {
        let rules = file_rules();
        let l = look(Glyph::App("icon:/a/icon.assets@1:2".to_owned()), "pdf");
        let p = plan_file(&rules, 256, 128.0, &l, advance);
        assert_eq!(
            p.layers.iter().map(|l| l.role).collect::<Vec<_>>(),
            [LayerRole::Back, LayerRole::Front]
        );
        // The glyph goes between the two layers.
        assert_eq!(p.glyph_at, 1);
        assert_eq!(p.label.as_ref().map(|l| l.plan.text.as_str()), Some("PDF"));
        // No symbol of a type exists for a file.
        assert!(p.app.is_some() && p.symbol.is_none() && p.document.is_none());
    }

    #[test]
    fn the_extension_label_is_the_same_grey_for_every_file() {
        let rules = file_rules();
        for glyph in [
            Glyph::None,
            Glyph::App("a".to_owned()),
            Glyph::Document("d".to_owned()),
        ] {
            let p = plan_file(&rules, 512, 256.0, &look(glyph, "abc"), advance);
            assert_eq!(p.label.as_ref().map(|l| l.color), Some(GREY));
        }
    }

    #[test]
    fn a_document_foreground_goes_in_the_glyph_box_instead_of_the_app_icon() {
        let rules = file_rules();
        let l = look(
            Glyph::Document("doc:/a/documents/x.svg@1:2".to_owned()),
            "pdf",
        );
        let p = plan_file(&rules, 512, 256.0, &l, advance);
        assert_eq!(
            p.document.as_ref().map(|d| d.rect),
            Some([131.0, 131.0, 250.0, 250.0])
        );
        assert!(p.app.is_none() && p.symbol.is_none());
        assert_eq!(p.glyph_at, 1);
        assert!(p.label.as_ref().is_some_and(|l| l.plan.weight == 500));
    }

    #[test]
    fn small_sizes_drop_the_label() {
        let rules = file_rules();
        let l = look(Glyph::App("a".to_owned()), "pdf");
        let p = plan_file(&rules, 48, 24.0, &l, advance);
        assert!(p.app.is_some() && p.label.is_none());
    }

    #[test]
    fn a_file_with_an_app_icon_uses_the_body_box() {
        let rules = file_rules();
        let l = look(Glyph::App("/a/icon.png@1".to_owned()), "txt");
        let p = plan_file(&rules, 1024, 512.0, &l, advance);
        let app = p.app.as_ref().map(|a| a.rect);
        assert!(app.is_some_and(|r| close(r[0], 253.10) && close(r[2], 517.8)));
        assert!(p.symbol.is_none());
        assert_eq!(p.label.as_ref().map(|l| l.color), Some(GREY));
        assert_eq!(p.label.as_ref().map(|l| l.plan.weight), Some(500));
    }

    #[test]
    fn without_a_glyph_the_label_stands_alone() {
        let rules = file_rules();
        let p = plan_file(&rules, 512, 256.0, &look(Glyph::None, "blend"), advance);
        let label = p
            .label
            .as_ref()
            .map(|l| (l.plan.weight, l.plan.text.as_str()));
        assert_eq!(label, Some((900, "BLEND")));
        assert!(p.symbol.is_none() && p.app.is_none() && p.document.is_none());
        // And hidden below 32 pt.
        let p = plan_file(&rules, 48, 24.0, &look(Glyph::None, "blend"), advance);
        assert!(p.label.is_none());
    }

    fn folder_rules() -> FolderRules {
        let step = |role, tint, when_not_empty| Step {
            role,
            tint,
            when_not_empty,
        };
        FolderRules {
            steps: vec![
                step(LayerRole::Back, true, false),
                step(LayerRole::Middle, false, true),
                step(LayerRole::Front, true, false),
                step(LayerRole::Symbol, true, false),
            ],
            symbol_box: [335.04, 415.04, 353.92, 353.92],
            bands: vec![
                Band {
                    min_size: 128.0,
                    weight: "light".into(),
                    scale: 1.0,
                    highlight: true,
                },
                Band {
                    min_size: 64.0,
                    weight: "regular".into(),
                    scale: 1.0,
                    highlight: true,
                },
                Band {
                    min_size: 32.0,
                    weight: "semibold".into(),
                    scale: 1.0,
                    highlight: false,
                },
                Band {
                    min_size: 16.0,
                    weight: "semibold".into(),
                    scale: 1.272,
                    highlight: false,
                },
            ],
            effect: Some(Effect {
                color: [0x1a, 0x64, 0xc8],
                highlight: [255, 255, 255],
                opacity: 0.78,
                highlight_opacity: 0.5,
                offset: 4.0,
            }),
            ramp: Some(Ramp {
                low: 0.35,
                high: 0.85,
                shade: 0.72,
                glow: 0.5,
            }),
            labels: vec![("blue".into(), [0x2f, 0x7c, 0xf6])],
            specials: vec![("DOCUMENTS".into(), "doc-text".into())],
        }
    }

    fn roles(p: &Plan) -> Vec<LayerRole> {
        p.layers.iter().map(|l| l.role).collect()
    }

    #[test]
    fn the_middle_layer_is_drawn_only_for_a_folder_with_items() {
        let rules = folder_rules();
        let mut look = FolderLook::default();
        let p = plan_folder(&rules, 256, 128.0, &look);
        assert_eq!(roles(&p), [LayerRole::Back, LayerRole::Front]);
        look.non_empty = true;
        let p = plan_folder(&rules, 256, 128.0, &look);
        assert_eq!(
            roles(&p),
            [LayerRole::Back, LayerRole::Middle, LayerRole::Front]
        );
        // The symbol goes after every layer.
        assert_eq!(p.glyph_at, 3);
    }

    #[test]
    fn the_tint_reaches_back_front_and_symbol_never_middle() {
        let rules = folder_rules();
        let look = FolderLook {
            non_empty: true,
            symbol: Some("doc-text".into()),
            tint: Some([0x2f, 0x7c, 0xf6]),
        };
        let p = plan_folder(&rules, 256, 128.0, &look);
        let tints: Vec<_> = p
            .layers
            .iter()
            .map(|l| (l.role, l.tint.is_some()))
            .collect();
        assert_eq!(
            tints,
            [
                (LayerRole::Back, true),
                (LayerRole::Middle, false),
                (LayerRole::Front, true)
            ]
        );
        // The engraving color goes through the ramp: darker than the tint, the highlight lighter.
        let s = p.symbol.as_ref();
        let ramp = rules.ramp;
        let (Some(s), Some(ramp)) = (s, ramp) else {
            panic!("symbol and ramp");
        };
        assert_eq!(
            s.color,
            tint_rgb8([0x1a, 0x64, 0xc8], [0x2f, 0x7c, 0xf6], ramp)
        );
        let lum = |c: [u8; 3]| {
            0.2126 * f32::from(c[0]) + 0.7152 * f32::from(c[1]) + 0.0722 * f32::from(c[2])
        };
        let hi = s.highlight.map(|h| h.0).unwrap_or([0; 3]);
        assert!(lum(s.color) < lum([0x2f, 0x7c, 0xf6]));
        assert!(lum(hi) > lum([0x2f, 0x7c, 0xf6]));
        // Without a tint nothing is tinted and the symbol keeps the engraving colors.
        let plain = FolderLook { tint: None, ..look };
        let p = plan_folder(&rules, 256, 128.0, &plain);
        assert!(p.layers.iter().all(|l| l.tint.is_none()));
        assert_eq!(p.symbol.as_ref().map(|s| s.color), Some([0x1a, 0x64, 0xc8]));
    }

    #[test]
    fn the_ramp_stops_at_the_extremes() {
        let ramp = Ramp {
            low: 0.35,
            high: 0.85,
            shade: 0.72,
            glow: 0.5,
        };
        let tint = [0.2, 0.4, 0.8];
        let dark = tint_color([0.0, 0.0, 0.0], tint, ramp);
        let light = tint_color([1.0, 1.0, 1.0], tint, ramp);
        for i in 0..3 {
            assert!(close(dark[i], tint[i] * 0.72));
            assert!(close(light[i], tint[i] + (1.0 - tint[i]) * 0.5));
        }
    }

    #[test]
    fn the_folder_symbol_follows_the_bands() {
        let rules = folder_rules();
        let look = FolderLook {
            symbol: Some("key".into()),
            ..FolderLook::default()
        };
        // 256 px at 2x is 128 pt: Light with the engraving highlight, 4 canvas units down.
        let p = plan_folder(&rules, 256, 128.0, &look);
        let s = p.symbol.as_ref();
        assert_eq!(s.map(|s| s.weight.as_str()), Some("light"));
        assert_eq!(s.and_then(|s| s.highlight).map(|h| h.2), Some(1.0));
        assert!(s.is_some_and(|s| {
            s.rect
                .iter()
                .zip([83.76, 103.76, 88.48, 88.48])
                .all(|(a, b)| close(*a, b))
        }));
        // 32 pt: Semibold, no highlight. 20 pt: the larger symbol. Below 16: none.
        let p = plan_folder(&rules, 64, 32.0, &look);
        assert_eq!(p.symbol.as_ref().map(|s| s.highlight.is_none()), Some(true));
        let p = plan_folder(&rules, 40, 20.0, &look);
        assert!(
            p.symbol
                .as_ref()
                .is_some_and(|s| close(s.rect[2], 353.92 * 1.272 * 40.0 / 1024.0))
        );
        assert!(plan_folder(&rules, 30, 15.0, &look).symbol.is_none());
        // On a tiny render the highlight shift is at least half a pixel.
        let p = plan_folder(&rules, 130, 65.0, &look);
        assert_eq!(
            p.symbol.as_ref().and_then(|s| s.highlight).map(|h| h.2),
            Some(0.5078125)
        );
        let p = plan_folder(&rules, 100, 100.0, &look);
        assert_eq!(
            p.symbol.as_ref().and_then(|s| s.highlight).map(|h| h.2),
            Some(0.5)
        );
    }

    #[test]
    fn the_folder_symbol_box_is_12_percent_bigger_around_the_same_center() {
        let old = [354.0_f32, 434.0, 316.0, 316.0];
        let new = folder_rules().symbol_box;
        assert!(close(new[2] / old[2], 1.12) && close(new[3] / old[3], 1.12));
        assert!(close(new[0] + new[2] / 2.0, old[0] + old[2] / 2.0));
        assert!(close(new[1] + new[3] / 2.0, old[1] + old[3] / 2.0));
    }
}
