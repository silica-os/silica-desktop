// The lock screen and the screen saver.
//
// The lock screen is the shell's client of ext-session-lock-v1: Ossido announces `session.locked`
// (`1`) and [LockController] (in the `screensaver` layer) asks the embedder for the session lock
// (`silica/lock`, `lock`); once Ossido grants it, the embedder gives the `lock` view a lock
// surface and [LockScreen] draws in it: the wallpaper under a veil, the shared Status Bar where
// the menu bar keeps it, the glass clock and the date, and the user with a password field. The password goes to silica-daemon (`System1.Unlock`, which only
// checks it); on a right one the embedder unlocks (`silica/lock`, `unlock`). While locked,
// Ossido draws and feeds nothing else; if this process dies the session stays locked and the
// shell, started again, takes the lock over. A wrong password shakes the field and leaves the
// dots selected. The look and the behaviour are docs/design/amalfi/lock-screen.md and
// docs/spec/lock.md (L-01).
//
// The screen saver (`screensaver.active`: the id of the saver, empty when none) draws in an
// ordinary overlay layer over the desktop and ends with any input. The design is ours: the
// reference did not show its lock screen (settings docs/spec/lock-screen.md).

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../menubar/bar_menu_host.dart';
import '../menubar/power.dart';
import '../notifications/model.dart';
import '../notifications/service.dart' show NotificationsSource;
import '../status/status_items.dart';
import '../system/dbus.dart';
import '../system/system_status.dart';
import '../system/layer_arbiter.dart';
import '../system/layer_control.dart';
import '../system/shell_channel.dart';
import '../wallpaper/wallpaper.dart';
import 'lock_layout.dart';
import 'lock_parts.dart';
import 'savers.dart';

export 'lock_layout.dart';

Future<void>? _dates;

/// The user whose password unlocks, and where the password is checked.
abstract class LockSource {
  /// The login name and the full name (the GECOS name, otherwise the login).
  ({String login, String name}) get user;

  /// True if the password was right; false to try again. Throws when the password cannot be
  /// checked at all. It lets nothing go: [release] does, once the screen has faded out.
  Future<bool> unlock(String password);

  /// Lets the session lock go (after a right password); false if it could not.
  Future<bool> release();
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

  // The lock is let go here, in the process that holds it.
  @override
  Future<bool> release() => LockControl.release();
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

/// The `lockscreen.*` settings of the Settings service that the lock screen draws, live:
/// `showUser`, `showHints` (password hints, on by default), `passwordHint`, `helpAddress` and
/// `showPowerButtons` (off by default).
class LockSettings {
  LockSettings(SilicaSettings s)
    : showUser = s.boolean('lockscreen.showUser', fallback: true),
      showHints = s.boolean('lockscreen.showHints', fallback: true),
      passwordHint = s.text('lockscreen.passwordHint', fallback: ''),
      helpAddress = s.text('lockscreen.helpAddress', fallback: ''),
      showPowerButtons = s.boolean(
        'lockscreen.showPowerButtons',
        fallback: false,
      );

  final ValueListenable<bool> showUser;
  final ValueListenable<bool> showHints;
  final ValueListenable<String> passwordHint;
  final ValueListenable<String> helpAddress;
  final ValueListenable<bool> showPowerButtons;

  Listenable get changed => Listenable.merge([
    showUser,
    showHints,
    passwordHint,
    helpAddress,
    showPowerButtons,
  ]);
}

/// The locked screen itself (kit page "Lock Screen").
class LockScreen extends StatefulWidget {
  const LockScreen({
    super.key,
    required this.source,
    this.settings,
    this.system,
    this.notifications,
    this.onPower,
    this.now,
    this.look,
  });

  final LockSource source;

  /// Where the `lockscreen.*` settings come from; the system's.
  final SilicaSettings? settings;

  /// Wi-Fi and battery for the Status Bar; null: none.
  final SystemSource? system;

  /// The notifications, shown collapsed (app and count, never the text); null: none.
  final NotificationsSource? notifications;

  /// A press on the power row (shown only with `showPowerButtons`); logind by default.
  final void Function(PowerAction action)? onPower;

  final DateTime Function()? now;

  /// The appearance and the accessibility settings; null: the system's.
  final LockLook? look;

  @override
  State<LockScreen> createState() => LockScreenState();
}

class LockScreenState extends State<LockScreen> with TickerProviderStateMixin {
  final TextEditingController _password = TextEditingController();
  late final FocusNode _focus = FocusNode(
    onKeyEvent: (node, event) {
      // Escape empties the field, whatever the platform does with the selection.
      if (event is KeyDownEvent &&
          event.logicalKey == LogicalKeyboardKey.escape) {
        _escape();
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    },
  );
  late final AnimationController _shake = AnimationController(
    vsync: this,
    duration: lockShake,
  );

  // Lock: the clock fades in over 250 ms, the user block 30 ms behind it.
  late final AnimationController _fade = AnimationController(
    vsync: this,
    duration: lockFade + lockUserOffset,
  );
  late final Animation<double> _clockOpacity = CurvedAnimation(
    parent: _fade,
    curve: Interval(
      0,
      lockFade.inMilliseconds / _fade.duration!.inMilliseconds,
      curve: lockFadeCurve,
    ),
  );
  late final Animation<double> _userOpacity = CurvedAnimation(
    parent: _fade,
    curve: Interval(
      lockUserOffset.inMilliseconds / _fade.duration!.inMilliseconds,
      1,
      curve: lockFadeCurve,
    ),
  );

  // Unlock: the whole screen fades out, decelerating.
  late final AnimationController _leave = AnimationController(
    vsync: this,
    duration: unlockFade,
  );
  late final Animation<double> _screenOpacity = ReverseAnimation(
    CurvedAnimation(parent: _leave, curve: const UnlockCurve()),
  );

  Timer? _tick;
  Timer? _wakeTimer;
  bool _busy = false;
  bool _wrong = false;
  String _wrongText = '';
  bool _helpShown = false;
  int _failures = 0;
  LockWakeStage _stage = LockWakeStage.full;

  /// The last try could not be checked at all (not a wrong password).
  bool _unavailable = false;

  /// The state of the password field.
  LockFieldState get fieldState => _busy
      ? LockFieldState.verifying
      : _wrong
      ? LockFieldState.wrong
      : _password.text.isEmpty
      ? LockFieldState.placeholder
      : LockFieldState.typing;

  /// The hint block is shown instead of the avatar and name: after the second failure, or when
  /// the help button asks (only with password hints on).
  bool get hintShown => _cfg.showHints.value && (_failures >= 2 || _helpShown);

  /// The wake stage on the screen.
  LockWakeStage get wakeStage => _stage;

  late final LockSettings _cfg = LockSettings(
    widget.settings ?? SilicaSettings.instance,
  );

  /// The input sources Ossido announces (the Status Bar's input source item and its menu).
  final InputSources _inputSources = InputSources();

  /// The power action waiting for its confirmation card (restart, shut down).
  PowerAction? _confirm;

  void _userSettingChanged() {
    if (mounted) setState(() {});
  }

  @override
  void initState() {
    super.initState();
    _cfg.changed.addListener(_userSettingChanged);
    _tick = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted) setState(() {});
    });
    _password.addListener(_typed);
    _fade.forward();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _focus.requestFocus();
    });
  }

  @override
  void dispose() {
    _cfg.changed.removeListener(_userSettingChanged);
    _tick?.cancel();
    _wakeTimer?.cancel();
    _password.removeListener(_typed);
    _password.dispose();
    _focus.dispose();
    _shake.dispose();
    _fade.dispose();
    _leave.dispose();
    _inputSources.dispose();
    _inputFocus.dispose();
    super.dispose();
  }

  void _typed() {
    // Typing replaces the selected dots: the failure is over. The platform's text input may not
    // honour the selection (it appends, or deletes one character): do what a selection does.
    final text = _password.text;
    if (_wrong && text != _wrongText) {
      _wrong = false;
      final old = _wrongText;
      String? replaced;
      if (text.length > old.length && text.startsWith(old)) {
        replaced = text.substring(old.length);
      } else if (text.length < old.length && old.startsWith(text)) {
        replaced = '';
      }
      if (replaced != null) {
        _password.removeListener(_typed);
        _password.value = TextEditingValue(
          text: replaced,
          selection: TextSelection.collapsed(offset: replaced.length),
        );
        _password.addListener(_typed);
      }
    }
    if (_unavailable && _password.text != _wrongText) _unavailable = false;
    if (mounted) setState(() {});
  }

  /// The display came back (spec "Wake"): the wallpaper and the Status Bar; 150 ms later the
  /// date, clock, avatar and name, 48 pt higher (one field row) and without the field; 20 ms
  /// later the field and the help button, with the user block in place. All cuts. Nothing calls it yet:
  /// Ossido does not tell the shell when the display turns on.
  void wake() {
    _wakeTimer?.cancel();
    setState(() => _stage = LockWakeStage.wallpaper);
    _wakeTimer = Timer(wakeClockDelay, () {
      if (!mounted) return;
      setState(() => _stage = LockWakeStage.clock);
      _wakeTimer = Timer(wakeFieldDelay, () {
        if (mounted) setState(() => _stage = LockWakeStage.full);
      });
    });
  }

  Future<void> _submit() async {
    if (_busy || _password.text.isEmpty) return;
    setState(() {
      _busy = true;
      _wrong = false;
      _unavailable = false;
    });
    var right = false;
    var unavailable = false;
    try {
      right = await widget.source.unlock(_password.text);
    } on PlatformException {
      unavailable = true;
    }
    if (!mounted) return;
    if (right) {
      await _leaveScreen();
      return;
    }
    final text = _password.text;
    setState(() {
      _busy = false;
      _unavailable = unavailable;
      if (!unavailable) {
        _failures++;
        _wrong = true;
      }
      _wrongText = text;
    });
    // The dots stay, selected: typing replaces them.
    _password.selection = TextSelection(
      baseOffset: 0,
      extentOffset: text.length,
    );
    _focus.requestFocus();
    _shake.forward(from: 0);
  }

  /// A right password: the user block returns to rest in one frame (hint gone, avatar back,
  /// field empty), then the screen fades to the desktop (~350 ms) and the lock is let go.
  Future<void> _leaveScreen() async {
    _password.removeListener(_typed);
    _password.clear();
    _password.addListener(_typed);
    setState(() {
      _busy = false;
      _wrong = false;
      _unavailable = false;
      _helpShown = false;
      _failures = 0;
    });
    await _leave.forward(from: 0);
    if (!mounted) return;
    final released = await widget.source.release();
    // Right: Ossido announces `session.locked` 0 and the layer drops this screen. If the
    // lock could not be let go, the screen comes back.
    if (!released && mounted) _leave.value = 0;
  }

  void _escape() {
    if (_busy) return;
    _password.clear();
    setState(() {
      _wrong = false;
      _unavailable = false;
    });
  }

  /// The input menu is open (from the Status Bar's input source item).
  bool _inputMenu = false;
  final GlobalKey _inputItemKey = GlobalKey(debugLabel: 'lock input source');

  final FocusNode _inputFocus = FocusNode(debugLabel: 'lock input source');
  bool _inputFocused = false;

  void _toggleInputMenu() => setState(() => _inputMenu = !_inputMenu);

  void _closeInputMenu() {
    if (!_inputMenu) return;
    setState(() => _inputMenu = false);
    _focus.requestFocus();
  }

  @override
  Widget build(BuildContext context) {
    final l = l10nOf(context);
    final look = widget.look ?? LockLook.of(context);
    return AmalfiTheme(
      brightness: look.brightness,
      // The glass is drawn here: the wallpaper is in this view (the lock surface is a view of
      // its own).
      child: AmalfiGlassBackendScope(
        backend: AmalfiGlassBackend.flutter,
        child: CallbackShortcuts(
          bindings: {const SingleActivator(LogicalKeyboardKey.escape): _escape},
          // Tab and Shift+Tab move field → help → input source (→ power row): the lock view
          // has no app around it to give it the traversal keys.
          child: Shortcuts(
            shortcuts: const {
              SingleActivator(LogicalKeyboardKey.tab): NextFocusIntent(),
              SingleActivator(LogicalKeyboardKey.tab, shift: true):
                  PreviousFocusIntent(),
            },
            child: Actions(
              actions: {
                NextFocusIntent: NextFocusAction(),
                PreviousFocusIntent: PreviousFocusAction(),
              },
              child: FocusTraversalGroup(
                policy: OrderedTraversalPolicy(),
                child: FadeTransition(
                  opacity: _screenOpacity,
                  child: LayoutBuilder(
                    builder: (context, constraints) =>
                        _body(context, constraints.biggest, l, look),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// The Status Bar of the lock screen (status-bar.md › Contexts): Input Source, Wi-Fi and
  /// Battery with its percentage, where the menu bar keeps them, from the same builders.
  Widget _statusBar(AppLocalizations l, LockLook look) {
    Widget bar(SystemSnapshot? s) => AmalfiStatusBar(
      key: const ValueKey('lock status bar'),
      placement: AmalfiStatusBarPlacement.lockScreen,
      legibility: look.legibility,
      items: [
        // Tab reaches the input source after the help button (lock-screen.md › Keyboard),
        // whatever the keyboard navigation setting; Return and Space open its menu, and the
        // Active capsule shows the focus.
        if (inputSourceStatusItem(
              l,
              _inputSources.value,
              boxKey: _inputItemKey,
              active: _inputMenu || _inputFocused,
              onPressed: _toggleInputMenu,
            )
            case final item?)
          FocusTraversalOrder(
            order: const NumericFocusOrder(3),
            child: Focus(
              focusNode: _inputFocus,
              onFocusChange: (on) => setState(() => _inputFocused = on),
              onKeyEvent: (node, event) {
                if (event is KeyDownEvent &&
                    (event.logicalKey == LogicalKeyboardKey.enter ||
                        event.logicalKey == LogicalKeyboardKey.numpadEnter ||
                        event.logicalKey == LogicalKeyboardKey.space)) {
                  _toggleInputMenu();
                  return KeyEventResult.handled;
                }
                return KeyEventResult.ignored;
              },
              child: ExcludeFocus(child: item),
            ),
          ),
        if (s != null) ...[
          ?networkStatusItem(l, s),
          ?batteryStatusItem(l, s, percentage: true),
        ],
      ],
    );
    final system = widget.system;
    return ValueListenableBuilder<InputSourceState>(
      valueListenable: _inputSources,
      builder: (context, _, _) => system == null
          ? bar(null)
          : ValueListenableBuilder<SystemSnapshot>(
              valueListenable: system,
              builder: (context, s, _) => bar(s),
            ),
    );
  }

  Widget _body(
    BuildContext context,
    Size size,
    AppLocalizations l,
    LockLook look,
  ) {
    final now = (widget.now ?? DateTime.now)();
    _dates ??= initializeDateFormatting();
    final locale = l.localeName;
    final time = DateFormat.Hm(locale).format(now);
    final date = DateFormat.MMMMEEEEd(locale).format(now);
    final user = widget.source.user;
    final showUser = _cfg.showUser.value;
    final stage = _stage;
    final clockShown = stage != LockWakeStage.wallpaper;
    final fieldShown = stage == LockWakeStage.full;
    final raised = stage == LockWakeStage.clock;
    final hint = hintShown;
    final reduceMotion = look.reduceMotion;
    final flash = reduceMotion && _shake.isAnimating;
    final hintText = _cfg.passwordHint.value.trim().isEmpty
        ? l.lockNoHint
        : _cfg.passwordHint.value.trim();
    final address = _cfg.helpAddress.value.trim();
    final colors = look.colors;
    final clock = LockMetrics.clock(size);
    final banners = widget.notifications;
    return Stack(
      fit: StackFit.expand,
      children: [
        // The wallpaper, as on the desktop and not blurred, under a black 6 % veil; the light
        // appearance adds the legibility scrim (provisional).
        const WallpaperLayer(),
        const Positioned.fill(
          child: ColoredBox(
            key: ValueKey('lock dim'),
            color: Color(0x0F000000),
          ),
        ),
        if (!look.dark)
          Positioned.fill(
            child: DecoratedBox(
              key: const ValueKey('lock scrim'),
              decoration: BoxDecoration(
                gradient: AmalfiScrims.lockLight.gradient,
              ),
            ),
          ),
        Positioned(
          top: 0,
          right: LockMetrics.statusTrailing,
          height: LockMetrics.statusHeight,
          child: _statusBar(l, look),
        ),
        if (clockShown) ...[
          Positioned.fromRect(
            key: const ValueKey('lock clock block'),
            rect: clock,
            child: FadeTransition(
              opacity: _clockOpacity,
              child: Stack(
                clipBehavior: Clip.none,
                children: [
                  Positioned(
                    key: const ValueKey('lock date box'),
                    left: 0,
                    right: 0,
                    top: 0,
                    height: LockMetrics.dateHeight,
                    child: Center(
                      child: KeyedSubtree(
                        key: const ValueKey('lock date'),
                        child: LockDate(date: date, look: look),
                      ),
                    ),
                  ),
                  Positioned(
                    key: const ValueKey('lock time box'),
                    left: 0,
                    right: 0,
                    top: LockMetrics.timeTop,
                    height: LockMetrics.timeHeight,
                    child: Center(
                      child: KeyedSubtree(
                        key: const ValueKey('lock time'),
                        child: LockClock(time: time, look: look),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          if (!hint || raised)
            Positioned.fromRect(
              key: const ValueKey('lock user block'),
              rect: LockMetrics.userBlock(size, raised: raised),
              child: FadeTransition(
                opacity: _userOpacity,
                child: Stack(
                  children: [
                    Positioned(
                      left:
                          (LockMetrics.userSize.width - LockMetrics.avatar) / 2,
                      top: 0,
                      child: LockAvatar(
                        key: const ValueKey('lock avatar'),
                        look: look,
                        showUser: showUser,
                      ),
                    ),
                    if (showUser)
                      Positioned(
                        left: 0,
                        top: LockMetrics.nameTop,
                        width: LockMetrics.userSize.width,
                        height: LockMetrics.nameHeight,
                        child: KeyedSubtree(
                          key: const ValueKey('lock user'),
                          child: LockPhotoText(
                            user.name,
                            look: look,
                            style: AmalfiType.title3(),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
        ],
        if (fieldShown && hint) ...[
          // The hint replaces the avatar and the name in one frame. It stands on the block's
          // bottom, 24 above the field, and grows upwards when it wraps further. The reset row
          // of the kit is not drawn: Silica has no account recovery yet.
          Positioned(
            key: const ValueKey('lock hint'),
            left: LockMetrics.hint(size).left,
            width: LockMetrics.hintSize.width,
            bottom: size.height - LockMetrics.hint(size).bottom,
            child: LockPhotoText(
              hintText,
              look: look,
              maxLines: 6,
              style: AmalfiType.headline(),
            ),
          ),
          if (address.isNotEmpty && !_unavailable)
            Positioned.fromRect(
              key: const ValueKey('lock help address'),
              rect: LockMetrics.helpAddress(size),
              child: LockPhotoText(
                address,
                look: look,
                color: look.onPhotoSecondary,
                style: AmalfiType.callout(),
              ),
            ),
        ],
        if (fieldShown && _unavailable)
          Positioned.fromRect(
            rect: LockMetrics.helpAddress(size),
            child: KeyedSubtree(
              key: const ValueKey('lock message'),
              child: LockPhotoText(
                l.lockUnavailable,
                look: look,
                style: AmalfiType.callout(),
              ),
            ),
          ),
        if (fieldShown)
          Positioned.fill(
            child: FadeTransition(
              opacity: _userOpacity,
              // The field and the help button shake together, sideways only.
              child: AnimatedBuilder(
                animation: _shake,
                builder: (context, child) => Transform.translate(
                  offset: Offset(
                    reduceMotion
                        ? 0
                        : lockShakeOffset(
                            _shake.value * lockShake.inMilliseconds,
                          ),
                    0,
                  ),
                  child: child,
                ),
                child: Stack(
                  children: [
                    Positioned.fromRect(
                      rect: LockMetrics.field(size),
                      child: FocusTraversalOrder(
                        order: const NumericFocusOrder(1),
                        child: LockPasswordField(
                          key: const ValueKey('lock password'),
                          controller: _password,
                          focusNode: _focus,
                          look: look,
                          state: fieldState,
                          placeholder: l.lockPassword,
                          edge: flash ? colors.paletteTerracotta : null,
                          onSubmit: () {
                            // The editing action unfocuses the field (EditableText's
                            // default for "done"): take the focus back at once, the check
                            // runs with it.
                            _focus.requestFocus();
                            _submit();
                          },
                        ),
                      ),
                    ),
                    if (_cfg.showHints.value)
                      Positioned.fromRect(
                        rect: LockMetrics.helpButton(size),
                        child: FocusTraversalOrder(
                          order: const NumericFocusOrder(2),
                          child: LockHelpButton(
                            key: const ValueKey('lock help'),
                            look: look,
                            edge: flash ? colors.paletteTerracotta : null,
                            onTap: () {
                              setState(() => _helpShown = !_helpShown);
                              _focus.requestFocus();
                            },
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
        if (fieldShown && _cfg.showPowerButtons.value)
          Positioned.fromRect(
            key: const ValueKey('lock power'),
            rect: LockMetrics.powerRow(size),
            child: FadeTransition(
              opacity: _userOpacity,
              child: FocusTraversalOrder(
                order: const NumericFocusOrder(4),
                child: Row(
                  children: [
                    for (final (i, action) in const [
                      PowerAction.sleep,
                      PowerAction.restart,
                      PowerAction.shutDown,
                    ].indexed) ...[
                      if (i > 0) const SizedBox(width: 24),
                      LockActionButton(
                        look: look,
                        symbol: switch (action) {
                          PowerAction.sleep => 'moon',
                          PowerAction.restart => 'restart',
                          _ => 'power',
                        },
                        label: switch (action) {
                          PowerAction.sleep => l.lockSleep,
                          PowerAction.restart => l.lockRestart,
                          _ => l.lockShutDown,
                        },
                        onTap: () {
                          if (action == PowerAction.sleep) {
                            (widget.onPower ?? _power)(action);
                          } else {
                            setState(() => _confirm = action);
                          }
                        },
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ),
        if (fieldShown && banners != null)
          ListenableBuilder(
            listenable: banners,
            builder: (context, _) {
              final groups = groupNotifications(banners.notifications.values);
              return Stack(
                children: [
                  for (var i = 0; i < groups.length && i < 5; i++)
                    Positioned.fromRect(
                      key: ValueKey('lock banner ${groups[i].key}'),
                      rect: LockMetrics.banner(size, i),
                      child: LockBanner(
                        latest: groups[i].latest,
                        count: groups[i].items.length,
                        look: look,
                      ),
                    ),
                ],
              );
            },
          ),
        if (_inputMenu) ..._inputMenuLayer(l),
        if (_confirm case final action?)
          Positioned.fill(
            child: Stack(
              children: [
                const Positioned.fill(
                  child: AbsorbPointer(child: SizedBox.expand()),
                ),
                Positioned(
                  left: (size.width - powerCardSize.width) / 2,
                  top: size.height * 0.205,
                  child: PowerConfirmation(
                    key: const ValueKey('lock power confirmation'),
                    action: action,
                    user: user.name,
                    onCancel: () {
                      setState(() => _confirm = null);
                      _focus.requestFocus();
                    },
                    onConfirm: () {
                      setState(() => _confirm = null);
                      (widget.onPower ?? _power)(action);
                    },
                  ),
                ),
              ],
            ),
          ),
        if (_wrong)
          Semantics(
            liveRegion: true,
            label: l.lockIncorrect,
            child: const SizedBox.shrink(),
          ),
      ],
    );
  }

  /// The input menu (spec "Status items": an ordinary menu with the input sources, the current
  /// one checked), hanging from the input source item like a menu bar menu; a click outside
  /// or Escape closes it. Adding or removing sources needs Settings, which the lock screen
  /// cannot open: those rows are not offered here.
  List<Widget> _inputMenuLayer(AppLocalizations l) {
    final box = _inputItemKey.currentContext?.findRenderObject();
    final root = context.findRenderObject();
    var anchor = const Offset(0, LockMetrics.statusHeight + 8);
    if (box is RenderBox && root is RenderBox && box.attached) {
      final left = box.localToGlobal(Offset.zero, ancestor: root).dx;
      anchor = Offset(left - 4, LockMetrics.statusHeight + 8);
    }
    return [
      Positioned.fill(
        child: GestureDetector(
          key: const ValueKey('lock input menu barrier'),
          behavior: HitTestBehavior.opaque,
          onTap: _closeInputMenu,
        ),
      ),
      Positioned.fill(
        child: BarMenuHost(
          key: const ValueKey('lock input menu'),
          entryKey: 'input',
          entries: amalfiEntries([
            for (final entry in inputSourceMenu(_inputSources, l.localeName))
              if (entry is MenuAction)
                MenuAction(
                  entry.label,
                  checked: entry.checked,
                  onSelected: () {
                    entry.onSelected?.call();
                    _closeInputMenu();
                  },
                )
              else
                entry,
          ]),
          anchor: anchor,
          onDismissed: _closeInputMenu,
        ),
      ),
    ];
  }

  void _power(PowerAction action) {
    unawaited(runPower(action, logOut: () {}));
  }
}
