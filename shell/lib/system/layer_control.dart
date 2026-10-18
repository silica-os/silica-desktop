// The embedder's silica/layer channel: keyboard and input region of the layer, for the
// components that keep running while hidden (a closed Faro must take neither pointer
// nor keyboard).

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';

import 'shell_channel.dart';

/// What the surfaces outside an open menu or panel receive (silica-shell-v1 `rule`,
/// #149). Whatever the rule, the wheel never closes anything, and Ossido shows the arrow
/// outside the menu or panel (no text cursor).
enum TransientRule {
  /// No hover, no wheel; a click only closes the menu (a menu's own app).
  modal,

  /// Hover and wheel work; a click, right click or drag closes the menu or panel and also
  /// reaches what is below.
  pass,

  /// Hover and wheel work; a left click closes the menu and only activates the window
  /// below (it does not reach its content), other buttons pass.
  activate,
}

/// The measured input model while a menu or panel is open (the reference's INPUT-01): a
/// menu is modal only for the app that owns it, system panels never block other windows,
/// the wheel never closes anything. Keys always go to the open surface (the caller asks for
/// the keyboard with [LayerControl.keyboard]).
@immutable
class TransientPolicy {
  const TransientPolicy({required this.owner, required this.other});

  /// For the surfaces of the owner app.
  final TransientRule owner;

  /// For every other surface.
  final TransientRule other;

  /// Menu-bar menus (owner: the frontmost app), the logo menu (the system), the desktop
  /// menu (the desktop's owner): a click in the owner's windows only closes the menu, in
  /// another app it passes.
  static const menu = TransientPolicy(
    owner: TransientRule.modal,
    other: TransientRule.pass,
  );

  /// The Dock's icon menu and stacks, with the app that was active as owner: a left click
  /// on its windows only makes it active again (the caret does not move), elsewhere it
  /// passes as on an inactive window; right click and wheel pass.
  static const dockMenu = TransientPolicy(
    owner: TransientRule.activate,
    other: TransientRule.pass,
  );

  /// Faro, the launcher, Control Center, status menus, Notification Center: everything
  /// passes.
  static const panel = TransientPolicy(
    owner: TransientRule.pass,
    other: TransientRule.pass,
  );

  /// A popover (the window's arrangement popover): any click outside closes it and passes,
  /// in its own window too.
  static const popover = TransientPolicy(
    owner: TransientRule.pass,
    other: TransientRule.pass,
  );

  /// The version 6 fallback (an older Ossido without owners): `consume` or `pass_through`
  /// for every surface.
  String get _press =>
      other == TransientRule.pass && owner == TransientRule.pass
      ? 'pass_through'
      : 'consume';
  String get _scroll =>
      owner == TransientRule.modal ? 'consume' : 'pass_through';

  @override
  bool operator ==(Object other) =>
      other is TransientPolicy &&
      other.owner == owner &&
      other.other == this.other;

  @override
  int get hashCode => Object.hash(owner, other);

  @override
  String toString() => 'TransientPolicy(owner: $owner, other: $other)';
}

/// The owner of the system's own menus (the logo menu, Control Center…): no app.
const String systemOwner = '';

/// The owner of the desktop menu: whichever app owns the desktop surface
/// (`desktop-icons:<app id>`), resolved by Ossido.
const String desktopOwner = 'desktop-icons';

/// The layer surface of a shell component. With one component per process there is one
/// ([LayerControl.main]); when the shell draws its layers as views of one engine (#201) each
/// component reaches its own through [LayerScope] ([LayerControl.of]), and the calls name
/// its view.
@immutable
class LayerControl {
  const LayerControl._(this.viewId);

  /// The layer of view `viewId` of this engine.
  const LayerControl.forView(int this.viewId);

  /// The process's only layer: calls without a view.
  static const LayerControl main = LayerControl._(null);

  /// The layer of the component `context` belongs to: [main] outside a [LayerScope]. It does
  /// not depend on the scope, so it may be read in `initState`.
  static LayerControl of(BuildContext context) =>
      context.getInheritedWidgetOfExactType<LayerScope>()?.control ?? main;

  /// The view of this layer, null for [main].
  final int? viewId;

  static const MethodChannel channel = MethodChannel(
    'silica/layer',
    JSONMethodCodec(),
  );

  /// The shell layers of this engine: view id → namespace (`menubar`, `dock`…), the ones the
  /// engine has added so far. Empty outside Silica or with one layer per process.
  static Future<Map<int, String>> views() async {
    try {
      final list = await channel.invokeMethod<Object?>('views');
      return {
        if (list is List)
          for (final item in list)
            if (item is Map &&
                item['viewId'] is int &&
                item['namespace'] is String)
              item['viewId'] as int: item['namespace'] as String,
      };
    } on MissingPluginException {
      return const {};
    }
  }

  /// A press outside this layer's open menu or panel ended it (#149).
  Stream<void> get transientDismissed =>
      ShellChannel.instance.transientDismissedFor(viewId);

  /// `none`, `exclusive` or `on-demand`.
  Future<void> keyboard(String mode) =>
      _call('setKeyboardInteractivity', {'mode': mode});

  /// The surface receives the pointer only in these rectangles; null: everywhere.
  Future<void> inputRegion(List<Rect>? rects) => _call('setInputRegion', {
    'rects': rects == null
        ? null
        : [
            for (final r in rects) [r.left, r.top, r.width, r.height],
          ],
  });

  /// A menu or panel of this surface is open (silica-shell-v1 version 6, #149): with the
  /// input region as small as the menu, the windows below keep the pointer; Ossido sends
  /// `transientDismissed` (ShellChannel) when a press outside ends it. `rects` count as
  /// inside even under another surface. `owner` is the app id whose surfaces follow
  /// `policy.owner` ([systemOwner]: none, [desktopOwner]: the desktop's app). False when
  /// Ossido does not support it: then the caller covers the screen to see the clicks
  /// outside.
  Future<bool> transient({
    List<Rect>? rects,
    TransientPolicy policy = TransientPolicy.menu,
    String owner = systemOwner,
  }) async {
    try {
      return await channel.invokeMethod<bool>(
            'setTransient',
            _args({
              'rects': rects == null
                  ? null
                  : [
                      for (final r in rects) [r.left, r.top, r.width, r.height],
                    ],
              'owner': owner,
              'ownerRule': policy.owner.name,
              'otherRule': policy.other.name,
              'press': policy._press,
              'scroll': policy._scroll,
            }),
          ) ??
          false;
    } on MissingPluginException {
      return false;
    }
  }

  /// Where the shell drew popup menu `serial` of an app (#241): Ossido already holds the
  /// menu as a transient owned by the app and only needs the rectangles.
  Future<bool> popupMenuRegion(int serial, List<Rect> rects) async {
    try {
      return await channel.invokeMethod<bool>(
            'setTransient',
            _args({
              'popupMenu': serial,
              'rects': [
                for (final r in rects) [r.left, r.top, r.width, r.height],
              ],
            }),
          ) ??
          false;
    } on MissingPluginException {
      return false;
    }
  }

  /// The menu or panel was closed by the shell itself (no `transientDismissed` follows).
  Future<void> clearTransient() => _call('clearTransient', {});

  /// Surface size in points; 0 follows the anchors (the width of the bar).
  Future<void> size(double width, double height) =>
      _call('setSize', {'width': width, 'height': height});

  /// The exclusive zone in points (the Dock gives its room back while it hides itself).
  Future<void> exclusiveZone(double zone) =>
      _call('setExclusiveZone', {'zone': zone});

  /// The screen size in points (null outside Silica or when unknown).
  static Future<Size?> outputSize() async {
    try {
      final size = await channel.invokeMethod<Object?>('outputSize');
      if (size is Map && size['width'] is num && size['height'] is num) {
        return Size(
          (size['width'] as num).toDouble(),
          (size['height'] as num).toDouble(),
        );
      }
    } on MissingPluginException {
      // Outside Silica.
    }
    return null;
  }

  /// The call's arguments, with this layer's view when it has one.
  Map<String, Object?> _args(Map<String, Object?> args) =>
      viewId == null ? args : {...args, 'viewId': viewId};

  Future<void> _call(String method, Map<String, Object?> args) async {
    try {
      await channel.invokeMethod<Object?>(method, _args(args));
    } on MissingPluginException {
      // Outside Silica (tests).
    }
  }

  @override
  bool operator ==(Object other) =>
      other is LayerControl && other.viewId == viewId;

  @override
  int get hashCode => viewId.hashCode;

  @override
  String toString() => 'LayerControl(view: $viewId)';
}

/// The layer of the shell component below it (#201): [LayerControl.of].
class LayerScope extends InheritedWidget {
  const LayerScope({super.key, required this.control, required super.child});

  final LayerControl control;

  @override
  bool updateShouldNotify(LayerScope oldWidget) => control != oldWidget.control;
}
