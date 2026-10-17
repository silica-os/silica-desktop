// Which optional status items the bar shows (Settings › Menu Bar), in
// `$XDG_CONFIG_HOME/silica/menubar.json`:
//
//   {"sound": "always" | "active" | "never", "bluetooth": true, "battery": true,
//    "batteryPercent": false, "focus": false}
//
// Defaults as on the reference (menubar.md › Status items, INV-A1): Bluetooth not in the bar,
// Sound "when active", battery shown when there is one. The file is watched: Settings changes
// apply at once.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

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

  factory ClockStyle.fromJson(Object? json) {
    if (json is! Map) return const ClockStyle();
    bool flag(String key, bool fallback) =>
        json[key] is bool ? json[key] as bool : fallback;
    return ClockStyle(
      hour24: flag('hour24', true),
      seconds: flag('seconds', false),
      weekday: flag('weekday', true),
      date: switch (json['date']) {
        'always' => ClockDate.always,
        'never' => ClockDate.never,
        _ => ClockDate.auto,
      },
    );
  }

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

  factory MenubarConfig.fromJson(Object? json) {
    if (json is! Map) return const MenubarConfig();
    final sound = switch (json['sound']) {
      'always' => SoundItem.always,
      'never' => SoundItem.never,
      _ => SoundItem.active,
    };
    bool flag(String key, bool fallback) =>
        json[key] is bool ? json[key] as bool : fallback;
    return MenubarConfig(
      sound: sound,
      bluetooth: flag('bluetooth', false),
      battery: flag('battery', true),
      batteryPercent: flag('batteryPercent', false),
      focus: flag('focus', false),
      clock: ClockStyle.fromJson(json['clock']),
      recentItems: switch (json['recentItems']) {
        final int n when recentItemChoices.contains(n) => n,
        _ => 10,
      },
    );
  }

  @override
  bool operator ==(Object other) =>
      other is MenubarConfig &&
      other.sound == sound &&
      other.bluetooth == bluetooth &&
      other.battery == battery &&
      other.batteryPercent == batteryPercent &&
      other.focus == focus &&
      other.clock == clock &&
      other.recentItems == recentItems;

  @override
  int get hashCode => Object.hash(
    sound,
    bluetooth,
    battery,
    batteryPercent,
    focus,
    clock,
    recentItems,
  );
}

/// The configuration file's path.
String menubarConfigPath([Map<String, String>? env]) {
  final e = env ?? Platform.environment;
  final config = e['XDG_CONFIG_HOME'] ?? '${e['HOME'] ?? ''}/.config';
  return '$config/silica/menubar.json';
}

MenubarConfig readMenubarConfig([Map<String, String>? env]) {
  try {
    return MenubarConfig.fromJson(
      jsonDecode(File(menubarConfigPath(env)).readAsStringSync()),
    );
  } on FileSystemException {
    return const MenubarConfig();
  } on FormatException {
    return const MenubarConfig();
  }
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

/// Shows or hides the status item of module [id] by rewriting `menubar.json` (the other keys
/// stay as they are, the watcher applies the change). Returns false when it could not write.
bool setMenubarModule(String id, bool shown, [Map<String, String>? env]) {
  final key = switch (id) {
    'bluetooth' => 'bluetooth',
    'sound' => 'sound',
    'dnd' => 'focus',
    'battery' => 'battery',
    _ => null,
  };
  if (key == null) return false;
  final file = File(menubarConfigPath(env));
  try {
    var json = <String, Object?>{};
    try {
      final read = jsonDecode(file.readAsStringSync());
      if (read is Map) json = Map<String, Object?>.from(read);
    } on FileSystemException {
      // No file yet: the defaults plus this key.
    } on FormatException {
      // Unreadable: start again from the defaults.
    }
    json[key] = key == 'sound' ? (shown ? 'always' : 'never') : shown;
    file.parent.createSync(recursive: true);
    file.writeAsStringSync(
      '${const JsonEncoder.withIndent('  ').convert(json)}\n',
    );
    return true;
  } on FileSystemException {
    return false;
  }
}

/// The configuration, re-read when its folder changes.
class MenubarConfigWatcher extends ValueNotifier<MenubarConfig> {
  MenubarConfigWatcher([Map<String, String>? env])
    : _env = env,
      super(readMenubarConfig(env)) {
    final dir = File(menubarConfigPath(env)).parent;
    try {
      _sub = dir.watch().listen(
        (_) => value = readMenubarConfig(_env),
        onError: (Object _) {},
      );
    } on FileSystemException {
      // No folder yet: the defaults stay until the next session.
    }
  }

  final Map<String, String>? _env;
  StreamSubscription<FileSystemEvent>? _sub;

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }
}
