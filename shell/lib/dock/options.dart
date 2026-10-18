// The Dock options, the `dock.*` keys of the Settings service (Settings › Desktop & Dock and
// the separator menu write them): one place that reads them, and a watcher so a change made
// by Settings applies at once.

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:silica_ui/silica_ui.dart';

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
    this.minimizeToApp = false,
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

  /// Minimized windows go into their app's icon, not the section right of the separator
  /// (`minimizeToApp`).
  final bool minimizeToApp;

  /// The recent and suggested apps section (D-08).
  final bool recents;

  /// Automatically hide and show the Dock (SH-DK-07, ⌥⌘D).
  final bool autohide;
  final DockPosition position;
  final MinimizeEffect minimizeEffect;

  /// The dots under open apps (SH-DK-12).
  final bool indicators;

  /// The options as the settings hold them now.
  static DockOptions of(SilicaSettings s) {
    final tile = clampCanvas(
      s.getInt('dock.tileSize', fallback: defaultCanvas.round()).toDouble(),
    );
    final magnified = s
        .getInt('dock.magnifiedSize', fallback: defaultMagnifiedCanvas.round())
        .toDouble();
    return DockOptions(
      magnify: s.getBool('dock.magnify', fallback: false),
      tileSize: tile,
      magnifiedSize: magnified.clamp(tile, maxCanvas).roundToDouble(),
      recents: s.getBool('dock.recents', fallback: false),
      autohide: s.getBool('dock.autohide', fallback: false),
      position: s.getEnum(
        'dock.position',
        DockPosition.values,
        fallback: DockPosition.bottom,
      ),
      minimizeEffect: s.getEnum(
        'dock.minimizeEffect',
        MinimizeEffect.values,
        fallback: MinimizeEffect.genie,
      ),
      indicators: s.getBool('dock.indicators', fallback: true),
      launchAnimation:
          s.getText('dock.launchAnimation', fallback: 'bounce') != 'none',
      minimizeToApp: s.getBool('dock.minimizeToApp', fallback: false),
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
      other.launchAnimation == launchAnimation &&
      other.minimizeToApp == minimizeToApp;

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
    minimizeToApp,
  );
}

/// The `dock.*` keys of [DockOptions].
const List<String> dockOptionKeys = [
  'magnify',
  'recents',
  'autohide',
  'position',
  'minimizeEffect',
  'indicators',
  'tileSize',
  'magnifiedSize',
  'launchAnimation',
  'minimizeToApp',
];

/// Changes one option (a key of [dockOptionKeys]); it applies at once and is sent to the
/// Settings service.
void setDockOption(String key, Object value, {SilicaSettings? settings}) {
  unawaited((settings ?? SilicaSettings.instance).set('dock.$key', value));
}

/// The options, following the Settings service.
class DockOptionsWatcher {
  DockOptionsWatcher({SilicaSettings? settings})
    : _settings = settings ?? SilicaSettings.instance {
    _sources = _settings.watch([for (final k in dockOptionKeys) 'dock.$k']);
    value = ValueNotifier(DockOptions.of(_settings));
    _sources.addListener(_changed);
  }

  final SilicaSettings _settings;
  late final Listenable _sources;
  late final ValueNotifier<DockOptions> value;

  void _changed() {
    final next = DockOptions.of(_settings);
    if (next != value.value) value.value = next;
  }

  void dispose() {
    _sources.removeListener(_changed);
    value.dispose();
  }
}
