// The embedder's silica/shell channel: the open windows (from Ossido) and the activate
// and launch commands. Outside Silica the channel does not answer: no windows.

import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'app_menu_model.dart';
import 'popup_menu_model.dart';

/// An open window.
@immutable
class Toplevel {
  const Toplevel({required this.id, required this.title, required this.appId});

  final String id;
  final String title;
  final String appId;

  static Toplevel fromJson(Object? json) {
    final map = json is Map ? json : const {};
    return Toplevel(
      id: '${map['id'] ?? ''}',
      title: '${map['title'] ?? ''}',
      appId: '${map['app_id'] ?? ''}',
    );
  }
}

class ShellChannel {
  ShellChannel._() {
    channel.setMethodCallHandler((call) async {
      switch (call.method) {
        case 'toplevels':
          _update(call.arguments);
        case 'focused':
          focused.value = '${call.arguments ?? ''}';
        case 'command':
          _commands.add('${call.arguments ?? ''}');
        case 'minimized':
          final list = call.arguments;
          minimized.value = [
            if (list is List)
              for (final id in list) '$id',
          ];
        case 'thumbnail':
          _thumbnailArrived(call.arguments);
        case 'broadcast':
          final args = call.arguments;
          if (args is Map) {
            states.value = {
              ...states.value,
              '${args['topic'] ?? ''}': '${args['value'] ?? ''}',
            };
          }
        case 'transientDismissed':
          // With several layers in one engine (#201) the embedder names the view.
          final args = call.arguments;
          final view = args is Map ? args['viewId'] : null;
          _transientDismissed.add(view is int ? view : null);
        case 'windowMenuAppearance':
          // Right before `windowMenu`, for the same window.
          final args = call.arguments;
          if (args is Map) {
            _menuAppearance = (
              id: '${args['id'] ?? ''}',
              brightness: switch (args['appearance']) {
                'dark' => ui.Brightness.dark,
                'light' => ui.Brightness.light,
                _ => null,
              },
            );
          }
        case 'popupMenu':
          final request = PopupMenuRequest.fromJson(call.arguments);
          if (request != null) _popupMenus.add(request);
        case 'popupMenuClosed':
          final serial = call.arguments;
          if (serial is int) _popupMenusClosed.add(serial);
        case 'appMenu':
          appMenu.value = AppMenuState.fromJson(call.arguments);
        case 'windowMenu':
          final args = call.arguments;
          if (args is Map) {
            final x = args['x'];
            final y = args['y'];
            if (x is num && y is num) {
              final id = '${args['id'] ?? ''}';
              final known = _menuAppearance;
              _menuAppearance = null;
              _windowMenus.add((
                id: id,
                tip: ui.Offset(x.toDouble(), y.toDouble()),
                brightness: known != null && known.id == id
                    ? known.brightness
                    : null,
              ));
            }
          }
      }
      return null;
    });
  }

  static final ShellChannel instance = ShellChannel._();

  static const MethodChannel channel = MethodChannel(
    'silica/shell',
    JSONMethodCodec(),
  );

  /// The open windows, updated on every change.
  final ValueNotifier<List<Toplevel>> toplevels = ValueNotifier(const []);

  final StreamController<String> _commands = StreamController.broadcast();

  /// The commands Ossido forwards to the shell (e.g. `faro.toggle`).
  Stream<String> get commands => _commands.stream;

  /// The states announced by the shell components (`controlcenter.open` = "1"…), by
  /// topic: the last value, including those from before the connection.
  final ValueNotifier<Map<String, String>> states = ValueNotifier(const {});

  /// Announces a state to the other components (and to itself).
  Future<bool> broadcast(String topic, String value) =>
      _call('broadcast', {'topic': topic, 'value': value});

  final StreamController<
    ({String id, ui.Offset tip, ui.Brightness? brightness})
  >
  _windowMenus = StreamController.broadcast();

  final StreamController<PopupMenuRequest> _popupMenus =
      StreamController.broadcast();

  /// The context menus and pop-up buttons the apps ask for (#241).
  Stream<PopupMenuRequest> get popupMenus => _popupMenus.stream;

  final StreamController<int> _popupMenusClosed = StreamController.broadcast();

  /// The serial of a popup menu Ossido ended (a press outside, a change of context).
  Stream<int> get popupMenusClosed => _popupMenusClosed.stream;

  /// The user chose item `id` of the popup menu: the app receives it.
  Future<bool> popupMenuChoose(int serial, int id) =>
      _call('popupMenuChoose', {'serial': serial, 'id': id});

  /// The user closed the popup menu (Esc): the app is told it was dismissed.
  Future<bool> popupMenuClose(int serial) =>
      _call('popupMenuClose', {'serial': serial});

  /// The appearance Ossido reported for the window of the next popover.
  ({String id, ui.Brightness? brightness})? _menuAppearance;

  /// Long press on the green button of a window: the arrangement popover, with the tip at
  /// `tip` (screen coordinates). `brightness` is the appearance the window overrides the
  /// system's with (`SilicaAppearance`), null when it follows the system.
  Stream<({String id, ui.Offset tip, ui.Brightness? brightness})>
  get windowMenus => _windowMenus.stream;

  /// The view whose transient ended, null when the embedder does not say.
  final StreamController<int?> _transientDismissed =
      StreamController.broadcast();

  /// A press outside this component's open menu or panel ended it (#149).
  Stream<void> get transientDismissed => transientDismissedFor(null);

  /// The same for the layer of view `viewId` when the shell's layers share one engine
  /// (#201): the others' dismissals are not its. Null: every dismissal (one layer per
  /// process).
  Stream<void> transientDismissedFor(int? viewId) => _transientDismissed.stream
      .where((view) => viewId == null || view == null || view == viewId);

  /// The frontmost app's menus (silica-menu-v1, #25), on every change.
  final ValueNotifier<AppMenuState> appMenu = ValueNotifier(
    const AppMenuState(),
  );

  /// An item of the frontmost app's menus was chosen: the app receives it.
  Future<bool> activateMenuItem(int serial, int id) =>
      _call('activateMenuItem', {'serial': serial, 'id': id});

  /// The identifier of the focused window (empty: none).
  final ValueNotifier<String> focused = ValueNotifier('');

  /// The minimized windows, from the first minimized (Ossido, silica-shell-v1 version 2).
  final ValueNotifier<List<String>> minimized = ValueNotifier(const []);

  final Map<String, Completer<ui.Image?>> _thumbnails = {};

  /// The thumbnail of a minimized window (null if there is none).
  Future<ui.Image?> thumbnail(String id) {
    final pending = _thumbnails[id];
    if (pending != null) return pending.future;
    final completer = Completer<ui.Image?>();
    _thumbnails[id] = completer;
    _call('thumbnail', {'id': id}).then((sent) {
      if (!sent && !completer.isCompleted) {
        _thumbnails.remove(id);
        completer.complete(null);
      }
    });
    return completer.future;
  }

  Future<void> _thumbnailArrived(Object? args) async {
    final map = args is Map ? args : const {};
    final id = '${map['id'] ?? ''}';
    final completer = _thumbnails.remove(id);
    if (completer == null) return;
    final width = map['width'] is int ? map['width'] as int : 0;
    final height = map['height'] is int ? map['height'] as int : 0;
    final path = '${map['path'] ?? ''}';
    if (width <= 0 || height <= 0 || path.isEmpty) {
      completer.complete(null);
      return;
    }
    try {
      final file = File(path);
      final bytes = await file.readAsBytes();
      await file.delete();
      if (bytes.length < width * height * 4) {
        completer.complete(null);
        return;
      }
      ui.decodeImageFromPixels(
        bytes,
        width,
        height,
        ui.PixelFormat.rgba8888,
        completer.complete,
      );
    } on FileSystemException {
      completer.complete(null);
    }
  }

  /// The focused window, if it is in the list.
  Toplevel? get focusedToplevel =>
      toplevels.value.where((t) => t.id == focused.value).firstOrNull;

  /// The frontmost app: the focused window's, or the windowless one Ossido announces
  /// (`app.frontmost`, #142); empty if none.
  String get frontmostAppId =>
      focusedToplevel?.appId ?? states.value['app.frontmost'] ?? '';

  void _update(Object? list) {
    toplevels.value = [
      if (list is List)
        for (final item in list) Toplevel.fromJson(item),
    ];
  }

  /// Reads the current list (at startup, for the windows announced before Dart).
  Future<void> refresh() async {
    try {
      _update(await channel.invokeMethod<Object?>('list'));
      focused.value = await channel.invokeMethod<String>('focused') ?? '';
      final list = await channel.invokeMethod<Object?>('minimized');
      minimized.value = [
        if (list is List)
          for (final id in list) '$id',
      ];
      appMenu.value = AppMenuState.fromJson(
        await channel.invokeMethod<Object?>('appMenu'),
      );
    } on MissingPluginException {
      // Outside Silica.
    }
  }

  Future<bool> activate(String id) => _call('activate', {'id': id});

  Future<bool> launch(String command) => _call('launch', {'command': command});

  /// Runs a command of Ossido's catalog (e.g. `faro.toggle` from the menu bar).
  /// Runs a command of Ossido's catalog, with the parameters if there are any.
  Future<bool> execute(String id, [Map<String, Object?>? params]) =>
      _call('execute', {'id': id, 'params': ?params});

  Future<bool> _call(String method, Map<String, Object?> args) async {
    try {
      return await channel.invokeMethod<bool>(method, args) ?? false;
    } on MissingPluginException {
      return false;
    }
  }
}
