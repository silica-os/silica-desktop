// The lock screen and the screen saver.
//
// The lock screen is the shell's client of ext-session-lock-v1: Ossido announces `session.locked`
// (`1`) and [LockController] (in the `screensaver` layer) asks the embedder for the session lock
// (`silica/lock`, `lock`); once Ossido grants it, the embedder gives the `lock` view a lock
// surface and [LockScreen] draws in it: the wallpaper under a veil, the time and date, and the
// user with a password field. The password goes to silica-daemon (`System1.Unlock`, which only
// checks it); on a right one the embedder unlocks (`silica/lock`, `unlock`). While locked,
// Ossido draws and feeds nothing else; if this process dies the session stays locked and the
// shell, started again, takes the lock over. A wrong password shakes the field and clears it.
//
// The screen saver (`screensaver.active`: the id of the saver, empty when none) draws in an
// ordinary overlay layer over the desktop and ends with any input. The design is ours: the
// reference did not show its lock screen (settings docs/spec/lock-screen.md).

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../system/dbus.dart';
import '../system/layer_arbiter.dart';
import '../system/layer_control.dart';
import '../system/shell_channel.dart';
import '../wallpaper/wallpaper.dart';
import 'savers.dart';

const Duration lockShake = Duration(milliseconds: 420);

Future<void>? _dates;

/// The user whose password unlocks, and where the password is checked.
abstract class LockSource {
  /// The login name and the full name (the GECOS name, otherwise the login).
  ({String login, String name}) get user;

  /// True if the password was right and the session is unlocked; false to try again. Throws
  /// when the password cannot be checked at all.
  Future<bool> unlock(String password);
}

/// `dev.frascella.Silica.System1` on the session bus.
class DBusLock implements LockSource {
  @override
  ({String login, String name}) get user => currentUser();

  @override
  Future<bool> unlock(String password) async {
    final reply = await DBus.instance.call(
      destination: 'dev.frascella.Silica',
      path: '/dev/frascella/Silica/System1',
      interface: 'dev.frascella.Silica.System1',
      method: 'Unlock',
      signature: 's',
      args: [password],
    );
    // Right: the lock is let go here, in the process that holds it.
    return reply.firstOrNull == true && await LockControl.release();
  }
}

/// The embedder's session lock (`silica/lock`): this process holds it for the compositor.
abstract final class LockControl {
  static const MethodChannel channel = MethodChannel(
    'silica/lock',
    JSONMethodCodec(),
  );

  static Future<bool> _call(String method) async {
    try {
      return await channel.invokeMethod<bool>(method) ?? false;
    } on MissingPluginException {
      return false;
    } on PlatformException {
      return false;
    }
  }

  /// Takes the session lock Ossido has asked for (true: held or asked for).
  static Future<bool> take() => _call('lock');

  /// Ends the lock; only after the password was right.
  static Future<bool> release() => _call('unlock');
}

/// The logged-in user from the environment and `/etc/passwd`.
({String login, String name}) currentUser({
  Map<String, String>? env,
  String passwd = '/etc/passwd',
}) {
  final e = env ?? Platform.environment;
  final login = e['USER'] ?? e['LOGNAME'] ?? '';
  var name = login;
  try {
    for (final line in const LineSplitter().convert(
      File(passwd).readAsStringSync(),
    )) {
      final f = line.split(':');
      if (f.length > 4 && f.first == login) {
        final full = f[4].split(',').first.trim();
        if (full.isNotEmpty) name = full;
        break;
      }
    }
  } on FileSystemException {
    // The login name stays.
  }
  return (login: login, name: name);
}

/// Whether the lock screen shows the user's name and picture (lockscreen.json `showUser`).
bool readShowUser({Map<String, String>? env}) {
  final e = env ?? Platform.environment;
  final base = e['XDG_CONFIG_HOME'] ?? '${e['HOME'] ?? ''}/.config';
  try {
    final json = jsonDecode(
      File('$base/silica/lockscreen.json').readAsStringSync(),
    );
    return json is Map && json['showUser'] is bool
        ? json['showUser'] as bool
        : true;
  } on FileSystemException {
    return true;
  } on FormatException {
    return true;
  }
}

/// Takes the session lock whenever Ossido says the session is locked: at once, again every few
/// seconds while it is (the lock is idempotent), and at the start of a shell that finds the
/// session locked (a locker that died).
class LockController extends StatefulWidget {
  const LockController({
    super.key,
    this.take = LockControl.take,
    this.every = const Duration(seconds: 3),
    required this.child,
  });

  final Future<bool> Function() take;
  final Duration every;
  final Widget child;

  @override
  State<LockController> createState() => _LockControllerState();
}

class _LockControllerState extends State<LockController> {
  Timer? _retry;

  @override
  void initState() {
    super.initState();
    ShellChannel.instance.states.addListener(_changed);
    _changed();
  }

  @override
  void dispose() {
    ShellChannel.instance.states.removeListener(_changed);
    _retry?.cancel();
    super.dispose();
  }

  void _changed() {
    final locked = ShellChannel.instance.states.value['session.locked'] == '1';
    if (!locked) {
      _retry?.cancel();
      _retry = null;
      return;
    }
    if (_retry != null) return;
    unawaited(widget.take());
    _retry = Timer.periodic(widget.every, (_) => unawaited(widget.take()));
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// The screen saver layer: nothing until Ossido says a saver is active (and the lock
/// controller that rides on this layer).
class ScreenSaverLayer extends StatefulWidget {
  const ScreenSaverLayer({super.key, this.now, this.take = LockControl.take});

  /// The clock; the system's by default (tests).
  final DateTime Function()? now;
  final Future<bool> Function() take;

  @override
  State<ScreenSaverLayer> createState() => _ScreenSaverLayerState();
}

class _ScreenSaverLayerState extends State<ScreenSaverLayer> {
  late final LayerArbiter _arbiter;
  String _saver = '';

  @override
  void initState() {
    super.initState();
    _arbiter = LayerArbiter.of(LayerControl.of(context));
    ShellChannel.instance.states.addListener(_changed);
    _changed();
  }

  @override
  void dispose() {
    ShellChannel.instance.states.removeListener(_changed);
    _arbiter.remove(this);
    super.dispose();
  }

  void _changed() {
    final saver =
        ShellChannel.instance.states.value['screensaver.active'] ?? '';
    if (saver == _saver) return;
    setState(() => _saver = saver);
    // The saver only looks: the first input ends it (Ossido) and reaches whatever is below.
    _arbiter.set(
      this,
      _saver.isEmpty ? const LayerWish.none() : const LayerWish.rects([]),
    );
  }

  @override
  Widget build(BuildContext context) {
    final saver = screenSaver(_saver, now: widget.now);
    return LockController(
      take: widget.take,
      child: saver == null
          ? const SizedBox.shrink()
          : KeyedSubtree(key: ValueKey('saver $_saver'), child: saver),
    );
  }
}

/// The locked screen itself.
class LockScreen extends StatefulWidget {
  const LockScreen({
    super.key,
    required this.source,
    this.showUser = true,
    this.now,
  });

  final LockSource source;
  final bool showUser;
  final DateTime Function()? now;

  @override
  State<LockScreen> createState() => LockScreenState();
}

class LockScreenState extends State<LockScreen>
    with SingleTickerProviderStateMixin {
  final TextEditingController _password = TextEditingController();
  final FocusNode _focus = FocusNode();
  late final AnimationController _shake = AnimationController(
    vsync: this,
    duration: lockShake,
  );
  Timer? _tick;
  bool _busy = false;

  /// The last try failed: the password was wrong (false) or could not be checked (true).
  bool? _failed;

  @override
  void initState() {
    super.initState();
    _tick = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted) setState(() {});
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _focus.requestFocus();
    });
  }

  @override
  void dispose() {
    _tick?.cancel();
    _password.dispose();
    _focus.dispose();
    _shake.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (_busy || _password.text.isEmpty) return;
    setState(() {
      _busy = true;
      _failed = null;
    });
    var right = false;
    var unavailable = false;
    try {
      right = await widget.source.unlock(_password.text);
    } on PlatformException {
      unavailable = true;
    }
    if (!mounted) return;
    setState(() {
      _busy = false;
      if (!right) _failed = unavailable;
    });
    if (!right) {
      _password.clear();
      _focus.requestFocus();
      _shake.forward(from: 0);
    }
    // Right: Ossido announces `session.locked` 0 and the layer drops this screen.
  }

  @override
  Widget build(BuildContext context) {
    final l = l10nOf(context);
    final colors = AmalfiColors.dark;
    final now = (widget.now ?? DateTime.now)();
    _dates ??= initializeDateFormatting();
    final locale = l.localeName;
    final time = DateFormat.Hm(locale).format(now);
    final date = DateFormat.MMMMEEEEd(locale).format(now);
    final user = widget.source.user;
    return AmalfiTheme(
      brightness: Brightness.dark,
      child: Stack(
        fit: StackFit.expand,
        children: [
          const WallpaperLayer(),
          // The veil: the wallpaper blurred and darkened, so that nothing under is legible.
          Positioned.fill(
            child: BackdropFilter(
              filter: ui.ImageFilter.blur(sigmaX: 24, sigmaY: 24),
              child: const ColoredBox(color: Color(0x80000000)),
            ),
          ),
          Align(
            alignment: const Alignment(0, -0.62),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  date,
                  key: const ValueKey('lock date'),
                  style: AmalfiType.title3(color: colors.textSecondary),
                ),
                Text(
                  time,
                  key: const ValueKey('lock time'),
                  style: AmalfiType.numeral104(color: colors.textPrimary),
                ),
              ],
            ),
          ),
          Align(
            alignment: const Alignment(0, 0.35),
            child: AnimatedBuilder(
              animation: _shake,
              builder: (context, child) {
                final t = _shake.value;
                final dx = t == 0 || t == 1
                    ? 0.0
                    : math.sin(t * math.pi * 6) * 10 * (1 - t);
                return Transform.translate(offset: Offset(dx, 0), child: child);
              },
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (widget.showUser) ...[
                    _Avatar(name: user.name),
                    const SizedBox(height: 10),
                    Text(
                      user.name,
                      key: const ValueKey('lock user'),
                      style: AmalfiType.title3(color: colors.textPrimary),
                    ),
                  ] else
                    AmalfiGlyph(
                      AmalfiGlyphs.lock,
                      size: 32,
                      color: colors.textSecondary,
                    ),
                  const SizedBox(height: 14),
                  AmalfiSecureField(
                    key: const ValueKey('lock password'),
                    controller: _password,
                    focusNode: _focus,
                    placeholder: l.lockPassword,
                    width: 220,
                    onSubmitted: (_) {
                      // The editing action unfocuses the field (EditableText's default for
                      // "done"): take the focus back at once, the check runs with it.
                      _focus.requestFocus();
                      _submit();
                    },
                  ),
                  const SizedBox(height: 10),
                  SizedBox(
                    height: 18,
                    child: Text(
                      switch (_failed) {
                        null => '',
                        false => l.lockIncorrect,
                        true => l.lockUnavailable,
                      },
                      key: const ValueKey('lock message'),
                      style: AmalfiType.callout(color: colors.textSecondary),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// The user's picture: no pictures are kept yet, so a disc with the initial.
class _Avatar extends StatelessWidget {
  const _Avatar({required this.name});

  final String name;

  @override
  Widget build(BuildContext context) {
    final colors = AmalfiColors.dark;
    final initial = name.isEmpty ? '' : String.fromCharCode(name.runes.first);
    return Container(
      width: 72,
      height: 72,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: colors.paletteLavanda.withValues(alpha: 0.85),
      ),
      child: Text(
        initial.toUpperCase(),
        style: AmalfiType.title1(color: colors.textOnAccent),
      ),
    );
  }
}
