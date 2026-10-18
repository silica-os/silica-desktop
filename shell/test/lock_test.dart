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

  @override
  ({String login, String name}) get user => (login: 'anna', name: 'Anna Rossi');

  @override
  Future<bool> unlock(String password) async {
    tries.add(password);
    if (throws) throw PlatformException(code: 'Failed');
    return password == 'secret';
  }
}

final DateTime fixedNow = DateTime(2026, 10, 18, 9, 5);

Future<void> pump(WidgetTester tester, FakeLock source) async {
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
      child: ShellLocalizations(
        child: compositorScope(LockLayer(source: source, now: () => fixedNow)),
      ),
    ),
  );
}

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

  testWidgets('nothing is drawn until the session is locked', (tester) async {
    await pump(tester, FakeLock());
    expect(find.byKey(const ValueKey('lock screen')), findsNothing);
    announce({'session.locked': '1'});
    await tester.pump();
    expect(find.byKey(const ValueKey('lock screen')), findsOneWidget);
    expect(find.text('09:05'), findsOneWidget);
    expect(find.text('Anna Rossi'), findsOneWidget);
    announce({'session.locked': '0'});
    await tester.pump();
    expect(find.byKey(const ValueKey('lock screen')), findsNothing);
  });

  testWidgets('a wrong password shakes and says so; the right one is sent', (
    tester,
  ) async {
    final source = FakeLock();
    await pump(tester, source);
    announce({'session.locked': '1'});
    await tester.pump();
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
    // The compositor unlocks and announces it: the screen goes.
    announce({'session.locked': '0'});
    await tester.pump();
    expect(find.byKey(const ValueKey('lock screen')), findsNothing);
  });

  testWidgets('a password that cannot be checked is told apart', (
    tester,
  ) async {
    final source = FakeLock()..throws = true;
    await pump(tester, source);
    announce({'session.locked': '1'});
    await tester.pump();
    await tester.pump();
    await tester.enterText(find.byKey(const ValueKey('lock password')), 'x');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(find.text('The password could not be checked.'), findsOneWidget);
  });

  testWidgets('the screen saver of the state is drawn and ends with it', (
    tester,
  ) async {
    await pump(tester, FakeLock());
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
    await pump(tester, FakeLock());
    announce({'screensaver.active': 'clock'});
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('09:05'), findsOneWidget);
  });
}
