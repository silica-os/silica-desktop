//! The Settings backend: `color-scheme` from the daemon's appearance, `SettingChanged` when it
//! changes, and the `appearance.json` file when there is no daemon.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_lite::StreamExt;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{interface, proxy};

#[proxy(
    interface = "org.freedesktop.impl.portal.Settings",
    default_service = "org.freedesktop.impl.portal.desktop.silica",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait Settings {
    fn read(&self, namespace: &str, key: &str) -> zbus::Result<OwnedValue>;
    fn read_all(
        &self,
        namespaces: &[&str],
    ) -> zbus::Result<HashMap<String, HashMap<String, OwnedValue>>>;
    #[zbus(signal)]
    fn setting_changed(
        &self,
        namespace: String,
        key: String,
        value: OwnedValue,
    ) -> zbus::Result<()>;
}

/// The fake daemon: `State` with `appearance.dark` and the `Changed` signal.
struct FakeSystem {
    dark: bool,
}

#[interface(name = "dev.frascella.Silica.System1")]
impl FakeSystem {
    fn state(&self) -> HashMap<String, OwnedValue> {
        HashMap::from([
            ("appearance.dark".to_owned(), OwnedValue::from(self.dark)),
            (
                "look.accent".to_owned(),
                Value::from("green").try_to_owned().unwrap(),
            ),
        ])
    }

    #[zbus(signal)]
    async fn changed(
        emitter: &SignalEmitter<'_>,
        state: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn private_bus() -> (Killed, String) {
    let mut bus = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("dbus-daemon (package dbus) is required for this test");
    let mut address = String::new();
    BufReader::new(bus.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    (Killed(bus), address.trim().to_owned())
}

fn portal(address: &str, config: &std::path::Path) -> Killed {
    Killed(
        Command::new(env!("CARGO_BIN_EXE_silica-portal"))
            .args(["--address", address])
            .env("XDG_CONFIG_HOME", config)
            .spawn()
            .unwrap(),
    )
}

async fn scheme(settings: &SettingsProxy<'_>) -> Option<u32> {
    for _ in 0..50 {
        if let Ok(value) = settings
            .read("org.freedesktop.appearance", "color-scheme")
            .await
        {
            return u32::try_from(value).ok();
        }
        async_io::Timer::after(Duration::from_millis(100)).await;
    }
    None
}

#[test]
fn color_scheme_follows_the_daemon() {
    let dir = std::env::temp_dir().join(format!("silica-portal-settings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (_bus, address) = private_bus();
    zbus::block_on(async {
        let daemon = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .name("dev.frascella.Silica")
            .unwrap()
            .serve_at("/dev/frascella/Silica/System1", FakeSystem { dark: false })
            .unwrap()
            .build()
            .await
            .unwrap();
        let _portal = portal(&address, &dir);
        let client = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let settings = SettingsProxy::new(&client).await.unwrap();
        // Light from the daemon (2).
        assert_eq!(scheme(&settings).await, Some(2));
        let all = settings.read_all(&["org.freedesktop.*"]).await.unwrap();
        assert!(all["org.freedesktop.appearance"].contains_key("color-scheme"));
        // The accent of the system look, as (ddd) 0-1: Green 62BA46.
        let accent = settings
            .read("org.freedesktop.appearance", "accent-color")
            .await
            .unwrap();
        let (r, g, b) = <(f64, f64, f64)>::try_from(accent).unwrap();
        assert!((r - 98.0 / 255.0).abs() < 1e-9 && (g - 186.0 / 255.0).abs() < 1e-9);
        assert!((b - 70.0 / 255.0).abs() < 1e-9);
        // The daemon switches to dark: SettingChanged and Read give 1.
        let mut changes = settings.receive_setting_changed().await.unwrap();
        let emitter = SignalEmitter::new(&daemon, "/dev/frascella/Silica/System1").unwrap();
        FakeSystem::changed(
            &emitter,
            HashMap::from([("appearance.dark".to_owned(), OwnedValue::from(true))]),
        )
        .await
        .unwrap();
        let signal = futures_lite::future::or(async { changes.next().await }, async {
            async_io::Timer::after(Duration::from_secs(5)).await;
            None
        })
        .await
        .expect("no SettingChanged");
        let args = signal.args().unwrap();
        assert_eq!(args.namespace, "org.freedesktop.appearance");
        assert_eq!(u32::try_from(args.value).unwrap(), 1);
        assert_eq!(scheme(&settings).await, Some(1));
    });
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn without_the_daemon_the_file_decides() {
    let dir = std::env::temp_dir().join(format!("silica-portal-file-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("silica")).unwrap();
    std::fs::write(dir.join("silica/appearance.json"), "{\"dark\": true}\n").unwrap();
    let (_bus, address) = private_bus();
    let _portal = portal(&address, &dir);
    zbus::block_on(async {
        let client = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let settings = SettingsProxy::new(&client).await.unwrap();
        assert_eq!(scheme(&settings).await, Some(1));
    });
    std::fs::remove_dir_all(&dir).unwrap();
}
