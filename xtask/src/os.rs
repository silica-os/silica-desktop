//! `cargo xtask os`: develop and test inside the real Silica OS, in a QEMU/KVM virtual machine
//! that stays open on the screen (docs/vm.md, "Development loop").
//!
//! The VM runs a released Silica OS disk (the qcow2 of silica-os) through a copy-on-write
//! overlay. Nothing is changed in the image: at boot, systemd credentials passed through
//! SMBIOS add what development needs:
//!
//! - root's SSH key (`ssh.authorized_keys.root`), with the guest's port 22 forwarded to the host;
//! - `/etc/silica/session.env.d/50-dev.env`: Ossido's control channel (`--control 7991` on the
//!   guest's loopback, reached through an SSH tunnel), the embedder's renderer, and a hook that
//!   runs the session from the host's share when one is deployed;
//! - a service that mounts the host's share (9p, read-only) on `/run/silica-dev`.
//!
//! `deploy` builds Ossido, the embedder, the services, the shell and (with `--apps`) the apps
//! here, copies them into the share and restarts the session in the guest: the user sees it
//! happen in the VM's window. The host's glibc must not be newer than the guest's (Fedora 44:
//! 2.43); `deploy` checks it.
//!
//! ```text
//! cargo xtask os start [--image PATH] [--fresh] [--virgl | --software] [--size WxH] [--headless]
//! cargo xtask os deploy [--apps | --apps=terminal,mail] [--no-build]
//! cargo xtask os shot PATH | cmd <silica-cli args> | ssh [command] | log | status | stop
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine as _;
use silica_control::Client;
use silica_control::commands::{PERF_FRAMES, UI_SCREENSHOT};

use crate::{Result, cargo_command, flutter, root, status};

// The embedder's driver choice, shared to pick the host's drivers for Venus.
#[path = "../../crates/embedder/src/vulkan_drivers.rs"]
#[allow(dead_code)]
mod vulkan_drivers;

/// Host ports (never 7979/7980, the development sessions on the host): Ossido's control
/// channel (SSH tunnel), the guest's SSH server, QEMU's monitor.
const CONTROL_PORT: u16 = 7993;
const SSH_PORT: u16 = 7998;
const MONITOR_PORT: u16 = 7996;
/// Ossido's control channel inside the guest (loopback only).
const GUEST_CONTROL: u16 = 7991;
/// Where the guest mounts the host's share (9p, no page cache): the staging area of `deploy`.
const GUEST_SHARE: &str = "/run/silica-dev";
/// Where the session runs from: a copy of the share on the guest's disk (`sync_to_guest_disk`).
/// Reading the apps, the engine and the icons over 9p made every launch seconds slower, and each
/// 9p request waits for QEMU's main loop (see docs/vm.md, "Launch times").
const GUEST_RUN: &str = "/var/lib/silica-dev";
/// The title of the VM's window.
const TITLE: &str = "Silica OS (dev)";
/// The user the image logs in automatically (silica-os, AUTOLOGIN_USER).
const GUEST_USER: &str = "silica";
/// Fedora 44's glibc: binaries built on a host with a newer one may not run in the guest.
const GUEST_GLIBC: (u32, u32) = (2, 43);

/// The apps deployed with `--apps`: repositories next to this one.
const APPS: &[&str] = &[
    "mustache",
    "mail/app",
    "terminal",
    "clock",
    "calculator",
    "settings",
    "preview",
];

pub fn os(args: &[String]) -> Result<()> {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("start") => start(rest),
        Some("deploy") => deploy(rest),
        Some("shot") => match rest.first() {
            Some(path) => shot(Path::new(path)),
            None => Err("usage: cargo xtask os shot PATH".to_owned()),
        },
        Some("cmd") => cmd(rest),
        Some("ssh") => ssh_interactive(rest),
        Some("log") => {
            let log = ssh_output(&format!(
                "cat /run/user/$(id -u {GUEST_USER})/silica-session.log"
            ))?;
            print!("{log}");
            Ok(())
        }
        Some("status") => status_report(),
        Some("restart") => {
            restart_session()?;
            std::thread::sleep(Duration::from_secs(3));
            wait_control(Duration::from_secs(120))
        }
        Some("stop") => stop(),
        _ => Err(
            "usage: cargo xtask os <start|deploy|shot|cmd|ssh|log|status|stop> (docs/vm.md)"
                .to_owned(),
        ),
    }
}

/// The work directory: overlay disk, keys, credentials, share, console, pid.
fn work_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("SILICA_OS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    Ok(Path::new(&home).join("build/silica-os-dev"))
}

fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Graphics {
    /// virtio-vga-gl with virgl: OpenGL in the guest on the host's GPU.
    Virgl,
    /// virgl plus Venus: Vulkan in the guest too (the default: the embedder renders with
    /// Impeller on Vulkan, Ossido with GLES, both on the host's GPU).
    Venus,
    /// virtio-vga without 3D: llvmpipe in the guest.
    Software,
}

impl Graphics {
    /// The embedder's renderer in the guest.
    fn renderer(self) -> &'static str {
        match self {
            Graphics::Venus => "vulkan",
            Graphics::Virgl => "impeller",
            Graphics::Software => "software",
        }
    }
}

struct StartOptions {
    image: Option<PathBuf>,
    fresh: bool,
    graphics: Graphics,
    size: (u32, u32),
    headless: bool,
}

fn parse_start(args: &[String]) -> Result<StartOptions> {
    let mut options = StartOptions {
        image: None,
        fresh: false,
        graphics: Graphics::Venus,
        size: (1920, 1200),
        headless: false,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--image" => options.image = args.next().map(PathBuf::from),
            "--fresh" => options.fresh = true,
            "--software" => options.graphics = Graphics::Software,
            "--venus" => options.graphics = Graphics::Venus,
            "--virgl" => options.graphics = Graphics::Virgl,
            "--headless" => options.headless = true,
            "--size" => {
                let text = args.next().ok_or("--size WxH")?;
                options.size = parse_size(text).ok_or(format!("invalid size: {text}"))?;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(options)
}

fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once('x')?;
    let size = (w.parse().ok()?, h.parse().ok()?);
    (size.0 >= 640 && size.1 >= 480).then_some(size)
}

/// The newest `silica-os-*.qcow2` in `~/silica-vm`, unless `--image` or `SILICA_OS_IMAGE`.
fn find_image(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(image) = explicit.or_else(|| std::env::var_os("SILICA_OS_IMAGE").map(PathBuf::from))
    {
        return Ok(image);
    }
    let home = std::env::var_os("HOME").ok_or("HOME not set")?;
    let dir = Path::new(&home).join("silica-vm");
    let mut images: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("silica-os-") && n.ends_with(".qcow2"))
        })
        .collect();
    images.sort_by_key(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());
    images.pop().ok_or(format!(
        "no silica-os-*.qcow2 in {}: gh release download -R silica-os/silica-os -p '*.qcow2' -D {}",
        dir.display(),
        dir.display()
    ))
}

fn start(args: &[String]) -> Result<()> {
    let options = parse_start(args)?;
    let work = work_dir()?;
    if running(&work) {
        // One long-running VM: `start` attaches to it (tunnel, control channel).
        eprintln!("xtask: the VM is already running: attaching");
        wait_ssh(&work, Duration::from_secs(300))?;
        tunnel(&work)?;
        return wait_control(Duration::from_secs(180));
    }
    std::fs::create_dir_all(work.join("share")).map_err(|e| format!("{e}"))?;
    let image = find_image(options.image.clone())?;
    let image = std::path::absolute(&image).map_err(|e| format!("{}: {e}", image.display()))?;

    let key = work.join("id_ed25519");
    if !key.exists() {
        let mut keygen = Command::new("ssh-keygen");
        keygen
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "silica-os-dev", "-f"])
            .arg(&key);
        status(keygen)?;
    }
    let overlay = work.join("overlay.qcow2");
    let backing = work.join("overlay.backing");
    let same_backing = std::fs::read_to_string(&backing).ok().as_deref()
        == Some(image.display().to_string().as_str());
    if options.fresh || !overlay.exists() || !same_backing {
        let _ = std::fs::remove_file(&overlay);
        let mut create = Command::new("qemu-img");
        create
            .args(["create", "-q", "-f", "qcow2", "-F", "qcow2", "-b"])
            .arg(&image)
            .arg(&overlay);
        status(create)?;
        write(&backing, &image.display().to_string())?;
    }
    let pubkey = std::fs::read_to_string(key.with_extension("pub"))
        .map_err(|e| format!("{}.pub: {e}", key.display()))?;
    let credentials = credentials(pubkey.trim(), options.graphics.renderer());

    let console = work.join("console.log");
    let (width, height) = options.size;
    let gl = options.graphics != Graphics::Software;
    let device = match options.graphics {
        Graphics::Software => format!("virtio-vga,xres={width},yres={height}"),
        Graphics::Virgl => format!("virtio-vga-gl,xres={width},yres={height}"),
        Graphics::Venus => {
            format!("virtio-vga-gl,xres={width},yres={height},hostmem=4G,blob=true,venus=on")
        }
    };
    let display = match (options.headless, gl) {
        (true, true) => "egl-headless".to_owned(),
        (true, false) => "none".to_owned(),
        (false, true) => "gtk,gl=on,full-screen=on,show-menubar=off".to_owned(),
        (false, false) => "gtk,full-screen=on,show-menubar=off".to_owned(),
    };
    let mut qemu = Command::new("setsid");
    qemu.arg("-f")
        .arg("qemu-system-x86_64")
        .args(["-name", TITLE, "-enable-kvm", "-cpu", "host", "-smp", "4"])
        .args(["-machine", "q35"])
        .arg("-drive")
        .arg(format!("file={},if=virtio,format=qcow2", overlay.display()))
        .args(["-device", &device, "-display", &display])
        .args([
            "-device",
            "virtio-keyboard-pci",
            "-device",
            "virtio-tablet-pci",
        ])
        // Sound through the host's PipeWire, so the interface sounds can be heard.
        .args([
            "-audiodev",
            "pipewire,id=snd0",
            "-device",
            "ich9-intel-hda",
            "-device",
            "hda-output,audiodev=snd0",
        ])
        .arg("-nic")
        .arg(format!(
            "user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:{SSH_PORT}-:22"
        ))
        .arg("-virtfs")
        .arg(format!(
            "local,path={},mount_tag=silica-dev,security_model=none,readonly=on",
            work.join("share").display()
        ))
        .arg("-serial")
        .arg(format!("file:{}", console.display()))
        .arg("-monitor")
        .arg(format!("tcp:127.0.0.1:{MONITOR_PORT},server,nowait"))
        .arg("-pidfile")
        .arg(work.join("qemu.pid"));
    if options.graphics == Graphics::Venus {
        // Venus maps guest memory into the host's Vulkan driver: shared memory.
        qemu.args([
            "-m",
            "6G",
            "-object",
            "memory-backend-memfd,id=mem,size=6G,share=on",
        ])
        .args(["-machine", "memory-backend=mem"]);
    } else {
        qemu.args(["-m", "6G"]);
    }
    for (name, data) in &credentials {
        let value = base64::engine::general_purpose::STANDARD.encode(data);
        qemu.arg("-smbios").arg(format!(
            "type=11,value=io.systemd.credential.binary:{name}={value}"
        ));
    }
    if options.graphics == Graphics::Venus
        && let Some(files) = host_vulkan_drivers()
    {
        eprintln!("xtask: Venus on the host's display GPU: VK_DRIVER_FILES={files}");
        qemu.env("VK_DRIVER_FILES", files);
    }
    // X11 (XWayland): the window can then be moved to another workspace with wmctrl.
    qemu.env("GDK_BACKEND", "x11")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(work.join("qemu.log")).map_err(|e| format!("{e}"))?);
    eprintln!(
        "xtask: starting {} on {} ({:?}, {width}x{height})",
        TITLE,
        image.display(),
        options.graphics
    );
    status(qemu)?;

    wait_ssh(&work, Duration::from_secs(300))?;
    tunnel(&work)?;
    wait_control(Duration::from_secs(180))?;
    if !options.headless && which("wmctrl") {
        // Best effort: the VM on the second workspace, out of the user's way.
        let mut wmctrl = Command::new("wmctrl");
        wmctrl.args(["-r", TITLE, "-t", "1"]);
        let _ = wmctrl.status();
    }
    eprintln!(
        "xtask: Silica OS is up: control 127.0.0.1:{CONTROL_PORT}, ssh -p {SSH_PORT} root@127.0.0.1 (key {}), monitor 127.0.0.1:{MONITOR_PORT}",
        key.display()
    );
    Ok(())
}

/// The host's Vulkan drivers for Venus (#191): only those of the GPU that drives the display.
/// Each Vulkan instance of the guest becomes one on the host, which otherwise loads every
/// driver and initialises every GPU (a discrete NVIDIA, llvmpipe): 100 to 200 ms per app
/// launch, against 15 ms with the integrated GPU alone, which the embedder picks anyway.
/// `None` (all drivers) when the user chose them or the GPU is not known.
fn host_vulkan_drivers() -> Option<String> {
    if vulkan_drivers::user_chooses() {
        return None;
    }
    let kernel = vulkan_drivers::primary_kernel_drivers(Path::new("/sys/class/drm"));
    let dirs = vulkan_drivers::manifest_dirs(|name| std::env::var(name).ok());
    let drivers = vulkan_drivers::select(&kernel, &vulkan_drivers::read_manifests(&dirs))?;
    let files: Vec<String> = drivers
        .iter()
        .map(|driver| driver.manifest.display().to_string())
        .collect();
    Some(files.join(":"))
}

fn which(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// The systemd credentials of the development VM (name, content).
fn credentials(pubkey: &str, renderer: &str) -> Vec<(String, String)> {
    let home = format!("/var/home/{GUEST_USER}");
    let tmpfiles = format!(
        "d /etc/silica/session.env.d 0755 root root -\n\
         f+^ /etc/silica/session.env.d/50-dev.env 0644 root root - tmpfiles.silica-dev-env\n\
         d {home}/.ssh 0700 {GUEST_USER} {GUEST_USER} -\n\
         f+^ {home}/.ssh/authorized_keys 0600 {GUEST_USER} {GUEST_USER} - ssh.authorized_keys.root\n"
    );
    let env = format!(
        "# Written at boot by `cargo xtask os` (a systemd credential): the development loop.\n\
         OSSIDO_ARGS=\"--control {GUEST_CONTROL}\"\n\
         SILICA_RENDERER={renderer}\n\
         RUST_LOG=info\n\
         # A deployed session (cargo xtask os deploy) replaces the installed one.\n\
         if [ -z \"$SILICA_DEV_SESSION\" ] && [ -x {GUEST_SHARE}/session.sh ]; then\n\
         \x20 SILICA_DEV_SESSION=1\n\
         \x20 export SILICA_DEV_SESSION\n\
         \x20 exec {GUEST_SHARE}/session.sh\n\
         fi\n"
    );
    let mount = format!(
        "[Unit]\n\
         Description=Silica development share from the host\n\
         Before=greetd.service\n\
         [Service]\n\
         Type=oneshot\n\
         RemainAfterExit=yes\n\
         ExecStart=/usr/bin/mkdir -p {GUEST_SHARE}\n\
         ExecStart=/usr/bin/mount -t 9p -o trans=virtio,version=9p2000.L,ro,msize=1048576,cache=none,context=system_u:object_r:bin_t:s0 silica-dev {GUEST_SHARE}\n"
    );
    let dropin =
        "[Unit]\nWants=silica-dev-mount.service sshd.service\nAfter=silica-dev-mount.service\n";
    vec![
        ("ssh.authorized_keys.root".to_owned(), format!("{pubkey}\n")),
        ("tmpfiles.extra".to_owned(), tmpfiles),
        ("tmpfiles.silica-dev-env".to_owned(), env),
        (
            "systemd.extra-unit.silica-dev-mount.service".to_owned(),
            mount,
        ),
        (
            "systemd.unit-dropin.greetd.service~silica-dev".to_owned(),
            dropin.to_owned(),
        ),
    ]
}

fn ssh_command(work: &Path) -> Command {
    let mut ssh = Command::new("ssh");
    ssh.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "LogLevel=ERROR",
        "-o",
        "ConnectTimeout=5",
        "-i",
    ])
    .arg(work.join("id_ed25519"))
    .args(["-p", &SSH_PORT.to_string()]);
    ssh
}

fn wait_ssh(work: &Path, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    let deadline = started + timeout;
    loop {
        let mut probe = ssh_command(work);
        probe
            .args(["root@127.0.0.1", "true"])
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        if probe.status().is_ok_and(|s| s.success()) {
            return Ok(());
        }
        // QEMU writes its pid file a moment after `setsid -f` returns.
        if started.elapsed() > Duration::from_secs(15) && !running(work) {
            let log = std::fs::read_to_string(work.join("qemu.log")).unwrap_or_default();
            return Err(format!("QEMU stopped: {}", log.trim()));
        }
        if Instant::now() > deadline {
            return Err(format!(
                "no SSH after {}s: see {}",
                timeout.as_secs(),
                work.join("console.log").display()
            ));
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// The SSH tunnel to Ossido's control channel (a master connection in the work directory).
fn tunnel(work: &Path) -> Result<()> {
    let socket = work.join("ssh.ctl");
    let mut check = ssh_command(work);
    check
        .arg("-S")
        .arg(&socket)
        .args(["-O", "check", "root@127.0.0.1"])
        .stderr(Stdio::null());
    if check.status().is_ok_and(|s| s.success()) {
        return Ok(());
    }
    let _ = std::fs::remove_file(&socket);
    let mut ssh = ssh_command(work);
    ssh.args(["-o", "ExitOnForwardFailure=yes", "-M", "-S"])
        .arg(&socket)
        .args(["-fN", "-L"])
        .arg(format!(
            "127.0.0.1:{CONTROL_PORT}:127.0.0.1:{GUEST_CONTROL}"
        ))
        .arg("root@127.0.0.1")
        .stdin(Stdio::null());
    status(ssh)
}

fn control_address() -> String {
    format!("127.0.0.1:{CONTROL_PORT}")
}

/// Waits until Ossido answers on the control channel.
fn wait_control(timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(mut client) = Client::connect(control_address())
            && client.call(PERF_FRAMES, serde_json::json!({})).is_ok()
        {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!(
                "Ossido does not answer on {} after {}s: cargo xtask os log",
                control_address(),
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// The QEMU of the work directory is alive.
fn running(work: &Path) -> bool {
    std::fs::read_to_string(work.join("qemu.pid"))
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok())
        .is_some_and(|pid| Path::new(&format!("/proc/{pid}")).exists())
}

fn require_running() -> Result<PathBuf> {
    let work = work_dir()?;
    if !running(&work) {
        return Err("the VM is not running: cargo xtask os start".to_owned());
    }
    tunnel(&work)?;
    Ok(work)
}

fn shot(path: &Path) -> Result<()> {
    require_running()?;
    let mut client =
        Client::connect(control_address()).map_err(|e| format!("control channel: {e}"))?;
    let result = client
        .call(UI_SCREENSHOT, serde_json::json!({}))
        .map_err(|e| format!("ui.screenshot: {e}"))?;
    let png = result
        .get("png_base64")
        .and_then(|value| value.as_str())
        .ok_or("ui.screenshot without png_base64")?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(png)
        .map_err(|e| format!("screenshot: {e}"))?;
    write_bytes(path, &png)?;
    eprintln!("xtask: {}", path.display());
    Ok(())
}

fn write_bytes(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}

/// `silica-cli` against the VM's control channel.
fn cmd(args: &[String]) -> Result<()> {
    require_running()?;
    let cli = target_dir()?.join("release/silica-cli");
    if !cli.exists() {
        status(cargo_command(&[
            "build",
            "--release",
            "--package",
            "silica-cli",
        ]))?;
    }
    let mut command = Command::new(cli);
    command.arg("--control").arg(control_address()).args(args);
    status(command)
}

fn ssh_interactive(args: &[String]) -> Result<()> {
    let work = require_running()?;
    let mut ssh = ssh_command(&work);
    ssh.arg("root@127.0.0.1").args(args);
    status(ssh)
}

/// Runs `script` as root in the guest and returns its output.
fn ssh_output(script: &str) -> Result<String> {
    let work = require_running()?;
    let mut ssh = ssh_command(&work);
    ssh.arg("root@127.0.0.1").arg(script);
    let output = ssh.output().map_err(|e| format!("ssh: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "in the guest: {script}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn status_report() -> Result<()> {
    let work = require_running()?;
    let report = ssh_output(
        "cat /etc/os-release | grep -E '^(PRETTY_NAME|IMAGE_VERSION)='; \
         findmnt -n -o TARGET,FSTYPE /run/silica-dev || echo 'no share mounted'; \
         ls /run/silica-dev 2>/dev/null | tr '\\n' ' '; echo; \
         pgrep -a ossido | cut -c1-120",
    )?;
    print!("{report}");
    println!(
        "work {}, control {}, ssh port {SSH_PORT}, monitor {MONITOR_PORT}",
        work.display(),
        control_address()
    );
    Ok(())
}

fn stop() -> Result<()> {
    let work = work_dir()?;
    let mut exit = ssh_command(&work);
    exit.arg("-S")
        .arg(work.join("ssh.ctl"))
        .args(["-O", "exit", "root@127.0.0.1"])
        .stderr(Stdio::null());
    let _ = exit.status();
    let Some(pid) = std::fs::read_to_string(work.join("qemu.pid"))
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok())
    else {
        return Err("the VM is not running".to_owned());
    };
    // A clean shutdown first: the overlay disk keeps the guest's state.
    let _ = ssh_output("systemctl poweroff");
    let deadline = Instant::now() + Duration::from_secs(60);
    while Path::new(&format!("/proc/{pid}")).exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(1));
    }
    if Path::new(&format!("/proc/{pid}")).exists() {
        let mut kill = Command::new("kill");
        kill.arg(pid.to_string());
        status(kill)?;
    }
    let _ = std::fs::remove_file(work.join("qemu.pid"));
    eprintln!("xtask: the VM is stopped");
    Ok(())
}

/// The cargo target directory (`CARGO_TARGET_DIR` or `target/`).
pub(crate) fn target_dir() -> Result<PathBuf> {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(root()?.join("target")),
    }
}

/// The host's glibc version (`ldd --version`).
fn host_glibc() -> Option<(u32, u32)> {
    let output = Command::new("ldd").arg("--version").output().ok()?;
    parse_glibc(&String::from_utf8_lossy(&output.stdout))
}

fn parse_glibc(text: &str) -> Option<(u32, u32)> {
    let version = text.lines().next()?.split_whitespace().last()?;
    let (major, minor) = version.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

struct DeployOptions {
    /// The apps to deploy: none, all of [`APPS`] (`--apps`) or some (`--apps=terminal,clock`).
    apps: Vec<&'static str>,
    build: bool,
}

/// The apps named in `--apps=a,b`: an app is named by its directory (`mail` for `mail/app`).
fn select_apps(list: &str) -> Result<Vec<&'static str>> {
    list.split(',')
        .filter(|name| !name.is_empty())
        .map(|name| {
            APPS.iter()
                .copied()
                .find(|app| app.split('/').next() == Some(name))
                .ok_or_else(|| format!("unknown app: {name} (known: {})", APPS.join(", ")))
        })
        .collect()
}

fn deploy(args: &[String]) -> Result<()> {
    let mut options = DeployOptions {
        apps: Vec::new(),
        build: true,
    };
    for arg in args {
        match arg.as_str() {
            "--apps" => options.apps = APPS.to_vec(),
            other if other.starts_with("--apps=") => {
                options.apps = select_apps(&other["--apps=".len()..])?;
            }
            "--no-build" => options.build = false,
            other => return Err(format!("unknown option: {other}")),
        }
    }
    let work = require_running()?;
    if let Some(glibc) = host_glibc()
        && glibc > GUEST_GLIBC
    {
        return Err(format!(
            "the host's glibc {}.{} is newer than the guest's {}.{}: the binaries would not run there",
            glibc.0, glibc.1, GUEST_GLIBC.0, GUEST_GLIBC.1
        ));
    }
    let root = root()?;
    let apps: Vec<PathBuf> = options
        .apps
        .iter()
        .filter_map(|app| sibling(app))
        .filter(|dir| dir.join("pubspec.yaml").exists())
        .collect();
    if options.build {
        status(cargo_command(&[
            "build",
            "--release",
            "--package",
            "ossido",
            "--package",
            "silica-embedder",
            "--package",
            "silica-daemon",
            "--package",
            "silica-portal",
            "--package",
            "silica-cli",
        ]))?;
        flutter::bundle_release("shell")?;
        for app in &apps {
            flutter::bundle_release(&app.display().to_string())?;
        }
    }

    let share = work.join("share");
    let release = target_dir()?.join("release");
    for binary in [
        "ossido",
        "silica-embedder",
        "silica-daemon",
        "silica-portal",
        "silica-cli",
        "silica-action",
    ] {
        replace_file(&release.join(binary), &share.join("bin").join(binary))?;
    }
    // The shell's own applications (Apps, Screenshot).
    for (name, entry) in
        crate::install::shell_entries(&root, &format!("{GUEST_RUN}/bin/silica-action"))?
    {
        write(&share.join("share/applications").join(name), &entry)?;
    }
    let engine = flutter::release_engine_dir()?;
    for file in ["libflutter_engine.so", "icudtl.dat"] {
        replace_file(&engine.join(file), &share.join("engine").join(file))?;
    }
    // The patches the engine was built with (Impeller's Vulkan backing stores, #203): an
    // engine built before them has no list, and the share must not keep an old one.
    let patches = share.join("engine").join(flutter::PATCH_LIST);
    if engine.join(flutter::PATCH_LIST).exists() {
        replace_file(&engine.join(flutter::PATCH_LIST), &patches)?;
    } else {
        let _ = std::fs::remove_file(&patches);
    }
    replace_dir(&root.join("shell/build/release"), &share.join("shell"))?;
    // The interface sounds, in $XDG_DATA_DIRS/silica/sounds (the session puts share/ first).
    replace_dir(
        &root.join("data/sounds"),
        &share.join("share/silica/sounds"),
    )?;
    // The compiled system assets (docs/assets-format.md), the same file for every process.
    let compiled = if options.build || !crate::silica_assets::default_output()?.is_file() {
        crate::silica_assets::build(&[])?
    } else {
        crate::silica_assets::default_output()?
    };
    replace_file(&compiled, &share.join("silica.assets"))?;
    if let Some(icons) = sibling("silica-icons") {
        let staged = work.join("icons.stage");
        let _ = std::fs::remove_dir_all(&staged);
        for group in ["apps", "badges", "files", "symbols"] {
            copy_tree(&icons.join(group), &staged.join(group))?;
        }
        swap_dir(&staged, &share.join("icons"))?;
    }
    for app in &apps {
        deploy_app(app, &share)?;
    }
    write_executable(&share.join("bin/silica-app"), &silica_app_script())?;
    write_executable(&share.join("session.sh"), &session_script())?;

    sync_to_guest_disk()?;
    prepare_guest_bus(&share)?;

    eprintln!("xtask: restarting the session in the guest");
    restart_session()?;
    std::thread::sleep(Duration::from_secs(3));
    wait_control(Duration::from_secs(120))?;
    let check = ssh_output(&format!(
        "pgrep -a ossido | grep -c '{GUEST_RUN}/bin/ossido' || true"
    ))?;
    if check.trim() == "0" {
        return Err(
            "the session restarted from the image, not from the share: cargo xtask os log"
                .to_owned(),
        );
    }
    eprintln!("xtask: deployed: the session runs from {GUEST_RUN}");
    Ok(())
}

/// A repository next to this one (`../name`), or in `~/silica` when this checkout is a
/// worktree somewhere else.
pub(crate) fn sibling(name: &str) -> Option<PathBuf> {
    let next = root().ok()?.parent().map(|dir| dir.join(name));
    let home = std::env::var_os("HOME").map(|home| Path::new(&home).join("silica").join(name));
    next.into_iter().chain(home).find(|dir| dir.is_dir())
}

/// Makes `GUEST_RUN` match the share, on the guest's disk, copying only what changed (size or
/// modification time) and removing what the share no longer has: an incremental copy like
/// `rsync -a --delete`, which the image does not ship. Runs in the guest, reading the share once
/// (sequential reads over 9p are fast; the many small random requests of a launch are not).
/// A file in use is unlinked before the new one is written, so running programs keep theirs.
fn sync_to_guest_disk() -> Result<()> {
    let script = format!(
        r#"set -e
export LC_ALL=C
S={GUEST_SHARE}; L={GUEST_RUN}
install -d "$L"
T=$(mktemp -d)
list() {{ (cd "$1" && find . \( -type f -o -type l \) -printf '%p\t%s\t%T@\t%l\n' | sort); }}
list "$S" > "$T/src"
list "$L" > "$T/dst"
comm -23 "$T/src" "$T/dst" | cut -f1 > "$T/copy"
comm -13 <(cut -f1 "$T/src" | sort) <(cut -f1 "$T/dst" | sort) > "$T/stale"
(cd "$L" && xargs -r -d '\n' rm -f -- < "$T/stale")
(cd "$S" && xargs -r -d '\n' cp -a --parents --remove-destination -t "$L" -- < "$T/copy")
find "$L" -mindepth 1 -depth -type d -empty -delete
# The share is mounted as bin_t: the copy gets the same label.
chcon -R -t bin_t "$L" 2>/dev/null || true
echo "$(wc -l < "$T/copy") files copied, $(wc -l < "$T/stale") removed"
rm -rf "$T"
"#
    );
    eprint!("xtask: copying the share to {GUEST_RUN} in the guest: ");
    let out = ssh_output(&script)?;
    eprintln!("{}", out.trim());
    Ok(())
}

/// The D-Bus services of the image that the share replaces: (bus name, binary in the share).
const DEV_SERVICES: [(&str, &str); 3] = [
    ("dev.frascella.Silica", "silica-daemon"),
    ("dev.frascella.Mail", "silica-mail-engine"),
    // The Open and Save dialogs: the share's portal starts the share's Mustache as it expects.
    (
        "org.freedesktop.impl.portal.desktop.silica",
        "silica-portal",
    ),
];

/// The user unit that brings graphical-session.target up (xdg-desktop-portal needs it).
const SESSION_TARGET: &str = include_str!("../../data/systemd/silica-session.target");

/// The user-level activation file that points `name` at the share's binary. The image ships
/// /usr/share/dbus-1/services/<name>.service with the packaged binary: without this override,
/// activating the name while the dev daemon does not own it (a session restart) starts an old
/// daemon from /usr/libexec/silica.
fn dev_service_file(name: &str, binary: &str) -> String {
    format!("[D-BUS Service]\nName={name}\nExec={GUEST_RUN}/bin/{binary}\n")
}

/// Makes the guest's bus activate the share's services instead of the packaged ones, and stops
/// the packaged daemons that are already running. Everything runs inside the guest (over ssh):
/// the PIDs are the guest's, never the host's.
fn prepare_guest_bus(share: &Path) -> Result<()> {
    let home = format!("/var/home/{GUEST_USER}");
    let dir = format!("{home}/.local/share/dbus-1/services");
    let mut script = format!(
        "install -d -o {GUEST_USER} -g {GUEST_USER} {home}/.local {home}/.local/share {home}/.local/share/dbus-1 {dir}\n"
    );
    let mut stale = Vec::new();
    for (name, binary) in DEV_SERVICES {
        let file = format!("{dir}/{name}.service");
        if share.join("bin").join(binary).exists() {
            script.push_str(&format!(
                "printf '%s' '{}' > {file} && chown {GUEST_USER}:{GUEST_USER} {file}\n",
                dev_service_file(name, binary)
            ));
            stale.push(format!("/usr/libexec/silica/{binary}"));
            // The share's own copy from an earlier deploy runs the old binary until it exits:
            // stopped, the bus activates the new one at the next call. The daemon is left
            // alone: the session restarts it with --replace.
            if binary != "silica-daemon" {
                stale.push(format!("{GUEST_RUN}/bin/{binary}"));
            }
        } else {
            // Nothing to point at: an override left by an earlier deploy would be dangling.
            script.push_str(&format!("rm -f {file}\n"));
        }
    }
    script.push_str(&format!(
        "restorecon -R {home}/.local/share/dbus-1 2>/dev/null || true\n"
    ));
    // The session target: the image may not ship it yet. User units under /etc are found by
    // every user's systemd.
    script.push_str(&format!(
        "install -d /etc/systemd/user\nprintf '%s' '{SESSION_TARGET}' > /etc/systemd/user/silica-session.target\n\
         restorecon /etc/systemd/user/silica-session.target 2>/dev/null || true\n"
    ));
    // The user's bus survives the session: tell it to read the new files.
    script.push_str(&format!(
        "uid=$(id -u {GUEST_USER}); \
         runuser -u {GUEST_USER} -- env XDG_RUNTIME_DIR=/run/user/$uid \
         DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus \
         busctl --user call org.freedesktop.DBus / org.freedesktop.DBus ReloadConfig || true\n\
         runuser -u {GUEST_USER} -- env XDG_RUNTIME_DIR=/run/user/$uid \
         DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus \
         systemctl --user daemon-reload || true\n"
    ));
    // Stale daemons still running (exact command line, in the guest's process table).
    for path in stale {
        script.push_str(&format!(
            "pids=$(pgrep -x -f '{path}' || true); [ -z \"$pids\" ] || kill $pids || true\n"
        ));
    }
    ssh_output(&script).map(|_| ())
}

/// Restarts the automatic login session: greetd runs `initial_session` only once per boot
/// (its run file records it), so the run file goes first.
fn restart_session() -> Result<()> {
    ssh_output("rm -f /run/greetd.run && systemctl restart greetd").map(|_| ())
}

/// An app's release bundle and its `.desktop` entry, pointing at the share.
fn deploy_app(app: &Path, share: &Path) -> Result<()> {
    let templates: Vec<PathBuf> = std::fs::read_dir(app.join("linux"))
        .map_err(|e| format!("{}/linux: {e}", app.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.to_string_lossy().ends_with(".desktop.in"))
        .collect();
    for template in templates {
        let name = template
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".in"))
            .ok_or_else(|| format!("{}: invalid name", template.display()))?;
        let id = name.strip_suffix(".desktop").unwrap_or(name);
        // The app's icon folder travels with the bundle; the entry points at the guest copy.
        let guest_icon =
            crate::app_icon::bundle_icon(app, &app.join("build/release"))?.map(|folder| {
                (
                    format!("{GUEST_RUN}/apps/{id}/icon"),
                    crate::app_icon::png_name(&folder),
                )
            });
        replace_dir(&app.join("build/release"), &share.join("apps").join(id))?;
        let text = std::fs::read_to_string(&template)
            .map_err(|e| format!("{}: {e}", template.display()))?;
        let entry = crate::install::render_entry(
            &text,
            &format!("{GUEST_RUN}/bin/silica-app"),
            &format!("{GUEST_RUN}/apps/{id}/flutter_assets"),
            guest_icon
                .as_ref()
                .map(|(folder, png)| (folder.as_str(), png.as_str())),
        );
        write(&share.join("share/applications").join(name), &entry)?;
        eprintln!("xtask: {id}");
    }
    Ok(())
}

/// Replaces `to` with a copy of `from` (a new file renamed over the old one: running programs
/// keep theirs).
fn replace_file(from: &Path, to: &Path) -> Result<()> {
    let dir = to.parent().ok_or("no parent directory")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let staged = to.with_extension("new");
    std::fs::copy(from, &staged)
        .map_err(|e| format!("{} → {}: {e}", from.display(), staged.display()))?;
    // The same modification time as the source: an unchanged binary is not copied again into
    // the guest's disk (`sync_to_guest_disk`).
    if let Ok(modified) = std::fs::metadata(from).and_then(|m| m.modified()) {
        let _ = std::fs::File::options()
            .write(true)
            .open(&staged)
            .and_then(|file| file.set_modified(modified));
    }
    std::fs::rename(&staged, to).map_err(|e| format!("{}: {e}", to.display()))
}

/// Replaces the directory `to` with a copy of `from`.
fn replace_dir(from: &Path, to: &Path) -> Result<()> {
    let staged = to.with_extension("new");
    let _ = std::fs::remove_dir_all(&staged);
    copy_tree(from, &staged)?;
    swap_dir(&staged, to)
}

fn swap_dir(staged: &Path, to: &Path) -> Result<()> {
    let old = to.with_extension("old");
    let _ = std::fs::remove_dir_all(&old);
    if to.exists() {
        std::fs::rename(to, &old).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    std::fs::rename(staged, to).map_err(|e| format!("{}: {e}", to.display()))?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let mut cp = Command::new("cp");
    // -p keeps the modification times: `sync_to_guest_disk` copies only what differs.
    cp.arg("-rLp").arg(from).arg(to);
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let output = cp.output().map_err(|e| format!("cp: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cp {} {}: {}",
            from.display(),
            to.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn write_executable(path: &Path, text: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let staged = path.with_extension("new");
    write(&staged, text)?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("{}: {e}", staged.display()))?;
    std::fs::rename(&staged, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The embedder with the share's engine (what silica-packages installs as `silica-app`).
fn silica_app_script() -> String {
    format!(
        "#!/bin/sh\n\
         # Generated by `cargo xtask os deploy`: the embedder of the share with its engine.\n\
         exec {GUEST_RUN}/bin/silica-embedder --engine {GUEST_RUN}/engine \\\n\
         \x20 --renderer \"${{SILICA_RENDERER:-vulkan}}\" \"$@\"\n"
    )
}

/// The session of the share: silica-session (silica-packages) with the share's paths. It is
/// started by the hook in 50-dev.env, after the environment files were read.
fn session_script() -> String {
    let components = crate::shell_component_args(
        &format!("{GUEST_RUN}/bin/silica-app"),
        &format!("{GUEST_RUN}/shell/flutter_assets"),
    );
    let desktop = crate::desktop_component_args(
        &format!("{GUEST_RUN}/bin/silica-app"),
        &format!("{GUEST_RUN}/apps/dev.frascella.Mustache/flutter_assets"),
    );
    let spawn: String = components
        .chunks(2)
        .filter_map(|pair| match pair {
            [flag, command] => Some(format!("  {flag} '{command}' \\\n")),
            _ => None,
        })
        .collect();
    let [desktop_flag, desktop_command] = desktop.as_slice() else {
        return String::new();
    };
    format!(
        r#"#!/bin/sh
# Generated by `cargo xtask os deploy`: the Silica session from the host's share
# ({GUEST_SHARE}, copied to {GUEST_RUN} on the guest's disk), started by
# /etc/silica/session.env.d/50-dev.env instead of the installed one. Same structure as
# silica-session (silica-packages).
D={GUEST_RUN}
# No copy yet (deploy syncs it before the restart): run from the share itself.
[ -x "$D/bin/ossido" ] || D={GUEST_SHARE}
exec >>"${{XDG_RUNTIME_DIR:-/tmp}}/silica-session.log" 2>&1
echo "silica-session (dev): $(date)"
export SILICA_RENDERER="${{SILICA_RENDERER:-vulkan}}"
export XDG_CURRENT_DESKTOP=Silica XDG_SESSION_DESKTOP=silica XDG_SESSION_TYPE=wayland
export PATH="$D/bin:/usr/libexec/silica:$PATH"
# The share's .desktop entries (deployed apps) come before the installed ones.
export XDG_DATA_DIRS="$D/share:${{XDG_DATA_DIRS:-/usr/local/share:/usr/share}}"
[ -d "$D/icons" ] && export SILICA_ICONS_DIR="$D/icons"
[ -f "$D/silica.assets" ] && export SILICA_ASSETS="$D/silica.assets"
# Pointer cursors: the standard Adwaita XCursor theme, for Ossido and for the apps (GTK, Qt,
# the embedder).
export XCURSOR_THEME=Adwaita XCURSOR_SIZE=24
# Reads the engine and the apps once, in the background, so that the first launch of each does
# not wait for the disk (a cold launch takes 1 to 2 s more than a warm one).
(nice -n 19 find "$D/engine" "$D/apps" "$D/bin" -type f -exec cat {{}} + >/dev/null 2>&1 &)

set -- \
{spawn}
if [ -n "$DBUS_SESSION_BUS_ADDRESS" ]; then
  set -- "$@" \
    --spawn "dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_DESKTOP XDG_SESSION_TYPE XDG_DATA_DIRS XCURSOR_THEME XCURSOR_SIZE SILICA_ICONS_DIR SILICA_ASSETS SILICA_RENDERER PATH && systemctl --user start silica-session.target" \
    --spawn "silica-daemon --replace"
else
  set -- --session-bus "$@"
fi
# The file manager's desktop (#147), when Mustache is deployed.
if [ -d "$D/apps/dev.frascella.Mustache/flutter_assets" ]; then
  set -- "$@" {desktop_flag} '{desktop_command}'
fi
# shellcheck disable=SC2086
"$D/bin/ossido" $OSSIDO_ARGS "$@"
status=$?
# graphical-session.target (xdg-desktop-portal requires it) goes down with the session.
[ -n "$DBUS_SESSION_BUS_ADDRESS" ] && systemctl --user stop silica-session.target
exit $status
"#
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn select_apps_by_directory() {
        assert_eq!(
            super::select_apps("terminal,mail"),
            Ok(vec!["terminal", "mail/app"])
        );
        assert!(super::select_apps("nope").is_err());
    }

    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1920x1200"), Some((1920, 1200)));
        assert_eq!(parse_size("100x100"), None);
        assert_eq!(parse_size("1920"), None);
    }

    #[test]
    fn glibc_from_ldd() {
        let text = "ldd (Ubuntu GLIBC 2.43-2ubuntu2.4) 2.43\nCopyright";
        assert_eq!(parse_glibc(text), Some((2, 43)));
        assert!(parse_glibc(text) <= Some(GUEST_GLIBC));
    }

    #[test]
    fn start_defaults_to_the_gpu() {
        let options = parse_start(&[]).expect("defaults");
        assert_eq!(options.graphics, Graphics::Venus);
        assert!(!options.headless);
        let software = parse_start(&["--software".to_owned()]).expect("software");
        assert_eq!(software.graphics.renderer(), "software");
    }

    #[test]
    fn credentials_hook_the_session() {
        let credentials = credentials("ssh-ed25519 AAAA test", "impeller");
        let env = credentials
            .iter()
            .find(|(name, _)| name == "tmpfiles.silica-dev-env")
            .map(|(_, text)| text.as_str())
            .unwrap_or_default();
        assert!(env.contains("--control 7991"));
        assert!(env.contains("SILICA_RENDERER=impeller"));
        assert!(env.contains("exec /run/silica-dev/session.sh"));
        assert!(
            credentials
                .iter()
                .any(|(name, _)| name == "ssh.authorized_keys.root")
        );
    }

    #[test]
    fn dev_services_override_the_packaged_ones() {
        let file = dev_service_file("dev.frascella.Silica", "silica-daemon");
        assert_eq!(
            file,
            "[D-BUS Service]\nName=dev.frascella.Silica\nExec=/var/lib/silica-dev/bin/silica-daemon\n"
        );
        assert!(!file.contains("/usr/libexec"));
    }

    #[test]
    fn session_runs_from_the_local_copy() {
        let script = session_script();
        assert!(script.contains("\n\"$D/bin/ossido\" $OSSIDO_ARGS"));
        assert!(script.contains("systemctl --user start silica-session.target"));
        assert!(script.contains("systemctl --user stop silica-session.target"));
        // The whole shell in one kept process (#201): one view per layer.
        assert!(
            script.contains("--keep-shell '/var/lib/silica-dev/bin/silica-app --layer background")
        );
        assert!(script.contains("--namespace menubar --view --layer top"));
        assert!(script.contains(
            "--namespace controlcenter --dart-arg shell /var/lib/silica-dev/shell/flutter_assets"
        ));
        assert_eq!(script.matches("--namespace ").count(), 8);
        assert!(!script.contains("--spawn-shell"));
        assert!(script.contains("SILICA_ICONS_DIR"));
        assert!(script.contains("export SILICA_ASSETS=\"$D/silica.assets\""));
        // Before the first copy it falls back to the share.
        assert!(script.contains("D=/var/lib/silica-dev"));
        assert!(script.contains("|| D=/run/silica-dev"));
        // The file manager's desktop, kept running, when Mustache is deployed (#147).
        assert!(script.contains("--keep-shell '/var/lib/silica-dev/bin/silica-app --layer bottom"));
        assert!(script.contains(
            "--namespace desktop-icons:dev.frascella.Mustache --dart-arg --desktop /var/lib/silica-dev/apps/dev.frascella.Mustache/flutter_assets"
        ));
    }
}
