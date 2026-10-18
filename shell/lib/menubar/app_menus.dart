// The frontmost app's own menus in the bar (silica-menu-v1, #25; menubar.md and the apps'
// notes in silica-reference). The app publishes them; the bar adds what the system provides:
// the app menu's system items (Services, Quit…), the window part of the Window menu (Minimize,
// Fill, Move & Resize, Full screen, Bring All to Front and the list of the app's windows) and
// Silica Help when the app has no help of its own. An item with an id goes back to the app;
// an item with a system role and no id is run here; anything else is shown disabled, never
// dead.

import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../system/app_menu_model.dart';
import '../system/shell_channel.dart' show Toplevel;
import 'menus.dart' show silicaHelpUrl;

/// What the bar can do with the app's menus.
class AppMenuActions {
  const AppMenuActions({
    required this.activate,
    required this.run,
    this.activateWindow,
    this.activateApp,
    this.openUrl,
    this.systemHide = false,
  });

  /// The app receives the chosen item.
  final void Function(int id) activate;

  /// Runs a command of Ossido's catalog.
  final void Function(String command) run;

  /// Brings one window of the app to the front, the others stay (silica-ui#101).
  final void Function(String id)? activateWindow;

  /// Brings all of the app's windows forward, this one on top (Bring All to Front);
  /// without it, [activateWindow].
  final void Function(String id)? activateApp;
  final void Function(String url)? openUrl;

  /// Ossido can hide apps (`app.hide`, `app.hide_others`, `app.show_all`).
  final bool systemHide;
}

/// A menu of the bar built from the app's model.
class BarMenu {
  const BarMenu({required this.title, required this.entries, this.app = false});

  final String title;
  final List<MenuEntry> entries;

  /// The app menu: its title is the app's name, in bold.
  final bool app;
}

/// The app's menus as the bar shows them: the app menu first, then the others in the app's
/// order, the Window menu completed by the system (added before Help if the app has none) and
/// Help. Menus left without items are dropped: no dead titles in the bar.
List<BarMenu> composeAppMenus(
  List<AppMenu> menus, {
  required String appName,
  required AppMenuActions actions,
  List<Toplevel> windows = const [],
  String focusedWindow = '',
  AppLocalizations? texts,
}) {
  final l = texts ?? englishTexts;
  final appMenu = menus.where((m) => m.role == 'app').firstOrNull;
  // The name the app gives itself (localized by the app) also names the system items.
  final name = appMenu == null || appMenu.label.isEmpty
      ? appName
      : appMenu.label;
  final build = _Builder(l, name, actions);
  final out = <BarMenu>[
    BarMenu(
      title: name,
      app: true,
      entries: appMenu == null
          ? build.entries(const [AppMenuItem(role: 'quit')])
          : build.entries(appMenu.items),
    ),
  ];
  final (systemPart, windowList) = build.windowPart(windows, focusedWindow);
  final window = _tidy([...systemPart, const MenuDivider(), ...windowList]);
  var hasWindow = false;
  for (final menu in menus) {
    if (menu.role == 'app') continue;
    if (menu.role == 'help' && !hasWindow) {
      out.add(BarMenu(title: l.menuWindow, entries: window));
      hasWindow = true;
    }
    final entries = switch (menu.role) {
      // The system items first, then the app's own, then the window list.
      'window' => _tidy([
        ...systemPart,
        const MenuDivider(),
        ...build.entries(menu.items),
        const MenuDivider(),
        ...windowList,
      ]),
      'help' => build.help(menu.items),
      _ => build.entries(menu.items),
    };
    if (menu.role == 'window') hasWindow = true;
    out.add(
      BarMenu(
        title: menu.label.isNotEmpty
            ? menu.label
            : switch (menu.role) {
                'window' => l.menuWindow,
                'help' => l.menuHelp,
                _ => '',
              },
        entries: entries,
      ),
    );
  }
  if (!hasWindow) out.add(BarMenu(title: l.menuWindow, entries: window));
  return [
    for (final menu in out)
      if (menu.entries.isNotEmpty && menu.title.isNotEmpty) menu,
  ];
}

class _Builder {
  _Builder(this.l, this.app, this.actions);

  final AppLocalizations l;
  final String app;
  final AppMenuActions actions;

  List<MenuEntry> entries(List<AppMenuItem> items) => _tidy([
    for (final item in items)
      if (entry(item) case final entry?) entry,
  ]);

  MenuEntry? entry(AppMenuItem item) {
    if (item.separator) return const MenuDivider();
    final shortcut = item.shortcut?.text;
    final id = item.id;
    if (item.items case final children?) {
      final label = item.label.isNotEmpty ? item.label : roleLabel(item.role);
      if (label == null) return null;
      return MenuAction(
        label,
        enabled: item.enabled,
        submenu: entries(children),
      );
    }
    // The app runs it (a role only gives the defaults).
    if (id != null) {
      final label = item.label.isNotEmpty ? item.label : roleLabel(item.role);
      if (label == null) return null;
      return MenuAction(
        label,
        shortcut: shortcut ?? roleShortcut(item.role),
        checked: item.checked,
        enabled: item.enabled,
        onSelected: () => actions.activate(id),
      );
    }
    if (item.role.isNotEmpty) {
      if (system(item) case final run?) return run;
      // A role the bar does not run, with no id: an edit item with nothing to act on is
      // shown disabled, as on the reference; an action nobody can do is left out.
      final label = item.label.isNotEmpty ? item.label : roleLabel(item.role);
      if (label == null || _actionRoles.contains(item.role)) return null;
      return MenuAction(
        label,
        shortcut: shortcut ?? roleShortcut(item.role),
        checked: item.checked,
        enabled: false,
      );
    }
    // Nothing to run: shown disabled (a note, a placeholder), never a dead control.
    if (item.label.isEmpty) return null;
    return MenuAction(
      item.label,
      shortcut: shortcut,
      checked: item.checked,
      enabled: false,
    );
  }

  /// The label the system gives a role, null for a role it does not know.
  String? roleLabel(String role) => switch (role) {
    'about' => l.aboutApp(app),
    'quit' => l.quitApp(app),
    'hide' => l.hideApp(app),
    'hide_others' => l.hideOthers,
    'show_all' => l.showAll,
    'services' => l.services,
    'close' => l.closeWindow,
    'close_all' => l.closeAll,
    'minimize' => l.minimize,
    'fullscreen' => l.fullScreen,
    'bring_all_to_front' => l.bringAllToFront,
    'help' => l.appHelp(app),
    'settings' => l.appSettings,
    'undo' => l.editUndo,
    'redo' => l.editRedo,
    'cut' => l.editCut,
    'copy' => l.editCopy,
    'paste' => l.editPaste,
    'delete' => l.editDelete,
    'select_all' => l.editSelectAll,
    _ => null,
  };

  String? roleShortcut(String role) => switch (role) {
    'quit' => '⌘Q',
    'hide' => '⌘H',
    'hide_others' => '⌥⌘H',
    'close' => '⌘W',
    'close_all' => '⌥⌘W',
    'minimize' => '⌘M',
    'fullscreen' => '⌃⌘F',
    'settings' => '⌘,',
    'undo' => '⌘Z',
    'redo' => '⇧⌘Z',
    'cut' => '⌘X',
    'copy' => '⌘C',
    'paste' => '⌘V',
    'select_all' => '⌘A',
    'help' => '⌘?',
    _ => null,
  };

  /// A system item the bar runs itself; null when the system cannot do it (it is left out).
  MenuEntry? system(AppMenuItem item) {
    final role = item.role;
    final label = item.label.isNotEmpty ? item.label : roleLabel(role);
    if (label == null) return null;
    final shortcut = item.shortcut?.text ?? roleShortcut(role);
    MenuAction run(String command) => MenuAction(
      label,
      shortcut: shortcut,
      enabled: item.enabled,
      onSelected: () => actions.run(command),
    );
    return switch (role) {
      'quit' => run('app.quit'),
      'close' => run('window.close'),
      'minimize' => run('window.minimize'),
      'fullscreen' => run('window.fullscreen'),
      'hide' when actions.systemHide => run('app.hide'),
      'hide_others' when actions.systemHide => run('app.hide_others'),
      'show_all' when actions.systemHide => run('app.show_all'),
      // No services are offered yet: the submenu says so.
      'services' => MenuAction(
        label,
        submenu: [MenuAction(l.noServices, enabled: false)],
      ),
      _ => null,
    };
  }

  /// The parts of the Window menu the system provides: the items that come first (Minimize,
  /// Fill, Move & Resize, Full Screen) and the list that comes last (Bring All to Front and
  /// the windows).
  (List<MenuEntry>, List<MenuEntry>) windowPart(
    List<Toplevel> windows,
    String focused,
  ) {
    MenuAction run(String label, String command, [String? shortcut]) =>
        MenuAction(
          label,
          shortcut: shortcut,
          onSelected: () => actions.run(command),
        );
    final activate = actions.activateWindow;
    final front = windows.where((w) => w.id == focused).firstOrNull;
    final system = <MenuEntry>[
      run(l.minimize, 'window.minimize', '⌘M'),
      run(l.fill, 'window.tile_fill'),
      const MenuDivider(),
      MenuAction(
        l.moveAndResize,
        submenu: [
          run(l.tileLeft, 'window.tile_left'),
          run(l.tileRight, 'window.tile_right'),
          run(l.tileTop, 'window.tile_top'),
          run(l.tileBottom, 'window.tile_bottom'),
          const MenuDivider(),
          run(l.arrangeLeftRight, 'window.arrange_2'),
          run(l.arrangeLeftQuarters, 'window.arrange_3'),
          run(l.arrangeQuarters, 'window.arrange_4'),
        ],
      ),
      run(l.fullScreen, 'window.fullscreen', '⌃⌘F'),
    ];
    final list = <MenuEntry>[
      if (activate != null && windows.isNotEmpty) ...[
        // Every window of the app forward, the front one on top.
        MenuAction(
          l.bringAllToFront,
          onSelected: () =>
              (actions.activateApp ?? activate)((front ?? windows.last).id),
        ),
        const MenuDivider(),
        for (final w in windows)
          MenuAction(
            w.title.isEmpty ? app : w.title,
            checked: w.id == focused,
            onSelected: () => activate(w.id),
          ),
      ],
    ];
    return (_tidy(system), _tidy(list));
  }

  /// The Help menu: the app's help, or Silica Help when it has none.
  List<MenuEntry> help(List<AppMenuItem> items) {
    final own = entries(items);
    if (own.isNotEmpty) return own;
    final open = actions.openUrl;
    return [
      MenuAction(
        l.silicaHelp,
        enabled: open != null,
        onSelected: () => open?.call(silicaHelpUrl),
      ),
    ];
  }
}

/// Roles that only make sense with something to run: without it the item is left out.
const Set<String> _actionRoles = {
  'about',
  'settings',
  'help',
  'hide',
  'hide_others',
  'show_all',
};

/// No separator first, last or twice in a row.
List<MenuEntry> _tidy(List<MenuEntry> entries) {
  final out = <MenuEntry>[];
  for (final e in entries) {
    if (e is MenuDivider && (out.isEmpty || out.last is MenuDivider)) continue;
    out.add(e);
  }
  while (out.isNotEmpty && out.last is MenuDivider) {
    out.removeLast();
  }
  return out;
}
