//! `silica-launch`: the trust check and the bubblewrap sandbox of app bundles
//! (docs/spec/launch.md). The pieces that decide the command line live here, free of side
//! effects, so that they are tested without starting anything.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use silica_apps::info::{AppKind, Devices, Info, Runtime};

/// The Flutter embedder command of the Silica platform.
pub const EMBEDDER: &str = "silica-app";
/// Where the image installs it when `PATH` does not have it.
pub const EMBEDDER_FALLBACK: &str = "/usr/libexec/silica/silica-app";

/// The session the app runs in (from the environment of `silica-launch`).
#[derive(Clone, Debug, Default)]
pub struct Session {
    pub home: PathBuf,
    pub uid: u32,
    pub runtime_dir: PathBuf,
    /// `WAYLAND_DISPLAY` (a name in the runtime dir, or an absolute path).
    pub wayland: Option<String>,
    /// Environment passed into the sandbox (language, cursor, Silica's own variables).
    pub env: BTreeMap<String, String>,
    /// Extra read-only folders of the platform (the development VM's `/var/lib/silica-dev`).
    pub platform_extra: Vec<PathBuf>,
    /// The embedder command (resolved on `PATH`).
    pub embedder: PathBuf,
    /// The Flatpak installations (user first): `<dir>/runtime/<name>/x86_64/<branch>/active`.
    pub flatpak_dirs: Vec<PathBuf>,
}

/// The variables copied into the sandbox, when set.
pub const PASSED_ENV: [&str; 16] = [
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_MESSAGES",
    "LC_TIME",
    "LC_NUMERIC",
    "LC_MONETARY",
    "LC_COLLATE",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "XCURSOR_THEME",
    "XCURSOR_SIZE",
    "SILICA_RENDERER",
    "SILICA_ICONS_DIR",
    "SILICA_ASSETS",
    "TZ",
];

impl Session {
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Session {
        let home = PathBuf::from(env("HOME").unwrap_or_else(|| "/".to_owned()));
        let uid = rustix_uid();
        let runtime_dir = env("XDG_RUNTIME_DIR")
            .filter(|v| v.starts_with('/'))
            .map_or_else(|| PathBuf::from(format!("/run/user/{uid}")), PathBuf::from);
        let path = env("PATH").unwrap_or_default();
        let embedder = std::env::split_paths(&path)
            .map(|d| d.join(EMBEDDER))
            .find(|p| p.is_file())
            .unwrap_or_else(|| PathBuf::from(EMBEDDER_FALLBACK));
        let mut vars = BTreeMap::new();
        for key in PASSED_ENV {
            if let Some(value) = env(key) {
                vars.insert(key.to_owned(), value);
            }
        }
        vars.insert("PATH".to_owned(), path);
        Session {
            flatpak_dirs: vec![
                home.join(".local/share/flatpak"),
                PathBuf::from("/var/lib/flatpak"),
            ],
            home,
            uid,
            runtime_dir,
            wayland: env("WAYLAND_DISPLAY").filter(|v| !v.is_empty()),
            env: vars,
            platform_extra: env("SILICA_PLATFORM_EXTRA")
                .unwrap_or_default()
                .split(':')
                .filter(|p| p.starts_with('/'))
                .map(PathBuf::from)
                .collect(),
            embedder,
        }
    }

    /// The Wayland socket's path.
    pub fn wayland_socket(&self) -> Option<PathBuf> {
        let display = self.wayland.as_deref()?;
        Some(if display.starts_with('/') {
            PathBuf::from(display)
        } else {
            self.runtime_dir.join(display)
        })
    }

    /// The app's data folder, `~/.var/app/<id>`.
    pub fn data_dir(&self, id: &str) -> PathBuf {
        self.home.join(".var/app").join(id)
    }
}

fn rustix_uid() -> u32 {
    // The owner of /proc/self is the real user of this process.
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").map_or(0, |m| m.uid())
}

/// Where a runtime's files are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFiles {
    /// The image's `/usr` (and `/etc`): the Silica platform.
    Host,
    /// A Flatpak runtime: its `files/` folder, and the GL extension when installed.
    Flatpak { files: PathBuf, gl: Option<PathBuf> },
    /// Nothing: the bundle carries everything.
    None,
}

/// Finds the files of `runtime`.
pub fn resolve_runtime(runtime: &Runtime, session: &Session) -> Result<RuntimeFiles, String> {
    match runtime {
        Runtime::None => Ok(RuntimeFiles::None),
        Runtime::Ref { name, branch } if name == silica_apps::info::PLATFORM => {
            if branch == "1" {
                Ok(RuntimeFiles::Host)
            } else {
                Err(format!(
                    "{name}//{branch} is not this Silica's platform (it provides branch 1)"
                ))
            }
        }
        Runtime::Ref { name, branch } => {
            let find = |name: &str, branch: &str| {
                session
                    .flatpak_dirs
                    .iter()
                    .map(|d| {
                        d.join("runtime")
                            .join(name)
                            .join("x86_64")
                            .join(branch)
                            .join("active/files")
                    })
                    .find(|p| p.is_dir())
            };
            let files = find(name, branch).ok_or_else(|| {
                format!("the runtime {name}//{branch} is not installed (flatpak install flathub {name}//{branch})")
            })?;
            let gl = find(&format!("{name}.GL.default"), branch);
            Ok(RuntimeFiles::Flatpak { files, gl })
        }
    }
}

/// The command that starts the app inside (`/app` is the bundle's `Contents/`) or outside
/// (`contents` given) the sandbox.
pub fn app_command(
    info: &Info,
    contents: &Path,
    embedder: &Path,
    targets: &[String],
) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();
    match info.kind {
        AppKind::Flutter => {
            argv.push(embedder.to_string_lossy().into_owned());
            argv.push("--app-id".to_owned());
            argv.push(info.id.clone());
            argv.extend(info.args.iter().cloned());
            argv.push(
                contents
                    .join(&info.exec)
                    .join("flutter_assets")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        AppKind::Native => {
            argv.push(contents.join(&info.exec).to_string_lossy().into_owned());
            argv.extend(info.args.iter().cloned());
        }
    }
    argv.extend(targets.iter().cloned());
    argv
}

/// `/.flatpak-info` for the sandbox: portals and the permission store read the app id here.
pub fn flatpak_info(info: &Info, instance: &str) -> String {
    let runtime = match &info.runtime {
        Runtime::Ref { name, branch } => format!("runtime/{name}/x86_64/{branch}"),
        Runtime::None => "runtime/none/x86_64/none".to_owned(),
    };
    let p = &info.permissions;
    let mut shared = Vec::new();
    if p.network {
        shared.push("network");
    }
    let devices = match p.devices {
        Devices::None => "",
        Devices::Gpu => "dri;",
        Devices::All => "all;",
    };
    let filesystems: String = p.filesystem.iter().map(|f| format!("{f};")).collect();
    format!(
        "[Application]\nname={id}\nruntime={runtime}\n\n\
         [Instance]\ninstance-id={instance}\napp-path=/app\nflatpak-version=1.16.0\n\
         session-bus-proxy=true\nsystem-bus-proxy=false\n\n\
         [Context]\nshared={shared}\nsockets=wayland;pulseaudio;session-bus;\ndevices={devices}\nfilesystems={filesystems}\n\n\
         [Silica]\nbundle=true\n",
        id = info.id,
        shared = shared.iter().map(|s| format!("{s};")).collect::<String>(),
    )
}

/// The rules of `xdg-dbus-proxy` for the app (`--filter` and what it may own, call and see).
pub fn proxy_rules(info: &Info) -> Vec<String> {
    let mut rules = vec![
        "--filter".to_owned(),
        format!("--own={}", info.id),
        format!("--own={}.*", info.id),
        "--talk=org.freedesktop.portal.*".to_owned(),
        "--talk=org.freedesktop.Notifications".to_owned(),
        "--talk=org.a11y.Bus".to_owned(),
        "--broadcast=org.freedesktop.portal.*=@/org/freedesktop/portal/*".to_owned(),
    ];
    const SILICA: &str = "dev.frascella.Silica";
    for (member, path) in [
        ("Settings1.Get", "Settings1"),
        ("Settings1.GetAll", "Settings1"),
        ("Thumbnails1.*", "Thumbnails1"),
        ("Share1.*", "Share1"),
        ("Sound1.*", "Sound1"),
        ("System1.State", "System1"),
    ] {
        rules.push(format!(
            "--call={SILICA}={SILICA}.{member}@/dev/frascella/Silica/{path}"
        ));
    }
    for (member, path) in [
        ("Settings1.Changed", "Settings1"),
        ("System1.Changed", "System1"),
    ] {
        rules.push(format!(
            "--broadcast={SILICA}={SILICA}.{member}@/dev/frascella/Silica/{path}"
        ));
    }
    for name in &info.permissions.talk {
        rules.push(format!("--talk={name}"));
    }
    rules
}

/// The host folder of a `filesystem` permission item, and whether it is read-only.
pub fn filesystem_path(item: &str, home: &Path) -> Option<(PathBuf, bool)> {
    let (item, ro) = match item.strip_suffix(":ro") {
        Some(rest) => (rest, true),
        None => (item, false),
    };
    let xdg = |name: &str| home.join(name);
    let path = match item {
        "home" => home.to_path_buf(),
        "xdg-documents" => xdg("Documents"),
        "xdg-download" => xdg("Downloads"),
        "xdg-music" => xdg("Music"),
        "xdg-pictures" => xdg("Pictures"),
        "xdg-videos" => xdg("Videos"),
        "xdg-desktop" => xdg("Desktop"),
        "xdg-templates" => xdg("Templates"),
        other => {
            if let Some(sub) = other.strip_prefix("xdg-config/") {
                home.join(".config").join(sub)
            } else if let Some(sub) = other.strip_prefix("xdg-data/") {
                home.join(".local/share").join(sub)
            } else if other.starts_with('/') {
                PathBuf::from(other)
            } else {
                return None;
            }
        }
    };
    Some((path, ro))
}

/// What the sandbox is made of.
#[derive(Clone, Debug)]
pub struct Sandbox<'a> {
    pub info: &'a Info,
    /// The bundle's `Contents/`.
    pub contents: &'a Path,
    pub runtime: RuntimeFiles,
    pub session: &'a Session,
    /// The `/.flatpak-info` file to bind.
    pub info_file: &'a Path,
    /// The app registry folder of the sandbox (its `$XDG_RUNTIME_DIR/silica-apps`): the app's
    /// socket is relayed to the session's registry by `silica-launch` (the app's own pid means
    /// nothing outside its pid namespace).
    pub registry: Option<&'a Path>,
    /// The D-Bus proxy's socket, when there is a session bus.
    pub bus_socket: Option<&'a Path>,
    /// Files from the launch, bound read-write at their own paths (document portal not used).
    pub targets: &'a [String],
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The `bwrap` arguments (without `bwrap` itself), then `--` and the app's command.
pub fn bwrap_args(sb: &Sandbox<'_>) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| (*s).to_owned()));
    push(&[
        "--die-with-parent",
        "--new-session",
        "--unshare-user-try",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-cgroup-try",
        "--clearenv",
    ]);
    let p = &sb.info.permissions;
    if !p.network {
        a.push("--unshare-net".to_owned());
    }
    let bind = |a: &mut Vec<String>, kind: &str, from: &Path, to: &Path| {
        a.push(kind.to_owned());
        a.push(s(from));
        a.push(s(to));
    };
    match &sb.runtime {
        RuntimeFiles::Host => {
            bind(&mut a, "--ro-bind", Path::new("/usr"), Path::new("/usr"));
            bind(&mut a, "--ro-bind", Path::new("/etc"), Path::new("/etc"));
            for extra in &sb.session.platform_extra {
                bind(&mut a, "--ro-bind-try", extra, extra);
            }
        }
        RuntimeFiles::Flatpak { files, gl } => {
            bind(&mut a, "--ro-bind", files, Path::new("/usr"));
            bind(&mut a, "--ro-bind", &files.join("etc"), Path::new("/etc"));
            if let Some(gl) = gl {
                bind(
                    &mut a,
                    "--ro-bind",
                    gl,
                    Path::new("/usr/lib/x86_64-linux-gnu/GL/default"),
                );
            }
            for file in [
                "passwd",
                "group",
                "resolv.conf",
                "hosts",
                "localtime",
                "machine-id",
            ] {
                let host = Path::new("/etc").join(file);
                bind(&mut a, "--ro-bind-try", &host, &host);
            }
        }
        RuntimeFiles::None => {
            a.extend(["--tmpfs".to_owned(), "/usr".to_owned()]);
            for loader in [
                "/usr/lib64/ld-linux-x86-64.so.2",
                "/lib64/ld-linux-x86-64.so.2",
            ] {
                bind(
                    &mut a,
                    "--ro-bind-try",
                    Path::new(loader),
                    Path::new(loader),
                );
            }
        }
    }
    for (link, target) in [
        ("/lib", "usr/lib"),
        ("/lib64", "usr/lib64"),
        ("/bin", "usr/bin"),
        ("/sbin", "usr/sbin"),
    ] {
        a.extend(["--symlink".to_owned(), target.to_owned(), link.to_owned()]);
    }
    a.extend(["--proc".to_owned(), "/proc".to_owned()]);
    a.extend(["--dev".to_owned(), "/dev".to_owned()]);
    match p.devices {
        Devices::None => {}
        Devices::Gpu => bind(
            &mut a,
            "--dev-bind-try",
            Path::new("/dev/dri"),
            Path::new("/dev/dri"),
        ),
        Devices::All => bind(&mut a, "--dev-bind", Path::new("/dev"), Path::new("/dev")),
    }
    // The GPU drivers find their device through /sys (libdrm): without it Mesa falls back to
    // software rendering. Read-only, the parts Flatpak gives its apps too.
    if p.devices != Devices::None {
        for sys in ["/sys/dev", "/sys/devices", "/sys/class", "/sys/bus"] {
            bind(&mut a, "--ro-bind-try", Path::new(sys), Path::new(sys));
        }
    }
    a.extend(["--tmpfs".to_owned(), "/tmp".to_owned()]);
    a.extend(["--dir".to_owned(), "/var".to_owned()]);
    a.extend([
        "--symlink".to_owned(),
        "../tmp".to_owned(),
        "/var/tmp".to_owned(),
    ]);
    bind(&mut a, "--ro-bind", sb.contents, Path::new("/app"));
    bind(
        &mut a,
        "--ro-bind",
        sb.info_file,
        Path::new("/.flatpak-info"),
    );
    // The runtime folder: only the sockets the app needs.
    let run = &sb.session.runtime_dir;
    a.extend(["--tmpfs".to_owned(), "/run".to_owned()]);
    a.extend([
        "--perms".to_owned(),
        "0700".to_owned(),
        "--dir".to_owned(),
        s(run),
    ]);
    if let Some(socket) = sb.session.wayland_socket() {
        bind(&mut a, "--ro-bind", &socket, &run.join("wayland-0"));
    }
    bind(
        &mut a,
        "--ro-bind-try",
        &run.join("pipewire-0"),
        &run.join("pipewire-0"),
    );
    // The app registry (silica-control `apps`): a folder of the instance, relayed.
    if let Some(registry) = sb.registry {
        bind(&mut a, "--bind", registry, &run.join("silica-apps"));
    }
    if let Some(bus) = sb.bus_socket {
        bind(&mut a, "--bind", bus, &run.join("bus"));
    }
    bind(
        &mut a,
        "--bind-try",
        &run.join("doc/by-app").join(&sb.info.id),
        &run.join("doc"),
    );
    // Home: only the app's data folder and the granted folders.
    let home = &sb.session.home;
    let data = sb.session.data_dir(&sb.info.id);
    a.extend(["--dir".to_owned(), s(home)]);
    bind(&mut a, "--bind", &data, &data);
    for item in &p.filesystem {
        if let Some((path, ro)) = filesystem_path(item, home) {
            bind(
                &mut a,
                if ro { "--ro-bind-try" } else { "--bind-try" },
                &path,
                &path,
            );
        }
    }
    for target in sb.targets {
        if let Some(path) = silica_apps::exec::target_path(target) {
            let path = PathBuf::from(path);
            bind(&mut a, "--bind-try", &path, &path);
        }
    }
    // The environment.
    let mut env: BTreeMap<String, String> = sb.session.env.clone();
    env.insert("HOME".to_owned(), s(home));
    env.insert("XDG_RUNTIME_DIR".to_owned(), s(run));
    env.insert("WAYLAND_DISPLAY".to_owned(), "wayland-0".to_owned());
    env.insert("FLATPAK_ID".to_owned(), sb.info.id.clone());
    env.insert("container".to_owned(), "flatpak".to_owned());
    env.insert("XDG_DATA_HOME".to_owned(), s(&data.join("data")));
    env.insert("XDG_CONFIG_HOME".to_owned(), s(&data.join("config")));
    env.insert("XDG_CACHE_HOME".to_owned(), s(&data.join("cache")));
    env.insert("XDG_STATE_HOME".to_owned(), s(&data.join(".local/state")));
    if sb.bus_socket.is_some() {
        env.insert(
            "DBUS_SESSION_BUS_ADDRESS".to_owned(),
            format!("unix:path={}", s(&run.join("bus"))),
        );
    }
    if matches!(sb.runtime, RuntimeFiles::None) {
        env.insert("LD_LIBRARY_PATH".to_owned(), "/app/lib".to_owned());
    }
    if matches!(sb.runtime, RuntimeFiles::Flatpak { .. }) {
        env.insert("PATH".to_owned(), "/app/bin:/usr/bin".to_owned());
    }
    for (k, v) in env {
        a.extend(["--setenv".to_owned(), k, v]);
    }
    a.extend(["--chdir".to_owned(), s(home)]);
    a.push("--".to_owned());
    a.extend(app_command(
        sb.info,
        Path::new("/app"),
        &sb.session.embedder,
        sb.targets,
    ));
    a
}

/// A random instance id (16 hex digits).
pub fn instance_id() -> String {
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        bytes = u64::from(std::process::id()).to_le_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(extra: &str, kind: &str, runtime: &str, exec: &str) -> Info {
        Info::parse(&format!(
            "format = 1\nid = \"dev.example.Hello\"\nname = \"Hello\"\nversion = \"1\"\nbuild = 1\n\
             kind = \"{kind}\"\nruntime = \"{runtime}\"\n{exec}\nargs = [\"--size\", \"300x200\"]\n{extra}\n\
             [developer]\nname = \"E\"\nkey = \"K\"\n"
        ))
        .unwrap()
    }

    fn session() -> Session {
        Session {
            home: PathBuf::from("/home/u"),
            uid: 1000,
            runtime_dir: PathBuf::from("/run/user/1000"),
            wayland: Some("wayland-1".to_owned()),
            env: [("LANG".to_owned(), "it_IT.UTF-8".to_owned())]
                .into_iter()
                .collect(),
            platform_extra: vec![PathBuf::from("/var/lib/silica-dev")],
            embedder: PathBuf::from("/var/lib/silica-dev/bin/silica-app"),
            flatpak_dirs: vec![],
        }
    }

    fn pairs(args: &[String], flag: &str) -> Vec<(String, String)> {
        args.windows(3)
            .filter(|w| w[0] == flag)
            .map(|w| (w[1].clone(), w[2].clone()))
            .collect()
    }

    #[test]
    fn a_flutter_bundle_on_the_platform() {
        let i = info("", "flutter", "dev.silica.Platform//1", "");
        let sess = session();
        let sb = Sandbox {
            info: &i,
            contents: Path::new("/home/u/Applications/Hello.app/Contents"),
            runtime: RuntimeFiles::Host,
            session: &sess,
            info_file: Path::new("/run/user/1000/silica-launch/abc/info"),
            bus_socket: Some(Path::new("/run/user/1000/silica-launch/abc/bus")),
            registry: Some(Path::new("/run/user/1000/silica-launch/abc/apps")),
            targets: &["/home/u/Documents/a.txt".to_owned()],
        };
        let args = bwrap_args(&sb);
        // No network without the permission; the GPU only.
        assert!(args.contains(&"--unshare-net".to_owned()));
        let ro = pairs(&args, "--ro-bind");
        assert!(ro.contains(&("/usr".into(), "/usr".into())));
        assert!(ro.contains(&(
            "/home/u/Applications/Hello.app/Contents".into(),
            "/app".into()
        )));
        assert!(ro.contains(&(
            "/run/user/1000/silica-launch/abc/info".into(),
            "/.flatpak-info".into()
        )));
        assert!(ro.contains(&(
            "/run/user/1000/wayland-1".into(),
            "/run/user/1000/wayland-0".into()
        )));
        assert!(
            pairs(&args, "--ro-bind-try")
                .contains(&("/var/lib/silica-dev".into(), "/var/lib/silica-dev".into()))
        );
        assert!(pairs(&args, "--dev-bind-try").contains(&("/dev/dri".into(), "/dev/dri".into())));
        assert!(
            pairs(&args, "--ro-bind-try").contains(&("/sys/devices".into(), "/sys/devices".into()))
        );
        let rw = pairs(&args, "--bind");
        assert!(rw.contains(&(
            "/home/u/.var/app/dev.example.Hello".into(),
            "/home/u/.var/app/dev.example.Hello".into()
        )));
        assert!(rw.contains(&(
            "/run/user/1000/silica-launch/abc/bus".into(),
            "/run/user/1000/bus".into()
        )));
        // Nothing else of the home folder.
        assert!(!rw.iter().any(|(from, _)| from == "/home/u"));
        let env = pairs(&args, "--setenv");
        assert!(env.contains(&(
            "XDG_CONFIG_HOME".into(),
            "/home/u/.var/app/dev.example.Hello/config".into()
        )));
        assert!(env.contains(&("LANG".into(), "it_IT.UTF-8".into())));
        assert!(env.contains(&("FLATPAK_ID".into(), "dev.example.Hello".into())));
        assert!(env.contains(&(
            "DBUS_SESSION_BUS_ADDRESS".into(),
            "unix:path=/run/user/1000/bus".into()
        )));
        // The command: the embedder with the app id, the args and the bundle.
        let dash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            &args[dash + 1..],
            [
                "/var/lib/silica-dev/bin/silica-app",
                "--app-id",
                "dev.example.Hello",
                "--size",
                "300x200",
                "/app/app/flutter_assets",
                "/home/u/Documents/a.txt"
            ]
        );
        // The launched file is reachable.
        assert!(pairs(&args, "--bind-try").contains(&(
            "/home/u/Documents/a.txt".into(),
            "/home/u/Documents/a.txt".into()
        )));
    }

    #[test]
    fn permissions_open_what_they_name() {
        let i = info(
            "[permissions]\nnetwork = true\ndevices = \"all\"\nfilesystem = [\"xdg-download:ro\", \"home\"]\ntalk = [\"org.example.Service\"]",
            "native",
            "none",
            "exec = \"bin/hello\"",
        );
        let sess = session();
        let sb = Sandbox {
            info: &i,
            contents: Path::new("/A/Hello.app/Contents"),
            runtime: RuntimeFiles::None,
            session: &sess,
            info_file: Path::new("/i"),
            bus_socket: None,
            registry: None,
            targets: &[],
        };
        let args = bwrap_args(&sb);
        assert!(!args.contains(&"--unshare-net".to_owned()));
        assert!(pairs(&args, "--dev-bind").contains(&("/dev".into(), "/dev".into())));
        assert!(
            pairs(&args, "--ro-bind-try")
                .contains(&("/home/u/Downloads".into(), "/home/u/Downloads".into()))
        );
        assert!(pairs(&args, "--bind-try").contains(&("/home/u".into(), "/home/u".into())));
        let env = pairs(&args, "--setenv");
        assert!(env.contains(&("LD_LIBRARY_PATH".into(), "/app/lib".into())));
        assert!(!env.iter().any(|(k, _)| k == "DBUS_SESSION_BUS_ADDRESS"));
        let dash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(&args[dash + 1..], ["/app/bin/hello", "--size", "300x200"]);
        let rules = proxy_rules(&i);
        assert!(rules.contains(&"--talk=org.example.Service".to_owned()));
        assert!(rules.contains(&"--own=dev.example.Hello".to_owned()));
        assert!(rules.iter().any(|r| {
            r.starts_with("--call=dev.frascella.Silica=dev.frascella.Silica.Settings1.Get@")
        }));
        // Settings cannot be changed from inside.
        assert!(!rules.iter().any(|r| r.contains("Settings1.Set")));
        let fi = flatpak_info(&i, "abc");
        assert!(fi.contains("[Application]\nname=dev.example.Hello\n"));
        assert!(fi.contains("shared=network;"));
        assert!(fi.contains("devices=all;"));
        assert!(fi.contains("filesystems=xdg-download:ro;home;"));
    }

    #[test]
    fn runtimes_resolve() {
        let mut sess = session();
        let platform = Runtime::parse("dev.silica.Platform//1").unwrap();
        assert_eq!(resolve_runtime(&platform, &sess), Ok(RuntimeFiles::Host));
        assert!(
            resolve_runtime(&Runtime::parse("dev.silica.Platform//2").unwrap(), &sess).is_err()
        );
        let root = std::env::temp_dir().join(format!("launch-rt-{}", std::process::id()));
        let files = root.join("runtime/org.freedesktop.Platform/x86_64/24.08/active/files");
        std::fs::create_dir_all(&files).unwrap();
        sess.flatpak_dirs = vec![root.clone()];
        let fd = Runtime::parse("org.freedesktop.Platform//24.08").unwrap();
        assert_eq!(
            resolve_runtime(&fd, &sess),
            Ok(RuntimeFiles::Flatpak { files, gl: None })
        );
        let missing = Runtime::parse("org.kde.Platform//6.8").unwrap();
        assert!(
            resolve_runtime(&missing, &sess)
                .unwrap_err()
                .contains("flatpak install")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
