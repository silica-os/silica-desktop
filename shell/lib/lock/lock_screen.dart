// The lock screen and the screen saver, one shell component (the `lock` layer, an overlay).
//
// Ossido announces the states: `session.locked` (`1` while the session is locked: windows get
// no input) and `screensaver.active` (the id of the saver, empty when none). Locked, the layer
// takes the pointer and the keyboard and shows the wallpaper under a veil, the time and date,
// and the user with a password field; the password goes to silica-daemon
// (`System1.Unlock`), which checks it and tells Ossido to unlock. A wrong password shakes the
// field and clears it. The screen saver draws over the desktop and ends with any input. The
// design is ours: the reference did not show its lock screen (settings docs/spec/lock-screen.md).

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
    return reply.firstOrNull == true;
  }
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

/// The component: nothing until the session is locked or the screen saver starts.
class LockLayer extends StatefulWidget {
  const LockLayer({super.key, required this.source, this.now});

  final LockSource source;

  /// The clock; the system's by default (tests).
  final DateTime Function()? now;

  @override
  State<LockLayer> createState() => _LockLayerState();
}

class _LockLayerState extends State<LockLayer> {
  late final LayerArbiter _arbiter;
  bool _locked = false;
  String _saver = '';
  bool _showUser = true;

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
    final states = ShellChannel.instance.states.value;
    final locked = states['session.locked'] == '1';
    final saver = states['screensaver.active'] ?? '';
    if (locked == _locked && saver == _saver) return;
    if (locked && !_locked) _showUser = readShowUser();
    setState(() {
      _locked = locked;
      _saver = saver;
    });
    // Locked: the whole surface and the keyboard. The saver only looks: the first input ends it
    // (Ossido) and reaches whatever is below.
    _arbiter.set(
      this,
      _locked
          ? const LayerWish.all()
          : _saver.isNotEmpty
          ? const LayerWish.rects([])
          : const LayerWish.none(),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (_locked) {
      return LockScreen(
        key: const ValueKey('lock screen'),
        source: widget.source,
        showUser: _showUser,
        now: widget.now,
      );
    }
    final saver = screenSaver(_saver, now: widget.now);
    return saver == null
        ? const SizedBox.shrink()
        : KeyedSubtree(key: ValueKey('saver $_saver'), child: saver);
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
                    enabled: !_busy,
                    onSubmitted: (_) => _submit(),
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
