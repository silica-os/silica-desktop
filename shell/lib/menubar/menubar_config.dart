// Which optional status items the bar shows (Settings › Menu Bar), the `menubar.*` keys of the
// Settings service: `sound` (always | active | never), `bluetooth`, `battery`,
// `batteryPercent`, `focus`, `clock.*`, `recentItems`, `autohide`, `background`.
//
// Defaults as on the reference (menubar.md › Status items, INV-A1): Bluetooth not in the bar,
// Sound "when active", battery shown when there is one. Settings changes apply at once.

import 'package:flutter/foundation.dart';
import 'package:silica_ui/silica_ui.dart';

enum SoundItem { always, active, never }

/// When the bar's clock shows the date (menubar.json `clock.date`).
enum ClockDate { auto, always, never }

/// The clock's format (menubar.json `clock`, Settings › Date & Time). `hour24` absent is 24
/// hours: the 12/24 preference of the system language is not read yet.
@immutable
class ClockStyle {
  const ClockStyle({
    this.hour24 = true,
    this.seconds = false,
    this.weekday = true,
    this.date = ClockDate.auto,
  });

  final bool hour24;
  final bool seconds;
  final bool weekday;
  final ClockDate date;

  factory ClockStyle.of(SilicaSettings s) => ClockStyle(
    hour24: s.getBool('menubar.clock.hour24', fallback: true),
    seconds: s.getBool('menubar.clock.seconds', fallback: false),
    weekday: s.getBool('menubar.clock.weekday', fallback: true),
    date: s.getEnum(
      'menubar.clock.date',
      ClockDate.values,
      fallback: ClockDate.auto,
    ),
  );

  @override
  bool operator ==(Object other) =>
      other is ClockStyle &&
      other.hour24 == hour24 &&
      other.seconds == seconds &&
      other.weekday == weekday &&
      other.date == date;

  @override
  int get hashCode => Object.hash(hour24, seconds, weekday, date);
}

/// The counts Settings offers for the recent items (menubar.json `recentItems`).
const List<int> recentItemChoices = [0, 5, 10, 15, 20, 30, 50];

/// When the bar hides itself (menubar.json `autohide`, Settings › Desktop & Dock).
enum MenubarAutohide { never, fullscreen, always }

@immutable
class MenubarConfig {
  const MenubarConfig({
    this.sound = SoundItem.active,
    this.bluetooth = false,
    this.battery = true,
    this.batteryPercent = false,
    this.focus = false,
    this.clock = const ClockStyle(),
    this.recentItems = 10,
    this.autohide = MenubarAutohide.fullscreen,
    this.background = false,
  });

  final SoundItem sound;
  final bool bluetooth;
  final bool battery;
  final bool batteryPercent;

  /// The Focus moon always shown (Control Center › Copy to Menu Bar); otherwise only while a
  /// Focus is on.
  final bool focus;

  /// The clock's format.
  final ClockStyle clock;

  /// How many recent applications and documents the Apple-menu equivalent lists (0: none).
  final int recentItems;

  /// When the bar slides away; it comes back when the pointer reaches the top edge.
  final MenubarAutohide autohide;

  /// A translucent background under the bar (off: the bar is bare on the wallpaper).
  final bool background;

  factory MenubarConfig.of(SilicaSettings s) => MenubarConfig(
    sound: s.getEnum(
      'menubar.sound',
      SoundItem.values,
      fallback: SoundItem.active,
    ),
    bluetooth: s.getBool('menubar.bluetooth', fallback: false),
    battery: s.getBool('menubar.battery', fallback: true),
    batteryPercent: s.getBool('menubar.batteryPercent', fallback: false),
    focus: s.getBool('menubar.focus', fallback: false),
    clock: ClockStyle.of(s),
    recentItems: switch (s.getInt('menubar.recentItems', fallback: 10)) {
      final int n when recentItemChoices.contains(n) => n,
      _ => 10,
    },
    autohide: s.getEnum(
      'menubar.autohide',
      MenubarAutohide.values,
      fallback: MenubarAutohide.fullscreen,
    ),
    background: s.getBool('menubar.background', fallback: false),
  );

  @override
  bool operator ==(Object other) =>
      other is MenubarConfig &&
      other.sound == sound &&
      other.bluetooth == bluetooth &&
      other.battery == battery &&
      other.batteryPercent == batteryPercent &&
      other.focus == focus &&
      other.clock == clock &&
      other.recentItems == recentItems &&
      other.autohide == autohide &&
      other.background == background;

  @override
  int get hashCode => Object.hash(
    sound,
    bluetooth,
    battery,
    batteryPercent,
    focus,
    clock,
    recentItems,
    autohide,
    background,
  );
}

/// The Control Center modules that have a status item in the bar, and which ones the bar shows
/// now (the module menu's Copy to / Remove from Menu Bar, silica-desktop#145). Wi-Fi is always
/// in the bar and cannot be removed.
const menubarModules = {'bluetooth', 'sound', 'dnd', 'battery'};

/// Whether the bar shows the status item of Control Center module [id] under [config].
bool menubarShows(MenubarConfig config, String id) => switch (id) {
  'bluetooth' => config.bluetooth,
  'sound' => config.sound == SoundItem.always,
  'dnd' => config.focus,
  'battery' => config.battery,
  _ => false,
};

/// Shows or hides the status item of module [id] (`menubar.*`; the watcher applies the change at
/// once). Returns false for a module the bar has no item for, or when the service refused it.
Future<bool> setMenubarModule(
  String id,
  bool shown, [
  SilicaSettings? settings,
]) async {
  final key = switch (id) {
    'bluetooth' => 'bluetooth',
    'sound' => 'sound',
    'dnd' => 'focus',
    'battery' => 'battery',
    _ => null,
  };
  if (key == null) return false;
  return (settings ?? SilicaSettings.instance).set(
    'menubar.$key',
    key == 'sound' ? (shown ? 'always' : 'never') : shown,
  );
}

/// Every `menubar.*` key of [MenubarConfig].
const List<String> menubarKeys = [
  'menubar.sound',
  'menubar.bluetooth',
  'menubar.battery',
  'menubar.batteryPercent',
  'menubar.focus',
  'menubar.clock.hour24',
  'menubar.clock.seconds',
  'menubar.clock.weekday',
  'menubar.clock.date',
  'menubar.recentItems',
  'menubar.autohide',
  'menubar.background',
];

/// The configuration, following the Settings service.
class MenubarConfigWatcher extends ValueNotifier<MenubarConfig> {
  MenubarConfigWatcher([SilicaSettings? settings])
    : _settings = settings ?? SilicaSettings.instance,
      super(MenubarConfig.of(settings ?? SilicaSettings.instance)) {
    _keys = _settings.watch(menubarKeys)..addListener(_changed);
  }

  final SilicaSettings _settings;
  late final Listenable _keys;

  void _changed() => value = MenubarConfig.of(_settings);

  @override
  void dispose() {
    _keys.removeListener(_changed);
    super.dispose();
  }
}
