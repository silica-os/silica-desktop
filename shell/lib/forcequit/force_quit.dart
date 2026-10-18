// Force quit (⌘⌥⎋): an Amalfi window in the centre of the screen: the title bar with its
// window controls (minimise and zoom disabled), the explanation, the list of open apps (the
// frontmost app already chosen), the shortcut hint at the bottom left and the primary
// "Force Quit" push button at the bottom right. The button kills the app with
// `app.force_quit`. The texts are our own. The list has no kit master ("Lists: no kit
// masters"): its surface is the `Dialogs/Browser/Files` file list (Surface/Raised, radius 12,
// 8 pt of padding) with 28 pt rows as `Sidebars/Row/Plain` (selected: Accent/Amalfi Soft, radius
// 8). Sizes are on the 4 pt grid.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../system/desktop_entries.dart';
import '../system/icon_lookup.dart';
import '../system/layer_arbiter.dart';
import '../system/layer_control.dart';
import '../system/shell_channel.dart';
import '../system/silica_icons.dart';

const Size forceQuitSize = Size(400, 368);

/// Under the title bar (48): 20 pt of padding, the explanation (2 lines of 16), 12 pt, the list
/// (192 pt), 16 pt, the footer (28 pt), 20 pt.
const double forceQuitPadding = 20;
const Rect forceQuitList = Rect.fromLTWH(20, 128, 360, 192);
const double forceQuitRow = 28;
const double forceQuitListPadding = 8;
const double forceQuitFooterTop = 336 - 8;

/// An open app in the list.
class ForceQuitApp {
  const ForceQuitApp(this.appId, this.name, this.icon);
  final String appId;
  final String name;
  final IconSource? icon;
}

class ForceQuitLayer extends StatefulWidget {
  const ForceQuitLayer({super.key, required this.entries});
  final Map<String, DesktopEntry> entries;

  @override
  State<ForceQuitLayer> createState() => _ForceQuitLayerState();
}

class _ForceQuitLayerState extends State<ForceQuitLayer> {
  /// The arbiter of this component's layer surface (#201).
  late final LayerArbiter _arbiter;

  StreamSubscription<String>? _commands;
  bool _open = false;
  String? _chosen;
  final FocusNode _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    _arbiter = LayerArbiter.of(LayerControl.of(context));
    _commands = ShellChannel.instance.commands.listen((id) {
      if (id == 'app.force_quit_panel') _show();
    });
    ShellChannel.instance.toplevels.addListener(_changed);
  }

  @override
  void dispose() {
    _arbiter.remove(this);
    _commands?.cancel();
    ShellChannel.instance.toplevels.removeListener(_changed);
    _focus.dispose();
    super.dispose();
  }

  void _changed() {
    if (_open) setState(() {});
  }

  /// The open apps, by name, once each.
  List<ForceQuitApp> _apps() {
    final seen = <String>{};
    final apps = <ForceQuitApp>[];
    for (final top in ShellChannel.instance.toplevels.value) {
      if (top.appId.isEmpty || !seen.add(top.appId)) continue;
      final entry = widget.entries.values
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
      apps.add(
        ForceQuitApp(
          top.appId,
          entry?.name ?? (top.title.isNotEmpty ? top.title : top.appId),
          icon,
        ),
      );
    }
    apps.sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    return apps;
  }

  void _show() {
    // Chosen: the frontmost app.
    final focused = ShellChannel.instance.focused.value;
    final top = ShellChannel.instance.toplevels.value
        .where((t) => t.id == focused)
        .firstOrNull;
    setState(() {
      _open = true;
      _chosen = top?.appId;
    });
    _arbiter.set(this, const LayerWish.all());
    WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  void _hide() {
    setState(() => _open = false);
    _arbiter.set(this, const LayerWish.none());
  }

  void _quit() {
    final app = _chosen;
    if (app == null) return;
    ShellChannel.instance.execute('app.force_quit', {'app_id': app});
    _hide();
  }

  /// ↑ ↓ move the choice, Return forces the chosen app to quit, Esc closes.
  KeyEventResult _key(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.escape) {
      _hide();
      return KeyEventResult.handled;
    }
    final apps = _apps();
    if (key == LogicalKeyboardKey.arrowDown ||
        key == LogicalKeyboardKey.arrowUp) {
      if (apps.isEmpty) return KeyEventResult.handled;
      final at = apps.indexWhere((a) => a.appId == _chosen);
      final next = key == LogicalKeyboardKey.arrowDown
          ? (at + 1).clamp(0, apps.length - 1)
          : (at < 0 ? apps.length - 1 : at - 1).clamp(0, apps.length - 1);
      setState(() => _chosen = apps[next].appId);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      _quit();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    if (!_open) return const SizedBox.shrink();
    final apps = _apps();
    final l10n = l10nOf(context);
    final colors = AmalfiTheme.of(context).colors;
    final list = DecoratedBox(
      decoration: AmalfiShapeDecoration(
        color: colors.surfaceRaised,
        shape: AmalfiSmoothBorder(
          radius: AmalfiDialogMetrics.filesRadius,
          side: BorderSide(
            color: colors.lineBorder,
            strokeAlign: BorderSide.strokeAlignInside,
          ),
        ),
      ),
      child: AmalfiScrollbar(
        child: ListView(
          key: const ValueKey('force quit list'),
          padding: const EdgeInsets.all(forceQuitListPadding),
          itemExtent: forceQuitRow,
          children: [for (final app in apps) _row(colors, app)],
        ),
      ),
    );
    return Focus(
      focusNode: _focus,
      onKeyEvent: _key,
      child: Center(
        child: SizedBox(
          key: const ValueKey('force quit'),
          width: forceQuitSize.width,
          height: forceQuitSize.height,
          child: AmalfiWindow(
            // The compositor does not draw this window: the layer does, shadow included.
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(
                  height: AmalfiToolbarMetrics.titleBarHeight,
                  child: AmalfiTitleBar(
                    title: l10n.forceQuitTitle,
                    onClose: _hide,
                  ),
                ),
                Expanded(
                  child: Padding(
                    padding: const EdgeInsets.fromLTRB(
                      forceQuitPadding,
                      forceQuitPadding,
                      forceQuitPadding,
                      forceQuitPadding,
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        SizedBox(
                          height: 32,
                          child: Text(
                            l10n.forceQuitHelp,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: AmalfiType.callout(
                              color: colors.textSecondary,
                            ),
                          ),
                        ),
                        const SizedBox(height: 12),
                        Expanded(child: list),
                        const SizedBox(height: 16),
                        Row(
                          children: [
                            Expanded(
                              child: Text(
                                l10n.forceQuitShortcut,
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: AmalfiType.caption(
                                  color: colors.textTertiary,
                                ),
                              ),
                            ),
                            const SizedBox(width: 12),
                            AmalfiPushButton(
                              key: const ValueKey('force quit button'),
                              label: l10n.forceQuitButton,
                              variant: AmalfiPushButtonVariant.primary,
                              onPressed: _chosen == null ? null : _quit,
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _row(AmalfiColors colors, ForceQuitApp app) {
    final chosen = app.appId == _chosen;
    return _ForceQuitRow(
      key: ValueKey('app ${app.appId}'),
      app: app,
      chosen: chosen,
      onTap: () => setState(() => _chosen = app.appId),
      onDoubleTap: () {
        setState(() => _chosen = app.appId);
        _quit();
      },
    );
  }
}

/// A row of the list: a 20 pt icon and the name; chosen, an `Accent/Amalfi Soft` capsule;
/// under the pointer, a `Line/Separator` one (`Sidebars/Row/Plain`).
class _ForceQuitRow extends StatefulWidget {
  const _ForceQuitRow({
    super.key,
    required this.app,
    required this.chosen,
    required this.onTap,
    required this.onDoubleTap,
  });

  final ForceQuitApp app;
  final bool chosen;
  final VoidCallback onTap;
  final VoidCallback onDoubleTap;

  @override
  State<_ForceQuitRow> createState() => _ForceQuitRowState();
}

class _ForceQuitRowState extends State<_ForceQuitRow> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final colors = AmalfiTheme.of(context).colors;
    final app = widget.app;
    final fill = widget.chosen
        ? colors.accentAmalfiSoft
        : _hovered
        ? colors.lineSeparator
        : null;
    return MouseRegion(
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: widget.onTap,
        onDoubleTap: widget.onDoubleTap,
        child: DecoratedBox(
          decoration: AmalfiShapeDecoration(
            color: fill,
            shape: const AmalfiSmoothBorder(radius: 8),
          ),
          child: Row(
            children: [
              const SizedBox(width: 12),
              SizedBox.square(
                dimension: 20,
                child: switch (app.icon) {
                  final icon? => SilicaIcon(icon, side: 20),
                  null => null,
                },
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  app.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: widget.chosen
                      ? AmalfiType.bodyEmphasis(color: colors.textPrimary)
                      : AmalfiType.body(color: colors.textPrimary),
                ),
              ),
              const SizedBox(width: 12),
            ],
          ),
        ),
      ),
    );
  }
}
