// The menu bar (Amalfi, kit `Menu Bar/*`): a 32 pt transparent strip on the wallpaper (no
// material), 28 pt items at y 2 with their text 8 pt from the edges, the Silica mark in a 36 pt
// item. Text and glyphs are Text/Primary of the appearance (dark on the light desktop, light on
// the dark one); the item of an open menu or panel has a translucent white capsule
// (`Menu Bar/Selection`, radius 14); label and glyph stay Text/Primary. The status cluster at the
// trailing end is the shared Status Bar (silica_ui `AmalfiStatusBar`, status-bar.md), the same
// component the lock screen shows. Menus open on press below their title (bar_menu_host.dart)
// and follow the pointer to the next title.

import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';

import '../dock/dock.dart' show shellQuote;
import '../faro/launcher.dart' show RecentApps;
import '../l10n/l10n.dart';
import '../system/desktop_entries.dart';
import '../system/open_app.dart';
import '../system/app_menu_model.dart';
import '../system/popup_menu_model.dart';
import '../system/shell_channel.dart';
import '../system/layer_control.dart';
import '../system/media.dart';
import '../system/system_status.dart';
import 'app_menus.dart';
import 'bar_menu_host.dart';
import 'desktop_menu.dart';
import 'logo_menu.dart';
import 'menubar_config.dart';
import 'power.dart';
import 'menus.dart';
import 'status_icons.dart';
import '../status/status_items.dart';
import 'window_menu.dart';

const double barHeight = 32;

/// Auto-hide timings (the Dock's; the bar's were not measured): rest on the edge before the
/// bar comes back, and the slides.
const Duration barAutohideDelay = Duration(milliseconds: 190);
const Duration barRevealDuration = Duration(milliseconds: 190);
const Duration barHideDuration = Duration(milliseconds: 175);

/// An item: 28 tall, 2 pt from the bar's top and bottom; its text is 8 pt from its edges.
const double itemHeight = 28;
const double itemTop = (barHeight - itemHeight) / 2;
const double itemPadding = 8;

/// The corner radius of the active item's capsule (`Menu Bar/Selection`: radius 14).
const double itemRadius = 14;

/// A status item (a glyph of 20 pt): 36 wide. The Silica mark's item is one too.
const double statusCell = 36;

/// The bar's inset from the screen's sides.
const double barSide = 8;

/// An open menu: its body 4 pt left of the title's item (the text lines up: 4 + 8 inside the
/// menu), 8 pt below the bar (the menu at y 40, 4 lower than with the 28 pt bar).
const double menuLeftOfItem = 4;
const double menuBelowBar = 8;
const Duration menuSwitchDelay = Duration(milliseconds: 85);

/// Room below the menu for its shadow, when the surface grows.
const double menuShadowRoom = 40;

/// The bar's text and glyph colour: `Text/Primary` of the appearance.
Color barTextColor(BuildContext context) =>
    AmalfiTheme.of(context).colors.textPrimary;

/// The capsule of the item that has its menu or panel open: the token `Menu Bar/Selection`
/// (silica-design cb7efda8; not in silica_ui's generated tokens yet): translucent white,
/// #FFFFFF66 light and #FFFFFF33 dark. Label and glyph keep `Text/Primary`.
Color barSelectionColor(BuildContext context) => AmalfiTheme.of(context).isDark
    ? const Color(0x33FFFFFF)
    : const Color(0x66FFFFFF);

/// The bar's background when Settings turns it on (menubar.json `background`): the kit has
/// no token for it (the reference bar is bare by default), so a translucent surface tint.
Color barBackgroundColor(BuildContext context) => AmalfiTheme.of(context).isDark
    ? const Color(0x8C1C1C1E)
    : const Color(0x8CF2F2F7);

/// The bar's text, like the kit: titles Body, the app name Headline (the clock is the Status
/// Bar's).
TextStyle barTitleStyle(Color color) => AmalfiType.body(color: color);
TextStyle barNameStyle(Color color) => AmalfiType.headline(color: color);

/// The menus of an app that published none (silica-menu-v1, #25): generic ids, shown with
/// `menuTitle`.
const List<String> genericMenus = ['file', 'edit', 'view', 'window', 'help'];

/// Date and time as in the bar: "Lun 12 ott  22:15" (double space between date and time),
/// as [style] says (Settings › Date & Time): 12 or 24 hours, seconds, the weekday, the date.
String clockText(
  DateTime t, [
  String locale = 'en',
  ClockStyle style = const ClockStyle(),
]) {
  _dates ??= initializeDateFormatting();
  final seconds = style.seconds
      ? ':${t.second.toString().padLeft(2, '0')}'
      : '';
  final String time;
  if (style.hour24) {
    final hh = t.hour.toString().padLeft(2, '0');
    final mm = t.minute.toString().padLeft(2, '0');
    time = '$hh:$mm$seconds';
  } else {
    final hour = t.hour % 12 == 0 ? 12 : t.hour % 12;
    final mm = t.minute.toString().padLeft(2, '0');
    final meridiem = DateFormat('a', locale).format(t);
    time = '$hour:$mm$seconds $meridiem';
  }
  if (style.date == ClockDate.never) {
    // Without the date the weekday alone stays, if asked for.
    if (!style.weekday) return time;
    final day = DateFormat('EEE', locale).format(t);
    return '${_capital(day)} $time';
  }
  final date = DateFormat(
    style.weekday ? 'EEE d MMM' : 'd MMM',
    locale,
  ).format(t);
  return '${_capital(date)}  $time';
}

// The weekday starts with a capital, as in the bar ("Mon", "Lun").
String _capital(String text) =>
    text.isEmpty ? text : text[0].toUpperCase() + text.substring(1);

/// The date symbols of every intl locale, loaded once (synchronously, from the package).
Future<void>? _dates;

/// The rules and owner of what the bar has open (INPUT-01): the app menus belong to the
/// frontmost app, the logo menu to the system, the desktop menu to the desktop's owner;
/// the battery menu is a status menu (a system panel) and the arrangement popover a
/// popover of the frontmost window.
({TransientPolicy policy, String owner}) menuBarTransient({
  String? open,
  bool windowMenu = false,
  bool desktopMenu = false,
  String frontmost = '',
}) {
  if (open == 'logo' || open == 'input') {
    return (policy: TransientPolicy.menu, owner: systemOwner);
  }
  if (open == 'battery') {
    return (policy: TransientPolicy.panel, owner: systemOwner);
  }
  if (open != null) return (policy: TransientPolicy.menu, owner: frontmost);
  if (windowMenu) return (policy: TransientPolicy.popover, owner: frontmost);
  if (desktopMenu) return (policy: TransientPolicy.menu, owner: desktopOwner);
  return (policy: TransientPolicy.menu, owner: systemOwner);
}

/// `child` in the appearance of a window that overrides the system's (`brightness`), as a
/// `SilicaAppearanceScope` would: the library reads the brightness from `MediaQuery`. null
/// leaves the system's.
Widget _withBrightness(Brightness? brightness, Widget child) {
  if (brightness == null) return child;
  return Builder(
    builder: (context) => MediaQuery(
      data: MediaQuery.of(context).copyWith(platformBrightness: brightness),
      child: child,
    ),
  );
}

class MenuBar extends StatefulWidget {
  const MenuBar({
    super.key,
    this.entries,
    this.onSearch,
    this.onClock,
    this.onControls,
    this.system,
    this.now,
    this.media,
    this.config,
  });

  /// Which optional status items to show (Settings › Menu Bar); null: the file, watched.
  final ValueListenable<MenubarConfig>? config;

  /// The installed apps, for the name of the focused app.
  final Map<String, DesktopEntry>? entries;

  /// Click on the Faro icon.
  final VoidCallback? onSearch;

  /// Click on the clock: the Notification Center.
  final VoidCallback? onClock;

  /// Click on the controls icon: the Control Center.
  final VoidCallback? onControls;

  /// System state: Wi-Fi or wired network, battery.
  final SystemSource? system;

  /// The media players, for the Now Playing item.
  final MediaPlayers? media;

  /// Fixed clock (tests).
  final DateTime? now;

  @override
  State<MenuBar> createState() => _MenuBarState();
}

class _MenuBarState extends State<MenuBar> with SingleTickerProviderStateMixin {
  /// This component's layer surface (several share one engine, #201).
  late final LayerControl _layer;

  Timer? _timer;

  /// Auto-hide (menubar.json `autohide`): 1 with the bar in place, 0 slid away above the
  /// screen. The timings are the Dock's (provisional: the bar's were not measured).
  late final AnimationController _shown = AnimationController(
    vsync: this,
    value: 1,
  );
  Timer? _autohideTimer;

  /// The bar has slid away: only a one-pixel strip at the top takes the pointer.
  bool _hidden = false;

  /// The pointer is on the bar (or on the strip that brings it back).
  bool _pointerOnBar = false;

  /// The room the bar claims on the screen now (the exclusive zone), and its width.
  double _zone = barHeight;
  double _width = 4096;

  /// The open menu (`logo`, `app` or the item name) and the item rectangles.
  String? _open;

  /// The entries of the open menu, taken when it opens: a rebuild while it is open (the
  /// frontmost app changing under a click that passes, the clock) must not hand the menu a
  /// new list, which restarts it and stops its closing fade halfway (#149).
  List<MenuEntry>? _openEntries;
  final Map<String, GlobalKey> _itemKeys = {};
  final GlobalKey _rootKey = GlobalKey();
  final GlobalKey<BarMenuHostState> _menuKey = GlobalKey();
  final GlobalKey<WindowMenuPopoverState> _windowKey = GlobalKey();
  final GlobalKey<BarMenuHostState> _desktopKey = GlobalKey();

  /// The next menu opens with its first row highlighted (it was reached with ← or →).
  bool _highlightFirst = false;
  Timer? _switchTimer;

  /// The bar items with a menu: the logo, the app menu, then the app's own menus
  /// (`menu0`, `menu1`… from its model) or the generic ones.
  List<String> get _menuNames {
    final own = _appMenus();
    return [
      'logo',
      'app',
      if (own == null)
        ...genericMenus
      else
        for (var i = 1; i < own.length; i++) 'menu$i',
    ];
  }

  /// The frontmost app's own menus (the app menu first), null if it published none. Built
  /// again only when the model, the windows, the focus or the language change.
  List<BarMenu>? _appMenus() {
    final shell = ShellChannel.instance;
    final state = shell.appMenu.value;
    final key = (
      state,
      shell.toplevels.value,
      shell.focused.value,
      l10nOf(context).localeName,
      shell.states.value['app.frontmost'],
    );
    if (key == _appMenusKey) return _appMenusCache;
    _appMenusKey = key;
    return _appMenusCache = _composeAppMenus(state);
  }

  Object? _appMenusKey;
  List<BarMenu>? _appMenusCache;

  List<BarMenu>? _composeAppMenus(AppMenuState state) {
    final shell = ShellChannel.instance;
    final menus = state.menus;
    if (!state.hasMenus || menus == null) return null;
    final focused = shell.focusedToplevel;
    final appId = focused?.appId ?? state.appId;
    return composeAppMenus(
      menus,
      appName: _appNameFromEntries,
      texts: l10nOf(context),
      windows: [
        for (final t in shell.toplevels.value)
          if (t.appId == appId && appId.isNotEmpty) t,
      ],
      focusedWindow: shell.focused.value,
      actions: AppMenuActions(
        activate: (id) => shell.activateMenuItem(state.serial, id),
        run: (id) => shell.execute(id),
        activateWindow: (id) => shell.activate(id),
        openUrl: (url) => shell.launch('xdg-open ${shellQuote(url)}'),
        // Ossido hides apps (app.hide, app.hide_others, app.show_all).
        systemHide: true,
      ),
    );
  }

  GlobalKey _keyFor(String name) => _itemKeys.putIfAbsent(name, GlobalKey.new);

  List<MenuEntry> _entries(String name) {
    final own = name == 'app' || name.startsWith('menu') ? _appMenus() : null;
    if (own != null) {
      final index = name == 'app' ? 0 : int.tryParse(name.substring(4)) ?? -1;
      return index >= 0 && index < own.length ? own[index].entries : const [];
    }
    return _systemEntries(name);
  }

  List<MenuEntry> _systemEntries(String name) => name == 'battery'
      ? _batteryMenu()
      : name == 'input'
      ? _inputMenu()
      : name == 'logo'
      ? logoMenu(
          l: l10nOf(context),
          run: (id) => ShellChannel.instance.execute(id),
          launch: _launch,
          open: (path) =>
              ShellChannel.instance.launch('xdg-open ${shellQuote(path)}'),
          option: _option,
          user: _user,
          settings: _settingsApp,
          recent: _recent,
          clearRecent: () {
            RecentApps().clear();
            RecentItems.clearDocuments();
          },
          power: (action, {required now}) => _requestPower(action, now: now),
          powerEnabled: powerAvailable(),
        )
      : barMenu(
          name,
          _appName,
          (id) => ShellChannel.instance.execute(id),
          texts: l10nOf(context),
          openUrl: (url) =>
              ShellChannel.instance.launch('xdg-open ${shellQuote(url)}'),
        );

  /// The battery menu: charge, power source, Battery Settings (not measured on the
  /// reference, which has no battery: provisional).
  List<MenuEntry> _batteryMenu() {
    final l = l10nOf(context);
    final s = widget.system?.value;
    final settings = _settingsApp;
    return [
      MenuSection(l.batteryMenuTitle),
      MenuAction(
        l.batteryMenuCharge((s?.batteryPercent ?? 0).round()),
        enabled: false,
      ),
      MenuAction(
        (s?.batteryCharging ?? false)
            ? l.batteryMenuPowerAdapter
            : l.batteryMenuOnBattery,
        enabled: false,
      ),
      const MenuDivider(),
      MenuAction(
        l.batteryMenuSettings,
        enabled: settings != null,
        onSelected: settings == null ? null : () => _launch(settings),
      ),
    ];
  }

  /// The input menu (status-bar.md, lock.md › Status items): the input sources, the current
  /// one checked, then Keyboard Settings.
  List<MenuEntry> _inputMenu() {
    final l = l10nOf(context);
    final settings = _settingsApp;
    return [
      ...inputSourceMenu(_inputSources, l.localeName),
      const MenuDivider(),
      MenuAction(
        l.statusInputSettings,
        enabled: settings != null,
        onSelected: settings == null ? null : () => _launch(settings),
      ),
    ];
  }

  /// The input sources Ossido announces (the input source item and its menu).
  final InputSources _inputSources = InputSources();

  /// ⌥ held: the logo menu shows its alternate items (SH-MB-04).
  bool _option = false;
  RecentItems _recent = const RecentItems();
  late final String _user = userFullName();

  bool _keyChanged(KeyEvent event) {
    final option = HardwareKeyboard.instance.isAltPressed;
    if (option != _option) {
      _option = option;
      if (_open == 'logo' && mounted) setState(() {});
    }
    return false;
  }

  void _launch(DesktopEntry entry) {
    openApp(entry);
    RecentApps().add(entry.id);
  }

  /// The Settings app, if installed.
  DesktopEntry? get _settingsApp {
    for (final e in widget.entries?.values ?? const <DesktopEntry>[]) {
      if (e.id.toLowerCase().endsWith('.settings')) return e;
    }
    return null;
  }

  RecentItems _loadRecent() {
    final entries = widget.entries ?? const <String, DesktopEntry>{};
    // Settings › Menu Bar › recent items: how many of each.
    final count = _config.value.recentItems;
    return RecentItems(
      apps: [
        for (final id in RecentApps().load())
          if (entries[id] case final e?) e,
      ].take(count).toList(),
      documents: RecentItems.loadDocuments(limit: count),
    );
  }

  /// The rectangle of a bar item (its box), in the surface.
  Rect? _rectOf(String name) {
    final box = _itemKeys[name]?.currentContext?.findRenderObject();
    final root = _rootKey.currentContext?.findRenderObject();
    if (box is! RenderBox || root is! RenderBox || !box.hasSize) return null;
    return box.localToGlobal(Offset.zero, ancestor: root) & box.size;
  }

  String? _itemAt(Offset p) {
    if (p.dy >= barHeight) return null;
    for (final name in _menuNames) {
      final rect = _rectOf(name);
      if (rect != null && p.dx >= rect.left && p.dx < rect.right) return name;
    }
    return null;
  }

  /// Opens a menu below its item: the body 4 pt left of the box, 8 pt below the bar. The
  /// surface grows down to the menu and its shadow.
  void _openMenu(String name, {bool byKeyboard = false}) {
    final entries = _entries(name);
    final rect = _rectOf(name);
    if (entries.isEmpty || rect == null) return;
    if (name == 'logo') _recent = _loadRecent();
    final anchor = Offset(rect.left - menuLeftOfItem, barHeight + menuBelowBar);
    _switchTimer?.cancel();
    setState(() {
      _open = name;
      _openEntries = entries;
      _highlightFirst = byKeyboard;
    });
    // The surface grows to the screen, but takes the pointer only on the bar and the menus
    // (`_menuRegion`, from the menu's rectangles): the windows below keep their hover, and a
    // press outside arrives from Ossido as transientDismissed (#149).
    _layer.size(0, anchor.dy + estimatedMenuHeight(entries) + menuShadowRoom);
    _layer.inputRegion([_barRect]);
    _layer.keyboard('exclusive');
    _coverScreen();
  }

  static const Rect _barRect = Rect.fromLTWH(0, 0, 16384, barHeight);

  /// A menu of this surface is open with these rectangles (submenus included): the input
  /// region is the bar plus the menus, and Ossido tells about a press outside. Without that
  /// support (an older Ossido) the surface takes every click, as before.
  Future<void> _menuRegion(List<Rect> rects) async {
    if (_service case final request?) {
      final menus = [for (final r in rects) r.inflate(2)];
      final supported = await _layer.popupMenuRegion(request.serial, menus);
      if (mounted && _service != null) {
        _layer.inputRegion(supported ? [_barRect, ...menus] : null);
      }
      return;
    }
    // Already closed (a late report of a menu that went away): no transient again.
    if (_open == null && _windowMenuTip == null && _desktopMenuAt == null) {
      return;
    }
    final menus = [for (final r in rects) r.inflate(2)];
    final (:policy, :owner) = menuBarTransient(
      open: _open,
      windowMenu: _windowMenuTip != null,
      desktopMenu: _desktopMenuAt != null,
      frontmost: ShellChannel.instance.frontmostAppId,
    );
    final supported = await _layer.transient(
      rects: menus,
      policy: policy,
      owner: owner,
    );
    if (!mounted ||
        (_open == null && _windowMenuTip == null && _desktopMenuAt == null)) {
      return;
    }
    _layer.inputRegion(supported ? [_barRect, ...menus] : null);
  }

  /// A press outside the open menu ended it (Ossido, #149).
  void _transientDismissed() {
    if (_open != null) {
      final menu = _menuKey.currentState;
      menu == null ? _closeMenu() : menu.close();
    }
    if (_windowMenuTip != null) {
      final popover = _windowKey.currentState;
      popover == null ? _closeWindowMenu() : popover.close();
    }
    if (_desktopMenuAt != null) {
      final menu = _desktopKey.currentState;
      menu == null ? _closeDesktopMenu() : menu.close();
    }
  }

  Future<void> _coverScreen() async {
    final screen = await LayerControl.outputSize();
    if (!mounted || _open == null || screen == null) return;
    _layer.size(0, screen.height);
  }

  void _closeMenu() {
    _switchTimer?.cancel();
    if (_open == null) return;
    setState(() {
      _open = null;
      _openEntries = null;
    });
    _layer.clearTransient();
    _layer.keyboard('none');
    _layer.inputRegion(_restRegion());
    _layer.size(0, barHeight);
  }

  /// → and ←: the neighbouring menu (only those with items).
  void _neighbour(int delta) {
    final withMenu = [
      for (final name in _menuNames)
        if (_entries(name).isNotEmpty) name,
    ];
    final at = withMenu.indexOf(_open ?? '');
    if (at < 0 || withMenu.isEmpty) return;
    _openMenu(withMenu[(at + delta) % withMenu.length], byKeyboard: true);
  }

  void _pointerDown(PointerDownEvent e) {
    final item = _itemAt(e.localPosition);
    if (item != null && _entries(item).isNotEmpty) {
      // The menu opens on press; on the item already open it closes.
      item == _open ? _menuKey.currentState?.close() : _openMenu(item);
      return;
    }
    if (_open != null) _menuKey.currentState?.pointerDown(e.localPosition);
    if (_desktopMenuAt != null) {
      _desktopKey.currentState?.pointerDown(e.localPosition);
    }
    if (_service != null)
      _serviceKey.currentState?.pointerDown(e.localPosition);
    final popover = _windowKey.currentState;
    if (_windowMenuTip != null &&
        popover != null &&
        popover.outside(e.localPosition)) {
      popover.close();
    }
  }

  void _pointerMoved(Offset p) {
    if (_open == null) return;
    // With a menu open, moving onto another item the menu changes after ~85 ms.
    final item = _itemAt(p);
    if (item != null && item != _open && _entries(item).isNotEmpty) {
      _switchTimer?.cancel();
      _switchTimer = Timer(menuSwitchDelay, () {
        if (_open != null) _openMenu(item);
      });
    } else if (item == null || item == _open) {
      _switchTimer?.cancel();
    }
  }

  MenubarConfigWatcher? _ownConfig;
  ValueListenable<MenubarConfig> get _config =>
      widget.config ?? (_ownConfig ??= MenubarConfigWatcher());

  @override
  void initState() {
    super.initState();
    _layer = LayerControl.of(context);
    _config.addListener(_changed);
    // Seconds shown or hidden: the clock ticks at another pace.
    _config.addListener(_scheduleTick);
    ShellChannel.instance.toplevels.addListener(_changed);
    ShellChannel.instance.focused.addListener(_changed);
    ShellChannel.instance.appMenu.addListener(_changed);
    ShellChannel.instance.states.addListener(_changed);
    ShellChannel.instance.states.addListener(_desktopMenuChanged);
    widget.system?.addListener(_changed);
    ShellChannel.instance.refresh();
    HardwareKeyboard.instance.addHandler(_keyChanged);
    widget.media?.start();
    widget.media?.addListener(_changed);
    _scheduleTick();
    _windowMenus = ShellChannel.instance.windowMenus.listen(
      (menu) => _openWindowMenu(menu.tip, menu.brightness),
    );
    _dismissed = _layer.transientDismissed.listen((_) => _transientDismissed());
    _serviceOpened = ShellChannel.instance.popupMenus.listen(_openService);
    _serviceEnded = ShellChannel.instance.popupMenusClosed.listen(
      _serviceClosedByOssido,
    );
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _syncAutohide();
    });
  }

  StreamSubscription<void>? _dismissed;

  /// The context menu or pop-up button of an app (#241): Ossido hands over the rows and owns
  /// the menu's life, the bar draws it with the same widgets as its own menus.
  StreamSubscription<PopupMenuRequest>? _serviceOpened;
  StreamSubscription<int>? _serviceEnded;
  PopupMenuRequest? _service;
  List<AmalfiMenuEntry>? _serviceEntries;
  final GlobalKey<BarMenuHostState> _serviceKey = GlobalKey();

  /// Whether Ossido already knows the menu is over (a choice, or it ended it itself).
  bool _serviceOver = false;

  Future<void> _openService(PopupMenuRequest request) async {
    _closeMenu();
    _closeWindowMenu();
    _closeDesktopMenu();
    final entries = popupEntries(request.rows, (id) {
      _serviceOver = true;
      unawaited(ShellChannel.instance.popupMenuChoose(request.serial, id));
    });
    if (entries.isEmpty) {
      unawaited(ShellChannel.instance.popupMenuClose(request.serial));
      return;
    }
    final screen = await LayerControl.outputSize();
    if (!mounted) return;
    setState(() {
      _service = request;
      _serviceEntries = entries;
      _serviceOver = false;
    });
    _layer.size(0, screen?.height ?? request.anchor.bottom + 600);
    _layer.inputRegion([_barRect]);
    _layer.keyboard('exclusive');
  }

  /// Ossido ended the menu: a press outside, a change of context.
  void _serviceClosedByOssido(int serial) {
    if (_service?.serial != serial) return;
    _serviceOver = true;
    final menu = _serviceKey.currentState;
    menu == null ? _closeService() : menu.close();
  }

  void _closeService() {
    final request = _service;
    if (request == null) return;
    // Esc or a click on nothing: Ossido has not been told yet.
    if (!_serviceOver) {
      unawaited(ShellChannel.instance.popupMenuClose(request.serial));
    }
    setState(() {
      _service = null;
      _serviceEntries = null;
    });
    _layer.keyboard('none');
    _layer.inputRegion(_restRegion());
    _layer.size(0, barHeight);
  }

  /// The arrangement popover of a window (long press on the green button): the tip at the
  /// point given by Ossido. While it is open the surface covers the screen but takes the
  /// pointer only on the popover; a click outside closes it (#149).
  StreamSubscription<({String id, Offset tip, Brightness? brightness})>?
  _windowMenus;
  Offset? _windowMenuTip;

  /// The popover follows the appearance of the window it was opened from when that window
  /// overrides the system's (null: the system's).
  Brightness? _windowMenuBrightness;

  Future<void> _openWindowMenu(Offset tip, Brightness? brightness) async {
    _closeMenu();
    final screen = await LayerControl.outputSize();
    if (!mounted) return;
    setState(() {
      _windowMenuTip = tip;
      _windowMenuBrightness = brightness;
    });
    _layer.size(0, screen?.height ?? tip.dy + 400);
    _layer.inputRegion([_barRect]);
    _layer.keyboard('exclusive');
  }

  /// The desktop context menu: the wallpaper announces `desktopmenu` = "x y", the file
  /// manager's desktop "x y desktop" or "x y items" (#147).
  Offset? _desktopMenuAt;
  DesktopMenuKind _desktopMenuKind = DesktopMenuKind.wallpaper;

  /// The desktop menu's entries, taken when it opens. The chosen item makes the file manager
  /// announce its new state (Move to Trash, Use Stacks…): a new list on that rebuild would
  /// restart the menu halfway through its closing fade and leave it open.
  List<MenuEntry>? _desktopMenuEntries;
  String _desktopMenuState = '';

  /// Each action sent to the file manager's desktop carries a new serial: the broadcast is a
  /// state, and the same action twice must still arrive.
  int _desktopActionSerial = 0;

  void _desktopMenuChanged() {
    final value = ShellChannel.instance.states.value['desktopmenu'] ?? '';
    if (value == _desktopMenuState) return;
    _desktopMenuState = value;
    final request = parseDesktopMenu(value);
    if (request == null) {
      _closeDesktopMenu(announce: false);
    } else {
      _desktopMenuKind = request.$2;
      _openDesktopMenu(request.$1);
    }
  }

  void _desktopAction(String id) {
    _desktopActionSerial++;
    ShellChannel.instance.broadcast(
      'desktop.action',
      '$id $_desktopActionSerial',
    );
  }

  Future<void> _openDesktopMenu(Offset at) async {
    _closeMenu();
    _closeWindowMenu();
    final screen = await LayerControl.outputSize();
    if (!mounted) return;
    setState(() {
      _desktopMenuAt = at;
      _desktopMenuEntries = _desktopEntries();
    });
    _layer.size(0, screen?.height ?? at.dy + 400);
    _layer.inputRegion([_barRect]);
    _layer.keyboard('exclusive');
  }

  void _closeDesktopMenu({bool announce = true}) {
    if (_desktopMenuAt == null) return;
    setState(() {
      _desktopMenuAt = null;
      _desktopMenuEntries = null;
    });
    if (announce) {
      _desktopMenuState = '';
      ShellChannel.instance.broadcast('desktopmenu', '');
    }
    _layer.clearTransient();
    _layer.keyboard('none');
    _layer.inputRegion(_restRegion());
    _layer.size(0, barHeight);
  }

  List<MenuEntry> _desktopEntries() {
    final settings = _settingsApp;
    final owner = DesktopOwnerState.parse(
      ShellChannel.instance.states.value['desktop.state'] ?? '',
    );
    switch (_desktopMenuKind) {
      case DesktopMenuKind.desktop:
        return ownerDesktopMenu(
          l: l10nOf(context),
          state: owner,
          onAction: _desktopAction,
          onChangeWallpaper: settings == null ? null : () => _launch(settings),
        );
      case DesktopMenuKind.items:
        return ownerItemsMenu(
          l: l10nOf(context),
          state: owner,
          onAction: _desktopAction,
        );
      case DesktopMenuKind.wallpaper:
        break;
    }
    return desktopMenu(
      l: l10nOf(context),
      onNewFolder: () =>
          newFolder(desktopFolder(), l10nOf(context).desktopUntitledFolder),
      onChangeWallpaper: settings == null ? null : () => _launch(settings),
    );
  }

  /// The power confirmation on screen (SH-MB-05), above everything: the surface covers the
  /// screen while it is up.
  PowerAction? _power;

  Future<void> _requestPower(PowerAction action, {required bool now}) async {
    _closeMenu();
    if (now || action == PowerAction.sleep) {
      unawaited(_runPower(action));
      return;
    }
    final screen = await LayerControl.outputSize();
    if (!mounted) return;
    setState(() => _power = action);
    _layer.size(0, screen?.height ?? 800);
    _layer.inputRegion(_restRegion());
    _layer.keyboard('exclusive');
  }

  Future<void> _runPower(PowerAction action) => runPower(
    action,
    logOut: () => ShellChannel.instance.execute('session.quit'),
  );

  void _closePower() {
    if (_power == null) return;
    setState(() => _power = null);
    _layer.keyboard('none');
    _layer.inputRegion(_restRegion());
    _layer.size(0, barHeight);
  }

  void _closeWindowMenu() {
    if (_windowMenuTip == null) return;
    setState(() => _windowMenuTip = null);
    _layer.clearTransient();
    _layer.keyboard('none');
    _layer.inputRegion(_restRegion());
    _layer.size(0, barHeight);
  }

  // The next minute (the next second with seconds shown), aligned with the clock.
  void _scheduleTick() {
    if (widget.now != null) return;
    _timer?.cancel();
    final now = DateTime.now();
    final seconds = _config.value.clock.seconds;
    final next = seconds
        ? DateTime(
            now.year,
            now.month,
            now.day,
            now.hour,
            now.minute,
            now.second + 1,
          )
        : DateTime(now.year, now.month, now.day, now.hour, now.minute + 1);
    _timer = Timer(next.difference(now), () {
      _changed();
      _scheduleTick();
    });
  }

  @override
  void dispose() {
    _config.removeListener(_changed);
    _config.removeListener(_scheduleTick);
    _ownConfig?.dispose();
    _inputSources.dispose();
    _timer?.cancel();
    _switchTimer?.cancel();
    _autohideTimer?.cancel();
    _shown.dispose();
    ShellChannel.instance.toplevels.removeListener(_changed);
    ShellChannel.instance.focused.removeListener(_changed);
    ShellChannel.instance.appMenu.removeListener(_changed);
    ShellChannel.instance.states.removeListener(_changed);
    ShellChannel.instance.states.removeListener(_desktopMenuChanged);
    widget.system?.removeListener(_changed);
    _windowMenus?.cancel();
    _dismissed?.cancel();
    _serviceOpened?.cancel();
    _serviceEnded?.cancel();
    HardwareKeyboard.instance.removeHandler(_keyChanged);
    widget.media?.removeListener(_changed);
    super.dispose();
  }

  void _changed() {
    if (!mounted) return;
    setState(() {});
    _syncAutohide();
  }

  /// A menu, a panel or a card of the bar is open: the bar stays.
  bool get _busy {
    final states = ShellChannel.instance.states.value;
    return _open != null ||
        _service != null ||
        _windowMenuTip != null ||
        _power != null ||
        _desktopMenuAt != null ||
        (states['statusmenu'] ?? '').isNotEmpty ||
        states['controlcenter.open'] == '1' ||
        states['notifications.open'] == '1';
  }

  /// Whether the bar hides itself in the situation of now (before the pointer is asked).
  bool get _autohides => switch (_config.value.autohide) {
    MenubarAutohide.never => false,
    MenubarAutohide.always => true,
    MenubarAutohide.fullscreen =>
      ShellChannel.instance.states.value['fullscreen.front'] == '1',
  };

  /// The input region at rest: all of the surface, or the strip while the bar is away.
  List<Rect>? _restRegion() =>
      _hidden ? [Rect.fromLTWH(0, 0, _width, 1)] : null;

  /// Brings the bar and the room it takes in line with the setting and the pointer.
  void _syncAutohide() {
    final autohides = _autohides;
    // The room: none while the bar can be away, so the windows use the whole screen.
    final zone = autohides ? 0.0 : barHeight;
    if (zone != _zone) {
      _zone = zone;
      _layer.exclusiveZone(zone);
    }
    final hide = autohides && !_busy && !_pointerOnBar;
    if (hide == _hidden) return;
    _hidden = hide;
    if (hide) {
      _shown.animateBack(0, duration: barHideDuration, curve: Curves.easeIn);
      _layer.inputRegion(_restRegion());
    } else {
      _layer.inputRegion(_restRegion());
      _shown.animateTo(1, duration: barRevealDuration, curve: Curves.easeOut);
    }
  }

  /// The pointer moved over the surface: onto the bar (or its strip) or away from it.
  void _edgeMoved(Offset p) {
    if (!_autohides && !_hidden) return;
    final on = p.dy <= (_hidden ? 1 : barHeight);
    if (on == _pointerOnBar) return;
    _pointerOnBar = on;
    _autohideTimer?.cancel();
    // Coming up takes a moment of resting on the edge, going away a moment after leaving.
    _autohideTimer = Timer(barAutohideDelay, _syncAutohide);
  }

  void _pointerLeft() {
    if (!_pointerOnBar) return;
    _pointerOnBar = false;
    _autohideTimer?.cancel();
    _autohideTimer = Timer(barAutohideDelay, _syncAutohide);
  }

  /// The app menu's title: the name the app gives its menu, else its entry's name.
  String get _appName => _appMenus()?.first.title ?? _appNameFromEntries;

  String get _appNameFromEntries {
    final window = ShellChannel.instance.focusedToplevel;
    // An app that closed its last window stays frontmost (#142, APP-LC): Ossido announces it.
    final windowless =
        ShellChannel.instance.states.value['app.frontmost'] ?? '';
    final appId = window?.appId ?? windowless;
    if (window == null && windowless.isEmpty) return 'Silica';
    final entry = widget.entries?.values
        .where((e) => e.matchesAppId(appId))
        .firstOrNull;
    return entry?.name ??
        (appId.isNotEmpty ? appId : (window?.title ?? 'Silica'));
  }

  @override
  Widget build(BuildContext context) {
    final text = barTextColor(context);
    final states = ShellChannel.instance.states.value;
    // Items with a capsule: the open menu's title, the Control Center and Notification
    // Center while they are up, the status item whose panel is open.
    final statusMenuKey = (states['statusmenu'] ?? '').split(' ').first;
    final controlsOpen = states['controlcenter.open'] == '1';
    final clockOpen = states['notifications.open'] == '1';
    bool on(String name) => _open == name || statusMenuKey == name;
    final bar = SizedBox(
      height: barHeight,
      child: Stack(
        children: [
          Positioned(
            left: barSide,
            top: 0,
            height: barHeight,
            child: Row(
              children: [
                _glyphItem(
                  key: _keyFor('logo'),
                  active: on('logo'),
                  child: SilicaMark(color: text),
                ),
                _titleItem(
                  _appName,
                  barNameStyle,
                  key: _keyFor('app'),
                  active: on('app'),
                ),
                if (_appMenus() case final own?)
                  for (var i = 1; i < own.length; i++)
                    _titleItem(
                      own[i].title,
                      barTitleStyle,
                      key: _keyFor('menu$i'),
                      active: on('menu$i'),
                    )
                else
                  for (final menu in genericMenus)
                    if (_entries(menu).isNotEmpty)
                      _titleItem(
                        menuTitle(l10nOf(context), menu),
                        barTitleStyle,
                        key: _keyFor(menu),
                        active: on(menu),
                      ),
              ],
            ),
          ),
          // The shared Status Bar (status-bar.md): the same component as on the lock screen.
          Positioned(
            right: barSide,
            top: 0,
            height: barHeight,
            child: AmalfiStatusBar(
              key: const ValueKey('menubar status bar'),
              placement: AmalfiStatusBarPlacement.menuBar,
              items: [
                ..._systemIcons(on),
                AmalfiStatusItem(
                  boxKey: _keyFor('search'),
                  label: l10nOf(context).statusSearch,
                  width: AmalfiStatusBarMetrics.glyphItemWidth,
                  onPressed: widget.onSearch,
                  child: const Center(
                    child: StatusIcon(kind: StatusKind.search),
                  ),
                ),
                AmalfiStatusClockItem(
                  boxKey: _keyFor('clock'),
                  text: clockText(
                    widget.now ?? DateTime.now(),
                    l10nOf(context).localeName,
                    _config.value.clock,
                  ),
                  label: l10nOf(context).statusClock,
                  active: clockOpen,
                  onPressed: widget.onClock,
                ),
                AmalfiStatusItem(
                  boxKey: _keyFor('controls'),
                  label: l10nOf(context).statusControlCenter,
                  width: AmalfiStatusBarMetrics.glyphItemWidth,
                  active: controlsOpen,
                  onPressed: widget.onControls,
                  child: const Center(
                    child: StatusIcon(kind: StatusKind.controls),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        _width = constraints.maxWidth.isFinite ? constraints.maxWidth : _width;
        return MouseRegion(
          onExit: (_) => _pointerLeft(),
          child: Listener(
            key: _rootKey,
            behavior: HitTestBehavior.translucent,
            onPointerDown: _pointerDown,
            onPointerMove: (e) => _pointerMoved(e.localPosition),
            onPointerHover: (e) {
              _edgeMoved(e.localPosition);
              _pointerMoved(e.localPosition);
            },
            child: SizedBox.expand(
              child: Stack(
                children: [
                  Positioned(
                    left: 0,
                    top: 0,
                    right: 0,
                    height: barHeight,
                    child: AnimatedBuilder(
                      animation: _shown,
                      builder: (context, child) => Transform.translate(
                        offset: Offset(0, -(1 - _shown.value) * barHeight),
                        child: KeyedSubtree(
                          key: const ValueKey('menubar slide'),
                          child: child!,
                        ),
                      ),
                      child: _config.value.background
                          ? DecoratedBox(
                              key: const ValueKey('menubar background'),
                              decoration: BoxDecoration(
                                color: barBackgroundColor(context),
                              ),
                              child: bar,
                            )
                          : bar,
                    ),
                  ),
                  if (_windowMenuTip case final tip?)
                    Positioned.fill(
                      child: _withBrightness(
                        _windowMenuBrightness,
                        WindowMenuPopover(
                          key: _windowKey,
                          tip: tip,
                          execute: (id) => ShellChannel.instance.execute(id),
                          texts: l10nOf(context),
                          onDismissed: _closeWindowMenu,
                          onRectChanged: (r) => _menuRegion([r]),
                        ),
                      ),
                    ),
                  if (_power case final action?)
                    Positioned.fill(
                      child: LayoutBuilder(
                        builder: (context, box) => Stack(
                          children: [
                            // Clicks outside the card do nothing (it is modal); the
                            // menu bar stays visible above it.
                            const Positioned.fill(
                              child: AbsorbPointer(child: SizedBox.expand()),
                            ),
                            Positioned(
                              left: (box.maxWidth - powerCardSize.width) / 2,
                              // Upper third: the measured card's top at ~20 % of the height.
                              top: box.maxHeight * 0.205,
                              child: PowerConfirmation(
                                action: action,
                                user: _user,
                                onCancel: _closePower,
                                onConfirm: () {
                                  _closePower();
                                  unawaited(_runPower(action));
                                },
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  if (_service case final request?)
                    Positioned.fill(
                      child: _withBrightness(
                        request.brightness,
                        BarMenuHost(
                          key: _serviceKey,
                          entryKey: request.serial,
                          entries: _serviceEntries ?? const [],
                          anchor: popupMenuTopLeft(
                            request,
                            above: desktopMenuAbove,
                          ),
                          control: popupMenuLayout(request),
                          flip: !request.popUp,
                          highlightFirst: false,
                          onDismissed: _closeService,
                          onRectsChanged: _menuRegion,
                        ),
                      ),
                    ),
                  if (_desktopMenuAt case final at?)
                    Positioned.fill(
                      child: BarMenuHost(
                        key: _desktopKey,
                        entries: amalfiEntries(
                          _desktopMenuEntries ?? _desktopEntries(),
                        ),
                        anchor: Offset(at.dx, at.dy - desktopMenuAbove),
                        flip: true,
                        onDismissed: _closeDesktopMenu,
                        onRectsChanged: _menuRegion,
                      ),
                    ),
                  if (_open case final name?)
                    Positioned.fill(
                      child: BarMenuHost(
                        key: _menuKey,
                        entryKey: name,
                        entries: amalfiEntries(_openEntries ?? _entries(name)),
                        anchor: Offset(
                          (_rectOf(name)?.left ?? 0) - menuLeftOfItem,
                          barHeight + menuBelowBar,
                        ),
                        highlightFirst: _highlightFirst,
                        onDismissed: _closeMenu,
                        onNext: () => _neighbour(1),
                        onPrevious: () => _neighbour(-1),
                        // With a submenu open the input region includes it.
                        onRectsChanged: _menuRegion,
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

  /// The status items before Search, from the system state (the shared builders of
  /// `status/status_items.dart` for the network, the battery and the input source); without
  /// the system state, the full Wi-Fi glyph. `on` tells whether the item of a name has its menu
  /// or panel open.
  List<Widget> _systemIcons(bool Function(String name) on) {
    final l = l10nOf(context);
    final s = widget.system?.value;
    final config = _config.value;
    Widget glyph(
      String name,
      StatusKind kind,
      String label, {
      double level = 1,
      bool dim = false,
      VoidCallback? onTap,
    }) => AmalfiStatusItem(
      boxKey: _keyFor(name),
      label: label,
      active: on(name),
      onPressed: onTap,
      width: AmalfiStatusBarMetrics.glyphItemWidth,
      child: Center(
        child: StatusIcon(kind: kind, level: level, dim: dim),
      ),
    );

    if (s == null) {
      return [glyph('wifi', StatusKind.network, l.statusWifi)];
    }
    final playing = widget.media?.now?.playing ?? false;
    return [
      // While media plays (SH-MB-33): the play glyph; the menu is the Now Playing panel.
      if (playing)
        glyph(
          'media',
          StatusKind.nowPlaying,
          l.statusNowPlaying,
          onTap: () => _toggleStatusMenu('media'),
        ),
      // While a Focus is on, its moon (SH-MB-26); the menu is the Focus panel.
      if (s.doNotDisturb || config.focus)
        glyph(
          'dnd',
          StatusKind.focus,
          l.statusFocus,
          onTap: () => _toggleStatusMenu('dnd'),
        ),
      // With two or more input sources: the input menu (status-bar.md › Contexts).
      ?inputSourceStatusItem(
        l,
        _inputSources.value,
        boxKey: _keyFor('input'),
        active: on('input'),
        onPressed: () => _open == 'input' ? _closeMenu() : _openMenu('input'),
      ),
      // Bluetooth and Sound when Settings › Menu Bar asks for them (SH-MB-24, SH-MB-25):
      // their menus are the Control Center's detail panels, like Wi-Fi. Sound "when
      // active" shows while media plays (provisional reading of the setting).
      if (config.bluetooth && s.bluetoothAvailable)
        glyph(
          'bluetooth',
          StatusKind.bluetooth,
          l.statusBluetooth,
          dim: !s.bluetoothPowered,
          onTap: () => _toggleStatusMenu('bluetooth'),
        ),
      ?networkStatusItem(
        l,
        s,
        boxKey: _keyFor(s.wired && s.wifiSsid.isEmpty ? 'wired' : 'wifi'),
        active: on('wifi'),
        onPressed: () => _toggleStatusMenu('wifi'),
      ),
      if (s.audioAvailable &&
          switch (config.sound) {
            SoundItem.always => true,
            SoundItem.active => playing,
            SoundItem.never => false,
          })
        glyph(
          'sound',
          StatusKind.sound,
          l.statusSound,
          level: s.muted ? 0 : s.volume,
          dim: s.muted,
          onTap: () => _toggleStatusMenu('sound'),
        ),
      // The battery menu (SH-MB-27): a standard menu with the charge, the power source and
      // Battery Settings.
      if (config.battery)
        ?batteryStatusItem(
          l,
          s,
          percentage: config.batteryPercent,
          boxKey: _keyFor('battery'),
          active: on('battery'),
          onPressed: () =>
              _open == 'battery' ? _closeMenu() : _openMenu('battery'),
        ),
    ];
  }

  /// A status menu (menubar.md › Status menu): the Control Center panel of `key` (`wifi`,
  /// `dnd`) below the icon, which the Center draws when the bar announces `statusmenu`.
  void _toggleStatusMenu(String key) {
    final shell = ShellChannel.instance;
    final rect = _rectOf(key);
    if (rect == null) return;
    final open =
        (shell.states.value['statusmenu'] ?? '').split(' ').first == key;
    shell.broadcast('statusmenu', open ? '' : '$key ${rect.right}');
  }

  /// An item's box: 28 tall, with the translucent white capsule (radius 14) while its menu
  /// or panel is open (`Menu Bar/Selection`).
  Widget _box({
    Key? key,
    required bool active,
    double? width,
    required Widget child,
  }) {
    final soft = barSelectionColor(context);
    // The capsule is 28 tall, but the item takes the bar's whole height: a click anywhere
    // above or below it counts (silica-desktop#208).
    return SizedBox(
      height: barHeight,
      child: Center(
        widthFactor: 1,
        child: SizedBox(
          key: key,
          width: width,
          height: itemHeight,
          child: DecoratedBox(
            decoration: ShapeDecoration(
              color: active ? soft : null,
              shape: const AmalfiSmoothBorder(radius: itemRadius),
            ),
            child: Center(widthFactor: width == null ? 1 : null, child: child),
          ),
        ),
      ),
    );
  }

  /// The Silica mark's item: 36 wide, the mark at its centre (the status items are the shared
  /// Status Bar's).
  Widget _glyphItem({Key? key, required bool active, required Widget child}) =>
      _box(key: key, active: active, width: statusCell, child: child);

  /// A text item (a menu title, the app name): the text 8 pt from the edges, in
  /// `Text/Primary` also while the item is open.
  Widget _titleItem(
    String text,
    TextStyle Function(Color color) style, {
    Key? key,
    required bool active,
  }) => _box(
    key: key,
    active: active,
    child: Padding(
      padding: const EdgeInsets.symmetric(horizontal: itemPadding),
      child: Text(
        text,
        maxLines: 1,
        softWrap: false,
        style: style(barTextColor(context)),
      ),
    ),
  );
}

/// How tall a menu of `entries` is (24 per item and header, 8 per separator, 4 of padding on
/// each side), to size the surface before the screen's size is known.
double estimatedMenuHeight(List<MenuEntry> entries) {
  var height = 8.0;
  for (final entry in entries) {
    height += entry is MenuDivider ? 8 : 24;
  }
  return height;
}

/// The user's full name for the log out item: the first field of the comment in
/// `/etc/passwd`, else the login name.
String userFullName({Map<String, String>? env, String? passwd}) {
  final e = env ?? Platform.environment;
  final login = e['USER'] ?? e['LOGNAME'] ?? '';
  String text;
  try {
    text = passwd ?? File('/etc/passwd').readAsStringSync();
  } on FileSystemException {
    return login;
  }
  for (final line in text.split('\n')) {
    final fields = line.split(':');
    if (fields.length > 4 && fields[0] == login) {
      final name = fields[4].split(',').first.trim();
      return name.isEmpty ? login : name;
    }
  }
  return login;
}
