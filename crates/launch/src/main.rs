//! `silica-launch [--print] <bundle.app | file.AppImage | app id> [URI…]` (docs/spec/launch.md).

use std::cell::OnceCell;
use std::collections::HashMap;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use silica_apps::info::Info;
use silica_apps::registry::{Dirs, Kind, Registry};
use silica_bundle::trust::{BundleFacts, Policy, Verdict, decide_bundle, verify_bundle};
use silica_launch::{
    RuntimeFiles, Sandbox, Session, app_command, bwrap_args, flatpak_info, instance_id,
    proxy_rules, resolve_runtime,
};
use zbus::zvariant::OwnedValue;

const USAGE: &str = "usage: silica-launch [--print] <bundle.app | file.AppImage | app id> [URI...]";

/// Exit statuses (docs/spec/launch.md).
const REFUSED: u8 = 3;
const NOT_FOUND: u8 = 4;
const NO_SANDBOX: u8 = 5;

fn fail(code: u8, text: &str) -> ExitCode {
    eprintln!("silica-launch: {text}");
    ExitCode::from(code)
}

/// When the launch started, with `SILICA_LAUNCH_TRACE=1` (docs/spec/launch.md, "Start time").
static TRACE: OnceLock<Option<Instant>> = OnceLock::new();

/// Writes how long the launch has taken so far, and the stage reached, on standard error (the
/// session log) when tracing is on.
fn trace(stage: &str) {
    if let Some(Some(start)) = TRACE.get() {
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("silica-launch: {ms:8.1} ms {stage}");
    }
}

/// The session bus, connected the first time something needs it: a system app started by its
/// path never connects.
struct Bus(OnceCell<Option<zbus::blocking::Connection>>);

impl Bus {
    fn get(&self) -> Option<&zbus::blocking::Connection> {
        self.0
            .get_or_init(|| {
                let bus = zbus::blocking::Connection::session().ok();
                trace("session bus connected");
                bus
            })
            .as_ref()
    }
}

fn main() -> ExitCode {
    let _ = TRACE.set(
        std::env::var_os("SILICA_LAUNCH_TRACE")
            .is_some_and(|v| v == "1")
            .then(Instant::now),
    );
    let mut print = false;
    let mut rest: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--print" if rest.is_empty() => print = true,
            "-h" | "--help" if rest.is_empty() => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => rest.push(arg),
        }
    }
    let Some((target, uris)) = rest.split_first() else {
        return fail(2, USAGE);
    };
    let env = |k: &str| std::env::var(k).ok();
    let bus = Bus(OnceCell::new());
    let resolved = match resolve(target, &bus, &env) {
        Ok(r) => r,
        Err(e) => return fail(NOT_FOUND, &e),
    };
    trace("resolved");
    match resolved {
        Target::Flatpak(id) => exec_plain(
            print,
            std::iter::once("flatpak".to_owned())
                .chain(["run".to_owned(), id])
                .chain(uris.iter().cloned())
                .collect(),
        ),
        Target::Entry(exec) => {
            let lines = silica_apps::exec::launch_lines(
                &exec,
                &silica_apps::exec::Context {
                    targets: uris,
                    ..Default::default()
                },
            );
            if let [line] = lines.as_slice() {
                return exec_plain(print, line.clone());
            }
            let mut code = ExitCode::SUCCESS;
            for line in lines {
                code = run_plain(print, line);
            }
            code
        }
        Target::Path(path) => launch_path(&path, uris, print, &bus, &env),
    }
}

/// What to start.
enum Target {
    /// A bundle folder or an AppImage file.
    Path(PathBuf),
    Flatpak(String),
    /// A desktop entry's `Exec`.
    Entry(String),
}

fn resolve(
    target: &str,
    bus: &Bus,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Target, String> {
    if target.contains('/') {
        let path = std::fs::canonicalize(target).map_err(|e| format!("{target}: {e}"))?;
        return Ok(Target::Path(path));
    }
    // An id: the registry of the session, else a scan of the same folders.
    if let Some(bus) = bus.get()
        && let Ok(reply) = bus.call_method(
            Some("dev.frascella.Silica"),
            "/dev/frascella/Silica/Apps1",
            Some("dev.frascella.Silica.Apps1"),
            "Get",
            &(target,),
        )
        && let Ok(props) = reply.body().deserialize::<HashMap<String, OwnedValue>>()
    {
        let get = |k: &str| {
            props
                .get(k)
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default()
        };
        return Ok(match get("kind").as_str() {
            "bundle" | "appimage" => Target::Path(PathBuf::from(get("path"))),
            "flatpak" => Target::Flatpak(get("id")),
            _ if get("desktop_file").is_empty() => Target::Path(PathBuf::from(get("path"))),
            _ => Target::Entry(get("exec")),
        });
    }
    let registry = Registry::scan(Dirs::from_env(env));
    let app = registry
        .get(target)
        .ok_or_else(|| format!("no app with the id {target}"))?;
    Ok(match app.kind {
        Kind::Flatpak => Target::Flatpak(app.id.clone()),
        _ if app.desktop_file.is_none() => Target::Path(app.path.clone()),
        _ => Target::Entry(app.exec.clone()),
    })
}

/// Becomes the command (no process stays between the caller and the app, whose pid is then the
/// one the caller started), or prints it with `--print`.
fn exec_plain(print: bool, argv: Vec<String>) -> ExitCode {
    if print {
        return run_plain(print, argv);
    }
    let Some((program, args)) = argv.split_first() else {
        return fail(NOT_FOUND, "empty command");
    };
    trace("exec");
    // `exec` returns only when it failed.
    let err = Command::new(program).args(args).exec();
    fail(NOT_FOUND, &format!("{program}: {err}"))
}

fn run_plain(print: bool, argv: Vec<String>) -> ExitCode {
    if print {
        for a in &argv {
            println!("{a}");
        }
        return ExitCode::SUCCESS;
    }
    let Some((program, args)) = argv.split_first() else {
        return fail(NOT_FOUND, "empty command");
    };
    match Command::new(program).args(args).status() {
        Ok(status) => ExitCode::from(u8::try_from(status.code().unwrap_or(1)).unwrap_or(1)),
        Err(e) => fail(NOT_FOUND, &format!("{program}: {e}")),
    }
}

/// Whether `path` belongs to the system image.
fn is_system(path: &Path) -> bool {
    silica_apps::registry::is_system_path(path)
}

/// The trust verdict: the daemon's (with its prompt), or without the daemon a local check that
/// lets only `allow` through.
fn trusted(path: &Path, bus: &Bus) -> Result<(), String> {
    if is_system(path) {
        return Ok(());
    }
    let text = path.to_string_lossy().into_owned();
    if let Some(bus) = bus.get() {
        let call = |method: &str| {
            bus.call_method(
                Some("dev.frascella.Silica"),
                "/dev/frascella/Silica/Trust1",
                Some("dev.frascella.Silica.Trust1"),
                method,
                &(text.as_str(),),
            )
        };
        if let Ok(reply) = call("Assess")
            && let Ok((verdict, _)) = reply
                .body()
                .deserialize::<(String, HashMap<String, OwnedValue>)>()
        {
            trace(&format!("assessed: {verdict}"));
            if verdict == "allow" {
                return Ok(());
            }
            // The alert in the shell; the user's choice comes back.
            let choice = call("Prompt")
                .ok()
                .and_then(|r| r.body().deserialize::<String>().ok())
                .unwrap_or_else(|| "cancel".to_owned());
            return if choice == "open" {
                Ok(())
            } else {
                Err(format!("not opened ({verdict}, {choice})"))
            };
        }
    }
    // No trust service: verify here; anything but allow stops (no way to ask the user).
    if path.join("Contents/Info.toml").exists() {
        let roots = silica_bundle::trust::Roots::system(None);
        let v = verify_bundle(path, &roots);
        let facts = BundleFacts {
            integrity: v.integrity,
            developer: v.developer,
            quarantined: silica_bundle::quarantine::read(path).is_some(),
            approved: false,
        };
        let (verdict, reason) = decide_bundle(&facts, Policy::Trusted);
        if verdict == Verdict::Allow {
            return Ok(());
        }
        return Err(format!(
            "the trust service is not running and the app is not allowed by itself ({}{})",
            verdict.as_str(),
            if reason.is_empty() {
                String::new()
            } else {
                format!(": {reason}")
            }
        ));
    }
    Err("the trust service is not running: AppImages need its confirmation".to_owned())
}

fn launch_path(
    path: &Path,
    uris: &[String],
    print: bool,
    bus: &Bus,
    env: &dyn Fn(&str) -> Option<String>,
) -> ExitCode {
    let session = Session::from_env(env);
    let is_bundle = path.join("Contents/Info.toml").exists();
    if !is_bundle && !silica_apps::appimage::is_appimage(path) {
        return fail(
            NOT_FOUND,
            &format!("{}: not an app bundle or an AppImage", path.display()),
        );
    }
    if !print && let Err(why) = trusted(path, bus) {
        return fail(REFUSED, &why);
    }
    if !is_bundle {
        // AppImages run as they are, after their confirmation.
        return exec_plain(
            print,
            std::iter::once(path.to_string_lossy().into_owned())
                .chain(uris.iter().cloned())
                .collect(),
        );
    }
    let info = match Info::read(path) {
        Ok(info) => info,
        Err(e) => return fail(NOT_FOUND, &e.to_string()),
    };
    trace("Info.toml read");
    let contents = path.join("Contents");
    if is_system(path) {
        // Part of the image: no sandbox, and nothing to wait for.
        return exec_plain(
            print,
            app_command(&info, &contents, &session.embedder, uris),
        );
    }
    let runtime = match resolve_runtime(&info.runtime, &session) {
        Ok(r) => r,
        Err(e) => return fail(NO_SANDBOX, &e),
    };
    run_sandboxed(
        &info,
        &contents,
        runtime,
        &session,
        uris,
        print,
        bus.get().is_some(),
    )
}

/// Waits until `path` exists, at most `limit`.
fn wait_for(path: &Path, limit: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < limit {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    path.exists()
}

fn run_sandboxed(
    info: &Info,
    contents: &Path,
    runtime: RuntimeFiles,
    session: &Session,
    uris: &[String],
    print: bool,
    has_bus: bool,
) -> ExitCode {
    let instance = instance_id();
    let dir = session.runtime_dir.join("silica-launch").join(&instance);
    let info_file = dir.join("info");
    let bus_socket = dir.join("bus");
    let registry = dir.join("apps");
    let data = session.data_dir(&info.id);
    if !print {
        if let Err(e) = std::fs::create_dir_all(&registry)
            .and_then(|()| std::fs::write(&info_file, flatpak_info(info, &instance)))
        {
            return fail(NO_SANDBOX, &format!("{}: {e}", dir.display()));
        }
        for sub in ["data", "config", "cache", ".local/state"] {
            if let Err(e) = std::fs::create_dir_all(data.join(sub)) {
                return fail(NO_SANDBOX, &format!("{}: {e}", data.display()));
            }
        }
    }
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS")
        .ok()
        .filter(|_| has_bus);
    let sandbox = Sandbox {
        info,
        contents,
        runtime,
        session,
        info_file: &info_file,
        bus_socket: address.as_ref().map(|_| bus_socket.as_path()),
        registry: Some(&registry),
        targets: uris,
    };
    let args = bwrap_args(&sandbox);
    trace("sandbox prepared");
    if print {
        println!("bwrap");
        for a in &args {
            println!("{a}");
        }
        return ExitCode::SUCCESS;
    }
    // The D-Bus proxy: it writes a byte on its stdout when ready, and exits when this process
    // (the read end) goes away.
    let mut proxy = None;
    if let Some(address) = &address {
        let child = Command::new("xdg-dbus-proxy")
            .arg(address)
            .arg(&bus_socket)
            .args(proxy_rules(info))
            .arg("--fd=1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn();
        match child {
            Ok(mut child) => {
                let mut ready = [0u8; 1];
                let synced = child
                    .stdout
                    .as_mut()
                    .is_some_and(|out| out.read_exact(&mut ready).is_ok());
                if !synced || !wait_for(&bus_socket, Duration::from_secs(3)) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_dir_all(&dir);
                    return fail(NO_SANDBOX, "xdg-dbus-proxy did not start");
                }
                trace("D-Bus proxy ready");
                proxy = Some(child);
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                return fail(
                    NO_SANDBOX,
                    &format!("xdg-dbus-proxy: {e} (install xdg-dbus-proxy)"),
                );
            }
        }
    }
    let relay = if print {
        None
    } else {
        registry_relay(&session.runtime_dir, &info.id, &registry)
    };
    trace("starting bwrap");
    let status = Command::new("bwrap")
        .args(&args)
        .stdin(Stdio::null())
        .status();
    if let Some(mut child) = proxy {
        // The proxy we started, by its own handle.
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(socket) = relay {
        let _ = std::fs::remove_file(socket);
    }
    let _ = std::fs::remove_dir_all(&dir);
    match status {
        Ok(status) => ExitCode::from(u8::try_from(status.code().unwrap_or(1)).unwrap_or(1)),
        Err(e) => fail(NO_SANDBOX, &format!("bwrap: {e} (install bubblewrap)")),
    }
}

/// Relays the session's app registry to the app in the sandbox: a socket
/// `$XDG_RUNTIME_DIR/silica-apps/<id>.<pid of silica-launch>.sock` (this process lives as long
/// as the app) whose connections go to the socket the app made in its own registry folder (its
/// pid there means nothing outside its pid namespace). Returns the relay socket.
fn registry_relay(runtime_dir: &Path, id: &str, inner: &Path) -> Option<PathBuf> {
    use std::os::unix::net::{UnixListener, UnixStream};
    let folder = runtime_dir.join("silica-apps");
    std::fs::create_dir_all(&folder).ok()?;
    let socket = folder.join(format!("{id}.{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).ok()?;
    let inner = inner.to_path_buf();
    let prefix = format!("{id}.");
    let _ = std::thread::Builder::new()
        .name("registry-relay".to_owned())
        .spawn(move || {
            for client in listener.incoming().flatten() {
                let target = std::fs::read_dir(&inner).ok().and_then(|read| {
                    read.flatten().map(|e| e.path()).find(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".sock"))
                    })
                });
                let Some(target) = target else { continue };
                let Ok(app) = UnixStream::connect(&target) else {
                    continue;
                };
                let (Ok(mut client_r), Ok(mut app_w)) = (client.try_clone(), app.try_clone())
                else {
                    continue;
                };
                let _ = std::thread::Builder::new()
                    .name("registry-relay-in".to_owned())
                    .spawn(move || {
                        let _ = std::io::copy(&mut client_r, &mut app_w);
                        let _ = app_w.shutdown(std::net::Shutdown::Write);
                    });
                let (mut app_r, mut client_w) = (app, client);
                let _ = std::io::copy(&mut app_r, &mut client_w);
            }
        });
    Some(socket)
}
