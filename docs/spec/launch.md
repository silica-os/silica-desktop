# `silica-launch`: trust check and sandbox

Status: implemented (silica-os#3, silica-desktop#138), `crates/launch`. Formats: silica-os
`docs/app-bundles.md`; trust: `trust.md`; registry: `launch-services.md`.

```
silica-launch [--no-trust-prompt] [--print] <bundle.app | file.AppImage | app id> [URI…]
```

Every bundle and AppImage starts through it: the registry's command line for them is
`silica-launch <path> %U`, so the Dock, Faro, the Launcher, Mustache and `xdg-open` all go
through the same check.

## Steps

1. **Resolve.** A path is read directly (`Info.toml` of a bundle, the AppImage file); an id is
   asked to `Apps1.Get`, falling back to an in-process registry scan when the daemon is not
   there.
2. **Assess.** `Trust1.Assess(path)` gives the verdict. `allow` continues. `confirm`, `block`
   and `malware` call `Trust1.Prompt(path)`, which shows the alert in the shell and returns the
   choice (`open`, `cancel`, `trash`); only `open` continues. Without the daemon, a bundle is
   verified in process with the same code (`silica-bundle`) and anything but `allow` stops with
   exit status 3: a launch never skips the check because a service is missing.
3. **Run**, by kind:
   - `system`: the bundle's command directly (no sandbox; it is part of the image).
   - `bundle`: the sandbox below.
   - `appimage`: directly, after the first-launch confirmation; with `sandbox = true` in the
     app's approval record (Settings, "Run in sandbox") the sandbox below with `runtime = none`
     and the AppImage bound at `/app/<name>.AppImage` (it needs FUSE inside: not granted, so
     only AppImages that support `--appimage-extract-and-run` work sandboxed).
   - `flatpak`: `flatpak run <id> <URI…>`.

   `--print` prints the command (one argument per line) instead of running it, for tests.

## The command of a bundle

- `kind = "flutter"`: `silica-app --app-id <id> [args…] <app>/flutter_assets`, where `<app>` is
  `Contents/<exec>` (default `app`): `/app/<exec>` inside the sandbox.
- `kind = "native"`: `Contents/<exec> [args…]` (`/app/<exec>` inside).

Then the URIs: files as paths (through the document portal inside the sandbox), other URIs as
they are.

## The sandbox

`bwrap` (bubblewrap) with:

| | |
| --- | --- |
| Namespaces | `--unshare-all --share-net` when `network = true` (otherwise no network), `--die-with-parent`, `--new-session` |
| `/usr` | the runtime's files, read-only: the image's `/usr` for `dev.silica.Platform//1`, `/var/lib/flatpak/runtime/<name>/x86_64/<branch>/active/files` (or the user installation) for a Flatpak runtime, nothing but the loader for `none` |
| `/lib`, `/lib64`, `/bin`, `/sbin` | links into `/usr`, as in Fedora and the freedesktop runtime |
| `/etc` | the runtime's `etc` (the image's `/etc` read-only for `dev.silica.Platform`), with the host's `passwd`, `group`, `resolv.conf`, `hosts`, `localtime`, `machine-id` |
| `/app` | the bundle's `Contents/`, read-only |
| `/.flatpak-info` | `[Application] name=<id> runtime=runtime/<ref>`, `[Instance] instance-id=… app-path=/app`, `[Context] shared=network…`: xdg-desktop-portal and the permission store identify the app from it |
| `/proc`, `/dev` | fresh; `/dev/dri` for `devices = gpu`, the whole `/dev` for `all` |
| `/tmp` | a private tmpfs |
| `$XDG_RUNTIME_DIR` | a tmpfs with the Wayland socket, `pipewire-0`, the D-Bus proxy socket as `bus`, `doc` (the document portal) |
| Home | only `~/.var/app/<id>` (created on first launch), with `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME` pointing into it, plus the `filesystem` items |
| Environment | cleared, then `PATH`, `LANG`/`LC_*`, `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_CURRENT_DESKTOP`, `XCURSOR_*`, `SILICA_*` (renderer, icons, assets), `FLATPAK_ID`, `HOME` |

D-Bus goes through `xdg-dbus-proxy` with `--filter`: the app owns `<id>` and `<id>.*`, talks to
`org.freedesktop.portal.*`, `org.freedesktop.Notifications`, `org.a11y.Bus`, the names in
`talk`, and `dev.frascella.Silica` for `Settings1.Get`/`GetAll`, `Thumbnails1`, `Share1` and
`Sound1` and `System1.State` only (`--call` rules), and sees broadcasts of the portal, of
Settings1 and of System1. The proxy writes a byte on its standard output when it is ready
(`--fd=1`): the app starts only then. `silica-launch` stays the parent of the proxy and of
`bwrap`, waits for the app, then stops the proxy it started and removes the instance folder
(`$XDG_RUNTIME_DIR/silica-launch/<instance>/`, which holds `info` and the proxy socket); if
`silica-launch` itself is killed, the proxy sees its pipe close and exits, and `bwrap`'s
`--die-with-parent` ends the app.

Files the app needs are reached through the portals: the Open and Save dialogs (Silica's
FileChooser backend, which also marks saved files with the quarantine attribute), the document
portal (`$XDG_RUNTIME_DIR/doc/by-app/<id>` is the sandbox's `doc/`), OpenURI, Notifications,
Screenshot. The files of the launch itself (opened from Mustache or the Dock) are bound
read-write at their own paths, so the app gets the path it was asked to open.

A bundle started from a disk image (`location = volume`) runs the same way (its `Contents/` is
already read-only), and the shell shows the "Move to Applications" suggestion (`trust.md`).

## Start time

`silica-launch` adds little to an app's start, and nothing it can avoid:

- A system app (`/usr/Applications`, the development VM's folder) started by its path never
  connects to D-Bus and is not assessed: `silica-launch` reads `Info.toml` and *becomes* the
  embedder (`exec`), so no process stays between the caller and the app, and the pid the caller
  started is the app's. Flatpak apps, AppImages and single-command desktop entries are `exec`ed
  the same way; only a sandboxed bundle keeps `silica-launch` as its parent (it owns the proxy
  and the registry relay).
- The session bus is connected only when something needs it (an id to resolve, a verdict).
- `Trust1` answers from its cache or verifies in full under 200 ms (`trust.md`), and writes
  `trust.bin` only when the outcome changed.
- `xdg-dbus-proxy` signals readiness on a pipe (`--fd`), so no polling waits for it.

`SILICA_LAUNCH_TRACE=1` prints the time of each stage (bus connected, resolved, assessed,
`Info.toml` read, sandbox prepared, proxy ready, `bwrap` or `exec`) on standard error, which
is the session log.

Measured in the development VM (virtio-gpu Venus, Vulkan, 2026-10-19), from `app.launch` (or a
click on the Dock) to the first frame drawn by the app, warm page cache:

| | before `bwrap`/`exec` | first frame |
| --- | --- | --- |
| Calculator, system bundle (direct) | under 2 ms | 0.35 to 0.38 s |
| A self-signed copy in `~/Applications` (sandbox, 30 MB verified in full) | 19 to 29 ms (assessment 14 to 24 ms, proxy 2 to 3 ms) | 0.40 to 0.41 s |
| Clock, desktop entry (no bundle) | | 0.51 s |

With the page cache dropped first: 1.2 s for the system Calculator, 0.76 s for the sandboxed copy
(the runtime's files were partly cached again by then). The ~8 s first seen in silica-desktop#260
came from a sandbox without `/sys`: Mesa found no GPU and Flutter drew in software; binding
`/sys/dev`, `/sys/devices`, `/sys/class` and `/sys/bus` for apps with the GPU fixed it.

## Exit status

the app's own exit status once it ends (0 for a clean exit); 2 usage; 3
refused by the trust check or by the user; 4 not found or not a valid bundle; 5 the sandbox
could not be set up (missing `bwrap` or `xdg-dbus-proxy`, missing runtime: the error names
it).
