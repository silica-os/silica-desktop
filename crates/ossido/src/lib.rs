//! Ossido, the Wayland compositor of Silica.

mod app_frames;
mod app_hide;
mod app_menu;
pub mod backend;
mod capture;
pub mod commands;
mod config;
mod control;
#[cfg(feature = "drm")]
mod cursor;
mod cursor_theme;
mod decorations;
mod desktop_entry;
pub mod display_layout;
mod displays;
mod dmabuf;
#[cfg(feature = "drm")]
mod drm;
mod egl;
mod gestures;
mod glass;
#[cfg(test)]
mod glass_equivalence;
mod handlers;
mod idle;
mod input;
mod keep;
pub mod keys;
mod layers;
mod lock;
pub(crate) mod panorama;
mod perf;
mod popup_menu;
mod reap;
mod render;
mod resize;
mod session_bus;
mod shell;
pub mod state;
mod system;
mod thumbnail_row;
mod transient;
mod window_appearance;
mod windows;
pub mod workspaces;

use std::io::Write;

use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
use tracing::warn;

use crate::state::State;

/// Startup options, read from the command line in `main.rs`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Window inside the host compositor (WSLg, a Wayland or X11 session).
    #[default]
    Winit,
    /// No window: automated tests, CI, agent loops.
    Headless,
    /// The real session on a TTY or in a VM: DRM/KMS, libinput, libseat.
    #[cfg(feature = "drm")]
    Drm,
}

impl BackendKind {
    /// Without `--backend`: winit inside a host session (`WAYLAND_DISPLAY` or `DISPLAY` set),
    /// otherwise the real session.
    pub fn from_environment(wayland: bool, x11: bool) -> Self {
        #[cfg(feature = "drm")]
        if !wayland && !x11 {
            return BackendKind::Drm;
        }
        let _ = (wayland, x11);
        BackendKind::Winit
    }
}

#[derive(Debug)]
pub struct Options {
    pub backend: BackendKind,
    /// Size of the headless output, in pixels.
    pub size: (i32, i32),
    /// Integer scale factor of the output (HiDPI).
    pub scale: i32,
    /// Port of the control channel on 127.0.0.1; 0 picks a free port.
    pub control: Option<u16>,
    /// Commands to start as clients once the Wayland socket is open.
    pub spawn: Vec<String>,
    /// Shell components to start (`--spawn-shell`): they see the shell protocols.
    pub spawn_shell: Vec<String>,
    /// Shell components kept running (`--keep-shell`): started again when they exit.
    pub keep_shell: Vec<String>,
    /// Desktop color until there is a wallpaper (RGBA 0–1, sRGB values).
    pub background: [f32; 4],
    /// Multiplier of the animation durations (0: no animation).
    pub animation_scale: f64,
    /// Glass quality (`--glass auto|full|frosted|opaque`).
    pub glass: state::GlassSetting,
    /// Where the first window opens, from the top left of the usable area, in points
    /// (`--window-origin`, which also sets the cascade step); `None`: 4 pt and 29 pt (handlers.rs).
    pub window_origin: Option<i32>,
    /// Starts a private session bus with `silica-daemon` (`--session-bus`).
    pub session_bus: bool,
    /// A key acting as Logo (`--logo-key`): nested in another desktop, the combinations with
    /// Super are taken by that desktop. `None`: backend default (right Ctrl when nested).
    pub logo_key: Option<keys::LogoKey>,
    /// Forced appearance (`--appearance light|dark`): true = dark. `None` follows silica-daemon.
    pub appearance: Option<bool>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            backend: BackendKind::Winit,
            size: (1280, 800),
            scale: 1,
            control: None,
            spawn: Vec::new(),
            spawn_shell: Vec::new(),
            keep_shell: Vec::new(),
            background: state::CLEAR_COLOR,
            animation_scale: 1.0,
            glass: state::GlassSetting::Auto,
            window_origin: None,
            session_bus: false,
            logo_key: None,
            appearance: None,
        }
    }
}

pub fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop: EventLoop<'static, _> = EventLoop::try_new()?;
    let display: Display<State> = Display::new()?;
    let mut ossido = match options.backend {
        BackendKind::Winit => backend::init_winit(&mut event_loop, display, options.size)?,
        BackendKind::Headless => backend::init_headless(&mut event_loop, display, options.size)?,
        #[cfg(feature = "drm")]
        BackendKind::Drm => drm::init_drm(&mut event_loop, display)?,
    };

    ossido.state.clear_color = options.background;
    ossido.state.animation_speed = options.animation_scale;
    ossido.state.glass_setting = options.glass;
    if let Some(origin) = options.window_origin {
        // The cascade step is the origin (32 gives the old (32, 32), (64, 64)… layout).
        ossido.state.window_origin = origin;
        ossido.state.cascade_step = origin;
    }
    ossido.state.appearance = options.appearance;
    ossido.state.logo_key = options.logo_key.unwrap_or(match options.backend {
        BackendKind::Winit => keys::LogoKey::RightCtrl,
        BackendKind::Headless => keys::LogoKey::None,
        #[cfg(feature = "drm")]
        BackendKind::Drm => keys::LogoKey::None,
    });
    if options.scale != 1 {
        ossido.state.output.change_current_state(
            None,
            None,
            Some(smithay::output::Scale::Integer(options.scale)),
            None,
        );
    }

    // The user's settings (keyboard.json, …), applied now and whenever Settings changes them.
    if let Some(dir) = config::config_dir() {
        match config::watch(&event_loop.handle(), &dir) {
            Ok(()) => {
                ossido.state.config_dir = Some(dir);
                ossido.state.apply_all_config();
            }
            Err(err) => warn!("settings folder: {err}"),
        }
    }

    // Energy: the idle times of power.json.
    event_loop.handle().insert_source(
        smithay::reexports::calloop::timer::Timer::from_duration(idle::CHECK),
        |_, _, ossido| {
            ossido.state.idle_check();
            smithay::reexports::calloop::timer::TimeoutAction::ToDuration(idle::CHECK)
        },
    )?;

    // The lines on stdout are the interface for scripts and tests: one per variable.
    let socket = ossido.state.socket_name.clone();
    println!("WAYLAND_DISPLAY={}", socket.to_string_lossy());
    // Lives until the end of `run`: closes the bus and the daemon on exit.
    let _session_bus = if options.session_bus {
        let bus = session_bus::SessionBus::start(&socket)?;
        println!("DBUS_SESSION_BUS_ADDRESS={}", bus.address);
        ossido.state.session_bus = Some(bus.address.clone());
        Some(bus)
    } else {
        None
    };
    ossido.state.system = system::SystemLink::start(ossido.state.session_bus.clone());
    if let Some(port) = options.control {
        let addr = control::listen(&event_loop.handle(), port)?;
        println!("OSSIDO_CONTROL={addr}");
    }
    // The actions socket, always: the Apps, Mission Control and Screenshot applications use it.
    if let Some(path) = silica_control::actions::socket_path(&socket.to_string_lossy())
        && let Err(err) = control::listen_actions(&event_loop.handle(), &path)
    {
        warn!("actions socket {}: {err}", path.display());
    }
    std::io::stdout().flush()?;
    for command in &options.spawn {
        if let Err(err) = ossido.state.launch(command) {
            warn!("{err}");
        }
    }
    for command in &options.spawn_shell {
        if let Err(err) = ossido.state.launch_with(command, true) {
            warn!("{err}");
        }
    }
    for command in &options.keep_shell {
        ossido.state.keep_shell(command);
    }

    event_loop.run(None, &mut ossido, |ossido| {
        ossido.state.space.refresh();
        ossido.state.forget_dead_windows();
        ossido.state.popups.cleanup();
        if let Err(err) = ossido.display.flush_clients() {
            warn!("client flush failed: {err}");
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::BackendKind;

    #[test]
    fn a_host_session_means_winit() {
        assert_eq!(
            BackendKind::from_environment(true, false),
            BackendKind::Winit
        );
        assert_eq!(
            BackendKind::from_environment(false, true),
            BackendKind::Winit
        );
    }

    #[cfg(feature = "drm")]
    #[test]
    fn a_tty_means_drm() {
        assert_eq!(
            BackendKind::from_environment(false, false),
            BackendKind::Drm
        );
    }
}
