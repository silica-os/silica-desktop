// The Dock items: the pinned apps (~/.config/silica/dock.json), then the open ones that are
// not pinned, the separator, the Download folder and the trash. The open windows come from
// Ossido (ShellChannel).

import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:silica_ui/silica_ui.dart';

import '../l10n/app_localizations.dart';
import '../system/desktop_entries.dart';
import '../system/icon_lookup.dart';
import '../system/shell_channel.dart';
import '../system/silica_icons.dart';
import 'dock.dart' show shellQuote;
import 'model.dart';

/// The Dock configuration file (dock.json).
File dockConfigFile({Map<String, String>? env}) => _dockConfig(env);

File _dockConfig(Map<String, String>? env) {
  final e = env ?? Platform.environment;
  final config = e['XDG_CONFIG_HOME'] ?? '${e['HOME'] ?? ''}/.config';
  return File('$config/silica/dock.json');
}

/// The Dock configuration ($XDG_CONFIG_HOME/silica/dock.json, usually ~/.config):
/// `pinned`, `magnify`, `recents`, `autohide`, `position`, `minimizeEffect`, `indicators`
/// (options.dart).
Map<String, Object?> readDockConfig({Map<String, String>? env}) {
  final file = _dockConfig(env);
  try {
    if (!file.existsSync()) return {};
    final json = jsonDecode(file.readAsStringSync());
    return json is Map
        ? {for (final e in json.entries) '${e.key}': e.value}
        : {};
  } on FileSystemException {
    return {};
  } on FormatException {
    // Broken configuration: the defaults.
    return {};
  }
}

/// Changes one key of the configuration leaving the others.
void saveDockOption(String key, Object? value, {Map<String, String>? env}) {
  final file = _dockConfig(env);
  final config = readDockConfig(env: env)..[key] = value;
  try {
    file.parent.createSync(recursive: true);
    file.writeAsStringSync(jsonEncode(config));
  } on FileSystemException {
    // Without a writable configuration the choice lasts until the end of the session.
  }
}

/// A yes/no Dock option.
bool dockFlag(String key, {required bool fallback, Map<String, String>? env}) {
  final value = readDockConfig(env: env)[key];
  return value is bool ? value : fallback;
}

/// Saves the pinned apps ("Keep in Dock" and "Remove").
void savePinned(List<String> ids, {Map<String, String>? env}) =>
    saveDockOption('pinned', ids, env: env);

/// The Apps application (#148): a .desktop entry shipped with the shell (`shell/linux`) whose
/// Exec is `silica-action launcher.toggle`, right after the file manager on the reference. The
/// Dock runs the action itself (no process, no launch bounce, never a running dot) and keeps
/// the item even where the entry is missing.
const String appsItemId = 'dev.frascella.Apps';

/// Apps pinned when the configuration is missing: the first installed ones of this list.
const List<List<String>> defaultPinned = [
  [fileManagerId, 'org.gnome.Nautilus'],
  [appsItemId],
  [
    'firefox_firefox',
    'firefox',
    'org.mozilla.firefox',
    'chromium_chromium',
    'google-chrome',
  ],
  ['dev.frascella.Mail'],
  [
    'dev.frascella.Terminal',
    'org.gnome.Ptyxis',
    'org.gnome.Terminal',
    'org.gnome.Console',
  ],
  ['org.gnome.TextEditor', 'org.gnome.gedit'],
  ['code', 'code_code'],
  ['dev.frascella.Calculator', 'org.gnome.Calculator'],
  ['dev.frascella.Clock', 'org.gnome.clocks'],
  ['org.gnome.Calendar'],
  ['org.gnome.Settings', 'gnome-control-center'],
];

/// How many recently closed apps the recents section holds (D-08: 3 observed).
const int recentAppsMax = 3;

/// The pinned apps: from the configuration, or from the default list.
List<String> pinnedIds(
  Map<String, DesktopEntry> entries, {
  Map<String, String>? env,
}) {
  final pinned = readDockConfig(env: env)['pinned'];
  if (pinned is List) return [for (final id in pinned) '$id'];
  bool available(String id) => id == appsItemId || entries.containsKey(id);
  return [
    for (final choices in defaultPinned)
      if (choices.where(available).firstOrNull case final id?) id,
  ];
}

/// The Apps item: the Launcher.
Item appsItem(String label) => Item.app(
  label,
  layered: findSilicaIcon('apps'),
  iconPath: findIcon('view-app-grid-symbolic'),
  action: 'launcher.toggle',
  id: appsItemId,
  pinned: true,
);

/// The user's Trash folder (freedesktop Trash specification).
String trashFilesDir({Map<String, String>? env}) {
  final e = env ?? Platform.environment;
  final data = e['XDG_DATA_HOME'];
  final base = (data == null || data.isEmpty)
      ? '${e['HOME'] ?? ''}/.local/share'
      : data;
  return '$base/Trash/files';
}

/// The .desktop file of app `id` (the first data folder that has it), for "Show in" the
/// file manager.
String? desktopFilePath(String id, {Map<String, String>? env}) {
  for (final dir in appDataDirs(env ?? Platform.environment)) {
    final file = File('$dir/applications/$id.desktop');
    if (file.existsSync()) return file.path;
  }
  return null;
}

/// The Settings app ("Dock settings…" in the separator menu).
const String settingsAppId = 'dev.frascella.Settings';

/// The command that opens `path`: a folder in the file manager when it is installed, a file
/// with its default app (`gio open`, which the image has; `xdg-open` where xdg-utils is).
String openPathCommand(Map<String, DesktopEntry> entries, String path) {
  final quoted = shellQuote(path);
  if (FileSystemEntity.isDirectorySync(path)) {
    if (entries[fileManagerId]?.exec case final exec?) return '$exec $quoted';
  }
  return 'gio open $quoted 2>/dev/null || xdg-open $quoted';
}

/// The command that opens the Trash: the file manager on the Trash folder when it is installed
/// (it shows that folder as the Trash; created first, it may not exist yet).
String trashCommand(
  Map<String, DesktopEntry> entries, {
  Map<String, String>? env,
}) {
  if (entries[fileManagerId]?.exec case final exec?) {
    final dir = shellQuote(trashFilesDir(env: env));
    return 'mkdir -p $dir && $exec $dir';
  }
  return 'xdg-open trash:///';
}

Item _appItem(
  DesktopEntry entry,
  List<Toplevel> windows, {
  bool pinned = false,
  bool fixed = false,
  bool registered = false,
}) => Item.app(
  entry.name,
  // An app keeps running when its last window closes (#142): the registry says so.
  running: windows.isNotEmpty || registered,
  iconPath: findIcon(entry.icon),
  layered: silicaIconFor(entry),
  command: entry.exec,
  openFiles: entry.rawExec,
  windows: [for (final w in windows) w.id],
  id: entry.id,
  pinned: pinned,
  fixed: fixed,
);

/// The Dock items for these apps and these windows.
List<Item> dockItems(
  Map<String, DesktopEntry> entries,
  List<String> pinned,
  List<Toplevel> toplevels, {
  List<String> minimized = const [],
  Map<String, ui.Image> thumbnails = const {},
  List<String>? recents,
  String trashLabel = 'Trash',
  String appsLabel = 'Apps',
  Set<String> registered = const {},
  List<String>? folders,
}) {
  List<Toplevel> windowsOf(DesktopEntry entry) => [
    for (final t in toplevels)
      if (entry.matchesAppId(t.appId)) t,
  ];
  bool isRegistered(DesktopEntry entry) => registered.any(entry.matchesAppId);
  final items = <Item>[];
  final shown = <String>{};
  // The file manager first, whatever the configuration says (#141).
  if (entries[fileManagerId] case final manager?) {
    items.add(
      _appItem(
        manager,
        windowsOf(manager),
        pinned: true,
        fixed: true,
        registered: isRegistered(manager),
      ),
    );
    shown.add(fileManagerId);
  }
  for (final id in pinned) {
    if (id == appsItemId) {
      if (shown.add(id)) items.add(appsItem(appsLabel));
      continue;
    }
    if (shown.contains(id)) continue;
    final entry = entries[id];
    if (entry == null) continue;
    items.add(
      _appItem(
        entry,
        windowsOf(entry),
        pinned: true,
        registered: isRegistered(entry),
      ),
    );
    shown.add(id);
  }
  // With recent apps (D-08) a second separator after the pinned ones: between the two the
  // open apps that are not pinned, then the last closed ones until the section has 3.
  final recentSection = recents != null;
  final before = items.length;
  // Open apps that are not pinned, in the order they appeared.
  for (final window in toplevels) {
    final entry = entries.values
        .where((e) => e.matchesAppId(window.appId))
        .firstOrNull;
    if (entry == null || !shown.add(entry.id)) continue;
    items.add(
      _appItem(entry, windowsOf(entry), registered: isRegistered(entry)),
    );
  }
  // Running apps without windows (#142), after those with windows.
  for (final appId in registered.toList()..sort()) {
    final entry = entries.values
        .where((e) => e.matchesAppId(appId))
        .firstOrNull;
    if (entry == null || !shown.add(entry.id)) continue;
    items.add(_appItem(entry, const [], registered: true));
  }
  if (recentSection) {
    for (final id in recents) {
      if (items.length - before >= recentAppsMax) break;
      final entry = entries[id];
      if (entry == null || !shown.add(id)) continue;
      items.add(_appItem(entry, const []));
    }
    if (items.length > before) items.insert(before, const Item.separator());
  }
  final home = Platform.environment['HOME'] ?? '';
  final trashFull = trashHasFiles(home);
  final downloads = downloadDir(home);
  return [
    ...items,
    const Item.separator(),
    // The folders right of the separator (dock.json `folders`; the downloads one by default).
    for (final folder in folders ?? [downloads])
      Item.app(
        // The folder's own name, as on disk.
        folder.split('/').where((p) => p.isNotEmpty).lastOrNull ?? folder,
        folder: folder,
        iconPath: findIcon(folder == downloads ? 'folder-download' : 'folder'),
        command: openPathCommand(entries, folder),
      ),
    // The minimized windows, from the first minimized, before the Trash (D-08).
    for (final id in minimized)
      if (toplevels.where((t) => t.id == id).firstOrNull case final window?)
        _windowItem(window, entries, thumbnails[id]),
    Item.app(
      trashLabel,
      iconPath: findIcon(trashFull ? 'user-trash-full' : 'user-trash'),
      layered: findSilicaIcon(trashFull ? 'trash-full' : 'trash-empty'),
      command: trashCommand(entries),
      trash: true,
    ),
  ];
}

Item _windowItem(
  Toplevel window,
  Map<String, DesktopEntry> entries,
  ui.Image? thumbnail,
) {
  final entry = entries.values
      .where((e) => e.matchesAppId(window.appId))
      .firstOrNull;
  return Item.window(
    window.title.isNotEmpty ? window.title : (entry?.name ?? window.appId),
    window: window.id,
    thumbnail: thumbnail,
    iconPath: entry == null ? null : findIcon(entry.icon),
    layered: entry == null ? null : silicaIconFor(entry),
  );
}

/// A user folder from xdg-user-dirs (`XDG_<key>_DIR`), else the first of `fallbacks` that
/// exists in the home folder; null when there is none.
String? userDirectory(
  String key,
  String home,
  List<String> fallbacks, {
  Map<String, String>? env,
}) {
  try {
    final config =
        (env ?? Platform.environment)['XDG_CONFIG_HOME'] ?? '$home/.config';
    final dirs = File('$config/user-dirs.dirs');
    if (dirs.existsSync()) {
      for (final line in dirs.readAsLinesSync()) {
        if (line.startsWith('XDG_${key}_DIR=')) {
          final value = line
              .substring('XDG_${key}_DIR='.length)
              .replaceAll('"', '')
              .replaceAll(r'$HOME', home);
          if (value != home && Directory(value).existsSync()) return value;
        }
      }
    }
  } on FileSystemException {
    // Try the usual names.
  }
  for (final name in fallbacks) {
    if (Directory('$home/$name').existsSync()) return '$home/$name';
  }
  return null;
}

/// The places of the file manager's Dock menu (DOCK-02), sorted by their localized name:
/// applications, documents, downloads, home (the user name), recents, desktop. A null path
/// is a place Mustache can't show yet (Recents, mustache#11).
List<(String, String?)> fileManagerPlaces(
  String home,
  AppLocalizations l, {
  Map<String, String>? env,
}) {
  final user = home.split('/').where((p) => p.isNotEmpty).lastOrNull ?? home;
  // /Applications on Silica OS (silica-os#3), else the installed entries.
  final apps = Directory('/Applications').existsSync()
      ? '/Applications'
      : '/usr/share/applications';
  final places = <(String, String?)>[
    (l.placeApplications, apps),
    (
      l.placeDocuments,
      userDirectory('DOCUMENTS', home, ['Documents', 'Documenti'], env: env),
    ),
    (l.placeDownloads, downloadDir(home, env: env)),
    (user, home),
    (l.placeRecents, null),
    (
      l.placeDesktop,
      userDirectory('DESKTOP', home, ['Desktop', 'Scrivania'], env: env),
    ),
  ];
  places.sort((a, b) => a.$1.toLowerCase().compareTo(b.$1.toLowerCase()));
  return places;
}

/// The downloads folder: the xdg-user-dirs one, otherwise ~/Download or ~/Downloads.
String downloadDir(String home, {Map<String, String>? env}) {
  try {
    // xdg-user-dirs: in $XDG_CONFIG_HOME, if there.
    final config =
        (env ?? Platform.environment)['XDG_CONFIG_HOME'] ?? '$home/.config';
    final dirs = File('$config/user-dirs.dirs');
    if (dirs.existsSync()) {
      for (final line in dirs.readAsLinesSync()) {
        if (line.startsWith('XDG_DOWNLOAD_DIR=')) {
          final value = line
              .substring('XDG_DOWNLOAD_DIR='.length)
              .replaceAll('"', '')
              .replaceAll(r'$HOME', home);
          if (Directory(value).existsSync()) return value;
        }
      }
    }
  } on FileSystemException {
    // Try the usual names.
  }
  for (final name in ['Download', 'Downloads', 'Scaricati']) {
    if (Directory('$home/$name').existsSync()) return '$home/$name';
  }
  return '$home/Download';
}

/// The user's trash contains something (freedesktop Trash specification).
bool trashHasFiles(String home) {
  try {
    final files = Directory('$home/.local/share/Trash/files');
    return files.existsSync() && files.listSync().isNotEmpty;
  } on FileSystemException {
    return false;
  }
}
