# Software update: `dev.frascella.Silica.Updates1`

Status: implemented (silica-os#3), `crates/daemon/src/updates.rs`.

One service for the three ways software changes: app bundles (their own feed), Flatpak apps
and runtimes, and the system image (bootc). Settings › General › Software Update shows them
together.

## Bundle feeds

`update.feed` in `Info.toml` names a feed: a TOML file next to a minisign signature
(`<feed>.minisig`) made with the bundle's developer key.

```toml
format = 1
id = "dev.example.Hello"
version = "1.3.0"
build = 43
minimum_silica = "0.2.0"
url = "https://example.org/hello/Hello-1.3.0.silicaapp"   # the bundle, zipped (silica-bundle pack)
sha256 = "…"                                              # of that file
size = 1234567
zsync = "https://example.org/hello/Hello-1.3.0.silicaapp.zsync"   # optional
notes = "What changed."
```

Checking: the feed and its signature are downloaded (`curl`, HTTPS only, 1 MiB at most), the
signature must be valid for the installed bundle's `developer.key` with the trusted comment
`silica-feed id=<id> build=<build>`, the id must match, and `build` must be higher than the
installed one. A feed signed by another key is refused: a developer cannot be impersonated by
whoever controls the web server.

Installing:

1. Download: with a `zsync` control file, the blocks the installed `.silicaapp` of the current
   build already has (kept in `~/.cache/silica/updates/<id>/`) are reused and only the missing
   ranges are fetched; otherwise, or when the delta fails its checksum, the whole file. Either
   way the result must match `sha256`.
2. Unpack next to the bundle as `Name.app.new` (same folder: a rename is atomic), verify its
   signature with the same developer key and its `Info.toml` (same id, the announced build).
3. When the app is not running (no process with its id in the app registry,
   `$XDG_RUNTIME_DIR/silica-apps`): rename `Name.app` to `Name.app.old`, `Name.app.new` to
   `Name.app`, remove `Name.app.old`. Otherwise the swap waits for the app to quit
   (`pending_restart`).
4. A bundle in `/Applications` that the user cannot write is updated through the polkit action
   `dev.silica.applications.install`.

The quarantine attribute is not set on updates (the developer key vouches for them), and the
approval is carried over when the developer key is the same.

## Flatpak and the system

- Flatpak: `flatpak remote-ls --updates --columns=application,version,branch` to check,
  `flatpak update --noninteractive [<id>]` to install (system installation through Flatpak's
  own polkit actions).
- The system: `bootc upgrade --check` to check (the version is the image's `IMAGE_VERSION`
  label), `bootc upgrade` to stage, applied at the next restart. Both through polkit
  (`org.freedesktop.bootc` is not a D-Bus service: the daemon runs `pkexec bootc` and the
  session's agent asks for the password).

## AppImages

AppImages that embed update information (`.upd_info` ELF section, `zsync|<url>`) are listed
with their available version when the `zsync` control file announces a newer one; updating
replaces the file atomically the same way (`Name.AppImage.new`, rename) after the download
matches the control file's SHA-1.

## Interface

Object `/dev/frascella/Silica/Updates1`.

| Member | Signature | |
| --- | --- | --- |
| `Check()` | `→ a(sssssa{sv})` | id, kind (`bundle`, `flatpak`, `appimage`, `system`), name, current version, available version, details (`size` t, `notes` s, `path` s) |
| `Update(s id)` | `→ s` | `installed`, `pending_restart` (the app is running), `staged` (system: restart to apply) |
| `UpdateAll()` | `→ a(ss)` | id, result |
| `Progress(s id, d fraction)` | signal | |
| `Finished(s id, s result)` | signal | `result` as above, or `failed: <reason>` |

Automatic checks run once a day while the session is on AC power and online; Settings checks on
open. Nothing installs without the user's click (no automatic install yet).
