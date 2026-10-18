//! The on-disk layout of `silica.assets` (docs/assets-format.md): constants, the enums stored
//! in the records and the little-endian readers and writers shared by the reader and the
//! compiler.

/// The first eight bytes of the file.
pub const MAGIC: [u8; 8] = *b"SILASSET";
/// The only format version this crate reads and writes.
pub const FORMAT: u32 = 1;

/// Size of the header.
pub const HEADER_SIZE: usize = 128;
/// Size of a section table record.
pub const SECTION_SIZE: usize = 24;
/// Size of an [`Entry`](crate::Entry) record.
pub const ENTRY_SIZE: usize = 64;
/// Size of a variant record.
pub const VARIANT_SIZE: usize = 32;
/// Size of a slot record.
pub const SLOT_SIZE: usize = 48;
/// Every section and every data blob starts on a multiple of this.
pub const ALIGN: usize = 64;
/// "No entry": the value of `alias_of` for a real entry.
pub const NONE: u32 = u32::MAX;

pub(crate) const SECTION_STRINGS: u32 = 1;
pub(crate) const SECTION_ENTRIES: u32 = 2;
pub(crate) const SECTION_VARIANTS: u32 = 3;
pub(crate) const SECTION_SLOTS: u32 = 4;
pub(crate) const SECTION_DATA: u32 = 5;
/// The lookup table of key space `n` has section kind `SECTION_LOOKUP + n`.
pub(crate) const SECTION_LOOKUP: u32 = 16;

macro_rules! byte_enum {
    ($(#[$meta:meta])* $name:ident: $repr:ty { $($(#[$vmeta:meta])* $variant:ident = $value:expr),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr($repr)]
        pub enum $name {
            $($(#[$vmeta])* $variant = $value),+
        }

        impl $name {
            /// Every value, in numeric order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The enum for a stored number, `None` for a number this version does not know.
            pub fn from_u32(value: u32) -> Option<$name> {
                match value {
                    $(v if v == $value => Some($name::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

byte_enum! {
    /// The lookup tables: one per kind of key.
    KeySpace: u8 {
        /// Application id, e.g. `dev.frascella.Mail` (or the icon id, e.g. `mail`).
        App = 0,
        /// File type id, the name of an icon of `silica-icons/files`, e.g. `file-pdf`.
        Type = 1,
        /// MIME type (or a wildcard `image/*`) to a type.
        Mime = 2,
        /// Uniform Type Identifier alias to a type.
        Uti = 3,
        /// Role: `folder`, `volume.external`, `trash.empty`, `badge.alias`, `dock.stack-close`.
        Role = 4,
        /// `<weight>/<name>`, e.g. `regular/key`.
        Symbol = 5,
        /// Font name, e.g. `Inter-Regular`.
        Font = 6,
        /// Sound name, e.g. `alert-drop`.
        Sound = 7,
        /// Wallpaper name, e.g. `silica`.
        Wallpaper = 8,
        /// File name extension without the dot, lowercase, to a type.
        Ext = 9,
    }
}

byte_enum! {
    /// What an entry is.
    EntryKind: u16 {
        App = 0,
        File = 1,
        Folder = 2,
        Volume = 3,
        Badge = 4,
        Symbol = 5,
        Font = 6,
        Sound = 7,
        Wallpaper = 8,
        /// A key that points to another entry (`alias_of`): MIME, UTI, extension and role names.
        Alias = 9,
        /// Reserved: no entry has it. It was a kind (`kind.pdf`) or a category of files; a file's
        /// icon is now its app's (docs/thumbnails.md).
        FileKind = 10,
    }
}

byte_enum! {
    /// The theme a variant is drawn for.
    Theme: u8 {
        Default = 0,
        Dark = 1,
        Clear = 2,
        ClearDark = 3,
        Tinted = 4,
        TintedDark = 5,
    }
}

byte_enum! {
    /// Which layer of the icon a variant holds.
    Layer: u8 {
        /// The whole icon (or the whole asset, for fonts, sounds and symbols).
        Composite = 0,
        HandHour = 1,
        HandMinute = 2,
        HandSecond = 3,
        Cap = 4,
        /// The back layer of a layered icon (generic file and folder: `back`).
        Background = 5,
        /// The front layer of a layered icon (generic file and folder: `front`).
        Foreground = 6,
        /// The middle layer of the generic folder (the sheet that shows when it has items).
        Middle = 7,
    }
}

byte_enum! {
    /// How the bytes of a variant are encoded.
    Encoding: u8 {
        /// Premultiplied RGBA, 8 bits per channel, `size_px` x `size_px`, no header.
        Rgba8Premul = 0,
        /// QOI of premultiplied RGBA.
        Qoi = 1,
        Ttf = 2,
        Otf = 3,
        Ogg = 4,
        Flac = 5,
        Svg = 6,
        /// A PNG file (wallpapers: large, smooth images that QOI compresses badly).
        Png = 7,
        Wav = 8,
        /// A JPEG file (wallpaper previews).
        Jpeg = 9,
    }
}

byte_enum! {
    /// The state of an icon with a "full" look.
    State: u8 {
        Normal = 0,
        /// A folder with something inside.
        Full = 1,
    }
}

byte_enum! {
    /// What a dynamic slot draws.
    SlotKind: u8 {
        ClockHand = 0,
        ClockCap = 1,
        CalendarMonth = 2,
        CalendarDay = 3,
        FolderEmblem = 4,
        /// The extension label of the generic file.
        TypeLabel = 5,
        /// Where a badge sits on an icon (`unit` is the [`Corner`]).
        Badge = 6,
        /// A color of the generic file's label for a category (`format` is the category).
        TypeCategory = 7,
        /// The display title of an entry (`format` is the title): wallpapers.
        Title = 8,
        /// One step of the stack of a layered icon, in drawing order (`unit` is the
        /// [`LayerRole`], `weight` the `LAYER_*` flags).
        LayerRule = 9,
        /// The glyph box of the generic file (`rect`).
        GlyphBox = 10,
        /// A band of symbol sizes (of the folder): from `min_size` pt, `format` is the weight name (`light`,
        /// `regular`, `semibold`), `rect` = (scale, highlight 0 or 1, 0, 0).
        SymbolBand = 11,
        /// Where the body of an app icon goes when it replaces the symbol (`rect`).
        AppIconBody = 12,
        /// The extension label next to a glyph: `unit` max characters, `min_size` smallest
        /// icon side in pt, `rect` box, `font` family, `size` in 1/64 of a canvas unit,
        /// `weight`, `format` the case, `tracking`, `max_width`, `color[0]` the text colour.
        FileLabel = 13,
        /// Reserved, no longer written (was the extension label alone; an unknown extension now
        /// shows the `unknown` system glyph with [`SlotKind::FileLabel`]).
        FileLabelAlone = 14,
        /// Reserved, no longer written (was the symbol and color of a kind of file).
        KindStyle = 15,
        /// Reserved, no longer written (was the fallback kind of the generic file).
        FallbackKind = 16,
        /// The engraving of the folder symbol: `color[0]` the color, `color[1]` the highlight
        /// (both `0xRRGGBB00`), `rect` = (opacity, highlight opacity, 0, 0), `tracking` the
        /// offset on the 1024 canvas.
        FolderEffect = 17,
        /// The tint ramp of the folder: `rect` = (luminance low, luminance high, shade, glow).
        FolderTint = 18,
        /// A named tint color of the folder (`format` the name, `color[0]` the color).
        TintLabel = 19,
        /// A special folder (`format` the key: `DOCUMENTS`, `HOME`...) and its symbol (`font`).
        SpecialFolder = 20,
        /// A structural file type with a system foreground (`font` the name, `format` the
        /// pattern: a MIME type, `major/*` or `.ext`). The artwork is the entry
        /// `file.system.<name>` of the `Role` key space, when the build had it.
        SystemType = 21,
    }
}

byte_enum! {
    /// The `unit` of a [`SlotKind::LayerRule`]: what is drawn at that step.
    LayerRole: u8 {
        Back = 0,
        Middle = 1,
        Front = 2,
        /// The glyph of the generic file: a symbol or an app icon.
        Glyph = 3,
        /// The engraved symbol of the generic folder.
        Symbol = 4,
        /// The extension label of the generic file.
        Label = 5,
    }
}

byte_enum! {
    /// The `unit` of a clock hand slot.
    Unit: u8 {
        Hour = 0,
        Minute = 1,
        Second = 2,
    }
}

byte_enum! {
    /// The `unit` of a badge slot.
    Corner: u8 {
        TopLeft = 0,
        TopRight = 1,
        BottomLeft = 2,
        BottomRight = 3,
    }
}

/// `LayerRule` flag: the step goes through the tint ramp of the folder.
pub const LAYER_TINT: u16 = 1;
/// `LayerRule` flag: the step is drawn only when the folder has at least one item.
pub const LAYER_WHEN_NOT_EMPTY: u16 = 2;

/// Entry flag: the entry has dynamic slots.
pub const FLAG_HAS_DYNAMIC: u16 = 1;
/// Entry flag: the image is a template the shell may tint (symbols).
pub const FLAG_TINTABLE: u16 = 2;
/// Entry flag: the entry has dark variants.
pub const FLAG_HAS_DARK: u16 = 4;

/// The pixel sides stored as raw premultiplied RGBA.
pub const RAW_SIDES: [u16; 4] = [16, 32, 64, 128];
/// The pixel sides stored as QOI.
pub const QOI_SIDES: [u16; 3] = [256, 512, 1024];

/// Rounds `value` up to a multiple of [`ALIGN`].
pub fn align_up(value: usize) -> usize {
    value.div_ceil(ALIGN).saturating_mul(ALIGN)
}

pub(crate) fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

pub(crate) fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    Some(u64::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

pub(crate) fn f32_at(bytes: &[u8], offset: usize) -> Option<f32> {
    u32_at(bytes, offset).map(f32::from_bits)
}
