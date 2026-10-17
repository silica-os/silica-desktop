// The Dock options in dock.json (Settings › Desktop & Dock and the separator menu write them):
// one place that reads them with their defaults, and a watcher so a change made by Settings
// applies at once.

import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';

import 'apps.dart';
import 'model.dart';

/// Where the Dock sits (dock.md › SH-DK-05/06). Only the bottom is implemented: the side
/// Docks are a vertical layout still to do (silica-desktop#72, #73).
enum DockPosition { left, bottom, right }

/// The effect that takes a window into the Dock (SH-DK-09).
enum MinimizeEffect { genie, scale }

@immutable
class DockOptions {
  const DockOptions({
    this.magnify = false,
    this.recents = false,
    this.autohide = false,
    this.position = DockPosition.bottom,
    this.minimizeEffect = MinimizeEffect.genie,
    this.indicators = true,
    this.tileSize = defaultCanvas,
    this.magnifiedSize = defaultMagnifiedCanvas,
    this.launchAnimation = true,
  });

  /// Magnification (separator menu, Settings): off by default.
  final bool magnify;

  /// The canvas of an app icon at rest, in whole points, 16-128 (`tileSize`: Settings and the
  /// drag on the separator write it).
  final double tileSize;

  /// The canvas of the icon under the pointer with magnification on, from [tileSize] up to
  /// 128 (`magnifiedSize`).
  final double magnifiedSize;

  /// The icon bounces while an app opens (`launchAnimation`: `bounce`, the default, or `none`).
  final bool launchAnimation;

  /// The recent and suggested apps section (D-08).
  final bool recents;

  /// Automatically hide and show the Dock (SH-DK-07, ⌥⌘D).
  final bool autohide;
  final DockPosition position;
  final MinimizeEffect minimizeEffect;

  /// The dots under open apps (SH-DK-12).
  final bool indicators;

  static DockOptions read({Map<String, String>? env}) {
    final c = readDockConfig(env: env);
    bool flag(String key, bool fallback) =>
        c[key] is bool ? c[key] as bool : fallback;
    double number(Object? v, double fallback) =>
        v is num ? v.toDouble() : fallback;
    T named<T extends Enum>(String key, List<T> values, T fallback) =>
        values.where((v) => v.name == c[key]).firstOrNull ?? fallback;
    final tile = clampCanvas(number(c['tileSize'], defaultCanvas));
    final magnified = number(c['magnifiedSize'], defaultMagnifiedCanvas);
    return DockOptions(
      magnify: flag('magnify', false),
      tileSize: tile,
      magnifiedSize: magnified.clamp(tile, maxCanvas).roundToDouble(),
      recents: flag('recents', false),
      autohide: flag('autohide', false),
      position: named('position', DockPosition.values, DockPosition.bottom),
      minimizeEffect: named(
        'minimizeEffect',
        MinimizeEffect.values,
        MinimizeEffect.genie,
      ),
      indicators: flag('indicators', true),
      launchAnimation: c['launchAnimation'] != 'none',
    );
  }

  @override
  bool operator ==(Object other) =>
      other is DockOptions &&
      other.magnify == magnify &&
      other.recents == recents &&
      other.autohide == autohide &&
      other.position == position &&
      other.minimizeEffect == minimizeEffect &&
      other.indicators == indicators &&
      other.tileSize == tileSize &&
      other.magnifiedSize == magnifiedSize &&
      other.launchAnimation == launchAnimation;

  @override
  int get hashCode => Object.hash(
    magnify,
    recents,
    autohide,
    position,
    minimizeEffect,
    indicators,
    tileSize,
    magnifiedSize,
    launchAnimation,
  );
}

/// The options, reloaded when dock.json changes (Settings writes it).
class DockOptionsWatcher {
  DockOptionsWatcher({Map<String, String>? env}) : _env = env {
    value.value = DockOptions.read(env: env);
    _watch();
  }

  final Map<String, String>? _env;
  final ValueNotifier<DockOptions> value = ValueNotifier(const DockOptions());
  StreamSubscription<FileSystemEvent>? _events;
  Timer? _debounce;

  void _watch() {
    final dir = dockConfigFile(env: _env).parent;
    try {
      if (!dir.existsSync()) dir.createSync(recursive: true);
      _events = dir.watch().listen((event) {
        if (!event.path.endsWith('dock.json')) return;
        // Writers replace the file in a few steps: read once they are done.
        _debounce?.cancel();
        _debounce = Timer(const Duration(milliseconds: 50), reload);
      });
    } on FileSystemException {
      // No configuration folder: the options change only from the Dock itself.
    }
  }

  void reload() => value.value = DockOptions.read(env: _env);

  void dispose() {
    _debounce?.cancel();
    _events?.cancel();
    value.dispose();
  }
}
