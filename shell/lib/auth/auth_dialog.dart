// The system authentication dialog: silica-daemon's polkit agent asks for an administrator's
// password (docs/daemon.md › Authentication) and the shell shows it here, in the switcher
// layer, as an Amalfi system alert (kit `Alerts/Alert`, glass variant): 280 wide, radius 20,
// Glass/Thick (Ossido draws it: `sheet` kind), the app icon (56) centred at the top, the title in
// Title 3 and the explanation in Callout, a user name field and a secure field, and the
// Cancel and Allow push buttons (Allow is the default one). A wrong password shakes the card and
// clears the field. The texts are our own.

import 'dart:async';
import 'dart:math' as math;

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

const double authDialogWidth = AmalfiAlertMetrics.width;
const Duration authShake = Duration(milliseconds: 420);

/// A request from the polkit agent.
@immutable
class AuthRequest {
  const AuthRequest({
    required this.id,
    required this.action,
    required this.message,
    required this.app,
    required this.users,
    required this.defaultUser,
  });

  final int id;
  final String action;

  /// polkit's own explanation, already in the session's language.
  final String message;

  /// The executable of the app that asked (empty: unknown).
  final String app;

  /// Who can answer: login name → full name.
  final Map<String, String> users;
  final String defaultUser;

  /// From the D-Bus tuple `(ussssa(ss)s)`; null if malformed.
  static AuthRequest? fromDBus(Object? value) {
    if (value is! List || value.length < 7) return null;
    final [id, action, message, _, app, users, user, ...] = value;
    if (id is! int || users is! List) return null;
    return AuthRequest(
      id: id,
      action: '$action',
      message: '$message',
      app: '$app',
      users: {
        for (final u in users)
          if (u is List && u.length >= 2) '${u[0]}': '${u[1]}',
      },
      defaultUser: '$user',
    );
  }
}

/// Where the requests come from: the daemon, or a fake in the tests.
abstract class AuthSource {
  Future<List<AuthRequest>> pending();
  Stream<AuthRequest> get requested;

  /// Request ids that ended (answered, cancelled, or withdrawn by polkit).
  Stream<int> get closed;

  /// True if the password is right; false to try again.
  Future<bool> respond(int id, String user, String password);
  Future<void> cancel(int id);
}

/// `dev.frascella.Silica.Authentication1` on the session bus.
class DBusAuth implements AuthSource {
  static const _name = 'dev.frascella.Silica';
  static const _path = '/dev/frascella/Silica/Authentication1';
  static const _interface = 'dev.frascella.Silica.Authentication1';

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
  Future<List<AuthRequest>> pending() async {
    try {
      final reply = await _call('Pending');
      final list = reply.isEmpty ? null : reply.first;
      return [
        if (list is List)
          for (final item in list) ?AuthRequest.fromDBus(item),
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
  Stream<AuthRequest> get requested => _signals('Requested')
      .map((s) => AuthRequest.fromDBus(s.args.firstOrNull))
      .where((r) => r != null)
      .cast<AuthRequest>();

  @override
  Stream<int> get closed => _signals(
    'Closed',
  ).map((s) => s.args.firstOrNull).where((id) => id is int).cast<int>();

  @override
  Future<bool> respond(int id, String user, String password) async {
    final reply = await _call('Respond', 'uss', [id, user, password]);
    return reply.firstOrNull == true;
  }

  @override
  Future<void> cancel(int id) async {
    try {
      await _call('Cancel', 'u', [id]);
    } on PlatformException {
      // Already closed.
    }
  }
}

/// The dialog's layer: shows the oldest waiting request.
class AuthLayer extends StatefulWidget {
  const AuthLayer({super.key, required this.source, required this.entries});

  final AuthSource source;
  final Map<String, DesktopEntry> entries;

  @override
  State<AuthLayer> createState() => _AuthLayerState();
}

class _AuthLayerState extends State<AuthLayer> {
  /// The arbiter of this component's layer surface (#201).
  late final LayerArbiter _arbiter;

  final List<AuthRequest> _queue = [];
  StreamSubscription<AuthRequest>? _requested;
  StreamSubscription<int>? _closed;

  @override
  void initState() {
    super.initState();
    _arbiter = LayerArbiter.of(LayerControl.of(context));
    _requested = widget.source.requested.listen(_add);
    _closed = widget.source.closed.listen(_remove);
    widget.source.pending().then((list) => list.forEach(_add));
  }

  @override
  void dispose() {
    _arbiter.remove(this);
    _requested?.cancel();
    _closed?.cancel();
    super.dispose();
  }

  void _add(AuthRequest request) {
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

  /// The app's name and icon from its desktop entry: the executable is the entry's command.
  DesktopEntry? _entry(String app) {
    if (app.isEmpty) return null;
    for (final entry in widget.entries.values) {
      final command = entry.exec.split(' ').first.split('/').last;
      if (command == app || entry.matchesAppId(app)) return entry;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final request = _queue.firstOrNull;
    if (request == null) return const SizedBox.shrink();
    final entry = _entry(request.app);
    return Stack(
      children: [
        // The rest of the screen stays visible; clicks outside do nothing.
        const Positioned.fill(child: AbsorbPointer()),
        Align(
          alignment: const Alignment(0, -0.45),
          child: AuthDialog(
            key: ValueKey('auth ${request.id}'),
            request: request,
            appName: entry?.name,
            icon: switch (entry) {
              null => null,
              final e => switch ((silicaIconFor(e), findIcon(e.icon))) {
                (final LayeredIcon l, _) => LayeredSource(l),
                (null, final String path) => FlatSource(path),
                _ => null,
              },
            },
            onRespond: (user, password) =>
                widget.source.respond(request.id, user, password),
            onCancel: () {
              widget.source.cancel(request.id);
              _remove(request.id);
            },
            onDone: () => _remove(request.id),
          ),
        ),
      ],
    );
  }
}

/// The card itself.
class AuthDialog extends StatefulWidget {
  const AuthDialog({
    super.key,
    required this.request,
    required this.onRespond,
    required this.onCancel,
    required this.onDone,
    this.appName,
    this.icon,
  });

  final AuthRequest request;
  final String? appName;
  final IconSource? icon;
  final Future<bool> Function(String user, String password) onRespond;
  final VoidCallback onCancel;

  /// The password was right.
  final VoidCallback onDone;

  @override
  State<AuthDialog> createState() => AuthDialogState();
}

class AuthDialogState extends State<AuthDialog>
    with SingleTickerProviderStateMixin {
  late final TextEditingController _user = TextEditingController(
    text: widget.request.defaultUser,
  );
  final TextEditingController _password = TextEditingController();
  final FocusNode _userFocus = FocusNode();
  final FocusNode _passwordFocus = FocusNode();
  late final AnimationController _shake = AnimationController(
    vsync: this,
    duration: authShake,
  );
  bool _busy = false;

  /// The last attempt failed for a reason other than the password (the helper is missing).
  bool _error = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _passwordFocus.requestFocus();
    });
  }

  @override
  void dispose() {
    _user.dispose();
    _password.dispose();
    _userFocus.dispose();
    _passwordFocus.dispose();
    _shake.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (_busy) return;
    final user = _user.text.trim();
    if (!widget.request.users.containsKey(user)) {
      _wrong();
      return;
    }
    setState(() {
      _busy = true;
      _error = false;
    });
    bool right;
    try {
      right = await widget.onRespond(user, _password.text);
    } on PlatformException {
      right = false;
      _error = true;
    }
    if (!mounted) return;
    setState(() => _busy = false);
    if (right) {
      widget.onDone();
    } else {
      _wrong();
    }
  }

  void _wrong() {
    _password.clear();
    _passwordFocus.requestFocus();
    _shake.forward(from: 0);
  }

  /// polkit's explanation as a sentence: it comes without a full stop.
  static String _sentence(String text) =>
      RegExp(r'[.!?…]$').hasMatch(text) ? text : '$text.';

  @override
  Widget build(BuildContext context) {
    final l10n = l10nOf(context);
    final colors = AmalfiTheme.of(context).colors;
    final title = switch (widget.appName) {
      final name? => l10n.authTitleApp(name),
      null => l10n.authTitle,
    };
    final message = widget.request.message.trim();
    return AnimatedBuilder(
      animation: _shake,
      builder: (context, child) {
        // Three damped swings, 10 pt at most.
        final t = _shake.value;
        final dx = t == 0 || t == 1
            ? 0.0
            : math.sin(t * math.pi * 6) * 10 * (1 - t);
        return Transform.translate(offset: Offset(dx, 0), child: child);
      },
      child: Focus(
        onKeyEvent: (node, event) {
          if (event is! KeyDownEvent) return KeyEventResult.ignored;
          if (event.logicalKey == LogicalKeyboardKey.escape) {
            widget.onCancel();
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        // The alert's layout (`AmalfiAlert`) on an `AmalfiGlass` of kind `sheet`: under the
        // layer's compositor backend Ossido draws the glass and the shadow.
        child: SizedBox(
          width: authDialogWidth,
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
                      child: switch (widget.icon) {
                        final icon? => SilicaIcon(
                          icon,
                          side: AmalfiAlertMetrics.icon,
                          dark: false,
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
                  ),
                  const SizedBox(height: AmalfiAlertMetrics.gap),
                  // The binding alert anatomy (components.md "Alerts (binding)"): centred
                  // text, title → message 8, the default button first, one per row.
                  Text(
                    title,
                    key: const ValueKey('auth title'),
                    textAlign: TextAlign.center,
                    style: AmalfiType.title3(color: colors.textPrimary),
                  ),
                  const SizedBox(height: 8),
                  Text(
                    [
                      if (message.isNotEmpty) _sentence(message),
                      _error ? l10n.authUnavailable : l10n.authPrompt,
                    ].join(' '),
                    textAlign: TextAlign.center,
                    style: AmalfiType.callout(color: colors.textSecondary),
                  ),
                  const SizedBox(height: AmalfiAlertMetrics.gap),
                  AmalfiTextField(
                    key: const ValueKey('auth user'),
                    controller: _user,
                    focusNode: _userFocus,
                    placeholder: l10n.authUserName,
                    width: double.infinity,
                    onSubmitted: (_) => _passwordFocus.requestFocus(),
                  ),
                  const SizedBox(height: 8),
                  AmalfiSecureField(
                    key: const ValueKey('auth password'),
                    controller: _password,
                    focusNode: _passwordFocus,
                    placeholder: l10n.authPassword,
                    width: double.infinity,
                    onSubmitted: (_) => _submit(),
                  ),
                  const SizedBox(height: 20),
                  SizedBox(
                    width: 240,
                    child: AmalfiPushButton(
                      key: const ValueKey('auth allow'),
                      label: l10n.authAllow,
                      variant: AmalfiPushButtonVariant.primary,
                      onPressed: _busy ? null : _submit,
                    ),
                  ),
                  const SizedBox(height: AmalfiAlertMetrics.buttonGap),
                  SizedBox(
                    width: 240,
                    child: AmalfiPushButton(
                      key: const ValueKey('auth cancel'),
                      label: l10n.cancel,
                      onPressed: widget.onCancel,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
