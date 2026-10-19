// Gatekeeper's alerts (silica-desktop docs/spec/trust.md): silica-daemon's Trust1 asks the
// shell to confirm the first launch of an app in quarantine, to say that an app was blocked
// (unsigned, unidentified developer, modified, or not from Flathub) or will damage the computer
// (revoked key), and to suggest moving an app started from a disk image to Applications. The
// shell registers as Trust1's agent and draws the alert here, in the switcher layer. The
// behaviour is the reference's; layout and texts are the Amalfi masters
// `Alerts/Gatekeeper/{Confirm,Blocked,Malware,Disk Image}` (docs/design/amalfi/
// app-installation.md § 1).

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../system/dbus.dart';
import '../system/desktop_entries.dart';
import '../system/icon_lookup.dart';
import '../system/layer_arbiter.dart';
import '../system/layer_control.dart';
import '../system/silica_icons.dart';

/// A prompt of Trust1.
@immutable
class GatekeeperRequest {
  const GatekeeperRequest({
    required this.id,
    required this.verdict,
    required this.info,
  });

  final int id;

  /// `confirm`, `block` or `malware`.
  final String verdict;
  final Map<String, Object?> info;

  String _s(String key) => info[key] is String ? info[key] as String : '';

  String get name => _s('name').isEmpty ? _s('path') : _s('name');
  String get id_ => _s('id');
  String get path => _s('path');
  String get developer => _s('developer');
  String get developerStatus => _s('developer_status');
  String get origin => _s('origin');
  String get agent => _s('agent');
  String get reason => _s('reason');
  String get kind => _s('kind');

  /// From the signal's arguments `(u s a{sv})` or a `Pending` tuple; null if malformed.
  static GatekeeperRequest? fromArgs(List<Object?> args) {
    if (args.length < 3) return null;
    final [id, verdict, info, ...] = args;
    if (id is! int || verdict is! String || info is! Map) return null;
    return GatekeeperRequest(
      id: id,
      verdict: verdict,
      info: info.cast<String, Object?>(),
    );
  }
}

/// Where the app came from, for the Confirm detail: the host of a web origin, the file name
/// of a disk image (a `file://` origin has no host), nothing otherwise.
String? originLine(GatekeeperRequest r) {
  final uri = Uri.tryParse(r.origin);
  if (uri == null || r.origin.isEmpty) return null;
  if (uri.scheme == 'file') {
    final name = uri.pathSegments.where((p) => p.isNotEmpty).lastOrNull;
    return name;
  }
  return uri.host.isEmpty ? null : uri.host;
}

/// The alert's variant (kit `Alerts/Gatekeeper/<Variant>`).
enum GatekeeperVariant { confirm, blocked, malware, diskImage }

GatekeeperVariant variantOf(GatekeeperRequest r) => switch (r.verdict) {
  'confirm' => GatekeeperVariant.confirm,
  'malware' => GatekeeperVariant.malware,
  'volume' => GatekeeperVariant.diskImage,
  _ => GatekeeperVariant.blocked,
};

/// A button of an alert: the choice it answers, its label, its look.
typedef GatekeeperButton = (
  String choice,
  String label,
  AmalfiPushButtonVariant variant,
);

/// The texts of an alert: title, message, detail lines, the buttons left to right (the default
/// one last) and the choice of Escape.
({
  String title,
  String? message,
  List<String> detail,
  List<GatekeeperButton> buttons,
  String escape,
})
gatekeeperTexts(AppLocalizations l10n, GatekeeperRequest r) {
  final name = r.name;
  const secondary = AmalfiPushButtonVariant.secondary;
  const primary = AmalfiPushButtonVariant.primary;
  switch (variantOf(r)) {
    case GatekeeperVariant.confirm:
      final developer = r.kind == 'appimage' || r.developer.isEmpty
          ? l10n.gkDeveloperUnknown
          : r.developerStatus == 'verified'
          ? l10n.gkDeveloper(r.developer)
          : l10n.gkDeveloperSelfSigned(r.developer);
      return (
        title: l10n.gkConfirmTitle(name),
        message: null,
        detail: [?originLine(r), developer],
        // Cancel is the default: the safe answer.
        buttons: [
          ('open', l10n.gkOpen, secondary),
          ('cancel', l10n.cancel, primary),
        ],
        escape: 'cancel',
      );
    case GatekeeperVariant.malware:
      return (
        title: l10n.gkMalwareTitle(name),
        message: l10n.gkMalwareBody,
        detail: const [],
        buttons: [
          ('cancel', l10n.gkDone, secondary),
          ('trash', l10n.gkMoveToTrash, AmalfiPushButtonVariant.destructive),
        ],
        escape: 'cancel',
      );
    case GatekeeperVariant.diskImage:
      return (
        title: l10n.gkVolumeTitle(name),
        message: l10n.gkVolumeBody,
        detail: const [],
        buttons: [
          ('cancel', l10n.gkNotNow, secondary),
          ('move', l10n.gkMoveToApplications, primary),
        ],
        escape: 'cancel',
      );
    case GatekeeperVariant.blocked:
      // One message for every reason (unsigned, self-signed, tampered, invalid, policy).
      return (
        title: l10n.gkBlockTitle(name),
        message: l10n.gkBlockBody(name),
        detail: const [],
        buttons: [
          ('trash', l10n.gkMoveToTrash, secondary),
          ('cancel', l10n.gkDone, primary),
        ],
        escape: 'cancel',
      );
  }
}

/// The measures of the Gatekeeper alerts (kit `Alerts/Gatekeeper/*`).
abstract final class GatekeeperMetrics {
  static const double width = 280;
  static const EdgeInsets padding = EdgeInsets.fromLTRB(20, 24, 20, 20);
  static const double icon = 56;
  static const double gap = 16;
  static const double textGap = 8;
  static const double textBottom = 4;
  static const double buttonGap = 8;
  static const double buttonMin = 72;
  static const double buttonPadding = 24;

  /// The text block, and the widest row of buttons before they stack.
  static const double content = 240;

  /// The critical badge over the icon: a 24 × 20 disc at (40, 26) of the icon, the warning
  /// glyph 16 at (4, 3) in it.
  static const Rect badge = Rect.fromLTWH(40, 26, 24, 20);
  static const double badgeGlyph = 16;
}

/// The width of an alert button whose label is [labelWidth] wide: `max(72, label + 24)` rounded
/// up to 4. The label is first rounded up to the whole point with half a point of margin: the
/// kit measures its text a little wider than the engine does ("Move to Trash" is 88.0 there,
/// 87.5 here), and the masters' widths (72, 80, 116, 156) come out exact.
double gatekeeperButtonWidth(double labelWidth) {
  final raw =
      (labelWidth + 0.5).ceilToDouble() + GatekeeperMetrics.buttonPadding;
  final stepped = (raw / 4).ceil() * 4.0;
  return stepped < GatekeeperMetrics.buttonMin
      ? GatekeeperMetrics.buttonMin
      : stepped;
}

/// Whether two buttons of these widths stack (each 240 wide, the default on top): when the pair
/// is wider than 240.
bool gatekeeperStacks(List<double> widths) =>
    widths.length > 1 &&
    widths.fold<double>(0, (a, w) => a + w) +
            GatekeeperMetrics.buttonGap * (widths.length - 1) >
        GatekeeperMetrics.content;

/// Where the prompts come from: the daemon, or a fake in the tests.
abstract class GatekeeperSource {
  /// Becomes the agent that draws the alerts.
  Future<void> register();
  Future<List<GatekeeperRequest>> pending();
  Stream<GatekeeperRequest> get requested;

  /// Prompt ids that ended (answered or timed out).
  Stream<int> get closed;
  Future<void> answer(int id, String choice);
}

/// `dev.frascella.Silica.Trust1` on the session bus.
class DBusGatekeeper implements GatekeeperSource {
  static const _name = 'dev.frascella.Silica';
  static const _path = '/dev/frascella/Silica/Trust1';
  static const _interface = 'dev.frascella.Silica.Trust1';

  Future<List<Object?>> _call(
    String method, [
    String signature = '',
    List<Object?> args = const [],
  ]) => DBus.instance.call(
    destination: _name,
    path: _path,
    interface: _interface,
    method: method,
    signature: signature,
    args: args,
  );

  @override
  Future<void> register() async {
    try {
      await _call('RegisterAgent');
    } on PlatformException {
      // No daemon: no prompts to draw.
    } on MissingPluginException {
      // Outside Silica.
    }
  }

  @override
  Future<List<GatekeeperRequest>> pending() async {
    try {
      final reply = await _call('Pending');
      final list = reply.isEmpty ? null : reply.first;
      return [
        if (list is List)
          for (final item in list)
            if (item is List) ?GatekeeperRequest.fromArgs(item),
      ];
    } on PlatformException {
      return const [];
    } on MissingPluginException {
      return const [];
    }
  }

  Stream<DBusSignal> _signals(String member) => DBus.instance
      .signals(path: _path, interface: _interface, member: member)
      .handleError((Object _) {});

  @override
  Stream<GatekeeperRequest> get requested => _signals('PromptRequested')
      .map((s) => GatekeeperRequest.fromArgs(s.args))
      .where((r) => r != null)
      .cast<GatekeeperRequest>();

  @override
  Stream<int> get closed => _signals(
    'PromptClosed',
  ).map((s) => s.args.firstOrNull).where((id) => id is int).cast<int>();

  @override
  Future<void> answer(int id, String choice) async {
    try {
      await _call('Answer', 'us', [id, choice]);
    } on PlatformException {
      // Already closed.
    }
  }
}

/// The alerts' layer: shows the oldest waiting prompt.
class GatekeeperLayer extends StatefulWidget {
  const GatekeeperLayer({
    super.key,
    required this.source,
    required this.entries,
  });

  final GatekeeperSource source;
  final Map<String, DesktopEntry> entries;

  @override
  State<GatekeeperLayer> createState() => _GatekeeperLayerState();
}

class _GatekeeperLayerState extends State<GatekeeperLayer> {
  late final LayerArbiter _arbiter;
  final List<GatekeeperRequest> _queue = [];
  StreamSubscription<GatekeeperRequest>? _requested;
  StreamSubscription<int>? _closed;

  @override
  void initState() {
    super.initState();
    _arbiter = LayerArbiter.of(LayerControl.of(context));
    _requested = widget.source.requested.listen(_add);
    _closed = widget.source.closed.listen(_remove);
    unawaited(widget.source.register());
    widget.source.pending().then((list) => list.forEach(_add));
  }

  @override
  void dispose() {
    _arbiter.remove(this);
    _requested?.cancel();
    _closed?.cancel();
    super.dispose();
  }

  void _add(GatekeeperRequest request) {
    if (!mounted || _queue.any((r) => r.id == request.id)) return;
    setState(() => _queue.add(request));
    _arbitrate();
  }

  void _remove(int id) {
    if (!mounted) return;
    setState(() => _queue.removeWhere((r) => r.id == id));
    _arbitrate();
  }

  void _arbitrate() {
    _arbiter.set(
      this,
      _queue.isEmpty ? const LayerWish.none() : const LayerWish.all(),
    );
  }

  IconSource? _icon(GatekeeperRequest r) {
    final entry = widget.entries[r.id_];
    if (entry == null) return null;
    return switch ((silicaIconFor(entry), findIcon(entry.icon))) {
      (final LayeredIcon l, _) => LayeredSource(l),
      (null, final String path) => FlatSource(path),
      _ => null,
    };
  }

  @override
  Widget build(BuildContext context) {
    final request = _queue.firstOrNull;
    if (request == null) return const SizedBox.shrink();
    return Stack(
      children: [
        const Positioned.fill(child: AbsorbPointer()),
        Align(
          alignment: const Alignment(0, -0.45),
          child: GatekeeperAlert(
            key: ValueKey('gatekeeper ${request.id}'),
            request: request,
            icon: _icon(request),
            onChoice: (choice) {
              widget.source.answer(request.id, choice);
              _remove(request.id);
            },
          ),
        ),
      ],
    );
  }
}

/// The alert itself.
class GatekeeperAlert extends StatelessWidget {
  const GatekeeperAlert({
    super.key,
    required this.request,
    required this.onChoice,
    this.icon,
  });

  final GatekeeperRequest request;
  final IconSource? icon;
  final void Function(String choice) onChoice;

  double _labelWidth(BuildContext context, String label) {
    final painter = TextPainter(
      text: TextSpan(
        text: label,
        style: AmalfiPushButtonMetrics.labelStyle(
          AmalfiControlSize.regular,
          const Color(0xFF000000),
        ),
      ),
      textDirection: TextDirection.ltr,
      textScaler: MediaQuery.textScalerOf(context),
      maxLines: 1,
    )..layout();
    final width = painter.width;
    painter.dispose();
    return width;
  }

  @override
  Widget build(BuildContext context) {
    final l10n = l10nOf(context);
    final colors = AmalfiTheme.of(context).colors;
    final texts = gatekeeperTexts(l10n, request);
    final variant = variantOf(request);
    final critical =
        variant == GatekeeperVariant.blocked ||
        variant == GatekeeperVariant.malware;
    final defaultChoice = texts.buttons.last.$1;
    final widths = [
      for (final b in texts.buttons)
        gatekeeperButtonWidth(_labelWidth(context, b.$2)),
    ];
    final stacked = gatekeeperStacks(widths);
    Widget button(int i, double width) {
      final (choice, label, look) = texts.buttons[i];
      return SizedBox(
        width: width,
        child: AmalfiPushButton(
          key: ValueKey('gatekeeper $choice'),
          label: label,
          variant: look,
          onPressed: () => onChoice(choice),
        ),
      );
    }

    final buttons = stacked
        ? Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              // The default on top.
              for (var i = texts.buttons.length - 1; i >= 0; i--) ...[
                if (i < texts.buttons.length - 1)
                  const SizedBox(height: GatekeeperMetrics.buttonGap),
                button(i, GatekeeperMetrics.content),
              ],
            ],
          )
        : Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (var i = 0; i < texts.buttons.length; i++) ...[
                if (i > 0) const SizedBox(width: GatekeeperMetrics.buttonGap),
                button(i, widths[i]),
              ],
            ],
          );
    final iconBox = SizedBox.square(
      dimension: GatekeeperMetrics.icon,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Positioned.fill(
            child: switch (icon) {
              final icon? => SilicaIcon(
                icon,
                side: GatekeeperMetrics.icon,
                dark: AmalfiTheme.of(context).isDark,
                besideApps: true,
              ),
              null => Center(
                child: AmalfiGlyph(
                  AmalfiGlyphs.lock,
                  size: 40,
                  color: colors.textSecondary,
                ),
              ),
            },
          ),
          if (critical)
            Positioned.fromRect(
              key: const ValueKey('gatekeeper badge'),
              rect: GatekeeperMetrics.badge,
              child: AmalfiAlertSurface(
                radius: GatekeeperMetrics.badge.height / 2,
                shadow: false,
                child: Center(
                  child: AmalfiGlyph(
                    AmalfiGlyphs.warning,
                    size: GatekeeperMetrics.badgeGlyph,
                    color: colors.paletteTerracotta,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
    final message = texts.message;
    final textBlock = Padding(
      key: const ValueKey('gatekeeper text'),
      padding: const EdgeInsets.only(bottom: GatekeeperMetrics.textBottom),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Semantics(
            header: true,
            child: Text(
              texts.title,
              key: const ValueKey('gatekeeper title'),
              style: AmalfiType.title3(color: colors.textPrimary),
            ),
          ),
          if (message != null) ...[
            const SizedBox(height: GatekeeperMetrics.textGap),
            Text(
              message,
              key: const ValueKey('gatekeeper message'),
              style: AmalfiType.callout(color: colors.textSecondary),
            ),
          ],
          if (texts.detail.isNotEmpty) ...[
            const SizedBox(height: GatekeeperMetrics.textGap),
            Text(
              texts.detail.join('\n'),
              key: const ValueKey('gatekeeper detail'),
              style: AmalfiType.callout(color: colors.textTertiary),
            ),
          ],
        ],
      ),
    );
    return Focus(
      autofocus: true,
      onKeyEvent: (node, event) {
        if (event is! KeyDownEvent) return KeyEventResult.ignored;
        if (event.logicalKey == LogicalKeyboardKey.escape) {
          onChoice(texts.escape);
          return KeyEventResult.handled;
        }
        if (event.logicalKey == LogicalKeyboardKey.enter ||
            event.logicalKey == LogicalKeyboardKey.numpadEnter) {
          onChoice(defaultChoice);
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Semantics(
        scopesRoute: true,
        namesRoute: true,
        explicitChildNodes: true,
        label: texts.title,
        child: SizedBox(
          key: const ValueKey('gatekeeper alert'),
          width: GatekeeperMetrics.width,
          child: AmalfiAlertSurface(
            radius: AmalfiAlertMetrics.radius,
            child: Padding(
              padding: GatekeeperMetrics.padding,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Center(child: iconBox),
                  const SizedBox(height: GatekeeperMetrics.gap),
                  textBlock,
                  const SizedBox(height: GatekeeperMetrics.gap),
                  Center(child: buttons),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
