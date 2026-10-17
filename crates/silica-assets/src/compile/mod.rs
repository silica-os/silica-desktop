//! The compiler: reads the sources (silica-icons, fonts, sounds, wallpapers and the type table
//! `data/types.toml`) and writes the bytes of a `silica.assets`. Deterministic: sources are read
//! in sorted order and no clock, thread or random number takes part in the output.

mod app_icon;
mod builder;
mod raster;
mod sources;

use std::fmt;
use std::path::PathBuf;

pub use app_icon::app_icon;
pub use builder::{Builder, SlotSpec, VariantSpec};
pub use raster::{Image, Rendition, ladder};

/// Why the sources could not be compiled.
#[derive(Debug)]
pub struct CompileError(String);

impl CompileError {
    pub(crate) fn new(message: impl Into<String>) -> CompileError {
        CompileError(message.into())
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CompileError {}

/// Where the sources are.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// silica-icons checkout (`apps/`, `files/`, `badges/`, `symbols/`).
    pub icons: PathBuf,
    /// silica-brand checkout; its `sounds` are used when present.
    pub brand: Option<PathBuf>,
    /// silica-wallpapers checkout: every `<set>/wallpapers.json` (`amalfi/`) with its images
    /// and previews.
    pub wallpapers: Option<PathBuf>,
    /// A folder with `.ttf` / `.otf` files.
    pub fonts: Option<PathBuf>,
    /// A folder with `.wav` / `.ogg` / `.flac` files (default: `<brand>/sounds` if it exists).
    pub sounds: Option<PathBuf>,
    /// The MIME / UTI / extension table (default: the one in this crate, `data/types.toml`).
    pub types: Option<PathBuf>,
    /// The composition rules of the generic file and folder (default: `data/compose.toml`).
    pub compose: Option<PathBuf>,
    /// Written in the header (default: `silica-assets <crate version>`).
    pub content_version: Option<String>,
}

/// What a compilation found, for the log.
#[derive(Debug, Default)]
pub struct Report {
    pub apps: usize,
    pub files: usize,
    pub badges: usize,
    pub symbols: usize,
    pub fonts: usize,
    pub sounds: usize,
    pub wallpapers: usize,
    pub aliases: usize,
    pub warnings: Vec<String>,
}

/// The type table shipped with the crate.
pub const DEFAULT_TYPES: &str = include_str!("../../data/types.toml");

/// The composition rules shipped with the crate.
pub const DEFAULT_COMPOSE: &str = include_str!("../../data/compose.toml");

/// Compiles the sources into the bytes of a `silica.assets`.
pub fn compile(options: &Options) -> Result<(Vec<u8>, Report), CompileError> {
    sources::compile(options)
}
