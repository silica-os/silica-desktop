//! The settings files in `$XDG_CONFIG_HOME/silica`: Ossido applies them at start and when
//! they change (inotify).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use common::Headless;
use serde_json::{Value, json};
use wayland_client::protocol::wl_keyboard::{self, WlKeyboard};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

/// Writes `name` like Settings does: a temporary file renamed over the real one.
fn write_setting(ossido: &Headless, name: &str, value: &Value) {
    let dir = ossido.dir.join("config/silica");
    std::fs::create_dir_all(&dir).unwrap();
    let temporary = dir.join(format!("{name}.tmp"));
    std::fs::write(&temporary, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    std::fs::rename(&temporary, dir.join(name)).unwrap();
}

/// Calls `command` until `check` accepts the result (5 s at most).
fn wait_for(ossido: &Headless, command: &str, check: impl Fn(&Value) -> bool) -> Value {
    let mut client = ossido.client();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let value = client.call(command, Value::Null).unwrap();
        if check(&value) {
            return value;
        }
        assert!(Instant::now() < deadline, "{command}: still {value}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[derive(Default)]
struct Keyboard {
    repeat: Vec<(i32, i32)>,
    keymaps: usize,
}

impl Dispatch<WlRegistry, ()> for Keyboard {
    fn event(
        _: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
            && interface == "wl_seat"
        {
            let seat: WlSeat = registry.bind(name, 7, qh, ());
            seat.get_keyboard(qh, ());
        }
    }
}

impl Dispatch<WlSeat, ()> for Keyboard {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wayland_client::protocol::wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlKeyboard, ()> for Keyboard {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::RepeatInfo { rate, delay } => state.repeat.push((rate, delay)),
            wl_keyboard::Event::Keymap {
                format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
                ..
            } => state.keymaps += 1,
            _ => {}
        }
    }
}

fn connect(socket: &Path) -> (Connection, wayland_client::EventQueue<Keyboard>, Keyboard) {
    let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
    let mut queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut keyboard = Keyboard::default();
    queue.roundtrip(&mut keyboard).unwrap();
    queue.roundtrip(&mut keyboard).unwrap();
    (connection, queue, keyboard)
}

#[test]
fn keyboard_json_sets_repeat_and_sources_live() {
    let ossido = Headless::start("keyboard-json");
    let (_connection, mut queue, mut keyboard) = connect(&ossido.wayland);
    // The defaults first (no file).
    let state = ossido.client().call("keyboard.state", Value::Null).unwrap();
    assert_eq!(state["repeatDelay"], 375);
    assert_eq!(state["repeatRate"], 11);
    assert_eq!(keyboard.repeat, [(11, 375)]);
    assert_eq!(keyboard.keymaps, 1);
    // With one source the shortcut does not switch.
    let next = ossido
        .client()
        .call("keyboard.next_source", Value::Null)
        .unwrap();
    assert_eq!(next["switched"], false);

    write_setting(
        &ossido,
        "keyboard.json",
        &json!({"repeatDelay": 250, "repeatRate": 30, "layouts": ["us", "it"]}),
    );
    let state = wait_for(&ossido, "keyboard.state", |s| s["repeatRate"] == 30);
    assert_eq!(state["repeatDelay"], 250);
    assert_eq!(state["layouts"], json!(["us", "it"]));
    assert_eq!(state["active"], 0);
    // The bound keyboard got the new repeat and a new keymap.
    queue.roundtrip(&mut keyboard).unwrap();
    assert_eq!(keyboard.repeat.last(), Some(&(30, 250)));
    assert_eq!(keyboard.keymaps, 2);

    let next = ossido
        .client()
        .call("keyboard.next_source", Value::Null)
        .unwrap();
    assert_eq!(next, json!({"switched": true, "active": 1}));
    // The input menu chooses a source by its index; past the last one nothing changes.
    let select = |index: u64| {
        ossido
            .client()
            .call("keyboard.select_source", json!({ "index": index }))
            .unwrap()
    };
    assert_eq!(select(0), json!({"switched": true, "active": 0}));
    assert_eq!(select(3), json!({"switched": false, "active": 0}));
    assert_eq!(select(1), json!({"switched": true, "active": 1}));

    // A layout xkb does not know keeps the keymap; the repeat still applies.
    write_setting(
        &ossido,
        "keyboard.json",
        &json!({"repeatDelay": 600, "repeatRate": 0, "layouts": ["us", "nosuchlayout"]}),
    );
    let state = wait_for(&ossido, "keyboard.state", |s| s["repeatDelay"] == 600);
    assert_eq!(state["repeatRate"], 0);
    assert_eq!(state["layouts"], json!(["us", "it"]));

    // Removed: the defaults again.
    std::fs::remove_file(ossido.dir.join("config/silica/keyboard.json")).unwrap();
    let state = wait_for(&ossido, "keyboard.state", |s| s["repeatDelay"] == 375);
    assert_eq!(state["repeatRate"], 11);
    assert_eq!(state["layouts"], json!([]));
}

#[test]
fn displays_are_listed_configured_and_saved() {
    let ossido = Headless::start("displays");
    let mut client = ossido.client();
    let list = client.call("output.list", Value::Null).unwrap();
    let outputs = list.as_array().unwrap();
    assert_eq!(outputs.len(), 1);
    let output = &outputs[0];
    let connector = output["connector"].as_str().unwrap().to_owned();
    assert_eq!(output["enabled"], true);
    assert_eq!(output["scale"], 1.0);
    assert_eq!(output["transform"], "normal");
    assert_eq!(output["current"]["width"], common::WIDTH);

    let configured = client
        .call(
            "output.configure",
            json!({"connector": connector, "scale": 2, "transform": "90"}),
        )
        .unwrap();
    assert_eq!(configured["scale"], 2.0);
    assert_eq!(configured["transform"], "90");
    // Saved for this display, by its identity (the connector without EDID).
    let saved: Value = serde_json::from_slice(
        &std::fs::read(ossido.dir.join("config/silica/displays.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["outputs"][&connector]["scale"], 2.0);
    assert_eq!(saved["outputs"][&connector]["transform"], "90");

    // Bad parameters and modes the display does not have are refused.
    for bad in [
        json!({"scale": 1.1}),
        json!({"transform": "sideways"}),
        json!({"mode": {"width": 7, "height": 7}}),
        json!({"connector": "nope", "scale": 1}),
    ] {
        assert!(
            client.call("output.configure", bad.clone()).is_err(),
            "{bad}"
        );
    }

    // A change made to the file applies at once.
    write_setting(
        &ossido,
        "displays.json",
        &json!({"outputs": {connector.clone(): {"scale": 1.5, "transform": "normal"}}}),
    );
    let list = wait_for(&ossido, "output.list", |l| l[0]["scale"] == 1.5);
    assert_eq!(list[0]["transform"], "normal");

    // Headless has no gamma: the night light is accepted but not applied.
    let night = client
        .call("output.color_temperature", json!({"kelvin": 3400}))
        .unwrap();
    assert_eq!(night, json!({"kelvin": 3400, "applied": false}));
    assert!(
        client
            .call("output.color_temperature", json!({"kelvin": 200}))
            .is_err()
    );
}

#[test]
fn power_json_turns_the_display_off_until_input() {
    let ossido = Headless::start("power-json");
    let state = ossido.client().call("idle.state", Value::Null).unwrap();
    assert_eq!(state["displayOff"], false);
    assert_eq!(state["displayOffMinutes"], Value::Null);

    // 0.02 minutes: 1.2 s.
    write_setting(
        &ossido,
        "power.json",
        &json!({"displayOffMinutes": 0.02, "preventSleep": true}),
    );
    let state = wait_for(&ossido, "idle.state", |s| s["displayOff"] == true);
    assert_eq!(state["preventSleep"], true);
    assert!(state["idleSeconds"].as_u64().unwrap() >= 1);
    // Input wakes it.
    ossido
        .client()
        .call("input.pointer_move", json!({"x": 10.0, "y": 10.0}))
        .unwrap();
    let state = ossido.client().call("idle.state", Value::Null).unwrap();
    assert_eq!(state["displayOff"], false);
    assert_eq!(state["idleSeconds"], 0);
}

#[test]
fn input_json_is_read_live() {
    let ossido = Headless::start("input-json");
    let state = ossido.client().call("input.state", Value::Null).unwrap();
    assert_eq!(state["mouse"]["speed"], Value::Null);
    assert_eq!(state["mouse"]["scrollSpeed"], 1.0);
    // Headless has no libinput devices.
    assert_eq!(state["devices"], json!({"mouse": false, "touchpad": false}));

    write_setting(
        &ossido,
        "input.json",
        &json!({
            "mouse": {"speed": 0.25, "naturalScroll": true, "secondaryClick": "left"},
            "touchpad": {"tapToClick": true, "secondaryClick": "twoFinger", "scrollSpeed": 1.5}
        }),
    );
    let state = wait_for(&ossido, "input.state", |s| s["mouse"]["speed"] == 0.25);
    assert_eq!(state["mouse"]["naturalScroll"], true);
    assert_eq!(state["mouse"]["leftHanded"], true);
    assert_eq!(state["touchpad"]["tapToClick"], true);
    assert_eq!(state["touchpad"]["secondaryClick"], "twoFinger");
    assert_eq!(state["touchpad"]["scrollSpeed"], 1.5);

    // Not an object: the defaults.
    write_setting(&ossido, "input.json", &json!("not an object"));
    wait_for(&ossido, "input.state", |s| s["mouse"]["speed"].is_null());
}

#[test]
fn input_type_follows_the_active_keymap_with_a_temporary_key_fallback() {
    let ossido = Headless::start("keyboard-type");
    let (_connection, mut queue, mut keyboard) = connect(&ossido.wayland);
    write_setting(&ossido, "keyboard.json", &json!({"layouts": ["it"]}));
    wait_for(&ossido, "keyboard.state", |s| s["layouts"] == json!(["it"]));
    queue.roundtrip(&mut keyboard).unwrap();
    // Characters the Italian layout has (the ones that put "ò" for ";" on US keycodes), some
    // with AltGr: no error, and no keymap change.
    let before = keyboard.keymaps;
    ossido
        .client()
        .call("input.type", json!({"text": "clear; ls @ò€"}))
        .unwrap();
    queue.roundtrip(&mut keyboard).unwrap();
    assert_eq!(keyboard.keymaps, before);
    // A character no layout has: a temporary keymap is sent, then the real one again.
    ossido
        .client()
        .call("input.type", json!({"text": "aǆb"}))
        .unwrap();
    queue.roundtrip(&mut keyboard).unwrap();
    assert_eq!(keyboard.keymaps, before + 2);
    // A control character other than the keys cannot be typed, and nothing is pressed.
    assert!(
        ossido
            .client()
            .call("input.type", json!({"text": "a\u{1}"}))
            .is_err()
    );
    assert_eq!(
        ossido.client().call("keyboard.state", Value::Null).unwrap()["layouts"],
        json!(["it"])
    );
}

#[test]
fn hot_corners_run_their_action_when_the_pointer_arrives() {
    let ossido = Headless::start("hot-corners");
    let mut client = ossido.client();
    let pointer = |client: &mut silica_control::Client, x: f64, y: f64| {
        client
            .call("input.pointer_move", json!({"x": x, "y": y}))
            .unwrap();
    };
    let open = |client: &mut silica_control::Client| {
        client.call("panorama.state", Value::Null).unwrap()["open"] == true
    };
    // Without a file, the corner does nothing.
    pointer(&mut client, 160.0, 120.0);
    pointer(&mut client, 0.0, 0.0);
    assert!(!open(&mut client));
    write_setting(
        &ossido,
        "hotcorners.json",
        &json!({"topLeft": "panorama", "bottomRight": "nonsense"}),
    );
    // The file is read as it lands: arrive again until the corner is armed with its action.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        pointer(&mut client, 160.0, 120.0);
        pointer(&mut client, 0.0, 0.0);
        if open(&mut client) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the corner did not run its action"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
