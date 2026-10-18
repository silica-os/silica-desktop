# Shell configuration: the Settings service

The shell components and the Settings app share one typed store, silica-daemon's
`dev.frascella.Silica.Settings1` (`docs/daemon.md`, schema in `crates/daemon/src/settings.rs`).
There are no per-component JSON files, parsers or file watchers any more: a component asks
`SilicaSettings` (silica_ui) for the keys it needs, listens to them, and applies a change in the
same frame as the daemon's `Changed` signal. Settings writes with `Set`/`SetMany`; the shell's own
controls (the Dock's separator menu, the Control Center's Edit Controls, the Screenshot bar) write
the same keys.

A key left unset has the default below. A value outside the schema (type, range, set) is refused
by the daemon with `InvalidArgs`; readers still fall back to the default for anything odd.

## Keys

| Key | Type | Default | Applies in |
| --- | --- | --- | --- |
| `dock.magnify` | bool | false | Dock: magnification |
| `dock.recents` | bool | false | Dock: recent and suggested apps section |
| `dock.autohide` | bool | false | Dock: slides away, back at the bottom edge |
| `dock.position` | `left` `bottom` `right` | bottom | Dock (only the bottom is drawn, #72, #73) |
| `dock.minimizeEffect` | `genie` `scale` | genie | Dock: how windows go in |
| `dock.indicators` | bool | true | Dock: dots under open apps |
| `dock.tileSize` | int 16-128 | 60 | Dock: icon size (also the separator drag) |
| `dock.magnifiedSize` | int 16-128 | 64 | Dock: magnified size, never below `tileSize` |
| `dock.launchAnimation` | `bounce` `none` | bounce | Dock: bounce while an app opens |
| `dock.minimizeToApp` | bool | false | Dock; Ossido (see below) |
| `menubar.sound` | `always` `active` `never` | active | menu bar: Sound item |
| `menubar.bluetooth` `menubar.battery` `menubar.batteryPercent` `menubar.focus` | bool | false, true, false, false | menu bar items |
| `menubar.clock.hour24` `.seconds` `.weekday` | bool | true, false, true | menu bar clock |
| `menubar.clock.date` | `auto` `always` `never` | auto | menu bar clock |
| `menubar.recentItems` | int 0-50 (0 5 10 15 20 30 50 are the offered ones) | 10 | menu bar: recent items count |
| `menubar.autohide` | `never` `fullscreen` `always` | fullscreen | menu bar |
| `menubar.background` | bool | false | menu bar: translucent background |
| `wallpaper.image` `wallpaper.color` `wallpaper.id` | text | empty | wallpaper: the first of image (an existing file), color (`#rrggbb`), id (Amalfi set) that is usable; the default (Ravello) otherwise |
| `wallpaper.fit` | `fill` `fit` `stretch` `center` `tile` | fill | wallpaper |
| `lockscreen.showUser` | bool | true | lock screen: name and picture |
| `lockscreen.passwordDelay` | int, seconds, -1 never | 900 | Ossido (see below) |
| `screensaver.minutes` | int 0-1440 (0 never) | 0 | Ossido (see below) |
| `screensaver.saver` | `drift` `clock` `colors` | drift | Ossido (see below) |
| `notifications.previews` | `always` `unlocked` `never` | always | notifications |
| `notifications.grouping` | `auto` `app` `off` | auto | Notification Center |
| `notifications.apps.<app>.style` | `banners` `alerts` `none` | banners | notifications |
| `notifications.apps.<app>.previews` | `default` `always` `unlocked` `never` | default | notifications |
| `notifications.apps.<app>.grouping` | `default` `auto` `app` `off` | default | Notification Center |
| `notifications.apps.<app>.showInCenter` `.sound` | bool | true | notifications |
| `controlcenter.modules` | list of `id:size`; empty is the default arrangement, `none` no modules | empty | Control Center |
| `screenshot.destination` | `desktop` `documents` `clipboard` `folder:<path>` `app:<id>` | desktop | Screenshot |
| `screenshot.timer` | int 0-60 (0 5 10 offered) | 0 | Screenshot |
| `screenshot.thumbnail` `.rememberSelection` `.pointer` | bool | true, true, false | Screenshot |
| `screenshot.mode` | `screen` `window` `selection` | selection | Screenshot bar |
| `screenshot.selection` `screenshot.bar` | list of 4 / 2 numbers, or empty | empty | Screenshot bar |
| `screenshot.recentFolders` | list of paths | empty | Screenshot |

`<app>` is the desktop entry id or the app name that appears in the notification (`group` or
`appName`); an id may contain dots, so the field is whatever follows the last dot.

## Components that apply the keys

- **Dock** (`shell/lib/dock/options.dart`): `DockOptionsWatcher` rebuilds `DockOptions` from the
  `dock.*` keys when any of them changes. The Dock's own data, the pinned apps and the folders,
  stays in `dock.json` (it is not a setting).
- **Menu bar** (`menubar_config.dart`): `MenubarConfigWatcher` follows `menubar.*`.
- **Wallpaper** (`wallpaper.dart`): `WallpaperLayer` follows `wallpaper.*`; a lock screen shows
  the same layer.
- **Lock screen** (`lock_screen.dart`): follows `lockscreen.showUser` while it is on screen.
- **Notifications** (`notifications/settings.dart`): the settings are read again for every
  notification, so a change applies to the next one.
- **Control Center** (`controlcenter/modules.dart`): follows `controlcenter.modules`.
- **Screenshot** (`screenshot/settings.dart`): reads the keys when the layer starts and writes
  them back all at once when an option changes.

## What Ossido still reads from JSON

Ossido (the compositor) reads a few files itself (`docs/control.md`): `input.json`, `theme.json`,
`hotcorners.json`, `shortcuts.json`, `keyboard.json`, `sound.json`, `displays` and the Dock's
`titleBarDoubleClick`. Of the service's keys, four are Ossido's too: `dock.minimizeToApp`,
`lockscreen.passwordDelay`, `screensaver.minutes`, `screensaver.saver`. The daemon writes them
into `dock.json`, `lockscreen.json` and `screensaver.json` whenever they change (keeping the
other keys of those files), until Ossido reads the service. The other files are written by
Settings directly, as before.

## Migration

The first time `settings.bin` does not exist, the daemon imports the values of `dock.json`,
`menubar.json`, `wallpaper.json`, `lockscreen.json`, `screensaver.json`, `notifications.json`,
`controlcenter.json` and `screenshot.json` (older spellings included: `automatic` and `byApp`
groupings) and writes the binary file. The JSON files stay where they are and are not read again.
