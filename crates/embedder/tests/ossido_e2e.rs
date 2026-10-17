//! The demo app runs in the embedder inside headless Ossido: first frame, click, HiDPI, close.
//!
//! The engine and the bundle are needed: `cargo xtask engine` and `cargo xtask bundle demo`
//! (`cargo xtask ci` prepares them by itself).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use silica_control::commands::{
    INPUT_CLICK, INPUT_KEY, INPUT_TYPE, LAYER_LIST, LayerInfo, WINDOW_CLOSE, WINDOW_FOCUS,
    WINDOW_LIST, WindowInfo,
};
use silica_control::{Client, ScreenshotParams};

const IDLE: [u8; 4] = [0x33, 0x66, 0xCC, 0xFF];
const TAPPED: [u8; 4] = [0xCC, 0x66, 0x33, 0xFF];
const TYPED: [u8; 4] = [0x33, 0xCC, 0x66, 0xFF];
const LO_DPI: [u8; 4] = [0xFF, 0x00, 0xFF, 0xFF];
const HI_DPI: [u8; 4] = [0x00, 0xFF, 0x00, 0xFF];
const POPUP: [u8; 4] = [0xEE, 0x22, 0x88, 0xFF];
const APP_ID: &str = "dev.frascella.silica.Flutter";
const CHILD_ENV: &str = "SILICA_E2E_OSSIDO";
const TIMEOUT: Duration = Duration::from_secs(20);

/// Not a test: it is Ossido itself, when this binary is relaunched by `Session`.
#[test]
fn ossido_child_process() {
    let Ok(config) = std::env::var(CHILD_ENV) else {
        return;
    };
    let (size, scale) = config.split_once('@').unwrap();
    let (w, h) = size.split_once('x').unwrap();
    ossido::run(ossido::Options {
        backend: ossido::BackendKind::Headless,
        size: (w.parse().unwrap(), h.parse().unwrap()),
        scale: scale.parse().unwrap(),
        control: Some(0),
        // Deterministic screenshots: windows appear right away at full size.
        animation_scale: 0.0,
        ..Default::default()
    })
    .unwrap();
    std::process::exit(0);
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A child process that dies with the test, even if the test panics.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Session {
    dir: PathBuf,
    _ossido: KillOnDrop,
    embedder: Option<Child>,
    others: Vec<KillOnDrop>,
    control: String,
    wayland: String,
    output: (u32, u32),
}

impl Session {
    /// Headless Ossido of `output` physical pixels at scale `scale`, with the demo app in a
    /// window of `window` logical pixels.
    fn start(name: &str, output: (u32, u32), scale: u32, window: (u32, u32)) -> Self {
        Self::start_with(name, output, scale, &size_args(window))
    }

    /// Like `start`, with embedder arguments of choice (for example a layer).
    fn start_with(name: &str, output: (u32, u32), scale: u32, args: &[String]) -> Self {
        let engine = silica_embedder::default_engine_dir();
        let bundle = repo().join("testapps/demo/build/flutter_assets");
        assert!(
            engine.join("libflutter_engine.so").exists() && bundle.join("kernel_blob.bin").exists(),
            "the engine and the bundle are needed: cargo xtask engine && cargo xtask bundle demo"
        );

        let dir = std::env::temp_dir().join(format!("silica-e2e-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();

        let mut ossido = KillOnDrop(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "ossido_child_process",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD_ENV, format!("{}x{}@{scale}", output.0, output.1))
                .env("XDG_RUNTIME_DIR", &dir)
                .env_remove("WAYLAND_DISPLAY")
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut wayland = None;
        let mut control = None;
        // libtest writes "test ossido_child_process ... " on the same line as Ossido's first
        // println!: the variable is searched anywhere in the line.
        let value = |line: &str, key: &str| line.split_once(key).map(|(_, v)| v.trim().to_owned());
        for line in BufReader::new(ossido.0.stdout.take().unwrap()).lines() {
            let line = line.unwrap();
            if let Some(name) = value(&line, "WAYLAND_DISPLAY=") {
                wayland = Some(name);
            }
            if let Some(addr) = value(&line, "OSSIDO_CONTROL=") {
                control = Some(addr);
                break;
            }
        }

        let wayland = wayland.expect("Ossido without a Wayland socket");
        let embedder = spawn_embedder(&dir, &wayland, args);
        Self {
            dir,
            _ossido: ossido,
            embedder: Some(embedder),
            others: Vec::new(),
            control: control.expect("Ossido without a control channel"),
            wayland,
            output,
        }
    }

    /// Another instance of the demo app in the same session.
    fn spawn(&mut self, args: &[String]) {
        self.others
            .push(KillOnDrop(spawn_embedder(&self.dir, &self.wayland, args)));
    }

    fn layer(&self, namespace: &str) -> LayerInfo {
        wait_for("the layer", || {
            let list = self.client().call(LAYER_LIST, Value::Null).unwrap();
            let layers: Vec<LayerInfo> = serde_json::from_value(list).unwrap();
            layers
                .into_iter()
                .find(|l| l.namespace == namespace && l.width > 0 && l.height > 0)
        })
    }

    fn client(&self) -> Client {
        Client::connect(&self.control).unwrap()
    }

    fn window(&self) -> WindowInfo {
        wait_for("the embedder window", || {
            let list = self.client().call(WINDOW_LIST, Value::Null).unwrap();
            let windows: Vec<WindowInfo> = serde_json::from_value(list).unwrap();
            windows
                .into_iter()
                .find(|w| w.app_id.as_deref() == Some(APP_ID) && w.width > 0)
        })
    }

    fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let path = self.dir.join("shot.png");
        self.client()
            .screenshot(&ScreenshotParams {
                path: Some(path.to_string_lossy().into_owned()),
            })
            .unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(std::fs::read(&path).unwrap()))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut pixels).unwrap();
        let offset = ((y * self.output.0 + x) * 4) as usize;
        pixels[offset..offset + 4].try_into().unwrap()
    }

    /// Waits for the physical pixel (x, y) to become `color`.
    fn expect_pixel(&self, x: u32, y: u32, color: [u8; 4]) {
        let mut last = [0; 4];
        let found = wait_until(|| {
            last = self.pixel(x, y);
            last == color
        });
        assert!(found, "pixel ({x}, {y}) = {last:?}, expected {color:?}");
    }

    /// Like `expect_pixel`, within a few levels (the faint edge of another window's shadow).
    fn expect_pixel_near(&self, x: u32, y: u32, color: [u8; 4]) {
        let mut last = [0; 4];
        let found = wait_until(|| {
            last = self.pixel(x, y);
            last.iter().zip(color).all(|(a, b)| a.abs_diff(b) <= 4)
        });
        assert!(
            found,
            "pixel ({x}, {y}) = {last:?}, expected about {color:?}"
        );
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(mut embedder) = self.embedder.take() {
            let _ = embedder.kill();
            let _ = embedder.wait();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn size_args((w, h): (u32, u32)) -> Vec<String> {
    vec!["--size".to_owned(), format!("{w}x{h}")]
}

fn spawn_embedder(dir: &Path, wayland: &str, args: &[String]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_silica-embedder"))
        .args(args)
        .arg(repo().join("testapps/demo/build/flutter_assets"))
        .env("XDG_RUNTIME_DIR", dir)
        .env("WAYLAND_DISPLAY", wayland)
        // Headless Ossido offers no GPU buffers: Mesa draws in software (llvmpipe) on wl_shm.
        .env("LIBGL_ALWAYS_SOFTWARE", "1")
        .env(
            "RUST_LOG",
            std::env::var("E2E_LOG").unwrap_or_else(|_| "warn".into()),
        )
        .spawn()
        .unwrap()
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn wait_for<T>(what: &str, mut find: impl FnMut() -> Option<T>) -> T {
    let mut found = None;
    assert!(
        wait_until(|| {
            found = find();
            found.is_some()
        }),
        "{what} did not arrive within {TIMEOUT:?}"
    );
    found.unwrap()
}

#[test]
fn demo_renders_reacts_to_clicks_and_closes() {
    let mut session = Session::start("demo", (640, 480), 1, (300, 200));
    let window = session.window();
    assert_eq!((window.width, window.height), (300, 200));
    let (x, y) = (window.x as u32, window.y as u32);

    // First frame: background and probe square at density 1.
    session.expect_pixel(x + 150, y + 190, IDLE);
    // Inside the probe but outside the corner cut by Ossido (superellipse, radius 21.5).
    session.expect_pixel(x + 15, y + 15, LO_DPI);

    // A click crosses Ossido, Wayland, the embedder and Flutter.
    session
        .client()
        .call(INPUT_CLICK, json!({"x": x + 150, "y": y + 100}))
        .unwrap();
    session.expect_pixel(x + 150, y + 190, TAPPED);

    // window.close: as on the reference the app keeps running without windows (#142). The
    // window leaves the list and the process stays, registered in the app registry.
    session.client().call(WINDOW_CLOSE, Value::Null).unwrap();
    assert!(
        wait_until(|| {
            let list = session.client().call(WINDOW_LIST, Value::Null).unwrap();
            let windows: Vec<WindowInfo> = serde_json::from_value(list).unwrap();
            windows.iter().all(|w| w.app_id.as_deref() != Some(APP_ID))
        }),
        "the closed window is still listed"
    );
    let mut embedder = session.embedder.take().unwrap();
    assert!(
        embedder.try_wait().unwrap().is_none(),
        "the app ended with its window"
    );
    let registry = session.dir.join("silica-apps");
    assert_eq!(
        silica_control::apps::instances(&registry, Some(APP_ID)).len(),
        1,
        "the app is not in the registry"
    );

    // Launched again: the running process shows its window and the new one exits at once.
    let mut second = spawn_embedder(&session.dir, &session.wayland, &size_args((300, 200)));
    let mut second_status = None;
    assert!(
        wait_until(|| {
            second_status = second.try_wait().unwrap();
            second_status.is_some()
        }),
        "the second launch did not hand over"
    );
    assert!(second_status.unwrap().success());
    let shown = session.window();
    assert_eq!((shown.width, shown.height), (300, 200));
    session.expect_pixel(x + 150, y + 190, TAPPED);

    // app.quit with the app id (Quit in the Dock menu): it ends cleanly.
    session
        .client()
        .call("app.quit", json!({ "app_id": APP_ID }))
        .unwrap();
    let mut status = None;
    assert!(
        wait_until(|| {
            status = embedder.try_wait().unwrap();
            status.is_some()
        }),
        "the embedder did not exit"
    );
    assert!(status.unwrap().success(), "exit with {status:?}");
}

/// The demo answering the main window's close request as `mode` (`--close=MODE`).
fn close_session(name: &str, mode: &str) -> (Session, WindowInfo) {
    let mut args = size_args((300, 200));
    args.extend(["--dart-arg".to_owned(), format!("--close={mode}")]);
    let session = Session::start_with(name, (640, 480), 1, &args);
    let window = session.window();
    // The app is up (its first frame), so a close is not answered by a starting engine.
    session.expect_pixel(window.x as u32 + 150, window.y as u32 + 190, IDLE);
    (session, window)
}

fn listed(session: &Session) -> bool {
    let list = session.client().call(WINDOW_LIST, Value::Null).unwrap();
    let windows: Vec<WindowInfo> = serde_json::from_value(list).unwrap();
    windows.iter().any(|w| w.app_id.as_deref() == Some(APP_ID))
}

/// Closes the main window and returns how long it took to leave the list.
fn close_main(session: &Session) -> Duration {
    let start = Instant::now();
    session.client().call(WINDOW_CLOSE, Value::Null).unwrap();
    assert!(
        wait_until(|| !listed(session)),
        "the closed window is still listed"
    );
    start.elapsed()
}

/// An app that does not answer `closeRequested` (silica-desktop#192): with no listener on
/// `silica/window` (silica_ui before 0.35, an app that never opens a window) the engine
/// buffers the request and no answer ever comes; a listener may hang. The window still
/// closes, within the embedder's wait.
#[test]
fn the_main_window_closes_when_the_app_does_not_answer() {
    for mode in ["none", "silent"] {
        let (session, _) = close_session(&format!("close-{mode}"), mode);
        let took = close_main(&session);
        assert!(
            took < Duration::from_secs(2),
            "--close={mode}: the window closed after {took:?}"
        );
    }
}

/// An app asking the user before its window closes (Terminal with a running process) keeps
/// it open as long as it asks; its answer closes it (silica-desktop#192). It says so with
/// `closeAsking`, or (silica_ui before 0.38) draws its question.
#[test]
fn a_close_being_asked_waits_for_the_answer() {
    a_close_being_asked_waits("ask");
}

#[test]
fn a_close_being_asked_by_drawing_waits_for_the_answer() {
    a_close_being_asked_waits("draw");
}

fn a_close_being_asked_waits(mode: &str) {
    let (session, window) = close_session(&format!("close-{mode}"), mode);
    session.client().call(WINDOW_CLOSE, Value::Null).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        listed(&session),
        "the window closed while the app was asking"
    );
    // A second close while asking is not asked twice: the window stays.
    session.client().call(WINDOW_CLOSE, Value::Null).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert!(listed(&session), "a second close request closed the window");
    // The answer (a tap in the demo): the app closes it.
    session
        .client()
        .call(
            INPUT_CLICK,
            json!({"x": window.x + 150, "y": window.y + 100}),
        )
        .unwrap();
    assert!(
        wait_until(|| !listed(&session)),
        "the window did not close after the answer"
    );
}

#[test]
fn keyboard_reaches_text_field_and_hardware_keyboard() {
    let session = Session::start("keys", (640, 480), 1, (300, 200));
    let window = session.window();
    let (x, y) = (window.x as u32, window.y as u32);
    session.expect_pixel(x + 150, y + 30, IDLE);

    // Text: keys not consumed by the framework → flutter/textinput → EditableText.
    let mut client = session.client();
    client.call(INPUT_TYPE, json!({"text": "word"})).unwrap();
    session.expect_pixel(x + 150, y + 30, TYPED);

    // Backspace and retyping go through the embedder's editing model.
    client.call(INPUT_KEY, json!({"key": "BackSpace"})).unwrap();
    session.expect_pixel(x + 150, y + 30, IDLE);
    client.call(INPUT_TYPE, json!({"text": "d"})).unwrap();
    session.expect_pixel(x + 150, y + 30, TYPED);

    // Esc: key event handled by HardwareKeyboard, which clears the field.
    client.call(INPUT_KEY, json!({"key": "Escape"})).unwrap();
    session.expect_pixel(x + 150, y + 30, IDLE);
}

#[test]
fn demo_renders_at_scale_two() {
    let session = Session::start("hidpi", (640, 480), 2, (200, 150));
    let window = session.window();
    // Logical geometry unchanged; the screenshot is in physical pixels.
    assert_eq!((window.width, window.height), (200, 150));
    let (x, y) = (window.x as u32 * 2, window.y as u32 * 2);
    session.expect_pixel(x + 30, y + 30, HI_DPI);
    // The probe square is 16 logical pixels, so 32 physical.
    session.expect_pixel(x + 31, y + 31, HI_DPI);
    session.expect_pixel(x + 33, y + 33, IDLE);
}

#[test]
fn gl_renderer_draws_at_scale_two_and_reacts_to_clicks() {
    let mut args = size_args((200, 150));
    args.extend(["--renderer".to_owned(), "gl".to_owned()]);
    let session = Session::start_with("gl", (640, 480), 2, &args);
    // The first GL buffer after switching to scale 2 may have the old size
    // (GlSurface::present): wait for the requested one.
    let window = wait_for("the window at 200 × 150", || {
        Some(session.window()).filter(|w| (w.width, w.height) == (200, 150))
    });
    let (x, y) = (window.x as u32 * 2, window.y as u32 * 2);
    // Same pixels as the software renderer: buffer scale and EGL window size.
    session.expect_pixel(x + 31, y + 31, HI_DPI);
    session.expect_pixel(x + 33, y + 33, IDLE);
    session
        .client()
        .call(
            INPUT_CLICK,
            json!({ "x": window.x + 100, "y": window.y + 75 }),
        )
        .unwrap();
    session.expect_pixel(x + 200, y + 280, TAPPED);
}

#[test]
fn impeller_renderer_draws_and_reacts_to_clicks() {
    // With a GPU it is Impeller; on llvmpipe (CI) the embedder falls back to Skia, because there
    // Impeller crashes inside the engine (gate 1 measurements, docs/gate1-dock.md).
    let mut args = size_args((200, 150));
    args.extend(["--renderer".to_owned(), "impeller".to_owned()]);
    let session = Session::start_with("impeller", (640, 480), 2, &args);
    let window = wait_for("the window at 200 × 150", || {
        Some(session.window()).filter(|w| (w.width, w.height) == (200, 150))
    });
    let (x, y) = (window.x as u32 * 2, window.y as u32 * 2);
    session.expect_pixel(x + 31, y + 31, HI_DPI);
    session.expect_pixel(x + 33, y + 33, IDLE);
    session
        .client()
        .call(
            INPUT_CLICK,
            json!({ "x": window.x + 100, "y": window.y + 75 }),
        )
        .unwrap();
    session.expect_pixel(x + 200, y + 280, TAPPED);
}

#[test]
fn gl_layer_at_scale_two_gets_its_size_and_clicks() {
    // The M1.5 case: OpenGL layer at scale 2. The first buffer after a resize may come out
    // at the old size (see GlSurface::present): it depends on timing, it was seen on
    // WSL with d3d12 and on GitHub with llvmpipe (gl_renderer_draws_at_scale_two_...).
    let args: Vec<String> = [
        "--renderer",
        "gl",
        "--layer",
        "bottom",
        "--anchor",
        "left,right,bottom",
        "--size",
        "320x100",
        "--namespace",
        "gl-layer",
    ]
    .map(str::to_owned)
    .to_vec();
    let session = Session::start_with("gl-layer", (640, 480), 2, &args);
    let layer = wait_for("the layer at the requested size", || {
        Some(session.layer("gl-layer")).filter(|l| (l.width, l.height) == (320, 100))
    });
    let (x, y) = (layer.x as u32 * 2, layer.y as u32 * 2);
    session.expect_pixel(x + 100, y + 150, IDLE);
    session
        .client()
        .call(
            INPUT_CLICK,
            json!({ "x": layer.x + 160, "y": layer.y + 50 }),
        )
        .unwrap();
    session.expect_pixel(x + 100, y + 150, TAPPED);
}

#[test]
fn layer_bar_reserves_space_takes_keyboard_and_clicks() {
    const BAR: u32 = 48;
    let bar_args: Vec<String> = [
        "--layer",
        "top",
        "--anchor",
        "top,left,right",
        "--size",
        "640x48",
        "--exclusive-zone",
        "48",
        "--keyboard",
        "exclusive",
        "--namespace",
        "bar",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let mut session = Session::start_with("layer", (640, 480), 1, &bar_args);

    // Anchored at the top across the whole width, with the keyboard.
    let bar = session.layer("bar");
    assert_eq!(
        (bar.x, bar.y, bar.width, bar.height),
        (0, 0, 640, BAR as i32)
    );
    assert_eq!(
        (bar.layer.as_str(), bar.exclusive_zone),
        ("top", BAR as i32)
    );
    assert!(bar.focused);
    session.expect_pixel(600, 40, IDLE);

    // A normal window opens below the exclusive zone.
    session.spawn(&size_args((200, 100)));
    let window = session.window();
    assert!(
        window.y >= BAR as i32,
        "window at {} above the bar",
        window.y
    );
    let (wx, wy) = (window.x as u32, window.y as u32);
    session.expect_pixel(wx + 100, wy + 90, IDLE);

    // The keyboard goes to the bar, not to the window.
    let mut client = session.client();
    client.call(INPUT_TYPE, json!({"text": "word"})).unwrap();
    session.expect_pixel(600, 40, TYPED);
    session.expect_pixel(wx + 100, wy + 90, IDLE);

    // A click on a layer reaches the layer.
    client.call(INPUT_KEY, json!({"key": "Escape"})).unwrap();
    session.expect_pixel(600, 40, IDLE);
    client
        .call(INPUT_CLICK, json!({"x": 600, "y": 40}))
        .unwrap();
    session.expect_pixel(600, 40, TAPPED);
    session.expect_pixel(wx + 100, wy + 90, IDLE);
}

#[test]
fn glass_widget_declares_its_region_to_ossido() {
    let session = Session::start("glass", (640, 480), 1, (300, 200));
    let window = session.window();
    // The demo puts a 40 × 20 Glass at the top right (testapps/demo, glassSize).
    let list = wait_for("the demo's glass regions", || {
        let list = session
            .client()
            .call(silica_control::commands::GLASS_LIST, Value::Null)
            .ok()?;
        list.as_array().filter(|list| !list.is_empty()).cloned()
    });
    let region = &list[0]["regions"][0];
    assert_eq!(list[0]["surface"], format!("window {}", window.id));
    assert_eq!(
        (region["x"].as_i64(), region["y"].as_i64()),
        (Some(260), Some(0))
    );
    assert_eq!(
        (region["width"].as_i64(), region["height"].as_i64()),
        (Some(40), Some(20))
    );
    assert_eq!(region["radius"].as_f64(), Some(6.0));
    assert_eq!(region["material"], "clear");
}

#[test]
fn the_window_title_comes_from_the_command_line() {
    let mut args = size_args((300, 200));
    args.extend(["--title".to_owned(), "Title test".to_owned()]);
    let session = Session::start_with("title", (640, 480), 1, &args);
    let window = session.window();
    assert_eq!(window.title.as_deref(), Some("Title test"));
}

const SECOND: [u8; 4] = [0x88, 0x33, 0xAA, 0xFF];

/// The app's windows in the session, top first.
fn app_windows(session: &Session) -> Vec<WindowInfo> {
    let list = session.client().call(WINDOW_LIST, Value::Null).unwrap();
    let windows: Vec<WindowInfo> = serde_json::from_value(list).unwrap();
    windows
        .into_iter()
        .filter(|w| w.app_id.as_deref() == Some(APP_ID) && w.width > 0)
        .collect()
}

/// The demo's second window (`F2`, testapps/demo), once it is listed at its size.
fn second_window(session: &Session) -> WindowInfo {
    wait_for("the second window", || {
        app_windows(session)
            .into_iter()
            .find(|w| w.title.as_deref() == Some("Second") && (w.width, w.height) == (200, 120))
    })
}

/// The point of `main` (logical) farthest from `other` and from its shadow, away from the
/// edges and the rounded corners.
fn uncovered(main: &WindowInfo, other: &WindowInfo) -> (i32, i32) {
    const EDGE: i32 = 12;
    const CORNER: i32 = 34;
    let distance = |(x, y): (i32, i32)| {
        let dx = (other.x - x).max(x - (other.x + other.width)).max(0);
        let dy = (other.y - y).max(y - (other.y + other.height)).max(0);
        dx.max(dy)
    };
    let corner = |(x, y): (i32, i32)| {
        let near = |v: i32, start: i32, len: i32| v - start < CORNER || start + len - v < CORNER;
        near(x, main.x, main.width) && near(y, main.y, main.height)
    };
    let points = (main.x + EDGE..main.x + main.width - EDGE)
        .step_by(4)
        .flat_map(|x| {
            (main.y + EDGE..main.y + main.height - EDGE)
                .step_by(4)
                .map(move |y| (x, y))
        });
    let best = points
        .filter(|point| !corner(*point))
        .max_by_key(|point| distance(*point))
        .expect("a window without points");
    assert!(
        distance(best) >= 30,
        "the second window covers the main one"
    );
    best
}

/// Several windows in one process (#146): F2 opens a second view in its own xdg_toplevel,
/// both draw, a tap closes it from the app, its close button closes it through the app.
/// Launching a running app again, with its windows on screen, brings its main window to the
/// front and gives it the focus (the second launch hands over and exits).
#[test]
fn a_second_launch_brings_the_main_window_forward() {
    let mut args = size_args((300, 200));
    args.push("--multi-window".to_owned());
    let session = Session::start_with("relaunch-raises", (800, 600), 1, &args);
    let main = wait_for("the main window at 300 x 200", || {
        Some(session.window()).filter(|w| (w.width, w.height) == (300, 200))
    });
    let mut client = session.client();
    client.call(INPUT_KEY, json!({"key": "F2"})).unwrap();
    let second = second_window(&session);
    let focused = |id: u64| {
        app_windows(&session)
            .iter()
            .any(|w| w.id == id && w.focused)
    };
    assert!(
        wait_until(|| focused(second.id)),
        "the second window has the focus"
    );
    let mut again = spawn_embedder(&session.dir, &session.wayland, &args);
    assert!(
        wait_until(|| again.try_wait().unwrap().is_some()),
        "the second launch did not hand over"
    );
    assert!(
        wait_until(|| focused(main.id)),
        "the second launch did not bring the main window forward"
    );
}

fn second_window_opens_draws_and_closes(name: &str, renderer: &[&str]) {
    let mut args = size_args((300, 200));
    args.push("--multi-window".to_owned());
    args.extend(renderer.iter().map(|s| (*s).to_owned()));
    let mut session = Session::start_with(name, (800, 600), 1, &args);
    let main = wait_for("the main window at 300 × 200", || {
        Some(session.window()).filter(|w| (w.width, w.height) == (300, 200))
    });
    session.expect_pixel(main.x as u32 + 150, main.y as u32 + 100, IDLE);

    // F2 in the main window: a second window of the same app, 200 × 120, all its color.
    let mut client = session.client();
    client.call(INPUT_KEY, json!({"key": "F2"})).unwrap();
    let second = second_window(&session);
    assert_ne!(second.id, main.id);
    session.expect_pixel(second.x as u32 + 100, second.y as u32 + 60, SECOND);
    // The main window still draws its own view.
    let (x, y) = uncovered(&main, &second);
    session.expect_pixel_near(x as u32, y as u32, IDLE);

    // The keyboard goes to the window that has it: the second window's field, not the main
    // window's (the engine is told which view has the focus).
    client.call(INPUT_TYPE, json!({"text": "word"})).unwrap();
    session.expect_pixel(second.x as u32 + 100, second.y as u32 + 30, TYPED);
    session.expect_pixel_near(x as u32, y as u32, IDLE);

    // A child window (set_parent): it opened over its parent, offset, and a click on the
    // parent does not bury it.
    assert_eq!((second.x, second.y), (main.x + 40, main.y + 40));
    client.call(INPUT_CLICK, json!({"x": x, "y": y})).unwrap();
    session.expect_pixel_near(x as u32, y as u32, TAPPED);
    let order: Vec<u64> = app_windows(&session).iter().map(|w| w.id).collect();
    assert_eq!(order, [second.id, main.id], "the child window went behind");

    // A tap in the second window: the app closes it (closeWindow); the process stays.
    client
        .call(
            INPUT_CLICK,
            json!({"x": second.x + 100, "y": second.y + 60}),
        )
        .unwrap();
    assert!(
        wait_until(|| app_windows(&session).len() == 1),
        "the second window is still listed"
    );
    let embedder = session.embedder.as_mut().unwrap();
    assert!(embedder.try_wait().unwrap().is_none(), "the app ended");
    // The keyboard is back in the parent window, with its field focused again.
    client.call(INPUT_TYPE, json!({"text": "word"})).unwrap();
    session.expect_pixel_near(x as u32, y as u32, TYPED);
    client.call(INPUT_KEY, json!({"key": "Escape"})).unwrap();
    session.expect_pixel_near(x as u32, y as u32, IDLE);

    // Again, closed by its close button: Ossido asks, the app answers closeRequested.
    client.call(INPUT_KEY, json!({"key": "F2"})).unwrap();
    let second = second_window(&session);
    session.expect_pixel(second.x as u32 + 100, second.y as u32 + 60, SECOND);
    client.call(WINDOW_CLOSE, json!({"id": second.id})).unwrap();
    assert!(
        wait_until(|| app_windows(&session).len() == 1),
        "the closed second window is still listed"
    );
    assert_eq!(session.window().id, main.id);
    let embedder = session.embedder.as_mut().unwrap();
    assert!(embedder.try_wait().unwrap().is_none(), "the app ended");
}

#[test]
fn a_second_window_opens_draws_and_closes_in_software() {
    second_window_opens_draws_and_closes("multi-sw", &[]);
}

#[test]
fn a_second_window_opens_draws_and_closes_with_gl() {
    second_window_opens_draws_and_closes("multi-gl", &["--renderer", "gl"]);
}

/// Every window in the session, top first.
fn all_windows(session: &Session) -> Vec<WindowInfo> {
    let list = session.client().call(WINDOW_LIST, Value::Null).unwrap();
    serde_json::from_value(list).unwrap()
}

/// The window titled `title`, once listed at its size.
fn titled(session: &Session, title: &str) -> Option<WindowInfo> {
    all_windows(session)
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title) && w.width > 0)
}

/// The portal's dialog (xdg-foreign v2): an app exports its window (`exportHandle`, F3 in the
/// demo) and another app's helper process opens with `--parent`: centered over it, kept
/// above it, and when it closes the focus goes back to it, not to the dialog app's other
/// window.
#[test]
fn a_helper_window_with_a_parent_handle_is_attached_to_it() {
    const OTHER: &str = "dev.frascella.silica.Other";
    let mut session = Session::start("foreign", (800, 600), 1, (300, 200));
    let main = session.window();
    let with = |size, extra: &[&str]| {
        let mut args = size_args(size);
        args.extend(extra.iter().map(|s| (*s).to_owned()));
        args
    };
    // Another window of the dialog's app (the file manager's own window).
    session.spawn(&with((200, 150), &["--app-id", OTHER, "--title", "Other"]));
    let other = wait_for("the other app's window", || titled(&session, "Other"));

    let mut client = session.client();
    client.call(WINDOW_FOCUS, json!({"id": main.id})).unwrap();
    client.call(INPUT_KEY, json!({"key": "F3"})).unwrap();
    let identifier = wait_for("the exported handle", || {
        session
            .window()
            .title
            .filter(|title| title.starts_with("wayland:"))
    });
    let handle = identifier.trim_start_matches("wayland:").to_owned();

    session.spawn(&with(
        (160, 100),
        &[
            "--app-id", OTHER, "--title", "Dialog", "--helper", "--parent", &handle,
        ],
    ));
    let dialog = wait_for("the dialog", || {
        titled(&session, "Dialog").filter(|w| (w.width, w.height) == (160, 100))
    });
    let center = |w: &WindowInfo| (w.x + w.width / 2, w.y + w.height / 2);
    let (dx, dy) = center(&dialog);
    let (mx, my) = center(&main);
    assert!(
        dx.abs_diff(mx) <= 1 && dy.abs_diff(my) <= 1,
        "the dialog {dialog:?} is not centered over {main:?}"
    );
    let order =
        |session: &Session| -> Vec<u64> { all_windows(session).iter().map(|w| w.id).collect() };
    assert_eq!(order(&session), [dialog.id, main.id, other.id]);

    // Its parent brought forward: the dialog stays above it.
    client.call(WINDOW_FOCUS, json!({"id": main.id})).unwrap();
    assert_eq!(order(&session), [dialog.id, main.id, other.id]);

    // The dialog closes (its process ends with it): the parent has the focus.
    client.call(WINDOW_FOCUS, json!({"id": other.id})).unwrap();
    client.call(WINDOW_FOCUS, json!({"id": dialog.id})).unwrap();
    client.call(WINDOW_CLOSE, json!({"id": dialog.id})).unwrap();
    assert!(
        wait_until(|| titled(&session, "Dialog").is_none()),
        "the dialog is still listed"
    );
    let focused = all_windows(&session).into_iter().find(|w| w.focused);
    assert_eq!(focused.map(|w| w.id), Some(main.id));
}

/// F4 until the popup is drawn at `at`: the app ignores F4 while it still holds the view of the
/// popup that just closed (it learns it with `windowClosed`, a moment after the pixel goes).
fn open_popup(session: &Session, client: &mut Client, at: (u32, u32)) {
    let found = wait_until(|| {
        client.call(INPUT_KEY, json!({"key": "F4"})).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        session.pixel(at.0, at.1) == POPUP
    });
    assert!(found, "F4 did not open the popup");
}

/// A menu as its own surface (#4): F4 opens an `xdg_popup` in a second view, hanging from the
/// main window's right edge, so it is drawn past the window; a tap in it closes it from the
/// app, and a press outside it makes Ossido dismiss it (`popup_done` → the view goes).
fn popup_extends_past_its_window(name: &str, renderer: &[&str]) {
    let mut args = size_args((300, 200));
    args.push("--multi-window".to_owned());
    args.extend(renderer.iter().map(|s| (*s).to_owned()));
    let session = Session::start_with(name, (800, 600), 1, &args);
    let main = wait_for("the main window at 300 × 200", || {
        Some(session.window()).filter(|w| (w.width, w.height) == (300, 200))
    });
    let mut client = session.client();
    // The popup hangs from (width - 20, height / 2) of the window: 140 × 80 past the right
    // edge of the window, 100 below its middle; the window ends at main.x + 300.
    let past = (main.x as u32 + 300 + 40, main.y as u32 + 100 + 40);
    open_popup(&session, &mut client, past);
    // Inside the window too (the popup starts 20 px before its edge).
    session.expect_pixel(main.x as u32 + 290, main.y as u32 + 100 + 40, POPUP);

    // A tap in the popup: the app closes it (closeWindow); the window and the process stay.
    client
        .call(INPUT_CLICK, json!({"x": past.0, "y": past.1}))
        .unwrap();
    assert!(
        wait_until(|| session.pixel(past.0, past.1) != POPUP),
        "the popup was not closed by the app"
    );

    // Again, and a press outside it (on the desktop, away from the window) dismisses it.
    open_popup(&session, &mut client, past);
    client
        .call(INPUT_CLICK, json!({"x": 700, "y": 550}))
        .unwrap();
    assert!(
        wait_until(|| session.pixel(past.0, past.1) != POPUP),
        "the popup stayed after a press outside it"
    );
    // The popup can open again: the app learned that it was gone.
    open_popup(&session, &mut client, past);
}

#[test]
fn a_popup_extends_past_its_window_in_software() {
    popup_extends_past_its_window("popup-sw", &[]);
}

#[test]
fn a_popup_extends_past_its_window_with_gl() {
    popup_extends_past_its_window("popup-gl", &["--renderer", "gl"]);
}

/// A menu of a shell layer as its own surface (silica-desktop#222): the demo as a layer
/// (with a second `--view` layer, like the shell) opens an `xdg_popup` adopted by the layer
/// (`zwlr_layer_surface_v1.get_popup`), drawn past the layer's edge; a tap in it closes it from
/// the app, and a press outside it makes Ossido dismiss it.
#[test]
fn a_popup_of_a_shell_layer_extends_past_its_layer() {
    let args: Vec<String> = [
        "--layer",
        "top",
        "--anchor",
        "top,left",
        "--size",
        "300x200",
        "--keyboard",
        "exclusive",
        "--namespace",
        "popup-layer",
        "--view",
        "--layer",
        "bottom",
        "--anchor",
        "bottom,left",
        "--size",
        "50x20",
        "--namespace",
        "popup-layer-2",
    ]
    .map(str::to_owned)
    .to_vec();
    let session = Session::start_with("popup-layer", (800, 600), 1, &args);
    let layer = session.layer("popup-layer");
    let mut client = session.client();
    // Hangs from (width - 20, height / 2): past the layer's right edge.
    let past = (layer.x as u32 + 300 + 40, layer.y as u32 + 100 + 40);
    open_popup(&session, &mut client, past);
    session.expect_pixel(layer.x as u32 + 290, layer.y as u32 + 100 + 40, POPUP);
    client
        .call(INPUT_CLICK, json!({"x": past.0, "y": past.1}))
        .unwrap();
    assert!(
        wait_until(|| session.pixel(past.0, past.1) != POPUP),
        "the popup was not closed by the app"
    );
    open_popup(&session, &mut client, past);
    client
        .call(INPUT_CLICK, json!({"x": 700, "y": 550}))
        .unwrap();
    assert!(
        wait_until(|| session.pixel(past.0, past.1) != POPUP),
        "the popup stayed after a press outside it"
    );
}
