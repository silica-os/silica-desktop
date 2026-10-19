//! Silica app bundles (silica-os `docs/app-bundles.md`): minisign-compatible keys and
//! signatures, the signed manifest of a bundle and its verification, the developer CA and the
//! revocation list, `.sxv` disk images and `.silicaapp` archives. Shared by the `silica-bundle`
//! tool, the daemon (`Trust1`, `Volumes1`, `Updates1`) and `silica-launch`.

pub mod assemble;
pub mod manifest;
pub mod minisign;
pub mod quarantine;
pub mod sxv;
pub mod trust;
pub mod zip;

/// The folder of a bundle's signature files, inside `Contents/`.
pub const SIGNATURE_DIR: &str = "_Signature";
