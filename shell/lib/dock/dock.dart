// The shell's Dock: layout and magnification from model.dart (measurements in
// plan/reference/dock.md), lens glass background computed by Ossido, icons, separator,
// indicators and label drawn here.

import 'dart:async';
import 'dart:math' as math;
import 'dart:io'
    show
        Directory,
        File,
        FileSystemEntity,
        FileSystemEvent,
        FileSystemException,
        Platform,
        Process,
        ProcessException;
import 'dart:ui' as ui;

import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart'
    show HardwareKeyboard, KeyDownEvent, KeyEvent, LogicalKeyboardKey;
import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart' show SvgPicture;
import 'package:silica_ui/silica_ui.dart';

import 'package:flutter/gestures.dart' show kPrimaryButton, kSecondaryButton;

import '../l10n/l10n.dart';
import '../system/desktop_entries.dart';
import '../system/glass_profile.dart';
import '../system/layer_control.dart';
import '../system/shadow_image.dart';
import '../system/app_registry.dart';
import '../system/shell_channel.dart';
import '../system/sounds.dart';
import 'apps.dart';
import 'autostart.dart';
import 'arrow.dart';
import 'dock_menu.dart';
import 'launcher_entry.dart';
import 'options.dart';
import 'recent_documents.dart';
import 'icons.dart';
import 'stack.dart';
import 'stack_settings.dart';
import 'trash_alert.dart';
import 'glass_groups.dart';
import 'model.dart';

/// The Dock with the real apps: installed (.desktop), pinned and open (from Ossido).
class LiveDock extends StatefulWidget {
  const LiveDock({super.key});

  @override
  State<LiveDock> createState() => _LiveDockState();
}

class _LiveDockState extends State<LiveDock> {
  /// This component's layer surface (several share one engine, #201).
  late final LayerControl _layer;

  /// The installed apps, live: the Dock redraws when one is installed or removed (#199).
  late final Map<String, DesktopEntry> _entries = desktopEntries();
  late final List<String> _pinned = pinnedIds(_entries);

  /// The folders right of the separator (dock.json `folders`), the downloads one by default.
  late final List<String> _folders = () {
    final saved = readDockConfig()['folders'];
    if (saved is List) return [for (final f in saved) '$f'];
    return [downloadDir(Platform.environment['HOME'] ?? '')];
  }();

  void _setFolders(void Function(List<String> folders) change) {
    setState(() => change(_folders));
    saveDockOption('folders', _folders);
    _watchFolders();
  }

  /// One non-recursive watch per Dock folder: when its items change the Dock redraws, so a
  /// folder that gains its first item (or loses its last) switches between the plain and the
  /// full icon.
  final Map<String, StreamSubscription<FileSystemEvent>> _folderWatches = {};
  Timer? _folderRedraw;

  void _watchFolders() {
    for (final path in _folderWatches.keys.toList()) {
      if (!_folders.contains(path)) _folderWatches.remove(path)?.cancel();
    }
    for (final path in _folders) {
      if (_folderWatches.containsKey(path)) continue;
      try {
        _folderWatches[path] = Directory(path).watch().listen((event) {
          // The folder's or an item's colour or symbol may have changed (an attribute change
          // is a modify event): the icons are asked again.
          dockIconRevision++;
          _folderRedraw?.cancel();
          _folderRedraw = Timer(const Duration(milliseconds: 150), () {
            if (mounted) setState(() {});
          });
        });
      } on FileSystemException {
        // No notifications for this folder: it is checked at every redraw.
      }
    }
  }

  /// Dock options (dock.json), reloaded when Settings changes them.
  final DockOptionsWatcher _options = DockOptionsWatcher();

  /// Badges, progress and attention asked by the apps (LauncherEntry on D-Bus).
  final LauncherEntries _launcher = LauncherEntries();
  DockOptions get _o => _options.value.value;
  bool get _magnify => _o.magnify;
  bool get _recentsOn => _o.recents;

  /// The apps open at the last change and those closed recently, from the most recent.
  Set<String> _running = {};
  final List<String> _closed = [];

  /// Windows from the most recent (focus): a click on the app raises the last one used.
  final List<String> _recent = [];

  /// The "Empty Trash" alert is open.
  bool _confirmEmpty = false;

  /// The thumbnails of minimized windows, requested from Ossido when minimizing.
  final Map<String, ui.Image> _thumbnails = {};

  /// The stack settings, by folder path.
  final Map<String, StackSettings> _stacks = loadStackSettings();

  /// The saved settings of a folder; the defaults otherwise: by name, but the Downloads folder
  /// by Date Added (newest first).
  StackSettings _stackOf(String folder) =>
      _stacks[folder] ?? defaultStackSettings(folder);

  void _setStack(String folder, StackSettings settings) {
    setState(() => _stacks[folder] = settings);
    saveStackSettings(_stacks);
  }

  void _openPath(String path) =>
      ShellChannel.instance.launch(openPathCommand(_entries, path));

  /// The settings of a stack as menu items (D-04): "Sort by", "Show as",
  /// "View content as", with the check mark on the choice.
  List<MenuEntry> _stackOptions(String folder) {
    final s = _stackOf(folder);
    final l = l10nOf(context);
    MenuAction pick(String label, bool checked, StackSettings next) =>
        MenuAction(
          label,
          checked: checked,
          onSelected: () => _setStack(folder, next),
        );
    return [
      MenuSection(l.stackSortBy),
      for (final (sort, label) in [
        (StackSort.name, l.sortName),
        (StackSort.added, l.sortDateAdded),
        (StackSort.modified, l.sortDateModified),
        (StackSort.created, l.sortDateCreated),
        (StackSort.kind, l.sortKind),
      ])
        pick(label, s.sort == sort, s.copyWith(sort: sort)),
      const MenuDivider(),
      MenuSection(l.stackShowAs),
      for (final (display, label) in [
        (StackDisplay.folder, l.stackFolder),
        (StackDisplay.stack, l.stackStack),
      ])
        pick(label, s.display == display, s.copyWith(display: display)),
      const MenuDivider(),
      MenuSection(l.stackViewContentAs),
      for (final (content, label) in [
        (StackContent.fan, l.stackFan),
        (StackContent.grid, l.stackGrid),
        (StackContent.list, l.stackList),
        (StackContent.automatic, l.stackAutomatic),
      ])
        pick(label, s.content == content, s.copyWith(content: content)),
    ];
  }

  /// The menu of a folder: the stack settings, Options, Open.
  List<MenuEntry> _folderMenu(Item item, String folder) {
    final l = l10nOf(context);
    return [
      ..._stackOptions(folder),
      const MenuDivider(),
      MenuAction(
        l.options,
        submenu: [
          MenuAction(
            l.removeFromDock,
            onSelected: () => _setFolders((f) => f.remove(folder)),
          ),
          MenuAction(
            l.dockShowInFileManager,
            onSelected: () => _openPath(File(folder).parent.path),
          ),
        ],
      ),
      const MenuDivider(),
      MenuAction(l.openNamed(item.label), onSelected: () => _openPath(folder)),
    ];
  }

  /// The List view of a stack: a menu with a small icon and the name of every file, then
  /// Options (the stack settings) and Open (dock-stacks.md › List).
  List<MenuEntry> _stackListMenu(Item item, List<FileSystemEntity> entries) {
    final folder = item.folder;
    if (folder == null) return const [];
    return [
      for (final entry in entries)
        MenuAction(
          fileName(entry.path),
          icon: (color) => SizedBox(
            width: 16,
            height: 16,
            child: switch (fileIcon(entry)) {
              final source? => SilicaIcon(source, side: 16),
              null => null,
            },
          ),
          onSelected: () => _openPath(entry.path),
        ),
      if (entries.isNotEmpty) const MenuDivider(),
      MenuAction(l10nOf(context).options, submenu: _stackOptions(folder)),
      MenuAction(
        l10nOf(context).openNamed(item.label),
        onSelected: () => _openPath(folder),
      ),
    ];
  }

  @override
  void initState() {
    super.initState();
    _layer = LayerControl.of(context);
    desktopEntryStore.addListener(_entriesChanged);
    ShellChannel.instance.toplevels.addListener(_changed);
    AppRegistry.instance.apps.addListener(_changed);
    AppRegistry.instance.start();
    ShellChannel.instance.focused.addListener(_focused);
    ShellChannel.instance.minimized.addListener(_minimized);
    ShellChannel.instance.refresh();
    _options.value.addListener(_optionsChanged);
    _applyOptions();
    _launcher.value.addListener(_launcherChanged);
    _launcher.start();
    _watchFolders();
    // Login items: Silica has no other runner; once per session.
    runAutostartOnce();
  }

  void _launcherChanged() => setState(() {});

  /// An app was installed or removed: the running ones get their tiles, the pinned ones that
  /// appeared theirs.
  void _entriesChanged() => _changed();

  /// The separator was dragged to a new size: the same key Settings writes (dock.json `tileSize`);
  /// the magnified size never goes below it.
  void _resized(double canvas) {
    saveDockOption('tileSize', canvas.round());
    if (_o.magnifiedSize < canvas) {
      saveDockOption('magnifiedSize', canvas.round());
    }
    _options.reload();
  }

  void _optionsChanged() {
    setState(() {});
    _applyOptions();
  }

  /// What the options ask of the compositor: the room the Dock keeps (none while it hides
  /// itself) and the minimize effect.
  void _applyOptions() {
    _layer.exclusiveZone(
      _o.autohide ? 0 : DockMetrics(_o.tileSize).backgroundHeight + bottomGap,
    );
    ShellChannel.instance.execute('window.minimize_effect', {
      'effect': _o.minimizeEffect.name,
    });
  }

  void _minimized() {
    final ids = ShellChannel.instance.minimized.value;
    _thumbnails.removeWhere((id, image) {
      final gone = !ids.contains(id);
      if (gone) image.dispose();
      return gone;
    });
    for (final id in ids) {
      if (_thumbnails.containsKey(id)) continue;
      ShellChannel.instance.thumbnail(id).then((image) {
        if (!mounted || image == null) return;
        if (ShellChannel.instance.minimized.value.contains(id)) {
          setState(() => _thumbnails[id] = image);
        } else {
          image.dispose();
        }
      });
    }
    setState(() {});
  }

  @override
  void dispose() {
    desktopEntryStore.removeListener(_entriesChanged);
    ShellChannel.instance.toplevels.removeListener(_changed);
    AppRegistry.instance.apps.removeListener(_changed);
    ShellChannel.instance.focused.removeListener(_focused);
    ShellChannel.instance.minimized.removeListener(_minimized);
    _options.value.removeListener(_optionsChanged);
    _options.dispose();
    _launcher.value.removeListener(_launcherChanged);
    _launcher.dispose();
    _folderRedraw?.cancel();
    for (final watch in _folderWatches.values) {
      watch.cancel();
    }
    for (final image in _thumbnails.values) {
      image.dispose();
    }
    super.dispose();
  }

  void _changed() {
    // An app with no more windows joins the recently closed ones.
    final now = {
      for (final appId in [
        for (final t in ShellChannel.instance.toplevels.value) t.appId,
        ...AppRegistry.instance.apps.value,
      ])
        if (_entries.values.where((e) => e.matchesAppId(appId)).firstOrNull
            case final entry?)
          entry.id,
    };
    for (final id in _running.difference(now)) {
      _closed
        ..remove(id)
        ..insert(0, id);
    }
    _closed.removeWhere(now.contains);
    if (_closed.length > 10) _closed.removeRange(10, _closed.length);
    _running = now;
    setState(() {});
  }

  void _setOption(String key, Object value) {
    saveDockOption(key, value);
    _options.reload();
  }

  /// The separator menu (D-04): hiding, magnification, position, minimize effect, then the
  /// Dock settings. The side positions are not implemented yet (#72, #73): shown disabled.
  List<MenuEntry> _separatorMenu() {
    final l = l10nOf(context);
    final settings = _entries[settingsAppId];
    return [
      MenuAction(
        _o.autohide ? l.dockHidingOff : l.dockHidingOn,
        onSelected: () => _setOption('autohide', !_o.autohide),
      ),
      MenuAction(
        _magnify ? l.magnificationOff : l.magnificationOn,
        onSelected: () => _setOption('magnify', !_magnify),
      ),
      MenuAction(
        l.dockPositionOnScreen,
        submenu: [
          for (final (position, label) in [
            (DockPosition.left, l.dockPositionLeft),
            (DockPosition.bottom, l.dockPositionBottom),
            (DockPosition.right, l.dockPositionRight),
          ])
            MenuAction(
              label,
              checked: _o.position == position,
              enabled: position == DockPosition.bottom,
              onSelected: () => _setOption('position', position.name),
            ),
        ],
      ),
      MenuAction(
        l.dockMinimizeEffect,
        submenu: [
          for (final (effect, label) in [
            (MinimizeEffect.genie, l.dockEffectGenie),
            (MinimizeEffect.scale, l.dockEffectScale),
          ])
            MenuAction(
              label,
              checked: _o.minimizeEffect == effect,
              onSelected: () => _setOption('minimizeEffect', effect.name),
            ),
        ],
      ),
      if (settings?.exec case final exec?) ...[
        const MenuDivider(),
        MenuAction(
          l.dockSettingsEllipsis,
          onSelected: () => ShellChannel.instance.launch(exec),
        ),
      ],
    ];
  }

  void _focused() {
    final id = ShellChannel.instance.focused.value;
    if (id.isEmpty) return;
    // An app asking attention stops when it comes to the front.
    final appId = ShellChannel.instance.toplevels.value
        .where((t) => t.id == id)
        .firstOrNull
        ?.appId;
    if (appId != null) {
      for (final entry in _entries.values) {
        if (entry.matchesAppId(appId)) _launcher.activated(entry.id);
      }
    }
    _recent
      ..remove(id)
      ..insert(0, id);
  }

  /// The window of the app used last.
  String _latest(Item item) {
    for (final id in _recent) {
      if (item.windows.contains(id)) return id;
    }
    return item.windows.first;
  }

  void _activate(Item item) {
    // A minimized window comes back itself; an app with all windows minimized brings back
    // the last minimized (D-03); an open app comes to the front; otherwise it launches.
    final minimized = ShellChannel.instance.minimized.value;
    if (item.action case final action?) {
      ShellChannel.instance.execute(action);
    } else if (item.minimizedWindow case final window?) {
      ShellChannel.instance.activate(window);
    } else if (item.windows.isNotEmpty &&
        item.windows.every(minimized.contains)) {
      ShellChannel.instance.activate(
        minimized.lastWhere(item.windows.contains),
      );
    } else if (item.windows.isNotEmpty) {
      // D-03 (a): all the app's windows come to the front keeping their relative order
      // (raised from the least recent, the most recent last); minimized ones stay in the
      // Dock (e′).
      final visible = [
        for (final id in item.windows)
          if (!minimized.contains(id)) id,
      ];
      int age(String id) {
        final i = _recent.indexOf(id);
        return i < 0 ? _recent.length : i;
      }

      visible.sort((a, b) => age(b).compareTo(age(a)));
      for (final id in visible) {
        ShellChannel.instance.activate(id);
      }
    } else if (item.command case final command?) {
      ShellChannel.instance.launch(command);
    }
  }

  /// Files dropped from another app (dock.md D-05, MU-40): on the Trash they end up there, on a
  /// stack they move into the folder, on an app they open with it.
  Future<void> _dropFiles(Item item, List<String> paths) async {
    if (item.trash) {
      // The Trash sound comes once the items are in (FILE-SOUNDS), not at the release.
      await trashDropped(paths);
    } else if (item.folder case final folder?) {
      try {
        final moved = await Process.run('mv', ['--', ...paths, folder]);
        if (moved.exitCode == 0) unawaited(playUiSound(UiSound.fileTransfer));
      } on ProcessException {
        // No mv: the files stay where they are.
      }
    } else if (item.openFiles case final exec?) {
      final command = execWithFiles(exec, paths);
      if (command != null) ShellChannel.instance.launch(command);
    }
    if (mounted) setState(() {});
  }

  void _setPinned(Item item, bool pinned) {
    final id = item.id;
    if (id == null) return;
    setState(() {
      _pinned.remove(id);
      if (pinned) _pinned.add(id);
    });
    savePinned(_pinned);
  }

  /// The menu of an icon (dock.md, D-04): the open windows, Options, then the actions.
  List<MenuEntry> _menu(Item item) {
    final shell = ShellChannel.instance;
    final l = l10nOf(context);
    final toplevels = shell.toplevels.value;
    if (item.minimizedWindow case final window?) {
      return [
        MenuAction(
          l.openNamed(item.label),
          onSelected: () => shell.activate(window),
        ),
      ];
    }
    if (item.kind == ItemKind.separator) return _separatorMenu();
    if (item.folder case final folder?) return _folderMenu(item, folder);
    // The launcher item (DOCK-02): Remove from Dock — Show Apps, nothing else.
    if (item.action case final action?) {
      return [
        MenuAction(l.removeFromDock, onSelected: () => _setPinned(item, false)),
        const MenuDivider(),
        MenuAction(l.dockShowApps, onSelected: () => shell.execute(action)),
      ];
    }
    if (item.fixed) return _fileManagerMenu(item);
    // Trash: Open; when full also Empty (D-07).
    if (item.id == null) {
      return [
        if (item.command case final command?)
          MenuAction(l.open, onSelected: () => shell.launch(command)),
        if (item.trash &&
            trashHasFiles(Platform.environment['HOME'] ?? '')) ...[
          const MenuDivider(),
          MenuAction(
            l.emptyTrashEllipsis,
            onSelected: () => setState(() => _confirmEmpty = true),
          ),
        ],
      ];
    }
    // The menu of an app (D-04): its recent documents, its windows (the front one
    // checked), Options, then Show All Windows, Hide, Quit; with Option held Hide becomes Hide
    // Others and Quit becomes Force Quit. A closed app: Options, Open.
    final entry = item.id == null ? null : _entries[item.id];
    final alt = HardwareKeyboard.instance.isAltPressed;
    final front = shell.focused.value;
    final opener = item.openFiles;
    final documents = entry == null || opener == null
        ? const <RecentDocument>[]
        : recentDocuments(entry);
    final appId =
        toplevels
            .where((t) => item.windows.contains(t.id))
            .firstOrNull
            ?.appId ??
        item.id;
    return [
      for (final document in documents)
        MenuAction(
          fileName(document.path),
          onSelected: () {
            if (execWithFiles(opener!, [document.path]) case final command?) {
              shell.launch(command);
            }
          },
        ),
      if (documents.isNotEmpty) const MenuDivider(),
      for (final id in item.windows)
        MenuAction(
          toplevels.where((t) => t.id == id).firstOrNull?.title ?? item.label,
          checked: id == front,
          onSelected: () => shell.activate(id),
        ),
      if (item.windows.isNotEmpty) const MenuDivider(),
      MenuAction(l.options, submenu: _appOptions(item, entry)),
      const MenuDivider(),
      if (item.running) ...[
        MenuAction(
          l.showAllWindows,
          enabled: item.windows.isNotEmpty,
          onSelected: () {
            shell.activate(_latest(item));
            shell.execute('panorama.app');
          },
        ),
        // App hiding is not in Ossido yet (#143).
        MenuAction(alt ? l.dockHideOthers : l.dockHide, enabled: false),
        if (alt)
          MenuAction(
            l.dockForceQuit,
            onSelected: () =>
                shell.execute('app.force_quit', {'app_id': appId}),
          )
        else
          MenuAction(
            l.quit,
            onSelected: () {
              if (item.windows.isEmpty) {
                // Running without windows (#142): quit it by its app id.
                shell.execute('app.quit', {'app_id': item.id});
              } else {
                shell.activate(_latest(item));
                shell.execute('app.quit');
              }
            },
          ),
      ] else
        MenuAction(l.open, onSelected: () => _activate(item)),
    ];
  }

  /// Options of an app (D-04): Keep in Dock / Remove from Dock, Open at Login, Show in the
  /// file manager, Assign To (not in Ossido yet, #144).
  List<MenuEntry> _appOptions(Item item, DesktopEntry? entry) {
    final l = l10nOf(context);
    final id = item.id;
    final desktopFile = id == null ? null : desktopFilePath(id);
    return [
      if (item.pinned)
        MenuAction(l.removeFromDock, onSelected: () => _setPinned(item, false))
      else
        MenuAction(l.keepInDock, onSelected: () => _setPinned(item, true)),
      if (id != null && entry != null)
        MenuAction(
          l.dockOpenAtLogin,
          checked: opensAtLogin(id),
          onSelected: () => setOpensAtLogin(
            id,
            entry.exec,
            entry.name,
            on: !opensAtLogin(id),
          ),
        ),
      if (desktopFile != null)
        MenuAction(
          l.dockShowInFileManager,
          onSelected: () => _openPath(File(desktopFile).parent.path),
        ),
      const MenuDivider(),
      MenuAction(l.dockAssignTo, enabled: false),
    ];
  }

  /// The file manager's menu (#141, DOCK-02): its windows (the front one checked) — New
  /// Window, New Smart Folder, Find — Go to Folder, Connect to Server — six places — Show All
  /// Windows, Hide. No Remove and no Quit: it always runs. What Mustache or Ossido can't do
  /// yet is shown disabled (mustache#11, silica-desktop#143).
  List<MenuEntry> _fileManagerMenu(Item item) {
    final shell = ShellChannel.instance;
    final l = l10nOf(context);
    final toplevels = shell.toplevels.value;
    final front = shell.focused.value;
    final home = Platform.environment['HOME'] ?? '/';
    return [
      for (final id in item.windows)
        MenuAction(
          toplevels.where((t) => t.id == id).firstOrNull?.title ?? item.label,
          checked: id == front,
          onSelected: () => shell.activate(id),
        ),
      if (item.windows.isNotEmpty) const MenuDivider(),
      if (item.command case final command?)
        MenuAction(l.newWindow, onSelected: () => shell.launch(command)),
      MenuAction(l.dockNewSmartFolder, enabled: false),
      MenuAction(l.dockFind, enabled: false),
      const MenuDivider(),
      MenuAction(l.dockGoToFolder, enabled: false),
      MenuAction(l.dockConnectToServer, enabled: false),
      const MenuDivider(),
      for (final (label, path) in fileManagerPlaces(home, l))
        MenuAction(
          label,
          enabled: path != null,
          onSelected: path == null ? null : () => _openPath(path),
        ),
      const MenuDivider(),
      MenuAction(
        l.showAllWindows,
        enabled: item.windows.isNotEmpty,
        onSelected: () {
          shell.activate(_latest(item));
          shell.execute('panorama.app');
        },
      ),
      MenuAction(l.dockHide, enabled: false),
    ];
  }

  @override
  Widget build(BuildContext context) => Dock(
    items: dockItems(
      _entries,
      _pinned,
      ShellChannel.instance.toplevels.value,
      minimized: ShellChannel.instance.minimized.value,
      thumbnails: _thumbnails,
      recents: _recentsOn ? _closed : null,
      trashLabel: l10nOf(context).trash,
      appsLabel: l10nOf(context).dockApps,
      registered: AppRegistry.instance.apps.value,
      folders: _folders,
    ),
    canvas: _o.tileSize,
    magnified: _magnify ? _o.magnifiedSize : null,
    onResize: _resized,
    autohide: _o.autohide,
    indicators: _o.indicators,
    revealAtStart: true,
    launcher: _launcher.value.value,
    onActivate: _activate,
    onDropFiles: _dropFiles,
    onAddFolders: (paths, index) => _setFolders((f) {
      final at = index.clamp(0, f.length);
      for (final path in paths.reversed) {
        f.remove(path);
        f.insert(at.clamp(0, f.length), path);
      }
    }),
    onSpringLoad: (item) {
      // Spring-loading (MU-40): the app's windows, to drop into one of them.
      if (item.windows.isNotEmpty) {
        ShellChannel.instance.activate(_latest(item));
        ShellChannel.instance.execute('panorama.app');
      }
    },
    menuFor: _menu,
    onOpenPath: _openPath,
    stackSettingsFor: _stackOf,
    stackListMenu: _stackListMenu,
    alert: _confirmEmpty
        ? TrashAlert(
            onEmpty: () async {
              setState(() => _confirmEmpty = false);
              await emptyTrash();
              if (mounted) setState(() {});
              // ~0.2 s after the Trash icon turns empty (FILE-SOUNDS).
              unawaited(soundTrashEmptied());
            },
            onCancel: () => setState(() => _confirmEmpty = false),
          )
        : null,
    onReorder: (order, dropped) {
      // The pinned ones in the new order; an open app dropped among them becomes pinned.
      final ids = [
        for (final item in order)
          if (item.id case final id?
              when item.pinned || identical(item, dropped))
            id,
      ];
      setState(() {
        _pinned
          ..clear()
          ..addAll(ids);
      });
      savePinned(_pinned);
    },
  );
}

/// Colors of the light appearance (dock.md).
const Color separatorColor = Color(0x591D1E21);
const Color indicatorColor = Color(0xFF5F636B);

/// Icon menu (D-04): at ~530 ms of pressing the label hides and the menu opens (the measured
/// 583-613 ms is end to end, including the input pipeline and compositing, so it is not added
/// on top); the press becomes a drag beyond 5 pt; the tip touches ~11 pt above the bottom of
/// the Dock. A click and a short press leave the label as it is; a right click hides it at
/// once. Once hidden for a menu it stays hidden until the pointer leaves the icon.
const Duration longPressMenu = Duration(milliseconds: 530);
const Duration longPressLabelHide = longPressMenu;
const double dragThreshold = 5;
const double menuTipAboveDock = 11;

/// Room above the menu for its shadow when the surface grows.
const double menuShadowRoom = 30;

/// The Dock input region: the background, plus what magnification and hysteresis need.
const double dockInputMargin = 60;
const double dockInputAbove = 60;

/// Dragging (D-05): the empty slot, the neighbours sliding (~265 ms, S-shaped), "Remove"
/// at ~130 pt above the grab point, fade of the removed item (~250 ms). The pointer counts
/// as "in the Dock" up to 20 pt above the bottom.
const Duration gapSlide = Duration(milliseconds: 265);
const double removeAbove = 130;
const Duration removeFade = Duration(milliseconds: 250);
const double dragInsideAbove = 20;

/// Auto-hide (SH-DK-07): delay before revealing, slide up, slide down.
const Duration autohideDelay = Duration(milliseconds: 190);
const Duration autohideReveal = Duration(milliseconds: 190);
const Duration autohideHide = Duration(milliseconds: 175);

/// Session start (provisional: the reference's login was not measured): the Dock waits below
/// the screen edge until every icon is decoded, then slides up like the auto-hide reveal.
const Duration sessionReveal = autohideReveal;

/// The longest wait for the icons at session start (a safety net for an icon that never
/// loads, not a measurement).
const Duration iconsWaitLimit = Duration(seconds: 3);

/// Spring-loading on an app (MU-40): ~830 ms of a file held over it.
const Duration springLoadDelay = Duration(milliseconds: 830);

/// The empty slot where the dragged icon would fall.
const Item gapItem = Item.app('');

/// A slot of the Dock: an item and how open its slot is. While `leaving` the item is no longer
/// in the list given to the Dock; it stays until the slot has closed (DOCK-ANIM).
class _Slot {
  _Slot(this.item, this.key, {required bool opening})
    : from = opening ? launchStartScale : 1,
      t = opening ? 0 : 1,
      hop = opening;

  Item item;
  final String key;

  /// Presence at the start of the current phase, and how far the phase has gone (0-1).
  double from;
  double t;

  /// Milliseconds since the phase started; the launch bounce and the dot follow it.
  double age = 0;
  bool leaving = false;

  /// A launch: the icon makes one hop with the slot.
  bool hop;

  /// Cell scale, 1 at rest: it scales the width, the icon and its opacity.
  double get presence => leaving
      ? from * (1 - quitCurve.transform(t))
      : from + (1 - from) * launchCurve.transform(t);

  /// Icon opacity: from 20 % of the size when opening, the square of the size when closing.
  double get opacity {
    final p = presence;
    return leaving ? p * p : ((p - 0.2) / 0.8).clamp(0.0, 1.0);
  }

  /// The hop as a fraction of its peak (0 when not hopping).
  double get hopFraction => hop && !leaving ? launchHop(age) : 0;

  /// The running dot: it fades in at the end of a launch and out at the start of a quit.
  double get dot => leaving
      ? (1 - age / quitDotFade.inMilliseconds).clamp(0.0, 1.0)
      : hop
      ? ((age - launchDotStart.inMilliseconds) / launchDotFade.inMilliseconds)
            .clamp(0.0, 1.0)
      : 1;

  bool get active => leaving
      ? t < 1
      : t < 1 || (hop && age < launchHopDuration.inMilliseconds);

  void startLeaving() {
    from = presence;
    leaving = true;
    t = 0;
    age = 0;
  }

  void startOpening() {
    from = presence;
    leaving = false;
    t = 0;
    age = 0;
    hop = false;
  }

  void advance(double ms) {
    age += ms;
    final duration = leaving ? quitDuration : launchDuration;
    t = (t + ms / duration.inMilliseconds).clamp(0.0, 1.0);
  }
}

/// A drag: the item taken, the grab point in the icon, where the pointer is.
class _DockDrag {
  _DockDrag({
    required this.index,
    required this.item,
    required this.grab,
    required this.side,
    required this.downY,
    required this.pointer,
  });

  final int index;
  final Item item;
  final Offset grab;
  final double side;
  final double downY;
  Offset pointer;
  bool started = false;
  bool inside = true;
  bool remove = false;
}

/// The removed item fading out: icon, position, side and start.
class _Ghost {
  _Ghost(this.item, this.at, this.side);
  final Item item;
  final Offset at;
  final double side;
  Duration elapsed = Duration.zero;
}

/// An argument for `sh -c`, in single quotes.
String shellQuote(String text) => "'${text.replaceAll("'", r"'\''")}'";

/// Minimized windows in the Dock (D-08): 57 × 47 thumbnail in the 63 cell, an 18 pt app icon
/// sticking out 4 pt below the thumbnail.
const double minimizedThumbWidth = 57;
const double minimizedThumbHeight = 47;
const double minimizedBadge = 18;
const double minimizedBadgeBelow = 4;

/// Pressed icon: darkened (estimate, the value is not measured).
const Color pressedShade = Color(0x40000000);

/// The Dock colors drawn by the shell, light and dark (dock.md: separator `#a9aaaf`,
/// indicator `#e5e6eb`). The label is glass like the menus ([DockLabel]).
@immutable
class DockColors {
  const DockColors({required this.separator, required this.indicator});

  final Color separator;
  final Color indicator;

  // Kit `Dock/Separator/Line` (#1d1e2124 / #f4f5f729) and `Dock/Indicator/Dot`
  // (#5f636b / #b0b4bc).
  static const DockColors light = DockColors(
    separator: Color(0x591D1E21),
    indicator: Color(0xFF5F636B),
  );

  static const DockColors dark = DockColors(
    separator: Color(0x4DF4F5F7),
    indicator: Color(0xFFB0B4BC),
  );

  static DockColors of(BuildContext context) =>
      MediaQuery.maybePlatformBrightnessOf(context) == Brightness.dark
      ? dark
      : light;
}

/// Label (kit `Dock/Label`, 64 x 34 for "Mail"): a Glass/Thick body 28 pt tall (radius 8) with
/// 16 pt of padding at each side around the Body text (width rounded up to the 4 pt grid),
/// and an arrow 6 pt deep under it, centred; the arrow's tip is 12 pt above the shelf's top.
const double labelHeight = 28;
const double labelTip = dockArrowDepth;
const double labelPadding = 16;
const double labelRadius = 8;

/// The Dock label text: Body, `Text/Primary` of the appearance.
TextStyle dockLabelStyle(BuildContext context) =>
    AmalfiType.body(color: AmalfiTheme.of(context).colors.textPrimary);

/// The width of a label for `text`: the text rounded up to the 4 pt grid plus the padding.
double dockLabelWidth(String text, TextStyle style, {double max = 224}) {
  final painter = TextPainter(
    text: TextSpan(text: text, style: style),
    textDirection: TextDirection.ltr,
    maxLines: 1,
  )..layout();
  final width = (painter.width / 4).ceil() * 4.0 + 2 * labelPadding;
  painter.dispose();
  return math.min(max, width);
}

class Dock extends StatefulWidget {
  const Dock({
    super.key,
    required this.items,
    this.canvas = defaultCanvas,
    this.magnified,
    this.onResize,
    this.autohide = false,
    this.indicators = true,
    this.launcher = const {},
    this.onAddFolders,
    this.onSpringLoad,
    this.onActivate,
    this.onDropFiles,
    this.menuFor,
    this.onReorder,
    this.alert,
    this.onOpenPath,
    this.stackSettingsFor,
    this.stackListMenu,
    this.revealAtStart = false,
  });

  final List<Item> items;

  /// Session start: hidden below the screen until its icons are decoded (never empty cells
  /// that fill in), then it slides up ([sessionReveal]). Without it the Dock is there at once
  /// (tests, the demo).
  final bool revealAtStart;

  /// The side of the whole canvas of an app icon at rest (whole points, 16-128).
  final double canvas;

  /// The canvas of the icon under the pointer, up to 128; null: no magnification.
  final double? magnified;

  /// The size was changed by dragging the separator and the pointer was released: the
  /// canvas to keep (dock.json `tileSize`).
  final ValueChanged<double>? onResize;

  /// Hidden below the screen until the pointer reaches the last row (SH-DK-07).
  final bool autohide;

  /// The dots under the open apps (SH-DK-12).
  final bool indicators;

  /// What the apps ask of their icons (badge, progress, attention), by item id.
  final Map<String, LauncherState> launcher;

  /// Folders dragged in from another app, dropped right of the separator not over an item:
  /// added at that position among the folders (D-05, MU-40).
  final void Function(List<String> paths, int folderIndex)? onAddFolders;

  /// A file held over an app for ~830 ms (MU-40): it flashes twice, then this.
  final void Function(Item item)? onSpringLoad;

  /// Click on an app.
  final void Function(Item item)? onActivate;

  /// Files dropped from another app on an item that accepts them ([Item.acceptsFiles]).
  final void Function(Item item, List<String> paths)? onDropFiles;

  /// A modal alert above the screen (Svuota il Cestino), [trashAlertSize] large.
  final Widget? alert;

  /// A file of the open stack, or the whole folder ("Open folder").
  final ValueChanged<String>? onOpenPath;

  /// The menu of an item (right click, long press); null or empty: no menu.
  final List<MenuEntry> Function(Item item)? menuFor;

  /// The stack settings of a folder (order, icon, view); null: the defaults.
  final StackSettings Function(String folder)? stackSettingsFor;

  /// The menu of a stack's List view: the files, Options, Open (dock-stacks.md › List).
  final List<MenuEntry> Function(Item item, List<FileSystemEntity> entries)?
  stackListMenu;

  /// Dragging (D-05): the new order of the apps left of the separator and the item dropped
  /// (null if it was removed with "Remove", and then it is missing from the order).
  /// Null: the icons do not drag.
  final void Function(List<Item> order, Item? dropped)? onReorder;

  @override
  State<Dock> createState() => _DockState();
}

class _DockState extends State<Dock> with TickerProviderStateMixin {
  /// This component's layer surface (several share one engine, #201).
  late final LayerControl _layer;

  late final Ticker _ticker = createTicker(_tick);

  /// The size being dragged on the separator (null: [Dock.canvas] stands), and the drag.
  double? _resizeCanvas;
  _Resize? _resize;
  bool _overSeparator = false;

  /// The geometry at the current size.
  DockMetrics get _m => DockMetrics(_resizeCanvas ?? widget.canvas);

  /// No magnification while the separator is being dragged.
  double? get _magnified => _resize == null ? widget.magnified : null;

  /// The label: the item it belongs to and its opacity.
  int? _labelIndex;
  late final AnimationController _labelAnim = AnimationController(vsync: this)
    ..addListener(() => setState(() {}));
  Timer? _labelTimer;

  /// The open menu: the item, the entries, whether it came from a long press.
  int? _menuIndex;
  List<MenuEntry> _menuEntries = const [];

  /// The panels of the open menu (the menu, then its submenus), from its host.
  List<Rect> _menuRects = const [];

  /// The open submenus of the menu (nested ones included), as its list reports them.
  List<Rect> _submenuRects = const [];
  bool _menuByLongPress = false;
  final GlobalKey<DockMenuHostState> _menuKey = GlobalKey();
  Timer? _longPress;

  /// Hides the label shortly before a long press opens the menu.
  Timer? _labelOff;

  /// The icon whose label was hidden for its menu: it stays hidden until the pointer leaves it.
  int? _labelHeldOff;
  Offset? _downAt;
  int? _downIndex;

  /// The drag in progress and the removed icon fading out in place.
  _DockDrag? _drag;
  _Ghost? _ghost;

  /// The slots in layout order, leaving ones included, and what they were given: [_items]
  /// (what is laid out and hit) and [_live] (the slots still animating, by identity of the item).
  List<_Slot> _slots = const [];
  List<Item> _items = const [];
  final Map<Item, _Slot> _live = Map.identity();
  Duration _slotClock = Duration.zero;
  late final Ticker _slotTicker = createTicker(_slotTick);

  /// The key of an item removed by dragging it out: it fades as a ghost, its slot just closes.
  String? _dragRemoved;
  DateTime? _settleUntil;
  late final Ticker _ghostTicker = createTicker((elapsed) {
    final ghost = _ghost;
    if (ghost == null || elapsed >= removeFade) {
      _ghostTicker.stop();
      setState(() => _ghost = null);
    } else {
      setState(() => ghost.elapsed = elapsed);
    }
  });

  /// The open stack (D-06): the item, the files, the view and the animation (0 closed, 1 open).
  int? _stackIndex;
  List<FileSystemEntity> _stackEntries = const [];
  StackView _stackView = StackView.fan;
  bool _stackClosing = false;

  /// Only the stack listens to it (an [AnimatedBuilder] around it): a frame of the opening
  /// rebuilds and repaints the stack, not the whole Dock.
  late final AnimationController _stackAnim = AnimationController(
    vsync: this,
    duration: stackOpen,
  );
  final FocusNode _stackFocus = FocusNode();
  Timer? _stackOpenTimer;

  /// The starting height of the surface and the screen (to center the alert).
  double? _baseHeight;
  Size? _screen;

  /// Where the open stack is in the surface, for the input region and clicks.
  Rect? _stackRect;

  /// What was last asked of the layer (input region, height, keyboard).
  String _layerState = '';

  /// The pressed app (darkened icon) and those bouncing, with their start time.
  int? _pressed;
  final Map<String, DateTime> _bounces = {};
  late final Ticker _bounceTicker = createTicker((_) {
    _endBounces();
    setState(() {});
  });
  double? _pointerX;
  double _amount = 0;
  bool _inside = false;

  /// Auto-hide (SH-DK-07): 1 shown, 0 hidden below the screen. It comes up only from the
  /// last row of the screen, at any x, after ~190 ms, sliding in ~190 ms (ease-out); it goes
  /// down with no delay in ~175 ms (ease-in). The whole Dock slides, no fade.
  late final AnimationController _shown = AnimationController(
    vsync: this,
    value: widget.autohide || widget.revealAtStart ? 0 : 1,
  )..addListener(() => setState(() {}));

  /// Every icon is decoded (or the Dock does not wait for them): it may show.
  late bool _iconsReady = !widget.revealAtStart;
  Timer? _iconsTimeout;
  Timer? _revealTimer;
  // Entrance or exit animation: where it starts and when.
  double _from = 0;
  Duration? _start;

  /// A drag of files from another app: the cell under it, darkened as when pressed.
  StreamSubscription<SilicaDragEvent>? _fileDrags;
  StreamSubscription<void>? _dismissed;
  int? _dropIndex;

  /// The slot of the gap a folder dragged in from another app opens right of the separator
  /// (an index in the items), null when there is none.
  int? _folderGap;

  /// Among the folders: how many are left of `x`, if `x` is in the folders' side of the Dock
  /// (right of the separator, in the active zone); null elsewhere.
  int? _folderSlotAt(Offset local, Size size) {
    final current = layout(
      presenceOf: _presenceOf,
      metrics: _m,
      _items,
      width: size.width,
      height: size.height,
    );
    final x = local.dx, y = local.dy, height = size.height;
    final separator = _items.indexWhere((i) => i.kind == ItemKind.separator);
    if (separator < 0 ||
        !inActiveZone(current, x, y, height) ||
        x <= current.center(separator)) {
      return null;
    }
    var before = 0;
    for (var i = separator + 1; i < _items.length; i++) {
      if (_items[i].folder != null && current.center(i) < x) before++;
    }
    return before;
  }

  void _setFolderGap(int? gap) {
    if (gap == _folderGap) return;
    setState(() {
      _folderGap = gap;
      _settleUntil = DateTime.now().add(gapSlide);
    });
  }

  void _fileDrag(SilicaDragEvent event) {
    final box = context.findRenderObject();
    if (!mounted || box is! RenderBox || !box.hasSize) return;
    final size = box.size;
    switch (event) {
      case SilicaDragOver(:final position):
        final local = box.globalToLocal(position);
        _hover(local, size);
        final current = layout(
          presenceOf: _presenceOf,
          metrics: _m,
          _items,
          width: size.width,
          height: size.height,
          pointerX: _pointerX,
          amount: _amount,
          magnified: _magnified,
        );
        final index = _inside ? itemAt(current, _items, local.dx) : null;
        final target = index != null && _items[index].acceptsFiles
            ? index
            : null;
        // No item takes the drag: a folder dropped here would be added, so the Dock makes room.
        final before = target == null ? _folderSlotAt(local, size) : null;
        _setFolderGap(
          before == null
              ? null
              : _items.indexWhere((i) => i.kind == ItemKind.separator) +
                    1 +
                    before,
        );
        if (target != _dropIndex) {
          setState(() {
            if (_pressed == _dropIndex) _pressed = target;
            _dropIndex = target;
          });
          _springTimer?.cancel();
          _springTimer = null;
          final item = target == null ? null : _items[target];
          if (item != null && item.openFiles != null) {
            _springTimer = Timer(springLoadDelay, () => _springLoad(target!));
          }
        }
      case SilicaDragLeave():
        _endFileDrag();
      case SilicaDrop(:final paths, :final position):
        final index = _dropIndex;
        _endFileDrag();
        if (index != null && index < _items.length && paths.isNotEmpty) {
          widget.onDropFiles?.call(_items[index], paths);
        } else if (paths.isNotEmpty &&
            paths.every(FileSystemEntity.isDirectorySync)) {
          // Folders right of the separator: added to the Dock where they were dropped.
          final local = box.globalToLocal(position);
          final before = _folderSlotAt(local, size);
          if (before != null) {
            widget.onAddFolders?.call(paths, before);
          }
        }
      case SilicaDragFinished():
        break;
    }
  }

  /// Spring-loading (MU-40): the app flashes twice (pressed, released, pressed, released),
  /// then its windows open so the file can be dropped into one.
  Timer? _springTimer;

  Future<void> _springLoad(int index) async {
    _springTimer = null;
    if (!mounted || _dropIndex != index || index >= _items.length) return;
    final item = _items[index];
    for (var i = 0; i < 2; i++) {
      setState(() => _pressed = null);
      await Future<void>.delayed(const Duration(milliseconds: 80));
      if (!mounted || _dropIndex != index) return;
      setState(() => _pressed = index);
      await Future<void>.delayed(const Duration(milliseconds: 80));
      if (!mounted || _dropIndex != index) return;
    }
    widget.onSpringLoad?.call(item);
  }

  void _endFileDrag() {
    _springTimer?.cancel();
    _springTimer = null;
    if (_folderGap != null) {
      _folderGap = null;
      _settleUntil = DateTime.now().add(gapSlide);
    }
    setState(() {
      if (_pressed == _dropIndex) _pressed = null;
      _dropIndex = null;
    });
    _setInside(false);
    _showLabel(null);
  }

  @override
  void initState() {
    super.initState();
    _layer = LayerControl.of(context);
    _syncSlots(first: true);
    HardwareKeyboard.instance.addHandler(_onKey);
    _fileDrags = SilicaDrag.events.listen(_fileDrag);
    _dismissed = _layer.transientDismissed.listen((_) => _transientDismissed());
    if (!_iconsReady) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _waitForIcons());
      _iconsTimeout = Timer(iconsWaitLimit, _showAtStart);
    }
    // The screen is needed by the first menu already (the surface covers it to catch the click outside).
    LayerControl.outputSize().then((size) {
      if (mounted && size != null && _screen == null) {
        setState(() => _screen = size);
      }
    });
  }

  @override
  void didUpdateWidget(Dock old) {
    super.didUpdateWidget(old);
    if (!identical(old.items, widget.items)) _syncSlots(first: false);
    if (old.canvas != widget.canvas && _resize == null) _resizeCanvas = null;
    if (old.autohide != widget.autohide) {
      widget.autohide ? _hideDock() : _revealDock();
    }
  }

  double _presenceOf(Item item) => _live[item]?.presence ?? 1;

  /// Stable identity of an item across rebuilds, with the n-th occurrence of equal ones.
  static List<String> _slotKeys(List<Item> items) {
    final seen = <String, int>{};
    return [
      for (final item in items)
        () {
          final base = item.kind == ItemKind.separator
              ? 'separator'
              : [
                  item.label,
                  item.command,
                  item.id,
                  item.folder,
                  item.minimizedWindow,
                  item.trash,
                ].join('\u0000');
          final n = seen[base] = (seen[base] ?? 0) + 1;
          return '$base\u0000$n';
        }(),
    ];
  }

  /// Aligns the slots with the items given: a new item opens a slot, a missing one closes it.
  /// Nothing animates the first list, the minimized windows (own effect) and a Dock hidden by
  /// auto-hide.
  void _syncSlots({required bool first}) {
    final keys = _slotKeys(widget.items);
    final animate =
        !first && _slots.isNotEmpty && !(widget.autohide && _shown.value == 0);
    final old = _slots;
    // An item that comes back while its slot is closing takes that slot again.
    final byKey = {for (final slot in old) slot.key: slot};
    final out = <_Slot>[];
    final kept = <_Slot>{};
    for (var i = 0; i < keys.length; i++) {
      final item = widget.items[i];
      var slot = byKey[keys[i]];
      if (slot != null) {
        slot.item = item;
        if (slot.leaving) slot.startOpening();
        kept.add(slot);
      } else {
        final instant = !animate || item.minimizedWindow != null;
        slot = _Slot(item, keys[i], opening: !instant);
      }
      out.add(slot);
    }
    // The missing ones stay, after the slot that preceded them, until they have closed.
    var after = -1;
    for (final slot in old) {
      if (kept.contains(slot)) {
        after = out.indexOf(slot);
        continue;
      }
      final skip = slot.key == _dragRemoved;
      if (!slot.leaving) {
        slot.startLeaving();
        if (skip || !animate || slot.item.minimizedWindow != null) continue;
      } else if (!slot.active) {
        continue;
      }
      out.insert(++after, slot);
    }
    // The parent answers a drag-out in the same frame; do not carry the mark further.
    if (_dragRemoved != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _dragRemoved = null);
    }
    _slots = out;
    _publishSlots();
    if (out.any((s) => s.active)) {
      if (!_slotTicker.isActive) {
        _slotClock = Duration.zero;
        _slotTicker.start();
      }
    }
  }

  void _publishSlots() {
    _items = [for (final slot in _slots) slot.item];
    _live.clear();
    for (final slot in _slots) {
      if (slot.active) _live[slot.item] = slot;
    }
  }

  void _slotTick(Duration elapsed) {
    final step = (elapsed - _slotClock).inMicroseconds;
    _slotClock = elapsed;
    setState(() {
      for (final slot in _slots) {
        slot.advance(step / 1000);
      }
      _slots = [
        for (final slot in _slots)
          if (!slot.leaving || slot.t < 1) slot,
      ];
      _publishSlots();
      if (_slots.every((s) => !s.active)) _slotTicker.stop();
    });
  }

  /// The items that stay in the Dock (without those whose slot is closing).
  List<Item> _staying(List<Item> items) => [
    for (final item in items)
      if (!_slots.any((s) => s.leaving && identical(s.item, item))) item,
  ];

  void _revealDock() {
    _revealTimer?.cancel();
    _revealTimer = null;
    if (!_iconsReady) return;
    _shown.animateTo(1, duration: autohideReveal, curve: Curves.easeOut);
  }

  /// After every frame at session start: once all the icons are decoded, the Dock comes up.
  /// An icon that finishes loading rebuilds and draws a frame, so this runs again after it.
  void _waitForIcons() {
    if (!mounted || _iconsReady) return;
    if (_iconsDecoded()) {
      _showAtStart();
    } else {
      WidgetsBinding.instance.addPostFrameCallback((_) => _waitForIcons());
    }
  }

  /// No icon of the Dock is still a placeholder: every SVG has its picture (flutter_svg puts
  /// the decoded picture in a FittedBox, the placeholder is an empty SizedBox) and every
  /// bitmap its image.
  bool _iconsDecoded() {
    var decoded = true;
    bool hasPicture(Element element) {
      var found = false;
      void find(Element e) {
        if (found) return;
        if (e.widget is FittedBox) {
          found = true;
          return;
        }
        e.visitChildElements(find);
      }

      element.visitChildElements(find);
      return found;
    }

    void visit(Element element) {
      if (!decoded) return;
      final widget = element.widget;
      if (widget is SvgPicture && !hasPicture(element)) {
        decoded = false;
        return;
      }
      if (widget is RawImage && widget.image == null) {
        decoded = false;
        return;
      }
      element.visitChildElements(visit);
    }

    context.visitChildElements(visit);
    return decoded;
  }

  void _showAtStart() {
    _iconsTimeout?.cancel();
    _iconsTimeout = null;
    if (!mounted || _iconsReady) return;
    setState(() => _iconsReady = true);
    if (!widget.autohide) {
      _shown.animateTo(1, duration: sessionReveal, curve: Curves.easeOut);
    }
  }

  /// Down, unless something of the Dock is in use (menu, drag, stack, alert).
  void _hideDock() {
    _revealTimer?.cancel();
    _revealTimer = null;
    if (!widget.autohide ||
        _menuIndex != null ||
        _drag != null ||
        _stackIndex != null ||
        widget.alert != null) {
      return;
    }
    _shown.animateBack(0, duration: autohideHide, curve: Curves.easeIn);
  }

  @override
  void dispose() {
    _fileDrags?.cancel();
    _dismissed?.cancel();
    _ticker.dispose();
    _bounceTicker.dispose();
    _labelAnim.dispose();
    _labelTimer?.cancel();
    _longPress?.cancel();
    _labelOff?.cancel();
    _ghostTicker.dispose();
    _slotTicker.dispose();
    _stackOpenTimer?.cancel();
    _stackAnim.dispose();
    _stackFocus.dispose();
    _shown.dispose();
    _iconsTimeout?.cancel();
    _revealTimer?.cancel();
    HardwareKeyboard.instance.removeHandler(_onKey);
    _springTimer?.cancel();
    super.dispose();
  }

  /// The last item left of the separator (where one can drag).
  int get _leftEnd {
    final separator = _items.indexWhere((i) => i.kind == ItemKind.separator);
    return separator < 0 ? _items.length : separator;
  }

  /// The items to lay out: while dragging without the one taken and, if the pointer is in
  /// the Dock, with an empty slot where it would fall.
  List<Item> _displayItems(Size size) {
    final drag = _drag;
    final gap = _folderGap;
    if (drag == null && gap != null && gap <= _items.length) {
      // A folder dragged in from another app: room where it would land (D-05, MU-40).
      return [..._items]..insert(gap, gapItem);
    }
    if (drag == null || !drag.started) return _items;
    final rest = [..._items]..removeAt(drag.index);
    if (!drag.inside || drag.remove) return rest;
    final slot = _dropSlot(rest, size, drag.pointer.dx);
    return [...rest]..insert(slot, gapItem);
  }

  /// Where the icon would fall: the first item on the left whose center is past the pointer.
  int _dropSlot(List<Item> rest, Size size, double x) {
    final l = layout(
      presenceOf: _presenceOf,
      metrics: _m,
      rest,
      width: size.width,
      height: size.height,
    );
    final end = rest.indexWhere((i) => i.kind == ItemKind.separator);
    final limit = end < 0 ? rest.length : end;
    // Nothing goes before the fixed items (the file manager, #141).
    final first = rest.takeWhile((i) => i.fixed).length;
    for (var i = first; i < limit; i++) {
      if (x < l.center(i)) return i;
    }
    return limit;
  }

  void _startDrag(Offset p, Size size) {
    final index = _downIndex;
    if (index == null || index >= _leftEnd || widget.onReorder == null) return;
    final item = _items[index];
    if (item.kind != ItemKind.app || item.fixed) return;
    final l = layout(
      presenceOf: _presenceOf,
      metrics: _m,
      _items,
      width: size.width,
      height: size.height,
      pointerX: _pointerX,
      amount: _amount,
      magnified: _magnified,
    );
    final (x, y, side) = iconRect(l, index, size.height);
    _longPress?.cancel();
    _labelOff?.cancel();
    _longPress = null;
    _showLabel(null);
    setState(() {
      _pressed = null;
      _drag = _DockDrag(
        index: index,
        item: item,
        grab: p - Offset(x, y),
        side: side,
        downY: _downAt?.dy ?? p.dy,
        pointer: p,
      )..started = true;
    });
  }

  void _moveDrag(Offset p, Size size) {
    final drag = _drag;
    if (drag == null) return;
    final rest = layout(
      presenceOf: _presenceOf,
      metrics: _m,
      _items,
      width: size.width,
      height: size.height,
    );
    setState(() {
      drag.pointer = p;
      drag.inside =
          inActiveZone(rest, p.dx, p.dy, size.height) ||
          p.dy >=
              size.height - bottomGap - _m.backgroundHeight - dragInsideAbove;
      // "Remove" appears 125–135 pt above the grab point.
      drag.remove = p.dy < drag.downY - removeAbove;
    });
  }

  void _endDrag(Size size) {
    final drag = _drag;
    if (drag == null) return;
    final rest = _staying([..._items]..removeAt(drag.index));
    final left = rest.indexWhere((i) => i.kind == ItemKind.separator);
    final leftItems = rest.sublist(0, left < 0 ? rest.length : left);
    if (drag.remove) {
      _dragRemoved = _slotKeys(_items)[drag.index];
      // Removed: the icon and the capsule fade out in place (~250 ms), with no other effects.
      _ghost = _Ghost(drag.item, drag.pointer - drag.grab, drag.side);
      _ghostTicker.start();
      widget.onReorder?.call(leftItems, null);
    } else if (drag.inside) {
      final slot = _dropSlot(rest, size, drag.pointer.dx);
      final order = [...leftItems]
        ..insert(slot.clamp(0, leftItems.length), drag.item);
      widget.onReorder?.call(order, drag.item);
    }
    setState(() {
      _drag = null;
      _settleUntil = DateTime.now().add(gapSlide);
    });
  }

  /// Hides the label of `index` for its menu, until the pointer leaves the icon.
  void _hideLabelForMenu(int index) {
    _labelTimer?.cancel();
    _labelTimer = null;
    _labelHeldOff = index;
    _labelAnim.value = 0;
    if (mounted) setState(() => _labelIndex = null);
  }

  /// Opens the menu of an item (D-04): the icon stays pressed, the label disappears.
  void _openMenu(int index, {required bool longPress}) {
    final entries = widget.menuFor?.call(_items[index]) ?? const [];
    if (entries.isEmpty) return;
    setState(() {
      _menuIndex = index;
      _menuEntries = entries;
      _menuByLongPress = longPress;
      _pressed = index;
    });
    _hideLabelForMenu(index);
  }

  /// With the menu open the Dock has the keyboard: Option swaps the alternate items (D-04:
  /// Hide → Hide Others, Quit → Force Quit) in place.
  bool _altHeld = false;

  bool _onKey(KeyEvent event) {
    // Esc closes the open menu or stack even when the keyboard focus is not on them (the
    // surface got the keyboard after the menu's rows asked for it, #206). With the focus on
    // the rows or the stack, they handle it themselves (a submenu closes before its menu).
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.escape) {
      if (_menuIndex != null) {
        if (!(_menuKey.currentState?.hasFocus ?? false)) {
          _menuKey.currentState?.close();
          return true;
        }
      } else if (_stackIndex != null && !_stackFocus.hasFocus) {
        _closeStack();
        return true;
      }
    }
    final alt = HardwareKeyboard.instance.isAltPressed;
    if (alt != _altHeld) {
      _altHeld = alt;
      final index = _menuIndex;
      if (index != null && index < _items.length) {
        final entries = widget.menuFor?.call(_items[index]) ?? const [];
        if (entries.isNotEmpty) setState(() => _menuEntries = entries);
      }
    }
    return false;
  }

  void _closeMenu() {
    if (!mounted) return;
    setState(() {
      _menuIndex = null;
      _menuEntries = const [];
      _menuRects = const [];
      _submenuRects = const [];
      _pressed = null;
    });
  }

  bool _menuContains(Offset p) => _menuKey.currentState?.contains(p) ?? false;

  void _pointerDown(PointerDownEvent e, Layout l, Size size) {
    final p = e.localPosition;
    // With the stack open a click outside it and its icon closes it.
    if (_stackIndex != null &&
        !(_stackRect?.contains(p) ?? false) &&
        itemAt(l, _items, p.dx) != _stackIndex) {
      _closeStack();
      return;
    }
    if (_menuIndex != null) {
      // The menu takes its own clicks; a press outside closes it.
      if (!_menuContains(p)) _menuKey.currentState?.close();
      return;
    }
    final rest = layout(
      presenceOf: _presenceOf,
      metrics: _m,
      _items,
      width: size.width,
      height: size.height,
    );
    final inZone = inActiveZone(rest, p.dx, p.dy, size.height) || _inside;
    final index = inZone ? itemAt(l, _items, p.dx) : null;
    // The separator has its menu (right click), but is neither pressed nor dragged.
    if (index == null && inZone && e.buttons & kSecondaryButton != 0) {
      final cell = cellAt(l, p.dx);
      if (cell != null && _items[cell].kind == ItemKind.separator) {
        _openMenu(cell, longPress: false);
      }
      return;
    }
    // The separator drags to resize the Dock.
    if (index == null && inZone && e.buttons & kPrimaryButton != 0) {
      final cell = cellAt(l, p.dx);
      if (cell != null && _items[cell].kind == ItemKind.separator) {
        _showLabel(null);
        setState(() => _resize = _Resize(p.dy, _m.canvas));
        return;
      }
    }
    if (index == null) return;
    if (e.buttons & kSecondaryButton != 0) {
      _openMenu(index, longPress: false);
      return;
    }
    setState(() => _pressed = index);
    _downAt = p;
    _downIndex = index;
    _longPress?.cancel();
    _labelOff?.cancel();
    final menu = widget.menuFor?.call(_items[index]) ?? const [];
    if (menu.isNotEmpty) {
      _labelOff = Timer(longPressLabelHide, () => _hideLabelForMenu(index));
    }
    _longPress = Timer(longPressMenu, () {
      _longPress = null;
      _openMenu(index, longPress: true);
    });
  }

  void _pointerMove(Offset p, Size size) {
    if (_resize case final resize?) {
      // Up is bigger: 1 pt of canvas per point dragged, whole points, no animation.
      final canvas = clampCanvas(resize.canvas + (resize.downY - p.dy));
      if (canvas != _resizeCanvas) setState(() => _resizeCanvas = canvas);
      return;
    }
    if (_drag != null) {
      _moveDrag(p, size);
      return;
    }
    if (_longPress != null &&
        _downAt != null &&
        (p - _downAt!).distance > dragThreshold) {
      // Beyond 5 pt the press becomes a drag.
      _startDrag(p, size);
      _longPress?.cancel();
      _labelOff?.cancel();
      _longPress = null;
    }
    if (_menuIndex != null && _menuByLongPress) {
      _menuKey.currentState?.pointerMoved(p);
    }
  }

  void _pointerUp(PointerUpEvent e, Layout l, Size size) {
    final p = e.localPosition;
    if (_resize case final resize?) {
      // Released: the size is kept (dock.json `tileSize`, the same key as Settings).
      final canvas = _resizeCanvas ?? resize.canvas;
      setState(() => _resize = null);
      if (canvas != resize.canvas) widget.onResize?.call(canvas);
      return;
    }
    if (_drag != null) {
      _endDrag(size);
      return;
    }
    if (_longPress != null) {
      // A click: the item under the pointer, if it is the same as at the press.
      _longPress?.cancel();
      _labelOff?.cancel();
      _longPress = null;
      final index = itemAt(l, _items, p.dx);
      setState(() => _pressed = null);
      if (index != null && index == _downIndex) _activate(index);
      return;
    }
    if (_menuIndex != null && _menuByLongPress) {
      // After the long press: releasing on an entry chooses it, on the icon closes.
      _menuByLongPress = false;
      if (_menuContains(p)) {
        _menuKey.currentState?.pointerUp(p);
      } else {
        _menuKey.currentState?.close();
      }
    }
  }

  /// A menu or the stack is open and Ossido knows it (#149): the Dock keeps the pointer only
  /// where it is and a press outside arrives as `transientDismissed`. False on an older
  /// Ossido, then the surface covers the screen to see the click outside.
  bool _transientActive = false;
  bool _transientSupported = true;

  /// A press outside the Dock, the menu or the stack ended them (Ossido, #149).
  void _transientDismissed() {
    if (_menuIndex != null) _closeMenu();
    if (_stackIndex != null) _closeStack();
  }

  /// Input region, surface height and keyboard: only when they change.
  void _syncLayer(Rect dock, Rect? menu, Rect? alert, Size size) {
    final stack = _stackIndex != null;
    // The open stack takes a surface as tall as the screen, like the alert.
    alert ??= stack ? _stackRect ?? Rect.zero : null;
    _baseHeight ??= size.height;
    final base = _baseHeight ?? size.height;
    final screen = _screen?.height;
    final open = menu != null || stack;
    // The whole screen only without transient support (older Ossido): a click outside (on the
    // desktop, on a window) closes the menu or the stack instead of going through.
    final catchAll = open && screen != null && !_transientSupported;
    // The surface stays as tall as the screen once its size is known, and only the input
    // region changes: the Dock is anchored to the bottom, so every height change moved the
    // content's origin and the Dock and its menu flashed up for a frame before the
    // compositor caught up (right click on an item). Without the screen size: as before.
    final height = screen != null
        ? screen
        : alert != null || catchAll
        ? size.height
        : menu != null
        ? (size.height - menu.top + menuShadowRoom).clamp(size.height, 4096.0)
        : base;
    final submenus = menu == null ? const <Rect>[] : _submenuRects;
    // Auto-hide: hidden, only the last row of the screen (the reveal edge); shown, that row too
    // (it stays up while the pointer runs along it).
    final bottomRow = Rect.fromLTWH(0, height - 1, size.width, 1);
    final hiddenDock =
        widget.autohide && _shown.value < 1 && menu == null && alert == null;
    // Before it comes up at session start nothing of the Dock takes the pointer.
    final waiting = !_iconsReady;
    final key =
        '$dock $menu $submenus $alert $height $catchAll $hiddenDock ${widget.autohide} $open $waiting';
    if (key == _layerState) return;
    _layerState = key;
    final rects = [
      if (widget.autohide) bottomRow,
      dock,
      if (menu != null) menu.inflate(4),
      for (final r in submenus) r.inflate(4),
      if (alert != null) alert,
    ];
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (height != size.height) _layer.size(0, height);
      _layer.inputRegion(
        waiting
            ? const <Rect>[]
            : catchAll
            ? [Offset.zero & Size(size.width, height)]
            : hiddenDock
            ? [bottomRow]
            : rects,
      );
      _layer.keyboard(menu == null && alert == null ? 'none' : 'exclusive');
      if (open) {
        _transientActive = true;
        // INPUT-01 (c): a left click on the app that was active only makes it active again
        // (its caret does not move); elsewhere clicks, right clicks and the wheel pass.
        _layer
            .transient(
              rects: rects,
              policy: TransientPolicy.dockMenu,
              owner: ShellChannel.instance.frontmostAppId,
            )
            .then((supported) {
              // Older Ossido: from now on the surface covers the screen while something is open.
              if (!supported && mounted && _transientSupported) {
                setState(() {
                  _transientSupported = false;
                  _transientActive = false;
                  _layerState = '';
                });
              }
            });
      } else if (_transientActive) {
        _transientActive = false;
        _layer.clearTransient();
      }
    });
  }

  /// The surface is taller than at rest (it grew to show the stack), or as tall as the screen.
  /// Before the screen size is known nothing waits.
  bool _surfaceGrown(double height) {
    final screen = _screen;
    if (screen == null) return true;
    final base = _baseHeight;
    return height >= screen.height - 1 || (base != null && height > base + 1);
  }

  static String _key(Item item) => '${item.label}\u0000${item.command}';

  /// A bounce in progress lands when the app has a window, or after the limit.
  void _endBounces() {
    final now = DateTime.now();
    _bounces.removeWhere((key, start) {
      final ms = now.difference(start).inMicroseconds / 1000;
      final item = _items.where((i) => _key(i) == key).firstOrNull;
      final landed = ms % bouncePeriod < 30 && ms > bouncePeriod / 2;
      return ms > bounceLimit.inMilliseconds ||
          item == null ||
          (item.running && landed);
    });
    if (_bounces.isEmpty && _bounceTicker.isActive) _bounceTicker.stop();
  }

  double _bounce(int index) => _bounceOf(_items[index]);

  double _bounceOf(Item item) {
    final start = _bounces[_key(item)];
    if (start == null) {
      // Attention (SH-DK-30): hops every 2 s until the app is activated.
      final since = widget.launcher[item.id]?.urgentSince;
      if (since == null) return 0;
      return _m.k *
          attentionHeight(
            DateTime.now().difference(since).inMicroseconds / 1000,
          );
    }
    return _m.k *
        bounceHeight(DateTime.now().difference(start).inMicroseconds / 1000);
  }

  /// The first files of a folder shown as a stack, for the icon: read again only when the
  /// folder changes (modification time) or the order changes.
  final Map<String, (DateTime, StackSort, int, List<IconSource>)> _piles = {};

  List<IconSource>? _pileOf(Item item) {
    final folder = item.folder;
    if (folder == null) return null;
    final settings =
        widget.stackSettingsFor?.call(folder) ?? const StackSettings();
    if (settings.display != StackDisplay.stack) return null;
    DateTime changed;
    try {
      changed = Directory(folder).statSync().modified;
    } on FileSystemException {
      return null;
    }
    final cached = _piles[folder];
    if (cached != null &&
        cached.$1 == changed &&
        cached.$2 == settings.sort &&
        cached.$3 == dockIconRevision) {
      return cached.$4;
    }
    final sources = [
      for (final entry in stackEntries(folder, sort: settings.sort).take(3))
        ?fileIcon(entry),
    ];
    _piles[folder] = (changed, settings.sort, dockIconRevision, sources);
    return sources;
  }

  void _openStack(int index) {
    final item = _items[index];
    final folder = item.folder;
    if (folder == null) return;
    final settings =
        widget.stackSettingsFor?.call(folder) ?? const StackSettings();
    final entries = stackEntries(folder, sort: settings.sort);
    // List view: a menu with the files; the icon stays the folder, pressed.
    if (settings.content == StackContent.list) {
      final menu = widget.stackListMenu?.call(item, entries) ?? const [];
      if (menu.isEmpty) return;
      setState(() {
        _menuIndex = index;
        _menuEntries = menu;
        _menuByLongPress = false;
        _pressed = index;
      });
      _showLabel(null);
      return;
    }
    setState(() {
      _stackIndex = index;
      _stackEntries = entries;
      _stackView = switch (settings.content) {
        StackContent.fan => StackView.fan,
        StackContent.grid => StackView.grid,
        _ => automaticView(entries.length),
      };
      _stackClosing = false;
    });
    _showLabel(null);
    _stackAnim.duration = stackOpen;
    _stackAnim.value = 0;
    // The motion starts ~90 ms (fan) or ~70 ms (grid) after the press (dock-stacks.md).
    _stackOpenTimer?.cancel();
    _stackOpenTimer = Timer(
      _stackView == StackView.fan ? fanOpenDelay : gridOpenDelay,
      () {
        _stackOpenTimer = null;
        if (mounted && _stackIndex != null && !_stackClosing) {
          _stackAnim.forward(from: 0);
        }
      },
    );
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => _stackFocus.requestFocus(),
    );
  }

  /// `byButton`: released on the close button (~300 ms); otherwise Esc or a click elsewhere.
  Future<void> _closeStack({
    bool byButton = false,
    bool afterFile = false,
  }) async {
    if (_stackIndex == null || _stackClosing) return;
    _stackClosing = true;
    _stackOpenTimer?.cancel();
    _stackOpenTimer = null;
    // Clicking a file: the stack closes ~13 ms after the release, in ~150 ms.
    if (afterFile) {
      await Future<void>.delayed(stackFileCloseDelay);
      if (!mounted) return;
    }
    _stackAnim.duration = afterFile
        ? stackFileClose
        : byButton
        ? stackCloseByButton
        : _stackView == StackView.fan
        ? fanClose
        : gridClose;
    await _stackAnim.reverse();
    if (mounted) {
      setState(() {
        _stackIndex = null;
        _stackClosing = false;
      });
    }
  }

  /// A file of the stack: the stack closes at once, then the file opens.
  /// A file dragged out of the open stack: it closes and the drag goes on as a system drag.
  void _dragFromStack(String path) {
    _closeStack();
    SilicaDrag.start([path]);
  }

  void _openFromStack(String path) {
    _closeStack(afterFile: true);
    widget.onOpenPath?.call(path);
  }

  void _activate(int index) {
    final item = _items[index];
    if (item.folder != null && widget.onOpenPath != null) {
      _stackIndex == index ? _closeStack(byButton: true) : _openStack(index);
      return;
    }
    if (launchBounceTarget(item, _items) case final target?) {
      _bounces.putIfAbsent(_key(target), DateTime.now);
      if (!_bounceTicker.isActive) _bounceTicker.start();
    }
    widget.onActivate?.call(item);
  }

  /// The label follows the cell under the pointer: immediately from one icon to the next.
  void _showLabel(int? index) {
    if (index != _labelHeldOff) _labelHeldOff = null;
    if (index != null && index == _labelHeldOff) return;
    if (index != null) {
      _labelTimer?.cancel();
      _labelTimer = null;
      final hidden = _labelIndex == null || _labelAnim.value < 1;
      _labelIndex = index;
      if (hidden) {
        _labelAnim.animateTo(1, duration: labelAppear);
      }
      return;
    }
    if (_labelIndex == null || _labelTimer != null) return;
    _labelTimer = Timer(labelHold, () {
      _labelTimer = null;
      _labelAnim.animateTo(0, duration: labelVanish).whenComplete(() {
        if (_labelAnim.value == 0 && mounted)
          setState(() => _labelIndex = null);
      });
    });
  }

  void _tick(Duration elapsed) {
    _start ??= elapsed;
    final duration = enterDuration((widget.magnified ?? _m.canvas) / _m.canvas);
    final t = (elapsed - _start!).inMicroseconds / duration.inMicroseconds;
    final target = _inside ? 1.0 : 0.0;
    final eased = enterProgress(t);
    setState(() => _amount = _from + (target - _from) * eased);
    if (t >= 1) {
      _ticker.stop();
      _start = null;
      if (!_inside) _pointerX = null;
    }
  }

  void _setInside(bool inside) {
    if (inside == _inside) return;
    _inside = inside;
    _from = _amount;
    _start = null;
    if (_ticker.isActive) _ticker.stop();
    _ticker.start();
  }

  void _hover(Offset position, Size size) {
    if (widget.autohide) {
      final bottomRow = position.dy >= size.height - 1;
      final hidden =
          _shown.value == 0 || _shown.status == AnimationStatus.reverse;
      if (hidden) {
        // Only the last row of the screen brings it up, after a short wait.
        if (bottomRow) {
          _revealTimer ??= Timer(autohideDelay, _revealDock);
        } else {
          _revealTimer?.cancel();
          _revealTimer = null;
        }
        return;
      }
    }
    final rest = layout(
      presenceOf: _presenceOf,
      metrics: _m,
      _items,
      width: size.width,
      height: size.height,
    );
    final current = layout(
      presenceOf: _presenceOf,
      metrics: _m,
      _items,
      width: size.width,
      height: size.height,
      pointerX: _pointerX,
      amount: _amount,
      magnified: _magnified,
    );
    // The separator shows the resize pointer.
    final cell = cellAt(current, position.dx);
    final overSeparator =
        cell != null &&
        _items[cell].kind == ItemKind.separator &&
        inActiveZone(rest, position.dx, position.dy, size.height);
    if (overSeparator != _overSeparator) {
      setState(() => _overSeparator = overSeparator);
    }
    // Entering uses the fixed threshold; exiting only by leaving the magnified cell (hysteresis).
    final inside = _inside
        ? staysInActiveZone(
            rest,
            current,
            _items,
            position.dx,
            position.dy,
            size.height,
          )
        : inActiveZone(rest, position.dx, position.dy, size.height);
    setState(() {
      // Magnification follows the pointer without inertia; on exit the last x stays.
      if (inside) _pointerX = position.dx;
    });
    _setInside(inside);
    _showLabel(inside ? itemAt(current, _items, position.dx) : null);
    // Leaving the Dock (but not along the bottom row) hides it again.
    if (widget.autohide && !inside && position.dy < size.height - 1) {
      _hideDock();
    }
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final size = constraints.biggest;
        final items = _displayItems(size);
        final dragging = _drag != null;
        final l = layout(
          presenceOf: _presenceOf,
          metrics: _m,
          items,
          width: size.width,
          height: size.height,
          pointerX: _pointerX,
          amount: _amount,
          magnified: _magnified,
        );
        final (bx, by, bw, bh) = l.background;
        final label = _labelIndex;
        // No label for the open stack's own item (its button closes the stack); the other
        // labels stay below the stack (DockGlass).
        final hovered =
            !dragging &&
                label != null &&
                label < _items.length &&
                label != _stackIndex
            ? label
            : null;
        // During and right after the drag the icons slide into place (~265 ms).
        final settle = _settleUntil;
        final sliding =
            dragging ||
            _folderGap != null ||
            (settle != null && DateTime.now().isBefore(settle));
        if (sliding && !dragging) {
          WidgetsBinding.instance.scheduleFrame();
        }
        if (widget.launcher.values.any((s) => s.urgentSince != null)) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) setState(() {});
          });
        }
        // The menu: above the item, the tip of its arrow 12k over the shelf. Until the
        // menu has been laid out its rectangle is estimated from the rows.
        final menuIndex = _menuIndex;
        final menuAnchor = menuIndex == null || menuIndex >= l.cells.length
            ? null
            : Offset(l.center(menuIndex), by - _m.labelOffset);
        final menuRect = menuAnchor == null
            ? null
            : _menuRects.isNotEmpty
            ? _menuRects.first
            : () {
                final height =
                    8.0 + 24.0 * _menuEntries.length + dockArrowDepth;
                return Rect.fromLTWH(
                  menuAnchor.dx - AmalfiMenuMetrics.minWidth / 2,
                  menuAnchor.dy - height,
                  AmalfiMenuMetrics.minWidth,
                  height,
                );
              }();
        // Outside the Dock (and the menu) the pointer goes to the windows below.
        final dockRect = Rect.fromLTRB(
          bx - dockInputMargin,
          by -
              dockInputAbove -
              math.max(0, (widget.magnified ?? 0) - _m.canvas),
          bx + bw + dockInputMargin,
          size.height,
        );
        // The alert (D-07): centered horizontally, the top at ~20 % of the screen, when
        // the surface is as tall as the screen.
        final screen = _screen;
        final alert = widget.alert;
        if (alert != null && screen == null) {
          LayerControl.outputSize().then((size) {
            if (mounted)
              setState(() => _screen = size ?? const Size(1280, 800));
          });
        }
        final alertRect = alert == null || screen == null
            ? null
            : Rect.fromLTWH(
                (size.width - trashAlertSize.width) / 2,
                screen.height * trashAlertTop - (screen.height - size.height),
                trashAlertSize.width,
                trashAlertSize.height,
              );
        // The stack: it needs the whole screen above the Dock.
        final stackIndex = _stackIndex;
        if (stackIndex != null && screen == null) {
          LayerControl.outputSize().then((value) {
            if (mounted)
              setState(() => _screen = value ?? const Size(1280, 800));
          });
        }
        final zoneTop = by - activeZoneAbove;
        Widget? stackWidget;
        if (stackIndex != null && stackIndex < l.cells.length) {
          final iconX = l.center(stackIndex);
          final item = _items[stackIndex];
          double progress() => Curves.easeOut.transform(_stackAnim.value);
          if (_stackView == StackView.fan) {
            final count = math.min(_stackEntries.length, fanMax) + 1;
            final top = fanSlot(count - 1);
            _stackRect = Rect.fromLTRB(
              iconX - 420,
              zoneTop + top.center.dy - top.side,
              iconX + top.center.dx + top.side / 2 + 120,
              zoneTop,
            );
            stackWidget = AnimatedBuilder(
              animation: _stackAnim,
              builder: (context, _) => Positioned.fill(
                child: FanStack(
                  entries: _stackEntries,
                  anchor: Offset(iconX, zoneTop),
                  progress: progress(),
                  onOpen: (e) => _openFromStack(e.path),
                  onOpenFolder: () => _openFromStack(item.folder ?? ''),
                  onDragOut: _dragFromStack,
                ),
              ),
            );
          } else {
            final panel = gridPanelSize(_stackEntries.length, zoneTop - 40);
            final double left = (iconX - panel.width / 2)
                .clamp(8.0, math.max(8.0, size.width - panel.width - 8))
                .toDouble();
            final rect = Rect.fromLTWH(
              left,
              by - gridTipAboveDock - gridTipHeight - panel.height,
              panel.width,
              panel.height,
            );
            _stackRect = rect;
            stackWidget = AnimatedBuilder(
              animation: _stackAnim,
              builder: (context, _) => GridStack(
                title: item.label,
                entries: _stackEntries,
                rect: rect,
                tipX: iconX - left,
                progress: progress(),
                onOpen: (e) => _openFromStack(e.path),
                onOpenFolder: () => _openFromStack(item.folder ?? ''),
                onDragOut: _dragFromStack,
              ),
            );
          }
        } else {
          _stackRect = null;
        }
        _syncLayer(dockRect, menuRect, alertRect, size);
        return Listener(
          behavior: HitTestBehavior.translucent,
          onPointerDown: (e) => _pointerDown(e, l, size),
          onPointerMove: (e) => _pointerMove(e.localPosition, size),
          onPointerUp: (e) => _pointerUp(e, l, size),
          onPointerCancel: (_) {
            if (_resize != null) {
              setState(() {
                _resize = null;
                _resizeCanvas = null;
              });
            }
            if (_drag != null) setState(() => _drag = null);
            _longPress?.cancel();
            _labelOff?.cancel();
            _longPress = null;
            if (_menuIndex == null) setState(() => _pressed = null);
          },
          child: MouseRegion(
            cursor: _resize != null || _overSeparator
                ? SystemMouseCursors.resizeUpDown
                : MouseCursor.defer,
            onHover: (event) => _hover(event.localPosition, size),
            onExit: (_) {
              if (_overSeparator) setState(() => _overSeparator = false);
              _setInside(false);
              _showLabel(null);
              _hideDock();
            },
            child: Transform.translate(
              // Auto-hide: the whole Dock slides below the screen.
              offset: Offset(
                0,
                (1 - _shown.value) * (_m.backgroundHeight + bottomGap + 2),
              ),
              child: Stack(
                children: [
                  Positioned(
                    left: bx,
                    top: by,
                    width: bw,
                    height: bh,
                    child: ProfiledGlass(
                      profile: ShellGlassProfile.dock,
                      radius: _m.radius,
                      material: GlassMaterial.regular,
                      // The lowest: the stack, the labels and the menus sit on it.
                      group: DockGlass.dock,
                      child: SizedBox.expand(),
                    ),
                  ),
                  Positioned.fill(
                    child: CustomPaint(
                      painter: _DockPainter(
                        items,
                        l,
                        size.height,
                        DockColors.of(context),
                        indicators: widget.indicators,
                        launcher: widget.launcher,
                        opacities: [
                          for (final item in items) _live[item]?.opacity ?? 1,
                        ],
                        dots: [for (final item in items) _live[item]?.dot ?? 1],
                      ),
                    ),
                  ),
                  for (var i = 0; i < items.length; i++)
                    if (items[i].minimizedWindow != null)
                      _windowCell(
                        l,
                        i,
                        iconRect(l, i, size.height).$1,
                        iconRect(l, i, size.height).$2,
                        iconRect(l, i, size.height).$3,
                        items[i].icon,
                        items[i],
                      )
                    else if (items[i].icon != null || i == _stackIndex)
                      _icon(
                        l,
                        i,
                        size.height,
                        items[i].icon,
                        items[i],
                        sliding,
                      ),
                  if (widget.launcher.values.any((s) => s.badge != null))
                    Positioned.fill(
                      child: IgnorePointer(
                        child: CustomPaint(
                          painter: _BadgePainter(
                            items,
                            l,
                            size.height,
                            widget.launcher,
                            [for (final item in items) _bounceOf(item)],
                          ),
                        ),
                      ),
                    ),
                  if (_drag case final drag?) ..._dragged(drag),
                  if (_ghost case final ghost?) _faded(ghost),
                  if (hovered != null && _menuIndex == null)
                    _label(
                      l,
                      hovered,
                      size.height,
                      _labelAnim.value.clamp(0.0, 1.0),
                    ),
                  // Once the surface has grown for it: Ossido may keep it below the menu bar's
                  // exclusive zone (770 of 800), so "as tall as the screen" never came and the
                  // open stack stayed invisible.
                  if (stackWidget != null && _surfaceGrown(size.height))
                    Positioned.fill(
                      child: Focus(
                        focusNode: _stackFocus,
                        onKeyEvent: (node, event) {
                          if (event is KeyDownEvent &&
                              event.logicalKey == LogicalKeyboardKey.escape) {
                            // A menu open over the stack closes first, like every menu.
                            if (_menuIndex != null) {
                              _menuKey.currentState?.close();
                            } else {
                              _closeStack();
                            }
                            return KeyEventResult.handled;
                          }
                          return KeyEventResult.ignored;
                        },
                        // The stack repaints on its own layer while it opens and closes.
                        child: RepaintBoundary(
                          child: StackUnderMenu(
                            menu: menuRect == null ? const [] : [menuRect],
                            child: Stack(
                              clipBehavior: Clip.none,
                              children: [stackWidget],
                            ),
                          ),
                        ),
                      ),
                    ),
                  if (alertRect != null && alert != null && alertRect.top >= 0)
                    Positioned.fromRect(rect: alertRect, child: alert),
                  if (menuAnchor != null)
                    Positioned.fill(
                      child: DockMenuHost(
                        key: _menuKey,
                        entries: _menuEntries,
                        anchorX: menuAnchor.dx,
                        bottom: menuAnchor.dy,
                        group: 0,
                        onDismissed: _closeMenu,
                        onRectChanged: (rect) => setState(() {
                          _menuRects = [rect];
                        }),
                        onSubmenusChanged: (rects) => setState(() {
                          _submenuRects = rects;
                        }),
                      ),
                    ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  /// The dragged icon: full size, opaque, hooked to the grab point; above it, the "Remove"
  /// capsule when it is high enough.
  List<Widget> _dragged(_DockDrag drag) {
    final source = drag.item.icon;
    final at = drag.pointer - drag.grab;
    return [
      if (source != null)
        Positioned(
          left: at.dx,
          top: at.dy,
          width: drag.side,
          height: drag.side,
          child: _drawn(
            (body) =>
                SilicaIcon(source, side: body, besideApps: !drag.item.isPlace),
            drag.side,
            place: drag.item.isPlace,
          ),
        ),
      if (drag.remove)
        _capsule(
          l10nOf(context).dockRemove,
          at.dx + drag.side / 2,
          at.dy - _m.labelGapFor(drag.side),
          1,
        ),
    ];
  }

  /// The removed item fades out in place with its capsule.
  Widget _faded(_Ghost ghost) {
    final t = ghost.elapsed.inMicroseconds / removeFade.inMicroseconds;
    final opacity = (1 - t).clamp(0.0, 1.0);
    final source = ghost.item.icon;
    return Positioned.fill(
      child: IgnorePointer(
        child: Opacity(
          opacity: opacity,
          child: Stack(
            children: [
              if (source != null)
                Positioned(
                  left: ghost.at.dx,
                  top: ghost.at.dy,
                  width: ghost.side,
                  height: ghost.side,
                  child: _drawn(
                    (body) => SilicaIcon(
                      source,
                      side: body,
                      besideApps: !ghost.item.isPlace,
                    ),
                    ghost.side,
                    place: ghost.item.isPlace,
                  ),
                ),
              _capsule(
                l10nOf(context).dockRemove,
                ghost.at.dx + ghost.side / 2,
                ghost.at.dy - _m.labelGapFor(ghost.side),
                1,
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// A capsule like the label, with its tip at (`centerX`, `tipY`).
  Widget _capsule(String text, double centerX, double tipY, double opacity) {
    final style = dockLabelStyle(context);
    final width = dockLabelWidth(text, style);
    return Positioned(
      left: centerX - width / 2,
      top: tipY - labelTip - labelHeight,
      width: width,
      height: labelHeight + labelTip,
      child: Opacity(
        opacity: opacity,
        child: DockLabel(text: text, style: style, arrow: true),
      ),
    );
  }

  /// A minimized window (D-08): the 57 × 47 thumbnail centered in the cell, flush with the
  /// top of the icon, with the 18 pt app icon in the bottom right corner, 4 pt lower than
  /// the thumbnail.
  Widget _windowCell(
    Layout l,
    int index,
    double x,
    double y,
    double side,
    IconSource? source,
    Item item,
  ) {
    final k = side / iconSize;
    final thumbWidth = minimizedThumbWidth * k;
    final thumbHeight = minimizedThumbHeight * k;
    final badge = minimizedBadge * k;
    final left = l.center(index) - thumbWidth / 2;
    final image = item.thumbnail;
    return Positioned(
      key: ValueKey(_key(item)),
      left: left,
      top: y,
      width: thumbWidth,
      height: thumbHeight + minimizedBadgeBelow * k,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Positioned(
            left: 0,
            top: 0,
            width: thumbWidth,
            height: thumbHeight,
            child: image == null
                ? const DecoratedBox(
                    decoration: BoxDecoration(color: Color(0x33FFFFFF)),
                  )
                : RawImage(image: image, fit: BoxFit.contain),
          ),
          if (source != null)
            Positioned(
              left: thumbWidth - badge + 2 * k,
              top: thumbHeight - badge + minimizedBadgeBelow * k,
              width: badge,
              height: badge,
              child: SilicaIcon(source, side: badge),
            ),
        ],
      ),
    );
  }

  /// The icon of an app: layered from silica-icons, or from the system theme.
  Widget _icon(
    Layout l,
    int index,
    double height,
    IconSource? source,
    Item item,
    bool sliding,
  ) {
    final (x, y, side) = iconRect(l, index, height);
    // With the stack open the folder icon becomes the button that closes it (D-06).
    final closing = _stackIndex == index;
    final pile = closing ? null : _pileOf(item);
    final icon = closing
        // The silica-icons "Stack Close" badge on the folder's canvas (dock-stacks.md,
        // "The icon in the Dock").
        ? StackCloseButton(
            side: side,
            dark:
                MediaQuery.maybePlatformBrightnessOf(context) ==
                Brightness.dark,
            pressed: _pressed == index,
          )
        : pile != null && pile.isNotEmpty
        ? _drawn(
            (body) => PileIcon(sources: pile, side: body),
            side,
            place: true,
          )
        : source == null
        ? const SizedBox.shrink()
        : _drawn(
            (body) => SilicaIcon(source, side: body, besideApps: !item.isPlace),
            side,
            place: item.isPlace,
          );
    final live = _live[item];
    // The launch hop: 17 pt (about 18 with the magnification) in the same animation.
    final hop = (live?.hopFraction ?? 0) * (launchHopPeak * _m.k);
    return AnimatedPositioned(
      key: ValueKey(_key(item)),
      duration: sliding ? gapSlide : Duration.zero,
      curve: Curves.easeInOut,
      left: x,
      top: y - math.max(hop, _bounceOf(item)),
      width: side,
      height: side,
      // Pressed: the icon darkens (D-02).
      child: Opacity(
        opacity: live?.opacity ?? 1,
        // The close tile darkens on its own (measured amounts).
        child: !sliding && _pressed == index && !closing
            ? ColorFiltered(
                colorFilter: const ColorFilter.mode(
                  pressedShade,
                  BlendMode.srcATop,
                ),
                child: icon,
              )
            : icon,
      ),
    );
  }

  /// An icon of the canvas `side` at the origin of its box: the canvas of an app, the body of
  /// a place (see [iconBody]); it spills over the box, unclipped.
  Widget _drawn(
    Widget Function(double body) build,
    double side, {
    required bool place,
  }) {
    final (x, y, body) = iconBody(side, place: place);
    return Stack(
      clipBehavior: Clip.none,
      children: [
        Positioned(
          left: x,
          top: y,
          width: body,
          height: body,
          child: build(body),
        ),
      ],
    );
  }

  Widget _label(Layout l, int index, double height, double opacity) {
    final (_, iconTop, side) = iconRect(l, index, height);
    // During the bounce the label rises with the icon.
    final iconTopY = iconTop - _bounce(index);
    final tipY = iconTopY - _m.labelGapFor(side);
    final style = dockLabelStyle(context);
    final width = dockLabelWidth(_items[index].label, style);
    return Positioned(
      left: l.center(index) - width / 2,
      top: tipY - labelTip - labelHeight,
      width: width,
      height: labelHeight + labelTip,
      child: Opacity(
        opacity: opacity,
        child: DockLabel(text: _items[index].label, style: style, arrow: true),
      ),
    );
  }
}

/// A drag on the separator: where it began and the canvas then.
class _Resize {
  _Resize(this.downY, this.canvas);
  final double downY;
  final double canvas;
}

class _DockPainter extends CustomPainter {
  _DockPainter(
    this.items,
    this.l,
    this.height,
    this.colors, {
    this.indicators = true,
    this.launcher = const {},
    this.opacities = const [],
    this.dots = const [],
  });

  /// Opacity of every icon and of its running dot, by item index (1: missing).
  final List<double> opacities;
  final List<double> dots;

  final List<Item> items;
  final Layout l;
  final double height;
  final DockColors colors;

  /// With indicators off nothing else changes (SH-DK-12).
  final bool indicators;
  final Map<String, LauncherState> launcher;

  @override
  void paint(Canvas canvas, Size size) {
    final (_, by, _, bh) = l.background;
    for (var i = 0; i < items.length; i++) {
      final item = items[i];
      if (item.kind == ItemKind.separator) {
        final x =
            (l.center(i) -
                    l.metrics.separatorWidth / 2 +
                    l.metrics.separatorLine)
                .roundToDouble();
        canvas.drawRect(
          Rect.fromLTWH(
            x,
            by + l.metrics.separatorInset,
            1,
            l.metrics.separatorHeight,
          ),
          Paint()
            ..color = colors.separator.withValues(
              alpha: colors.separator.a * l.presence[i],
            ),
        );
        continue;
      }
      if (identical(item, gapItem) || item.minimizedWindow != null) continue;
      final (ix, iy, side) = iconRect(l, i, height);
      final presence = i < opacities.length ? opacities[i] : 1.0;
      final dot = i < dots.length ? dots[i] : 1.0;
      if (item.icon == null) {
        if (presence < 1) {
          canvas.saveLayer(
            null,
            Paint()..color = Color.fromRGBO(0, 0, 0, presence),
          );
        }
        final (bx, byy, body) = iconBody(side, place: item.isPlace);
        paintIcon(
          canvas,
          Rect.fromLTWH(ix + bx, iy + byy, body, body),
          item.label,
          i,
        );
        if (presence < 1) canvas.restore();
      }
      if (launcher[item.id]?.progress case final progress?) {
        // The progress bar replaces the dot (SH-DK-30).
        final bar = RRect.fromRectAndRadius(
          Rect.fromCenter(
            center: Offset(l.center(i), by + bh - l.metrics.dotAbove),
            width: progressWidth,
            height: progressHeight,
          ),
          const Radius.circular(progressHeight / 2),
        );
        canvas.save();
        canvas.clipRRect(bar);
        canvas.drawRect(
          Rect.fromLTWH(bar.left, bar.top, bar.width * progress, bar.height),
          Paint()..color = colors.indicator,
        );
        canvas.restore();
        canvas.drawRRect(
          bar.deflate(0.5),
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1
            ..color = colors.indicator,
        );
      } else if (item.running && indicators) {
        canvas.drawCircle(
          Offset(l.center(i), by + bh - l.metrics.dotAbove),
          l.metrics.dot / 2,
          Paint()
            ..color = colors.indicator.withValues(
              alpha: colors.indicator.a * dot,
            ),
        );
      }
    }
  }

  @override
  bool shouldRepaint(_DockPainter old) => true;
}

/// The badges (SH-DK-29): above the icons, following their bounce and magnification.
class _BadgePainter extends CustomPainter {
  _BadgePainter(this.items, this.l, this.height, this.launcher, this.lifts);

  final List<Item> items;
  final Layout l;
  final double height;
  final Map<String, LauncherState> launcher;
  final List<double> lifts;

  @override
  void paint(Canvas canvas, Size size) {
    for (var i = 0; i < items.length && i < l.cells.length; i++) {
      final text = launcher[items[i].id]?.badge;
      if (text == null || l.presence[i] < 1) continue;
      final (ix, iy, side) = iconRect(l, i, height);
      // Measured at the 48 pt icon: everything scales with the magnified icon.
      final k = side / iconSize;
      // Cells are (left, width).
      final cellRight = l.cells[i].$1 + l.cells[i].$2;
      final w = badgeWidth(text) * k;
      final rect = Rect.fromLTWH(
        cellRight - badgeRightInset * k - w,
        iy - (badgeIconTop - badgeTop) * k - (i < lifts.length ? lifts[i] : 0),
        w,
        badgeHeight * k,
      );
      canvas.drawRRect(
        RRect.fromRectAndRadius(rect, Radius.circular(rect.height / 2)),
        Paint()..color = badgeFill,
      );
      // Provisional: the text size and weight are not measured (~14 pt semibold).
      final painter = TextPainter(
        text: TextSpan(
          text: text,
          style: TextStyle(
            fontFamily: uiFontFamily,
            package: 'silica_ui',
            fontSize: 14 * k,
            fontWeight: FontWeight.w600,
            color: const Color(0xFFFFFFFF),
          ),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
      painter.paint(
        canvas,
        rect.center - Offset(painter.width / 2, painter.height / 2),
      );
      painter.dispose();
    }
  }

  @override
  bool shouldRepaint(_BadgePainter old) => true;
}

/// The Dock label (kit `Dock/Label`): Glass/Thick drawn by Ossido (`tooltip` kind), radius 8, a
/// body [labelHeight] tall and, with [arrow], the 6 pt arrow under it (the widget is then
/// `height + dockArrowDepth` tall), and the Thick drop shadow (0, 12, blur 32) outside the
/// shape.
class DockLabel extends StatelessWidget {
  const DockLabel({
    super.key,
    required this.text,
    required this.style,
    this.group = DockGlass.label,
    this.height = labelHeight,
    this.shadow = true,
    this.arrow = false,
  });

  final String text;
  final TextStyle style;

  /// Its own glass group (glass_groups.dart): below the menus, never merged with the stack's
  /// panel or the other labels.
  final int group;

  /// The height of the body (the kit's label: 28).
  final double height;

  /// The Thick shadow under the label (the fan's labels have none, DOCK-FAN).
  final bool shadow;

  /// The arrow towards the icon, under the body (the fan's labels have none).
  final bool arrow;

  /// The glass the label declares (`materials.md`: tooltips are Thick).
  static const GlassMaterial material = GlassMaterial.thick;

  @override
  Widget build(BuildContext context) {
    final brightness = AmalfiTheme.of(context).brightness;
    return LayoutBuilder(
      builder: (context, box) {
        final capsule = Align(
          alignment: Alignment.topCenter,
          child: SizedBox(
            height: height,
            child: ProfiledGlass(
              profile: ShellGlassProfile.tooltip,
              radius: labelRadius,
              material: material,
              group: group,
              tip: arrow
                  ? GlassTip(
                      x: box.maxWidth / 2,
                      width: dockArrowHalfBase * 2,
                      height: dockArrowDepth,
                    )
                  : null,
              child: Center(
                child: Text(
                  text,
                  style: style,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ),
          ),
        );
        if (!shadow) return capsule;
        final shadows = AmalfiGlass.shadowsOf(
          AmalfiMaterials.thick,
          brightness,
        );
        final tip = arrow ? dockArrowDepth : 0.0;
        return CachedShadow(
          cacheKey: Object.hash(box.maxWidth, height, brightness, arrow),
          bleed: 80,
          paintShadow: _LabelShadow(box.maxWidth, height + tip, shadows).paint,
          child: capsule,
        );
      },
    );
  }
}

/// The label's shadow: the token's drop shadows of the shape (body and arrow), outside it only.
class _LabelShadow extends CustomPainter {
  const _LabelShadow(this.width, this.height, this.shadows);

  /// The frame, with the arrow if there is one (the body is 6 pt shorter then).
  final double width;
  final double height;
  final List<BoxShadow> shadows;

  @override
  void paint(Canvas canvas, Size size) {
    final frame = Rect.fromLTWH(0, 0, width, height);
    final shape = height > labelHeight
        ? const DockArrowBorder(radius: labelRadius).getOuterPath(frame)
        : Path.combine(
            PathOperation.union,
            Path(),
            const AmalfiSmoothBorder(radius: labelRadius).getOuterPath(frame),
          );
    canvas
      ..save()
      ..clipPath(
        Path.combine(
          PathOperation.difference,
          Path()..addRect(frame.inflate(80)),
          shape,
        ),
      );
    for (final shadow in shadows) {
      canvas.drawPath(
        shape.shift(shadow.offset),
        Paint()
          ..color = shadow.color
          ..maskFilter = MaskFilter.blur(BlurStyle.normal, shadow.blurSigma),
      );
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(_LabelShadow old) =>
      old.width != width || old.height != height || old.shadows != shadows;
}
