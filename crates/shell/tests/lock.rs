//! The lock screen on a headless Ossido: `session.lock` makes Ossido announce the lock, the
//! shell takes it (ext-session-lock) and draws its surfaces. Skipped when Ossido is not built
//! in this target directory.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use silica_control::Client;

struct Kill(Child);

impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn ossido_binary() -> Option<PathBuf> {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    ["debug", "release"]
        .iter()
        .map(|p| target.join(p).join("ossido"))
        .find(|p| p.is_file())
}

#[test]
fn the_shell_takes_the_session_lock_when_ossido_locks() {
    let Some(ossido) = ossido_binary() else {
        eprintln!("skipped: no Ossido built");
        return;
    };
    let dir = std::env::temp_dir().join(format!("silica-shell-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new(&ossido)
        .args(["--backend", "headless", "--size", "1280x800"])
        .args(["--animation-scale", "0", "--control", "0"])
        .env("XDG_RUNTIME_DIR", &dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", dir.join("no-bus").display()),
        )
        .env("RUST_LOG", "warn")
        .env_remove("WAYLAND_DISPLAY")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut wayland = None;
    let mut control = None;
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line.unwrap();
        if let Some(n) = line.strip_prefix("WAYLAND_DISPLAY=") {
            wayland = Some(n.to_string());
        }
        if let Some(a) = line.strip_prefix("OSSIDO_CONTROL=") {
            control = Some(a.to_string());
            break;
        }
    }
    let _ossido = Kill(child);
    let (wayland, control) = (wayland.unwrap(), control.unwrap());
    let shell = Command::new(env!("CARGO_BIN_EXE_silica-shell"))
        .env("XDG_RUNTIME_DIR", &dir)
        .env("WAYLAND_DISPLAY", &wayland)
        .env("SILICA_SHELL", "1")
        .env("VETRO_RENDERER", "software")
        .env("VETRO_MULTI_INSTANCE", "1")
        .env("VETRO_DETERMINISTIC", "1")
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", dir.join("no-bus").display()),
        )
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let _shell = Kill(shell);
    let mut client = Client::connect(&control).unwrap();
    // Ossido refuses to lock until a shell can unlock again.
    let start = Instant::now();
    loop {
        if client.call("session.lock", Value::Null).is_ok() {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the shell never joined"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    // The shell took the lock: Ossido sees a live locker.
    let start = Instant::now();
    loop {
        let state = client.call("idle.state", json!({})).unwrap();
        if state["lock"]["locked"] == json!(true) && state["lock"]["locker"] == json!(true) {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "no locker: {state}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
