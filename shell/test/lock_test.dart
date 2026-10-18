import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_shell/lock/lock_screen.dart';
import 'package:silica_shell/lock/savers.dart';
import 'package:silica_shell/system/shell_channel.dart';

import 'glass_backend_probe.dart';

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
}

final DateTime fixedNow = DateTime(2026, 10, 18, 9, 5);

Future<void> pumpApp(WidgetTester tester, Widget child) async {
  tester.view.physicalSize = const Size(1280, 800);
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
Future<void> pump(
  WidgetTester tester,
  FakeLock source, {
  bool showUser = true,
}) => pumpApp(
  tester,
  LockScreen(source: source, showUser: showUser, now: () => fixedNow),
);

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

  test('showUser comes from lockscreen.json, true by default', () {
    final dir = Directory.systemTemp.createTempSync('lockcfg');
    addTearDown(() => dir.deleteSync(recursive: true));
    final env = {'XDG_CONFIG_HOME': dir.path};
    expect(readShowUser(env: env), isTrue);
    Directory('${dir.path}/silica').createSync();
    File(
      '${dir.path}/silica/lockscreen.json',
    ).writeAsStringSync('{"showUser": false}');
    expect(readShowUser(env: env), isFalse);
    File('${dir.path}/silica/lockscreen.json').writeAsStringSync('not json');
    expect(readShowUser(env: env), isTrue);
  });

  testWidgets('the lock screen shows the time, the date and the user', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    expect(find.text('09:05'), findsOneWidget);
    expect(find.text('Anna Rossi'), findsOneWidget);
    expect(find.byKey(const ValueKey('lock date')), findsOneWidget);
  });

  testWidgets('without showUser the name and the initial are not shown', (
    tester,
  ) async {
    await pump(tester, FakeLock(), showUser: false);
    expect(find.text('Anna Rossi'), findsNothing);
    expect(find.text('A'), findsNothing);
    expect(find.byKey(const ValueKey('lock password')), findsOneWidget);
  });

  testWidgets('typed characters replace the placeholder and stay obscured', (
    tester,
  ) async {
    await pump(tester, FakeLock());
    await tester.pump();
    expect(find.text('Enter password'), findsOneWidget);
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'abc');
    await tester.pump();
    expect(find.text('Enter password'), findsNothing);
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
    expect(find.text('Incorrect password.'), findsOneWidget);
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
    expect(find.text('Incorrect password.'), findsOneWidget);
    final field = tester.widget<EditableText>(
      find.descendant(
        of: find.byKey(const ValueKey('lock password')),
        matching: find.byType(EditableText),
      ),
    );
    expect(field.controller.text, isEmpty);
    expect(field.obscureText, isTrue);

    await tester.enterText(
      find.byKey(const ValueKey('lock password')),
      'secret',
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(source.tries, ['nope', 'secret']);
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
}
