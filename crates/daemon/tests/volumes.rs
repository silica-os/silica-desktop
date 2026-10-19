//! `dev.frascella.Silica.Volumes1` with real images: built by mkfs.erofs, mounted by erofsfuse.
//! Skipped where the tools or FUSE are not available.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use silica_bundle::minisign::SecretKey;
use silica_bundle::sxv;
use silica_daemon::volumes::{VOLUMES_PATH, Volumes, VolumesConfig};
use zbus::zvariant::{OwnedValue, Value};
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
    interface = "dev.frascella.Silica.Volumes1",
    default_path = "/dev/frascella/Silica/Volumes1"
)]
trait Volumes1 {
    fn inspect(&self, image: &str) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn attach(&self, image: &str, options: HashMap<&str, Value<'_>>) -> zbus::Result<String>;
    fn detach(&self, mountpoint: &str) -> zbus::Result<()>;
    #[allow(clippy::type_complexity)]
    fn list(&self) -> zbus::Result<Vec<(String, String, HashMap<String, OwnedValue>)>>;
}

fn tool(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

fn s(map: &HashMap<String, OwnedValue>, key: &str) -> String {
    String::try_from(map.get(key).cloned().unwrap()).unwrap()
}

fn app(root: &Path, key: &SecretKey) -> PathBuf {
    let app = root.join("Hello.app");
    std::fs::create_dir_all(app.join("Contents/bin")).unwrap();
    std::fs::write(
        app.join("Contents/Info.toml"),
        format!(
            "format = 1\nid = \"dev.example.Hello\"\nname = \"Hello\"\nversion = \"1\"\nbuild = 1\n\
             kind = \"native\"\nruntime = \"none\"\nexec = \"bin/hello\"\n\
             [developer]\nname = \"Example\"\nkey = \"{}\"\n",
            key.public().to_base64()
        ),
    )
    .unwrap();
    std::fs::write(app.join("Contents/bin/hello"), "#!/bin/sh\n").unwrap();
    app
}

#[test]
fn attach_inspect_detach() {
    let (Some(mkfs), Some(fuse)) = (tool("mkfs.erofs"), tool("erofsfuse")) else {
        eprintln!("mkfs.erofs or erofsfuse not installed: skipped");
        return;
    };
    // Under target/ (a real disk): some systems refuse FUSE mounts on /tmp or /dev/shm.
    let root =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("volumes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let key = SecretKey::from_seed([3; 32], [3; 8]);
    let bundle = app(&root, &key);
    std::fs::write(root.join("LICENSE"), "Be nice to the app.").unwrap();
    let image = root.join("Hello.sxv");
    let options = sxv::BuildOptions {
        license: Some(root.join("LICENSE")),
        mkfs: Some(mkfs.clone()),
        ..sxv::BuildOptions::default()
    };
    sxv::build(&bundle, &options, Some(&key), None, &image).unwrap();
    // FUSE itself may be unavailable (containers): probe once.
    let probe = root.join("probe");
    std::fs::create_dir_all(&probe).unwrap();
    let header = sxv::inspect(&image).unwrap().header;
    if sxv::mount(&image, &header, &probe, Some(&fuse)).is_err() {
        eprintln!("FUSE mounts are not allowed here: skipped");
        return;
    }
    sxv::unmount(&probe).unwrap();

    let config = VolumesConfig {
        dir: root.join("run/silica-volumes"),
        settings_dir: Some(root.join("settings")),
        ca_dir: root.join("ca"),
        revocation_lists: vec![],
        erofsfuse: Some(fuse),
    };
    let bus = Bus::start();
    async_io::block_on(async {
        let server = bus.connect().await;
        server
            .object_server()
            .at(VOLUMES_PATH, Volumes::new(config))
            .await
            .unwrap();
        let name = server.unique_name().unwrap().to_string();
        let client = bus.connect().await;
        let volumes = Volumes1Proxy::builder(&client)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();
        let path = image.to_string_lossy().into_owned();
        let info = volumes.inspect(&path).await.unwrap();
        assert_eq!(s(&info, "name"), "Hello");
        assert_eq!(s(&info, "signature"), "valid");
        assert_eq!(s(&info, "developer_status"), "self-signed");
        assert_eq!(s(&info, "license"), "Be nice to the app.");
        // The license must be accepted.
        let err = volumes.attach(&path, HashMap::new()).await.unwrap_err();
        assert!(err.to_string().contains("LicenseRequired"), "{err}");
        let mut accept = HashMap::new();
        accept.insert("accept_license", Value::from(true));
        let mount = volumes.attach(&path, accept.clone()).await.unwrap();
        assert!(mount.ends_with("silica-volumes/Hello"));
        let mounted = Path::new(&mount);
        assert!(mounted.join("Hello.app/Contents/Info.toml").is_file());
        assert_eq!(
            std::fs::read_link(mounted.join("Applications")).unwrap(),
            Path::new("/Applications")
        );
        // Read-only.
        assert!(std::fs::write(mounted.join("x"), "x").is_err());
        // Attached twice: the same mount.
        assert_eq!(volumes.attach(&path, accept.clone()).await.unwrap(), mount);
        let list = volumes.list().await.unwrap();
        assert_eq!(list.len(), 1);
        let window: HashMap<String, OwnedValue> =
            HashMap::try_from(list[0].2.get("window").cloned().unwrap()).unwrap();
        assert_eq!(
            f64::try_from(window.get("width").cloned().unwrap()).unwrap(),
            540.0
        );
        volumes.detach(&mount).await.unwrap();
        assert!(!sxv::is_mounted(mounted));
        assert!(volumes.list().await.unwrap().is_empty());

        // Altered: refused.
        let mut bytes = std::fs::read(&image).unwrap();
        let at = sxv::HEADER_LEN as usize + 600;
        bytes[at] ^= 0x01;
        let bad = root.join("Bad.sxv");
        std::fs::write(&bad, bytes).unwrap();
        let err = volumes
            .attach(&bad.to_string_lossy(), accept.clone())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("BadSignature"), "{err}");
        // Unsigned: refused under the default policy.
        let plain = root.join("Plain.sxv");
        let options = sxv::BuildOptions {
            mkfs: Some(mkfs),
            ..sxv::BuildOptions::default()
        };
        sxv::build(&bundle, &options, None, None, &plain).unwrap();
        let err = volumes
            .attach(&plain.to_string_lossy(), HashMap::new())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unsigned"), "{err}");
    });
    let _ = std::fs::remove_dir_all(&root);
}
