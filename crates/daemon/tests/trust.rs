//! `dev.frascella.Silica.Trust1` on a private bus: verdicts for real signed bundles, the prompt
//! answered by a stand-in shell, approvals, tampering, the blocked list.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use futures_lite::{FutureExt, StreamExt};
use silica_bundle::minisign::SecretKey;
use silica_bundle::quarantine::{self, Quarantine};
use silica_bundle::trust::{certify, sign_bundle};
use silica_daemon::trust::{TRUST_PATH, Trust, TrustConfig};
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
    interface = "dev.frascella.Silica.Trust1",
    default_path = "/dev/frascella/Silica/Trust1"
)]
trait Trust1 {
    fn assess(&self, path: &str) -> zbus::Result<(String, HashMap<String, OwnedValue>)>;
    fn prompt(&self, path: &str) -> zbus::Result<String>;
    fn answer(&self, id: u32, choice: &str) -> zbus::Result<()>;
    fn register_agent(&self) -> zbus::Result<()>;
    fn blocked(&self) -> zbus::Result<Vec<(String, String, u64)>>;
    fn open_anyway(&self, path: &str) -> zbus::Result<bool>;
    #[zbus(signal)]
    fn prompt_requested(
        &self,
        id: u32,
        verdict: String,
        info: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
}

fn bundle(dir: &Path, name: &str, key: &SecretKey) -> PathBuf {
    let path = dir.join(format!("{name}.app"));
    let c = path.join("Contents");
    std::fs::create_dir_all(c.join("bin")).unwrap();
    std::fs::write(
        c.join("Info.toml"),
        format!(
            "format = 1\nid = \"dev.example.{name}\"\nname = \"{name}\"\nversion = \"1\"\nbuild = 1\n\
             kind = \"native\"\nruntime = \"none\"\nexec = \"bin/run\"\n\
             [developer]\nname = \"Example\"\nkey = \"{}\"\n",
            key.public().to_base64()
        ),
    )
    .unwrap();
    std::fs::write(c.join("bin/run"), "#!/bin/sh\n").unwrap();
    path
}

fn s(info: &HashMap<String, OwnedValue>, key: &str) -> String {
    String::try_from(info.get(key).cloned().unwrap()).unwrap()
}

#[test]
fn verdicts_prompts_and_approvals() {
    let root = std::env::temp_dir().join(format!("trust1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let apps = root.join("Applications");
    std::fs::create_dir_all(&apps).unwrap();
    let ca = SecretKey::from_seed([1; 32], [1; 8]);
    let dev = SecretKey::from_seed([2; 32], [2; 8]);
    let ca_dir = root.join("ca");
    std::fs::create_dir_all(&ca_dir).unwrap();
    std::fs::write(ca_dir.join("silica.pub"), ca.public().to_file()).unwrap();

    // A verified bundle in quarantine, a self-signed one, an unsigned one.
    let verified = bundle(&apps, "Verified", &dev);
    sign_bundle(
        &verified,
        &dev,
        Some(&certify(&ca, &dev.public(), "Example")),
    )
    .unwrap();
    let marked = quarantine::write(
        &verified,
        &Quarantine::now(
            quarantine::FROM_VOLUME,
            "dev.frascella.Mustache",
            "file:///Downloads/V.sxv",
        ),
    );
    if marked.is_err() {
        eprintln!("no user xattrs on this file system: skipped");
        return;
    }
    let selfsigned = bundle(&apps, "Mine", &dev);
    sign_bundle(&selfsigned, &dev, None).unwrap();
    let unsigned = bundle(&apps, "Plain", &dev);

    let config = TrustConfig {
        state_dir: root.join("state"),
        settings_dir: Some(root.join("settings")),
        ca_dir,
        revocation_lists: vec![],
        system_bus: None,
        launcher: "true".to_owned(),
    };
    let bus = Bus::start();
    async_io::block_on(async {
        let server = bus.connect().await;
        server
            .object_server()
            .at(TRUST_PATH, Trust::new(config))
            .await
            .unwrap();
        let name = server.unique_name().unwrap().to_string();
        let client = bus.connect().await;
        let trust = Trust1Proxy::builder(&client)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();
        let path = |p: &Path| p.to_string_lossy().into_owned();

        let (verdict, info) = trust.assess(&path(&verified)).await.unwrap();
        assert_eq!(verdict, "confirm");
        assert_eq!(s(&info, "developer_status"), "verified");
        assert_eq!(s(&info, "origin"), "file:///Downloads/V.sxv");
        let (verdict, info) = trust.assess(&path(&selfsigned)).await.unwrap();
        assert_eq!(
            (verdict.as_str(), s(&info, "reason").as_str()),
            ("block", "self-signed")
        );
        let (verdict, info) = trust.assess(&path(&unsigned)).await.unwrap();
        assert_eq!(
            (verdict.as_str(), s(&info, "reason").as_str()),
            ("block", "unsigned")
        );
        // Assessing the same bundles again finds the same outcome: trust.bin is not rewritten
        // (silica-desktop#260).
        let store = root.join("state/trust.bin");
        let written = std::fs::metadata(&store).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(trust.assess(&path(&selfsigned)).await.unwrap().0, "block");
        assert_eq!(trust.assess(&path(&unsigned)).await.unwrap().0, "block");
        assert_eq!(
            std::fs::metadata(&store).unwrap().modified().unwrap(),
            written
        );
        assert_eq!(trust.assess("/usr/bin/true").await.unwrap().0, "allow");

        // Without a shell to ask, a prompt is answered at once.
        assert_eq!(trust.prompt(&path(&verified)).await.unwrap(), "cancel");
        trust.register_agent().await.unwrap();
        // The prompt of the verified app: a stand-in shell answers Open.
        let mut requests = trust.receive_prompt_requested().await.unwrap();
        let shell = async {
            let signal = requests.next().await.unwrap();
            let args = signal.args().unwrap();
            assert_eq!(args.verdict, "confirm");
            assert_eq!(s(&args.info, "name"), "Verified");
            trust.answer(args.id, "open").await.unwrap();
            futures_lite::future::pending::<()>().await;
        };
        let target = path(&verified);
        let choice = trust.prompt(&target).or(async {
            shell.await;
            panic!("the stand-in shell stopped")
        });
        assert_eq!(choice.await.unwrap(), "open");
        // Accepted: out of quarantine, approved, allowed from now on.
        assert!(quarantine::read(&verified).is_none());
        assert_eq!(trust.assess(&path(&verified)).await.unwrap().0, "allow");

        // The self-signed one: the alert says it was blocked; Done is cancel.
        let shell = async {
            let signal = requests.next().await.unwrap();
            let args = signal.args().unwrap();
            assert_eq!(args.verdict, "block");
            trust.answer(args.id, "cancel").await.unwrap();
            futures_lite::future::pending::<()>().await;
        };
        let target = path(&selfsigned);
        let choice = trust.prompt(&target).or(async {
            shell.await;
            panic!("the stand-in shell stopped")
        });
        assert_eq!(choice.await.unwrap(), "cancel");
        let blocked = trust.blocked().await.unwrap();
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].1, "Mine");
        // Open Anyway needs polkit: without a system bus it is refused.
        assert!(!trust.open_anyway(&path(&selfsigned)).await.unwrap());
        // Not blocked recently: an error.
        assert!(trust.open_anyway(&path(&unsigned)).await.is_err());

        // A file of the approved app modified in place: blocked as tampered.
        std::fs::write(verified.join("Contents/bin/run"), "#!/bin/sh\nevil\n").unwrap();
        let (verdict, info) = trust.assess(&path(&verified)).await.unwrap();
        assert_eq!(
            (verdict.as_str(), s(&info, "reason").as_str()),
            ("block", "tampered")
        );
        // A bad answer is refused.
        assert!(trust.answer(999, "open").await.is_err());
    });
    let _ = std::fs::remove_dir_all(&root);
}
