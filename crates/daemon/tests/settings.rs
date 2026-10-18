//! `dev.frascella.Silica.Settings1` on a private bus: types, errors, `Changed`, persistence.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use futures_lite::StreamExt;
use silica_daemon::settings::{SETTINGS_PATH, Settings, SettingsStore};
use zbus::zvariant::OwnedValue;
use zbus::{Connection, proxy};

struct Bus {
    child: Child,
    address: String,
}

impl Bus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon (package dbus) is required for these tests");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        Self {
            child,
            address: line.trim().to_owned(),
        }
    }

    async fn connect(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[proxy(
    interface = "dev.frascella.Silica.Settings1",
    default_path = "/dev/frascella/Silica/Settings1"
)]
trait Settings1 {
    fn get(&self, key: &str) -> zbus::Result<OwnedValue>;
    fn get_all(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn set(&self, key: &str, value: &zbus::zvariant::Value<'_>) -> zbus::Result<()>;
    fn set_many(&self, values: HashMap<&str, zbus::zvariant::Value<'_>>) -> zbus::Result<()>;
    fn reset(&self, key: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    fn changed(&self, key: &str, value: zbus::zvariant::Value<'_>) -> zbus::Result<()>;
}

#[test]
fn typed_values_errors_signals_and_persistence() {
    use zbus::zvariant::Value;
    let dir = std::env::temp_dir().join(format!("silica-settings-bus-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bus = Bus::start();
    zbus::block_on(async {
        let server = bus.connect().await;
        server
            .object_server()
            .at(SETTINGS_PATH, Settings::new(SettingsStore::open(&dir)))
            .await
            .unwrap();
        let client = bus.connect().await;
        let proxy = Settings1Proxy::builder(&client)
            .destination(server.unique_name().unwrap().to_owned())
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut changed = proxy.receive_changed().await.unwrap();

        let v = proxy.get("dock.tileSize").await.unwrap();
        assert_eq!(*v, Value::I64(60));
        // An int32 from a client is fine for an integer key.
        proxy.set("dock.tileSize", &Value::I32(100)).await.unwrap();
        let signal = changed.next().await.unwrap();
        let args = signal.args().unwrap();
        assert_eq!(*args.key(), "dock.tileSize");
        assert_eq!(*args.value(), Value::I64(100));

        // Wrong type, out of range, unknown key: InvalidArgs, and nothing changes.
        for (key, value) in [
            ("dock.tileSize", Value::I64(1000)),
            ("dock.tileSize", Value::from("big")),
            ("dock.position", Value::from("top")),
            ("nope", Value::Bool(true)),
        ] {
            let err = proxy.set(key, &value).await.unwrap_err();
            assert!(err.to_string().contains("InvalidArgs"), "{key}: {err}");
        }
        assert_eq!(*proxy.get("dock.tileSize").await.unwrap(), Value::I64(100));

        // Lists, all or none.
        proxy
            .set_many(HashMap::from([
                ("wallpaper.color", Value::from("#102030")),
                (
                    "controlcenter.modules",
                    Value::from(vec!["wifi:large".to_owned()]),
                ),
            ]))
            .await
            .unwrap();
        // One Changed per key.
        let mut keys = Vec::new();
        for _ in 0..2 {
            let signal = changed.next().await.unwrap();
            keys.push(signal.args().unwrap().key().to_string());
        }
        keys.sort();
        assert_eq!(keys, ["controlcenter.modules", "wallpaper.color"]);
        let all = proxy.get_all().await.unwrap();
        assert!(all.contains_key("dock.autohide"));
        assert!(
            proxy
                .set_many(HashMap::from([
                    ("dock.autohide", Value::Bool(true)),
                    ("dock.position", Value::from("top")),
                ]))
                .await
                .is_err()
        );
        assert_eq!(
            *proxy.get("dock.autohide").await.unwrap(),
            Value::Bool(false)
        );

        proxy.reset("dock.tileSize").await.unwrap();
        let mut seen = None;
        while let Some(signal) = changed.next().await {
            let args = signal.args().unwrap();
            if *args.key() == "dock.tileSize" {
                seen = Some(i64::try_from(args.value().try_clone().unwrap()).unwrap());
                break;
            }
        }
        assert_eq!(seen, Some(60));
    });
    drop(bus);

    // A new service on the same folder has the values that were set.
    let bus = Bus::start();
    zbus::block_on(async {
        let server = bus.connect().await;
        server
            .object_server()
            .at(SETTINGS_PATH, Settings::new(SettingsStore::open(&dir)))
            .await
            .unwrap();
        let client = bus.connect().await;
        let proxy = Settings1Proxy::builder(&client)
            .destination(server.unique_name().unwrap().to_owned())
            .unwrap()
            .build()
            .await
            .unwrap();
        assert_eq!(
            *proxy.get("wallpaper.color").await.unwrap(),
            Value::from("#102030")
        );
        assert_eq!(*proxy.get("dock.tileSize").await.unwrap(), Value::I64(60));
    });
    let _ = std::fs::remove_dir_all(&dir);
}
