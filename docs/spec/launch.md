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
`Sound1` only (`--call` rules), and sees broadcasts of the portal and of Settings1. The proxy is
started by `silica-launch` with a sync pipe (`--fd`), so the app starts only once the proxy is
ready, and exits with the app.

Files the app needs are reached through the portals: the Open and Save dialogs (Silica's
FileChooser backend, which also marks saved files with the quarantine attribute), the document
portal for files opened from Mustache (the URIs of the launch are exported to it first, so the
app gets `/run/user/<uid>/doc/<id>/<name>` paths), OpenURI, Notifications, Screenshot.

A bundle started from a disk image (`location = volume`) runs the same way (its `Contents/` is
already read-only), and the shell shows the "Move to Applications" suggestion (`trust.md`).

## Exit status

0 the app was started (`silica-launch` execs `bwrap`, which exits with the app); 2 usage; 3
refused by the trust check or by the user; 4 not found or not a valid bundle; 5 the sandbox
could not be set up (missing `bwrap` or `xdg-dbus-proxy`, missing runtime: the error names
it).
