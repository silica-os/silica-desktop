//! The lock screen: a session lock needs the `lock` layer, and while it is on the windows get
//! neither clicks nor shortcuts.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;

use common::Headless;
use common::client::{FIRST_WINDOW_AT, TestWindow};
use serde_json::{Value, json};
use silica_control::commands::{INPUT_CLICK, WINDOW_LIST, WindowInfo};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, ZwlrLayerShellV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self, Anchor, ZwlrLayerSurfaceV1,
};

#[derive(Default)]
struct App {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    size: Option<(u32, u32)>,
}

impl Dispatch<WlRegistry, ()> for App {
    fn event(
        app: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => app.compositor = Some(registry.bind(name, 4, qh, ())),
                "wl_shm" => app.shm = Some(registry.bind(name, 1, qh, ())),
                "zwlr_layer_shell_v1" => app.layer_shell = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for App {
    fn event(
        app: &mut Self,
        surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            surface.ack_configure(serial);
            app.size = Some((width, height));
        }
    }
}

impl Dispatch<WlShm, ()> for App {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: WlShmPool);
delegate_noop!(App: WlRegion);
delegate_noop!(App: ignore ZwlrLayerShellV1);

#[test]
fn a_locked_session_gives_the_windows_no_clicks_and_no_shortcuts() {
    let ossido = Headless::start("lock");
    let _first = TestWindow::open(&ossido.wayland, "first");
    let _second = TestWindow::open(&ossido.wayland, "second");
    let mut client = ossido.client();
    // Without a lock screen, locking is refused (nobody could unlock).
    assert!(client.call("session.lock", Value::Null).is_err());

    // The lock screen: a full-screen overlay named `lock` that draws and takes the clicks.
    let conn = Connection::from_socket(UnixStream::connect(&ossido.wayland).unwrap()).unwrap();
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let mut app = App::default();
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut app).unwrap();
    let surface = app.compositor.as_ref().unwrap().create_surface(&qh, ());
    let layer = app.layer_shell.as_ref().unwrap().get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        "lock".to_owned(),
        &qh,
        (),
    );
    layer.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
    surface.commit();
    while app.size.is_none() {
        queue.blocking_dispatch(&mut app).unwrap();
    }
    let (w, h) = app.size.unwrap();
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&vec![0xFFu8; (w * h * 4) as usize]).unwrap();
    let pool = app
        .shm
        .as_ref()
        .unwrap()
        .create_pool(file.as_fd(), (w * h * 4) as i32, &qh, ());
    let buffer = pool.create_buffer(
        0,
        w as i32,
        h as i32,
        (w * 4) as i32,
        wl_shm::Format::Argb8888,
        &qh,
        (),
    );
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    queue.roundtrip(&mut app).unwrap();

    let windows = |client: &mut silica_control::Client| -> Vec<WindowInfo> {
        serde_json::from_value(client.call(WINDOW_LIST, Value::Null).unwrap()).unwrap()
    };
    assert_eq!(windows(&mut client).len(), 2);
    client.call("session.lock", Value::Null).unwrap();
    assert_eq!(
        client.call("idle.state", Value::Null).unwrap()["lock"]["locked"],
        true
    );

    // A click on the first window does not reach it (the lock screen is above); neither does
    // Logo+M, which would minimize the window.
    let at = FIRST_WINDOW_AT as f64;
    client
        .call(INPUT_CLICK, json!({"x": at + 5.0, "y": at + 30.0}))
        .unwrap();
    // (A click through would have raised the first window.)
    assert_eq!(windows(&mut client)[0].title.as_deref(), Some("second"));
    client
        .call("input.key", json!({"key": "Super_L", "pressed": true}))
        .unwrap();
    client.call("input.key", json!({"key": "m"})).unwrap();
    client
        .call("input.key", json!({"key": "Super_L", "pressed": false}))
        .unwrap();
    assert!(windows(&mut client).iter().all(|w| !w.minimized));

    // Unlocked, the clicks and the shortcut work again.
    client.call("session.unlock", Value::Null).unwrap();
    // The lock screen hides: an empty input region, as the shell does.
    let empty = app.compositor.as_ref().unwrap().create_region(&qh, ());
    surface.set_input_region(Some(&empty));
    surface.commit();
    queue.roundtrip(&mut app).unwrap();
    client
        .call(INPUT_CLICK, json!({"x": at + 5.0, "y": at + 30.0}))
        .unwrap();
    assert_eq!(windows(&mut client)[0].title.as_deref(), Some("first"));
    client
        .call("input.key", json!({"key": "Super_L", "pressed": true}))
        .unwrap();
    client.call("input.key", json!({"key": "m"})).unwrap();
    client
        .call("input.key", json!({"key": "Super_L", "pressed": false}))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !windows(&mut client).iter().any(|w| w.minimized) {
        assert!(
            std::time::Instant::now() < deadline,
            "Logo+M did not minimize"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
