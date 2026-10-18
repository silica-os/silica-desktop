//! `icon.assets`: one app icon, compiled (docs/assets-format.md, "App icon file").
//!
//! The source is a silica-icons app folder (`icon.json` format 1 or 2, docs/icon-format.md).
//! The compiler ([`compile`](crate::compile::app_icon)) resolves every default (role modes,
//! opacity factors, dark overrides) so that a reader has no JSON and no rules to apply: for each
//! layer there is one [`Form`] for the light forms and one for the dark forms. The SVG layers are
//! stored as they are, in a blob table.
//!
//! Layout (little endian): `"SAIC"`, `u32` version (1), `u64` digest (xxh3 of the payload), then
//! the payload: two [`Background`]s (light, dark), the glass tuning (3 `f32`), `u32` layer count
//! and the layers, `u32` blob count and the blobs (`u32` length, bytes). Decoding checks every
//! length and never panics; it fails with `None` on malformed input.

use xxhash_rust::xxh3::xxh3_64;

pub const MAGIC: &[u8; 4] = b"SAIC";
pub const VERSION: u32 = 2;
/// More layers than this are refused (the format allows 9; format 1 has one).
pub const MAX_LAYERS: usize = 16;
/// A blob (SVG) bigger than this is refused.
pub const MAX_BLOB: usize = 16 * 1024 * 1024;

/// How a layer's pixels are transformed in clear and tinted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Ramp,
    Light,
    Glass,
    Accent,
    Keep,
    Hide,
}

impl Mode {
    const ALL: [Mode; 6] = [
        Mode::Ramp,
        Mode::Light,
        Mode::Glass,
        Mode::Accent,
        Mode::Keep,
        Mode::Hide,
    ];

    pub fn parse(name: &str) -> Option<Mode> {
        Some(match name {
            "ramp" => Mode::Ramp,
            "light" => Mode::Light,
            "glass" => Mode::Glass,
            "accent" => Mode::Accent,
            "keep" => Mode::Keep,
            "hide" => Mode::Hide,
            _ => return None,
        })
    }

    fn from_u8(v: u8) -> Option<Mode> {
        Self::ALL.get(usize::from(v)).copied()
    }

    fn to_u8(self) -> u8 {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0) as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Multiply,
    Screen,
}

impl Blend {
    pub fn parse(name: &str) -> Option<Blend> {
        Some(match name {
            "normal" => Blend::Normal,
            "multiply" => Blend::Multiply,
            "screen" => Blend::Screen,
            _ => return None,
        })
    }

    fn from_u8(v: u8) -> Option<Blend> {
        Some(match v {
            0 => Blend::Normal,
            1 => Blend::Multiply,
            2 => Blend::Screen,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Body,
    Detail,
    Highlight,
    Dynamic,
}

impl Role {
    pub fn parse(name: &str) -> Option<Role> {
        Some(match name {
            "body" => Role::Body,
            "detail" => Role::Detail,
            "highlight" => Role::Highlight,
            "dynamic" => Role::Dynamic,
            _ => return None,
        })
    }

    fn from_u8(v: u8) -> Option<Role> {
        Some(match v {
            0 => Role::Body,
            1 => Role::Detail,
            2 => Role::Highlight,
            3 => Role::Dynamic,
            _ => return None,
        })
    }
}

/// The cast shadow the host draws under a layer (the composer of the daemon does not).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shadow {
    None,
    Neutral,
    Chromatic,
}

impl Shadow {
    pub fn parse(name: &str) -> Option<Shadow> {
        Some(match name {
            "none" => Shadow::None,
            "neutral" => Shadow::Neutral,
            "chromatic" => Shadow::Chromatic,
            _ => return None,
        })
    }

    fn from_u8(v: u8) -> Option<Shadow> {
        Some(match v {
            0 => Shadow::None,
            1 => Shadow::Neutral,
            2 => Shadow::Chromatic,
            _ => return None,
        })
    }
}

/// What a layer does in one appearance (clear or tinted), defaults resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModeSpec {
    pub mode: Mode,
    /// Multiplies the layer opacity.
    pub opacity: f32,
    /// Replaces the ends of the ramp: grey levels 0..255 in clear, positions 0..1 in tinted.
    pub luma: Option<[f32; 2]>,
}

/// A layer as drawn in the light forms or in the dark forms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Form {
    /// Index of the SVG in the blob table.
    pub svg: u32,
    pub opacity: f32,
    pub blend: Blend,
    pub shadow: Shadow,
    /// The layer is drawn only when the icon side in pt is at least this.
    pub min_size: f32,
    pub clear: ModeSpec,
    pub tinted: ModeSpec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicKind {
    ClockHand,
    ClockCap,
    CalendarMonth,
    CalendarDay,
}

impl DynamicKind {
    pub fn parse(name: &str) -> Option<DynamicKind> {
        Some(match name {
            "clock-hand" => DynamicKind::ClockHand,
            "clock-cap" => DynamicKind::ClockCap,
            "calendar-month" => DynamicKind::CalendarMonth,
            "calendar-day" => DynamicKind::CalendarDay,
            _ => return None,
        })
    }

    fn from_u8(v: u8) -> Option<DynamicKind> {
        Some(match v {
            0 => DynamicKind::ClockHand,
            1 => DynamicKind::ClockCap,
            2 => DynamicKind::CalendarMonth,
            3 => DynamicKind::CalendarDay,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandUnit {
    Hour,
    Minute,
    Second,
}

impl HandUnit {
    pub fn parse(name: &str) -> Option<HandUnit> {
        Some(match name {
            "hour" => HandUnit::Hour,
            "minute" => HandUnit::Minute,
            "second" => HandUnit::Second,
            _ => return None,
        })
    }
}

/// Letter case of a dynamic text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextCase {
    AsIs,
    Upper,
    Lower,
}

impl TextCase {
    pub fn parse(name: &str) -> Option<TextCase> {
        Some(match name {
            "upper" => TextCase::Upper,
            "lower" => TextCase::Lower,
            _ => return None,
        })
    }

    fn from_u8(v: u8) -> Option<TextCase> {
        Some(match v {
            0 => TextCase::AsIs,
            1 => TextCase::Upper,
            2 => TextCase::Lower,
            _ => return None,
        })
    }

    fn to_u8(self) -> u8 {
        match self {
            TextCase::AsIs => 0,
            TextCase::Upper => 1,
            TextCase::Lower => 2,
        }
    }
}

/// When a dynamic text is redrawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    Never,
    /// At local midnight and on a date or time zone change.
    Midnight,
}

impl Refresh {
    pub fn parse(name: &str) -> Option<Refresh> {
        Some(match name {
            "never" => Refresh::Never,
            "midnight" => Refresh::Midnight,
            _ => return None,
        })
    }

    fn from_u8(v: u8) -> Option<Refresh> {
        Some(match v {
            0 => Refresh::Never,
            1 => Refresh::Midnight,
            _ => return None,
        })
    }
}

/// The text fields of a calendar layer (`calendar-month`, `calendar-day`): the host draws the
/// localized month or day in [`rect`](Self::rect), centred, instead of the layer's SVG (which is
/// the text at the design pose, for the fallback).
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicText {
    pub font: String,
    pub size: f32,
    pub weight: u16,
    pub tracking: f32,
    /// Light and dark colour.
    pub color: [[u8; 3]; 2],
    /// `[x, y, width, height]` on the 1024 canvas.
    pub rect: [f32; 4],
    pub case: TextCase,
    /// `short` (three letters), `long` or `narrow` for a month; empty for a day.
    pub format: String,
    pub refresh: Refresh,
}

/// The data of a layer the host draws live.
#[derive(Clone, Debug, PartialEq)]
pub struct Dynamic {
    pub kind: DynamicKind,
    pub unit: Option<HandUnit>,
    pub pivot: [f32; 2],
    /// Only for the calendar kinds.
    pub text: Option<DynamicText>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub id: String,
    pub role: Role,
    pub dynamic: Option<Dynamic>,
    /// Light and dark forms (equal when the layer has no dark override).
    pub forms: [Form; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    /// Vertical gradient over the body height.
    Gradient([u8; 3], [u8; 3]),
    /// Index of an SVG in the blob table.
    Svg(u32),
}

/// A compiled app icon.
#[derive(Clone, Debug, PartialEq)]
pub struct AppIcon {
    /// Light and dark background.
    pub background: [Background; 2],
    /// Tuning of the `glass` mode: opacity at the top, at the bottom, blur sigma (canvas units).
    pub glass: [f32; 3],
    pub layers: Vec<Layer>,
    pub blobs: Vec<Vec<u8>>,
}

struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
    }
}

struct In<'a>(&'a [u8]);

impl<'a> In<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(n)?;
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn rgb(&mut self) -> Option<[u8; 3]> {
        self.take(3)?.try_into().ok()
    }
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        if n > MAX_BLOB {
            return None;
        }
        self.take(n)
    }
}

fn put_background(o: &mut Out, b: &Background) {
    match b {
        Background::Gradient(top, bottom) => {
            o.u8(0);
            o.0.extend_from_slice(top);
            o.0.extend_from_slice(bottom);
        }
        Background::Svg(i) => {
            o.u8(1);
            o.u32(*i);
        }
    }
}

fn get_background(i: &mut In) -> Option<Background> {
    match i.u8()? {
        0 => Some(Background::Gradient(i.rgb()?, i.rgb()?)),
        1 => Some(Background::Svg(i.u32()?)),
        _ => None,
    }
}

fn put_spec(o: &mut Out, s: &ModeSpec) {
    o.u8(s.mode.to_u8());
    o.f32(s.opacity);
    match s.luma {
        Some([lo, hi]) => {
            o.u8(1);
            o.f32(lo);
            o.f32(hi);
        }
        None => o.u8(0),
    }
}

fn get_spec(i: &mut In) -> Option<ModeSpec> {
    let mode = Mode::from_u8(i.u8()?)?;
    let opacity = i.f32()?;
    let luma = match i.u8()? {
        0 => None,
        1 => Some([i.f32()?, i.f32()?]),
        _ => return None,
    };
    Some(ModeSpec {
        mode,
        opacity,
        luma,
    })
}

fn put_form(o: &mut Out, f: &Form) {
    o.u32(f.svg);
    o.f32(f.opacity);
    o.u8(match f.blend {
        Blend::Normal => 0,
        Blend::Multiply => 1,
        Blend::Screen => 2,
    });
    o.u8(match f.shadow {
        Shadow::None => 0,
        Shadow::Neutral => 1,
        Shadow::Chromatic => 2,
    });
    o.f32(f.min_size);
    put_spec(o, &f.clear);
    put_spec(o, &f.tinted);
}

fn get_form(i: &mut In) -> Option<Form> {
    Some(Form {
        svg: i.u32()?,
        opacity: i.f32()?,
        blend: Blend::from_u8(i.u8()?)?,
        shadow: Shadow::from_u8(i.u8()?)?,
        min_size: i.f32()?,
        clear: get_spec(i)?,
        tinted: get_spec(i)?,
    })
}

impl AppIcon {
    /// The bytes of `icon.assets`. Deterministic.
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Out(Vec::new());
        for b in &self.background {
            put_background(&mut o, b);
        }
        for v in self.glass {
            o.f32(v);
        }
        o.u32(self.layers.len() as u32);
        for layer in &self.layers {
            o.bytes(layer.id.as_bytes());
            o.u8(match layer.role {
                Role::Body => 0,
                Role::Detail => 1,
                Role::Highlight => 2,
                Role::Dynamic => 3,
            });
            match &layer.dynamic {
                Some(d) => {
                    o.u8(1);
                    o.u8(match d.kind {
                        DynamicKind::ClockHand => 0,
                        DynamicKind::ClockCap => 1,
                        DynamicKind::CalendarMonth => 2,
                        DynamicKind::CalendarDay => 3,
                    });
                    o.u8(match d.unit {
                        None => 0,
                        Some(HandUnit::Hour) => 1,
                        Some(HandUnit::Minute) => 2,
                        Some(HandUnit::Second) => 3,
                    });
                    o.f32(d.pivot[0]);
                    o.f32(d.pivot[1]);
                    match &d.text {
                        Some(t) => {
                            o.u8(1);
                            o.bytes(t.font.as_bytes());
                            o.f32(t.size);
                            o.u32(u32::from(t.weight));
                            o.f32(t.tracking);
                            for c in t.color {
                                o.0.extend_from_slice(&c);
                            }
                            for v in t.rect {
                                o.f32(v);
                            }
                            o.u8(t.case.to_u8());
                            o.bytes(t.format.as_bytes());
                            o.u8(match t.refresh {
                                Refresh::Never => 0,
                                Refresh::Midnight => 1,
                            });
                        }
                        None => o.u8(0),
                    }
                }
                None => o.u8(0),
            }
            for form in &layer.forms {
                put_form(&mut o, form);
            }
        }
        o.u32(self.blobs.len() as u32);
        for blob in &self.blobs {
            o.bytes(blob);
        }
        let mut out = Vec::with_capacity(o.0.len() + 16);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&xxh3_64(&o.0).to_le_bytes());
        out.extend_from_slice(&o.0);
        out
    }

    /// Reads `icon.assets` bytes; None when they are malformed, of another version, or refer
    /// to a blob that is not there.
    pub fn decode(bytes: &[u8]) -> Option<AppIcon> {
        let mut head = In(bytes);
        if head.take(4)? != MAGIC || head.u32()? != VERSION {
            return None;
        }
        let digest = u64::from_le_bytes(head.take(8)?.try_into().ok()?);
        let payload = head.0;
        if xxh3_64(payload) != digest {
            return None;
        }
        let mut i = In(payload);
        let background = [get_background(&mut i)?, get_background(&mut i)?];
        let glass = [i.f32()?, i.f32()?, i.f32()?];
        let count = i.u32()? as usize;
        if count > MAX_LAYERS {
            return None;
        }
        let mut layers = Vec::with_capacity(count);
        for _ in 0..count {
            let id = String::from_utf8(i.bytes()?.to_vec()).ok()?;
            let role = Role::from_u8(i.u8()?)?;
            let dynamic = match i.u8()? {
                0 => None,
                1 => {
                    let kind = DynamicKind::from_u8(i.u8()?)?;
                    let unit = match i.u8()? {
                        0 => None,
                        1 => Some(HandUnit::Hour),
                        2 => Some(HandUnit::Minute),
                        3 => Some(HandUnit::Second),
                        _ => return None,
                    };
                    let pivot = [i.f32()?, i.f32()?];
                    let text = match i.u8()? {
                        0 => None,
                        1 => {
                            let font = String::from_utf8(i.bytes()?.to_vec()).ok()?;
                            let size = i.f32()?;
                            let weight = u16::try_from(i.u32()?).ok()?;
                            let tracking = i.f32()?;
                            let color = [i.rgb()?, i.rgb()?];
                            let rect = [i.f32()?, i.f32()?, i.f32()?, i.f32()?];
                            let case = TextCase::from_u8(i.u8()?)?;
                            let format = String::from_utf8(i.bytes()?.to_vec()).ok()?;
                            let refresh = Refresh::from_u8(i.u8()?)?;
                            Some(DynamicText {
                                font,
                                size,
                                weight,
                                tracking,
                                color,
                                rect,
                                case,
                                format,
                                refresh,
                            })
                        }
                        _ => return None,
                    };
                    Some(Dynamic {
                        kind,
                        unit,
                        pivot,
                        text,
                    })
                }
                _ => return None,
            };
            let forms = [get_form(&mut i)?, get_form(&mut i)?];
            layers.push(Layer {
                id,
                role,
                dynamic,
                forms,
            });
        }
        let blob_count = i.u32()? as usize;
        if blob_count > 4 * MAX_LAYERS {
            return None;
        }
        let mut blobs = Vec::with_capacity(blob_count);
        for _ in 0..blob_count {
            blobs.push(i.bytes()?.to_vec());
        }
        let icon = AppIcon {
            background,
            glass,
            layers,
            blobs,
        };
        icon.references_valid().then_some(icon)
    }

    fn references_valid(&self) -> bool {
        let n = self.blobs.len();
        let ok = |i: u32| (i as usize) < n;
        self.background.iter().all(|b| match b {
            Background::Svg(i) => ok(*i),
            Background::Gradient(..) => true,
        }) && self
            .layers
            .iter()
            .all(|l| l.forms.iter().all(|f| ok(f.svg)))
    }

    /// The SVG at a blob index.
    pub fn blob(&self, index: u32) -> Option<&[u8]> {
        self.blobs.get(index as usize).map(Vec::as_slice)
    }

    /// The digest of the payload: it names the icon in caches.
    pub fn digest(bytes: &[u8]) -> Option<u64> {
        let mut head = In(bytes);
        if head.take(4)? != MAGIC || head.u32()? != VERSION {
            return None;
        }
        Some(u64::from_le_bytes(head.take(8)?.try_into().ok()?))
    }
}
