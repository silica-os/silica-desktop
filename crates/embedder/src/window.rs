//! The Wayland window hosting the engine, and the platform thread loop.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use silica_protocols::glass::client::silica_glass_manager_v1::SilicaGlassManagerV1;
use silica_protocols::glass::client::silica_glass_v1::SilicaGlassV1;
use silica_protocols::menu::client::silica_menu_manager_v1::SilicaMenuManagerV1;
use silica_protocols::menu::client::silica_menu_v1::{self, SilicaMenuV1};
use silica_protocols::shell::client::silica_shell_manager_v1::{
    Outside, Rule, SilicaShellManagerV1,
};
use silica_protocols::window::client::silica_window_manager_v1::SilicaWindowManagerV1;
use silica_protocols::window::client::silica_window_v1::SilicaWindowV1;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{
    CopyPasteSource, DataSourceHandler, DragSource,
};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::channel::{self, Channel, Event};
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::{
    EventLoop, Interest, LoopHandle, Mode as LoopMode, PostAction, RegistrationToken,
};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::protocol::wl_data_device::WlDataDevice;
use smithay_client_toolkit::reexports::client::protocol::wl_data_device_manager::DndAction;
use smithay_client_toolkit::reexports::client::protocol::wl_data_source::WlDataSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent as SeatKeyEvent, KeyboardHandler, Keysym, Modifiers,
};
use smithay_client_toolkit::seat::pointer::{
    PointerEvent as SeatPointerEvent, PointerEventKind, PointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::session_lock::{
    SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
    SessionLockSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgShell, XdgSurface};
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_data_device, delegate_keyboard, delegate_layer, delegate_output,
    delegate_pointer, delegate_registry, delegate_seat, delegate_session_lock, delegate_shm,
    delegate_xdg_popup, delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use tracing::{debug, info, warn};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::{
    self, ExtForeignToplevelHandleV1,
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_list_v1::{
    self, ExtForeignToplevelListV1,
};
use wayland_protocols::xdg::foreign::zv2::client::zxdg_exported_v2::{self, ZxdgExportedV2};
use wayland_protocols::xdg::foreign::zv2::client::zxdg_exporter_v2::ZxdgExporterV2;
use wayland_protocols::xdg::foreign::zv2::client::zxdg_imported_v2::{self, ZxdgImportedV2};
use wayland_protocols::xdg::foreign::zv2::client::zxdg_importer_v2::ZxdgImporterV2;
use wayland_protocols::xdg::shell::client::xdg_positioner;

use crate::{LayerKeyboard, LayerKind, MultiViewRenderer, Options, Renderer};

/// A drag that left this window (`silica/drag` › `start`).
struct DragOut {
    source: DragSource,
    /// The `text/uri-list` body to send to whoever asks for it.
    body: Arc<Vec<u8>>,
    /// The icon and its buffer, alive until the end.
    icon: Option<(wl_surface::WlSurface, Buffer)>,
    /// The last action chosen by the compositor.
    action: DndAction,
}

/// The surface hosting the app: a normal window, a shell layer or a popup (a menu or
/// popover that can extend past its window, silica-desktop#4).
enum Shell {
    Window(Window),
    Layer(LayerSurface),
    Popup(Popup),
    /// The lock screen's surface (ext-session-lock-v1).
    Lock(SessionLockSurface),
}

impl Shell {
    fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            Shell::Window(window) => window.wl_surface(),
            Shell::Layer(layer) => layer.wl_surface(),
            Shell::Popup(popup) => popup.wl_surface(),
            Shell::Lock(lock) => lock.wl_surface(),
        }
    }

    fn commit(&self) {
        match self {
            Shell::Window(window) => window.commit(),
            Shell::Layer(layer) => layer.commit(),
            Shell::Popup(popup) => popup.wl_surface().commit(),
            Shell::Lock(lock) => lock.wl_surface().commit(),
        }
    }

    /// The window title; shell layers have none.
    fn set_title(&self, title: &str) {
        if let Shell::Window(window) = self {
            window.set_title(title.to_owned());
        }
    }
}
use crate::dbus;
use crate::ffi::{
    Callbacks, Engine, EngineConfig, Frame, FrameClock, GlSurface, KeyEvent, PlatformMessage,
    PointerEvent, PointerPhase, ResponseHandle, Task, VsyncBaton, VulkanSurface,
};
use crate::glass;
use crate::keyboard::{Keyboard, edit_key, raw_key_message};
use crate::shell::{self, ShellLink};
use crate::views::{self, Closing, GlassViews, MAIN_VIEW, WindowSpec, window_side};

/// The app state channel (StringCodec).
const LIFECYCLE_CHANNEL: &str = "flutter/lifecycle";

/// How many software frames of a stale size may be dropped in a row before one is presented
/// anyway (a client of a compositor that keeps sending sizes must still show something).
const MAX_STALE_FRAMES: u32 = 2;

/// Whether a software frame of `frame` pixels is dropped instead of presented (#160): it
/// was drawn for a size the window no longer has (`size` logical pixels at `scale`), and
/// fewer than [`MAX_STALE_FRAMES`] were dropped in a row. The compositor would show it at
/// the old size after the configure was acked; the frame at the new size follows.
fn drop_stale_frame(frame: (u32, u32), size: (u32, u32), scale: i32, dropped: u32) -> bool {
    let scale = u32::try_from(scale.max(1)).unwrap_or(1);
    let expected = (size.0.saturating_mul(scale), size.1.saturating_mul(scale));
    frame != expected && dropped < MAX_STALE_FRAMES
}

/// The `flutter/lifecycle` message for an active window or not.
fn lifecycle_message(active: bool) -> &'static str {
    if active {
        "AppLifecycleState.resumed"
    } else {
        "AppLifecycleState.inactive"
    }
}

/// The channel for the input region and keyboard of a layer (shell components).
const LAYER_CHANNEL: &str = "silica/layer";
/// The lock screen: `lock` takes the session lock, `unlock` ends it (JSON method channel).
const LOCK_CHANNEL: &str = "silica/lock";

/// The window appearance channel (`silica-window-v1`): full-size content with the app's
/// toolbar, and moving the window by dragging its bar.
const WINDOW_CHANNEL: &str = "silica/window";
use crate::text_input::{self, EditKey, TextInput};

/// Outcome of a key sent to the framework, with the editing action to do if ignored.
struct KeyResult {
    handled: bool,
    edit: Option<EditKey>,
}

/// Collects the answers of the two paths of a key; on the last one it sends the result.
struct KeyJoin {
    remaining: u8,
    handled: bool,
    edit: Option<EditKey>,
    results: channel::Sender<KeyResult>,
}

impl KeyJoin {
    fn done(join: &Mutex<KeyJoin>, handled: bool) {
        let Ok(mut join) = join.lock() else {
            return;
        };
        join.handled |= handled;
        join.remaining = join.remaining.saturating_sub(1);
        if join.remaining == 0 {
            let result = KeyResult {
                handled: join.handled,
                edit: join.edit.take(),
            };
            let _ = join.results.send(result);
        }
    }
}

/// Mouse button codes (linux/input-event-codes.h) and Flutter masks.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

/// Frame period given to the engine with every vsync: 60 Hz.
const FRAME_PERIOD_NANOS: u64 = 16_666_667;

/// Maximum interval between two loop iterations when no task is queued.
const IDLE_WAKEUP: Duration = Duration::from_millis(100);

/// A queued task, ordered by deadline and then by arrival order.
struct Queued {
    target_nanos: u64,
    seq: u64,
    task: Task,
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        (self.target_nanos, self.seq) == (other.target_nanos, other.seq)
    }
}
impl Eq for Queued {}
impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Queued {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.target_nanos, self.seq).cmp(&(other.target_nanos, other.seq))
    }
}

/// A window of the app and its engine view (silica-desktop#146): the main one (view 0, the
/// engine's implicit view, a window or a shell layer) and those opened with `openWindow`.
struct View {
    surface: Shell,
    /// Size in logical pixels and scale factor of the surface.
    size: (u32, u32),
    scale: i32,
    configured: bool,
    /// The last software frame not yet presented.
    pending: Option<Frame>,
    /// Software frames of a stale size dropped in a row ([`drop_stale_frame`]).
    stale_drops: u32,
    /// Waiting for the compositor's frame callback before presenting again.
    waiting_frame: bool,
    /// A frame callback asked for the vsync and not yet arrived.
    vsync_requested: bool,
    buffer: Option<Buffer>,
    /// The pixels of the last presented software frame, to report only what changed
    /// (`damage.rs`).
    last_pixels: Vec<u8>,
    /// Software has presented at least one frame (with OpenGL the EGL surface knows).
    presented: bool,
    /// `silica-window-v1`: the window object (created on the first request).
    appearance: Option<SilicaWindowV1>,
    /// The window active for Ossido (xdg_toplevel's `activated`).
    activated: Option<bool>,
    /// The compositor says the window is maximized (zoomed), from the last configure.
    maximized: bool,
    /// The compositor says the window is full screen, from the last configure.
    fullscreen: bool,
    /// The engine has added the view (the main one always): frames and metrics may flow.
    added: bool,
    /// `FlutterEngineAddView` is under way.
    adding: bool,
    /// `openWindow`'s reply, sent once the engine has added the view.
    open_reply: Option<ResponseHandle>,
    /// xdg-foreign: the window exported for another app (`exportHandle`, `foreign.rs`).
    export: Option<crate::foreign::Export<ZxdgExportedV2, ResponseHandle>>,
    /// Being removed: the engine is asked to drop the view; the surface waits for it.
    removing: bool,
    /// A shell layer's namespace (`menubar`, `dock`…): Dart picks what to draw by it.
    namespace: Option<String>,
    /// For a popup, the view it hangs from.
    popup_parent: Option<i64>,
    /// For a popup, where Ossido put it: x, y relative to its parent, width, height.
    popup_placed: Option<(i32, i32, i32, i32)>,
}

impl View {
    fn new(surface: Shell, size: (u32, u32), scale: i32, added: bool) -> Self {
        Self {
            surface,
            size,
            scale,
            configured: false,
            pending: None,
            stale_drops: 0,
            waiting_frame: false,
            vsync_requested: false,
            buffer: None,
            last_pixels: Vec::new(),
            presented: false,
            appearance: None,
            activated: None,
            maximized: false,
            fullscreen: false,
            added,
            adding: false,
            open_reply: None,
            export: None,
            removing: false,
            namespace: None,
            popup_parent: None,
            popup_placed: None,
        }
    }

    /// Whether this view is a popup (a menu), not a window.
    fn is_popup(&self) -> bool {
        matches!(self.surface, Shell::Popup(_))
    }

    /// The size in physical pixels.
    fn physical(&self) -> (u32, u32) {
        let scale = u32::try_from(self.scale.max(1)).unwrap_or(1);
        (
            self.size.0.saturating_mul(scale),
            self.size.1.saturating_mul(scale),
        )
    }
}

/// The `xdg_positioner` anchor for an [`views::Edge`].
fn popup_anchor(edge: views::Edge) -> xdg_positioner::Anchor {
    use views::Edge;
    match edge {
        Edge::Center => xdg_positioner::Anchor::None,
        Edge::Top => xdg_positioner::Anchor::Top,
        Edge::Bottom => xdg_positioner::Anchor::Bottom,
        Edge::Left => xdg_positioner::Anchor::Left,
        Edge::Right => xdg_positioner::Anchor::Right,
        Edge::TopLeft => xdg_positioner::Anchor::TopLeft,
        Edge::TopRight => xdg_positioner::Anchor::TopRight,
        Edge::BottomLeft => xdg_positioner::Anchor::BottomLeft,
        Edge::BottomRight => xdg_positioner::Anchor::BottomRight,
    }
}

/// The `xdg_positioner` gravity for an [`views::Edge`].
fn popup_gravity(edge: views::Edge) -> xdg_positioner::Gravity {
    use views::Edge;
    match edge {
        Edge::Center => xdg_positioner::Gravity::None,
        Edge::Top => xdg_positioner::Gravity::Top,
        Edge::Bottom => xdg_positioner::Gravity::Bottom,
        Edge::Left => xdg_positioner::Gravity::Left,
        Edge::Right => xdg_positioner::Gravity::Right,
        Edge::TopLeft => xdg_positioner::Gravity::TopLeft,
        Edge::TopRight => xdg_positioner::Gravity::TopRight,
        Edge::BottomLeft => xdg_positioner::Gravity::BottomLeft,
        Edge::BottomRight => xdg_positioner::Gravity::BottomRight,
    }
}

/// What the engine threads tell the main loop about the views.
enum ViewEvent {
    /// `FlutterEngineAddView` done (true: added).
    Added(i64, bool),
    /// `FlutterEngineRemoveView` done.
    Removed(i64),
    /// The app's answer to `closeRequested` number `request`: true if it closes the window
    /// itself.
    CloseAnswered {
        id: i64,
        request: u64,
        handled: bool,
    },
    /// The app's answer to `closeProbe`: true if it says `closeAsking` when it asks.
    CloseProbeAnswered(bool),
}

struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    /// The windows, by engine view id; [`MAIN_VIEW`] is always there.
    views: BTreeMap<i64, View>,
    /// Several windows (`--multi-window`): the engine composes through backing stores.
    multi_view: bool,
    next_view: i64,
    /// For new windows: the xdg shell and the engine thread's view events.
    xdg_shell: Option<XdgShell>,
    /// For the shell layers drawn as more views of this engine (`--view`, #201).
    layer_shell: Option<LayerShell>,
    /// ext-session-lock-v1, for the lock screen view, and the lock held now.
    session_lock_state: SessionLockState,
    session_lock: Option<SessionLock>,
    view_events: Option<channel::Sender<ViewEvent>>,
    /// The view under the pointer and the one with the keyboard.
    pointer_view: i64,
    keyboard_view: Option<i64>,
    /// The view a drag between apps is over.
    drag_view: i64,
    pointer: Option<wl_pointer::WlPointer>,
    /// wp_cursor_shape_v1: Ossido draws the shape Flutter asks for (`flutter/mousecursor`).
    cursor_shapes: Option<smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager>,
    cursor_device: Option<smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::WpCursorShapeDeviceV1>,
    /// The last shape asked by Flutter, `None` = hidden; set again at every pointer enter.
    cursor_kind: Option<String>,
    enter_serial: u32,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keys: Keyboard,
    modifiers: Modifiers,
    text_input: TextInput,
    /// Glass regions of the `Glass` widgets (`silica/glass`) of every window, taken in by the
    /// platform task that delivers them (see `platform_message` in `run`) and kept in step
    /// with the frames (software frames are committed here).
    glass: Arc<GlassViews>,
    glass_manager: Option<SilicaGlassManagerV1>,
    /// `silica-window-v1`: the manager (each view has its window object).
    appearance_manager: Option<SilicaWindowManagerV1>,
    /// xdg-foreign v2: exports the windows for other apps (`exportHandle`), and the other
    /// app's window imported as this window's parent (`--parent`).
    exporter: Option<ZxdgExporterV2>,
    /// Kept for the life of the process: destroying it would end the relationship.
    _parent_import: Option<ZxdgImportedV2>,
    /// The app is active (a window of it is), as Flutter was told (`flutter/lifecycle`).
    app_active: Option<bool>,
    /// Drag between apps: the manager, the seat's device, the drag that left from here and
    /// the offer dropped here (the data arrives from a thread on `drops`).
    data_manager: Option<DataDeviceManagerState>,
    data_device: Option<DataDevice>,
    drag_out: Option<DragOut>,
    drag_in: bool,
    /// The selection this window owns (`Clipboard.setData`) and its text.
    clipboard: Option<(CopyPasteSource, Arc<Vec<u8>>)>,
    /// The serial of the last key or button press: setting the selection needs one.
    input_serial: u32,
    /// To send requests before waiting on another client (a paste).
    conn: Connection,
    dropped: Option<DragOffer>,
    /// Pseudo-terminals of the app (`silica/pty`), their sources in the loop and whether the
    /// frame timer runs.
    ptys: crate::pty::PtyHost,
    pty_sources: BTreeMap<u32, RegistrationToken>,
    pty_ticking: bool,
    pty_timer: Option<RegistrationToken>,
    /// Until when the ptys tick fast ([`crate::pty::TICK`]): a little while after output or
    /// input, so a change goes out at once; otherwise at the frame period.
    pty_busy_until: Option<std::time::Instant>,
    loop_handle: LoopHandle<'static, App>,
    drops: Option<channel::Sender<(f64, f64, Vec<u8>)>>,
    /// The pointer seat and the serial of the last press (to move the window).
    seat: Option<wl_seat::WlSeat>,
    press_serial: u32,
    /// Windows and commands for the shell components (`silica/shell`).
    shell: ShellLink,
    /// The app's menus for the menu bar (`silica/menu`, `menu.rs`).
    menu: crate::menu::MenuLink,
    /// For the input regions (`silica/layer`).
    compositor: CompositorState,
    /// The `silica/dbus` channel, on a separate thread.
    dbus: Option<dbus::Bridge<ResponseHandle>>,
    qh: QueueHandle<App>,
    key_results: channel::Sender<KeyResult>,
    engine: Option<Engine>,

    tasks: BinaryHeap<Reverse<Queued>>,
    next_seq: u64,

    buttons: i64,
    /// With OpenGL the engine's swap attaches and commits: here only the scale is handled.
    gl: bool,
    /// The engine's vsync request waiting for the compositor's frame callback.
    vsync: Option<VsyncBaton>,
    exit: bool,
    /// The app lifecycle (silica-desktop#142, `app_lifecycle.rs`): the app id, whether the
    /// app ends with its last window, since when its window is hidden, and this process in
    /// the app registry with its socket.
    app_id: String,
    quit_with_last_window: bool,
    /// A helper process (`--helper`): it ends with its window, whatever the app asks.
    helper: bool,
    hidden_since: Option<std::time::Instant>,
    /// The `closeRequested` the app has not answered yet (silica-desktop#192).
    close_requests: views::CloseRequests,
    /// When the engine last asked for a frame: an app drawing after a close request is
    /// likely showing a confirmation (silica_ui before `closeAsking`).
    frame_requested_at: Option<std::time::Instant>,
    /// The app says `closeAsking` when it asks before a close (silica_ui 0.38): a frame
    /// does not count as asking.
    says_close_asking: bool,
    /// The app answers `reopen` itself (silica_ui `SilicaApp`): it builds its new window and
    /// calls `openWindow`, and only then the window comes back (APP-LC). Without it the
    /// hidden window comes back at once.
    handles_reopen: bool,
    /// While hidden, the engine keeps drawing until then: the app tears its window down
    /// after a close, and builds the new one before it is shown.
    free_run_until: Option<std::time::Instant>,
    /// A reopen is waiting for the app's `openWindow` (or the fallback timer).
    show_pending: bool,
    /// The file manager has no Quit (DOCK-02): `setQuitEnabled false` ignores Cmd+Q.
    quit_enabled: bool,
    registration: Option<crate::app_lifecycle::Registration>,
    registry_listener: Option<std::os::unix::net::UnixListener>,
}

/// A layer surface for `layer` on `surface`, of `size` logical pixels (the sides anchored on
/// both edges are the compositor's), committed so that the compositor configures it.
fn create_layer(
    shell: &LayerShell,
    qh: &QueueHandle<App>,
    surface: wl_surface::WlSurface,
    layer: &crate::LayerOptions,
    size: (u32, u32),
) -> LayerSurface {
    let level = match layer.layer {
        LayerKind::Background => Layer::Background,
        LayerKind::Bottom => Layer::Bottom,
        LayerKind::Top => Layer::Top,
        LayerKind::Overlay => Layer::Overlay,
    };
    let surface =
        shell.create_layer_surface(qh, surface, level, Some(layer.namespace.clone()), None);
    let mut anchor = Anchor::empty();
    for (on, edge) in [
        (layer.anchor.top, Anchor::TOP),
        (layer.anchor.bottom, Anchor::BOTTOM),
        (layer.anchor.left, Anchor::LEFT),
        (layer.anchor.right, Anchor::RIGHT),
    ] {
        anchor.set(edge, on);
    }
    surface.set_anchor(anchor);
    // A size anchored to both sides is decided by the compositor (0).
    let (w, h) = size;
    let width = if layer.anchor.left && layer.anchor.right {
        0
    } else {
        w
    };
    let height = if layer.anchor.top && layer.anchor.bottom {
        0
    } else {
        h
    };
    surface.set_size(width, height);
    surface.set_exclusive_zone(layer.exclusive_zone);
    surface.set_keyboard_interactivity(match layer.keyboard {
        LayerKeyboard::None => KeyboardInteractivity::None,
        LayerKeyboard::Exclusive => KeyboardInteractivity::Exclusive,
        LayerKeyboard::OnDemand => KeyboardInteractivity::OnDemand,
    });
    surface.commit();
    surface
}

/// How long a hidden app may draw to tear its closed window down (#142).
const TEARDOWN: Duration = Duration::from_millis(250);
/// How long an app has to build its new window after a reopen before it is shown anyway.
const BUILD_LIMIT: Duration = Duration::from_millis(500);

pub fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    crate::trace::mark("run begin");
    for path in [&options.library, &options.icu_data] {
        if !path.exists() {
            return Err(format!("{} missing: run `cargo xtask engine`", path.display()).into());
        }
    }
    // A debug bundle carries the Dart kernel, a release one the asset manifest and libapp.so.
    if !options.bundle.join("kernel_blob.bin").exists()
        && !options.bundle.join("AssetManifest.bin").exists()
    {
        return Err(format!(
            "{} is not a Flutter bundle: run `cargo xtask bundle <app>`",
            options.bundle.display()
        )
        .into());
    }

    // Launched again while its window is hidden: the running process shows it (#142).
    if options.layer.is_none()
        && !options.helper
        && options.dart_args.is_empty()
        && crate::app_lifecycle::forward(&options.app_id)
    {
        info!("{}: the running process shows its window", options.app_id);
        return Ok(());
    }
    // Launched again with files (Open With, a share target): the running process opens them.
    if options.layer.is_none()
        && !options.helper
        && !options.dart_args.is_empty()
        && crate::app_lifecycle::forward_open(&options.app_id, &options.dart_args)
    {
        info!("{}: the running process opens the files", options.app_id);
        return Ok(());
    }
    // The engine reads these after the renderer is ready: from disk while the GPU starts.
    crate::prefetch::start(
        [
            Some(&options.library),
            options.aot.as_ref(),
            Some(&options.icu_data),
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect(),
        options.bundle.clone(),
    );

    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init(&conn)?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<App> = EventLoop::try_new()?;
    WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| format!("Wayland source: {e}"))?;

    crate::trace::mark("wayland connected");
    let compositor = CompositorState::bind(&globals, &qh)?;
    let shm = Shm::bind(&globals, &qh)?;
    let surface = compositor.create_surface(&qh);
    let xdg_shell: Option<XdgShell>;
    let mut layer_shell = None;
    let mut parent_import = None;
    let surface = match &options.layer {
        None => {
            let shell = XdgShell::bind(&globals, &qh)?;
            let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
            // The parent goes first: Ossido places a window when it learns its app id.
            if let Some(handle) = &options.parent {
                match globals.bind::<ZxdgImporterV2, App, ()>(&qh, 1..=1, ()) {
                    Ok(importer) => {
                        let imported = importer.import_toplevel(handle.clone(), &qh, ());
                        imported.set_parent_of(window.wl_surface());
                        parent_import = Some(imported);
                    }
                    Err(err) => warn!("--parent: no xdg-foreign importer ({err})"),
                }
            }
            window.set_title(options.title.clone());
            window.set_app_id(options.app_id.clone());
            window.commit();
            xdg_shell = Some(shell);
            Shell::Window(window)
        }
        Some(layer) => {
            let shell = LayerShell::bind(&globals, &qh)?;
            let surface = create_layer(&shell, &qh, surface, layer, options.size);
            layer_shell = Some(shell);
            // The menus and popovers of a shell layer are xdg popups adopted by the layer
            // (silica-desktop#222): the xdg shell is needed for them, not for a window.
            xdg_shell = XdgShell::bind(&globals, &qh).ok();
            Shell::Layer(surface)
        }
    };

    crate::trace::mark("shell surface committed");
    // Several windows are for apps; a shell layer has its one surface, unless the shell
    // draws its layers in one engine (`--view`, #201).
    let multi_view =
        (options.multi_window && options.layer.is_none()) || !options.layer_views.is_empty();
    let mut engine_args = options.engine_args.clone();
    // Vulkan with Impeller; if Vulkan is not there (or does not present on this surface) it
    // falls back to OpenGL ES with Impeller. Several views draw into Vulkan backing stores
    // (`ffi/compositor.rs`): with Impeller where the engine has them, otherwise with Skia
    // (`crate::multi_view_renderer`, #203).
    let multi_view_renderer = crate::multi_view_renderer(
        std::env::var("SILICA_MULTI_VIEW_RENDERER").ok().as_deref(),
        crate::engine_has_patch(&options.library, crate::IMPELLER_VULKAN_BACKING_STORES),
    );
    let vulkan = match options.renderer {
        Renderer::Vulkan if multi_view && multi_view_renderer == MultiViewRenderer::Gl => {
            info!("several views: OpenGL ES with Impeller (SILICA_MULTI_VIEW_RENDERER=gl)");
            None
        }
        Renderer::Vulkan => match VulkanSurface::new(&conn, surface.wl_surface(), options.size) {
            Ok(vulkan) => {
                info!("renderer Vulkan: {}", vulkan.renderer());
                if !multi_view || multi_view_renderer == MultiViewRenderer::Impeller {
                    engine_args.push("--enable-impeller".to_owned());
                }
                if multi_view {
                    info!("several views: {multi_view_renderer:?} on Vulkan");
                    vulkan.set_multi_view(multi_view_renderer == MultiViewRenderer::Impeller);
                }
                Some(vulkan)
            }
            Err(err) => {
                warn!("Vulkan unavailable ({err}): using OpenGL ES with Impeller");
                None
            }
        },
        _ => None,
    };
    let gl = match options.renderer {
        Renderer::Vulkan if vulkan.is_some() => None,
        Renderer::Gl | Renderer::Impeller | Renderer::Vulkan => 'gl: {
            let gl = GlSurface::new(&conn, surface.wl_surface(), options.size)?;
            info!("renderer OpenGL ES: {}", gl.renderer());
            if multi_view && let Err(err) = gl.set_multi_view() {
                warn!("{err}: drawing in software");
                break 'gl None;
            }
            if options.renderer != Renderer::Gl {
                if crate::is_software_gl(gl.renderer()) {
                    warn!("Impeller crashes on a software GL driver: using Skia");
                } else {
                    engine_args.push("--enable-impeller".to_owned());
                }
            }
            let (redraw, redraw_channel) = channel::channel::<()>();
            event_loop
                .handle()
                .insert_source(redraw_channel, |event, _, app: &mut App| {
                    if let (Event::Msg(()), Some(engine)) = (event, &app.engine)
                        && let Err(err) = engine.schedule_frame()
                    {
                        warn!("{err}");
                    }
                })
                .map_err(|e| format!("redraw channel: {e}"))?;
            gl.on_redraw(move || {
                let _ = redraw.send(());
            });
            Some(gl)
        }
        Renderer::Software => None,
    };
    crate::trace::mark("renderer ready");
    let gl_mode = gl.is_some() || vulkan.is_some();

    let (width, height) = options.size;
    let pool = SlotPool::new((width * height * 4) as usize, &shm)?;

    // Tasks and frames arrive from the engine threads: two channels to this thread.
    let (task_sender, task_channel): (channel::Sender<Task>, Channel<Task>) = channel::channel();
    let (frame_sender, frame_channel): (channel::Sender<Frame>, Channel<Frame>) =
        channel::channel();
    event_loop
        .handle()
        .insert_source(task_channel, |event, _, app: &mut App| {
            if let Event::Msg(task) = event {
                app.next_seq = app.next_seq.wrapping_add(1);
                app.tasks.push(Reverse(Queued {
                    target_nanos: task.target_nanos,
                    seq: app.next_seq,
                    task,
                }));
            }
        })
        .map_err(|e| format!("task channel: {e}"))?;
    let (message_sender, message_channel) = channel::channel::<PlatformMessage>();
    event_loop
        .handle()
        .insert_source(message_channel, |event, _, app: &mut App| {
            if let Event::Msg(message) = event {
                app.handle_message(message);
            }
        })
        .map_err(|e| format!("message channel: {e}"))?;
    let (dbus_sender, dbus_channel) = channel::channel::<dbus::Event<ResponseHandle>>();
    event_loop
        .handle()
        .insert_source(dbus_channel, |event, _, app: &mut App| {
            if let Event::Msg(event) = event {
                app.handle_dbus(event);
            }
        })
        .map_err(|e| format!("D-Bus channel: {e}"))?;
    let dbus = dbus::Bridge::start(dbus::Addresses::default(), move |event| {
        let _ = dbus_sender.send(event);
    })
    .map_err(|err| warn!("silica/dbus unavailable: {err}"))
    .ok();
    let (vsync_sender, vsync_channel) = channel::channel::<VsyncBaton>();
    let vsync_qh = qh.clone();
    event_loop
        .handle()
        .insert_source(vsync_channel, move |event, _, app: &mut App| {
            if let Event::Msg(baton) = event {
                app.request_vsync(baton, &vsync_qh);
            }
        })
        .map_err(|e| format!("vsync channel: {e}"))?;
    let (key_results, key_channel) = channel::channel::<KeyResult>();
    event_loop
        .handle()
        .insert_source(key_channel, |event, _, app: &mut App| {
            if let Event::Msg(result) = event {
                app.key_result(result);
            }
        })
        .map_err(|e| format!("key channel: {e}"))?;
    let frame_qh = qh.clone();
    event_loop
        .handle()
        .insert_source(frame_channel, move |event, _, app: &mut App| {
            if let Event::Msg(frame) = event {
                let id = frame.view_id;
                if let Some(view) = app.views.get_mut(&id) {
                    view.pending = Some(frame);
                    app.present(id, &frame_qh);
                }
            }
        })
        .map_err(|e| format!("frame channel: {e}"))?;
    let (view_sender, view_channel) = channel::channel::<ViewEvent>();
    event_loop
        .handle()
        .insert_source(view_channel, |event, _, app: &mut App| {
            if let Event::Msg(event) = event {
                app.view_event(event);
            }
        })
        .map_err(|e| format!("view channel: {e}"))?;

    // The glass regions travel with the frames of the raster thread, window by window.
    let glass = Arc::new(GlassViews::default());
    let glass_manager = globals
        .bind::<SilicaGlassManagerV1, App, ()>(&qh, 1..=9, ())
        .ok();
    glass.insert(
        MAIN_VIEW,
        glass_manager.clone(),
        &conn,
        surface.wl_surface(),
    );
    let frames = {
        let before = Arc::clone(&glass);
        let after = Arc::clone(&glass);
        Arc::new(FrameClock::new(
            move |view, seq| before.before_present(view, seq),
            move |seq| after.after_task(seq),
        ))
    };
    let software_frames = Arc::clone(&glass);
    let (glass_inbox, glass_qh) = (Arc::clone(&glass), qh.clone());
    let callbacks = Callbacks {
        platform_thread: std::thread::current().id(),
        post_task: Box::new(move |task| {
            if task_sender.send(task).is_err() {
                warn!("task dropped: the main loop is closed");
            }
        }),
        present: Box::new(move |frame: Frame| {
            software_frames.software_frame(frame.view_id, frame.raster_seq);
            let _ = frame_sender.send(frame);
        }),
        platform_message: Box::new(move |message| {
            // The glass regions are taken in here, in the platform task that delivers them, to
            // reach their frame before the raster thread presents it (`GlassSync`).
            if message.channel == glass::CHANNEL {
                glass_inbox.receive(&message.data, message.raster_seq, &glass_qh);
            }
            // If the loop is closed the message falls with its handle: the engine is shutting
            // down and no longer waits for answers.
            let _ = message_sender.send(message);
        }),
        gl,
        vulkan,
        vsync: Some(Box::new(move |baton| {
            let _ = vsync_sender.send(baton);
        })),
        frames,
        multi_view,
    };
    crate::trace::mark("engine run begin");
    let engine = Engine::run(
        &EngineConfig {
            library: &options.library,
            assets: &options.bundle,
            icu_data: &options.icu_data,
            aot: options.aot.as_deref(),
            engine_args: &engine_args,
            dart_args: &options.dart_args,
        },
        callbacks,
    )?;
    crate::trace::mark("engine run end");
    info!("engine started with {}", options.bundle.display());

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool,
        views: BTreeMap::from([(MAIN_VIEW, {
            let mut view = View::new(surface, options.size, 1, true);
            view.namespace = options.layer.as_ref().map(|layer| layer.namespace.clone());
            view
        })]),
        multi_view,
        next_view: MAIN_VIEW + 1,
        xdg_shell,
        layer_shell,
        session_lock_state: SessionLockState::new(&globals, &qh),
        session_lock: None,
        view_events: Some(view_sender),
        pointer_view: MAIN_VIEW,
        keyboard_view: None,
        drag_view: MAIN_VIEW,
        pointer: None,
        keyboard: None,
        keys: Keyboard::default(),
        modifiers: Modifiers::default(),
        text_input: TextInput::default(),
        appearance_manager: globals
            .bind::<SilicaWindowManagerV1, App, ()>(&qh, 1..=4, ())
            .ok(),
        exporter: globals.bind::<ZxdgExporterV2, App, ()>(&qh, 1..=1, ()).ok(),
        _parent_import: parent_import,
        app_active: None,
        data_manager: DataDeviceManagerState::bind(&globals, &qh).ok(),
        cursor_shapes:
            smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager::bind(
                &globals, &qh,
            )
            .ok(),
        cursor_device: None,
        cursor_kind: Some("basic".to_owned()),
        enter_serial: 0,
        data_device: None,
        drag_out: None,
        drag_in: false,
        clipboard: None,
        input_serial: 0,
        conn: conn.clone(),
        dropped: None,
        ptys: crate::pty::PtyHost::default(),
        pty_sources: BTreeMap::new(),
        pty_ticking: false,
        pty_timer: None,
        pty_busy_until: None,
        loop_handle: event_loop.handle(),
        drops: None,
        seat: None,
        press_serial: 0,
        glass,
        glass_manager,
        menu: crate::menu::MenuLink::new(
            globals
                .bind::<SilicaMenuManagerV1, App, ()>(&qh, 1..=1, ())
                .ok(),
        ),
        shell: ShellLink::new(
            globals
                .bind::<SilicaShellManagerV1, App, ()>(&qh, 1..=9, ())
                .ok(),
            globals
                .bind::<ExtForeignToplevelListV1, App, ()>(&qh, 1..=1, ())
                .ok(),
        ),
        compositor,
        dbus,
        qh: qh.clone(),
        key_results,
        engine: Some(engine),
        tasks: BinaryHeap::new(),
        next_seq: 0,
        buttons: 0,
        gl: gl_mode,
        vsync: None,
        exit: false,
        app_id: options.app_id.clone(),
        quit_with_last_window: false,
        helper: options.helper,
        hidden_since: None,
        close_requests: views::CloseRequests::default(),
        frame_requested_at: None,
        says_close_asking: false,
        handles_reopen: false,
        free_run_until: None,
        show_pending: false,
        quit_enabled: true,
        registration: None,
        registry_listener: None,
    };
    if options.layer.is_none() && !options.helper {
        app.register(&event_loop.handle());
    }
    for view in &options.layer_views {
        app.open_layer(view);
    }

    // Appearance: light until the portal says otherwise, then on every change.
    // The system languages, before the first frame.
    if let Some(engine) = &app.engine
        && let Err(err) = engine.update_locales(&crate::locale::from_env(|k| std::env::var(k).ok()))
    {
        warn!("languages: {err}");
    }
    app.send_appearance(false);
    let (appearance_sender, appearance_channel) = channel::channel::<bool>();
    event_loop
        .handle()
        .insert_source(appearance_channel, |event, _, app: &mut App| {
            if let Event::Msg(dark) = event {
                app.send_appearance(dark);
            }
        })
        .map_err(|e| format!("appearance channel: {e}"))?;
    let _appearance = crate::appearance::watch(appearance_sender);

    // The data of a drop, read on a thread (the source may be this very window).
    let (drop_sender, drop_channel) = channel::channel::<(f64, f64, Vec<u8>)>();
    event_loop
        .handle()
        .insert_source(drop_channel, |event, _, app: &mut App| {
            if let Event::Msg((x, y, data)) = event {
                app.deliver_drop(x, y, &data);
            }
        })
        .map_err(|e| format!("drop channel: {e}"))?;
    app.drops = Some(drop_sender);

    while !app.exit {
        let timeout = app.run_due_tasks();
        event_loop.dispatch(Some(timeout), &mut app)?;
    }
    // Shuts down the engine before closing the Wayland connection.
    app.engine = None;
    Ok(())
}

impl App {
    /// Gives Ossido the shape Flutter asked for (needs the serial of the last pointer enter).
    fn apply_cursor(&self) {
        let Some(pointer) = &self.pointer else {
            return;
        };
        match self
            .cursor_kind
            .as_deref()
            .and_then(crate::mouse_cursor::shape_of)
        {
            Some(shape) => {
                if let Some(device) = &self.cursor_device {
                    device.set_shape(self.enter_serial, shape);
                }
            }
            // `none`: no cursor surface hides the pointer.
            None => pointer.set_cursor(self.enter_serial, None, 0, 0),
        }
    }

    /// Answers a framework message. Every message with a handle gets a
    /// response, empty if the channel is not handled.
    fn handle_message(&mut self, message: PlatformMessage) {
        // silica/dbus answers later, from its thread (`handle_dbus`).
        if message.channel == dbus::CHANNEL
            && let Some(bridge) = &self.dbus
        {
            if let Some(handle) = message.response {
                bridge.handle(&message.data, handle);
            }
            return;
        }
        // `openWindow` answers once the engine has added the view (`view_event`),
        // `exportHandle` once the compositor sent the handle.
        if message.channel == WINDOW_CHANNEL
            && let Ok(call) = serde_json::from_slice::<serde_json::Value>(&message.data)
        {
            match call.get("method").and_then(serde_json::Value::as_str) {
                Some("openWindow") => return self.open_view(&call, message.response),
                Some("openPopup") => return self.open_popup(&call, message.response),
                Some("exportHandle") => return self.export_handle(&call, message.response),
                _ => {}
            }
        }
        let reply = match message.channel.as_str() {
            text_input::CHANNEL => self.text_input.handle_call(&message.data),
            shell::CHANNEL => self.shell.handle_call(&message.data, &self.qh),
            crate::menu::CHANNEL => self.menu.handle_call(&message.data, &self.qh),
            LAYER_CHANNEL => self.handle_layer_call(&message.data),
            LOCK_CHANNEL => self.handle_lock_call(&message.data),
            WINDOW_CHANNEL => self.handle_window_call(&message.data),
            crate::drag::CHANNEL => self.handle_drag_call(&message.data),
            crate::app_lifecycle::CHANNEL => self.handle_app_call(&message.data),
            crate::pty::CHANNEL => self.handle_pty_call(&message.data),
            // Already taken in by the platform task (`platform_message` in `run`): the reply.
            glass::CHANNEL => self.glass.reply(&message.data),
            "flutter/platform" => self.handle_platform_call(&message.data),
            crate::mouse_cursor::CHANNEL => {
                if let Some(kind) = crate::mouse_cursor::activate_kind(&message.data) {
                    self.cursor_kind = (kind != "none").then_some(kind);
                    self.apply_cursor();
                }
                // StandardMethodCodec success with a null result.
                vec![0, 0]
            }
            // StandardMethodCodec: getKeyboardState, the pressed keys physical → logical.
            "flutter/keyboard" => crate::codec::success_int_map(
                self.keys
                    .pressed()
                    .iter()
                    .map(|(physical, logical)| (*physical as i64, *logical as i64)),
            ),
            _ => Vec::new(),
        };
        if let (Some(engine), Some(handle)) = (&self.engine, message.response)
            && let Err(err) = engine.respond(handle, &reply)
        {
            warn!("response on {}: {err}", message.channel);
        }
    }

    /// The `silica/window` channel (JSONMethodCodec):
    /// - `setFullSizeContent {toolbarHeight}`: full-size content (0 goes back to Ossido's
    ///   bar); answers whether Ossido offers it;
    /// - `setAppearance {appearance}` (`"system"`, `"light"`, `"dark"`): the app overrides the
    ///   system appearance for everything Ossido draws for this window; answers whether
    ///   Ossido offers it (version 3);
    /// - `setSizeIncrement {baseWidth, baseHeight, stepWidth, stepHeight}`: interactive resizing
    ///   snaps the content size to base + n * step (a terminal's cell size, version 4); answers
    ///   whether Ossido offers it;
    /// - `setFullscreen {enabled}`: the window fills the display (`set_fullscreen`) or leaves
    ///   it; the app is told when the compositor changes it on its own with
    ///   `fullscreenChanged {viewId, enabled}` (Esc, a space change; silica-desktop#173);
    /// - `startMove`: moves the window with the pointer (from the app's toolbar);
    /// - `zoom`: zooms the window (xdg_toplevel `set_maximized`), or puts it back when it is
    ///   zoomed (`unset_maximized`); `minimize`: minimizes it (`set_minimized`). The title
    ///   bar double click of an app that draws its own bar (silica-desktop#169); both answer
    ///   whether the window exists;
    /// - `setSize {width, height}`: the app chooses its window size (logical pixels);
    /// - `setSizeLimits {minWidth?, minHeight?, maxWidth?, maxHeight?}`: limits for resizing;
    ///   min = max makes the window fixed-size (Ossido then disables the zoom button).
    ///
    /// Every call takes an optional `viewId` (several windows, `views.rs`), which also adds
    /// `closeWindow {viewId}` and `setTitle {viewId, title}`; `openWindow` is in `open_view`,
    /// `exportHandle` (the window's xdg-foreign identifier for another app) in
    /// `export_handle`.
    fn handle_window_call(&mut self, data: &[u8]) -> Vec<u8> {
        let Ok(call) = serde_json::from_slice::<serde_json::Value>(data) else {
            return Vec::new();
        };
        let id = views::view_arg(&call);
        let method = call.get("method").and_then(serde_json::Value::as_str);
        if method == Some("closeWindow") {
            self.close_view(id);
            return crate::text_input::success(serde_json::Value::Null);
        }
        if method == Some("closeAsking") {
            // The app asks the user before closing window `id`: its answer is waited for.
            let request = call
                .pointer("/args/request")
                .and_then(serde_json::Value::as_u64);
            if let Some(pending) = self.close_requests.asking(id, request) {
                info!(
                    "window {id}: the app is asking before closing ({} ms)",
                    pending.since.elapsed().as_millis()
                );
            }
            return crate::text_input::success(serde_json::Value::Null);
        }
        if method == Some("popupsAvailable") {
            // Menus and popovers as their own surfaces need several views (`--multi-window`,
            // or the shell's `--view` layers) and a window or a shell layer to hang from.
            let window = matches!(
                self.views.get(&MAIN_VIEW),
                Some(View {
                    surface: Shell::Window(_) | Shell::Layer(_),
                    ..
                })
            );
            return crate::text_input::success(serde_json::json!(
                self.multi_view && self.xdg_shell.is_some() && window
            ));
        }
        let Some(View {
            surface: Shell::Window(window),
            appearance: view_appearance,
            ..
        }) = self.views.get_mut(&id)
        else {
            return crate::text_input::success(serde_json::json!(false));
        };
        match method {
            Some("setTitle") => {
                let title: String = call
                    .pointer("/args/title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .chars()
                    .take(256)
                    .collect();
                window.set_title(title);
                window.commit();
                crate::text_input::success(serde_json::json!(true))
            }
            Some("setFullSizeContent") => {
                let Some(manager) = &self.appearance_manager else {
                    return crate::text_input::success(serde_json::json!(false));
                };
                let height = call
                    .pointer("/args/toolbarHeight")
                    .and_then(serde_json::Value::as_f64)
                    .and_then(views::toolbar_height)
                    .unwrap_or(0);
                let object = view_appearance
                    .get_or_insert_with(|| manager.get_window(window.wl_surface(), &self.qh, ()));
                object.set_full_size_content(height);
                window.commit();
                crate::text_input::success(serde_json::json!(true))
            }
            Some("setAppearance") => {
                // The override for everything Ossido draws for this window (version 3).
                let (Some(manager), Some(appearance)) = (
                    &self.appearance_manager,
                    crate::window_appearance::parse(call.get("args")),
                ) else {
                    return crate::text_input::success(serde_json::json!(false));
                };
                if wayland_client::Proxy::version(manager) < 3 {
                    return crate::text_input::success(serde_json::json!(false));
                }
                let object = view_appearance
                    .get_or_insert_with(|| manager.get_window(window.wl_surface(), &self.qh, ()));
                object.set_appearance(appearance);
                window.commit();
                crate::text_input::success(serde_json::json!(true))
            }
            Some("setSizeIncrement") => {
                // Interactive resizing in whole steps (version 4): a terminal's cell size.
                let (Some(manager), args) = (&self.appearance_manager, call.get("args")) else {
                    return crate::text_input::success(serde_json::json!(false));
                };
                if wayland_client::Proxy::version(manager) < 4 {
                    return crate::text_input::success(serde_json::json!(false));
                }
                let side = |key: &str| {
                    args.and_then(|a| a.get(key))
                        .and_then(serde_json::Value::as_f64)
                        .and_then(window_side)
                        .unwrap_or(0)
                };
                let object = view_appearance
                    .get_or_insert_with(|| manager.get_window(window.wl_surface(), &self.qh, ()));
                object.set_size_increment(
                    side("baseWidth"),
                    side("baseHeight"),
                    side("stepWidth"),
                    side("stepHeight"),
                );
                window.commit();
                crate::text_input::success(serde_json::json!(true))
            }
            Some("startMove") => {
                if let Some(seat) = &self.seat {
                    window.move_(seat, self.press_serial);
                }
                crate::text_input::success(serde_json::Value::Null)
            }
            Some("zoom") => {
                let maximized = self.views.get(&id).is_some_and(|view| view.maximized);
                if let Some(View {
                    surface: Shell::Window(window),
                    ..
                }) = self.views.get(&id)
                {
                    if maximized {
                        window.unset_maximized();
                    } else {
                        window.set_maximized();
                    }
                    window.commit();
                }
                crate::text_input::success(serde_json::json!(true))
            }
            Some("setFullscreen") => {
                // A slideshow fills the display (xdg_toplevel full screen); the answer says
                // the window exists. `fullscreenChanged` tells when the compositor leaves it.
                let enabled = call
                    .pointer("/args/enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if enabled {
                    window.set_fullscreen(None);
                } else {
                    window.unset_fullscreen();
                }
                window.commit();
                crate::text_input::success(serde_json::json!(true))
            }
            Some("minimize") => {
                window.set_minimized();
                window.commit();
                crate::text_input::success(serde_json::json!(true))
            }
            Some("setSizeLimits") => {
                let limit = |key: &str| {
                    call.pointer(&format!("/args/{key}"))
                        .and_then(serde_json::Value::as_f64)
                        .and_then(window_side)
                };
                let pair = |w, h| match (limit(w), limit(h)) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => None,
                };
                window.set_min_size(pair("minWidth", "minHeight"));
                window.set_max_size(pair("maxWidth", "maxHeight"));
                window.commit();
                crate::text_input::success(serde_json::json!(true))
            }
            Some("setSize") => {
                let side = |key: &str| {
                    call.pointer(&format!("/args/{key}"))
                        .and_then(serde_json::Value::as_f64)
                        .and_then(window_side)
                };
                let (Some(width), Some(height)) = (side("width"), side("height")) else {
                    return crate::text_input::success(serde_json::json!(false));
                };
                // A floating window may choose its own size: the next buffer has it and Ossido
                // follows the committed geometry.
                if let Some(view) = self.views.get_mut(&id) {
                    view.size = (width, height);
                }
                self.send_metrics(id);
                let qh = self.qh.clone();
                self.present(id, &qh);
                crate::text_input::success(serde_json::json!(true))
            }
            _ => Vec::new(),
        }
    }

    /// `exportHandle {viewId?}`: the window's xdg-foreign identifier (`"wayland:<handle>"`)
    /// for another app (the portal's `parent_window`), or null (`foreign.rs`).
    fn export_handle(&mut self, call: &serde_json::Value, mut reply: Option<ResponseHandle>) {
        let id = views::view_arg(call);
        let answer = match (&self.exporter, self.views.get_mut(&id)) {
            (
                Some(exporter),
                Some(View {
                    surface: Shell::Window(window),
                    export,
                    ..
                }),
            ) => {
                let export = export.get_or_insert_with(|| {
                    crate::foreign::Export::new(exporter.export_toplevel(
                        window.wl_surface(),
                        &self.qh,
                        id,
                    ))
                });
                match export.ask(&mut reply) {
                    // Answered when the handle arrives (`export_ready`).
                    None => return,
                    Some(identifier) => serde_json::json!(identifier),
                }
            }
            _ => serde_json::Value::Null,
        };
        self.respond_json(reply, answer, "exportHandle");
    }

    /// The compositor sent the handle of window `id`'s export: the waiting calls get it.
    fn export_ready(&mut self, id: i64, handle: String) {
        let Some(export) = self
            .views
            .get_mut(&id)
            .and_then(|view| view.export.as_mut())
        else {
            return;
        };
        let (identifier, waiting) = export.ready(handle);
        for reply in waiting {
            self.respond_json(Some(reply), serde_json::json!(identifier), "exportHandle");
        }
    }

    /// Answers a call on `silica/window` with `value` (JSONMethodCodec success).
    fn respond_json(&self, reply: Option<ResponseHandle>, value: serde_json::Value, what: &str) {
        if let (Some(engine), Some(handle)) = (&self.engine, reply)
            && let Err(err) = engine.respond(handle, &crate::text_input::success(value))
        {
            warn!("{what} reply: {err}");
        }
    }

    /// The reply of an `openWindow` / `openPopup` that did not open anything: null.
    fn refuse_open(&self, reply: Option<ResponseHandle>, why: &str) {
        warn!("openWindow: {why}");
        if let (Some(engine), Some(handle)) = (&self.engine, reply)
            && let Err(err) =
                engine.respond(handle, &crate::text_input::success(serde_json::Value::Null))
        {
            warn!("openWindow reply: {err}");
        }
    }

    /// Gives the engine the new view `id` (OpenGL or Vulkan surface) and its glass.
    fn attach_view(&mut self, id: i64, view: &View) -> Result<(), String> {
        if let Some(gl) = self.engine.as_ref().and_then(Engine::gl) {
            gl.add_view(id, view.surface.wl_surface(), view.physical())
                .map_err(|err| err.to_string())?;
        }
        if let Some(vulkan) = self.engine.as_ref().and_then(Engine::vulkan) {
            vulkan
                .add_view(id, view.surface.wl_surface())
                .map_err(|err| err.to_string())?;
        }
        self.glass.insert(
            id,
            self.glass_manager.clone(),
            &self.conn,
            view.surface.wl_surface(),
        );
        Ok(())
    }

    /// `openWindow` (several windows, `views.rs`): a new xdg_toplevel and its engine view.
    /// `reply` gets the view id once the engine has added the view, or null.
    fn open_view(&mut self, call: &serde_json::Value, reply: Option<ResponseHandle>) {
        let refuse = |app: &Self, reply: Option<ResponseHandle>, why: &str| {
            app.refuse_open(reply, why);
        };
        let Some(spec) = call.get("args").and_then(WindowSpec::parse) else {
            return refuse(self, reply, "no valid size");
        };
        if !self.multi_view {
            return refuse(self, reply, "the app was not started with --multi-window");
        }
        if self.views.len() > views::MAX_WINDOWS {
            return refuse(self, reply, "too many windows");
        }
        let (Some(xdg_shell), Some(_)) = (&self.xdg_shell, &self.engine) else {
            return refuse(self, reply, "no xdg shell");
        };
        let id = self.next_view;
        self.next_view = self.next_view.saturating_add(1);
        let surface = self.compositor.create_surface(&self.qh);
        let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &self.qh);
        // A child window stays above its parent and belongs to the same app (Info, compose).
        // The parent goes first: Ossido places a window when it learns its app id.
        if let Some(parent) = spec.parent
            && let Some(View {
                surface: Shell::Window(parent),
                ..
            }) = self.views.get(&parent)
        {
            window.set_parent(Some(parent));
        }
        window.set_title(spec.title.clone());
        window.set_app_id(self.app_id.clone());
        window.set_min_size(spec.min_size);
        window.set_max_size(spec.max_size);
        // The same scale as the main window until Ossido tells the window's own.
        let scale = self.views.get(&MAIN_VIEW).map_or(1, |view| view.scale);
        let mut view = View::new(Shell::Window(window), spec.size, scale, false);
        if let (Some(height), Some(manager), Shell::Window(window)) =
            (spec.toolbar_height, &self.appearance_manager, &view.surface)
        {
            let object = manager.get_window(window.wl_surface(), &self.qh, ());
            object.set_full_size_content(height);
            view.appearance = Some(object);
        }
        view.surface.commit();
        if let Err(err) = self.attach_view(id, &view) {
            return refuse(self, reply, &err);
        }
        // The engine gets the view at the window's first configure (`configured`): nothing
        // may be drawn on an xdg surface before it is configured.
        view.open_reply = reply;
        info!("window {id} opened: {:?} {:?}", spec.title, spec.size);
        self.views.insert(id, view);
    }

    /// `openPopup` (`views.rs`): a menu or popover as an `xdg_popup` of window `parent`, with
    /// its own engine view, so it can extend past the window (silica-desktop#4). `reply` gets
    /// the view id once the engine has added the view, or null.
    fn open_popup(&mut self, call: &serde_json::Value, reply: Option<ResponseHandle>) {
        let Some(spec) = call.get("args").and_then(views::PopupSpec::parse) else {
            return self.refuse_open(reply, "popup: no valid size or anchor");
        };
        if !self.multi_view {
            return self.refuse_open(reply, "popup: the app was not started with --multi-window");
        }
        if self.views.len() > views::MAX_WINDOWS {
            return self.refuse_open(reply, "popup: too many windows");
        }
        let Some(xdg_shell) = &self.xdg_shell else {
            return self.refuse_open(reply, "popup: no xdg shell");
        };
        // The parent: a window, a shell layer or a popup that is open and drawn (the
        // compositor wants it mapped). A layer has no xdg_surface: the popup is created
        // without a parent and adopted with `zwlr_layer_surface_v1.get_popup`.
        let parent = match self.views.get(&spec.parent) {
            Some(View {
                surface: Shell::Window(window),
                configured: true,
                removing: false,
                ..
            }) => Some(window.xdg_surface().clone()),
            Some(View {
                surface: Shell::Popup(popup),
                configured: true,
                removing: false,
                ..
            }) => Some(popup.xdg_surface().clone()),
            Some(View {
                surface: Shell::Layer(_),
                configured: true,
                removing: false,
                ..
            }) => None,
            _ => return self.refuse_open(reply, "popup: its parent is not an open window"),
        };
        let positioner = match XdgPositioner::new(xdg_shell) {
            Ok(positioner) => positioner,
            Err(err) => return self.refuse_open(reply, &format!("popup positioner: {err}")),
        };
        let (x, y, w, h) = spec.anchor_rect;
        positioner.set_size(spec.size.0 as i32, spec.size.1 as i32);
        positioner.set_anchor_rect(x, y, w, h);
        positioner.set_anchor(popup_anchor(spec.anchor));
        positioner.set_gravity(popup_gravity(spec.gravity));
        positioner.set_constraint_adjustment(
            xdg_positioner::ConstraintAdjustment::from_bits_truncate(spec.adjust),
        );
        let surface = self.compositor.create_surface(&self.qh);
        let popup =
            match Popup::from_surface(parent.as_ref(), &positioner, &self.qh, surface, xdg_shell) {
                Ok(popup) => popup,
                Err(err) => return self.refuse_open(reply, &format!("popup: {err}")),
            };
        if parent.is_none()
            && let Some(View {
                surface: Shell::Layer(layer),
                ..
            }) = self.views.get(&spec.parent)
        {
            layer.get_popup(popup.xdg_popup());
        }
        if spec.grab
            && let Some(seat) = &self.seat
        {
            popup.xdg_popup().grab(seat, self.input_serial);
        }
        let id = self.next_view;
        self.next_view = self.next_view.saturating_add(1);
        // The same scale as its window until Ossido tells the popup's own.
        let scale = self
            .views
            .get(&spec.parent)
            .map_or(1, |parent| parent.scale);
        let mut view = View::new(Shell::Popup(popup), spec.size, scale, false);
        view.popup_parent = Some(spec.parent);
        view.surface.commit();
        if let Err(err) = self.attach_view(id, &view) {
            return self.refuse_open(reply, &err);
        }
        // Like a window, the engine gets the view at the popup's first configure.
        view.open_reply = reply;
        info!(
            "popup {id} opened: {:?} at {:?}",
            spec.size, spec.anchor_rect
        );
        self.views.insert(id, view);
    }

    /// The shell layers of this engine, for `silica/layer` `views`: `[{viewId, namespace}]`,
    /// the views the engine has added (Dart sees them in `PlatformDispatcher.views`).
    fn layer_views(&self) -> Vec<serde_json::Value> {
        self.views
            .iter()
            .filter(|(_, view)| (view.added || view.adding) && !view.removing)
            .filter_map(|(id, view)| {
                let namespace = view.namespace.as_ref()?;
                Some(serde_json::json!({ "viewId": id, "namespace": namespace }))
            })
            .collect()
    }

    /// The lock screen's views go and the lock is let go (the session is unlocked, or the
    /// compositor refused it).
    fn release_lock(&mut self) {
        let ids: Vec<i64> = self
            .views
            .iter()
            .filter(|(_, view)| matches!(view.surface, Shell::Lock(_)))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.remove_view(id);
        }
        self.session_lock = None;
    }

    /// A call on `silica/lock` from the shell's lock component:
    /// - `lock`: takes the session lock (true if it holds it, or asked for it);
    /// - `unlock`: ends it with `unlock_and_destroy`, after the password was right.
    fn handle_lock_call(&mut self, data: &[u8]) -> Vec<u8> {
        let Ok(call) = serde_json::from_slice::<serde_json::Value>(data) else {
            return Vec::new();
        };
        let done = match call.get("method").and_then(serde_json::Value::as_str) {
            Some("lock") => {
                if self.session_lock.is_some() {
                    true
                } else {
                    match self.session_lock_state.lock(&self.qh) {
                        Ok(lock) => {
                            // Held from now; the surfaces come with `locked`.
                            self.session_lock = Some(lock);
                            true
                        }
                        Err(err) => {
                            warn!("session lock: {err}");
                            false
                        }
                    }
                }
            }
            Some("unlock") => match self.session_lock.take() {
                Some(lock) => {
                    lock.unlock();
                    self.release_lock();
                    true
                }
                None => false,
            },
            _ => false,
        };
        text_input::success(serde_json::json!(done))
    }

    /// One more shell layer drawn by this engine (`--view`, #201): its own layer surface,
    /// added to the engine at its first configure, like a new window.
    fn open_layer(&mut self, spec: &crate::LayerView) {
        let (Some(layer_shell), Some(engine)) = (&self.layer_shell, &self.engine) else {
            warn!("--view {}: no layer shell", spec.layer.namespace);
            return;
        };
        let id = self.next_view;
        self.next_view = self.next_view.saturating_add(1);
        let surface = self.compositor.create_surface(&self.qh);
        let layer = create_layer(layer_shell, &self.qh, surface, &spec.layer, spec.size);
        let scale = self.views.get(&MAIN_VIEW).map_or(1, |view| view.scale);
        let mut view = View::new(Shell::Layer(layer), spec.size, scale, false);
        view.namespace = Some(spec.layer.namespace.clone());
        if let Some(gl) = engine.gl()
            && let Err(err) = gl.add_view(id, view.surface.wl_surface(), view.physical())
        {
            warn!("layer {}: {err}", spec.layer.namespace);
            return;
        }
        if let Some(vulkan) = engine.vulkan()
            && let Err(err) = vulkan.add_view(id, view.surface.wl_surface())
        {
            warn!("layer {}: {err}", spec.layer.namespace);
            return;
        }
        self.glass.insert(
            id,
            self.glass_manager.clone(),
            &self.conn,
            view.surface.wl_surface(),
        );
        info!("layer {id} opened: {}", spec.layer.namespace);
        self.views.insert(id, view);
    }

    /// Adds window `id`'s view to the engine, at its configured size.
    fn add_to_engine(&mut self, id: i64) {
        let (Some(engine), Some(events), Some(view)) =
            (&self.engine, &self.view_events, self.views.get_mut(&id))
        else {
            return;
        };
        view.adding = true;
        let events = events.clone();
        let added = engine.add_view(
            id,
            view.physical(),
            f64::from(view.scale.max(1)),
            move |added| {
                let _ = events.send(ViewEvent::Added(id, added));
            },
        );
        if let Err(err) = added {
            warn!("window {id}: {err}");
            view.adding = false;
            if let Some(handle) = view.open_reply.take() {
                let _ =
                    engine.respond(handle, &crate::text_input::success(serde_json::Value::Null));
            }
            self.forget_view(id);
        }
    }

    /// The engine thread's news about a view.
    fn view_event(&mut self, event: ViewEvent) {
        match event {
            ViewEvent::Added(id, added) => {
                let Some(view) = self.views.get_mut(&id) else {
                    return;
                };
                view.adding = false;
                view.added = added;
                if view.removing {
                    // Closed while the engine was adding it.
                    if added {
                        self.remove_from_engine(id);
                    } else {
                        self.forget_view(id);
                    }
                    return;
                }
                let reply = view.open_reply.take();
                let value = if let (true, Some((x, y, width, height))) = (added, view.popup_placed)
                {
                    // A popup: where Ossido put it, after flipping and sliding it onto the
                    // output (a popover points its arrow at its anchor from this).
                    serde_json::json!({
                        "viewId": id, "x": x, "y": y, "width": width, "height": height,
                    })
                } else if added {
                    serde_json::json!(id)
                } else {
                    serde_json::Value::Null
                };
                if let (Some(engine), Some(handle)) = (&self.engine, reply)
                    && let Err(err) = engine.respond(handle, &crate::text_input::success(value))
                {
                    warn!("openWindow reply: {err}");
                }
                if added {
                    self.send_metrics(id);
                    if let Some(engine) = &self.engine {
                        // The keyboard came to the window before its view existed.
                        if self.keyboard_view == Some(id)
                            && let Err(err) = engine.send_view_focus(id, true)
                        {
                            warn!("view focus: {err}");
                        }
                        if let Err(err) = engine.schedule_frame() {
                            warn!("{err}");
                        }
                    }
                } else {
                    warn!("window {id}: the engine did not add its view");
                    self.forget_view(id);
                }
            }
            ViewEvent::Removed(id) => self.forget_view(id),
            ViewEvent::CloseProbeAnswered(says) => self.says_close_asking |= says,
            ViewEvent::CloseAnswered {
                id,
                request,
                handled,
            } => {
                let Some(pending) = self.close_requests.answered(id, request) else {
                    debug!("window {id}: stale answer to close request {request}");
                    return;
                };
                let took = pending.since.elapsed().as_millis();
                if handled {
                    info!("window {id}: the app answers its close itself ({took} ms)");
                } else {
                    info!("window {id}: close answered in {took} ms");
                    self.close_view(id);
                }
            }
        }
    }

    /// The windows on screen other than `id` (the main one when not hidden; those being
    /// removed do not count).
    fn others_on_screen(&self, id: i64) -> usize {
        self.views
            .iter()
            .filter(|(other, view)| {
                **other != id
                    && !view.is_popup()
                    && !view.removing
                    && (**other != MAIN_VIEW || self.hidden_since.is_none())
            })
            .count()
    }

    /// Closes window `id`: its close button (after the app agreed, for a window other than
    /// the main one), or the app's `closeWindow`. The app lifecycle covers every window
    /// (APP-LC): only the last one on screen makes the app windowless, or ends it.
    fn close_view(&mut self, id: i64) {
        if let Some(pending) = self.close_requests.forget(id) {
            info!(
                "window {id}: closed {} ms after its close request",
                pending.since.elapsed().as_millis()
            );
        }
        let Some(view) = self.views.get(&id) else {
            return;
        };
        // A popup is not a window: closing it leaves the app lifecycle alone.
        if view.is_popup() {
            if !view.removing {
                self.remove_view(id);
            }
            return;
        }
        if view.removing || (id == MAIN_VIEW && self.hidden_since.is_some()) {
            return;
        }
        let hideable = self.registration.is_some()
            && self
                .appearance_manager
                .as_ref()
                .is_some_and(|manager| wayland_client::Proxy::version(manager) >= 2);
        let others = self.others_on_screen(id);
        match views::closing(
            id,
            others,
            self.quit_with_last_window || self.helper,
            hideable,
        ) {
            Closing::Exit => self.exit = true,
            Closing::HideMain { last } => self.hide_main(last),
            Closing::Remove { last } => {
                self.remove_view(id);
                if last {
                    self.became_windowless();
                }
            }
        }
    }

    /// Window `id` goes: the engine drops its view first, then the window goes
    /// (`forget_view`).
    fn remove_view(&mut self, id: i64) {
        // The popups hanging from it go first (a popup is destroyed before its parent).
        for child in self.popups_of(id) {
            self.remove_view(child);
        }
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        view.removing = true;
        if let Some(handle) = view.open_reply.take()
            && let Some(engine) = &self.engine
        {
            let _ = engine.respond(handle, &crate::text_input::success(serde_json::Value::Null));
        }
        match (view.added, view.adding) {
            (true, _) => self.remove_from_engine(id),
            // The engine is adding it: its Added event removes it.
            (false, true) => {}
            (false, false) => self.forget_view(id),
        }
    }

    /// The popups that hang directly from view `id`.
    fn popups_of(&self, id: i64) -> Vec<i64> {
        self.views
            .iter()
            .filter(|(_, view)| view.popup_parent == Some(id))
            .map(|(child, _)| *child)
            .collect()
    }

    /// Asks the engine to remove view `id`; `forget_view` follows when it is done.
    fn remove_from_engine(&mut self, id: i64) {
        let (Some(engine), Some(events)) = (&self.engine, &self.view_events) else {
            self.forget_view(id);
            return;
        };
        let events = events.clone();
        if let Err(err) = engine.remove_view(id, move |_| {
            let _ = events.send(ViewEvent::Removed(id));
        }) {
            warn!("window {id}: {err}");
            self.forget_view(id);
        }
    }

    /// The window of view `id` goes, with its EGL surface and glass; the app learns it.
    fn forget_view(&mut self, id: i64) {
        if id == MAIN_VIEW {
            return;
        }
        for child in self.popups_of(id) {
            self.forget_view(child);
        }
        if let Some(gl) = self.engine.as_ref().and_then(Engine::gl) {
            gl.remove_view(id);
        }
        if let Some(vulkan) = self.engine.as_ref().and_then(Engine::vulkan) {
            vulkan.remove_view(id);
        }
        self.glass.remove(id);
        if let Some(mut view) = self.views.remove(&id) {
            if let Some(object) = view.appearance.take() {
                object.destroy();
            }
            if let Some(export) = view.export.take() {
                export.object.destroy();
                for reply in export.waiting {
                    self.respond_json(Some(reply), serde_json::Value::Null, "exportHandle");
                }
            }
            info!("window {id} closed");
        }
        if self.pointer_view == id {
            self.pointer_view = MAIN_VIEW;
        }
        if self.keyboard_view == Some(id) {
            self.keyboard_view = None;
        }
        self.send_window_event("windowClosed", serde_json::json!({ "viewId": id }));
        self.update_app_active();
    }

    /// A call to the app on `silica/window`.
    fn send_window_event(&self, method: &str, args: serde_json::Value) {
        if let Some(engine) = &self.engine
            && let Err(err) = engine.send_message(WINDOW_CHANNEL, &crate::drag::call(method, args))
        {
            warn!("silica/window: {err}");
        }
    }

    /// The close button of window `id` (the main one too): the app decides (`closeRequested`
    /// answered true: it closes the window itself, after asking to save a draft); otherwise
    /// the window closes now. With no answer and no `closeAsking` within `CLOSE_WAIT` it
    /// closes anyway (silica-desktop#192): a hung app. An app with no listener on
    /// `silica/window` never answers either (the engine buffers the request): `closeProbe`
    /// right after it makes the engine answer it at once.
    fn request_close(&mut self, id: i64) {
        let (Some(engine), Some(events)) = (&self.engine, &self.view_events) else {
            self.close_view(id);
            return;
        };
        let Some(request) = self.close_requests.ask(id, std::time::Instant::now()) else {
            // The app is still answering the previous request (a confirmation is up).
            debug!("window {id}: close request already being answered");
            return;
        };
        let probe_events = events.clone();
        let events = events.clone();
        let sent = engine.send_message_with_reply(
            WINDOW_CHANNEL,
            &crate::drag::call(
                "closeRequested",
                serde_json::json!({ "viewId": id, "request": request }),
            ),
            move |reply| {
                let handled = close_answer(&reply);
                let _ = events.send(ViewEvent::CloseAnswered {
                    id,
                    request,
                    handled,
                });
            },
        );
        if let Err(err) = sent {
            warn!("closeRequested: {err}");
            self.close_view(id);
            return;
        }
        // With no listener on the channel the engine holds a message (one at most) instead
        // of answering it: this one pushes the request out, and the engine answers it empty
        // at once (the window closes now). With a listener it is answered: true when the app
        // says `closeAsking` when it asks.
        if let Err(err) = engine.send_message_with_reply(
            WINDOW_CHANNEL,
            &crate::drag::call("closeProbe", serde_json::json!({ "viewId": id })),
            move |reply| {
                let _ = probe_events.send(ViewEvent::CloseProbeAnswered(close_answer(&reply)));
            },
        ) {
            warn!("closeProbe: {err}");
        }
        let timer = Timer::from_duration(views::CLOSE_WAIT);
        let inserted = self
            .loop_handle
            .insert_source(timer, move |_, _, app: &mut App| {
                app.close_wait_over(id, request);
                TimeoutAction::Drop
            });
        if let Err(err) = inserted {
            warn!("close timer: {err}");
        }
    }

    /// `CLOSE_WAIT` after close request `request` of window `id`: unanswered, it closes the
    /// window, unless the app said it is asking (`closeAsking`). An app that does not say it
    /// (silica_ui before 0.38, Terminal's confirmation) is waited for when it has asked for a
    /// frame since: it may be drawing its question.
    fn close_wait_over(&mut self, id: i64, request: u64) {
        let drawing = !self.says_close_asking
            && self
                .close_requests
                .since(id, request)
                .zip(self.frame_requested_at)
                .is_some_and(|(since, frame)| frame > since);
        if drawing {
            if self.close_requests.asking(id, Some(request)).is_some() {
                info!("window {id}: the app draws since its close request, waiting for it");
            }
            return;
        }
        if self.close_requests.expired(id, request).is_some() {
            warn!(
                "window {id}: no answer to its close request in {} ms, closing it",
                views::CLOSE_WAIT.as_millis()
            );
            self.close_view(id);
        }
    }

    /// The `flutter/platform` channel (JSONMethodCodec): for now the window title
    /// (`SystemChrome.setApplicationSwitcherDescription`, `label`). The rest is not
    /// implemented (empty response).
    fn handle_platform_call(&mut self, data: &[u8]) -> Vec<u8> {
        let Ok(call) = serde_json::from_slice::<serde_json::Value>(data) else {
            return Vec::new();
        };
        match call.get("method").and_then(serde_json::Value::as_str) {
            Some("Clipboard.setData") => {
                self.copy(crate::clipboard::set_data_text(call.get("args")));
                b"[null]".to_vec()
            }
            Some("Clipboard.getData") => crate::clipboard::get_data_reply(self.paste().as_deref()),
            Some("Clipboard.hasStrings") => crate::clipboard::has_strings_reply(self.has_strings()),
            Some("SystemChrome.setApplicationSwitcherDescription") => {
                let label = call
                    .get("args")
                    .and_then(|args| args.get("label"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                // A very long title from an app must not become a problem for Ossido.
                let title: String = label.chars().take(256).collect();
                if let Some(view) = self.views.get(&MAIN_VIEW) {
                    view.surface.set_title(&title);
                    view.surface.commit();
                }
                b"[null]".to_vec()
            }
            _ => Vec::new(),
        }
    }

    /// `Clipboard.setData`: this window becomes the selection owner with `text`.
    fn copy(&mut self, text: Option<String>) {
        let (Some(text), Some(manager), Some(device)) =
            (text, &self.data_manager, &self.data_device)
        else {
            return;
        };
        let source = manager.create_copy_paste_source(&self.qh, crate::clipboard::TEXT_TYPES);
        source.set_selection(device, self.input_serial);
        self.clipboard = Some((source, Arc::new(text.into_bytes())));
    }

    /// `Clipboard.getData`: the selection's text, from this window or from its owner.
    fn paste(&mut self) -> Option<String> {
        if let Some((_, text)) = &self.clipboard {
            return Some(String::from_utf8_lossy(text).into_owned());
        }
        let offer = self.data_device.as_ref()?.data().selection_offer()?;
        let mime = offer.with_mime_types(crate::clipboard::text_type)?;
        let pipe = offer.receive(mime).ok()?;
        // The request must reach the owner before waiting for its answer.
        let _ = self.conn.flush();
        crate::clipboard::read_text(pipe.into(), crate::clipboard::READ_TIMEOUT)
    }

    /// `Clipboard.hasStrings`: is there text to paste?
    fn has_strings(&self) -> bool {
        self.clipboard.is_some()
            || self
                .data_device
                .as_ref()
                .and_then(|device| device.data().selection_offer())
                .is_some_and(|offer| offer.with_mime_types(crate::clipboard::text_type).is_some())
    }

    /// Joins the app registry: the socket in the loop, the idle check on a timer (#142).
    fn register(&mut self, handle: &LoopHandle<'static, App>) {
        use std::os::fd::AsFd;
        let (registration, listener) = match crate::app_lifecycle::register(&self.app_id) {
            Ok(registered) => registered,
            Err(err) => {
                warn!("app registry: {err}: the app ends with its last window");
                return;
            }
        };
        let fd = match listener.as_fd().try_clone_to_owned() {
            Ok(fd) => fd,
            Err(err) => {
                warn!("app registry: {err}");
                return;
            }
        };
        let source = Generic::new(fd, Interest::READ, LoopMode::Level);
        if let Err(err) = handle.insert_source(source, |_, _, app: &mut App| {
            app.accept_registry_clients();
            Ok(PostAction::Continue)
        }) {
            warn!("app registry: {err}");
            return;
        }
        // No automatic termination (APP-LC), except under the debug limit.
        if let Some(limit) = crate::app_lifecycle::idle_limit() {
            let period = crate::app_lifecycle::check_period(limit);
            if let Err(err) =
                handle.insert_source(Timer::from_duration(period), move |_, _, app: &mut App| {
                    app.check_windowless();
                    TimeoutAction::ToDuration(period)
                })
            {
                warn!("app registry timer: {err}");
            }
        }
        self.registration = Some(registration);
        self.registry_listener = Some(listener);
    }

    fn accept_registry_clients(&mut self) {
        loop {
            let accepted = match &self.registry_listener {
                Some(listener) => listener.accept(),
                None => return,
            };
            let stream = match accepted {
                Ok((stream, _)) => stream,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => return,
                Err(err) => {
                    warn!("app registry: {err}");
                    return;
                }
            };
            use crate::app_lifecycle::{Request, answer, read_request};
            match read_request(&stream) {
                Some(Request::Status) => {
                    let status = if self.windowless() {
                        "hidden"
                    } else {
                        "visible"
                    };
                    answer(&stream, status);
                }
                Some(Request::Reopen) => {
                    self.reopen();
                    answer(&stream, "ok");
                }
                Some(Request::Open(args)) => {
                    self.reopen();
                    self.send_app("open", serde_json::json!({ "args": args }));
                    answer(&stream, "ok");
                }
                Some(Request::Quit) => {
                    if self.quit_enabled {
                        answer(&stream, "ok");
                        info!("{}: quit", self.app_id);
                        self.exit = true;
                    } else {
                        // The file manager has no Quit (DOCK-02).
                        answer(&stream, "ignored");
                    }
                }
                None => answer(&stream, "?"),
            }
        }
    }

    /// No window of the app is on screen (the main one hidden, no other one open).
    fn windowless(&self) -> bool {
        self.hidden_since.is_some() && self.others_on_screen(MAIN_VIEW) == 0
    }

    /// The main window closes (#142, APP-LC): it goes away (hidden: the engine's implicit
    /// view stays) and the app keeps running. When it was the `last` window on screen the
    /// app becomes windowless (`became_windowless`).
    fn hide_main(&mut self, last: bool) {
        // The menus and popovers of the window go with it: hidden, they would come back with
        // the window when it is shown again (silica-os/mustache#25).
        let popups = self.popups_of(MAIN_VIEW);
        for child in &popups {
            self.remove_view(*child);
        }
        if !popups.is_empty() {
            // The engine removes their views on its next frames, which a hidden window would
            // not draw.
            self.free_run(TEARDOWN);
        }
        let Some(View {
            surface: Shell::Window(window),
            appearance,
            activated,
            ..
        }) = self.views.get_mut(&MAIN_VIEW)
        else {
            self.exit = true;
            return;
        };
        if let Some(manager) = &self.appearance_manager {
            let object = appearance
                .get_or_insert_with(|| manager.get_window(window.wl_surface(), &self.qh, ()));
            object.set_hidden(1);
        }
        *activated = Some(false);
        self.hidden_since = Some(std::time::Instant::now());
        self.show_pending = false;
        self.send_window_event("windowClosed", serde_json::json!({ "viewId": MAIN_VIEW }));
        if last {
            self.became_windowless();
        } else {
            self.update_app_active();
        }
    }

    /// The last window on screen closed (APP-LC): the app keeps running with no window. It
    /// learns it (`lastWindowClosed`) and tears the closed window's content down: a reopen
    /// builds a new window, it does not bring the old one back (that would be minimizing).
    fn became_windowless(&mut self) {
        self.app_active = Some(false);
        self.send_lifecycle(false);
        self.send_app("lastWindowClosed", serde_json::json!({}));
        // A few frames while hidden, so the app can drop the closed window's content.
        if self.handles_reopen {
            self.free_run(TEARDOWN);
        }
        info!("{}: last window closed, the app keeps running", self.app_id);
    }

    /// `flutter/lifecycle` follows the app: resumed while one of its windows is active.
    fn update_app_active(&mut self) {
        let active = self.views.iter().any(|(id, view)| {
            view.activated == Some(true) && !view.removing && {
                *id != MAIN_VIEW || self.hidden_since.is_none()
            }
        });
        if self.app_active != Some(active) {
            self.app_active = Some(active);
            self.send_lifecycle(active);
        }
    }

    /// Lets the engine draw for `span` while the window is hidden.
    fn free_run(&mut self, span: Duration) {
        self.free_run_until = Some(std::time::Instant::now() + span);
        if let Some(baton) = self.vsync.take() {
            self.answer_vsync(baton);
        }
        if let Some(engine) = &self.engine
            && let Err(err) = engine.schedule_frame()
        {
            warn!("{err}");
        }
    }

    /// A reopen (the Dock, a second launch, the Launcher or Faro). With a window on screen the
    /// app only learns about it. With none, an app that answers reopen builds its new window
    /// and calls `openWindow`; any other app gets its hidden window back at once.
    fn reopen(&mut self) {
        let windowless = self.windowless();
        if windowless && self.handles_reopen {
            if !self.show_pending {
                self.show_pending = true;
                self.free_run(BUILD_LIMIT);
                // An app that never calls openWindow still gets its window.
                let timer = Timer::from_duration(BUILD_LIMIT);
                if let Err(err) = self
                    .loop_handle
                    .insert_source(timer, |_, _, app: &mut App| {
                        if app.show_pending {
                            warn!("{}: no openWindow after a reopen", app.app_id);
                            app.show_window();
                        }
                        TimeoutAction::Drop
                    })
                {
                    warn!("reopen timer: {err}");
                }
            }
        } else if windowless {
            self.show_window();
        } else {
            self.activate_main();
        }
        self.send_app("reopen", serde_json::json!({ "windowless": windowless }));
    }

    /// The app built its window (`openWindow`): it is shown after the frame being drawn, so
    /// the first image on screen is the new window, never the closed one.
    fn open_window(&mut self) {
        if self.hidden_since.is_none() {
            return;
        }
        self.free_run(BUILD_LIMIT);
        let timer = Timer::from_duration(Duration::from_nanos(2 * FRAME_PERIOD_NANOS));
        if let Err(err) = self
            .loop_handle
            .insert_source(timer, |_, _, app: &mut App| {
                app.show_window();
                TimeoutAction::Drop
            })
        {
            warn!("open window timer: {err}");
            self.show_window();
        }
    }

    /// A reopen with the window already on screen (the Dock, a second launch): it comes to
    /// the front, focused, as Ossido does for `set_hidden(0)`. Without this a window behind
    /// other windows stays where it is and the launch seems to do nothing.
    fn activate_main(&mut self) {
        if self.hidden_since.is_some() {
            return;
        }
        let Some(manager) = &self.appearance_manager else {
            return;
        };
        if let Some(View {
            surface: Shell::Window(window),
            appearance,
            ..
        }) = self.views.get_mut(&MAIN_VIEW)
        {
            let object = appearance
                .get_or_insert_with(|| manager.get_window(window.wl_surface(), &self.qh, ()));
            object.set_hidden(0);
        }
    }

    /// The hidden window is on screen again.
    fn show_window(&mut self) {
        if self.hidden_since.take().is_none() {
            return;
        }
        self.show_pending = false;
        self.free_run_until = None;
        if let Some(object) = self
            .views
            .get(&MAIN_VIEW)
            .and_then(|view| view.appearance.as_ref())
        {
            object.set_hidden(0);
        }
        // The engine draws again: the vsync it was waiting for, then the frame callbacks.
        if let Some(baton) = self.vsync.take() {
            self.answer_vsync(baton);
        }
        if let Some(engine) = &self.engine
            && let Err(err) = engine.schedule_frame()
        {
            warn!("{err}");
        }
    }

    /// Only with `SILICA_APP_IDLE_LIMIT` set (debugging): a windowless idle app is ended. The
    /// reference does not end windowless apps (APP-LC: still running after 15 minutes).
    fn check_windowless(&mut self) {
        let (Some(since), Some(limit)) = (self.hidden_since, crate::app_lifecycle::idle_limit())
        else {
            return;
        };
        if !self.windowless() {
            return;
        }
        if since.elapsed() >= limit {
            if let Some(registration) = &self.registration {
                registration.park();
            }
            info!("{}: windowless and idle, ended (debug limit)", self.app_id);
            self.exit = true;
        }
    }

    /// The `silica/app` channel (JSONMethodCodec, see `app_lifecycle.rs`).
    fn handle_app_call(&mut self, data: &[u8]) -> Vec<u8> {
        let Ok(call) = serde_json::from_slice::<serde_json::Value>(data) else {
            return Vec::new();
        };
        match call.get("method").and_then(serde_json::Value::as_str) {
            Some("setQuitWhenLastWindowCloses") => {
                self.quit_with_last_window = call
                    .pointer("/args/value")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                crate::text_input::success(serde_json::Value::Null)
            }
            Some("quit") => {
                if self.quit_enabled {
                    self.exit = true;
                }
                crate::text_input::success(serde_json::Value::Null)
            }
            Some("setHandlesReopen") => {
                self.handles_reopen = call
                    .pointer("/args/value")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                crate::text_input::success(serde_json::Value::Null)
            }
            Some("setQuitEnabled") => {
                self.quit_enabled = call
                    .pointer("/args/value")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                crate::text_input::success(serde_json::Value::Null)
            }
            Some("openWindow") => {
                self.open_window();
                crate::text_input::success(serde_json::Value::Null)
            }
            // The app closes its window itself (its last tab, its shell exited): as the
            // close button.
            Some("closeWindow") => {
                self.close_view(MAIN_VIEW);
                crate::text_input::success(serde_json::Value::Null)
            }
            Some("isHidden") => {
                crate::text_input::success(serde_json::json!(self.hidden_since.is_some()))
            }
            _ => Vec::new(),
        }
    }

    fn send_app(&self, method: &str, args: serde_json::Value) {
        if let Some(engine) = &self.engine
            && let Err(err) = engine.send_message(
                crate::app_lifecycle::CHANNEL,
                &crate::drag::call(method, args),
            )
        {
            warn!("silica/app: {err}");
        }
    }

    /// `flutter/lifecycle`: the active window is `resumed`, the others `inactive` (apps follow
    /// the state with `AppLifecycleListener`, for the colors of the inactive window).
    fn send_lifecycle(&mut self, active: bool) {
        if let Some(engine) = &self.engine
            && let Err(err) =
                engine.send_message(LIFECYCLE_CHANNEL, lifecycle_message(active).as_bytes())
        {
            warn!("flutter/lifecycle: {err}");
        }
    }

    /// The `silica/pty` channel (see `pty.rs`).
    fn handle_pty_call(&mut self, data: &[u8]) -> Vec<u8> {
        let reply = match crate::pty::parse_call(data) {
            Ok(call) => self.ptys.handle(call),
            Err(err) => Err(err),
        };
        let reply = match reply {
            Ok(reply) => reply,
            Err(err) => return crate::pty::error(&err),
        };
        if let Some(id) = reply.closed
            && let Some(token) = self.pty_sources.remove(&id)
        {
            self.loop_handle.remove(token);
        }
        if let Some(id) = reply.spawned {
            self.watch_pty(id);
        }
        self.start_pty_ticks();
        // Input usually brings output back (echo, a prompt): be ready to send it at once.
        self.pty_activity();
        crate::text_input::success(reply.value)
    }

    /// Reads the pty `id` when its master has data.
    fn watch_pty(&mut self, id: u32) {
        let fd = match self.ptys.master(id).map(|fd| fd.try_clone_to_owned()) {
            Some(Ok(fd)) => fd,
            Some(Err(err)) => {
                warn!("pty {id}: {err}");
                return;
            }
            None => return,
        };
        let source = Generic::new(fd, Interest::READ, LoopMode::Level);
        let inserted = self
            .loop_handle
            .insert_source(source, move |_, _, app: &mut App| {
                let readable = app.ptys.readable(id);
                // Output arrived: frames go out at once while the pty is busy.
                app.pty_activity();
                Ok(match readable {
                    crate::pty::Readable::Continue => PostAction::Continue,
                    crate::pty::Readable::Pause => PostAction::Disable,
                    crate::pty::Readable::Remove => {
                        app.pty_sources.remove(&id);
                        PostAction::Remove
                    }
                })
            });
        match inserted {
            Ok(token) => {
                self.pty_sources.insert(id, token);
            }
            Err(err) => warn!("pty {id}: {err}"),
        }
    }

    /// One message per frame per pty (the host keeps the 60 Hz limit) from a timer that runs
    /// while there are ptys: every [`crate::pty::TICK`] around activity, so the first change
    /// after a quiet moment goes out at once, and at the frame period when idle.
    fn start_pty_ticks(&mut self) {
        if self.pty_ticking {
            return;
        }
        let timer = Timer::from_duration(crate::pty::TICK);
        let inserted = self
            .loop_handle
            .insert_source(timer, |_, _, app: &mut App| {
                if app.pty_tick() {
                    TimeoutAction::ToDuration(app.pty_cadence())
                } else {
                    app.pty_ticking = false;
                    app.pty_timer = None;
                    TimeoutAction::Drop
                }
            });
        match inserted {
            Ok(token) => {
                self.pty_ticking = true;
                self.pty_timer = Some(token);
            }
            Err(err) => warn!("pty timer: {err}"),
        }
    }

    /// The interval to the next pty tick.
    fn pty_cadence(&self) -> Duration {
        let busy = self
            .pty_busy_until
            .is_some_and(|until| std::time::Instant::now() < until);
        if busy {
            crate::pty::TICK
        } else {
            Duration::from_nanos(FRAME_PERIOD_NANOS)
        }
    }

    /// Output or input on a pty: tick fast for a while. Coming out of a quiet moment the timer
    /// may be up to a frame away: it starts again from now.
    fn pty_activity(&mut self) {
        let now = std::time::Instant::now();
        let quiet = self.pty_busy_until.is_none_or(|until| now >= until);
        self.pty_busy_until = Some(now + crate::pty::BUSY_SPELL);
        if quiet && let Some(token) = self.pty_timer.take() {
            self.loop_handle.remove(token);
            self.pty_ticking = false;
            self.start_pty_ticks();
        }
    }

    /// Sends the output and the exits of the frame; false when no pty is left.
    fn pty_tick(&mut self) -> bool {
        let tick = self.ptys.tick(std::time::Instant::now());
        if let Some(engine) = &self.engine {
            for message in &tick.messages {
                if let Err(err) = engine.send_message(crate::pty::CHANNEL, message) {
                    warn!("silica/pty: {err}");
                }
            }
        }
        for id in &tick.resume {
            if let Some(token) = self.pty_sources.get(id)
                && let Err(err) = self.loop_handle.enable(token)
            {
                warn!("pty {id}: {err}");
            }
        }
        for id in &tick.gone {
            if let Some(token) = self.pty_sources.remove(id) {
                self.loop_handle.remove(token);
            }
        }
        !tick.idle
    }

    /// The `silica/drag` channel (see `drag.rs`): `start` begins a drag.
    fn handle_drag_call(&mut self, data: &[u8]) -> Vec<u8> {
        let reply = |started: bool| crate::text_input::success(serde_json::json!(started));
        let Ok(call) = serde_json::from_slice::<serde_json::Value>(data) else {
            return Vec::new();
        };
        if call.get("method").and_then(serde_json::Value::as_str) != Some("start") {
            return Vec::new();
        }
        let Some(start) = call.get("args").and_then(crate::drag::parse_start) else {
            return reply(false);
        };
        let (Some(manager), Some(device)) = (&self.data_manager, &self.data_device) else {
            return reply(false);
        };
        let source = manager.create_drag_and_drop_source(
            &self.qh,
            [crate::drag::URI_LIST],
            DndAction::Copy | DndAction::Move,
        );
        let icon = start.icon.as_ref().and_then(|icon| {
            let side = |v: u32| i32::try_from(v).ok();
            let (width, height) = (side(icon.width)?, side(icon.height)?);
            let (buffer, canvas) = self
                .pool
                .create_buffer(width, height, width * 4, wl_shm::Format::Argb8888)
                .ok()?;
            canvas
                .get_mut(..icon.bgra.len())?
                .copy_from_slice(&icon.bgra);
            let surface = self.compositor.create_surface(&self.qh);
            Some((surface, buffer, width, height, icon.hotspot))
        });
        // The drag starts in the window under the pointer.
        let Some(origin) = self
            .views
            .get(&self.pointer_view)
            .or_else(|| self.views.get(&MAIN_VIEW))
        else {
            return reply(false);
        };
        source.start_drag(
            device,
            origin.surface.wl_surface(),
            icon.as_ref().map(|(surface, ..)| surface),
            self.press_serial,
        );
        let icon = icon.and_then(|(surface, buffer, width, height, (x, y))| {
            buffer.attach_to(&surface).ok()?;
            // The grab point on the pointer: the icon's surface moves by (-x, -y) from it.
            if (x, y) != (0, 0) {
                if wayland_client::Proxy::version(&surface) >= 5 {
                    surface.offset(-x, -y);
                } else {
                    surface.attach(Some(buffer.wl_buffer()), -x, -y);
                }
            }
            surface.damage_buffer(0, 0, width, height);
            surface.commit();
            Some((surface, buffer))
        });
        self.drag_out = Some(DragOut {
            source,
            body: Arc::new(crate::drag::uri_list(&start.uris)),
            icon,
            action: DndAction::empty(),
        });
        reply(true)
    }

    /// A call to the app on `silica/drag`.
    fn send_drag(&self, method: &str, args: serde_json::Value) {
        if let Some(engine) = &self.engine
            && let Err(err) =
                engine.send_message(crate::drag::CHANNEL, &crate::drag::call(method, args))
        {
            warn!("silica/drag: {err}");
        }
    }

    /// The drag that left from here has finished: the app learns with which action.
    fn finish_drag_out(&mut self, completed: bool) {
        let Some(drag) = self.drag_out.take() else {
            return;
        };
        let action = if !completed {
            "none"
        } else if drag.action.contains(DndAction::Move) {
            "move"
        } else if drag.action.contains(DndAction::Copy) {
            "copy"
        } else {
            "none"
        };
        if let Some((surface, _)) = &drag.icon {
            surface.destroy();
        }
        self.send_drag("finished", serde_json::json!({ "action": action }));
    }

    /// The data of a drop has arrived: the URIs to the app, then the offer closes.
    fn deliver_drop(&mut self, x: f64, y: f64, data: &[u8]) {
        let uris = crate::drag::parse_uri_list(data);
        self.send_drag(
            "drop",
            serde_json::json!({ "x": x, "y": y, "uris": uris, "viewId": self.drag_view }),
        );
        if let Some(offer) = self.dropped.take() {
            offer.finish();
            offer.destroy();
        }
    }

    /// `flutter/settings` with the appearance (light or dark): `MediaQuery.platformBrightness`.
    fn send_appearance(&mut self, dark: bool) {
        if let Some(engine) = &self.engine
            && let Err(err) = engine.send_message(
                crate::appearance::SETTINGS_CHANNEL,
                &crate::appearance::settings_message(dark),
            )
        {
            warn!("flutter/settings: {err}");
        }
    }

    /// An outcome of the D-Bus thread: the reply to a call or a signal for Dart.
    fn handle_dbus(&mut self, event: dbus::Event<ResponseHandle>) {
        let Some(engine) = &self.engine else { return };
        let result = match event {
            dbus::Event::Reply(handle, reply) => engine.respond(handle, &reply),
            dbus::Event::Signal(message) => engine.send_message(dbus::CHANNEL, &message),
        };
        if let Err(err) = result {
            warn!("silica/dbus: {err}");
        }
    }

    /// The `silica/layer` channel: a hidden shell component must not take the
    /// pointer or the keyboard (a closed Faro keeps running, transparent).
    /// - `setKeyboardInteractivity {mode: none|exclusive|on-demand}`;
    /// - `setInputRegion {rects: [[x, y, w, h], …] | null}` (null: the whole surface);
    /// - `outputSize` → `{width, height}` of the screen in points (null if unknown);
    /// - `setSize {width, height}` in points, 0 to follow the anchors (the menu bar grows
    ///   while a menu is open);
    /// - `setExclusiveZone {zone}` in points (the Dock hiding itself gives its room back);
    /// - `setTransient {rects | null, owner?, ownerRule?, otherRule?, press, scroll}`: a menu
    ///   or panel is open; a press outside it arrives as `transientDismissed` on silica/shell
    ///   (#149); `clearTransient` when the shell closes it itself. With silica-shell-v1
    ///   version 7 the owner app (`""` the system) and the rules `modal` | `pass` |
    ///   `activate` for its surfaces and the others; on version 6 `press` and `scroll`
    ///   (`consume` | `pass_through`).
    fn handle_layer_call(&mut self, data: &[u8]) -> Vec<u8> {
        let Ok(call) = serde_json::from_slice::<serde_json::Value>(data) else {
            return Vec::new();
        };
        let args = call.get("args").cloned().unwrap_or(serde_json::Value::Null);
        if call.get("method").and_then(serde_json::Value::as_str) == Some("views") {
            return text_input::success(serde_json::Value::Array(self.layer_views()));
        }
        // The layer the call is for (`viewId`, #201); the main one without it.
        let target = views::view_arg(&call);
        let Some(View { surface, .. }) = self.views.get(&target) else {
            return text_input::success(serde_json::json!(false));
        };
        let done = match call.get("method").and_then(serde_json::Value::as_str) {
            Some("setKeyboardInteractivity") => {
                let mode = match args.get("mode").and_then(serde_json::Value::as_str) {
                    Some("exclusive") => KeyboardInteractivity::Exclusive,
                    Some("on-demand") => KeyboardInteractivity::OnDemand,
                    _ => KeyboardInteractivity::None,
                };
                match surface {
                    Shell::Layer(layer) => {
                        layer.set_keyboard_interactivity(mode);
                        true
                    }
                    Shell::Window(_) | Shell::Popup(_) | Shell::Lock(_) => false,
                }
            }
            Some("outputSize") => {
                // The logical size of the screen (the first output): to center an alert
                // in a surface that grows until it covers it.
                let size = self
                    .output_state
                    .outputs()
                    .next()
                    .and_then(|output| self.output_state.info(&output))
                    .and_then(|info| info.logical_size);
                return text_input::success(match size {
                    Some((w, h)) => serde_json::json!({"width": w, "height": h}),
                    None => serde_json::Value::Null,
                });
            }
            Some("setSize") => {
                let value = |key: &str| {
                    args.get(key)
                        .and_then(serde_json::Value::as_f64)
                        .filter(|v| v.is_finite())
                        .map(|v| v.round().clamp(0.0, 16384.0) as u32)
                };
                match (surface, value("width"), value("height")) {
                    (Shell::Layer(layer), Some(w), Some(h)) => {
                        layer.set_size(w, h);
                        true
                    }
                    _ => false,
                }
            }
            Some("setExclusiveZone") => {
                let zone = args
                    .get("zone")
                    .and_then(serde_json::Value::as_f64)
                    .filter(|v| v.is_finite())
                    .map(|v| v.round().clamp(-1.0, 16384.0) as i32);
                match (surface, zone) {
                    (Shell::Layer(layer), Some(zone)) => {
                        layer.set_exclusive_zone(zone);
                        true
                    }
                    _ => false,
                }
            }
            Some("setInputRegion") => match args.get("rects").and_then(serde_json::Value::as_array)
            {
                None => {
                    surface.wl_surface().set_input_region(None);
                    true
                }
                Some(rects) => match self.region_from(rects) {
                    Some(region) => {
                        surface
                            .wl_surface()
                            .set_input_region(Some(region.wl_region()));
                        true
                    }
                    None => false,
                },
            },
            // A menu or panel is open (#149): Ossido tells when a press outside ends it
            // (`transientDismissed` on silica/shell), so the surface keeps a small input region.
            Some("setTransient") => {
                let outside = |key: &str| match args.get(key).and_then(serde_json::Value::as_str) {
                    Some("pass_through") => Outside::PassThrough,
                    _ => Outside::Consume,
                };
                let region = match args.get("rects").and_then(serde_json::Value::as_array) {
                    Some(rects) => self.region_from(rects),
                    None => None,
                };
                let rule = |key: &str| match args.get(key).and_then(serde_json::Value::as_str) {
                    Some("modal") => Some(Rule::Modal),
                    Some("pass") => Some(Rule::Pass),
                    Some("activate") => Some(Rule::Activate),
                    _ => None,
                };
                let owner = args
                    .get("owner")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                // A popup menu of an app (#241): Ossido owns its transient, this says where
                // the shell drew it.
                if let Some(serial) = args
                    .get("popupMenu")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                {
                    let sent = self
                        .shell
                        .manager()
                        .filter(|m| wayland_client::Proxy::version(*m) >= 9)
                        .is_some_and(|manager| {
                            manager.popup_menu_transient(
                                serial,
                                surface.wl_surface(),
                                region.as_ref().map(Region::wl_region),
                            );
                            true
                        });
                    if sent {
                        surface.commit();
                    }
                    return text_input::success(serde_json::json!(sent));
                }
                match self
                    .shell
                    .manager()
                    .filter(|m| wayland_client::Proxy::version(*m) >= 6)
                {
                    Some(manager) => {
                        let wl_region = region.as_ref().map(Region::wl_region);
                        match (rule("ownerRule"), rule("otherRule")) {
                            (Some(owner_rule), Some(other_rule))
                                if wayland_client::Proxy::version(manager) >= 7 =>
                            {
                                manager.set_owned_transient(
                                    surface.wl_surface(),
                                    wl_region,
                                    owner.to_owned(),
                                    owner_rule,
                                    other_rule,
                                );
                            }
                            _ => manager.set_transient(
                                surface.wl_surface(),
                                wl_region,
                                outside("press"),
                                outside("scroll"),
                            ),
                        }
                        true
                    }
                    None => false,
                }
            }
            Some("clearTransient") => match self
                .shell
                .manager()
                .filter(|m| wayland_client::Proxy::version(*m) >= 6)
            {
                Some(manager) => {
                    manager.clear_transient(surface.wl_surface());
                    true
                }
                None => false,
            },
            _ => return Vec::new(),
        };
        if done {
            surface.commit();
        }
        text_input::success(serde_json::json!(done))
    }

    /// A region from `[[x, y, w, h], …]` in surface points (empty rectangles skipped).
    fn region_from(&self, rects: &[serde_json::Value]) -> Option<Region> {
        let region = match Region::new(&self.compositor) {
            Ok(region) => region,
            Err(err) => {
                warn!("input region: {err}");
                return None;
            }
        };
        for rect in rects {
            let value = |i: usize| {
                rect.get(i)
                    .and_then(serde_json::Value::as_f64)
                    .filter(|v| v.is_finite())
                    .map(|v| v.round().clamp(-16384.0, 16384.0) as i32)
            };
            if let (Some(x), Some(y), Some(w), Some(h)) = (value(0), value(1), value(2), value(3))
                && w > 0
                && h > 0
            {
                region.add(x, y, w, h);
            }
        }
        Some(region)
    }

    /// Sends a key to the framework through the two paths it expects (key data and
    /// `flutter/keyevent` message); if neither consumes it, the editing action goes to
    /// the text field.
    fn send_key(&self, event: &KeyEvent, raw: Option<Vec<u8>>, edit: Option<EditKey>) {
        let Some(engine) = &self.engine else {
            return;
        };
        let join = Arc::new(Mutex::new(KeyJoin {
            remaining: if raw.is_some() { 2 } else { 1 },
            handled: false,
            edit,
            results: self.key_results.clone(),
        }));
        let data_join = Arc::clone(&join);
        if let Err(err) = engine.send_key(event, move |handled| KeyJoin::done(&data_join, handled))
        {
            warn!("{err}");
            KeyJoin::done(&join, false);
        }
        if let Some(raw) = raw {
            let raw_join = Arc::clone(&join);
            let sent = engine.send_message_with_reply("flutter/keyevent", &raw, move |reply| {
                let handled = serde_json::from_slice::<serde_json::Value>(&reply)
                    .ok()
                    .and_then(|reply| reply.get("handled")?.as_bool())
                    .unwrap_or(false);
                KeyJoin::done(&raw_join, handled);
            });
            if let Err(err) = sent {
                warn!("{err}");
                KeyJoin::done(&join, false);
            }
        }
    }

    fn gtk_modifiers(&self) -> i64 {
        use crate::keyboard::gtk_modifiers::{ALT, CAPS_LOCK, CONTROL, META, NUM_LOCK, SHIFT};
        let m = &self.modifiers;
        [
            (m.shift, SHIFT),
            (m.caps_lock, CAPS_LOCK),
            (m.ctrl, CONTROL),
            (m.alt, ALT),
            (m.num_lock, NUM_LOCK),
            (m.logo, META),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |mask, (_, bit)| mask | bit)
    }

    /// A key not consumed by the framework goes to the active text field.
    fn key_result(&mut self, result: KeyResult) {
        tracing::debug!(
            "key handled by the framework: {}, editing: {:?}",
            result.handled,
            result.edit
        );
        let (false, Some(edit)) = (result.handled, result.edit) else {
            return;
        };
        let Some(engine) = &self.engine else {
            return;
        };
        for message in self.text_input.handle_key(&edit) {
            if let Err(err) = engine.send_message(text_input::CHANNEL, &message) {
                warn!("{err}");
            }
        }
    }

    fn now_micros(&self) -> u64 {
        self.engine
            .as_ref()
            .map_or(0, |engine| engine.current_time() / 1000)
    }

    /// Runs the expired tasks and tells how long to wait for the next one.
    fn run_due_tasks(&mut self) -> Duration {
        let Some(engine) = &self.engine else {
            return IDLE_WAKEUP;
        };
        loop {
            let now = engine.current_time();
            match self.tasks.peek() {
                Some(Reverse(next)) if next.target_nanos <= now => {
                    if let Some(Reverse(queued)) = self.tasks.pop()
                        && let Err(err) = engine.run_task(&queued.task)
                    {
                        warn!("{err}");
                    }
                }
                Some(Reverse(next)) => {
                    return Duration::from_nanos(next.target_nanos - now).min(IDLE_WAKEUP);
                }
                None => return IDLE_WAKEUP,
            }
        }
    }

    /// The view whose window has `surface`.
    fn view_of(&self, surface: &wl_surface::WlSurface) -> Option<i64> {
        self.views
            .iter()
            .find(|(_, view)| view.surface.wl_surface() == surface)
            .map(|(id, _)| *id)
    }

    /// Whether window `id` has shown a frame (software here, OpenGL or Vulkan in the engine).
    fn view_presented(&self, id: i64) -> bool {
        let Some(view) = self.views.get(&id) else {
            return false;
        };
        if view.presented {
            return true;
        }
        match &self.engine {
            Some(engine) if self.gl && id == MAIN_VIEW => {
                engine.gl_presented()
                    || engine
                        .vulkan()
                        .is_some_and(|vulkan| vulkan.view_presented(id))
            }
            Some(engine) if self.gl => {
                engine.gl().is_some_and(|gl| gl.view_presented(id))
                    || engine
                        .vulkan()
                        .is_some_and(|vulkan| vulkan.view_presented(id))
            }
            _ => false,
        }
    }

    fn send_metrics(&self, id: i64) {
        let (Some(engine), Some(view)) = (&self.engine, self.views.get(&id)) else {
            return;
        };
        if !view.added {
            return;
        }
        let (w, h) = view.physical();
        // Before the first frame the EGL window takes the size at once (with d3d12 in WSL
        // Mesa applies a size only from the next buffer). Afterwards the raster thread
        // resizes it at the start of every frame, to that frame's size (`gl_fbo`; Vulkan
        // rebuilds its swapchain the same way): resizing it from here while a frame is being
        // drawn could reallocate the back buffer halfway through (a garbage frame, #160).
        if id == MAIN_VIEW && !engine.gl_presented() {
            engine.resize_surface(w, h);
        }
        if let Err(err) = engine.send_window_metrics(id, w, h, f64::from(view.scale.max(1))) {
            warn!("{err}");
        }
    }

    fn send_pointer(&self, phase: PointerPhase, position: (f64, f64), scroll: Option<(f64, f64)>) {
        let id = self.pointer_view;
        let (Some(engine), Some(view)) = (&self.engine, self.views.get(&id)) else {
            return;
        };
        if !view.added {
            return;
        }
        let scale = f64::from(view.scale.max(1));
        let event = PointerEvent {
            phase,
            x: position.0 * scale,
            y: position.1 * scale,
            buttons: self.buttons,
            scroll: scroll.map(|(x, y)| (x * scale, y * scale)),
            timestamp_micros: engine.current_time() / 1000,
            view_id: id,
        };
        if let Err(err) = engine.send_pointer(&event) {
            warn!("{err}");
        }
    }

    /// The windows on screen whose frame callbacks pace the engine.
    fn pacing_views(&self) -> Vec<i64> {
        self.views
            .iter()
            .filter(|(id, view)| {
                view.added && !view.removing && (**id != MAIN_VIEW || self.hidden_since.is_none())
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// The engine wants the next vsync: it arrives with the compositor's frame callback of
    /// a window on screen (the first that comes). Before a window's first frame the
    /// compositor sends none, so it answers right away.
    fn request_vsync(&mut self, baton: VsyncBaton, qh: &QueueHandle<Self>) {
        self.frame_requested_at = Some(std::time::Instant::now());
        let pacing = self.pacing_views();
        // No window on screen draws nothing (the engine waits until one comes back), except
        // while the app tears down its closed window or builds the next one (#142).
        if pacing.is_empty() {
            if self
                .free_run_until
                .is_some_and(|until| std::time::Instant::now() < until)
            {
                self.answer_vsync(baton);
            } else {
                self.vsync = Some(baton);
            }
            return;
        }
        let ready = pacing.iter().all(|id| {
            self.view_presented(*id) && self.views.get(id).is_some_and(|view| view.configured)
        });
        if !ready {
            self.answer_vsync(baton);
            return;
        }
        self.vsync = Some(baton);
        for id in pacing {
            let Some(view) = self.views.get_mut(&id) else {
                continue;
            };
            // A frame callback already asked for (by the vsync or by the presentation) is
            // enough.
            if view.vsync_requested || view.waiting_frame {
                continue;
            }
            let surface = view.surface.wl_surface();
            surface.frame(qh, surface.clone());
            view.surface.commit();
            view.vsync_requested = true;
        }
    }

    fn answer_vsync(&self, baton: VsyncBaton) {
        let Some(engine) = &self.engine else {
            return;
        };
        let start = engine.current_time();
        if let Err(err) = engine.on_vsync(baton, start, start.saturating_add(FRAME_PERIOD_NANOS)) {
            warn!("{err}");
        }
    }

    /// Presents window `id`'s last software frame, if the compositor is ready for one.
    fn present(&mut self, id: i64, qh: &QueueHandle<Self>) {
        if self.gl {
            return;
        }
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        if view.waiting_frame || !view.configured || !view.added {
            return;
        }
        let Some(frame) = view.pending.take() else {
            return;
        };
        if drop_stale_frame(
            (frame.width, frame.height),
            view.size,
            view.scale,
            view.stale_drops,
        ) {
            view.stale_drops = view.stale_drops.saturating_add(1);
            tracing::debug!(
                "frame {}x{} dropped: window {id} is now {:?} at scale {}",
                frame.width,
                frame.height,
                view.size,
                view.scale
            );
            if let Some(engine) = &self.engine
                && let Err(err) = engine.schedule_frame()
            {
                warn!("{err}");
            }
            return;
        }
        view.stale_drops = 0;
        let (Ok(width), Ok(height)) = (i32::try_from(frame.width), i32::try_from(frame.height))
        else {
            return;
        };
        let stride = width.saturating_mul(4);
        let (buffer, canvas) =
            match self
                .pool
                .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
            {
                Ok(created) => created,
                Err(err) => {
                    warn!("shm buffer not created: {err}");
                    return;
                }
            };
        let row = stride as usize;
        for (dst, src) in canvas
            .chunks_exact_mut(row)
            .zip(frame.pixels.chunks(frame.stride.max(1)))
        {
            if let (Some(dst), Some(src)) = (dst.get_mut(..row), src.get(..row)) {
                dst.copy_from_slice(src);
            }
        }

        // The glass measured on this frame goes with its commit.
        self.glass.before_present(id, frame.raster_seq);
        let surface = view.surface.wl_surface();
        surface.set_buffer_scale(view.scale.max(1));
        // Only the changed rectangle is damaged (silica-desktop#205).
        let (w, h) = (frame.width as usize, frame.height as usize);
        if let Some((x, y, dw, dh)) =
            crate::damage::changed_rect(&view.last_pixels, &frame.pixels, frame.stride, w, h)
        {
            surface.damage_buffer(x, y, dw, dh);
        }
        crate::damage::remember(&mut view.last_pixels, &frame.pixels, frame.stride, w, h);
        surface.frame(qh, surface.clone());
        if let Err(err) = buffer.attach_to(surface) {
            warn!("buffer attach failed: {err}");
            return;
        }
        view.surface.commit();
        view.buffer = Some(buffer);
        view.waiting_frame = true;
        if !view.presented {
            info!("window {id}: first frame presented");
        }
        view.presented = true;
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        let Some(id) = self.view_of(surface) else {
            return;
        };
        let gl = self.gl;
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        if factor != view.scale && factor >= 1 {
            view.scale = factor;
            if gl {
                // It applies from the next commit, that is from the swap of the frame at the new size.
                view.surface.wl_surface().set_buffer_scale(factor);
            }
            self.send_metrics(id);
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let Some(id) = self.view_of(surface) else {
            return;
        };
        if let Some(view) = self.views.get_mut(&id) {
            view.waiting_frame = false;
            view.vsync_requested = false;
        }
        if let Some(baton) = self.vsync.take() {
            self.answer_vsync(baton);
        }
        self.present(id, qh);
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl App {
    fn configured(&mut self, id: i64, qh: &QueueHandle<Self>, size: (u32, u32)) {
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        let resized = size.0 > 0 && size.1 > 0 && size != view.size;
        if resized {
            view.size = size;
        }
        view.configured = true;
        // A new window's view joins the engine once it is configured, at its size.
        if !view.added && !view.adding && !view.removing {
            self.add_to_engine(id);
            return;
        }
        // The new size goes to the engine at once, and the frame for it starts now (#160):
        // the vsync waiting for the frame callback is answered without waiting for it.
        self.send_metrics(id);
        if resized && let Some(baton) = self.vsync.take() {
            self.answer_vsync(baton);
        }
        self.present(id, qh);
    }
}

impl SessionLockHandler for App {
    /// Ossido locked the session: one lock surface for the output, drawn by a view of the
    /// engine (the shell's `lock` component).
    fn locked(&mut self, _: &Connection, qh: &QueueHandle<Self>, session_lock: SessionLock) {
        let Some(output) = self.output_state.outputs().next() else {
            warn!("session lock: no output");
            return;
        };
        let size = self
            .output_state
            .info(&output)
            .and_then(|info| info.logical_size)
            .map_or((1280, 800), |(w, h)| {
                (
                    u32::try_from(w).unwrap_or(1280),
                    u32::try_from(h).unwrap_or(800),
                )
            });
        let Some(engine) = &self.engine else {
            return;
        };
        let id = self.next_view;
        self.next_view = self.next_view.saturating_add(1);
        let surface = self.compositor.create_surface(qh);
        let lock_surface = session_lock.create_lock_surface(surface, &output, qh);
        let scale = self.views.get(&MAIN_VIEW).map_or(1, |view| view.scale);
        let mut view = View::new(Shell::Lock(lock_surface), size, scale, false);
        view.namespace = Some("lock".to_owned());
        if let Some(gl) = engine.gl()
            && let Err(err) = gl.add_view(id, view.surface.wl_surface(), view.physical())
        {
            warn!("lock view: {err}");
            return;
        }
        if let Some(vulkan) = engine.vulkan()
            && let Err(err) = vulkan.add_view(id, view.surface.wl_surface())
        {
            warn!("lock view: {err}");
            return;
        }
        self.glass.insert(
            id,
            self.glass_manager.clone(),
            &self.conn,
            view.surface.wl_surface(),
        );
        info!("lock view {id} opened");
        self.views.insert(id, view);
        self.session_lock = Some(session_lock);
    }

    /// The compositor refused the lock (someone holds it) or ended it.
    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        self.release_lock();
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _: u32,
    ) {
        if let Some(id) = self.view_of(surface.wl_surface()) {
            self.configured(id, qh, configure.new_size);
        }
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let id = self.view_of(layer.wl_surface()).unwrap_or(MAIN_VIEW);
        self.configured(id, qh, configure.new_size);
    }
}

impl PopupHandler for App {
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        popup: &Popup,
        configure: PopupConfigure,
    ) {
        let Some(id) = self.view_of(popup.wl_surface()) else {
            return;
        };
        let side = |v: i32| u32::try_from(v).unwrap_or(0);
        if let Some(view) = self.views.get_mut(&id) {
            view.popup_placed = Some((
                configure.position.0,
                configure.position.1,
                configure.width,
                configure.height,
            ));
        }
        self.configured(id, qh, (side(configure.width), side(configure.height)));
    }

    /// Ossido dismissed the popup (a press outside it, another window taking the focus):
    /// its view goes, and the app learns it with `windowClosed`.
    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        if let Some(id) = self.view_of(popup.wl_surface()) {
            info!("popup {id} dismissed");
            self.remove_view(id);
        }
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(id) = self.view_of(window.wl_surface()) {
            self.request_close(id);
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        let Some(id) = self.view_of(window.wl_surface()) else {
            return;
        };
        let size = match configure.new_size {
            (Some(w), Some(h)) => (w.get(), h.get()),
            _ => (0, 0),
        };
        let mut fullscreen_changed = None;
        if let Some(view) = self.views.get_mut(&id) {
            view.activated = Some(configure.is_activated());
            view.maximized = configure.is_maximized();
            if view.fullscreen != configure.is_fullscreen() {
                view.fullscreen = configure.is_fullscreen();
                fullscreen_changed = Some(view.fullscreen);
            }
        }
        if let (Some(enabled), Some(engine)) = (fullscreen_changed, &self.engine)
            && let Err(err) = engine.send_message(
                WINDOW_CHANNEL,
                &crate::drag::call(
                    "fullscreenChanged",
                    serde_json::json!({ "viewId": id, "enabled": enabled }),
                ),
            )
        {
            warn!("fullscreenChanged: {err}");
        }
        self.update_app_active();
        self.configured(id, qh, size);
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            // Key repeat is the client's job on Wayland: SCTK runs the timer from the
            // compositor's repeat_info and calls back for every repeat (held ⌫ keeps deleting).
            let repeat: smithay_client_toolkit::seat::keyboard::repeat::RepeatCallback<App> =
                Box::new(
                    |app: &mut App, _: &wl_keyboard::WlKeyboard, event: SeatKeyEvent| {
                        app.key_down(event);
                    },
                );
            match self.seat_state.get_keyboard_with_repeat(
                qh,
                &seat,
                None,
                self.loop_handle.clone(),
                repeat,
            ) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(err) => warn!("keyboard unavailable: {err}"),
            }
        }
        if capability == Capability::Pointer {
            self.seat = Some(seat.clone());
            if self.data_device.is_none()
                && let Some(manager) = &self.data_manager
            {
                self.data_device = Some(manager.get_data_device(qh, &seat));
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(pointer) => {
                    self.cursor_device = self
                        .cursor_shapes
                        .as_ref()
                        .map(|shapes| shapes.get_shape_device(&pointer, qh));
                    self.pointer = Some(pointer);
                }
                Err(err) => warn!("pointer unavailable: {err}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

fn button_mask(button: u32) -> i64 {
    match button {
        BTN_LEFT => 1,
        BTN_RIGHT => 2,
        BTN_MIDDLE => 4,
        _ => 0,
    }
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[SeatPointerEvent],
    ) {
        for event in events {
            // Every event goes to the view of the window it happened in.
            let Some(id) = self.view_of(&event.surface) else {
                continue;
            };
            self.pointer_view = id;
            let position = event.position;
            match event.kind {
                PointerEventKind::Enter { serial } => {
                    self.enter_serial = serial;
                    self.apply_cursor();
                    self.send_pointer(PointerPhase::Add, position, None);
                }
                PointerEventKind::Leave { .. } => {
                    // With a button pressed (a drag handed over to Ossido) the gesture in
                    // progress is cancelled before removing the pointer.
                    if self.buttons != 0 {
                        self.send_pointer(PointerPhase::Cancel, position, None);
                    }
                    self.buttons = 0;
                    self.send_pointer(PointerPhase::Remove, position, None);
                }
                PointerEventKind::Motion { .. } => {
                    let phase = if self.buttons == 0 {
                        PointerPhase::Hover
                    } else {
                        PointerPhase::Move
                    };
                    self.send_pointer(phase, position, None);
                }
                PointerEventKind::Press { button, serial, .. } => {
                    self.press_serial = serial;
                    self.input_serial = serial;
                    let was_up = self.buttons == 0;
                    self.buttons |= button_mask(button);
                    let phase = if was_up {
                        PointerPhase::Down
                    } else {
                        PointerPhase::Move
                    };
                    self.send_pointer(phase, position, None);
                }
                PointerEventKind::Release { button, .. } => {
                    self.buttons &= !button_mask(button);
                    let phase = if self.buttons == 0 {
                        PointerPhase::Up
                    } else {
                        PointerPhase::Move
                    };
                    self.send_pointer(phase, position, None);
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    let phase = if self.buttons == 0 {
                        PointerPhase::Hover
                    } else {
                        PointerPhase::Move
                    };
                    let delta = (horizontal.absolute, vertical.absolute);
                    self.send_pointer(phase, position, Some(delta));
                }
            }
        }
    }
}

impl App {
    /// A press, or a repeat of the held key (the second press without a release is a
    /// `KeyPhase::Repeat` for Flutter, and the text field gets the edit again).
    fn key_down(&mut self, event: SeatKeyEvent) {
        let keysym = event.keysym.raw();
        let utf8 = event.utf8.as_deref();
        let micros = self.now_micros();
        let key = self.keys.press(event.raw_code, keysym, utf8, micros);
        let raw = raw_key_message(true, event.raw_code, keysym, utf8, self.gtk_modifiers());
        let edit = edit_key(
            keysym,
            utf8,
            self.modifiers.shift,
            self.modifiers.ctrl || self.modifiers.alt,
        );
        self.send_key(&key, Some(raw), edit);
    }
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        // With several windows the framework moves its focus into the view with the keyboard
        // (the Info window's field, not the main window's).
        let id = self.view_of(surface);
        self.keyboard_view = id;
        if let (true, Some(id), Some(engine)) = (self.multi_view, id, &self.engine)
            && self.views.get(&id).is_some_and(|view| view.added)
            && let Err(err) = engine.send_view_focus(id, true)
        {
            warn!("view focus: {err}");
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        // Outside the window releases do not arrive: they are synthesized now.
        let micros = self.now_micros();
        for event in self.keys.release_all(micros) {
            self.send_key(&event, None, None);
        }
        // The keyboard going to one of its popups (a menu) leaves the window active: a menu
        // does not make its window look inactive.
        let has_popup = |app: &Self, id: i64| {
            app.views
                .values()
                .any(|view| view.popup_parent == Some(id) && !view.removing)
        };
        if let (true, Some(id), Some(engine)) =
            (self.multi_view, self.view_of(surface), &self.engine)
            && !has_popup(self, id)
            && self
                .views
                .get(&id)
                .is_some_and(|view| view.added && !view.removing)
            && let Err(err) = engine.send_view_focus(id, false)
        {
            warn!("view focus: {err}");
        }
        self.keyboard_view = None;
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: SeatKeyEvent,
    ) {
        self.input_serial = serial;
        self.key_down(event);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: SeatKeyEvent,
    ) {
        let micros = self.now_micros();
        if let Some(key) = self.keys.release(event.raw_code, micros) {
            let raw = raw_key_message(
                false,
                event.raw_code,
                event.keysym.raw(),
                None,
                self.gtk_modifiers(),
            );
            self.send_key(&key, Some(raw), None);
        }
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl DataDeviceHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &wl_surface::WlSurface,
    ) {
        let Some(id) = self.view_of(surface) else {
            return;
        };
        self.drag_view = id;
        let Some(offer) = self
            .data_device
            .as_ref()
            .filter(|d| d.inner() == device)
            .and_then(|d| d.data().drag_offer())
        else {
            return;
        };
        let files = offer.with_mime_types(|mimes| mimes.iter().any(|m| m == crate::drag::URI_LIST));
        if !files {
            offer.accept_mime_type(offer.serial, None);
            return;
        }
        offer.accept_mime_type(offer.serial, Some(crate::drag::URI_LIST.to_owned()));
        offer.set_actions(DndAction::Copy | DndAction::Move, DndAction::Copy);
        self.drag_in = true;
        self.send_drag("enter", serde_json::json!({ "x": x, "y": y, "viewId": id }));
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        if std::mem::take(&mut self.drag_in) {
            self.send_drag("leave", serde_json::Value::Null);
        }
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, x: f64, y: f64) {
        if self.drag_in {
            self.send_drag(
                "motion",
                serde_json::json!({ "x": x, "y": y, "viewId": self.drag_view }),
            );
        }
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        if !std::mem::take(&mut self.drag_in) {
            return;
        }
        let Some(offer) = self
            .data_device
            .as_ref()
            .filter(|d| d.inner() == device)
            .and_then(|d| d.data().drag_offer())
        else {
            return;
        };
        let (x, y) = (offer.x, offer.y);
        let pipe = match offer.receive(crate::drag::URI_LIST.to_owned()) {
            Ok(pipe) => pipe,
            Err(err) => {
                warn!("unreadable drop: {err}");
                return;
            }
        };
        let Some(sender) = self.drops.clone() else {
            return;
        };
        self.dropped = Some(offer);
        std::thread::spawn(move || {
            let mut data = Vec::new();
            let mut file = std::fs::File::from(std::os::fd::OwnedFd::from(pipe));
            // At most 16 MB of URIs: beyond that it is truncated.
            let _ = std::io::Read::read_to_end(
                &mut std::io::Read::take(&mut file, 16 << 20),
                &mut data,
            );
            let _ = sender.send((x, y, data));
        });
    }
}

impl DataOfferHandler for App {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl DataSourceHandler for App {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        mut pipe: WritePipe,
    ) {
        if let Some((copied, text)) = &self.clipboard
            && copied.inner() == source
        {
            if crate::clipboard::TEXT_TYPES.contains(&mime.as_str()) {
                let text = Arc::clone(text);
                // On a thread: the reader may be this very window.
                std::thread::spawn(move || {
                    let _ = std::io::Write::write_all(&mut pipe, &text);
                });
            }
            return;
        }
        let Some(drag) = self
            .drag_out
            .as_ref()
            .filter(|d| d.source.inner() == source)
        else {
            return;
        };
        if mime != crate::drag::URI_LIST {
            return;
        }
        // On a thread: the reader may be this very window.
        let body = Arc::clone(&drag.body);
        std::thread::spawn(move || {
            if let Err(err) = std::io::Write::write_all(&mut pipe, &body) {
                warn!("drag: data not sent: {err}");
            }
        });
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        // Another client took the selection.
        if self
            .clipboard
            .as_ref()
            .is_some_and(|(copied, _)| copied.inner() == source)
        {
            self.clipboard = None;
            source.destroy();
            return;
        }
        if self
            .drag_out
            .as_ref()
            .is_some_and(|d| d.source.inner() == source)
        {
            self.finish_drag_out(false);
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if self
            .drag_out
            .as_ref()
            .is_some_and(|d| d.source.inner() == source)
        {
            self.finish_drag_out(true);
        }
    }

    fn action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        action: DndAction,
    ) {
        if let Some(drag) = self
            .drag_out
            .as_mut()
            .filter(|d| d.source.inner() == source)
        {
            drag.action = action;
        }
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_pointer!(App);
delegate_keyboard!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_xdg_popup!(App);
delegate_layer!(App);
delegate_session_lock!(App);
delegate_registry!(App);
delegate_data_device!(App);
wayland_client::delegate_noop!(App: ignore SilicaGlassManagerV1);
wayland_client::delegate_noop!(App: ignore SilicaWindowManagerV1);
wayland_client::delegate_noop!(App: ignore SilicaWindowV1);
wayland_client::delegate_noop!(App: ignore SilicaGlassV1);
wayland_client::delegate_noop!(App: ignore SilicaMenuManagerV1);
wayland_client::delegate_noop!(App: ignore ZxdgExporterV2);
wayland_client::delegate_noop!(App: ignore ZxdgImporterV2);

/// The handle of a window exported for another app (`exportHandle`).
impl wayland_client::Dispatch<ZxdgExportedV2, i64> for App {
    fn event(
        app: &mut Self,
        _: &ZxdgExportedV2,
        event: zxdg_exported_v2::Event,
        id: &i64,
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_exported_v2::Event::Handle { handle } = event {
            app.export_ready(*id, handle);
        }
    }
}

/// The parent window of `--parent` is gone (or the handle was never valid).
impl wayland_client::Dispatch<ZxdgImportedV2, ()> for App {
    fn event(
        _: &mut Self,
        _: &ZxdgImportedV2,
        event: zxdg_imported_v2::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_imported_v2::Event::Destroyed = event {
            info!("--parent: the parent window is gone");
        }
    }
}

/// An item of the app's menus chosen in the bar (`menu.rs`).
impl wayland_client::Dispatch<SilicaMenuV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &SilicaMenuV1,
        event: silica_menu_v1::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
        if let silica_menu_v1::Event::Activated { id } = event
            && let Some(engine) = &app.engine
            && let Err(err) =
                engine.send_message(crate::menu::CHANNEL, &crate::menu::activated_message(id))
        {
            warn!("silica/menu: {err}");
        }
    }
}
impl wayland_client::Dispatch<SilicaShellManagerV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &SilicaShellManagerV1,
        event: silica_protocols::shell::client::silica_shell_manager_v1::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
        use silica_protocols::shell::client::silica_shell_manager_v1::Event;
        let message = match event {
            Event::Focused { identifier } => app.shell.set_focused(identifier),
            // A catalog command for the shell (e.g. faro.toggle).
            Event::Command { id } => {
                serde_json::to_vec(&serde_json::json!({ "method": "command", "args": id }))
                    .unwrap_or_default()
            }
            Event::Minimized {
                identifier,
                minimized,
            } => app.shell.set_minimized(identifier, minimized != 0),
            Event::Thumbnail {
                identifier,
                width,
                height,
            } => app.shell.thumbnail_ready(&identifier, width, height),
            // The state announced by a shell component (Control Center open…).
            Event::Broadcast { topic, value } => serde_json::to_vec(&serde_json::json!({
                "method": "broadcast",
                "args": { "topic": topic, "value": value },
            }))
            .unwrap_or_default(),
            // A press outside the open menu or panel ended it (#149).
            // With several layers in this engine, the view whose transient ended.
            Event::Dismissed { surface } => serde_json::to_vec(&serde_json::json!({
                "method": "transientDismissed",
                "args": app.view_of(&surface).map(|id| serde_json::json!({ "viewId": id })),
            }))
            .unwrap_or_default(),
            // The appearance of the window of the popover that follows.
            Event::WindowMenuAppearance {
                identifier,
                appearance,
            } => serde_json::to_vec(&serde_json::json!({
                "method": "windowMenuAppearance",
                "args": { "id": identifier, "appearance": crate::window_appearance::name(appearance) },
            }))
            .unwrap_or_default(),
            // The frontmost app's menus (silica-menu-v1, #25).
            Event::AppMenu {
                app_id,
                serial,
                fd,
                size,
            } => {
                let message = crate::menu::shell_message(&app_id, serial, fd, size);
                app.shell.remember_app_menu(&message);
                message
            }
            // A context menu or pop-up button's menu of an app, drawn by the shell (#241).
            Event::PopupMenu {
                serial,
                app_id,
                kind,
                x,
                y,
                width,
                height,
                selected,
                appearance,
            } => {
                app.shell.begin_popup_menu(
                    serial,
                    app_id,
                    kind,
                    [x, y, width, height],
                    selected,
                    appearance,
                );
                return;
            }
            Event::PopupMenuRow {
                serial,
                kind,
                id,
                label,
                flags,
                icon,
                key,
                modifiers,
            } => {
                app.shell
                    .popup_menu_row(serial, kind, id, label, flags, icon, key, modifiers);
                return;
            }
            Event::PopupMenuEnd { serial } => match app.shell.end_popup_menu(serial) {
                Some(message) => message,
                None => return,
            },
            Event::PopupMenuClosed { serial } => serde_json::to_vec(&serde_json::json!({
                "method": "popupMenuClosed",
                "args": serial,
            }))
            .unwrap_or_default(),
            // Long press on the green button: the arrangement popover.
            Event::WindowMenu { identifier, x, y } => serde_json::to_vec(&serde_json::json!({
                "method": "windowMenu",
                "args": { "id": identifier, "x": x, "y": y },
            }))
            .unwrap_or_default(),
            _ => return,
        };
        if let Some(engine) = &app.engine
            && let Err(err) = engine.send_message(shell::CHANNEL, &message)
        {
            warn!("shell event to Dart: {err}");
        }
    }
}

impl wayland_client::Dispatch<ExtForeignToplevelListV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            app.shell.add(toplevel);
        }
    }

    wayland_client::event_created_child!(App, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl wayland_client::Dispatch<ExtForeignToplevelHandleV1, ()> for App {
    fn event(
        app: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &wayland_client::Connection,
        _: &QueueHandle<Self>,
    ) {
        if app.shell.event(handle, event)
            && let Some(engine) = &app.engine
            && let Err(err) = engine.send_message(shell::CHANNEL, &app.shell.toplevels_message())
        {
            warn!("window list to Dart: {err}");
        }
    }
}

/// The app's answer to `closeRequested` (a JSONMethodCodec success envelope): true when the
/// app closes the window itself or keeps it open. No handler (an empty reply, an error, an
/// older silica_ui) or a malformed reply is false: the window closes now.
fn close_answer(reply: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(reply)
        .ok()
        .and_then(|reply| reply.get(0).and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

#[cfg(test)]
mod lifecycle_tests {
    use super::{close_answer, lifecycle_message};

    #[test]
    fn the_apps_answer_to_a_close_request_defaults_to_closing() {
        assert!(close_answer(b"[true]"));
        assert!(!close_answer(b"[false]"));
        assert!(!close_answer(b""));
        assert!(!close_answer(b"[null]"));
        assert!(!close_answer(b"[\"error\",\"x\",null]"));
        assert!(!close_answer(b"not json"));
    }

    #[test]
    fn window_sides_from_the_app_are_bounded() {
        use super::window_side;
        assert_eq!(window_side(230.4), Some(230));
        assert_eq!(window_side(0.5), None);
        assert_eq!(window_side(-3.0), None);
        assert_eq!(window_side(f64::NAN), None);
        assert_eq!(window_side(1e9), Some(8192));
    }

    #[test]
    fn frames_of_a_stale_size_are_dropped_a_few_at_most() {
        use super::{MAX_STALE_FRAMES, drop_stale_frame};
        // The frame of the size configured, at scale 1 and 2: presented.
        assert!(!drop_stale_frame((300, 200), (300, 200), 1, 0));
        assert!(!drop_stale_frame((600, 400), (300, 200), 2, 0));
        // A frame drawn before the new size arrived: dropped.
        assert!(drop_stale_frame((300, 200), (340, 200), 1, 0));
        assert!(drop_stale_frame((300, 200), (300, 200), 2, 1));
        // Not forever: after a few in a row one is shown anyway.
        assert!(!drop_stale_frame(
            (300, 200),
            (340, 200),
            1,
            MAX_STALE_FRAMES
        ));
        // A scale of 0 or less counts as 1.
        assert!(!drop_stale_frame((300, 200), (300, 200), 0, 0));
    }

    #[test]
    fn the_active_window_is_resumed_the_others_inactive() {
        assert_eq!(lifecycle_message(true), "AppLifecycleState.resumed");
        assert_eq!(lifecycle_message(false), "AppLifecycleState.inactive");
    }
}
