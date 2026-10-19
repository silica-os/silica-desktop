//! End to end: a signed bundle opened by `silica-launch` with the real Trust1 service (on a
//! private bus) and the real bubblewrap and xdg-dbus-proxy. Inside, a probe script reports what
//! it sees: `/.flatpak-info`, the network, the home folder, the bus. Skipped where bwrap or
//! xdg-dbus-proxy are missing or user namespaces are not allowed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use silica_bundle::minisign::SecretKey;
use silica_bundle::trust::sign_bundle;
use silica_daemon::trust::{TRUST_PATH, Trust, TrustConfig};

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
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

const PROBE: &str = r#"#!/bin/sh
out="$XDG_DATA_HOME/result"
{
  [ -f /.flatpak-info ] && grep '^name=' /.flatpak-info
  echo "net=$(tail -n +3 /proc/net/dev | wc -l)"
  [ -e "$HOME/secret.txt" ] && echo "home=visible" || echo "home=hidden"
  [ -e /app/Info.toml ] && echo "app=mounted"
  dbus-send --session --print-reply --dest=org.freedesktop.DBus / org.freedesktop.DBus.ListNames 2>&1 | grep -c org.example.Secret | sed 's/^/secret_names=/'
  touch /app/x 2>/dev/null && echo "app=writable" || echo "app=readonly"
} > "$out"
"#;

fn bundle(dir: &Path, name: &str, key: &SecretKey, network: bool) -> PathBuf {
    let path = dir.join(format!("{name}.app"));
    let c = path.join("Contents");
    std::fs::create_dir_all(c.join("bin")).unwrap();
    std::fs::write(
        c.join("Info.toml"),
        format!(
            "format = 1\nid = \"dev.example.{name}\"\nname = \"{name}\"\nversion = \"1\"\nbuild = 1\n\
             kind = \"native\"\nruntime = \"dev.silica.Platform//1\"\nexec = \"bin/probe\"\n\
             [permissions]\nnetwork = {network}\n\
             [developer]\nname = \"Example\"\nkey = \"{}\"\n",
            key.public().to_base64()
        ),
    )
    .unwrap();
    std::fs::write(c.join("bin/probe"), PROBE).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(c.join("bin/probe"), std::fs::Permissions::from_mode(0o755)).unwrap();
    sign_bundle(&path, key, None).unwrap();
    path
}

#[test]
fn a_bundle_runs_in_its_sandbox() {
    if !available("bwrap") || !available("xdg-dbus-proxy") {
        eprintln!("bwrap or xdg-dbus-proxy missing: skipped");
        return;
    }
    let ok = Command::new("bwrap")
        .args([
            "--unshare-user-try",
            "--unshare-net",
            "--ro-bind",
            "/",
            "/",
            "true",
        ])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!("bwrap cannot make namespaces here: skipped");
        return;
    }
    // Short paths: the proxy's socket lives in the runtime folder (108 bytes at most).
    let root = PathBuf::from(format!("/tmp/sl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let run = root.join("run");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&run).unwrap();
    std::fs::write(home.join("secret.txt"), "private").unwrap();
    // The developer key is a CA key here: the bundles are verified and not in quarantine.
    let key = SecretKey::from_seed([7; 32], [7; 8]);
    let ca = root.join("ca");
    std::fs::create_dir_all(&ca).unwrap();
    std::fs::write(ca.join("test.pub"), key.public().to_file()).unwrap();
    let apps = home.join("Applications");
    let offline = bundle(&apps, "Offline", &key, false);
    let online = bundle(&apps, "Online", &key, true);

    let bus = Bus::start();
    let config = TrustConfig {
        state_dir: root.join("state"),
        settings_dir: None,
        ca_dir: ca,
        revocation_lists: vec![],
        system_bus: None,
        launcher: "true".to_owned(),
        applications_dir: root.join("Applications"),
        volumes_dir: root.join("volumes"),
    };
    let address = bus.address.clone();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        async_io::block_on(async {
            let connection = zbus::connection::Builder::address(address.as_str())
                .unwrap()
                .name("dev.frascella.Silica")
                .unwrap()
                .name("org.example.Secret")
                .unwrap()
                .serve_at(TRUST_PATH, Trust::new(config))
                .unwrap()
                .build()
                .await
                .unwrap();
            while stop_rx.try_recv().is_err() {
                async_io::Timer::after(std::time::Duration::from_millis(20)).await;
            }
            drop(connection);
        });
    });
    std::thread::sleep(std::time::Duration::from_millis(200));
    let launch = |bundle: &Path| {
        Command::new(env!("CARGO_BIN_EXE_silica-launch"))
            .arg(bundle)
            .env_clear()
            .env("HOME", &home)
            .env("XDG_RUNTIME_DIR", &run)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .env("PATH", "/usr/bin:/bin")
            .status()
            .unwrap()
    };
    assert!(launch(&offline).success());
    let result =
        std::fs::read_to_string(home.join(".var/app/dev.example.Offline/data/result")).unwrap();
    assert!(result.contains("name=dev.example.Offline"), "{result}");
    assert!(result.contains("net=1\n"), "only the loopback: {result}");
    assert!(result.contains("home=hidden"), "{result}");
    assert!(result.contains("app=mounted"), "{result}");
    assert!(result.contains("app=readonly"), "{result}");
    // The proxy hides the names the app may not see.
    assert!(result.contains("secret_names=0"), "{result}");
    // With the network permission, the host's interfaces are there.
    assert!(launch(&online).success());
    let result =
        std::fs::read_to_string(home.join(".var/app/dev.example.Online/data/result")).unwrap();
    let net: usize = result
        .lines()
        .find_map(|l| l.strip_prefix("net="))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(net >= 1, "{result}");
    // The instance folders are cleaned up.
    assert_eq!(
        std::fs::read_dir(run.join("silica-launch"))
            .unwrap()
            .count(),
        0
    );
    // A tampered bundle is refused (exit 3) and never runs.
    std::fs::write(
        offline.join("Contents/bin/probe"),
        "#!/bin/sh\necho evil > /tmp/evil\n",
    )
    .unwrap();
    assert_eq!(launch(&offline).code(), Some(3));
    let _ = stop_tx.send(());
    let _ = server.join();
    let _ = std::fs::remove_dir_all(&root);
}
