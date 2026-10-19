# Launch Services: `dev.frascella.Silica.Apps1`

Status: implemented (silica-os#3, silica-desktop#138). The bundle format is specified in
silica-os `docs/app-bundles.md`.

The app registry of the session: which apps are installed, how each one starts, which app opens
a type or a URL scheme. It lives in `silica-daemon` (`crates/daemon/src/apps.rs`), on top of the
shared parsers of `crates/apps` (the `silica-apps` crate), which Ossido and `silica-launch` use
directly. Before it there were four parsers (Ossido's `desktop_entry.rs`, the daemon's
`icons/apps.rs`, the shell's `desktop_entries.dart`, Mustache's `apps.dart`); the Rust ones are
now `silica-apps`, the Dart ones read the registry over D-Bus and keep a local reader only as a
fallback when the daemon is not running (tests, another desktop).

## Sources

| Source | Where | Kind |
| --- | --- | --- |
| System bundles | `/usr/Applications/*.app` (and the development VM's root-owned `/var/lib/silica-dev/Applications`) | `system` |
| Bundles | `/Applications/*.app`, `~/Applications/*.app` | `bundle` |
| AppImages | type-2 AppImages (`*.AppImage`, magic `AI\x02` at offset 8) in the same folders | `appimage` |
| Flatpak | desktop entries under `…/flatpak/exports/share/applications` (or with `X-Flatpak=`) | `flatpak` |
| Desktop entries | `applications/` of `XDG_DATA_HOME` and `XDG_DATA_DIRS` | `system` |

Rules:

- The folders are read one level deep (a bundle inside a bundle is not an app).
- Equal ids: a bundle wins over a desktop entry (the entry may be its compatibility entry),
  `~/Applications` over `/Applications` over `/usr/Applications`; for desktop entries the first
  data folder wins, as in the Desktop Entry specification.
- A desktop entry with `X-Silica-Bundle=<path>` is a bundle's compatibility entry: skipped when
  that bundle is registered, used as a plain entry otherwise.
- Hidden entries (`Hidden=true`) are dropped; `NoDisplay=true` entries are registered (they can
  open types) with `no_display = true`, and the shell does not list them.
- An AppImage carries a desktop entry and its icon in the root of its squashfs (which starts
  where the ELF runtime ends: after its section header table, magic `hsqs`). The daemon's
  registry extracts them once with `unsquashfs -o <offset>` (the root's `*.desktop`, `*.png`,
  `*.svg` and `.DirIcon`, regular files only) into `$XDG_CACHE_HOME/silica/appimages/<key>/`,
  the key a hash of the file's path, size and modification time (a replaced or updated
  AppImage is read again); Ossido and `silica-launch` only read that cache. The app then has
  the entry's name, localized names, comment, categories, keywords, types, URL schemes and
  window class, the extracted icon, and the entry's file name as its id when that is a
  reverse-DNS id (silica-desktop#257). Without an entry (no `unsquashfs`, a damaged image) its
  name is the file name without `.AppImage` and the version suffix, and its id
  `appimage.<name>`.

## Watching

Every source folder is watched with inotify (create, delete, move, attribute and close-write
events, one level deep, plus `Contents/Info.toml` of each bundle). Changes are coalesced for
150 ms, then the registry is rebuilt and compared with the previous one: `Added`, `Removed`
and `Changed` are emitted per id. A folder that does not exist yet (`~/Applications`, the first
Flatpak export) is watched through its nearest existing parent and picked up when it appears.
When inotify runs out of watches the registry falls back to a rescan every 10 s.

Moving or renaming a bundle keeps its id: the registry emits `Changed` with the new path, so
the Dock keeps its pin. A bundle that disappears and appears elsewhere within the same 150 ms
window is one `Changed`.

## Compatibility entries

For every `bundle` and `appimage` the registry writes a desktop entry in
`$XDG_DATA_HOME/applications/silica-bundles/<id>.desktop` (with `X-Silica-Bundle=`), removes
those whose app is gone, and runs `update-desktop-database` on that folder when its contents
changed, so that `xdg-open`, GTK and Qt apps see them. The entry's `Exec` is
`silica-launch <path> %U`.

## Database

`$XDG_CACHE_HOME/silica/apps.bin` keeps, per id, the last launch time and the time the app was
first seen; the rest is rebuilt from the sources at start. It is a binary file (magic `SAPP`,
version, then length-prefixed records), written atomically (temporary file and rename). A
damaged file is ignored and rewritten. An app is *new* when it was first seen after the
database existed and has not been launched since: the Launcher shows its dot.

## Associations

`DefaultFor(type)` follows the "Association between MIME types and applications" specification:
`[Default Applications]` of the `mimeapps.list` files (`$XDG_CONFIG_HOME`, `XDG_CONFIG_DIRS`,
then `applications/` of the data folders), for the type and then each of its parents (the
shared MIME database), skipping ids listed in `[Removed Associations]` and ids not installed;
then `[Added Associations]`; then the registered apps that declare the type, editors before
viewers, by name. URL schemes use the type `x-scheme-handler/<scheme>`.

`SetDefault(type, id)` writes `[Default Applications]` of `$XDG_CONFIG_HOME/mimeapps.list`
(the other lines kept as they are) and emits `DefaultChanged(type, id)`.

## Interface

Object `/dev/frascella/Silica/Apps1` on `dev.frascella.Silica`, interface
`dev.frascella.Silica.Apps1`. Only D-Bus types: no JSON.

| Member | Signature | |
| --- | --- | --- |
| `List()` | `→ a(ssssa{sv})` | every app: id, name, kind, path, properties |
| `Get(s id)` | `→ a{sv}` | one app's properties; error `…Apps1.Error.NotFound` |
| `AppsFor(s type)` | `→ as` | the ids that open `type`, the default first |
| `DefaultFor(s type)` | `→ s` | the default id, `""` when none |
| `SetDefault(s type, s id)` | | |
| `Open(s id, as uris, a{sv} options)` | `→ u` | starts the app with the URIs, returns the pid |
| `OpenUris(as uris)` | `→ u` | opens each URI with its default app (grouped per app) |
| `NoteLaunched(s id)` | | the shell started the app itself (clears *new*) |
| `Uninstall(s id)` | `→ b` | Flatpak apps: `flatpak uninstall`; other kinds are moved to the Trash by Mustache |
| `Added(s id)`, `Removed(s id)`, `Changed(s id)` | signals | |
| `DefaultChanged(s type, s id)` | signal | |

Properties of an app (`a{sv}`; absent keys are empty):

| Key | Type | |
| --- | --- | --- |
| `id`, `name`, `kind`, `path` | `s` | `kind`: `system`, `bundle`, `flatpak`, `appimage` |
| `location` | `s` | `system`, `local` (`/Applications`), `user` (`~/Applications`), `volume`, `xdg` |
| `names` | `a{ss}` | localized names by language |
| `generic_name`, `comment`, `version` | `s` | |
| `exec` | `s` | the command line with its field codes (`%U`…) |
| `icon` | `s` | the freedesktop icon (name or path) |
| `silica_icon` | `s` | the silica-icons folder or the `silica.assets` name |
| `wm_class` | `s` | `StartupWMClass` |
| `keywords` | `as` | |
| `keywords_l10n` | `a{ss}` | localized keywords, `;`-joined, by language |
| `categories`, `mime_types`, `url_schemes` | `as` | |
| `types` | `a{ss}` | type → `editor` or `viewer` |
| `document_icons` | `a{ss}` | type → document icon name |
| `actions` | `a(sss)` | id, name, command line |
| `permissions` | `a{sv}` | bundles: `network` b, `devices` s, `filesystem` as, `talk` as, `background` b |
| `app_kind`, `runtime`, `developer` | `s` | bundles |
| `desktop_file` | `s` | the entry the app came from |
| `no_display`, `terminal`, `share`, `new` | `b` | `share`: `X-Silica-Share=true` |
| `added`, `last_used` | `t` | unix seconds |

`Open` options: `activation_token` (`s`, passed as `XDG_ACTIVATION_TOKEN`). The app is started
through `silica-launch` for `bundle` and `appimage` (trust check and sandbox, `launch.md`),
directly from its `Exec` otherwise, detached in its own session.

## Consumers

- Ossido's `app.launch {app}`: `silica_apps::Registry` in process (no D-Bus round trip on the
  compositor's thread); same rules, same command line.
- The shell (`desktop_entries.dart`): `SilicaApps` (silica_ui) at start and on the signals.
  A window (or a running app of the process registry) belongs to one app only: the one whose
  id, or `wm_class`, is the window's app id; the looser guesses (the last part of the id, the
  program of `exec`, never a launcher such as `silica-launch` or `flatpak`) apply only when no
  app claims it exactly, so the Dock, the switcher, the menu bar and Force Quit never show an
  app twice (silica-desktop#260). Dock pins saved under another name of an app (its desktop
  entry's file name, its window class, another case) are rewritten to the registry id when the
  apps load.
- `Thumbnails1.AppIcon` and the document icons: the daemon's live registry.
- `Share1`: the apps with `share = true`.
- Mustache's Open With: `AppsFor`, `DefaultFor`, `SetDefault`, `Open`.
