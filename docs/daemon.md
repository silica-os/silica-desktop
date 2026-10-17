# silica-daemon

Silica's system services on D-Bus (`crates/daemon`, binary `silica-daemon`): notifications,
the system state for the Control Center, the clipboard history, Faro's actions and the
interface sound effects.

```
silica-daemon [--address ADDRESS] [--replace]
```

It connects to the session bus (`DBUS_SESSION_BUS_ADDRESS`, or `--address`) and takes the
names `org.freedesktop.Notifications` and `dev.frascella.Silica`. If another notification
server already has them (nested in GNOME), it exits with an error; `--replace` takes them if
the owner allows it.

## Private session bus

`ossido --session-bus` (passed by `cargo xtask run --shell`) starts its own `dbus-daemon` and
`silica-daemon` on that bus, prints `DBUS_SESSION_BUS_ADDRESS=…` on stdout and passes the
address to all the clients it launches (`--spawn`, `--spawn-shell`, `app.launch`).
Nested in GNOME, the names of the host session stay its own; the system bus is the
machine's.

`silica-daemon` is looked for next to Ossido's executable, then in `PATH`; if it is missing,
Ossido warns and goes on without notifications. The bus dies with Ossido even if Ossido is
killed (`dbus-daemon` runs under a shell that terminates when the stdin connected to Ossido
closes) and `silica-daemon` exits when it loses the bus.

## The silica/dbus channel

The embedder gives Dart D-Bus with JSON values (`crates/embedder/src/dbus`, Dart side
`shell/lib/system/dbus.dart`), on a separate thread. Codec `JSONMethodCodec`:

- `call {bus?, destination, path, interface?, method, signature?, args?}` → the list of the
  reply's arguments. `bus` is `session` (default) or `system`. The arguments are converted
  according to `signature` (`"su"`, `"a{sv}"`…); inside a variant the type is deduced from
  the JSON (integers `i` or `x`, decimals `d`, string lists `as`, objects `a{sv}`).
  Structures → lists, dictionaries → maps. A D-Bus error becomes a `PlatformException` with
  the error name as the code; after 25 s `org.freedesktop.DBus.Error.Timeout`.
- `subscribe {bus?, sender?, path?, interface?, member?}` → an id; signals arrive as the
  call `signal {id, sender, path, interface, member, args}` (at most 256
  subscriptions). `unsubscribe {id}` → `true` if it existed.

In Dart: `DBus.instance.call(...)`, `getProperty`, `getAll` and `signals(...)` (a stream:
the subscription closes when it is cancelled).

## org.freedesktop.Notifications

The [freedesktop specification](https://specifications.freedesktop.org/notification-spec/) 1.2
on `/org/freedesktop/Notifications`, for the apps (`notify-send`, libnotify, portals).

- `Notify`: returns the id (never 0). With the `replaces_id` of a notification still present
  it replaces it and keeps the id.
- `CloseNotification`: closes with reason 3; an id that no longer exists is not an error.
- `GetCapabilities`: `actions`, `body`, `icon-static`, `persistence`. No markup: the
  shell shows the body as text.
- `GetServerInformation`: `Silica`, `Silica`, crate version, `1.2`.
- Signals `NotificationClosed(id, reason)` and `ActionInvoked(id, key)`.

Hints read: `urgency` (byte or integer), `category`, `desktop-entry`, `image-path`,
`transient`, `resident` (booleans or integers). The others are ignored, as are wrong types.

Expiry: as in the reference's Notification Center, a notification stays in the history until
the user closes it. It expires (reason 1) only with a positive `expire_timeout`; -1 and 0 do
not expire. The banner time is decided by the shell.

Limits against hostile clients: title 1024 characters, body 8192, names 256, 32 action
entries (a key without a label is discarded), history of 256 notifications (beyond that the
oldest is closed with reason 4).

## dev.frascella.Silica.Notifications1

On `/dev/frascella/Silica/Notifications1`, for the shell (banners and Notification Center).

- `List() → a(ussssasysssbbti)`: the history, oldest first. Fields: id, app, icon,
  title, body, actions (key, label, flattened), urgency, category,
  desktop-entry, image, transient, resident, reception in Unix ms, `expire_timeout`.
- `Dismiss(id)`: closed by the user (reason 2). `Clear()`: all of them.
- `InvokeAction(id, key)`: emits `ActionInvoked`; without `resident` it then closes with
  reason 2. A missing key is the `InvalidArgs` error.
- `Reply(id, text)`: for notifications with the `inline-reply` action (KDE's extension,
  announced in `GetCapabilities`): emits `NotificationReplied(id, text)` on
  `org.freedesktop.Notifications`; without `resident` it then closes with reason 2.
- Signals `Added(notification)` (new or replaced: same id) and `Removed(id, reason)`.

## Trying it by hand

```
dbus-daemon --session --fork --print-address    # prints the address
silica-daemon --address unix:path=…
DBUS_SESSION_BUS_ADDRESS=unix:path=… notify-send Hello "from the daemon"
DBUS_SESSION_BUS_ADDRESS=unix:path=… gdbus call --session -d dev.frascella.Silica \
  -o /dev/frascella/Silica/Notifications1 -m dev.frascella.Silica.Notifications1.List
```

On Ubuntu the AppArmor profile of `notify-send` blocks its `GetCapabilities` call towards an
unconfined server: `notify-send` then shows the notification without actions. The actions
are tried with the tests (`cargo test -p silica-daemon`, on a private `dbus-daemon`).

## dev.frascella.Silica.System1

On `/dev/frascella/Silica/System1`: the system state for the Control Center and the menu
bar.

- `State() → a{sv}`: flat keys. `wifi.available`, `wifi.enabled`, `wifi.ssid`,
  `wifi.strength` (0–100), `wifi.wired`; `bluetooth.available`, `bluetooth.powered`;
  `battery.present`, `battery.percent`, `battery.charging`; `brightness.available`,
  `brightness` (0–1); `audio.available`, `audio.volume` (0–1), `audio.muted`, `audio.name`
  (name of the output, PipeWire's `node.description`); `dnd`; `keyboard.available`,
  `keyboard.brightness` (0–1), `keyboard.requests` (the keyboard backlight, UPower's
  `KbdBacklight`); `privacy` (`as`: `app␟kind␟1|0` per app that recorded from the
  microphone or the camera, now or in the last 10 minutes, the ones recording now first; from
  the active capture streams of `wpctl status`, every 2 s).
- `SetWifiEnabled(b)`, `SetBluetoothPowered(b)`, `SetVolume(d)`, `SetMuted(b)`,
  `SetBrightness(d)` (0–1, never to zero), `SetKeyboardBrightness(d)`,
  `SetDoNotDisturb(b)`. A missing source is the `org.freedesktop.DBus.Error.Failed` error.
- For the Control Center's detail panels, read on request:
  `WifiNetworks() → a(sybbb)` (name, signal 0–100, secured, saved, in use; the one in use
  first, then the saved ones, then by signal; at most 40), `ConnectWifi(s)` (saved networks
  only: a new network asks for a password), `AudioOutputs() → a(usb)` (PipeWire id, name,
  default; from `wpctl status`), `SetAudioOutput(u)`.
- Signal `Changed(a{sv})` with the whole state on every change.
- What Ossido knows and apps need, reported by Ossido with `ReportCompositor(s json)` (a JSON
  object; only these keys are kept, the rest is ignored) and part of `State`/`Changed` from
  then on: `input.touchpad` and `input.mouse` (b: a touchpad / another pointer is connected;
  Settings shows its Trackpad pane only with a touchpad). Ossido sends the whole report again
  whenever the daemon (re)appears. `displays` (s): the JSON of `output.list`, sent again on
  every change (a display plugged in, a new mode, scale or rotation), so `Changed` tells apps.

### Focus

- `SetFocus(s id)`: turns on a Focus mode, `dnd` (Do Not Disturb) or the `id` of a mode in
  `$XDG_CONFIG_HOME/silica/focus.json`; an empty id turns Focus off; an unknown id is
  `InvalidArgs`. Any mode silences banners like Do Not Disturb (`dnd` follows it, and
  `SetDoNotDisturb` sets the mode to `dnd` or none). The active id is saved as `active` in the
  file (not restored at start yet).
- State keys `focus` (the active id, empty when off), `focus.name`, `focus.symbol`,
  `focus.allowedApps` and `focus.allowedPeople` (`as`): the shell lets the allowed apps'
  notifications through (not done in the shell yet).

`focus.json`, written by Settings: `{"modes": [{"id", "name", "symbol", "allowedApps",
"allowedPeople"}], "active"}` (at most 32 modes; the id `dnd` is reserved). Schedules are not
handled yet.

### Energy

- `Suspend()`: logind's `Suspend(false)`; Ossido calls it when `sleepMinutes` of `power.json`
  is up (`docs/control.md`). Without the system bus: `Failed`.
- `preventSleep` of `$XDG_CONFIG_HOME/silica/power.json` (read again every 30 s): while it is
  on the daemon holds logind's `idle` inhibitor (mode `block`), so logind's own idle action
  does not suspend either; a sleep the user asks for still happens. State keys
  `power.preventSleep` and `power.inhibited` (the inhibitor is held).

### System look

- `SetLook(a{sv})`: the Appearance choices every app follows live, any of `accent`
  (`multicolour|blue|purple|pink|red|orange|yellow|green|graphite`), `highlight` and
  `folderColour` (an accent name, or `automatic`), `sidebarIconSize` (`small|medium|large`),
  `scrollBars` (`automatic|whenScrolling|always`), `scrollClick` (`page|spot`), `iconStyle`
  (`default|dark|clear|tinted`) as strings, `glass` (d, 0–1) and `windowTint` (b). An invalid value
  is `InvalidArgs` and changes nothing. Not saved by the daemon: Settings keeps them in
  `$XDG_CONFIG_HOME/silica/theme.json` (its own persistence, read at start; Ossido watches it for
  `glass` and `windowTint`).
- State keys `look.accent`, `look.highlight`, `look.sidebarIconSize`, `look.scrollBars`,
  `look.scrollClick`, `look.iconStyle`, `look.folderColour`, `look.glass`, `look.windowTint`.
  silica_ui's `SilicaSystemLook` reads them (typed; no app parses JSON).

### Displays and night light

- `Outputs() → s`: the displays as JSON (Ossido's `output.list`, `docs/control.md`).
- `ConfigureOutput(s json) → s`: changes a display, `{connector?, mode?: {width, height,
  refresh?}, scale?, transform?}`; returns the display after the change. Applied at once and
  saved by Ossido in `displays.json` for that display (a confirmation with a revert timer is
  up to Settings). Not a JSON object: `InvalidArgs`; refused by Ossido or no Ossido: `Failed`.
- `SetNightLight(a{sv})`: any of `enabled` (b), `temperature` (u, 1900–6500 K, default 3400),
  `schedule` (`off`: on while enabled; `sunset`: from sunset to sunrise; `custom`: from `from`
  to `to`), `from` and `to` (`HH:MM`, local time, the window may cross midnight). An invalid
  value is `InvalidArgs` and changes nothing. Saved in `$XDG_CONFIG_HOME/silica/nightlight.json`
  (same keys), which is read again at every check, so Settings may also write it directly.
- State keys `nightlight.enabled`, `nightlight.temperature`, `nightlight.schedule`,
  `nightlight.from`, `nightlight.to` and `nightlight.active` (b: the displays are warm now).

The schedule is checked every 30 s and at once after a change; the temperature goes to Ossido
(`output.color_temperature` on its actions socket) when it changes. Sunset and sunrise come
from the time zone's coordinates (tzdata's `zone1970.tab` for the zone of `/etc/localtime`),
with NOAA's solar equations (`crates/daemon/src/sun.rs`); without a known place or in polar day
and night the night is 19:00–07:00. The local time comes from `date +%z`. No location service
is used.

Sources: NetworkManager (`WirelessEnabled`, primary connection, access point strength),
BlueZ (the first adapter), UPower (`DisplayDevice`, only if it is a battery) on the system
bus, read again on each of their signals; the backlight in `/sys/class/backlight`
(written with logind's `SetBrightness`, otherwise in the file) and the audio of the default
output with `wpctl`, read again every second. If a service is missing the source is absent
(`*.available = false`) and it is retried every 10 s.

## Light and dark appearance

- `System1` has the `appearance.dark` and `appearance.mode` keys and the `SetDark(b)` and
  `SetAppearanceMode(s)` methods, which save the choice in
  `$XDG_CONFIG_HOME/silica/appearance.json` (`{"mode": "auto", "dark": true}`) for the next
  start. The mode is `light`, `dark` or `auto`; `SetDark` sets it to `light` or `dark`, any
  other mode is `InvalidArgs`. A file without `mode` (older) means `dark` or `light` from
  `dark`.
- In `auto` the daemon turns `appearance.dark` on at sunset and off at sunrise, checked every
  30 s (the same sun as the night light: the time zone's coordinates, 19:00–07:00 without them),
  emits `Changed` and keeps `dark` in the file up to date. The file is read again at every
  check, so Settings may also write `mode` there directly.
- `silica-portal` offers `org.freedesktop.impl.portal.Settings` with
  `org.freedesktop.appearance` › `color-scheme` (1 dark, 2 light): it reads it from the daemon
  and follows its `Changed` (without the daemon, from the file), and emits `SettingChanged`.
  GTK, Qt and libadwaita apps follow it from the frontend; the embedder reads it from the
  backend and sends Flutter `flutter/settings` with `platformBrightness`.
- Ossido starts `silica-portal` together with `silica-daemon` on its private bus; the
  commands `system.dark` and `system.dark_toggle` call `SetDark`.

## dev.frascella.Silica.Clipboard1

The clipboard history, for Faro's clipboard scope (`crates/daemon/src/clipboard.rs`), on
`/dev/frascella/Silica/Clipboard1`.

- `History() → s`: the entries as JSON, newest first: `[{id, kind, text, image?, time}]`. `kind`
  is `text`, `image` (a PNG kept in `$XDG_RUNTIME_DIR/silica/clipboard/`) or `files`, a
  `text/uri-list`.
- `Copy(t id) → b`: puts the entry back on the clipboard, from the daemon's own source.
- `CopyImage(s path) → b`: puts a PNG file on the clipboard as `image/png` (a screenshot sent
  to the clipboard: ⌃⇧⌘3, ⌃⇧⌘4, or the "Clipboard" place of the screenshot tools). The file
  is read before the call returns, so the caller may delete it right after. False if it is
  not a PNG or is larger than 128 MiB.
- `Remove(t id) → b` and `Clear()`.
- Signal `Changed()` on every change.

How it watches the selection: over `ext-data-control-v1`. Ossido offers that global only to
`silica-daemon`, recognised by its executable through the pid of its socket. A selection that
carries the `x-kde-passwordManagerHint` type is never read. The history lives in memory and is
bounded (50 entries, 8 hours, both provisional).

## dev.frascella.Silica.Actions1

The system actions of Faro's actions scope (`crates/daemon/src/actions.rs`), on
`/dev/frascella/Silica/Actions1`.

- `List() → s`: `[{id, label, description, shortcut}]` from Ossido's command catalog
  (`silica-control`), for the commands a person runs by name. Faro runs them with
  `engine.execute`.

## dev.frascella.Silica.Sound1

The interface sound effects (`crates/daemon/src/sound.rs`), on `/dev/frascella/Silica/Sound1`,
for apps and the shell.

- `PlaySound(s name)`: plays `<name>.wav` (or `.oga`, `.ogg`) with PipeWire's `pw-play` and
  returns at once. The name is 1–64 characters among `a-z`, `0-9` and `-`; anything else is
  `InvalidArgs`, a name with no file `FileNotFound`. At most 4 sounds play at once (more are
  dropped, not an error); without `pw-play` nothing plays and nothing fails.
- `UiSoundsEnabled() → b` and `SetUiSoundsEnabled(b)`: "Play user interface sound effects", on
  by default, saved as `uiSounds` in `$XDG_CONFIG_HOME/silica/sound.json` (the other keys of
  the file stay; the older `ui_sounds` is still read). The file is read again on each sound,
  so a change made elsewhere applies at once. Signal `UiSoundsChanged(b)`.
- `PlayAlert(s name)`: plays the alert sound at the alert volume (`pw-play --volume`). Empty
  `name`: the chosen one (`alert` in `sound.json`, default `alert-crystal`); otherwise an
  `alert-*` name, for Settings' preview (anything else is `InvalidArgs`). Alerts play even
  with the interface sounds off; with `alertVolume` 0 nothing plays.
- `AlertSounds() → as`: the `alert-*` sounds found, sorted (the choices of the Alert sound
  pop-up).
- `PlayVolumeFeedback() → b`: plays `volume-feedback` when `volumeFeedback` is on (off by
  default); true if it played. For the volume keys.

`sound.json`, written by Settings:

| Key | Type | Default |
| --- | --- | --- |
| `uiSounds` | boolean | true |
| `alert` | string, an `alert-*` sound | `alert-crystal` |
| `alertVolume` | number 0–1, relative to the output volume | 1 |
| `volumeFeedback` | boolean | false |

Where the sounds are looked for, first match wins: `$SILICA_SOUNDS_DIR`, then
`$XDG_DATA_HOME/silica/sounds`, then `silica/sounds` in each of `$XDG_DATA_DIRS`. The sounds
are original, synthesized by `tools/sounds/synth.py` into `data/sounds/`; `cargo xtask run`
points `SILICA_SOUNDS_DIR` there, `cargo xtask os deploy` copies them into the share's
`share/silica/sounds`, and a package installs them in `/usr/share/silica/sounds`.

| Name | When |
| --- | --- |
| `trash-move` | items moved to the Trash, once the move is done: a menu, ⌘⌫, a script or a drop on the Dock's Trash (file manager windows, desktop, Dock) |
| `trash-drag` | an alias of `trash-move` (the Trash sound is one); nothing plays it any more |
| `trash-empty` | the Trash emptied, ~0.2 s after the Dock's Trash icon turns empty |
| `file-transfer` | a move, copy or Duplicate finished (drag and drop, paste, ⌘D, a drop on a Dock folder), once, on completion |
| `screen-capture` | a screenshot taken (⇧⌘3, ⇧⌘4, ⇧⌘5, also to the clipboard); provisional, not measured |
| `alert-crystal`, `alert-woodblock`, `alert-drop`, `alert-pebble` | alerts (`PlayAlert`) |
| `volume-feedback` | the volume keys with `volumeFeedback` on (`PlayVolumeFeedback`) |

```
gdbus call --session -d dev.frascella.Silica -o /dev/frascella/Silica/Sound1 \
  -m dev.frascella.Silica.Sound1.PlaySound trash-empty
```

## dev.frascella.Silica.Thumbnails1

`/dev/frascella/Silica/Thumbnails1`. Thumbnails of files for apps and the shell, cached per the
freedesktop thumbnail specification (`$XDG_CACHE_HOME/thumbnails/{normal 128, large 256, x-large 512,
xx-large 1024}/<md5 of the URI>.png`, `Thumb::URI` and `Thumb::MTime` text chunks, failures in
`fail/silica-daemon/`), so other Linux apps share the cache.

- `Thumbnail(s uri, u size_px) -> s path`: the cached PNG in the smallest bucket with an edge of at
  least `size_px`, generated on a bounded worker pool (not on the bus thread); "" when the file has
  no thumbnail (unsupported type, folder, unreadable, or a recorded failure): the client keeps the
  file-type icon. A cached file is reused while its `Thumb::MTime` equals the file's mtime.
- `Invalidate(as uris)`: forgets the thumbnails of these files in every bucket.
- `Icon(s uri, u size_px, d scale, u flags) -> (s path, b preview)`: the icon of a file or folder
  at exactly `size_px` pixels (`size_px / scale` points wide), composed from the system layers;
  see [`thumbnails.md`](thumbnails.md). For a file that has a preview (as `Thumbnail`) the preview
  is returned with `preview` true, unless `flags` has `NO_PREVIEW` (1). ("", false) when nothing can
  be made (no `silica.assets`, missing file, queue full): the client keeps its own icon.
- Signal `Ready(s uri, u size_px, s path)` after a successful `Thumbnail` or `Icon`.

Generators: PNG, JPEG, GIF (first frame), WebP, BMP, TIFF (EXIF orientation applied), and the first
page of a PDF through `pdftoppm` when poppler is installed; then, for other types, the freedesktop
thumbnailers (`$XDG_DATA_DIRS/thumbnailers/*.thumbnailer`) of the installed apps, each run in a
subprocess killed after 15 s. Not yet: SVG, video, text previews. The composed icons (folders, and
files without a preview: the icon of the app that opens them, its document icons, or the unknown
extension) are in [`thumbnails.md`](thumbnails.md).

## Authentication (polkit agent)

The session's polkit authentication agent (`crates/daemon/src/auth.rs`): when an app asks for
something that polkit allows only after a password (`auth_admin_keep`: the time zone and
automatic time with `timedatectl`, the computer name with `hostnamectl`, the language and
region with `localectl`), polkit calls this agent and the shell shows the system
authentication dialog. Without an agent those calls fail with "Interactive authentication
required".

- At start the daemon registers `org.freedesktop.PolicyKit1.AuthenticationAgent` at
  `/dev/frascella/Silica/PolkitAgent` on the **system** bus with
  `RegisterAuthenticationAgent` for its logind session (`unix-session`, the id from
  `XDG_SESSION_ID` or logind's `GetSessionByPID`), with the session's `LANG` as locale. If it
  fails (polkit not running, another agent already registered for the session, as nested in
  GNOME) it tries again every 30 s; when polkit restarts it registers again.
- `BeginAuthentication` is accepted only from the owner of `org.freedesktop.PolicyKit1`. The
  users offered are polkit's `unix-user` identities (groups arrive expanded) with their names
  from `/etc/passwd`; the user running the session comes first when polkit accepts them. The
  app that asked is the first ancestor of `polkit.subject-pid` that is not a command-line
  wrapper (`timedatectl`, `sh`, `pkexec`…), by executable name. At most 8 requests wait at
  once.
- The password is checked by polkit's helper, which runs PAM and reports to polkit itself:
  through `/run/polkit/agent-helper.socket` when it exists (polkit ≥ 126, socket-activated:
  the user name and the cookie on the first two lines), otherwise by running
  `polkit-agent-helper-1 USER` (setuid; the cookie on stdin). The conversation is the helper's:
  `PAM_PROMPT_ECHO_OFF` gets the password, `PAM_PROMPT_ECHO_ON` the user name, then
  `SUCCESS` or `FAILURE`. A second secret prompt (a one-time code) counts as a failure. The
  password is never logged or kept.
- Cancelled by the user: polkit gets `org.freedesktop.PolicyKit1.Error.Cancelled`.
  `CancelAuthentication` from polkit closes the dialog.

### dev.frascella.Silica.Authentication1

On `/dev/frascella/Silica/Authentication1` (session bus), for the shell's dialog and for apps
that want to know whether a password can be asked:

- `Pending() → a(ussssa(ss)s)`: the waiting requests, oldest first: id, action id, polkit's
  message (in the session's language), icon name, app (executable name, may be empty), users
  (login name, full name), default user.
- `Respond(u id, s user, s password) → b`: true if the password is right (the request then
  closes with `success`), false to try again. `user` must be one of the request's users,
  otherwise `InvalidArgs`; a check already running or a helper that cannot be reached is
  `Failed`. It returns when PAM answers (a few seconds after a wrong password).
- `Cancel(u id)`: the user dismissed the dialog.
- Property `Registered` (b, with `PropertiesChanged`): the agent is registered with polkit.
- Signals `Requested(request)` (same tuple as `Pending`) and `Closed(u id, s outcome)`
  (`success` or `cancelled`).

The dialog is drawn by the shell in the switcher layer (`shell/lib/auth/auth_dialog.dart`): a
system alert above every window with the app's icon (from its desktop entry, a padlock if
unknown), a title naming the app, polkit's message, the user name and password fields and
Allow / Cancel. A wrong password shakes it and clears the field; Escape cancels.

```
pkexec true      # or: timedatectl set-timezone Europe/Rome
```

### Without a password: a polkit rule for administrators

An image may also let the administrators of an active local session change these settings
without a password (the agent stays the general fix). Install it as
`/etc/polkit-1/rules.d/50-silica-settings.rules`:

```js
// Administrators at the machine change the time, the computer name and the language
// without typing their password.
polkit.addRule(function (action, subject) {
    var actions = [
        "org.freedesktop.timedate1.set-timezone",
        "org.freedesktop.timedate1.set-ntp",
        "org.freedesktop.timedate1.set-time",
        "org.freedesktop.hostname1.set-hostname",
        "org.freedesktop.hostname1.set-static-hostname",
        "org.freedesktop.locale1.set-locale",
        "org.freedesktop.locale1.set-keyboard",
    ];
    if (actions.indexOf(action.id) >= 0 && subject.active && subject.local &&
        subject.isInGroup("wheel")) {
        return polkit.Result.YES;
    }
});
```

## Replacement

When a newer daemon takes `dev.frascella.Silica` (`--replace`, from a new session on the same
bus), the old one exits instead of polling the system next to it.
