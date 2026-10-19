import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_shell/lock/lock_screen.dart';
import 'package:silica_shell/lock/lock_parts.dart';
import 'package:silica_shell/lock/savers.dart';
import 'package:silica_shell/menubar/power.dart';
import 'package:silica_shell/notifications/model.dart';
import 'package:silica_shell/notifications/service.dart';
import 'package:silica_shell/system/system_status.dart';
import 'package:silica_shell/system/shell_channel.dart';
import 'package:silica_ui/silica_ui.dart';

import 'glass_backend_probe.dart';
import 'support/settings.dart';

class FakeLock implements LockSource {
  final tries = <String>[];
  bool throws = false;

  /// When set, `unlock` waits for it.
  Completer<void>? gate;

  @override
  ({String login, String name}) get user => (login: 'anna', name: 'Anna Rossi');

  @override
  Future<bool> unlock(String password) async {
    tries.add(password);
    await gate?.future;
    if (throws) throw PlatformException(code: 'Failed');
    return password == 'secret';
  }

  var released = 0;

  @override
  Future<bool> release() async {
    released++;
    return true;
  }
}

class FakeSystem extends ValueNotifier<SystemSnapshot> implements SystemSource {
  FakeSystem(super.value);

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class FakeNotifications extends ChangeNotifier implements NotificationsSource {
  FakeNotifications(this.notifications);

  @override
  final Map<int, ShellNotification> notifications;

  @override
  Stream<ShellNotification> get posted => const Stream.empty();

  @override
  Future<void> dismiss(int id) async {}
  @override
  Future<void> invoke(int id, String action) async {}
  @override
  Future<void> reply(int id, String text) async {}
  @override
  Future<void> clear() async {}
}

Finder labelled(String label) => find.byWidgetPredicate(
  (w) => w is Semantics && w.properties.label == label,
);

final DateTime fixedNow = DateTime(2026, 10, 18, 9, 5);

Future<void> pumpApp(
  WidgetTester tester,
  Widget child, {
  Size size = const Size(1280, 800),
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    const MethodChannel('silica/layer', JSONMethodCodec()),
    (call) async => true,
  );
  addTearDown(() => ShellChannel.instance.states.value = const {});
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: ShellLocalizations(child: compositorScope(child)),
    ),
  );
}

/// The lock screen, as the view of a lock surface.
Future<MemorySettingsBackend> pump(
  WidgetTester tester,
  FakeLock source, {
  bool showUser = true,
  Map<String, Object?> values = const {},
  SystemSource? system,
  NotificationsSource? notifications,
  void Function(PowerAction)? onPower,
  DateTime Function()? now,
  Size size = const Size(1280, 800),
  LockLook? look,
}) async {
  final settings = await useSettings({
    'lockscreen.showUser': showUser,
    ...values,
  });
  await pumpApp(
    tester,
    LockScreen(
      source: source,
      system: system,
      notifications: notifications,
      onPower: onPower,
      now: now ?? () => fixedNow,
      look: look,
    ),
    size: size,
  );
  return settings;
}

/// The screen saver layer, with a fake for the lock request.
Future<void> pumpSaver(WidgetTester tester, {Future<bool> Function()? take}) =>
    pumpApp(
      tester,
      ScreenSaverLayer(now: () => fixedNow, take: take ?? () async => true),
    );

void announce(Map<String, String> states) {
  ShellChannel.instance.states.value = states;
}

void main() {
  test('the user: login and the full name of /etc/passwd', () {
    final dir = Directory.systemTemp.createTempSync('passwd');
    addTearDown(() => dir.deleteSync(recursive: true));
    final file = File('${dir.path}/passwd')
      ..writeAsStringSync(
        'root:x:0:0:root:/root:/bin/bash\n'
        'anna:x:1000:1000:Anna Rossi,,,:/home/anna:/bin/bash\n',
      );
    expect(currentUser(env: {'USER': 'anna'}, passwd: file.path), (
      login: 'anna',
      name: 'Anna Rossi',
    ));
    expect(currentUser(env: {'USER': 'bob'}, passwd: file.path), (
      login: 'bob',
      name: 'bob',
    ));
  });

  testWidgets('the lock screen shows the time, the date and the user', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    expect(labelled('09:05'), findsOneWidget);
    expect(find.text('Anna Rossi'), findsOneWidget);
    expect(find.byKey(const ValueKey('lock date')), findsOneWidget);
  });

  testWidgets('without showUser the name and the initial are not shown', (
    tester,
  ) async {
    await pump(tester, FakeLock(), showUser: false);
    expect(find.text('Anna Rossi'), findsNothing);
    expect(find.byKey(const ValueKey('lock password')), findsOneWidget);
  });

  testWidgets('lockscreen.showUser applies live', (tester) async {
    final backend = await pump(tester, FakeLock());
    expect(find.text('Anna Rossi'), findsOneWidget);
    backend.emit('lockscreen.showUser', false);
    await tester.pump();
    expect(find.text('Anna Rossi'), findsNothing);
    backend.emit('lockscreen.showUser', true);
    await tester.pump();
    expect(find.text('Anna Rossi'), findsOneWidget);
  });

  testWidgets('typed characters replace the placeholder and stay obscured', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    await tester.pump();
    expect(find.text('Enter Password'), findsOneWidget);
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'abc');
    await tester.pump();
    expect(find.text('Enter Password'), findsNothing);
    final field = tester.widget<EditableText>(
      find.descendant(
        of: find.byKey(const ValueKey('lock password')),
        matching: find.byType(EditableText),
      ),
    );
    expect(field.controller.text, 'abc');
    expect(field.obscureText, isTrue);
  });

  testWidgets('the field keeps focus while the password is checked and after '
      'a failure', (tester) async {
    final source = FakeLock()..gate = Completer<void>();
    await pump(tester, source);
    await tester.pump();
    EditableText field() => tester.widget<EditableText>(
      find.descendant(
        of: find.byKey(const ValueKey('lock password')),
        matching: find.byType(EditableText),
      ),
    );
    expect(field().focusNode.hasFocus, isTrue);
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'nope');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    expect(field().focusNode.hasFocus, isTrue);
    source.gate!.complete();
    await tester.pumpAndSettle();
    expect(
      tester.state<LockScreenState>(find.byType(LockScreen)).fieldState,
      LockFieldState.wrong,
    );
    expect(field().focusNode.hasFocus, isTrue);
    // Typing goes on without a click.
    tester.testTextInput.enterText('again');
    await tester.pump();
    expect(field().controller.text, 'again');
  });

  testWidgets('a wrong password shakes and says so; the right one is sent', (
    tester,
  ) async {
    final source = FakeLock();
    await pump(tester, source);
    await tester.pump();
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'nope');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    await tester.pumpAndSettle();
    expect(source.tries, ['nope']);
    expect(labelled('Incorrect password.'), findsOneWidget);
    final field = tester.widget<EditableText>(
      find.descendant(
        of: find.byKey(const ValueKey('lock password')),
        matching: find.byType(EditableText),
      ),
    );
    // The dots stay, selected: typing replaces them.
    expect(field.controller.text, 'nope');
    expect(
      field.controller.selection,
      const TextSelection(baseOffset: 0, extentOffset: 4),
    );
    expect(field.obscureText, isTrue);

    await tester.enterText(
      find.byKey(const ValueKey('lock password')),
      'secret',
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(source.tries, ['nope', 'secret']);
    expect(source.released, 1);
  });

  testWidgets('a password that cannot be checked is told apart', (
    tester,
  ) async {
    final source = FakeLock()..throws = true;
    await pump(tester, source);
    await tester.pump();
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'x');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(find.text('The password could not be checked.'), findsOneWidget);
    expect(
      tester.state<LockScreenState>(find.byType(LockScreen)).fieldState,
      isNot(LockFieldState.wrong),
    );
  });

  testWidgets('the screen saver of the state is drawn and ends with it', (
    tester,
  ) async {
    await pumpSaver(tester);
    for (final id in screenSaverIds) {
      announce({'screensaver.active': id});
      await tester.pump();
      expect(find.byKey(ValueKey('saver $id')), findsOneWidget, reason: id);
      await tester.pump(const Duration(seconds: 1));
    }
    announce({'screensaver.active': ''});
    await tester.pump();
    for (final id in screenSaverIds) {
      expect(find.byKey(ValueKey('saver $id')), findsNothing);
    }
    // An unknown saver draws nothing.
    announce({'screensaver.active': 'fire'});
    await tester.pump();
    expect(find.byType(CustomPaint), findsNothing);
  });

  testWidgets('the clock saver shows the time', (tester) async {
    await pumpSaver(tester);
    announce({'screensaver.active': 'clock'});
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('09:05'), findsOneWidget);
  });

  testWidgets('the session lock is asked for while Ossido says locked', (
    tester,
  ) async {
    var asked = 0;
    await pumpSaver(
      tester,
      take: () async {
        asked++;
        return true;
      },
    );
    expect(asked, 0);
    announce({'session.locked': '1'});
    await tester.pump();
    expect(asked, 1);
    // Again every few seconds (the request is idempotent), until it is unlocked.
    await tester.pump(const Duration(seconds: 3));
    await tester.pump(const Duration(seconds: 3));
    expect(asked, 3);
    announce({'session.locked': '0'});
    await tester.pump();
    await tester.pump(const Duration(seconds: 10));
    expect(asked, 3);
  });

  testWidgets('a shell that starts with the session locked asks at once', (
    tester,
  ) async {
    var asked = 0;
    ShellChannel.instance.states.value = const {'session.locked': '1'};
    await pumpSaver(
      tester,
      take: () async {
        asked++;
        return true;
      },
    );
    expect(asked, 1);
  });

  testWidgets('a shell that missed session.locked pulls the states and asks', (
    tester,
  ) async {
    var asked = 0;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      ShellChannel.channel,
      (call) async => call.method == 'states' ? {'session.locked': '1'} : null,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ShellChannel.channel,
        null,
      ),
    );
    await pumpSaver(
      tester,
      take: () async {
        asked++;
        return true;
      },
    );
    await tester.pump();
    expect(asked, 1);
  });

  testWidgets('the right password releases the lock through the embedder', (
    tester,
  ) async {
    final calls = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      LockControl.channel,
      (call) async {
        calls.add(call.method);
        return true;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        LockControl.channel,
        null,
      ),
    );
    expect(await LockControl.take(), isTrue);
    expect(await LockControl.release(), isTrue);
    expect(calls, ['lock', 'unlock']);
  });

  Rect rectOf(WidgetTester tester, String key) =>
      tester.getRect(find.byKey(ValueKey(key)));

  Future<void> fail(WidgetTester tester, String password) async {
    await tester.enterText(
      find.byKey(const ValueKey('lock password')),
      password,
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
  }

  LockScreenState stateOf(WidgetTester tester) =>
      tester.state<LockScreenState>(find.byType(LockScreen));

  test('the layout is placed from the edges (1920 x 1200 and 2560 x 1440)', () {
    // lock-screen.md › Screen layout, at the kit's size.
    const a = Size(1920, 1200);
    expect(LockMetrics.clock(a), const Rect.fromLTWH(480, 104, 960, 240));
    expect(LockMetrics.userBlock(a), const Rect.fromLTWH(800, 916, 320, 108));
    expect(LockMetrics.avatarIn(a), const Rect.fromLTWH(924, 916, 72, 72));
    expect(LockMetrics.name(a), const Rect.fromLTWH(800, 1000, 320, 22));
    expect(LockMetrics.field(a), const Rect.fromLTWH(856, 1040, 208, 32));
    expect(LockMetrics.helpButton(a), const Rect.fromLTWH(1072, 1040, 32, 32));
    expect(LockMetrics.hint(a), const Rect.fromLTWH(800, 948, 320, 68));
    expect(LockMetrics.helpAddress(a), const Rect.fromLTWH(800, 1084, 320, 16));
    expect(LockMetrics.banner(a, 0), const Rect.fromLTWH(1544, 48, 360, 60));
    expect(LockMetrics.banner(a, 1).top, 116);
    expect(LockMetrics.powerRow(a), const Rect.fromLTWH(816, 1104, 288, 72));
    // The spacing of the doc: avatar to name 12, name box to field 18, hint to field 24,
    // field to address 12, field to power row 32; the field's bottom 128 above the edge.
    expect(LockMetrics.name(a).top - LockMetrics.avatarIn(a).bottom, 12);
    expect(LockMetrics.field(a).top - LockMetrics.name(a).bottom, 18);
    expect(LockMetrics.field(a).top - LockMetrics.hint(a).bottom, 24);
    expect(LockMetrics.helpAddress(a).top - LockMetrics.field(a).bottom, 12);
    expect(LockMetrics.powerRow(a).top - LockMetrics.field(a).bottom, 32);
    expect(a.height - LockMetrics.field(a).bottom, 128);
    // The same rule at the reference size (the doc's check table).
    const b = Size(2560, 1440);
    expect(LockMetrics.clock(b).topLeft, const Offset(800, 104));
    expect(LockMetrics.userBlock(b).topLeft, const Offset(1120, 1156));
    expect(LockMetrics.field(b).topLeft, const Offset(1176, 1280));
    expect(LockMetrics.helpButton(b).left, 1392);
    // The help button hangs outside the centring.
    expect(LockMetrics.field(b).center.dx, 1280);
    expect(LockMetrics.helpButton(b).top, LockMetrics.field(b).top);
    // Wake, step 2: the user block is one field row (48 pt) higher.
    expect(
      LockMetrics.userBlock(a).top - LockMetrics.userBlock(a, raised: true).top,
      48,
    );
  });

  testWidgets('the widgets sit where the layout says', (tester) async {
    await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    const s = Size(1280, 800);
    expect(rectOf(tester, 'lock password'), LockMetrics.field(s));
    expect(rectOf(tester, 'lock help'), LockMetrics.helpButton(s));
    expect(rectOf(tester, 'lock user block'), LockMetrics.userBlock(s));
    expect(rectOf(tester, 'lock clock block'), LockMetrics.clock(s));
    expect(rectOf(tester, 'lock user'), LockMetrics.name(s));
    expect(rectOf(tester, 'lock avatar'), LockMetrics.avatarIn(s));
    // The wallpaper is dimmed by 6 % and nothing blurs it.
    final dim = tester.widget<ColoredBox>(
      find.byKey(const ValueKey('lock dim')),
    );
    expect(dim.color, const Color(0x0F000000));
  });

  testWidgets('the field never changes size across its states', (tester) async {
    final source = FakeLock()..gate = Completer<void>();
    await pump(tester, source);
    await tester.pumpAndSettle();
    final rest = rectOf(tester, 'lock password');
    expect(stateOf(tester).fieldState, LockFieldState.placeholder);
    await tester.enterText(
      find.byKey(const ValueKey('lock password')),
      'a long password that goes on',
    );
    await tester.pump();
    expect(stateOf(tester).fieldState, LockFieldState.typing);
    expect(rectOf(tester, 'lock password'), rest);
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    expect(stateOf(tester).fieldState, LockFieldState.verifying);
    expect(rectOf(tester, 'lock password'), rest);
    expect(find.byType(AmalfiSpinner), findsOneWidget);
    expect(find.byKey(const ValueKey('lock submit')), findsNothing);
    source.gate!.complete();
    await tester.pumpAndSettle();
    expect(stateOf(tester).fieldState, LockFieldState.wrong);
    expect(rectOf(tester, 'lock password'), rest);
  });

  testWidgets('the submit button shows with the first character only', (
    tester,
  ) async {
    final source = FakeLock();
    await pump(tester, source);
    await tester.pumpAndSettle();
    AnimatedOpacity fade() => tester.widget<AnimatedOpacity>(
      find.byKey(const ValueKey('lock submit fade')),
    );
    expect(fade().opacity, 0);
    expect(find.byKey(const ValueKey('lock placeholder')), findsOneWidget);
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'x');
    await tester.pump();
    // In with `fade.quick` (150 ms), with the first character.
    expect(fade().opacity, 1);
    expect(fade().duration, const Duration(milliseconds: 150));
    expect(find.byKey(const ValueKey('lock placeholder')), findsNothing);
    await tester.pumpAndSettle();
    // The button submits.
    await tester.tap(find.byKey(const ValueKey('lock submit')));
    await tester.pumpAndSettle();
    expect(source.tries, ['x']);
    // Esc empties the field and the button goes away in 60 ms.
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(fade().opacity, 0);
    expect(fade().duration, const Duration(milliseconds: 60));
    expect(stateOf(tester).fieldState, LockFieldState.placeholder);
    // Hidden, it takes no clicks.
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('lock submit fade')),
      warnIfMissed: false,
    );
    await tester.pumpAndSettle();
    expect(source.tries, ['x']);
  });

  test('the dots: 8 pt, 12 pitch, first at 16; the selection and the caret', () {
    expect(LockFieldMetrics.dotLeft(0), 16);
    expect(LockFieldMetrics.dotLeft(3), 52);
    expect(LockFieldMetrics.caret(0), const Rect.fromLTWH(16, 8, 2, 16));
    expect(LockFieldMetrics.caret(2), const Rect.fromLTWH(40, 8, 2, 16));
    // Six dots (the kit's Typing and Wrong): the caret at 88, the selection 10 to 90.
    expect(LockFieldMetrics.caret(6), const Rect.fromLTWH(88, 8, 2, 16));
    expect(LockFieldMetrics.selection(6), const Rect.fromLTRB(10, 8, 90, 24));
    // More than 13 dots scroll inside the field: the caret stays before the submit.
    expect(LockFieldMetrics.caret(40), LockFieldMetrics.caret(13));
    expect(
      LockFieldMetrics.caret(13).right,
      lessThan(LockFieldMetrics.submitX),
    );
  });

  testWidgets('typing replaces the selected dots after a failure', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    await fail(tester, 'nope');
    expect(stateOf(tester).fieldState, LockFieldState.wrong);
    tester.testTextInput.enterText('s');
    await tester.pump();
    expect(stateOf(tester).fieldState, LockFieldState.typing);
  });

  testWidgets('the hint replaces the avatar and the name after the second '
      'failure, in one frame', (tester) async {
    await pump(
      tester,
      FakeLock(),
      values: {
        'lockscreen.passwordHint': 'My cat',
        'lockscreen.helpAddress': 'help@example.org',
      },
    );
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('lock hint')), findsNothing);
    await fail(tester, 'one');
    expect(find.text('Anna Rossi'), findsOneWidget);
    expect(find.byKey(const ValueKey('lock hint')), findsNothing);
    final field = rectOf(tester, 'lock password');
    await fail(tester, 'two');
    expect(stateOf(tester).hintShown, isTrue);
    expect(find.text('Anna Rossi'), findsNothing);
    expect(find.text('My cat'), findsOneWidget);
    expect(find.text('help@example.org'), findsOneWidget);
    // The hint stands on the hint block's bottom, 24 above the field, 320 wide.
    final hint = rectOf(tester, 'lock hint');
    final block = LockMetrics.hint(const Size(1280, 800));
    expect(hint.bottom, block.bottom);
    expect(hint.left, block.left);
    expect(hint.width, 320);
    expect(
      rectOf(tester, 'lock help address'),
      LockMetrics.helpAddress(const Size(1280, 800)),
    );
    // The field does not move.
    expect(rectOf(tester, 'lock password'), field);
  });

  testWidgets('the hint settings apply live', (tester) async {
    final backend = await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('lock help')), findsOneWidget);
    expect(find.byKey(const ValueKey('lock power')), findsNothing);
    backend.emit('lockscreen.showHints', false);
    backend.emit('lockscreen.showPowerButtons', true);
    await tester.pump();
    expect(find.byKey(const ValueKey('lock help')), findsNothing);
    expect(find.byKey(const ValueKey('lock power')), findsOneWidget);
  });

  testWidgets('with hints off there is no hint and no help button', (
    tester,
  ) async {
    await pump(tester, FakeLock(), values: {'lockscreen.showHints': false});
    await tester.pumpAndSettle();
    await fail(tester, 'one');
    await fail(tester, 'two');
    expect(find.byKey(const ValueKey('lock hint')), findsNothing);
    expect(find.byKey(const ValueKey('lock help')), findsNothing);
    expect(find.text('Anna Rossi'), findsOneWidget);
  });

  testWidgets(
    'the help button shows the hint, and a default text without one',
    (tester) async {
      await pump(tester, FakeLock());
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('lock help')));
      await tester.pump();
      expect(
        find.text('No password hint is set for this account.'),
        findsOneWidget,
      );
      await tester.tap(find.byKey(const ValueKey('lock help')));
      await tester.pump();
      expect(find.byKey(const ValueKey('lock hint')), findsNothing);
    },
  );

  test(
    'the shake keyframes: the measured ones scaled by 208 / 160 in ~400 ms',
    () {
      expect(lockShake, const Duration(milliseconds: 400));
      expect(lockShakeOffset(0), 0);
      // −10, +14, −11, +10, −7, +6 measured, × 1.3 (lock-screen.md › Motion).
      expect(lockShakeOffset(31), closeTo(-13, 1e-9));
      expect(lockShakeOffset(94), closeTo(18, 1e-9));
      expect(lockShakeOffset(156), closeTo(-14, 1e-9));
      expect(lockShakeOffset(219), closeTo(13, 1e-9));
      expect(lockShakeOffset(281), closeTo(-9, 1e-9));
      expect(lockShakeOffset(344), closeTo(8, 1e-9));
      expect(lockShakeOffset(400), 0);
      // First to the left, sine-like between the peaks.
      expect(lockShakeOffset(10), lessThan(0));
      expect(lockShakeOffset(62.5), closeTo(2.5, 0.5));
    },
  );

  testWidgets('a wrong password shakes the field and the help button only', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    final field = rectOf(tester, 'lock password');
    final help = rectOf(tester, 'lock help');
    final block = rectOf(tester, 'lock user block');
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'nope');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    // The check is instant: the shake starts on the next frames.
    await tester.pump();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 31));
    final moved = rectOf(tester, 'lock password').left - field.left;
    expect(moved, closeTo(-13, 3));
    expect(rectOf(tester, 'lock help').left - help.left, closeTo(moved, 1e-9));
    expect(rectOf(tester, 'lock help').top, help.top);
    expect(rectOf(tester, 'lock user block'), block);
    await tester.pump(const Duration(milliseconds: 63));
    expect(rectOf(tester, 'lock password').left - field.left, closeTo(18, 3));
    await tester.pumpAndSettle();
    expect(rectOf(tester, 'lock password'), field);
  });

  test('the lock and unlock timings', () {
    // Lock: ~250 ms, progress ~0.6 at half time.
    expect(lockFade, const Duration(milliseconds: 250));
    expect(lockFadeCurve.transform(0.5), closeTo(0.6, 0.06));
    expect(lockFadeCurve.transform(1), 1);
    // Unlock: 95 % at ~235 ms, done at 350 ms.
    expect(unlockFade, const Duration(milliseconds: 350));
    const curve = UnlockCurve();
    expect(curve.transform(235 / 350), closeTo(0.95, 0.01));
    expect(curve.transform(1), 1);
    expect(curve.transform(0), 0);
    // Wake: +150 ms, then +20 ms. Minute roll: ~900 ms.
    expect(wakeClockDelay, const Duration(milliseconds: 150));
    expect(wakeFieldDelay, const Duration(milliseconds: 20));
    expect(minuteRoll, const Duration(milliseconds: 900));
    expect(minuteRollCurve.transform(0.5), greaterThan(0.8));
    expect(bannerSlideLeft(0), 1);
    expect(bannerSlideLeft(400), 0);
  });

  testWidgets('the screen fades in over the lock fade, the user block behind '
      'the clock', (tester) async {
    await pump(tester, FakeLock());
    double opacityOf(String key) => tester
        .widget<FadeTransition>(
          find
              .descendant(
                of: find.byKey(ValueKey(key)),
                matching: find.byType(FadeTransition),
              )
              .first,
        )
        .opacity
        .value;
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 10));
    expect(opacityOf('lock clock block'), greaterThan(0));
    expect(opacityOf('lock user block'), 0);
    await tester.pump(const Duration(milliseconds: 40));
    expect(opacityOf('lock user block'), greaterThan(0));
    expect(
      opacityOf('lock clock block'),
      greaterThan(opacityOf('lock user block')),
    );
    await tester.pumpAndSettle();
    expect(opacityOf('lock clock block'), 1);
    expect(opacityOf('lock user block'), 1);
  });

  testWidgets('a right password fades the screen out in ~350 ms and then lets '
      'the lock go', (tester) async {
    final source = FakeLock();
    await pump(tester, source);
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('lock password')),
      'secret',
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    await tester.pump();
    expect(source.tries, ['secret']);
    expect(source.released, 0);
    // The user block is at rest at once: the field is empty.
    expect(stateOf(tester).fieldState, LockFieldState.placeholder);
    await tester.pump(const Duration(milliseconds: 200));
    expect(source.released, 0);
    await tester.pump(const Duration(milliseconds: 160));
    await tester.pump();
    expect(source.released, 1);
  });

  testWidgets('the staged wake: wallpaper, then the clock, then the field', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    stateOf(tester).wake();
    await tester.pump();
    expect(stateOf(tester).wakeStage, LockWakeStage.wallpaper);
    expect(find.byKey(const ValueKey('lock date')), findsNothing);
    expect(find.byKey(const ValueKey('lock password')), findsNothing);
    await tester.pump(const Duration(milliseconds: 150));
    expect(stateOf(tester).wakeStage, LockWakeStage.clock);
    expect(find.byKey(const ValueKey('lock date')), findsOneWidget);
    expect(find.byKey(const ValueKey('lock password')), findsNothing);
    // The user block is 48 pt higher (one field row), as the doc scales the measured 36.
    expect(
      rectOf(tester, 'lock user block').top,
      LockMetrics.userBlock(const Size(1280, 800), raised: true).top,
    );
    await tester.pump(const Duration(milliseconds: 20));
    expect(stateOf(tester).wakeStage, LockWakeStage.full);
    expect(find.byKey(const ValueKey('lock password')), findsOneWidget);
    expect(
      rectOf(tester, 'lock user block').top,
      LockMetrics.userBlock(const Size(1280, 800)).top,
    );
  });

  testWidgets('only the digits that change roll, the others stay put', (
    tester,
  ) async {
    var now = DateTime(2026, 10, 18, 9, 5);
    await pump(tester, FakeLock(), now: () => now);
    await tester.pumpAndSettle();
    final rest = {
      for (var i = 0; i < 5; i++)
        i: tester.getRect(find.byKey(ValueKey('lock digit $i'))),
    };
    expect(rest[0]!.height, LockMetrics.timeHeight);
    now = DateTime(2026, 10, 18, 9, 6);
    // The seconds timer rebuilds the screen with the new minute.
    await tester.pump(const Duration(seconds: 1));
    final clock = tester.state<LockClockState>(find.byType(LockClock));
    expect(clock.rolling, isTrue);
    // The tens of the hour, the hour and the colon do not roll.
    await tester.pump(const Duration(milliseconds: 450));
    expect(clock.progressOf(2), greaterThan(0.4));
    expect(clock.progressOf(0), greaterThan(0.4));
    // The ones digit starts 60 ms after: it is behind.
    expect(clock.progressOf(3), lessThan(clock.progressOf(2)));
    for (var i = 0; i < 5; i++) {
      expect(tester.getRect(find.byKey(ValueKey('lock digit $i'))), rest[i]);
    }
    await tester.pump(const Duration(seconds: 1));
    expect(clock.rolling, isFalse);
  });

  testWidgets('the power row is hidden by default', (tester) async {
    await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('lock power')), findsNothing);
  });

  testWidgets('the power row works when shown', (tester) async {
    final done = <PowerAction>[];
    await pump(
      tester,
      FakeLock(),
      values: {'lockscreen.showPowerButtons': true},
      onPower: done.add,
    );
    await tester.pumpAndSettle();
    expect(
      rectOf(tester, 'lock power'),
      LockMetrics.powerRow(const Size(1280, 800)),
    );
    await tester.tap(find.text('Sleep'));
    expect(done, [PowerAction.sleep]);
    // Restart and Shut Down ask first, like the Logo menu.
    await tester.tap(find.text('Restart'));
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('lock power confirmation')),
      findsOneWidget,
    );
    expect(done, [PowerAction.sleep]);
    await tester.tap(find.byKey(const ValueKey('power cancel')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('lock power confirmation')), findsNothing);
    expect(done, [PowerAction.sleep]);
    await tester.tap(find.text('Restart'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('power confirm')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Shut Down'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('power confirm')));
    await tester.pumpAndSettle();
    expect(done, [
      PowerAction.sleep,
      PowerAction.restart,
      PowerAction.shutDown,
    ]);
  });

  testWidgets(
    'the Status Bar: Wi-Fi and battery where the menu bar keeps them',
    (tester) async {
      final system = FakeSystem(
        const SystemSnapshot(
          wifiAvailable: true,
          wifiEnabled: true,
          wifiSsid: 'home',
          wifiStrength: 80,
          batteryPresent: true,
          batteryPercent: 60,
        ),
      );
      await pump(tester, FakeLock(), system: system);
      await tester.pumpAndSettle();
      final bar = rectOf(tester, 'lock status bar');
      expect(bar.right, 1280 - 8);
      expect(bar.top, 0);
      expect(bar.height, 32);
      final wifi = rectOf(tester, 'status wifi');
      final battery = rectOf(tester, 'status battery');
      // Items touch, 36 wide for a glyph, the hit area the bar's full height.
      expect(wifi.size, const Size(36, 32));
      expect(battery.left, wifi.right);
      expect(battery.right, 1280 - 8);
      expect(find.text('60%'), findsOneWidget);
      system.value = const SystemSnapshot();
      await tester.pump();
      expect(find.byKey(const ValueKey('status wifi')), findsNothing);
      expect(find.byKey(const ValueKey('status battery')), findsNothing);
    },
  );

  testWidgets('collapsed banners: the app and the count, never the text', (
    tester,
  ) async {
    final stamp = DateTime(2026, 10, 18, 8, 30);
    ShellNotification n(int id, String app, String body) => ShellNotification(
      id: id,
      appName: app,
      summary: 'Secret subject',
      body: body,
      desktopEntry: app.toLowerCase(),
      timestamp: stamp.add(Duration(minutes: id)),
    );
    final source = FakeNotifications({
      1: n(1, 'Mail', 'Secret body'),
      2: n(2, 'Mail', 'Secret body'),
      3: n(3, 'Chat', 'Secret body'),
    });
    await pump(tester, FakeLock(), notifications: source);
    await tester.pumpAndSettle();
    expect(find.text('Mail'), findsOneWidget);
    expect(find.text('2 notifications'), findsOneWidget);
    expect(find.text('Chat'), findsOneWidget);
    expect(find.text('1 notification'), findsOneWidget);
    expect(find.textContaining('Secret'), findsNothing);
    final first = tester.getRect(
      find.byKey(const ValueKey('lock banner chat')),
    );
    expect(first, LockMetrics.banner(const Size(1280, 800), 0));
  });

  testWidgets('reduce motion: the minute roll is a cross-fade and the shake an '
      'edge flash', (tester) async {
    var now = DateTime(2026, 10, 18, 9, 5);
    tester.binding.platformDispatcher.accessibilityFeaturesTestValue =
        const FakeAccessibilityFeatures(disableAnimations: true);
    addTearDown(
      tester.binding.platformDispatcher.clearAccessibilityFeaturesTestValue,
    );
    await pump(tester, FakeLock(), now: () => now);
    await tester.pumpAndSettle();
    final field = rectOf(tester, 'lock password');
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'nope');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 94));
    expect(rectOf(tester, 'lock password'), field);
    await tester.pumpAndSettle();
  });

  EditableText inputOf(WidgetTester tester) => tester.widget<EditableText>(
    find.descendant(
      of: find.byKey(const ValueKey('lock password')),
      matching: find.byType(EditableText),
    ),
  );

  testWidgets('after a failure, typing replaces the dots with no click', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    // Type into the focused field as the platform does, then Return.
    tester.testTextInput.enterText('bad');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(stateOf(tester).fieldState, LockFieldState.wrong);
    expect(inputOf(tester).focusNode.hasFocus, isTrue);
    expect(tester.testTextInput.hasAnyClients, isTrue);
    // The platform appends at the caret, without honouring the selection.
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'bads',
        selection: TextSelection.collapsed(offset: 4),
      ),
    );
    await tester.pump();
    expect(inputOf(tester).controller.text, 's');
    expect(stateOf(tester).fieldState, LockFieldState.typing);
    // Or it honours it and replaces.
    tester.testTextInput.enterText('silica');
    await tester.pump();
    expect(inputOf(tester).controller.text, 'silica');
  });

  testWidgets('after a failure, a platform that deletes one character '
      'clears the selected dots', (tester) async {
    await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    await fail(tester, 'bad');
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'ba',
        selection: TextSelection.collapsed(offset: 2),
      ),
    );
    await tester.pump();
    expect(inputOf(tester).controller.text, isEmpty);
  });

  testWidgets('Escape clears the field, with and without a failure', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    await tester.pumpAndSettle();
    tester.testTextInput.enterText('abcdefg');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(inputOf(tester).controller.text, isEmpty);
    expect(stateOf(tester).fieldState, LockFieldState.placeholder);
    await fail(tester, 'bad');
    expect(stateOf(tester).fieldState, LockFieldState.wrong);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(inputOf(tester).controller.text, isEmpty);
    expect(stateOf(tester).fieldState, LockFieldState.placeholder);
  });

  testWidgets(
    'the clock glass stays off the edges of its glyph mask (silica-desktop#250)',
    (tester) async {
      // A cell at a fractional x, as at 1.5x: the mask's bounds cut device pixels.
      await tester.pumpWidget(
        const Directionality(
          textDirection: TextDirection.ltr,
          child: Stack(
            children: [
              Positioned(
                left: 10.3,
                top: 0,
                width: 180,
                height: 200,
                child: LockGlyphGlass('2', look: LockLook(), inset: 24),
              ),
            ],
          ),
        ),
      );
      final mask = tester.getRect(find.byType(ShaderMask));
      final glass = tester.getRect(
        find.descendant(
          of: find.byType(ShaderMask),
          matching: find.byType(ClipRect),
        ),
      );
      // Destination-in keeps the child where a bound only partly covers a pixel: the glass
      // leaves more than a device pixel empty on every side.
      expect(glass.left - mask.left, greaterThanOrEqualTo(2));
      expect(glass.top - mask.top, greaterThanOrEqualTo(2));
      expect(mask.right - glass.right, greaterThanOrEqualTo(2));
      expect(mask.bottom - glass.bottom, greaterThanOrEqualTo(2));
    },
  );
}
