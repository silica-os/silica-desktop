// The installed apps. From the session's app registry (silica-daemon's Launch Services,
// `SilicaApps`, silica-desktop docs/spec/launch-services.md) when it runs: desktop entries,
// app bundles, AppImages and Flatpak apps, kept current by its signals. Without it (tests,
// another session) from the .desktop files themselves (freedesktop Desktop Entry
// specification): the applications/ folders of XDG_DATA_HOME and XDG_DATA_DIRS, the first one
// wins on equal ids.

import 'dart:async';
import 'dart:collection' show UnmodifiableMapView;
import 'dart:io';

import 'package:flutter/foundation.dart' show ChangeNotifier;
import 'package:silica_ui/silica_ui.dart'
    show SilicaApps, SilicaAppsEvent, SilicaInstalledApp, xdgDataDirs;

import '../l10n/l10n.dart' show systemLanguage;

export 'package:silica_ui/silica_ui.dart' show xdgDataDirs;

/// A launchable app.
class DesktopEntry {
  const DesktopEntry({
    required this.id,
    required this.name,
    required this.exec,
    this.rawExec,
    this.icon,
    this.wmClass,
    this.keywords = const [],
    this.silicaIcon,
    this.categories = const [],
    this.actions = const [],
    this.mimeTypes = const [],
    this.kind = 'system',
    this.path = '',
    this.isNew = false,
  });

  /// File name without `.desktop` (e.g. `org.gnome.Nautilus`).
  final String id;
  final String name;

  /// Command without field codes (`%f`, `%U`, …), ready for `sh -c`.
  final String exec;

  /// Exec as written in the entry, with field codes: to open files ([execWithFiles]).
  final String? rawExec;
  final String? icon;
  final String? wmClass;
  final List<String> keywords;

  /// The layered silica-icons icon chosen by the app (`X-Silica-Icon`).
  final String? silicaIcon;

  /// The freedesktop categories (`Categories`, e.g. `Utility`, `Office`).
  final List<String> categories;

  /// The types of file it opens (`MimeType`).
  final List<String> mimeTypes;

  /// The entry's own actions (`Actions` and their `[Desktop Action …]` groups).
  final List<DesktopAction> actions;

  /// Where the app comes from: `system`, `bundle`, `flatpak` or `appimage`.
  final String kind;

  /// The bundle folder, the AppImage or the desktop entry file ('' when unknown).
  final String path;

  /// Installed and never launched (the Launcher's dot).
  final bool isNew;

  /// The window with this app id belongs to this app.
  bool matchesAppId(String appId) {
    if (appId.isEmpty) return false;
    final lower = appId.toLowerCase();
    return id.toLowerCase() == lower ||
        (wmClass?.toLowerCase() == lower) ||
        id.toLowerCase().split('.').last == lower ||
        exec.split(' ').first.split('/').last.toLowerCase() == lower;
  }
}

/// An action of an app (a `[Desktop Action <id>]` group): a name and its command.
class DesktopAction {
  const DesktopAction({
    required this.id,
    required this.name,
    required this.exec,
    this.icon,
  });

  final String id;
  final String name;

  /// Command without field codes, ready for `sh -c`.
  final String exec;
  final String? icon;
}

/// The command that opens `paths` with the app (Desktop Entry specification, "Exec"): `%f`/`%u`
/// with the first, `%F`/`%U` with all of them (`file://` URIs for `%u`/`%U`), quoted for `sh -c`.
/// Null if Exec takes no files: the app does not accept them.
String? execWithFiles(String exec, List<String> paths) {
  if (paths.isEmpty) return null;
  String quote(String s) => "'${s.replaceAll("'", r"'\''")}'";
  String uri(String p) => Uri.file(p).toString();
  final out = StringBuffer();
  var takes = false;
  for (var i = 0; i < exec.length; i++) {
    if (exec[i] != '%' || i + 1 >= exec.length) {
      out.write(exec[i]);
      continue;
    }
    i++;
    switch (exec[i]) {
      case '%':
        out.write('%');
      case 'f':
        takes = true;
        out.write(quote(paths.first));
      case 'F':
        takes = true;
        out.write(paths.map(quote).join(' '));
      case 'u':
        takes = true;
        out.write(quote(uri(paths.first)));
      case 'U':
        takes = true;
        out.write(paths.map((p) => quote(uri(p))).join(' '));
      default:
      // The other codes are dropped.
    }
  }
  if (!takes) return null;
  return out.toString().replaceAll(RegExp(r'\s+'), ' ').trim();
}

/// Removes the Exec field codes (`%f %F %u %U %d %D %n %N %i %c %k %v %m`) and `%%`.
String stripFieldCodes(String exec) {
  final out = StringBuffer();
  for (var i = 0; i < exec.length; i++) {
    if (exec[i] == '%' && i + 1 < exec.length) {
      if (exec[i + 1] == '%') out.write('%');
      i++;
      continue;
    }
    out.write(exec[i]);
  }
  return out.toString().replaceAll(RegExp(r'\s+'), ' ').trim();
}

/// The file manager: a system component, always first in the Dock, neither dragged nor
/// removed; Faro and the Launcher list it like any app.
// TODO(#142): start it with the session and restart it when it exits, once the embedder keeps
// an app alive without windows (today it would open a window at login).
const String fileManagerId = 'dev.frascella.Mustache';

/// Reads an entry; null if it is not an app to show (NoDisplay, Hidden, no Exec).
DesktopEntry? parseDesktopEntry(
  String id,
  String text, {
  String locale = 'en',
}) {
  final values = <String, String>{};
  final groups = <String, Map<String, String>>{};
  const actionHeader = '[Desktop Action ';
  Map<String, String>? group;
  for (final raw in text.split('\n')) {
    final line = raw.trim();
    if (line.startsWith('[')) {
      group = line == '[Desktop Entry]'
          ? values
          : line.startsWith(actionHeader) && line.endsWith(']')
          ? groups.putIfAbsent(
              line.substring(actionHeader.length, line.length - 1),
              () => {},
            )
          : null;
      continue;
    }
    if (group == null || line.startsWith('#')) continue;
    final eq = line.indexOf('=');
    if (eq <= 0) continue;
    group.putIfAbsent(
      line.substring(0, eq).trim(),
      () => line.substring(eq + 1).trim(),
    );
  }
  if (values['Type'] != 'Application' ||
      values['NoDisplay'] == 'true' ||
      values['Hidden'] == 'true') {
    return null;
  }
  final exec = values['Exec'];
  final name = values['Name[$locale]'] ?? values['Name'];
  if (exec == null || name == null) return null;
  return DesktopEntry(
    id: id,
    name: name,
    exec: stripFieldCodes(exec),
    rawExec: exec,
    icon: values['Icon'],
    wmClass: values['StartupWMClass'],
    keywords: (values['Keywords[$locale]'] ?? values['Keywords'] ?? '')
        .split(';')
        .where((k) => k.isNotEmpty)
        .toList(),
    silicaIcon: values['X-Silica-Icon'],
    categories: (values['Categories'] ?? '')
        .split(';')
        .where((c) => c.isNotEmpty)
        .toList(),
    mimeTypes: (values['MimeType'] ?? '')
        .split(';')
        .where((m) => m.isNotEmpty)
        .toList(),
    actions: [
      for (final action in (values['Actions'] ?? '').split(';'))
        if (groups[action] case final g?)
          if ((g['Name[$locale]'] ?? g['Name']) case final actionName?)
            if (g['Exec'] case final actionExec?)
              DesktopAction(
                id: action,
                name: actionName,
                exec: stripFieldCodes(actionExec),
                icon: g['Icon'],
              ),
    ],
  );
}

/// All installed apps, by id.
/// The installed apps, with names in `locale` (`Name[it]`), English by default.
Map<String, DesktopEntry> loadDesktopEntries({
  Map<String, String>? env,
  String locale = 'en',
}) {
  final entries = <String, DesktopEntry>{};
  final seen = <String>{};
  for (final dir in appDataDirs(env ?? Platform.environment)) {
    final apps = Directory('$dir/applications');
    if (!apps.existsSync()) continue;
    for (final file in apps.listSync(recursive: true).whereType<File>()) {
      if (!file.path.endsWith('.desktop')) continue;
      final relative = file.path.substring(apps.path.length + 1);
      final id = relative
          .substring(0, relative.length - '.desktop'.length)
          .replaceAll('/', '-');
      // The first folder wins, even if the app is hidden there.
      if (!seen.add(id)) continue;
      try {
        final entry = parseDesktopEntry(
          id,
          file.readAsStringSync(),
          locale: locale,
        );
        if (entry != null) entries[id] = entry;
      } on FileSystemException {
        continue;
      }
    }
  }
  return entries;
}

/// The entries of the registry's apps, with names in `locale`: the apps to show (no
/// `NoDisplay`), with a command for `sh -c`.
Map<String, DesktopEntry> entriesFromApps(
  Iterable<SilicaInstalledApp> apps, {
  String locale = 'en',
}) => {
  for (final app in apps)
    if (!app.noDisplay && app.exec.isNotEmpty)
      app.id: DesktopEntry(
        id: app.id,
        name: app.nameIn(locale),
        exec: stripFieldCodes(app.exec),
        rawExec: app.exec,
        icon: app.icon.isEmpty ? null : app.icon,
        wmClass: app.wmClass.isEmpty ? null : app.wmClass,
        keywords: app.keywordsIn(locale),
        silicaIcon: app.silicaIcon.isEmpty ? null : app.silicaIcon,
        categories: app.categories,
        mimeTypes: app.mimeTypes,
        actions: [
          for (final a in app.actions)
            DesktopAction(
              id: a.id,
              name: a.name,
              exec: stripFieldCodes(a.exec),
            ),
        ],
        kind: app.kind.name,
        path: app.path,
        isNew: app.isNew,
      ),
};

/// The data folders to look for apps and their icons in: the XDG ones, plus the Flatpak
/// exports (user and system) when the session's XDG_DATA_DIRS does not list them (a shell
/// started outside the login scripts that add them, #198).
List<String> appDataDirs(Map<String, String> env) {
  final dirs = xdgDataDirs(env);
  final home = env['HOME'] ?? '';
  final flatpak = [
    if (home.isNotEmpty) '$home/.local/share/flatpak/exports/share',
    '/var/lib/flatpak/exports/share',
  ];
  bool trimmed(String a, String b) =>
      a.replaceAll(RegExp(r'/+$'), '') == b.replaceAll(RegExp(r'/+$'), '');
  return [
    ...dirs,
    for (final dir in flatpak)
      if (!dirs.any((d) => trimmed(d, dir))) dir,
  ];
}

/// The installed apps, kept up to date: the `applications/` folders are watched (an app
/// installed or removed while the session runs, Flatpak included) and [entries] is reloaded in
/// place, so everyone holding it sees the change; listeners are told after each reload.
class DesktopEntryStore extends ChangeNotifier {
  DesktopEntryStore({
    this.locale = 'en',
    Map<String, String>? env,
    this.debounce = const Duration(milliseconds: 300),
    this.rescan = const Duration(seconds: 10),
    bool? service,
  }) : _env = env {
    _live.addAll(loadDesktopEntries(env: _env, locale: locale));
    _watch();
    // The registry, unless told otherwise; widget tests have no daemon.
    if (service ?? !Platform.environment.containsKey('FLUTTER_TEST')) {
      unawaited(_tryService());
    }
    // A folder that did not exist yet (the first Flatpak install creates its exports) cannot
    // be watched: look again now and then.
    // (A zero [rescan] turns the look off.)
    if (rescan > Duration.zero) {
      _timer = Timer.periodic(rescan, (_) {
        if (_fromService) return;
        final now = _existing();
        if (now.length != _watched.length || !now.every(_watched.containsKey)) {
          _watch();
          _schedule();
        } else if (_unwatchable.isNotEmpty) {
          _poll();
        }
      });
    }
  }

  final String locale;
  final Duration debounce;
  final Duration rescan;
  final Map<String, String>? _env;
  final Map<String, DesktopEntry> _live = {};
  late final Map<String, DesktopEntry> entries = UnmodifiableMapView(_live);
  final Map<String, StreamSubscription<FileSystemEvent>> _watched = {};
  // Folders inotify refused (the per-user instance or watch limit is spent, ENOSPC): they are
  // read again on every [rescan] tick instead, so installs still show up.
  final Set<String> _unwatchable = {};
  Timer? _timer;
  Timer? _reload;
  bool _disposed = false;

  /// Whether [entries] come from the registry (its signals keep them current; the folders are
  /// not watched).
  bool get fromService => _fromService;
  bool _fromService = false;
  StreamSubscription<SilicaAppsEvent>? _events;
  Timer? _serviceReload;

  Future<void> _tryService() async {
    // Subscribed before the first list, so that no change falls between the two.
    _events = SilicaApps.events().listen((_) {
      if (!_fromService) return;
      // A burst of changes (a Flatpak install) is one reload.
      _serviceReload?.cancel();
      _serviceReload = Timer(const Duration(milliseconds: 50), reload);
    });
    final apps = await SilicaApps.list();
    if (_disposed) return;
    if (apps == null) {
      await _events?.cancel();
      _events = null;
      return;
    }
    _fromService = true;
    for (final sub in _watched.values) {
      sub.cancel();
    }
    _watched.clear();
    _reload?.cancel();
    _apply(entriesFromApps(apps, locale: locale));
  }

  Future<void> _reloadFromService() async {
    final apps = await SilicaApps.list();
    if (_disposed) return;
    if (apps == null) {
      // The registry went away: back to the folders.
      _fromService = false;
      await _events?.cancel();
      _events = null;
      _apply(loadDesktopEntries(env: _env, locale: locale));
      _watch();
      return;
    }
    _apply(entriesFromApps(apps, locale: locale));
  }

  void _apply(Map<String, DesktopEntry> fresh) {
    if (_signature(fresh) == _signature(_live)) return;
    _live
      ..clear()
      ..addAll(fresh);
    notifyListeners();
  }

  Set<String> _existing() => {
    for (final dir in appDataDirs(_env ?? Platform.environment))
      if (Directory('$dir/applications').existsSync()) '$dir/applications',
  };

  void _watch() {
    final now = _existing();
    for (final path in _watched.keys.toList()) {
      if (!now.contains(path)) _watched.remove(path)?.cancel();
    }
    for (final path in now) {
      if (_watched.containsKey(path)) continue;
      try {
        _watched[path] = Directory(path)
            .watch(recursive: true)
            .listen(
              (_) => _schedule(),
              onError: (Object _) {
                _watched.remove(path)?.cancel();
                _unwatchable.add(path);
              },
            );
        _unwatchable.remove(path);
      } on FileSystemException {
        // Not watchable: the periodic look reads the folder instead.
        _unwatchable.add(path);
      }
    }
  }

  String _signature(Map<String, DesktopEntry> entries) =>
      (entries.values
              .map(
                (e) =>
                    '${e.id}\u0000${e.name}\u0000${e.exec}\u0000${e.isNew}'
                    '\u0000${e.silicaIcon}\u0000${e.path}',
              )
              .toList()
            ..sort())
          .join('\u0001');

  /// Polling fallback for folders that cannot be watched: reloads and tells listeners only when
  /// something changed.
  void _poll() {
    if (_disposed) return;
    final fresh = loadDesktopEntries(env: _env, locale: locale);
    if (_signature(fresh) == _signature(_live)) return;
    _live
      ..clear()
      ..addAll(fresh);
    notifyListeners();
  }

  void _schedule() {
    _reload?.cancel();
    _reload = Timer(debounce, reload);
  }

  /// Reads the apps again now (from the registry, or the folders).
  void reload() {
    if (_disposed) return;
    if (_fromService) {
      unawaited(_reloadFromService());
      return;
    }
    final fresh = loadDesktopEntries(env: _env, locale: locale);
    _live
      ..clear()
      ..addAll(fresh);
    notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _timer?.cancel();
    _reload?.cancel();
    _serviceReload?.cancel();
    _events?.cancel();
    for (final sub in _watched.values) {
      sub.cancel();
    }
    _watched.clear();
    super.dispose();
  }
}

/// The installed apps of this process, shared by every component and kept up to date by
/// watching the applications folders (#198, #200).
DesktopEntryStore get desktopEntryStore =>
    _desktopEntryStore ??= DesktopEntryStore(
      locale: systemLanguage(),
      // Widget tests build components in fake time: no timer left running.
      rescan: Platform.environment.containsKey('FLUTTER_TEST')
          ? Duration.zero
          : const Duration(seconds: 10),
    );
DesktopEntryStore? _desktopEntryStore;

/// The desktop entries: a read-only live view that changes when apps are installed or removed
/// (listen to [desktopEntryStore] to redraw).
Map<String, DesktopEntry> desktopEntries() => desktopEntryStore.entries;
