// Gatekeeper's alerts (silica-desktop docs/spec/trust.md): silica-daemon's Trust1 asks the
// shell to confirm the first launch of an app in quarantine, or to say that an app was blocked
// (unsigned, unidentified developer, modified) or will damage the computer (revoked key). The
// shell registers as Trust1's agent and draws the alert here, in the switcher layer, with the
// layout of the system alert (as the authentication dialog). The behaviour is the reference's;
// the texts are our own.
//
// PROVISIONAL(amalfi-gatekeeper): layout and buttons follow the authentication dialog's alert
// until the Amalfi masters of the Gatekeeper alerts arrive from the design session.

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

/// What the alert says about where the app came from: a disk image, a website, an app.
String? originLine(AppLocalizations l10n, GatekeeperRequest r) {
  final origin = r.origin;
  if (origin.startsWith('file://') && origin.endsWith('.sxv')) {
    final name = Uri.decodeComponent(origin.split('/').last);
    return l10n.gkFromImage(name.substring(0, name.length - '.sxv'.length));
  }
  final uri = Uri.tryParse(origin);
  if (uri != null && (uri.scheme == 'https' || uri.scheme == 'http')) {
    return l10n.gkFromSite(uri.host);
  }
  if (r.agent.isNotEmpty) return l10n.gkFromApp(r.agent);
  return null;
}

/// The texts of an alert: title, body lines, and the buttons as (choice, label, primary).
({String title, List<String> body, List<(String, String, bool)> buttons})
gatekeeperTexts(AppLocalizations l10n, GatekeeperRequest r) {
  final name = r.name;
  final developer = r.developer.isEmpty
      ? null
      : r.developerStatus == 'verified'
      ? l10n.gkDeveloperVerified(r.developer)
      : l10n.gkDeveloperSelfSigned(r.developer);
  switch (r.verdict) {
    case 'confirm':
      return (
        title: l10n.gkConfirmTitle(name),
        body: [?originLine(l10n, r), ?developer],
        // Cancel is the default: the safe answer (the reference's order).
        buttons: [('cancel', l10n.cancel, true), ('open', l10n.gkOpen, false)],
      );
    case 'malware':
      return (
        title: l10n.gkMalwareTitle(name),
        body: [l10n.gkMalwareBody],
        buttons: [
          ('cancel', l10n.gkDone, false),
          ('trash', l10n.gkMoveToTrash, true),
        ],
      );
    default:
      final why = switch (r.reason) {
        'unsigned' => l10n.gkBlockUnsigned(name),
        'self-signed' => l10n.gkBlockSelfSigned(name),
        'tampered' => l10n.gkBlockTampered(name),
        _ => l10n.gkBlockInvalid(name),
      };
      final canOverride = r.reason == 'unsigned' || r.reason == 'self-signed';
      return (
        title: l10n.gkBlockTitle(name),
        body: [why, if (canOverride) l10n.gkBlockHint],
        buttons: [
          ('trash', l10n.gkMoveToTrash, false),
          ('cancel', l10n.gkDone, true),
        ],
      );
  }
}

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

  @override
  Widget build(BuildContext context) {
    final l10n = l10nOf(context);
    final colors = AmalfiTheme.of(context).colors;
    final texts = gatekeeperTexts(l10n, request);
    final primary = texts.buttons.firstWhere((b) => b.$3).$1;
    return Focus(
      autofocus: true,
      onKeyEvent: (node, event) {
        if (event is! KeyDownEvent) return KeyEventResult.ignored;
        if (event.logicalKey == LogicalKeyboardKey.escape) {
          onChoice('cancel');
          return KeyEventResult.handled;
        }
        if (event.logicalKey == LogicalKeyboardKey.enter ||
            event.logicalKey == LogicalKeyboardKey.numpadEnter) {
          onChoice(primary);
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: SizedBox(
        width: AmalfiAlertMetrics.width,
        child: AmalfiGlass(
          material: AmalfiMaterials.thick,
          radius: AmalfiAlertMetrics.radius,
          kind: AmalfiGlassKind.sheet,
          child: Padding(
            padding: AmalfiAlertMetrics.padding,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: [
                Center(
                  child: SizedBox.square(
                    dimension: AmalfiAlertMetrics.icon,
                    child: switch (icon) {
                      final icon? => SilicaIcon(
                        icon,
                        side: AmalfiAlertMetrics.icon,
                        dark: false,
                        besideApps: true,
                      ),
                      null => Center(
                        child: AmalfiGlyph(
                          request.verdict == 'confirm'
                              ? AmalfiGlyphs.lock
                              : AmalfiGlyphs.warning,
                          size: 40,
                          color: colors.textSecondary,
                        ),
                      ),
                    },
                  ),
                ),
                const SizedBox(height: AmalfiAlertMetrics.gap),
                Text(
                  texts.title,
                  key: const ValueKey('gatekeeper title'),
                  textAlign: TextAlign.center,
                  style: AmalfiType.title3(color: colors.textPrimary),
                ),
                for (final line in texts.body) ...[
                  const SizedBox(height: AmalfiAlertMetrics.gap),
                  Text(
                    line,
                    textAlign: TextAlign.center,
                    style: AmalfiType.callout(color: colors.textSecondary),
                  ),
                ],
                const SizedBox(height: AmalfiAlertMetrics.gap),
                Center(
                  child: Wrap(
                    alignment: WrapAlignment.center,
                    spacing: AmalfiAlertMetrics.buttonGap,
                    runSpacing: AmalfiAlertMetrics.buttonGap,
                    children: [
                      for (final (choice, label, isPrimary) in texts.buttons)
                        ConstrainedBox(
                          constraints: const BoxConstraints(
                            minWidth: AmalfiAlertMetrics.buttonMinWidth,
                          ),
                          child: AmalfiPushButton(
                            key: ValueKey('gatekeeper $choice'),
                            label: label,
                            variant: isPrimary && choice == 'trash'
                                ? AmalfiPushButtonVariant.destructive
                                : isPrimary
                                ? AmalfiPushButtonVariant.primary
                                : AmalfiPushButtonVariant.secondary,
                            onPressed: () => onChoice(choice),
                          ),
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
