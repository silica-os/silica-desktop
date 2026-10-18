//! Flutter embedder in Rust: a Wayland client that hosts the Flutter engine.
//!
//! - The main thread is the platform thread: it runs the Wayland loop (calloop) and
//!   executes the tasks the engine queues on it.
//! - The engine draws on its raster thread with OpenGL ES (EGL on the `wl_surface`, the
//!   swap commits) or in software: every frame arrives here over a channel and goes into a
//!   `wl_shm` buffer. Software is the default because it runs everywhere, even on headless Ossido.
//! - Pointer events become `FlutterPointerEvent` in physical pixels; keys become
//!   `FlutterKeyEvent`, and those the framework does not consume go to the active text
//!   field (`flutter/textinput`).

mod app_lifecycle;
mod appearance;
mod clipboard;
mod codec;
mod damage;
pub mod dbus;
mod drag;
mod ffi;
mod foreign;
mod glass;
mod keyboard;
mod keymap;
mod locale;
mod menu;
mod mouse_cursor;
mod popup_menu;
mod prefetch;
mod pty;
mod shell;
mod termgrid;
mod text_input;
mod trace;
mod views;
mod vulkan_drivers;
mod window;
mod window_appearance;

use std::path::PathBuf;

pub use foreign::parent_handle;
pub use window::run;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Anchors {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayerKeyboard {
    #[default]
    None,
    Exclusive,
    OnDemand,
}

/// The app as a shell surface (wlr-layer-shell) instead of a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerOptions {
    pub layer: LayerKind,
    pub anchor: Anchors,
    /// Logical pixels reserved on the anchored edge; windows do not go over them.
    pub exclusive_zone: i32,
    pub keyboard: LayerKeyboard,
    pub namespace: String,
}

/// One more shell layer drawn by the same engine (`--view`, #201): a view of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerView {
    pub layer: LayerOptions,
    /// Initial size in logical pixels (the sides anchored on both edges are the compositor's).
    pub size: (u32, u32),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Renderer {
    #[default]
    Software,
    /// OpenGL ES with Skia.
    Gl,
    /// OpenGL ES with Impeller, the shell renderer (gate 1). On a software GL driver
    /// (llvmpipe) Impeller crashes inside the engine: there it falls back to Skia.
    Impeller,
    /// Vulkan with Impeller (swapchain on the Wayland surface). Without Vulkan it falls back to
    /// OpenGL ES with Impeller.
    Vulkan,
}

/// How the views of an engine with several of them draw when the renderer is Vulkan
/// (silica-desktop#203). Measured in the VM (docs/vm.md, "Renderers with several views").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiViewRenderer {
    /// Impeller into Vulkan backing stores: needs an engine with the
    /// [`IMPELLER_VULKAN_BACKING_STORES`] patch.
    Impeller,
    /// Skia into Vulkan backing stores (every engine).
    Skia,
    /// OpenGL ES with Impeller, a blit per frame into each view's EGL surface (before #203).
    Gl,
}

/// The engine patch that gives Impeller Vulkan backing stores
/// (`crates/embedder/engine-patches`, upstream flutter/flutter#193003).
pub const IMPELLER_VULKAN_BACKING_STORES: &str = "impeller-vulkan-backing-stores";

/// The renderer of several views on Vulkan: `choice` is `SILICA_MULTI_VIEW_RENDERER`
/// (`impeller`, `skia`, `gl`), `impeller_backing_stores` whether the engine has them.
/// Impeller when it can, as one view draws; Skia otherwise.
pub fn multi_view_renderer(
    choice: Option<&str>,
    impeller_backing_stores: bool,
) -> MultiViewRenderer {
    let best = if impeller_backing_stores {
        MultiViewRenderer::Impeller
    } else {
        MultiViewRenderer::Skia
    };
    match choice.map(str::trim) {
        Some("gl") => MultiViewRenderer::Gl,
        Some("skia") => MultiViewRenderer::Skia,
        _ => best,
    }
}

/// The patches the release engine was built with: `silica-patches` next to
/// `libflutter_engine.so`, one name per line (`cargo xtask engine --release`). The engines
/// Google publishes have none.
pub fn engine_has_patch(library: &std::path::Path, patch: &str) -> bool {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(library.with_file_name("silica-patches")) else {
        return false;
    };
    let mut text = String::new();
    // A short list: anything past 4 KiB is not ours.
    if file.take(4096).read_to_string(&mut text).is_err() {
        return false;
    }
    text.lines().any(|line| line.trim() == patch)
}

/// GL drivers that rasterize on the CPU, from Mesa's `GL_RENDERER`.
pub fn is_software_gl(renderer: &str) -> bool {
    let renderer = renderer.to_ascii_lowercase();
    ["llvmpipe", "softpipe", "swrast", "lavapipe"]
        .iter()
        .any(|name| renderer.contains(name))
}

pub struct Options {
    /// The `flutter_assets` folder of the bundle.
    pub bundle: PathBuf,
    pub library: PathBuf,
    pub icu_data: PathBuf,
    /// Initial window size in logical pixels, if the compositor does not choose one.
    /// For a layer, the sides anchored on both edges are decided by the compositor.
    pub size: (u32, u32),
    pub layer: Option<LayerOptions>,
    /// More shell layers in this engine, after the main one (`--view`, #201).
    pub layer_views: Vec<LayerView>,
    pub renderer: Renderer,
    /// AOT-compiled `libapp.so`, for a release engine; ignored with a debug engine.
    pub aot: Option<PathBuf>,
    /// Options passed to the engine as they are (`--engine-arg`).
    pub engine_args: Vec<String>,
    /// Arguments of the app's `main(List<String> args)` (`--dart-arg`).
    pub dart_args: Vec<String>,
    /// Initial window title (`--title`); the app changes it with
    /// `SystemChrome.setApplicationSwitcherDescription`.
    pub title: String,
    /// Window app id (`--app-id`): ties the window to its `.desktop` entry.
    pub app_id: String,
    /// Several windows in this process (`--multi-window`, #146): the engine composes its
    /// views through backing stores, in software or with OpenGL ES (Vulkan becomes OpenGL ES
    /// with Impeller). The app opens them with `openWindow` on `silica/window`.
    pub multi_window: bool,
    /// A helper process of the app (`--helper`), such as the Open and Save dialog the portal
    /// starts: it is not the app's running instance. It never hands over to the running
    /// process, stays out of the app registry (a reopen from the Dock never reaches it) and
    /// ends with its window.
    pub helper: bool,
    /// Another app's window as the parent of this one (`--parent`, an xdg-foreign v2
    /// handle): the portal's dialog is attached to the app that asked for it.
    pub parent: Option<String>,
}

/// `target/flutter-engine/<ENGINE_VERSION>` of the workspace, where `cargo xtask engine` puts it.
pub fn default_engine_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/flutter-engine")
        .join(include_str!("../ENGINE_VERSION").trim())
}

/// The release engine built by `cargo xtask engine --release` (`docs/engine-release.md`).
pub fn release_engine_dir() -> PathBuf {
    let mut dir = default_engine_dir().into_os_string();
    dir.push("-release");
    PathBuf::from(dir)
}

#[cfg(test)]
mod tests {
    use super::{
        IMPELLER_VULKAN_BACKING_STORES, MultiViewRenderer, engine_has_patch, is_software_gl,
        multi_view_renderer,
    };

    #[test]
    fn several_views_draw_with_impeller_only_where_the_engine_can() {
        assert_eq!(multi_view_renderer(None, true), MultiViewRenderer::Impeller);
        assert_eq!(multi_view_renderer(None, false), MultiViewRenderer::Skia);
        assert_eq!(multi_view_renderer(Some("gl"), true), MultiViewRenderer::Gl);
        assert_eq!(
            multi_view_renderer(Some("skia"), true),
            MultiViewRenderer::Skia
        );
        // Impeller cannot be forced on an engine without its Vulkan backing stores.
        assert_eq!(
            multi_view_renderer(Some("impeller"), false),
            MultiViewRenderer::Skia
        );
        assert_eq!(
            multi_view_renderer(Some("nonsense"), true),
            MultiViewRenderer::Impeller
        );
    }

    #[test]
    fn engine_patches_are_read_from_next_to_the_library() {
        let dir = std::env::temp_dir().join(format!("silica-patches-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let library = dir.join("libflutter_engine.so");
        assert!(!engine_has_patch(&library, IMPELLER_VULKAN_BACKING_STORES));
        std::fs::write(
            dir.join("silica-patches"),
            format!("other\n {IMPELLER_VULKAN_BACKING_STORES} \n"),
        )
        .unwrap();
        assert!(engine_has_patch(&library, IMPELLER_VULKAN_BACKING_STORES));
        assert!(!engine_has_patch(&library, "impeller"));
        // Garbage (not UTF-8) is no patch, not a crash.
        std::fs::write(dir.join("silica-patches"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(!engine_has_patch(&library, IMPELLER_VULKAN_BACKING_STORES));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn software_gl_drivers_are_recognised() {
        assert!(is_software_gl("llvmpipe (LLVM 21.1.8, 256 bits)"));
        assert!(is_software_gl("softpipe"));
        assert!(!is_software_gl("Mesa Intel(R) Iris(R) Xe Graphics (RPL-P)"));
        assert!(!is_software_gl(
            "D3D12 (NVIDIA GeForce RTX 4070 Laptop GPU)"
        ));
        assert!(!is_software_gl(""));
    }
}
