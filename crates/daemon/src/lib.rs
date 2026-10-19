//! Silica system services (D-Bus).
//!
//! Notifications (the `org.freedesktop.Notifications` server for apps and the
//! `dev.frascella.Silica.Notifications1` interface for the shell) and the system state for the
//! Control Center (`dev.frascella.Silica.System1`), the interface sound effects
//! (`dev.frascella.Silica.Sound1`). See `docs/daemon.md`.

pub mod actions;
pub mod apps;
pub mod auth;
pub mod clipboard;
pub mod icons;
pub mod notifications;
pub mod server;
pub mod settings;
pub mod share;
pub mod sound;
pub mod sun;
pub mod system;
pub mod thumbnails;
pub mod trust;
pub mod updates;
pub mod volumes;

pub use auth::{AuthConfig, AuthService};
pub use clipboard::ClipboardService;
pub use server::Daemon;
pub use system::{SystemConfig, SystemService};
