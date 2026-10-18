// Banners and the Notification Centre on the Amalfi kit (`Notifications/*`), with the
// behaviour measured in docs/spec/notifications.md.
//
// A 360 pt wide column 16 pt from the right edge and 16 pt below the menu bar. A banner is a
// Glass/Regular card (radius 18) 76 pt tall (108 with actions): 36 pt app icon at (12, 12), the
// title (Headline) and body (Callout, `Text/Secondary`, two lines) from x 56, the time (Caption)
// at the top right, and the actions as small secondary push buttons. It slides in from the
// trailing edge with `spring.panel` and leaves with `fade.standard` (motion.md).
//
// The Centre (the `notifications.toggle` command, a click on the clock) is a 380 × 416
// Glass/Thick panel (radius 24): "Notifications" title and a "Clear all" link, the sections
// Today and Earlier, and the notifications as opaque `Surface/Raised` cards (glass is never
// stacked on glass) 348 pt wide, radius 16; a group of one app folds into a stack of plates.
//
// The component keeps running on a full-screen overlay layer: without banners or the Centre it
// takes neither pointer nor keyboard (silica/layer).

import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:silica_ui/silica_ui.dart';

import '../menubar/menubar.dart' show barHeight;
import '../l10n/l10n.dart';
import '../system/cross_glyph.dart';
import '../system/desktop_entries.dart';
import '../system/glass_profile.dart';
import '../system/icon_lookup.dart';
import '../system/layer_control.dart';
import '../system/shadow_image.dart';
import '../system/sounds.dart';
import '../system/shell_channel.dart';
import '../system/system_status.dart';
import 'banner.dart';
import 'model.dart';
import 'service.dart';
import 'settings.dart';

/// The banner column.
const double columnWidth = 360;
const double columnMargin = 16;

/// The surface covers the whole screen (exclusive zone -1): the column starts below the
/// menu bar.
const double columnTop = barHeight + columnMargin;

/// Banner (`Banner/Basic`): Glass/Regular, radius 18, padding 12, the 36 pt icon, the text from
/// x 56 (44 after the content's own padding), the body ends 40 pt short of the right edge
/// so that the time fits above it.
const double cardRadius = 18;
const double iconSize = 36;
const double cardPadding = 12;
const double textLeft = cardPadding + iconSize + 8;
const double bodyRight = 40;
const double bannerHeight = 76;

/// Actions (`Banner/With Actions`): small secondary push buttons 24 pt tall, 8 pt under the
/// content, 8 pt apart, from the text's x.
const double actionsTop = 8;

/// The content block of a card: the title and two lines of body.
const double contentHeight = 52;
const double actionsGap = 8;

/// The Centre (`Centre/Panel`): 380 × 416, Glass/Thick, radius 24, 16 pt inset; the cards are
/// opaque (`Banner/Panel`: 348 wide, radius 16).
const double centreWidth = 380;
const double centreMaxHeight = 416;
const double centreRadius = 24;
const double centreInset = 16;
const double panelCardRadius = 16;
const double centreHeader = 24;
const double sectionHeight = 16;
const double cardGap = 8;

/// Buttons that appear when the pointer passes over the banner (session 8, N-02).
const double closeSize = 20;
const Offset closeCenter = Offset(4, 6);
const Size detailsSize = Size(64, 24);
const double detailsInset = 12;

/// Space between the shortened text and the capsule (not measured).
const double detailsGap = 8;

/// A collapsed group in the Centre: two plates peek out under the card, inset 16 and 32 pt,
/// 10 pt apart (`Stack/Grouped`).
const double plateStep = 10;
const List<double> plateInsets = [16, 32];

/// Expanded group: the cards at a 6 pt pitch, the header 24 pt tall.
const double expandedGap = 6;
const double groupHeaderHeight = 24;

/// Opens the app that sent `n`: its desktop entry (`desktop-entry` hint), or an installed
/// app with the same name. Nothing when it cannot be found.
void openSenderApp(ShellNotification n) {
  final entries = loadDesktopEntries();
  final byId = n.desktopEntry.isEmpty
      ? null
      : entries[n.desktopEntry] ??
            entries[n.desktopEntry.replaceAll(RegExp(r'\.desktop$'), '')];
  final entry =
      byId ?? entries.values.where((e) => e.name == n.appName).firstOrNull;
  if (entry == null || entry.exec.isEmpty) return;
  ShellChannel.instance.launch(entry.exec);
}

/// `now` for a notification of the last minute, otherwise `HH:MM` of the arrival time.
String timeLabel(DateTime t, {DateTime? now, String nowLabel = 'now'}) {
  final reference = now ?? DateTime.now();
  if (reference.difference(t) < const Duration(minutes: 1)) return nowLabel;
  return '${t.hour.toString().padLeft(2, '0')}:${t.minute.toString().padLeft(2, '0')}';
}

class NotificationsLayer extends StatefulWidget {
  const NotificationsLayer({
    super.key,
    required this.source,
    this.system,
    this.startOpen = false,
    this.settings = NotificationSettings.read,
    this.openApp = openSenderApp,
    this.playAlert = playAlertSound,
  });

  /// A click on a notification without a `default` action opens its app (N-02).
  final void Function(ShellNotification) openApp;

  /// Plays the alert sound of a notification that comes with a banner (and a sound).
  final Future<void> Function() playAlert;

  final NotificationsSource source;

  /// The per-app styles (banners, alerts, none), read again for every notification so a
  /// change in Settings applies at once.
  final NotificationSettings Function() settings;

  /// With Do Not Disturb on, notifications go to the Centre without a banner.
  final SystemSource? system;
  final bool startOpen;

  @override
  State<NotificationsLayer> createState() => _NotificationsLayerState();
}

class _BannerEntry {
  _BannerEntry(this.notification, {this.alert = false});

  final Key key = UniqueKey();
  ShellNotification notification;

  /// Alerts style: it stays until the user acts on it.
  final bool alert;
  bool leaving = false;
}

class _NotificationsLayerState extends State<NotificationsLayer>
    with SingleTickerProviderStateMixin {
  /// This component's layer surface (several share one engine, #201).
  late final LayerControl _layer;

  /// Opening and closing of the Centre: the value is the elapsed time, relative to the
  /// duration of the current phase.
  late final AnimationController _center;
  final FocusNode _focus = FocusNode();
  StreamSubscription<ShellNotification>? _posted;
  StreamSubscription<String>? _commands;

  /// The banners on screen: at most one entering or staying, the others leaving.
  final List<_BannerEntry> _banners = [];
  bool _centerOpen = false;
  final Set<String> _expanded = {};

  /// The surface size at the last layout.
  Size? _size;

  @override
  void initState() {
    super.initState();
    _layer = LayerControl.of(context);
    _center = AnimationController(vsync: this)
      ..addListener(() => setState(() {}));
    widget.source.addListener(_changed);
    _posted = widget.source.posted.listen(_post);
    _commands = ShellChannel.instance.commands.listen((id) {
      if (id == 'notifications.toggle') _toggleCenter();
    });
    _dismissed = _layer.transientDismissed.listen((_) {
      if (!_centerOpen) return;
      _dismissedAt = DateTime.now();
      _closeCenter();
    });
    if (widget.startOpen) {
      _openCenter();
    } else {
      _updateInput();
    }
  }

  @override
  void dispose() {
    widget.source.removeListener(_changed);
    _posted?.cancel();
    _commands?.cancel();
    _dismissed?.cancel();
    _center.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _changed() {
    // Closed elsewhere (from the Centre, by the app, expired): the banner leaves.
    for (final banner in _banners) {
      if (!widget.source.notifications.containsKey(banner.notification.id)) {
        banner.leaving = true;
      }
    }
    setState(() {});
  }

  void _post(ShellNotification n) {
    // With a Focus (Do Not Disturb) on the notification is delivered silently, without a
    // banner, and waits in the Centre (SH-NC-07, measured).
    if (_centerOpen || (widget.system?.value.doNotDisturb ?? false)) return;
    final settings = widget.settings();
    final style = settings.styleOf(n);
    if (style == NotificationStyle.none) return;
    if (settings.soundOf(n)) unawaited(widget.playAlert());
    n = settings.present(n);
    setState(() {
      final same = _banners.where(
        (b) => !b.leaving && b.notification.id == n.id,
      );
      if (same.isNotEmpty) {
        // Replaced by the app (same id): the content changes in place.
        same.first.notification = n;
        return;
      }
      // A new notification replaces the current banner, which leaves while it enters.
      for (final banner in _banners) {
        banner.leaving = true;
      }
      _banners.add(_BannerEntry(n, alert: style == NotificationStyle.alerts));
    });
    _updateInput();
  }

  void _gone(_BannerEntry banner) {
    setState(() => _banners.remove(banner));
    _updateInput();
  }

  // A press on the bar's clock while the Centre is open: Ossido first ends the panel
  // (transientDismissed, the press goes through), then the bar asks for the toggle, which
  // must not open it again.
  void _toggleCenter() {
    if (!_centerOpen && _dismissedAt != null) {
      final since = DateTime.now().difference(_dismissedAt!);
      if (since < const Duration(milliseconds: 400)) return;
    }
    _centerOpen ? _closeCenter() : _openCenter();
  }

  StreamSubscription<void>? _dismissed;
  DateTime? _dismissedAt;

  void _openCenter() {
    setState(() {
      _banners.clear();
      _centerOpen = true;
      ShellChannel.instance.broadcast('notifications.open', '1');
    });
    _center
      ..duration = centerOpenDuration
      ..forward(from: 0);
    _updateInput();
    WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  void _closeCenter() {
    if (!_centerOpen) return;
    setState(() {
      _centerOpen = false;
      ShellChannel.instance.broadcast('notifications.open', '0');
      _expanded.clear();
    });
    _center
      ..duration = centerCloseDuration
      ..forward(from: 0);
    _updateInput();
  }

  /// The pointer is needed only where there is something: the banner, or the Centre's
  /// panel while it is open; a press outside it arrives from Ossido (#149), so the
  /// windows below keep their hover. Without that support the Centre covers the screen.
  void _updateInput() {
    if (_centerOpen) {
      _layer.keyboard('exclusive');
      final size = _size;
      if (size == null) {
        _layer.inputRegion(null);
        return;
      }
      final left = size.width - columnMargin - centreWidth - 8;
      final column = Rect.fromLTWH(
        left,
        barHeight,
        size.width - left,
        columnMargin + centreMaxHeight + 32,
      );
      _layer.transient(rects: [column], policy: TransientPolicy.panel).then((
        supported,
      ) {
        if (mounted && _centerOpen) {
          _layer.inputRegion(supported ? [column] : null);
        }
      });
      return;
    }
    _layer.clearTransient();
    _layer.keyboard('none');
    final size = _size;
    if (_banners.isEmpty || size == null) {
      _layer.inputRegion(const []);
      return;
    }
    // The column at the top right, with room for the close button.
    final left = size.width - columnMargin - columnWidth - 8;
    _layer.inputRegion([
      Rect.fromLTWH(left, barHeight, size.width - left, 260),
    ]);
  }

  /// A click: the app's `default` action, otherwise the sender app opens (N-02: the click
  /// launched the sender with its window).
  void _activate(ShellNotification n) {
    if (n.hasDefault) {
      widget.source.invoke(n.id, 'default');
    } else {
      // As after the `default` action the notification is handled and leaves the Centre.
      widget.openApp(n);
      if (!n.resident) widget.source.dismiss(n.id);
    }
    // Clicked in the Centre: the app comes forward and the Centre closes.
    _closeCenter();
  }

  /// Rightward offset of the Centre and opacity in the current phase; null if closed.
  (double, double)? _centerPose() {
    final ms = _center.value * (_center.duration?.inMilliseconds ?? 0);
    if (_centerOpen) {
      final x = centerOpenOffset(_center.isCompleted ? 1e9 : ms);
      return (x, 1 - x / centerDistance);
    }
    if (!_center.isAnimating) return null;
    final x = centerCloseOffset(ms);
    return (x, 1 - x / centerDistance);
  }

  @override
  Widget build(BuildContext context) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && !_centerOpen && _banners.isNotEmpty) _updateInput();
    });
    final pose = _centerPose();
    return LayoutBuilder(
      builder: (context, constraints) {
        _size = constraints.biggest;
        final width = constraints.maxWidth;
        final left = width - columnMargin - columnWidth;
        return Stack(
          clipBehavior: Clip.none,
          children: [
            if (pose != null)
              Positioned.fill(
                child: Opacity(
                  opacity: pose.$2.clamp(0.0, 1.0),
                  child: Transform.translate(
                    offset: Offset(pose.$1, 0),
                    child: _centerView(width - columnMargin - centreWidth),
                  ),
                ),
              ),
            if (!_centerOpen)
              for (final banner in _banners)
                Positioned(
                  key: banner.key,
                  left: left,
                  top: columnTop,
                  width: columnWidth,
                  child: BannerSlot(
                    notification: banner.notification,
                    leaving: banner.leaving,
                    persistent: banner.alert,
                    onGone: () => _gone(banner),
                    onActivate: () => _activate(banner.notification),
                    onDismiss: () =>
                        widget.source.dismiss(banner.notification.id),
                    onAction: (key) =>
                        widget.source.invoke(banner.notification.id, key),
                    onReply: (text) =>
                        widget.source.reply(banner.notification.id, text),
                    onReplyFocus: (focused) =>
                        _layer.keyboard(focused ? 'exclusive' : 'none'),
                  ),
                ),
          ],
        );
      },
    );
  }

  Widget _centerView(double left) {
    final settings = widget.settings();
    final groups = groupNotifications([
      for (final n in widget.source.notifications.values)
        if (settings.inCenter(n)) settings.present(n),
    ], ungrouped: (n) => settings.groupingOf(n) == GroupingMode.off);
    return Focus(
      focusNode: _focus,
      onKeyEvent: (node, event) {
        if (event is KeyDownEvent &&
            event.logicalKey == LogicalKeyboardKey.escape) {
          _closeCenter();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Stack(
        children: [
          Positioned.fill(
            child: GestureDetector(
              onTap: _closeCenter,
              behavior: HitTestBehavior.opaque,
            ),
          ),
          Positioned(
            left: left,
            top: columnTop,
            width: centreWidth,
            child: _CentrePanel(
              empty: groups.isEmpty,
              onClear: widget.source.clear,
              children: _sections(groups),
            ),
          ),
        ],
      ),
    );
  }

  /// The sections of the panel: the groups that arrived today, then the earlier ones, each
  /// under its label.
  List<Widget> _sections(List<NotificationGroup> groups) {
    final l = l10nOf(context);
    final midnight = DateTime.now().copyWith(
      hour: 0,
      minute: 0,
      second: 0,
      millisecond: 0,
      microsecond: 0,
    );
    final today = [
      for (final g in groups)
        if (!g.latest.timestamp.isBefore(midnight)) g,
    ];
    final earlier = [
      for (final g in groups)
        if (g.latest.timestamp.isBefore(midnight)) g,
    ];
    Widget label(String text) => SizedBox(
      height: sectionHeight,
      child: Builder(
        builder: (context) => Text(
          text,
          maxLines: 1,
          style: AmalfiType.label(
            color: AmalfiTheme.of(context).colors.textTertiary,
          ).copyWith(fontWeight: FontWeight.w600),
        ),
      ),
    );
    return [
      if (today.isNotEmpty) ...[
        label(l.notificationsToday),
        const SizedBox(height: cardGap),
        for (final g in today) ..._group(g),
      ],
      if (earlier.isNotEmpty) ...[
        label(l.notificationsEarlier),
        const SizedBox(height: cardGap),
        for (final g in earlier) ..._group(g),
      ],
    ];
  }

  List<Widget> _group(NotificationGroup group) {
    final expanded = _expanded.contains(group.key) || group.items.length == 1;
    if (!expanded) {
      final plates = (group.items.length - 1).clamp(0, plateInsets.length);
      return [
        GestureDetector(
          onTap: () => setState(() => _expanded.add(group.key)),
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              // The plates of the kit's grouped stack: behind the card, the nearest one
              // wider than the farthest.
              for (var i = plates - 1; i >= 0; i--)
                Positioned(
                  left: plateInsets[i],
                  right: plateInsets[i],
                  bottom: (plates - 1 - i) * plateStep,
                  height: bannerHeight,
                  child: const _Plate(),
                ),
              Padding(
                padding: EdgeInsets.only(bottom: plates * plateStep),
                child: _CenterCard(
                  notification: group.latest,
                  source: widget.source,
                  onTap: () => setState(() => _expanded.add(group.key)),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: cardGap),
      ];
    }
    return [
      if (group.items.length > 1)
        _GroupHeader(
          title: group.latest.appName,
          onCollapse: () => setState(() => _expanded.remove(group.key)),
          onClear: () {
            for (final n in group.items) {
              widget.source.dismiss(n.id);
            }
          },
        ),
      for (final n in group.items) ...[
        _CenterCard(
          notification: n,
          source: widget.source,
          onTap: () => _activate(n),
        ),
        const SizedBox(height: expandedGap),
      ],
      const SizedBox(height: cardGap - expandedGap),
    ];
  }
}

/// The Centre panel (`Centre/Panel`): Glass/Thick, radius 24, the header (title and the "Clear
/// all" link) on top and the sections below, which scroll when they do not fit 416 pt.
class _CentrePanel extends StatelessWidget {
  const _CentrePanel({
    required this.empty,
    required this.onClear,
    required this.children,
  });

  final bool empty;
  final VoidCallback onClear;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final l = l10nOf(context);
    final colors = AmalfiTheme.of(context).colors;
    return ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: centreMaxHeight),
      child: DecoratedBox(
        decoration: AmalfiShapeDecoration(
          shape: AmalfiSmoothBorder(radius: centreRadius),
          shadows: [
            BoxShadow(
              color: colors.shadow3,
              offset: const Offset(0, 12),
              blurRadius: 32,
            ),
          ],
        ),
        child: ProfiledGlass(
          profile: ShellGlassProfile.panel,
          radius: centreRadius,
          material: GlassMaterial.thick,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(
                  centreInset + 4,
                  centreInset,
                  centreInset,
                  8,
                ),
                child: SizedBox(
                  height: centreHeader,
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          l.notificationsTitle,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: AmalfiType.title3(color: colors.textPrimary),
                        ),
                      ),
                      if (!empty)
                        AmalfiLinkButton(
                          label: l.notificationsClearAll,
                          onPressed: onClear,
                        ),
                    ],
                  ),
                ),
              ),
              if (empty)
                Padding(
                  padding: const EdgeInsets.fromLTRB(
                    centreInset,
                    24,
                    centreInset,
                    48,
                  ),
                  child: Center(
                    child: Text(
                      l.noNotifications,
                      style: AmalfiType.body(color: colors.textSecondary),
                    ),
                  ),
                )
              else
                Flexible(
                  child: AmalfiScrollbar(
                    child: SingleChildScrollView(
                      clipBehavior: Clip.none,
                      padding: const EdgeInsets.fromLTRB(
                        centreInset,
                        0,
                        centreInset,
                        centreInset,
                      ),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: children,
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A plate under a collapsed group: an opaque `Surface/Raised` card with a hairline border.
class _Plate extends StatelessWidget {
  const _Plate();

  @override
  Widget build(BuildContext context) {
    final colors = AmalfiTheme.of(context).colors;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: colors.surfaceRaised,
        borderRadius: BorderRadius.circular(panelCardRadius),
        border: Border.all(color: colors.lineSeparator),
      ),
    );
  }
}

/// The header of an expanded group: the app name on the left, the "Collapse" and "Clear"
/// links on the right.
class _GroupHeader extends StatelessWidget {
  const _GroupHeader({
    required this.title,
    required this.onCollapse,
    required this.onClear,
  });

  final String title;
  final VoidCallback onCollapse;
  final VoidCallback onClear;

  @override
  Widget build(BuildContext context) {
    final l = l10nOf(context);
    final colors = AmalfiTheme.of(context).colors;
    return SizedBox(
      height: groupHeaderHeight,
      child: Row(
        children: [
          const SizedBox(width: 4),
          Expanded(
            child: Text(
              title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: AmalfiType.headline(color: colors.textPrimary),
            ),
          ),
          AmalfiLinkButton(label: l.collapse, onPressed: onCollapse),
          AmalfiLinkButton(label: l.clear, onPressed: onClear),
        ],
      ),
    );
  }
}

/// A Centre card: with the time, the actions and the close button on hover.
class _CenterCard extends StatefulWidget {
  const _CenterCard({
    required this.notification,
    required this.source,
    required this.onTap,
  });

  final ShellNotification notification;
  final NotificationsSource source;
  final VoidCallback onTap;

  @override
  State<_CenterCard> createState() => _CenterCardState();
}

/// A card in the Centre. Dragged to the right it follows the pointer and, on release, fades
/// with the banner's exit (measured) and is cleared (SH-NC-12; the Centre drag was not measured: the
/// banner's N-02 behaviour, provisional).
class _CenterCardState extends State<_CenterCard>
    with SingleTickerProviderStateMixin {
  bool _hover = false;
  double _drag = 0;
  late final AnimationController _exit = AnimationController(
    vsync: this,
    duration: bannerExitDuration,
  )..addListener(() => setState(() {}));

  @override
  void dispose() {
    _exit.dispose();
    super.dispose();
  }

  Future<void> _leave() async {
    if (_exit.isAnimating || _exit.value > 0) return;
    await _exit.forward(from: 0);
    if (mounted) widget.source.dismiss(widget.notification.id);
  }

  @override
  Widget build(BuildContext context) {
    final n = widget.notification;
    final exitMs = _exit.value * bannerExitDuration.inMilliseconds;
    final exiting = _exit.value > 0;
    final dx = _drag + (exiting ? bannerExitOffset(exitMs) : 0);
    final opacity = exiting ? bannerExitOpacity(exitMs) : 1.0;
    return Transform.translate(
      offset: Offset(dx, 0),
      child: Opacity(
        opacity: opacity.clamp(0.0, 1.0),
        child: MouseRegion(
          onEnter: (_) => setState(() => _hover = true),
          onExit: (_) => setState(() => _hover = false),
          child: GestureDetector(
            onTap: exiting ? null : widget.onTap,
            onHorizontalDragUpdate: exiting
                ? null
                : (d) => setState(
                    () => _drag = (_drag + d.delta.dx).clamp(0, 1e4),
                  ),
            // Released after a drag to the right: it leaves; a drag back to the start stays.
            onHorizontalDragEnd: exiting
                ? null
                : (_) => _drag > 0 ? _leave() : null,
            child: NotificationCard(
              notification: n,
              panel: true,
              showTime: true,
              showActions: true,
              bodyLines: 4,
              showClose: _hover && !exiting,
              onClose: () => widget.source.dismiss(n.id),
              onAction: (key) => widget.source.invoke(n.id, key),
              onReply: (text) => widget.source.reply(n.id, text),
            ),
          ),
        ),
      ),
    );
  }
}

/// The card of a notification (`Banner/Basic`, `Banner/With Actions`): Glass/Regular for a
/// banner on the desktop, an opaque `Surface/Raised` card (`Banner/Panel`) with [panel] in the
/// Centre. The icon, title, body and, in the Centre, the time and the actions.
class NotificationCard extends StatelessWidget {
  const NotificationCard({
    super.key,
    required this.notification,
    this.panel = false,
    this.showTime = false,
    this.showActions = false,
    this.bodyLines = 2,
    this.shadow = false,
    this.showClose = false,
    this.onClose,
    this.onAction,
    this.showDetails = false,
    this.onDetails,
    this.onReply,
    this.onReplyFocus,
  });

  final ShellNotification notification;

  /// A card of the Centre panel: opaque, radius 16, no shadow.
  final bool panel;

  /// The reply field, for notifications that take one (with the actions shown).
  final ValueChanged<String>? onReply;

  /// The reply field takes or loses the keyboard (the banner layer asks for it).
  final ValueChanged<bool>? onReplyFocus;
  final bool showTime;
  final bool showActions;
  final int bodyLines;
  final bool shadow;
  final bool showClose;
  final VoidCallback? onClose;
  final ValueChanged<String>? onAction;

  /// The "Details" capsule on the right (banner on hover): the text shortens to make room
  /// for it.
  final bool showDetails;
  final VoidCallback? onDetails;

  @override
  Widget build(BuildContext context) {
    final n = notification;
    final colors = AmalfiTheme.of(context).colors;
    final buttons = showActions ? n.buttons : const <(String, String)>[];
    final content = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _content(
          context,
          colors,
          n,
          withActions:
              (showActions && n.canReply && onReply != null) ||
              buttons.isNotEmpty,
        ),
        if (showActions && n.canReply && onReply != null)
          _ReplyField(
            key: ValueKey('reply ${n.id}'),
            placeholder: n.replyLabel,
            onSubmit: onReply!,
            onFocus: onReplyFocus,
          ),
        if (buttons.isNotEmpty) _actions(buttons),
      ],
    );
    final Widget card = panel
        ? DecoratedBox(
            decoration: BoxDecoration(
              color: colors.surfaceRaised,
              borderRadius: BorderRadius.circular(panelCardRadius),
            ),
            child: content,
          )
        : Stack(
            clipBehavior: Clip.none,
            children: [
              if (shadow)
                Positioned.fill(
                  child: CachedShadow(
                    cacheKey: ('notification card', colors.shadow2),
                    bleed: 40,
                    paintShadow: (canvas, size) =>
                        _paintOutsideShadow(canvas, size, colors.shadow2),
                  ),
                ),
              const Positioned.fill(
                child: ProfiledGlass(
                  profile: ShellGlassProfile.banner,
                  radius: cardRadius,
                  material: GlassMaterial.regular,
                ),
              ),
              content,
            ],
          );
    return Stack(
      clipBehavior: Clip.none,
      children: [
        card,
        // The x on hover: a 20 pt circle centred 4 pt right of and 6 pt below the top left
        // corner of the card.
        if (showClose)
          Positioned(
            left: closeCenter.dx - closeSize / 2,
            top: closeCenter.dy - closeSize / 2,
            child: GestureDetector(
              onTap: onClose,
              child: const CloseCircle(size: closeSize),
            ),
          ),
        if (showDetails)
          Positioned(
            right: detailsInset,
            top: 0,
            bottom: 0,
            child: Center(
              child: GestureDetector(
                onTap: onDetails,
                child: Container(
                  width: detailsSize.width,
                  height: detailsSize.height,
                  alignment: Alignment.center,
                  decoration: ShapeDecoration(
                    color: colors.surfaceControl,
                    shape: AmalfiSmoothBorder(radius: detailsSize.height / 2),
                  ),
                  child: Text(
                    l10nOf(context).details,
                    style: AmalfiType.label(color: colors.textPrimary),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }

  Widget _content(
    BuildContext context,
    AmalfiColors colors,
    ShellNotification n, {
    required bool withActions,
  }) {
    return Padding(
      padding: EdgeInsets.fromLTRB(
        cardPadding,
        cardPadding,
        showDetails
            ? detailsInset + detailsSize.width + detailsGap
            : cardPadding,
        withActions ? 0 : cardPadding,
      ),
      child: ConstrainedBox(
        // The kit's 52 pt of content (title and two lines of body): 76 pt cards.
        constraints: const BoxConstraints(minHeight: contentHeight),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(width: iconSize, height: iconSize, child: AppIcon(n)),
            const SizedBox(width: textLeft - cardPadding - iconSize),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Expanded(
                        child: Text(
                          n.summary.isEmpty ? n.appName : n.summary,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: AmalfiType.headline(color: colors.textPrimary),
                        ),
                      ),
                      if (showTime) ...[
                        const SizedBox(width: 8),
                        Text(
                          timeLabel(
                            n.timestamp,
                            nowLabel: l10nOf(context).notificationsNow,
                          ),
                          style: AmalfiType.caption(color: colors.textTertiary),
                        ),
                      ],
                    ],
                  ),
                  if (n.body.isNotEmpty)
                    Padding(
                      padding: EdgeInsets.only(
                        right: showTime ? 0 : bodyRight - cardPadding,
                      ),
                      child: Text(
                        n.body,
                        maxLines: bodyLines,
                        overflow: TextOverflow.ellipsis,
                        style: AmalfiType.callout(color: colors.textSecondary),
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _actions(List<(String, String)> buttons) {
    return Padding(
      padding: const EdgeInsets.only(
        left: textLeft,
        top: actionsTop,
        right: cardPadding,
        bottom: cardPadding,
      ),
      // The buttons take the press for themselves: a tap on one must not also activate
      // the card behind it.
      child: GestureDetector(
        onTap: () {},
        child: Wrap(
          spacing: actionsGap,
          runSpacing: actionsGap,
          children: [
            for (final (key, label) in buttons)
              AmalfiPushButton(
                label: label,
                size: AmalfiControlSize.small,
                onPressed: () => onAction?.call(key),
              ),
          ],
        ),
      ),
    );
  }
}

/// The card's shadow, outside the card only: `shadows.2` of Glass/Regular (y 4, blur 12).
void _paintOutsideShadow(Canvas canvas, Size size, Color color) {
  final card = RRect.fromRectAndRadius(
    Offset.zero & size,
    const Radius.circular(cardRadius),
  );
  canvas
    ..save()
    ..clipPath(
      Path.combine(
        PathOperation.difference,
        Path()..addRect((Offset.zero & size).inflate(40)),
        Path()..addRRect(card),
      ),
    )
    ..drawRRect(
      card.shift(const Offset(0, 4)),
      Paint()
        ..color = color
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 6),
    )
    ..restore();
}

/// The app icon (`App Icon/*`, 36 pt): the notification image, the requested icon, the one of
/// the desktop entry, otherwise a tile with the initial.
class AppIcon extends StatelessWidget {
  const AppIcon(this.notification, {super.key});

  final ShellNotification notification;

  @override
  Widget build(BuildContext context) {
    final n = notification;
    final path =
        findIcon(n.imagePath.isNotEmpty ? n.imagePath : null) ??
        findIcon(n.appIcon) ??
        findIcon(n.desktopEntry);
    if (path != null) {
      return path.endsWith('.svg')
          ? SvgPicture.file(File(path))
          : Image.file(File(path));
    }
    final colors = AmalfiTheme.of(context).colors;
    return CustomPaint(
      painter: _InitialIcon(
        n.appName,
        colors.surfaceSunken,
        colors.lineBorder,
        colors.textSecondary,
      ),
    );
  }
}

class _InitialIcon extends CustomPainter {
  const _InitialIcon(this.name, this.fill, this.line, this.text);

  final String name;
  final Color fill;
  final Color line;
  final Color text;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final shape = RRect.fromRectAndRadius(rect, const Radius.circular(8));
    canvas.drawRRect(shape, Paint()..color = fill);
    canvas.drawRRect(
      shape.deflate(0.5),
      Paint()
        ..style = PaintingStyle.stroke
        ..color = line,
    );
    final initial = name.isEmpty ? '' : name.characters.first.toUpperCase();
    final painter = TextPainter(
      text: TextSpan(
        text: initial,
        style: AmalfiType.title3(color: text),
      ),
      textDirection: TextDirection.ltr,
    )..layout();
    painter.paint(
      canvas,
      rect.center - Offset(painter.width / 2, painter.height / 2),
    );
  }

  @override
  bool shouldRepaint(_InitialIcon old) =>
      old.name != name ||
      old.fill != fill ||
      old.line != line ||
      old.text != text;
}

/// The reply field of a notification that takes one (inline-reply; the reference reply was
/// not tried: provisional). Return sends the text; an empty field sends nothing.
class _ReplyField extends StatefulWidget {
  const _ReplyField({
    super.key,
    required this.placeholder,
    required this.onSubmit,
    this.onFocus,
  });

  final String placeholder;
  final ValueChanged<String> onSubmit;
  final ValueChanged<bool>? onFocus;

  @override
  State<_ReplyField> createState() => _ReplyFieldState();
}

class _ReplyFieldState extends State<_ReplyField> {
  final TextEditingController _text = TextEditingController();
  final FocusNode _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    _focus.addListener(() => widget.onFocus?.call(_focus.hasFocus));
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _send(String text) {
    final trimmed = text.trim();
    if (trimmed.isEmpty) return;
    widget.onSubmit(trimmed);
    _text.clear();
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(
      left: textLeft,
      top: actionsTop,
      right: cardPadding,
      bottom: cardPadding,
    ),
    child: LayoutBuilder(
      builder: (context, constraints) => AmalfiTextField(
        controller: _text,
        focusNode: _focus,
        size: AmalfiFieldSize.small,
        width: constraints.maxWidth,
        placeholder: widget.placeholder.isEmpty
            ? l10nOf(context).replyPlaceholder
            : widget.placeholder,
        onSubmitted: _send,
      ),
    ),
  );
}

/// The small round button on a card (close): `Surface/Control` with the x in `Text/Secondary`.
class CloseCircle extends StatelessWidget {
  const CloseCircle({super.key, required this.size});

  final double size;

  @override
  Widget build(BuildContext context) {
    final colors = AmalfiTheme.of(context).colors;
    return DecoratedBox(
      decoration: ShapeDecoration(
        color: colors.surfaceControl,
        shape: CircleBorder(side: BorderSide(color: colors.lineBorder)),
        shadows: amalfiControlShadows(colors),
      ),
      child: CustomPaint(
        size: Size.square(size),
        painter: CrossGlyph(colors.textSecondary, arm: size * 0.2, stroke: 1.5),
      ),
    );
  }
}
