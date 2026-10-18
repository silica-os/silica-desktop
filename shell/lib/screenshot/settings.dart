// The screenshot settings: where captures go, the timer, the thumbnail, the pointer, the
// mode and selection the ⇧⌘5 bar remembers and where the bar was left. Kept in the
// `screenshot.*` keys of the Settings service; ⇧⌘3 and ⇧⌘4 follow the same place and thumbnail
// choices as the bar (as on the reference system).

import 'dart:async';
import 'dart:ui' show Offset, Rect;

import 'package:silica_ui/silica_ui.dart';

/// Where a capture goes.
sealed class ShotDestination {
  const ShotDestination();

  /// From its saved form: `desktop`, `documents`, `clipboard`, `folder:<path>`, `app:<id>`.
  static ShotDestination parse(Object? text) {
    if (text is! String) return const ToDesktop();
    if (text == 'documents') return const ToDocuments();
    if (text == 'clipboard') return const ToClipboard();
    if (text.startsWith('folder:') && text.length > 'folder:'.length) {
      return ToFolder(text.substring('folder:'.length));
    }
    if (text.startsWith('app:') && text.length > 'app:'.length) {
      return ToApp(text.substring('app:'.length));
    }
    return const ToDesktop();
  }

  String get saved;

  @override
  bool operator ==(Object other) =>
      other is ShotDestination && other.saved == saved;

  @override
  int get hashCode => saved.hashCode;
}

class ToDesktop extends ShotDestination {
  const ToDesktop();
  @override
  String get saved => 'desktop';
}

class ToDocuments extends ShotDestination {
  const ToDocuments();
  @override
  String get saved => 'documents';
}

class ToClipboard extends ShotDestination {
  const ToClipboard();
  @override
  String get saved => 'clipboard';
}

/// A folder chosen with "Other Folder…".
class ToFolder extends ShotDestination {
  const ToFolder(this.path);
  final String path;
  @override
  String get saved => 'folder:$path';
}

/// An app that opens the image (the default image viewer), by its desktop entry id.
class ToApp extends ShotDestination {
  const ToApp(this.id);
  final String id;
  @override
  String get saved => 'app:$id';
}

/// The bar's capture modes, left to right.
enum ToolbarChoice { screen, window, selection }

/// The timer before a capture from the bar.
const List<int> timerChoices = [0, 5, 10];

class ScreenshotSettings {
  ScreenshotSettings({
    this.destination = const ToDesktop(),
    this.timer = 0,
    this.showThumbnail = true,
    this.rememberSelection = true,
    this.showPointer = false,
    this.mode = ToolbarChoice.selection,
    this.selection,
    this.barCenter,
    List<String>? recentFolders,
  }) : recentFolders = recentFolders ?? [];

  ShotDestination destination;

  /// Seconds before the capture (0, 5 or 10).
  int timer;
  bool showThumbnail;

  /// The bar opens with the last selection (otherwise a default one).
  bool rememberSelection;

  /// The pointer drawn into the captures.
  bool showPointer;

  /// The bar's mode, kept from one opening to the next.
  ToolbarChoice mode;

  /// The last selection of the bar, in points.
  Rect? selection;

  /// Where the bar was left (its center), in points; null: centered above the Dock.
  Offset? barCenter;

  /// Folders chosen with "Other Folder…", newest first (at most 3, provisional).
  final List<String> recentFolders;

  static const int maxRecentFolders = 3;

  /// The `screenshot.*` keys, with the values of these settings.
  Map<String, Object> toValues() => {
    'screenshot.destination': destination.saved,
    'screenshot.timer': timer,
    'screenshot.thumbnail': showThumbnail,
    'screenshot.rememberSelection': rememberSelection,
    'screenshot.pointer': showPointer,
    'screenshot.mode': mode.name,
    'screenshot.selection': [
      if (selection case final s?) ...[s.left, s.top, s.width, s.height],
    ],
    'screenshot.bar': [
      if (barCenter case final c?) ...[c.dx, c.dy],
    ],
    'screenshot.recentFolders': recentFolders,
  };

  /// The settings now in the service.
  static ScreenshotSettings of(SilicaSettings s) {
    final rect = s.getNumbers('screenshot.selection');
    final bar = s.getNumbers('screenshot.bar');
    final timer = s.getInt('screenshot.timer', fallback: 0);
    return ScreenshotSettings(
      destination: ShotDestination.parse(
        s.getText('screenshot.destination', fallback: 'desktop'),
      ),
      timer: timerChoices.contains(timer) ? timer : 0,
      showThumbnail: s.getBool('screenshot.thumbnail', fallback: true),
      rememberSelection: s.getBool(
        'screenshot.rememberSelection',
        fallback: true,
      ),
      showPointer: s.getBool('screenshot.pointer', fallback: false),
      mode: s.getEnum(
        'screenshot.mode',
        ToolbarChoice.values,
        fallback: ToolbarChoice.selection,
      ),
      selection: rect.length == 4 && rect[2] > 0 && rect[3] > 0
          ? Rect.fromLTWH(rect[0], rect[1], rect[2], rect[3])
          : null,
      barCenter: bar.length == 2 ? Offset(bar[0], bar[1]) : null,
      recentFolders: [
        for (final f
            in s.getTexts('screenshot.recentFolders').take(maxRecentFolders))
          if (f.startsWith('/')) f,
      ],
    );
  }

  /// Sends the settings to the service (all keys at once).
  void save(SilicaSettings s) => unawaited(s.setMany(toValues()));

  /// A folder chosen with "Other Folder…": it becomes the destination and the first of the
  /// recent ones.
  void chooseFolder(String path) {
    destination = ToFolder(path);
    recentFolders
      ..remove(path)
      ..insert(0, path);
    if (recentFolders.length > maxRecentFolders) {
      recentFolders.removeRange(maxRecentFolders, recentFolders.length);
    }
  }
}
