//! `silica.assets`: the compiled, memory-mapped system assets (docs/assets-format.md).
//!
//! - [`Assets::open`] maps a file read-only and checks it; lookups, variant picks and blobs are
//!   borrowed slices of the mapping. Malformed input returns an [`Error`], never a panic.
//! - [`ffi`] is the C ABI of `libsilica_assets.so` (header: `include/silica_assets.h`).
//! - [`compile`] (feature `compile`, on by default) turns the sources (silica-icons, fonts,
//!   sounds, wallpapers) into the file.

pub mod appicon;
pub mod ffi;
pub mod format;
mod mph;
mod reader;

#[cfg(feature = "compose")]
pub mod compose;

#[cfg(feature = "compile")]
pub mod compile;

pub use format::{
    Corner, Encoding, EntryKind, FLAG_HAS_DARK, FLAG_HAS_DYNAMIC, FLAG_TINTABLE, FORMAT, KeySpace,
    LAYER_TINT, LAYER_WHEN_NOT_EMPTY, Layer, LayerRole, NONE, SlotKind, State, Theme, Unit,
};
pub use reader::{Assets, Entry, EntryId, Error, Query, Slot, Variant};
