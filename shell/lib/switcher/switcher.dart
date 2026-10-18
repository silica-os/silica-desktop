// The app switcher (⌘Tab), on the Amalfi design system.
//
// Panel centred on the screen, Glass/Regular (the Dock's kind) with smooth corners of 28 and the
// level 2 shadow: 96 pt icons in one row, 120 pt pitch, 20 pt from the sides and the top; with 4
// apps it is 496 × 160 pt. The chosen app has a 112 × 112 box behind it (radius 24, a state
// overlay of the appearance) and below it the name (Body Emphasis, `Text/Primary`). The panel
// appears in one frame (no fade, no scale) and, once the modifier is released, fades out in
// ~190 ms, linearly; Esc closes it at once (all measured, app-switcher.md). Sizes are on the 4 pt grid; there is
// no switcher master in the kit, so they are ours, scaled from the Dock shelf (tile 52).
//
// Ossido handles the keys and forwards `switcher.next`, `switcher.previous` (⇧⌘Tab, with ⌘
// held), `switcher.commit` (⌘ released), `switcher.cancel` (Esc) and `switcher.expose` (↑ or
// ↓: App Exposé of the chosen app). The usage order of the apps is kept by this component
// from the focus events of the silica/shell channel. The component always runs on a
// full-screen overlay layer; the keyboard is Ossido's, and the pointer reaches only the
// panel while it is open: it moves the selection, and a file dragged from another app
// can be dropped on an icon to open it with that app.
//
// ⌘` is not bound (measured: it does nothing on the Italian layout; Ossido matches
// shortcuts on the layout's own symbols).

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../system/desktop_entries.dart';
import '../system/glass_profile.dart';
import '../system/icon_lookup.dart';
import '../system/layer_control.dart';
import '../system/shadowed_glass.dart';
import '../system/shell_channel.dart';
import '../system/silica_icons.dart';

const double iconSide = 96;
const double iconPitch = 120;
const double sidePadding = 20;
const double topPadding = 20;
const double panelHeight = 160;

/// Panel corners (smooth, like the Faro and Launcher panels).
const double panelRadius = AmalfiRadii.faroAndLauncher;
const double selectionSide = 112;
const double selectionRadius = 24;
const double labelGap = 4;

/// Capsule width for `count` apps at full size.
double panelWidth(int count) =>
    2 * sidePadding +
    count * iconSide +
    math.max(0, count - 1) * (iconPitch - iconSide);

/// An app in the switcher: its windows from the most recent.
@immutable
class SwitcherApp {
  const SwitcherApp({
    required this.key,
    required this.name,
    required this.windows,
    this.icon,
    this.openFiles,
  });

  /// The app_id (or the window id, if the app has none).
  final String key;
  final String name;
  final List<String> windows;
  final IconSource? icon;

  /// The app's command line, to open files dropped on its icon (null: it takes none).
  final String? openFiles;
}

/// Where the panel is on a screen of `size` with `count` apps: too many apps for the screen and
/// everything shrinks together by `k`.
({double left, double top, double width, double height, double k})
switcherFrame(Size size, int count) {
  final natural = panelWidth(count);
  final room = size.width - 2 * sidePadding;
  final k = natural > room && room > 0 ? room / natural : 1.0;
  final width = natural * k;
  final height = panelHeight * k;
  return (
    left: (size.width - width) / 2,
    top: (size.height - height) / 2,
    width: width,
    height: height,
    k: k,
  );
}

/// The app under `position` (the panel's cells, one pitch wide), or null outside the panel.
int? switcherIndexAt(Size size, int count, Offset position) {
  if (count == 0) return null;
  final f = switcherFrame(size, count);
  final panel = Rect.fromLTWH(f.left, f.top, f.width, f.height);
  if (!panel.contains(position)) return null;
  final gap = iconPitch - iconSide;
  final first = f.left + (sidePadding - gap / 2) * f.k;
  return ((position.dx - first) / (iconPitch * f.k)).floor().clamp(
    0,
    count - 1,
  );
}

/// The usage order of the windows and the open switcher.
class SwitcherModel {
  SwitcherModel({Map<String, DesktopEntry>? entries})
    : _entries = entries ?? const {};

  final Map<String, DesktopEntry> _entries;

  /// Windows from the most recent.
  final List<String> _recent = [];
  List<Toplevel> _toplevels = const [];

  /// The apps of the open switcher (null: closed) and the chosen one.
  List<SwitcherApp>? apps;
  int selected = 0;

  bool get visible => apps != null;

  void toplevels(List<Toplevel> list) {
    _toplevels = list;
    final ids = {for (final t in list) t.id};
    _recent.removeWhere((id) => !ids.contains(id));
    // New windows at the end, in the order they appeared.
    for (final t in list.reversed) {
      if (!_recent.contains(t.id)) _recent.add(t.id);
    }
  }

  void focused(String id) {
    if (id.isEmpty) return;
    _recent
      ..remove(id)
      ..insert(0, id);
  }

  /// The apps in usage order.
  List<SwitcherApp> recentApps() {
    final byId = {for (final t in _toplevels) t.id: t};
    final order = <String>[];
    final windows = <String, List<String>>{};
    final first = <String, Toplevel>{};
    for (final id in _recent) {
      final t = byId[id];
      if (t == null) continue;
      final key = t.appId.isEmpty ? 'window:${t.id}' : t.appId;
      if (!windows.containsKey(key)) {
        order.add(key);
        first[key] = t;
      }
      windows.putIfAbsent(key, () => []).add(id);
    }
    return [for (final key in order) _app(key, first[key]!, windows[key]!)];
  }

  SwitcherApp _app(String key, Toplevel top, List<String> ids) {
    final entry = _entries.values
        .where((e) => e.matchesAppId(top.appId))
        .firstOrNull;
    final IconSource? icon = switch (entry) {
      null => null,
      final e => switch ((silicaIconFor(e), findIcon(e.icon))) {
        (final LayeredIcon l, _) => LayeredSource(l),
        (null, final String path) => FlatSource(path),
        _ => null,
      },
    };
    final name = entry?.name ?? (top.title.isNotEmpty ? top.title : top.appId);
    return SwitcherApp(
      key: key,
      name: name,
      windows: ids,
      icon: icon,
      openFiles: entry?.rawExec,
    );
  }

  /// ⌘Tab: opens the switcher on the second app (the one used before), or moves forward.
  void next() => _step(1);

  /// ⇧⌘Tab: opens on the last app, or moves back.
  void previous() => _step(-1);

  void _step(int delta) {
    if (apps case final list?) {
      if (list.isEmpty) return;
      selected = (selected + delta) % list.length;
      return;
    }
    final list = recentApps();
    apps = list;
    selected = list.length < 2 ? 0 : (delta > 0 ? 1 : list.length - 1);
  }

  /// Moves the choice to `index` (the pointer, a file dragged over an icon).
  void select(int index) {
    final list = apps;
    if (list == null || index < 0 || index >= list.length) return;
    selected = index;
  }

  /// ⌘ released: the window to activate (the most recent of the chosen app).
  String? commit() {
    final list = apps;
    apps = null;
    if (list == null || list.isEmpty) return null;
    return list[selected.clamp(0, list.length - 1)].windows.firstOrNull;
  }

  void cancel() => apps = null;
}

class AppSwitcher extends StatefulWidget {
  const AppSwitcher({super.key, required this.entries});

  final Map<String, DesktopEntry> entries;

  @override
  State<AppSwitcher> createState() => _AppSwitcherState();
}

class _AppSwitcherState extends State<AppSwitcher>
    with SingleTickerProviderStateMixin {
  /// This component's layer surface (several share one engine, #201).
  late final LayerControl _layer;

  late final SwitcherModel _model = SwitcherModel(entries: widget.entries);

  /// Measured (app-switcher.md): releasing the modifier fades the panel out in ~190 ms,
  /// roughly linear; it appears in one frame and Esc closes it in the next, with no animation.
  static const Duration releaseFade = Duration(milliseconds: 190);
  late final AnimationController _release = AnimationController(
    vsync: this,
    duration: releaseFade,
    value: 1,
  )..addListener(() => setState(() {}));

  /// The panel being faded out after a release (null: none).
  ({List<SwitcherApp> apps, int selected})? _leaving;

  /// A new switcher opens over a fading one: the old panel goes at once.
  void _reopen() {
    if (_leaving == null) return;
    _release
      ..stop()
      ..value = 1;
    _leaving = null;
  }

  /// Commits the choice and fades the panel out from where it was.
  String? _commit() {
    final apps = _model.apps;
    final selected = _model.selected;
    final window = _model.commit();
    if (apps != null && apps.isNotEmpty) {
      _leaving = (apps: apps, selected: selected);
      _release.reverse(from: 1).whenCompleteOrCancel(() {
        if (mounted) setState(() => _leaving = null);
      });
    }
    return window;
  }

  StreamSubscription<String>? _commands;
  StreamSubscription<SilicaDragEvent>? _drags;
  final ShellChannel _shell = ShellChannel.instance;

  /// The layer's size (from the last build) and the input region last given to Ossido.
  Size _size = Size.zero;
  String _regionKey = '';

  @override
  void initState() {
    super.initState();
    _layer = LayerControl.of(context);
    _layer.inputRegion(const []);
    _layer.keyboard('none');
    _shell.toplevels.addListener(_toplevels);
    _shell.focused.addListener(_focused);
    _toplevels();
    _focused();
    _commands = _shell.commands.listen(_command);
    _drags = SilicaDrag.events.listen(_drag);
    unawaited(_shell.refresh());
  }

  @override
  void dispose() {
    _release.dispose();
    _commands?.cancel();
    _drags?.cancel();
    _shell.toplevels.removeListener(_toplevels);
    _shell.focused.removeListener(_focused);
    super.dispose();
  }

  void _toplevels() => _model.toplevels(_shell.toplevels.value);

  void _focused() => _model.focused(_shell.focused.value);

  void _command(String id) {
    switch (id) {
      case 'switcher.next':
        _reopen();
        setState(_model.next);
      case 'switcher.previous':
        _reopen();
        setState(_model.previous);
      case 'switcher.commit':
        final window = _commit();
        setState(() {});
        if (window != null) unawaited(_shell.activateApp(window));
      case 'switcher.cancel':
        setState(_model.cancel);
      case 'switcher.expose':
        // ↑ or ↓: App Exposé of the chosen app, which stays after ⌘ is released.
        final window = _commit();
        setState(() {});
        if (window != null) unawaited(_exposeOf(window));
    }
  }

  Future<void> _exposeOf(String window) async {
    await _shell.activateApp(window);
    await _shell.execute('panorama.app');
  }

  /// A file dragged from another app: the icon under it is chosen; dropped on it, the app
  /// opens the file.
  void _drag(SilicaDragEvent event) {
    final apps = _model.apps;
    if (apps == null || apps.isEmpty) return;
    switch (event) {
      case SilicaDragOver(:final position):
        final index = switcherIndexAt(_size, apps.length, position);
        if (index != null && index != _model.selected) {
          setState(() => _model.select(index));
        }
      case SilicaDrop(:final position, :final paths):
        final index = switcherIndexAt(_size, apps.length, position);
        if (index == null || paths.isEmpty) return;
        final exec = apps[index].openFiles;
        final command = exec == null ? null : execWithFiles(exec, paths);
        setState(_model.cancel);
        if (command != null) unawaited(_shell.launch(command));
      case SilicaDragLeave() || SilicaDragFinished():
        break;
    }
  }

  /// The pointer reaches the panel and nothing else while the switcher is open.
  void _syncRegion(List<SwitcherApp>? apps) {
    final key = apps == null || apps.isEmpty
        ? ''
        : '${_size.width},${_size.height},${apps.length}';
    if (key == _regionKey) return;
    _regionKey = key;
    final rects = key.isEmpty
        ? const <Rect>[]
        : [switcherFrame(_size, apps!.length)]
              .map((f) => Rect.fromLTWH(f.left, f.top, f.width, f.height))
              .toList();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _layer.inputRegion(rects);
    });
  }

  /// A click on an icon activates that app, as releasing ⌘ on it would.
  void _click(int index) {
    setState(() => _model.select(index));
    final window = _commit();
    setState(() {});
    if (window != null) unawaited(_shell.activateApp(window));
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        _size = constraints.biggest;
        final apps = _model.apps;
        _syncRegion(apps);
        if (apps == null || apps.isEmpty) {
          // After a release the panel fades out, linearly, without taking the pointer.
          final leaving = _leaving;
          if (leaving == null) return const SizedBox.expand();
          return IgnorePointer(
            child: Opacity(
              opacity: _release.value.clamp(0.0, 1.0),
              child: SwitcherPanel(
                apps: leaving.apps,
                selected: leaving.selected,
              ),
            ),
          );
        }
        // Opens in one frame, with no fade and no scale.
        return SwitcherPanel(
          apps: apps,
          selected: _model.selected,
          onHover: (i) {
            if (i != _model.selected) setState(() => _model.select(i));
          },
          onTap: _click,
        );
      },
    );
  }
}

/// The panel, centered in the space it has.
class SwitcherPanel extends StatelessWidget {
  const SwitcherPanel({
    super.key,
    required this.apps,
    required this.selected,
    this.onHover,
    this.onTap,
  });

  final List<SwitcherApp> apps;
  final int selected;

  /// The pointer entered an icon's cell (it moves the choice at once); a click on it.
  final ValueChanged<int>? onHover;
  final ValueChanged<int>? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = AmalfiTheme.of(context);
    return LayoutBuilder(
      builder: (context, constraints) {
        // Too many apps for the screen: everything shrinks together.
        final frame = switcherFrame(constraints.biggest, apps.length);
        final (:left, :top, :width, :height, :k) = frame;
        return Stack(
          children: [
            Positioned(
              left: left,
              top: top,
              width: width,
              height: height,
              child: ShadowedGlass(
                profile: ShellGlassProfile.dock,
                radius: panelRadius * k,
                child: const SizedBox.expand(),
              ),
            ),
            for (var i = 0; i < apps.length; i++)
              ..._item(
                theme,
                i,
                left + (sidePadding + i * iconPitch) * k,
                top + topPadding * k,
                k,
              ),
            // On top, so the pointer reaches them through the selection box and the icons.
            for (var i = 0; i < apps.length; i++) ..._cell(i, left, top, k),
          ],
        );
      },
    );
  }

  /// The cell of an icon (one pitch wide, the panel's height): hover and click.
  List<Widget> _cell(int index, double left, double top, double k) {
    if (onHover == null && onTap == null) return const [];
    final gap = iconPitch - iconSide;
    return [
      Positioned(
        key: ValueKey('switcher cell $index'),
        left: left + (sidePadding - gap / 2 + index * iconPitch) * k,
        top: top,
        width: iconPitch * k,
        height: panelHeight * k,
        child: MouseRegion(
          onEnter: (_) => onHover?.call(index),
          onHover: (_) => onHover?.call(index),
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: () => onTap?.call(index),
          ),
        ),
      ),
    ];
  }

  List<Widget> _item(
    AmalfiThemeData theme,
    int index,
    double x,
    double y,
    double k,
  ) {
    final app = apps[index];
    final side = iconSide * k;
    final inset = (selectionSide - iconSide) / 2 * k;
    return [
      if (index == selected)
        Positioned(
          key: const ValueKey('switcher selection'),
          left: x - inset,
          top: y - inset,
          width: selectionSide * k,
          height: selectionSide * k,
          child: DecoratedBox(
            decoration: AmalfiShapeDecoration(
              color: theme.isDark
                  ? theme.colors.stateHoverOverlay
                  : theme.colors.statePressOverlay,
              shape: AmalfiSmoothBorder(radius: selectionRadius * k),
            ),
          ),
        ),
      Positioned(
        left: x,
        top: y,
        width: side,
        height: side,
        child: switch (app.icon) {
          final source? => SilicaIcon(source, side: side),
          null => DecoratedBox(
            decoration: AmalfiShapeDecoration(
              color: theme.colors.surfaceControl,
              shape: AmalfiSmoothBorder(radius: side * 0.225),
            ),
          ),
        },
      ),
      if (index == selected)
        Positioned(
          left: x - inset - 40 * k,
          top: y - inset + (selectionSide + labelGap) * k,
          width: (selectionSide + 80) * k,
          child: Text(
            app.name,
            textAlign: TextAlign.center,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: AmalfiType.bodyEmphasis(color: theme.colors.textPrimary)
                .copyWith(fontSize: AmalfiType.bodyEmphasisToken.size * k),
          ),
        ),
    ];
  }
}
