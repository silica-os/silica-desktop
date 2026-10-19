# The real session in a virtual machine

`cargo xtask vm` runs Silica the way a real session does, without a host compositor: Ossido
on DRM/KMS with libinput and libseat (`--backend drm`, `crates/ossido/src/drm.rs`), the
shell on top, inside a QEMU/KVM virtual machine. It is the first step of M9 (real session,
in a VM first) and runs in CI on every push (`.github/workflows/vm.yml`).

## What it does

1. Builds Ossido, the embedder and `silica-daemon` in release, downloads the debug Flutter
   engine and builds the shell's debug bundle (no engine build from source is needed: the
   debug engine runs the JIT bundle).
2. Packs them with `run-session.sh` into `target/vm/payload.tar`.
3. Downloads the Ubuntu cloud image of the **same release as the host**
   (`VERSION_CODENAME` in `/etc/os-release`, or `SILICA_VM_RELEASE`): the binaries are built
   on the host and need the same glibc. The image is kept in `target/vm/` and the guest boots
   from a throwaway qcow2 overlay.
4. Serves the cloud-init configuration (NoCloud, `-smbios type=1,serial=ds=nocloud;s=…`) and
   the tarball over HTTP from xtask itself: QEMU's user network reaches the host's loopback at
   `10.0.2.2`. No ISO tools or 9p shares are needed.
5. The guest installs the runtime libraries (seatd, Mesa, libinput, libseat, …), unpacks the
   session into `/opt/silica` and starts `ossido --backend drm` on the guest's seatd service with the shell
   components (embedder with `--renderer software`).
6. Ossido's control channel listens on the guest's loopback (7991); socat exposes it on 7990,
   which QEMU forwards to the host (default 7990, never 7979/7980, used by the sessions on the
   host).
7. xtask waits for the channel, checks that the serial console shows `drm backend ready`
   (Ossido really runs on DRM), takes `ui.screenshot` and fails if the frame is black.
   The screenshot and the serial console are in `target/vm/` (`screenshot.png`,
   `console.log`).

Choice of image: an Ubuntu cloud image with cloud-init, rather than mkosi or debootstrap,
because it needs no root on the host (mkosi and debootstrap do), it is a single download, and
its release can match the build host, which CI's Ubuntu runner and the development machines
already are. A Fedora image is the natural next step with the D-track packages
(`silica-packages`), when the session is installed from RPMs instead of a tarball.

The GPU is `virtio-vga` without 3D: in the guest Mesa renders with llvmpipe into GBM dumb
buffers (`kms_swrast`), slow but deterministic, like headless CI.

## Options

```text
cargo xtask vm [--port N] [--timeout SECONDS] [--out PATH] [--show] [--keep] [--payload-only]
```

- `--show`: a GTK window with the guest's screen, to watch or use the session (keyboard and
  tablet are virtio devices, read by libinput).
- `--keep`: leaves the VM running after the check.
- `--payload-only`: only builds `payload.tar` (no QEMU needed).
- `--timeout`: seconds to wait for the control channel (default 900: the first boot installs
  packages).

## Host requirements

- `qemu-system-x86_64` and `qemu-img`: on Ubuntu `sudo apt install qemu-system-x86 qemu-utils`.
- KVM: `/dev/kvm` readable and writable by the user, on Ubuntu
  `sudo usermod -aG kvm $USER` and a new login. Without KVM the VM is emulated (TCG), many
  times slower.
- Network access for the image download and the guest's packages.
- To build Ossido with the `drm` feature (default): `libudev-dev libinput-dev libseat-dev
  libgbm-dev` (already in CI's dependencies).

## Development loop: `cargo xtask os`

All development and testing happens inside the real Silica OS, not only in the nested session
on the host: build here, check in the VM, then push to CI. `cargo xtask os` runs a released
Silica OS disk (the qcow2 of a silica-os release, in `~/silica-vm/`) under QEMU/KVM, in a
window on the user's screen, so the work can be watched while it happens. Keep one VM running
for the whole session: `deploy` restarts the Silica session inside it.

```bash
gh release download -R silica-os/silica-os -p '*.qcow2' -D ~/silica-vm   # once
cargo xtask os start          # the VM in a full-screen window ("Silica OS (dev)")
cargo xtask os deploy         # build Ossido, the embedder, the services and the shell, run them
cargo xtask os deploy --apps  # the apps too (mustache, mail, terminal, clock, calculator, settings)
cargo xtask os deploy --toolkit=vetro  # the native shell (crates/shell on Vetro) instead of the Flutter one
cargo xtask os deploy --apps=terminal,mail  # only some of them
cargo xtask os shot out.png   # screenshot through Ossido's control channel
cargo xtask os cmd window.list            # any silica-cli command
cargo xtask os ssh 'journalctl -b -p warning'   # a root shell in the guest
cargo xtask os log            # the deployed session's log (Ossido, the embedders)
cargo xtask os restart        # restart the session (automatic login again)
cargo xtask os status | stop
```

`start` options: `--image PATH`, `--fresh` (a new overlay disk), `--size WxH` (default
1920x1200), `--headless` (no window, for CI), and the graphics:

| Option | Device | Ossido | Shell and apps |
| --- | --- | --- | --- |
| (default) | `virtio-vga-gl` with virgl and Venus | GLES on the host's GPU | Impeller on Vulkan (Venus) |
| `--virgl` | `virtio-vga-gl` with virgl | GLES on the host's GPU | Impeller on GLES |
| `--software` | `virtio-vga` | llvmpipe | software |

How it works, without root on the host and without changing the image:

- **Credentials.** systemd credentials passed through SMBIOS (`-smbios type=11`) give the
  guest root's SSH key, `/etc/silica/session.env.d/50-dev.env` (Ossido's control channel on
  the guest's loopback, the embedder's renderer, `RUST_LOG`) and a service that mounts the
  share. Nothing persists in the image: the overlay disk holds the guest's state.
- **Ports.** SSH on 127.0.0.1:7998 (guest 22), Ossido's control channel on 127.0.0.1:7993
  through an SSH tunnel (guest 7991: SELinux keeps socket proxies from binding a port),
  QEMU's monitor on 127.0.0.1:7996. Never 7979/7980, the sessions on the host.
- **The share.** `~/build/silica-os-dev/share` is mounted read-only with 9p on
  `/run/silica-dev` (staging; the session runs from its copy): `bin/` (Ossido, embedder, services, CLI), `engine/`, `shell/`, `apps/`,
  `share/applications/` (the apps' `.desktop` entries, before the installed ones), `icons/`
  (silica-icons) and `session.sh`. The hook in `50-dev.env` runs `session.sh` instead of the
  installed session when it exists; silica-session also has `SILICA_PREFIX` for the same.
- **D-Bus activation.** The image ships activation files in `/usr/share/dbus-1/services`
  (`dev.frascella.Silica`, `dev.frascella.Mail`) that start the packaged daemons from
  `/usr/libexec/silica`. When the bus name is requested while the dev daemon does not own it
  (a session restart), that would start an old daemon, and calls such as `SetDark` would reach
  the wrong one. `deploy` writes user-level overrides in
  `~silica/.local/share/dbus-1/services/` (they take precedence) with
  `Exec=/run/silica-dev/bin/silica-daemon` (and the mail engine, when the share has it), owned
  by `silica`, reloads the user bus (`busctl --user call org.freedesktop.DBus /
  org.freedesktop.DBus ReloadConfig`) and kills the packaged daemons still running, found by
  exact command line with `pgrep -x -f` inside the guest only.
- **Local copy.** `deploy` then copies the share onto the guest's disk, `/var/lib/silica-dev`
  (incrementally: only files whose size or modification time changed, and it removes the ones
  the share no longer has, like `rsync -a --delete`), and the session, the `.desktop` entries
  and the D-Bus overrides run from there. The share stays the staging area (and the fallback
  of `session.sh` before the first copy). See "Launch times".
- **Binaries.** Built on the host: its glibc must not be newer than Fedora 44's (2.43),
  which `deploy` checks.
- **Restart.** greetd logs in automatically only once per boot (its run file in `/run`):
  `deploy` and `restart` remove it and restart greetd.
- **The window.** QEMU runs with `GDK_BACKEND=x11`; with `wmctrl` installed `start` moves it
  to the second workspace.

The work directory is `~/build/silica-os-dev` (`SILICA_OS_DIR`).

## Looking at animations and frames

- `SILICA_TIME_DILATION=10` in the shell's environment slows every animation of the shell
  tenfold (Flutter's `timeDilation`), to take screenshots through a transition.
- `RUST_LOG=ossido::handlers=trace,ossido::layers=trace` logs every commit of every surface:
  the layer namespace, whether it carries a new buffer and its glass regions (count and
  highest opacity), and the layers whose buffer does not have the configured size (not
  drawn yet). A glass change on a commit without a new buffer is a frame early or late: the
  embedder commits the glass regions with the frame they were measured on (`GlassSync` in
  `crates/embedder/src/glass.rs`).

## Launch times

Measured in the dev VM with Impeller on Vulkan (Venus), from the launch by Ossido to the first
frame on screen (`perf.frames`, `startup_ms`), and in the embedder with
`SILICA_STARTUP_TRACE=1` (one line per phase on stderr: process start, Wayland, renderer,
engine library, AOT data, engine initialised, engine started, Dart's first frame request, first
present; the platform messages are listed too, which dates what the app does in `main`).

What decides the time:

1. **The VM window must be visible.** With the QEMU window on another workspace (iconic, not
   drawn) QEMU's main loop waits for the display, and everything that goes through it, the
   virtio-gpu commands of Venus and every 9p request, is served in steps of about one second.
   A launch then takes 2 to 15 s with stalls of whole seconds in random phases (Vulkan
   initialisation, the Dart thread in `p9_client_rpc`, the first present). With
   `cargo xtask os start --headless` (or the window in front) the same launches take 0.3 to
   1 s. Measure with `--headless`.
2. **9p.** The share is mounted without page cache: every launch reads the engine, `libapp.so`,
   the assets, the icons (`SILICA_ICONS_DIR`) and the `.desktop` entries again, with many small
   lookups (a Dart thread in `v9fs_vfs_lookup`). That is why `deploy` copies them to the disk.
3. **Cold disk.** After `echo 3 > /proc/sys/vm/drop_caches` the same launch takes 1 to 3 s.

4. **Vulkan drivers (#191).** The Khronos loader opens every installed driver at
   `vkCreateInstance`: in Silica OS a dozen Mesa drivers of 12 to 26 MB for hardware that is
   not there, 200 ms warm and 0.5 to 0.8 s from a cold disk. The embedder hands the loader only
   the drivers of the GPUs it finds under `/sys/class/drm` (`crates/embedder/src/vulkan_drivers.rs`,
   `VK_LUNARG_direct_driver_loading`); the loader's own variables (`VK_DRIVER_FILES`,
   `VK_LOADER_DRIVERS_SELECT`, ...) still choose when set.
5. **Venus on the host.** Every Vulkan instance in the guest is one on the host too, which
   initialises every host GPU (on a laptop with NVIDIA: the integrated GPU, the NVIDIA one and
   llvmpipe), 100 to 200 ms per launch. `start` gives QEMU only the drivers of the host's
   display GPU (`VK_DRIVER_FILES`, printed at start), the one the embedder picks anyway: the
   instance takes 10 to 15 ms. It applies from the next `start`.
6. **Fonts.** Before Dart runs, the engine opens every font under `/usr/share/fonts` (Skia's
   directory font manager, 126 files in Silica OS): 10 ms warm, 0.3 to 0.5 s from a cold disk.
   The shell components read them at login, so an app's first launch finds them in memory.

How to measure: launch through the control channel with `SILICA_STARTUP_TRACE=1` in the
command and read Ossido's `startup_ms` (`perf.frames`) for the window; "first" launches evict
only the app's own files (`posix_fadvise(DONTNEED)` on its bundle), "cold" ones drop every
cache. Headless private VM, host load below 2, medians (#191):

| | warm | first launch | cold disk |
| --- | --- | --- | --- |
| before (single-window apps, Vulkan) | 290-415 ms | 470-700 ms | 950-1395 ms |
| the embedder's drivers only | 270-335 ms | 210-440 ms | 850-1000 ms |
| and QEMU with the host's display GPU only | 120-180 ms | 125-170 ms | 500-830 ms |
| Mustache (several windows, OpenGL ES) | 115-160 ms | 120-160 ms | 570-740 ms |

Per phase after, warm (ms from process start): Wayland 1, Vulkan instance 10 to 15, device and
swapchain 35, engine library, AOT data and initialisation 5, engine started +10, Dart's first
frame request +15, first present +30 to 50, window mapped in Ossido +10 to 30. Settings logs its
own steps with `SETTINGS_STARTUP_LOG=1`.

## Renderers with several views (#203)

An app with several windows (`--multi-window`) and the shell's layers in one engine draw
through the engine's backing stores. With `--renderer vulkan` the backing store of a view is
the next image of that view's own swapchain, and the views of a frame are presented together
with one `vkQueuePresentKHR` when the raster task ends: no copy, the buffers reach Ossido as
dmabufs (`WAYLAND_DEBUG` shows `zwp_linux_buffer_params_v1.add` for every image, no `wl_shm`).
Impeller draws them with the engine patch `impeller-vulkan-backing-stores`
(`docs/engine-release.md`); an engine without it (the debug one) draws them with Skia on Vulkan.
`SILICA_MULTI_VIEW_RENDERER=gl|skia` forces the OpenGL ES path of before or Skia.

Measured on 2026-10-15/16 in the dev VM (Venus on Intel Iris Xe, 1920x1200) with
`testapps/framebench` (24 moving cards with shadows and text, 1000x700 window, second window
480x360), 600 frames after 120 of warm-up, release engine. Milliseconds, `FrameTiming`;
median of three runs (runs where the VM stalled for a second, a QEMU display artefact, are
left out of the median when another run is clean).

| Renderer | Views | Raster p50 | Raster p95 | Build + raster p50 | Frame interval p50 / p95 |
| --- | --- | --- | --- | --- | --- |
| Impeller, Vulkan, root surface (one view, before) | 1 | 3.5 | 13.0 | 6.8 | 11.6 / 17.7 |
| Impeller, OpenGL ES, backing stores (several views, before) | 1 | 1.5 | 9.7 | 4.2 | 10.3 / 17.2 |
| Impeller, OpenGL ES, backing stores | 2 | 3.8 | 11.2 | 8.2 | 13.3 / 18.6 |
| Skia, Vulkan, backing stores | 1 | 4.1 | 17.2 | 7.8 | 13.6 / 18.8 |
| Skia, Vulkan, backing stores | 2 | 4.7 | 10.6 | 8.0 | 9.3 / 17.4 |
| **Impeller, Vulkan, backing stores (now)** | 1 | **1.0** | **3.9** | **3.9** | 12.4 / 17.8 |
| **Impeller, Vulkan, backing stores (now)** | 2 | **1.8** | **9.4** | **7.0** | 13.9 / 19.0 |

With Vulkan backing stores the present runs after the raster task (it is not in the raster
time); with the root surface and with OpenGL ES it is. Presenting each view on its own made two
views wait twice for the GPU inside Venus's present (raster p50 15 ms with two windows) before
the presents were batched. Skia also stalls: in the first round
of measurements three of six Skia runs had stalls of about 1 s, none of the six
Impeller runs on Vulkan. Without the patch Skia keeps Vulkan and zero copy, but it loses the lens glass
refraction of silica_ui (shader image filters need Impeller).

## The shell in one engine (#201)

The session starts the shell as one embedder process, kept running by Ossido
(`--keep-shell`: it is started again if it crashes, as checked in the VM with `kill -9`): every
layer is a view of the same engine (`--view`), each still its own layer surface with its
layer, anchor, size, exclusive zone and keyboard mode. Dart builds one view per layer
(`ShellViews`), and a view is drawn again only when something in it changed: while Faro
animates, the wallpaper, menu bar, Dock and the other overlays present no frame. Mustache's
desktop icons stay their own process.

Measured in the dev VM on 2026-10-16, before (seven processes, the commit before #201) and
after, each three times after a session restart, back to back under one hold of the VM:

| | Shell processes | RSS | PSS |
| --- | --- | --- | --- |
| Before | 7 | 524–528 MiB | 173–176 MiB |
| After | 1 | 83–88 MiB | 53–58 MiB |

Login to first frame (Ossido's `startup_ms` of the layers, from the launch to the first
buffer): with the VM's window visible, before 0.85–0.97 s for the wallpaper, menu bar and
Dock (1.37 s until the last overlay drew its first, empty, frame), after 0.85–1.50 s (the
overlays draw nothing until they open). During the back-to-back runs the VM's window was not
visible and QEMU served the GPU in one-second steps (see "Launch times"): before 9.0–23.6 s,
median 17.6 s, after 9.9–17.6 s, median 10.0 s; those numbers say more about the stalls than
about the shell.
