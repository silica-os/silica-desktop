//! `dev.frascella.Silica.Apps1` on a private bus: an app copied into a folder shows up within a
//! second, bundles get compatibility entries, moves keep the id, defaults are written, apps are
//! launched with their files.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use futures_lite::{FutureExt, StreamExt};
use silica_apps::{Dirs, Location};
use silica_daemon::apps::{AppsConfig, AppsService};
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
    interface = "dev.frascella.Silica.Apps1",
    default_path = "/dev/frascella/Silica/Apps1"
)]
trait Apps1 {
    #[allow(clippy::type_complexity)]
    fn list(
        &self,
    ) -> zbus::Result<Vec<(String, String, String, String, HashMap<String, OwnedValue>)>>;
    fn get(&self, id: &str) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn apps_for(&self, r#type: &str) -> zbus::Result<Vec<String>>;
    fn default_for(&self, r#type: &str) -> zbus::Result<String>;
    fn set_default(&self, r#type: &str, id: &str) -> zbus::Result<()>;
    fn open(
        &self,
        id: &str,
        uris: &[&str],
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<u32>;
    fn note_launched(&self, id: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    fn added(&self, id: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn removed(&self, id: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn changed(&self, id: String) -> zbus::Result<()>;
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("apps1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A bundle made in a staging folder and moved into place in one rename, as a copy that
/// finishes does.
fn install_bundle(staging: &Path, into: &Path, name: &str, id: &str) -> PathBuf {
    let built = staging.join(format!("{name}.app"));
    write(
        &built.join("Contents/Info.toml"),
        &format!(
            "format = 1\nid = \"{id}\"\nname = \"{name}\"\nversion = \"1.0\"\nbuild = 1\n\
             kind = \"native\"\nruntime = \"none\"\nexec = \"bin/run\"\n\
             [types]\n\"text/plain\" = \"viewer\"\n\
             [developer]\nname = \"Dev\"\nkey = \"RWQ\"\n"
        ),
    );
    std::fs::create_dir_all(into).unwrap();
    let path = into.join(format!("{name}.app"));
    std::fs::rename(&built, &path).unwrap();
    path
}

/// Waits for the next signal of the stream, at most `limit`.
async fn within<S, T>(stream: &mut S, limit: Duration) -> Option<T>
where
    S: futures_lite::Stream<Item = T> + Unpin,
{
    async { stream.next().await }
        .or(async {
            async_io::Timer::after(limit).await;
            None
        })
        .await
}

fn string(map: &HashMap<String, OwnedValue>, key: &str) -> String {
    String::try_from(map.get(key).cloned().unwrap()).unwrap()
}

#[test]
fn apps_appear_move_and_open() {
    let root = tmp("move");
    let home = root.join("home");
    let dirs = Dirs {
        config_home: home.join(".config"),
        data_home: home.join(".local/share"),
        data_dirs: vec![root.join("usr/share")],
        app_folders: vec![(home.join("Applications"), Location::User)],
        ..Dirs::default()
    };
    std::fs::create_dir_all(&dirs.data_home).unwrap();
    let config = AppsConfig {
        dirs: dirs.clone(),
        db_path: home.join(".cache/silica/apps.bin"),
        mime_dirs: vec![],
        compat: true,
        publish_live: false,
    };
    let bus = Bus::start();
    async_io::block_on(async {
        let server = bus.connect().await;
        let service = AppsService::serve(server.clone(), config).await.unwrap();
        let name = server.unique_name().unwrap().to_string();
        let client = bus.connect().await;
        let apps = Apps1Proxy::builder(&client)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut added = apps.receive_added().await.unwrap();
        let mut removed = apps.receive_removed().await.unwrap();
        let mut changed = apps.receive_changed().await.unwrap();
        let test = async {
            assert!(apps.list().await.unwrap().is_empty());

            // A desktop entry copied in: Added within a second.
            let start = Instant::now();
            write(
                &dirs.data_home.join("applications/org.example.Copy.desktop"),
                &format!(
                    "[Desktop Entry]\nType=Application\nName=Copy\nExec=cp %f {}\nMimeType=text/x-copy;\n",
                    root.join("copied").display()
                ),
            );
            let signal = within(&mut added, Duration::from_secs(1))
                .await
                .expect("Added");
            assert_eq!(signal.args().unwrap().id, "org.example.Copy");
            assert!(start.elapsed() < Duration::from_secs(1));
            let list = apps.list().await.unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].2, "system");

            // ~/Applications does not exist yet: a bundle moved in still shows up.
            let staging = root.join("staging");
            let path = install_bundle(
                &staging,
                &home.join("Applications"),
                "Viewer",
                "dev.example.Viewer",
            );
            let signal = within(&mut added, Duration::from_secs(2))
                .await
                .expect("Added bundle");
            assert_eq!(signal.args().unwrap().id, "dev.example.Viewer");
            let props = apps.get("dev.example.Viewer").await.unwrap();
            assert_eq!(string(&props, "kind"), "bundle");
            assert_eq!(string(&props, "location"), "user");
            assert_eq!(string(&props, "path"), path.to_string_lossy());
            assert!(string(&props, "exec").starts_with("silica-launch "));
            let compat = dirs.compat_dir().join("dev.example.Viewer.desktop");
            assert!(
                std::fs::read_to_string(&compat)
                    .unwrap()
                    .contains("X-Silica-Bundle=")
            );

            // Renamed: the same id, a new path.
            let moved = home.join("Applications/Viewer Renamed.app");
            std::fs::rename(&path, &moved).unwrap();
            let signal = within(&mut changed, Duration::from_secs(2))
                .await
                .expect("Changed");
            assert_eq!(signal.args().unwrap().id, "dev.example.Viewer");
            let props = apps.get("dev.example.Viewer").await.unwrap();
            assert_eq!(string(&props, "path"), moved.to_string_lossy());

            // Defaults: the declaring app, then the one chosen.
            assert_eq!(
                apps.default_for("text/plain").await.unwrap(),
                "dev.example.Viewer"
            );
            write(
                &dirs.data_home.join("applications/org.example.Edit.desktop"),
                "[Desktop Entry]\nType=Application\nName=Edit\nExec=true %U\nMimeType=text/plain;\n",
            );
            within(&mut added, Duration::from_secs(2))
                .await
                .expect("Added Edit");
            // An editor wins over a viewer when nothing is chosen.
            assert_eq!(
                apps.default_for("text/plain").await.unwrap(),
                "org.example.Edit"
            );
            apps.set_default("text/plain", "dev.example.Viewer")
                .await
                .unwrap();
            assert_eq!(
                apps.default_for("text/plain").await.unwrap(),
                "dev.example.Viewer"
            );
            assert_eq!(
                apps.apps_for("text/plain").await.unwrap(),
                ["dev.example.Viewer", "org.example.Edit"]
            );
            let list = std::fs::read_to_string(home.join(".config/mimeapps.list")).unwrap();
            assert!(list.contains("text/plain=silica-bundles-dev.example.Viewer.desktop;"));

            // Open: the file goes through the field code.
            let source = root.join("source.txt");
            std::fs::write(&source, "hello").unwrap();
            let uri = format!("file://{}", source.display());
            let pid = apps
                .open("org.example.Copy", &[uri.as_str()], HashMap::new())
                .await
                .unwrap();
            assert!(pid > 0);
            let deadline = Instant::now() + Duration::from_secs(5);
            while !root.join("copied").exists() && Instant::now() < deadline {
                async_io::Timer::after(Duration::from_millis(20)).await;
            }
            assert_eq!(
                std::fs::read_to_string(root.join("copied")).unwrap(),
                "hello"
            );
            assert!(apps.open("missing.App", &[], HashMap::new()).await.is_err());

            // Trashed: Removed, and the compatibility entry goes.
            std::fs::remove_dir_all(&moved).unwrap();
            let signal = within(&mut removed, Duration::from_secs(2))
                .await
                .expect("Removed");
            assert_eq!(signal.args().unwrap().id, "dev.example.Viewer");
            assert!(!compat.exists());
            assert!(apps.get("dev.example.Viewer").await.is_err());
        };
        test.or(async {
            service.run().await.unwrap();
            panic!("the service stopped");
        })
        .await;
    });
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn new_apps_are_marked_until_launched() {
    let root = tmp("new");
    let home = root.join("home");
    let dirs = Dirs {
        config_home: home.join(".config"),
        data_home: home.join(".local/share"),
        ..Dirs::default()
    };
    write(
        &dirs.data_home.join("applications/org.example.Old.desktop"),
        "[Desktop Entry]\nType=Application\nName=Old\nExec=true\n",
    );
    let config = AppsConfig {
        dirs: dirs.clone(),
        db_path: home.join(".cache/silica/apps.bin"),
        mime_dirs: vec![],
        compat: false,
        publish_live: false,
    };
    let bus = Bus::start();
    async_io::block_on(async {
        let server = bus.connect().await;
        let service = AppsService::serve(server.clone(), config).await.unwrap();
        let name = server.unique_name().unwrap().to_string();
        let client = bus.connect().await;
        let apps = Apps1Proxy::builder(&client)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut added = apps.receive_added().await.unwrap();
        let test = async {
            let new = |props: &HashMap<String, OwnedValue>| {
                bool::try_from(props.get("new").cloned().unwrap()).unwrap()
            };
            // Present when the database was created: not new.
            assert!(!new(&apps.get("org.example.Old").await.unwrap()));
            // A second later (the database's times are in seconds), a new app is.
            async_io::Timer::after(Duration::from_millis(1100)).await;
            write(
                &dirs
                    .data_home
                    .join("applications/org.example.Fresh.desktop"),
                "[Desktop Entry]\nType=Application\nName=Fresh\nExec=true\n",
            );
            within(&mut added, Duration::from_secs(2))
                .await
                .expect("Added");
            assert!(new(&apps.get("org.example.Fresh").await.unwrap()));
            apps.note_launched("org.example.Fresh").await.unwrap();
            assert!(!new(&apps.get("org.example.Fresh").await.unwrap()));
            assert!(home.join(".cache/silica/apps.bin").is_file());
        };
        test.or(async {
            service.run().await.unwrap();
            panic!("the service stopped");
        })
        .await;
    });
    let _ = std::fs::remove_dir_all(&root);
}
