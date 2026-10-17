import 'dart:math' as math;

import 'dart:async';

import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/notifications/model.dart';
import 'package:silica_shell/system/glass_profile.dart';
import 'package:silica_shell/notifications/notifications.dart';
import 'package:silica_shell/notifications/service.dart';
import 'package:silica_shell/notifications/settings.dart';
import 'package:silica_shell/system/system_status.dart';

class FakeSource extends ChangeNotifier implements NotificationsSource {
  final Map<int, ShellNotification> items = {};
  final StreamController<ShellNotification> _posted =
      StreamController.broadcast(sync: true);
  final List<String> calls = [];

  void post(ShellNotification n) {
    items[n.id] = n;
    _posted.add(n);
    notifyListeners();
  }

  @override
  Map<int, ShellNotification> get notifications => items;

  @override
  Stream<ShellNotification> get posted => _posted.stream;

  @override
  Future<void> dismiss(int id) async {
    calls.add('dismiss $id');
    items.remove(id);
    notifyListeners();
  }

  @override
  Future<void> invoke(int id, String action) async =>
      calls.add('invoke $id $action');

  @override
  Future<void> reply(int id, String text) async => calls.add('reply $id $text');

  @override
  Future<void> clear() async {
    calls.add('clear');
    items.clear();
    notifyListeners();
  }
}

ShellNotification note(
  int id, {
  String app = 'Mail',
  int minute = 0,
  List<(String, String)> actions = const [],
  int urgency = 1,
  bool transient = false,
  String entry = '',
}) => ShellNotification(
  id: id,
  appName: app,
  desktopEntry: entry,
  summary: 'Titolo $id',
  body: 'Corpo della notifica $id',
  actions: actions,
  urgency: urgency,
  transient: transient,
  timestamp: DateTime(2026, 10, 13, 9, minute),
);

Future<void> pumpLayer(
  WidgetTester tester,
  FakeSource source, {
  NotificationSettings settings = const NotificationSettings(),
}) async {
  tester.view.physicalSize = const Size(1280, 686);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: NotificationsLayer(
        source: source,
        settings: () => settings,
        openApp: (n) => source.calls.add('open ${n.appName}'),
      ),
    ),
  );
}

Future<void> sendCommand(WidgetTester tester, String id) =>
    tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      'silica/shell',
      const JSONMethodCodec().encodeMethodCall(MethodCall('command', id)),
      (_) {},
    );

void main() {
  test('the D-Bus structure becomes a notification', () {
    final n = ShellNotification.fromDBus([
      7, 'Mail', 'mail', 'Titolo', 'Corpo', //
      ['default', 'Open', 'reply', 'Rispondi'],
      2, '', 'org.example.Mail', '', true, false, 1791878005135, -1,
    ])!;
    expect(n.id, 7);
    expect(n.critical, isTrue);
    expect(n.transient, isTrue);
    expect(n.hasDefault, isTrue);
    expect(n.buttons, [('reply', 'Rispondi')]);
    expect(n.group, 'org.example.Mail');
    expect(ShellNotification.fromDBus([1, 2]), isNull);
    expect(ShellNotification.fromDBus('x'), isNull);
  });

  test(
    'the Center groups by app, from the most recent, without the transient ones',
    () {
      final groups = groupNotifications([
        note(1, minute: 1),
        note(2, app: 'Messages', minute: 2),
        note(3, minute: 3),
        note(4, minute: 4, transient: true),
      ]);
      expect([for (final g in groups) g.key], ['Mail', 'Messages']);
      expect([for (final n in groups.first.items) n.id], [3, 1]);
    },
  );

  test('the banner entrance is the measured exponential, tau 71 ms', () {
    expect(bannerEnterOffset(0), bannerEnterDistance);
    // The remaining fraction is the reference's at any distance: exp(-t / 71).
    for (final ms in [13.0, 40.0, 71.0, 142.0, 213.0, 300.0]) {
      expect(
        bannerEnterOffset(ms) / bannerEnterDistance,
        closeTo(math.exp(-ms / 71), 1e-9),
        reason: '$ms ms',
      );
    }
    // No overshoot: monotone to 0, and within 1 pt in about 400 ms.
    var last = bannerEnterDistance;
    for (var ms = 10.0; ms < 500; ms += 10) {
      final x = bannerEnterOffset(ms);
      expect(x, lessThanOrEqualTo(last));
      expect(x, greaterThanOrEqualTo(0));
      last = x;
    }
    expect(bannerEnterOffset(bannerEnterDuration.inMilliseconds.toDouble()), 0);
    expect(bannerEnterDuration.inMilliseconds, inInclusiveRange(440, 480));
  });

  testWidgets('a banner enters at the top right and leaves by itself', (
    tester,
  ) async {
    final source = FakeSource();
    await pumpLayer(tester, source);
    source.post(note(1));
    await tester.pump();
    await tester.pump(bannerEnterDuration);
    final card = find.byType(NotificationCard);
    expect(card, findsOneWidget);
    expect(tester.getTopLeft(card), const Offset(1280 - 16 - 360, 32 + 16));
    expect(tester.getSize(card), const Size(360, 76));
    expect(find.text('Titolo 1'), findsOneWidget);

    await tester.pump(bannerDuration);
    await tester.pumpAndSettle();
    expect(find.byType(NotificationCard), findsNothing);
    // It stays in the Center.
    expect(source.items, contains(1));
  });

  testWidgets('critical notifications stay, a click activates the default', (
    tester,
  ) async {
    final source = FakeSource();
    await pumpLayer(tester, source);
    source.post(note(2, urgency: 2, actions: [('default', 'Open')]));
    await tester.pump();
    await tester.pump(bannerEnterDuration);
    await tester.pump(bannerDuration * 3);
    expect(find.text('Titolo 2'), findsOneWidget);
    await tester.tap(find.text('Titolo 2'));
    await tester.pumpAndSettle();
    expect(source.calls, ['invoke 2 default']);
    expect(find.text('Titolo 2'), findsNothing);
  });

  testWidgets('the Center shows stacked groups, actions and clear all', (
    tester,
  ) async {
    final source = FakeSource()
      ..items[1] = note(1, minute: 1)
      ..items[2] = note(2, minute: 2)
      ..items[3] = note(
        3,
        app: 'Calendar',
        minute: 3,
        actions: [('default', 'Open'), ('snooze', 'Posticipa')],
      );
    await pumpLayer(tester, source);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();

    expect(find.text('Notifications'), findsOneWidget);
    // Calendario is the most recent; Posta is collapsed with a card below.
    expect(find.text('Titolo 3'), findsOneWidget);
    expect(find.text('Titolo 2'), findsOneWidget);
    expect(find.text('Titolo 1'), findsNothing);
    expect(find.text('09:03'), findsOneWidget);
    expect(
      tester.getTopLeft(find.text('Titolo 3')).dy,
      lessThan(tester.getTopLeft(find.text('Titolo 2')).dy),
    );
    // The panel (380 wide, 16 pt from the right edge) has the header on top, then the
    // section label and the cards.
    final header = tester.getTopLeft(find.text('Notifications')).dy;
    expect(tester.getTopLeft(find.text('Earlier')).dy, greaterThan(header));
    expect(
      tester.getTopLeft(find.text('Titolo 3')).dy,
      greaterThan(tester.getTopLeft(find.text('Earlier')).dy),
    );
    expect(find.text('Clear all'), findsOneWidget);
    // The cards of the panel are 348 wide (opaque, radius 16).
    expect(
      tester.getSize(find.byType(NotificationCard).first),
      const Size(348, 108),
    );

    await tester.tap(find.text('Posticipa'));
    expect(source.calls, ['invoke 3 snooze']);

    // A click on the group opens it.
    await tester.tap(find.text('Titolo 2'));
    await tester.pumpAndSettle();
    expect(find.text('Titolo 1'), findsOneWidget);

    await tester.tap(find.text('Clear all'));
    await tester.pumpAndSettle();
    expect(source.calls.last, 'clear');
    expect(find.text('No Recent Notifications'), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.text('Notifications'), findsNothing);
  });

  testWidgets('an expanded group has a header with Collapse and Clear', (
    tester,
  ) async {
    final source = FakeSource()
      ..items[1] = note(1, minute: 1)
      ..items[2] = note(2, minute: 2);
    await pumpLayer(tester, source);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    await tester.tap(find.text('Titolo 2'));
    await tester.pumpAndSettle();
    expect(find.text('Collapse'), findsOneWidget);
    await tester.tap(find.text('Clear'));
    await tester.pumpAndSettle();
    expect(source.items, isEmpty);
  });

  test('per-app styles from notifications.json', () {
    final settings = NotificationSettings.parse(
      '{"apps": {"org.example.Chat": {"style": "alerts"}, '
      '"Mail": {"style": "none"}, "Bad": {"style": "loud"}}}',
    );
    expect(
      settings.styleOf(note(1, app: 'Chat', entry: 'org.example.Chat')),
      NotificationStyle.alerts,
    );
    expect(settings.styleOf(note(2, app: 'Mail')), NotificationStyle.none);
    expect(settings.styleOf(note(3, app: 'Bad')), NotificationStyle.banners);
    expect(
      NotificationSettings.parse('not json').styleOf(note(4)),
      NotificationStyle.banners,
    );
  });

  testWidgets('alerts stay with the × visible; style none shows no banner', (
    tester,
  ) async {
    final source = FakeSource();
    await pumpLayer(
      tester,
      source,
      settings: const NotificationSettings({
        'Chat': NotificationStyle.alerts,
        'Quiet': NotificationStyle.none,
      }),
    );
    source.post(note(1, app: 'Quiet'));
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.text('Titolo 1'), findsNothing);
    source.post(note(2, app: 'Chat'));
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.text('Titolo 2'), findsOneWidget);
    expect(find.byType(CloseCircle), findsOneWidget);
    // Well past the banner duration it is still there.
    await tester.pump(const Duration(seconds: 12));
    expect(find.text('Titolo 2'), findsOneWidget);
  });

  testWidgets('a card dragged right in the Center leaves and is cleared', (
    tester,
  ) async {
    final source = FakeSource()..items[1] = note(1, minute: 1);
    await pumpLayer(tester, source);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    await tester.drag(find.text('Titolo 1'), const Offset(40, 0));
    await tester.pumpAndSettle();
    expect(source.calls, ['dismiss 1']);
  });

  testWidgets('a long press opens the banner: its actions work and it leaves', (
    tester,
  ) async {
    final source = FakeSource();
    await pumpLayer(tester, source);
    source.post(note(1, actions: [('default', 'Open'), ('snooze', 'Snooze')]));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 600));
    expect(find.text('Snooze'), findsNothing);
    await tester.longPress(find.text('Titolo 1'));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('Snooze'), findsOneWidget);
    await tester.tap(find.text('Snooze'));
    await tester.pumpAndSettle();
    expect(source.calls, ['invoke 1 snooze']);
    expect(find.text('Titolo 1'), findsNothing);
  });

  testWidgets('the reply field sends its text (inline-reply)', (tester) async {
    final source = FakeSource()
      ..items[1] = note(
        1,
        minute: 1,
        actions: [('inline-reply', 'Reply to Anna')],
      );
    await pumpLayer(tester, source);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    expect(find.text('Reply to Anna'), findsOneWidget);
    // inline-reply is a field, never a button.
    expect(source.items[1]!.buttons, isEmpty);
    await tester.tap(find.byType(EditableText));
    await tester.enterText(find.byType(EditableText), 'on my way');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(source.calls, ['reply 1 on my way']);
  });

  testWidgets('a click without a default action opens the sender app', (
    tester,
  ) async {
    final source = FakeSource()..items[1] = note(1, app: 'Chat', minute: 1);
    await pumpLayer(tester, source);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    await tester.tap(find.text('Titolo 1'));
    await tester.pumpAndSettle();
    expect(source.calls, ['open Chat', 'dismiss 1']);
    expect(find.text('Notifications'), findsNothing);
  });

  testWidgets('"Clear all" in the panel header clears every notification', (
    tester,
  ) async {
    final source = FakeSource()
      ..items[1] = note(1, minute: 1)
      ..items[2] = note(2, app: 'Calendar', minute: 2);
    await pumpLayer(tester, source);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    expect(source.calls, isEmpty);
    await tester.tap(find.text('Clear all'));
    await tester.pumpAndSettle();
    expect(source.calls, ['clear']);
    expect(find.text('No Recent Notifications'), findsOneWidget);
  });

  testWidgets('empty Centre: the panel keeps its title and says so', (
    tester,
  ) async {
    final source = FakeSource();
    await pumpLayer(tester, source);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    // 380 wide Glass/Thick panel at the top right, 16 pt from the edge and from the bar.
    final title = tester.getTopLeft(find.text('Notifications'));
    expect(title.dx, 1280 - 16 - 380 + 16 + 4);
    expect(title.dy, greaterThan(30 + 16));
    expect(find.text('No Recent Notifications'), findsOneWidget);
    // Nothing to clear: no "Clear all", and no pill that does nothing.
    expect(find.text('Clear all'), findsNothing);
    expect(find.text('Choose Widgets'), findsNothing);
  });

  testWidgets('the panel is Glass/Thick (panel) and the cards are opaque', (
    tester,
  ) async {
    final source = FakeSource()..items[1] = note(1, minute: 1);
    await pumpLayer(tester, source);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    final glass = tester.widgetList<ProfiledGlass>(find.byType(ProfiledGlass));
    expect(glass, hasLength(1));
    expect(glass.single.surface, ShellGlassProfile.panel);
    expect(glass.single.radius, centreRadius);
    expect(tester.getSize(find.byType(ProfiledGlass)).width, 380);
    expect(tester.getSize(find.byType(NotificationCard)).width, 348);
  });

  testWidgets('a banner is Glass/Regular, radius 18', (tester) async {
    final source = FakeSource();
    await pumpLayer(tester, source);
    source.post(note(1));
    await tester.pump();
    await tester.pump(bannerEnterDuration);
    final glass = tester.widget<ProfiledGlass>(find.byType(ProfiledGlass));
    expect(glass.surface, ShellGlassProfile.banner);
    expect(glass.radius, 18);
    // With actions the banner is 108 tall (the kit's Banner/With Actions).
    await tester.longPress(find.text('Titolo 1'));
    await tester.pump(const Duration(milliseconds: 100));
  });

  testWidgets(
    'with Do Not Disturb no banner, the notification stays in the Center',
    (tester) async {
      final source = FakeSource();
      final system = _Dnd();
      tester.view.physicalSize = const Size(1280, 686);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: NotificationsLayer(source: source, system: system),
        ),
      );
      source.post(note(5));
      await tester.pumpAndSettle();
      expect(find.byType(NotificationCard), findsNothing);
      expect(source.items, contains(5));
    },
  );

  test('banner and Centre motion are the measured curves', () {
    // Banner in: opacity over ~210 ms, blur sharp in ~215 ms.
    expect(bannerEnterOpacity(0), 0);
    expect(bannerEnterOpacity(105), closeTo(0.5, 1e-9));
    expect(bannerEnterOpacity(210), 1);
    expect(bannerEnterBlur(0), bannerBlurSigma);
    expect(bannerEnterBlur(215), 0);
    // Banner out: ~190 ms, offset at 13.3 ms steps (scaled to the Amalfi distance), opacity
    // down to half, growing blur.
    expect(bannerExitDuration, const Duration(milliseconds: 190));
    const k = bannerEnterDistance / 360;
    for (final (i, pt) in bannerExitSteps.indexed) {
      expect(
        bannerExitOffset(i * 1000 / 75),
        closeTo(k * pt, 1e-6),
        reason: 'step $i',
      );
    }
    expect(bannerExitOpacity(0), 1);
    expect(bannerExitOpacity(190), closeTo(0.5, 1e-9));
    expect(bannerExitBlur(190), bannerBlurSigma);
    // Centre in: almost all of it in ~35 ms, a long tail up to ~235 ms.
    expect(centerOpenOffset(0), centerDistance);
    expect(
      centerOpenOffset(35) / centerDistance,
      closeTo((347 * math.exp(-35 / 12) + 40 * math.exp(-35 / 70)) / 387, 1e-9),
    );
    expect(centerOpenOffset(35), lessThan(0.15 * centerDistance));
    expect(centerOpenDuration, const Duration(milliseconds: 235));
    expect(centerOpenOffset(235), 0);
    // Centre out: ~300 ms, slight slow shift then a fast exit and a tail.
    expect(centerCloseDuration, const Duration(milliseconds: 300));
    expect(centerCloseOffset(0), 0);
    expect(centerCloseOffset(60) / centerDistance, closeTo(10 / 387, 1e-9));
    expect(centerCloseOffset(150) / centerDistance, closeTo(357 / 387, 1e-9));
    expect(centerCloseOffset(300), closeTo(centerDistance, 1e-9));
  });

  testWidgets(
    'a new notification replaces the banner: the old one leaves while it enters',
    (tester) async {
      final source = FakeSource();
      await pumpLayer(tester, source);
      source.post(note(1));
      await tester.pump();
      await tester.pump(bannerEnterDuration);
      source.post(note(2));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 60));
      // For a moment both are there.
      expect(find.text('Titolo 1'), findsOneWidget);
      expect(find.text('Titolo 2'), findsOneWidget);
      await tester.pump(bannerExitDuration);
      await tester.pump();
      expect(find.text('Titolo 1'), findsNothing);
      expect(find.text('Titolo 2'), findsOneWidget);
    },
  );

  testWidgets('under the pointer the banner stays, then leaves ~1.1 s later', (
    tester,
  ) async {
    final source = FakeSource();
    await pumpLayer(tester, source);
    source.post(
      note(3, actions: [('default', 'Open'), ('snooze', 'Posticipa')]),
    );
    await tester.pump();
    await tester.pump(bannerEnterDuration);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.text('Titolo 3')));
    await tester.pump();
    // On hover: the × and the Dettagli capsule.
    expect(find.byType(CloseCircle), findsOneWidget);
    expect(find.text('Details'), findsOneWidget);
    await tester.pump(bannerDuration * 2);
    expect(find.text('Titolo 3'), findsOneWidget);

    // Dettagli shows the actions in the banner.
    await tester.tap(find.text('Details'));
    await tester.pump();
    expect(find.text('Posticipa'), findsOneWidget);

    await mouse.moveTo(const Offset(10, 600));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 1000));
    expect(find.text('Titolo 3'), findsOneWidget);
    await tester.pump(const Duration(milliseconds: 200));
    await tester.pumpAndSettle();
    expect(find.text('Titolo 3'), findsNothing);
  });

  testWidgets(
    'dragged to the right it leaves anyway, without opening the app',
    (tester) async {
      final source = FakeSource();
      await pumpLayer(tester, source);
      source.post(note(4, actions: [('default', 'Open')]));
      await tester.pump();
      await tester.pump(bannerEnterDuration);
      final start = tester.getTopLeft(find.byType(NotificationCard));
      final drag = await tester.startGesture(
        tester.getCenter(find.text('Titolo 4')),
      );
      await drag.moveBy(const Offset(20, 0));
      await drag.moveBy(const Offset(15, 0));
      await tester.pump();
      // It follows the pointer 1:1 (beyond the recognizer's threshold).
      expect(
        tester.getTopLeft(find.byType(NotificationCard)).dx - start.dx,
        greaterThan(14),
      );
      await drag.up();
      await tester.pumpAndSettle();
      expect(find.text('Titolo 4'), findsNothing);
      expect(source.calls, isEmpty);
      expect(source.items, contains(4));
    },
  );
  test('previews off hide the text, per app and for all', () {
    final all = NotificationSettings.parse('{"previews": "never"}');
    expect(all.present(note(1)).body, isEmpty);
    expect(all.present(note(1)).summary, 'Titolo 1');
    final perApp = NotificationSettings.parse(
      '{"previews": "never", "apps": {"Mail": {"previews": "always"}, '
      '"Chat": {"previews": "never"}}}',
    );
    expect(perApp.present(note(1)).body, isNotEmpty);
    expect(perApp.present(note(2, app: 'Chat')).body, isEmpty);
    expect(perApp.present(note(3, app: 'Other')).body, isEmpty);
    // No lock screen: unlocked previews are always shown.
    final unlocked = NotificationSettings.parse('{"previews": "unlocked"}');
    expect(unlocked.present(note(1)).body, isNotEmpty);
    expect(const NotificationSettings().present(note(1)).body, isNotEmpty);
  });

  test('showInCenter and grouping per app', () {
    final settings = NotificationSettings.parse(
      '{"grouping": "off", "apps": {"Mail": {"showInCenter": false, '
      '"grouping": "app"}, "Chat": {"showInCenter": true}}}',
    );
    expect(settings.inCenter(note(1)), isFalse);
    expect(settings.inCenter(note(2, app: 'Chat')), isTrue);
    expect(settings.groupingOf(note(1)), GroupingMode.app);
    expect(settings.groupingOf(note(2, app: 'Chat')), GroupingMode.off);
    final groups = groupNotifications([
      note(1, minute: 1),
      note(2, minute: 2),
    ], ungrouped: (_) => true);
    expect(groups.length, 2);
  });

  testWidgets('the banner has no text when previews are off; the Center skips '
      'the apps that opt out and lists each notification when not grouping', (
    tester,
  ) async {
    final source = FakeSource()
      ..items[1] = note(1, app: 'Chat', minute: 1)
      ..items[2] = note(2, app: 'Chat', minute: 2)
      ..items[3] = note(3, app: 'Mail', minute: 3);
    await pumpLayer(
      tester,
      source,
      settings: NotificationSettings.parse(
        '{"previews": "never", "apps": {"Mail": {"showInCenter": false}, '
        '"Chat": {"grouping": "off"}}}',
      ),
    );
    source.post(note(4, app: 'Chat', minute: 4));
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.text('Titolo 4'), findsOneWidget);
    expect(find.text('Corpo della notifica 4'), findsNothing);
    await sendCommand(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    expect(find.text('Titolo 3'), findsNothing);
    expect(find.text('Titolo 2'), findsOneWidget);
    expect(find.text('Titolo 1'), findsOneWidget);
  });
}

class _Dnd extends ValueNotifier<SystemSnapshot> implements SystemSource {
  _Dnd() : super(const SystemSnapshot(doNotDisturb: true));

  @override
  Future<void> setWifi(bool enabled) async {}
  @override
  Future<void> setBluetooth(bool powered) async {}
  @override
  Future<void> setBrightness(double v) async {}
  @override
  Future<void> setVolume(double v) async {}
  @override
  Future<void> setMuted(bool muted) async {}
  @override
  Future<void> setDoNotDisturb(bool enabled) async {}
  @override
  Future<void> setNightLight(bool enabled) async {}
  @override
  Future<List<WifiNetwork>> wifiNetworks() async => const [];
  @override
  Future<void> connectWifi(String ssid) async {}
  @override
  Future<List<AudioOutput>> audioOutputs() async => const [];
  @override
  Future<void> setAudioOutput(int id) async {}
  @override
  Future<List<BluetoothDevice>> bluetoothDevices() async => const [];
  @override
  Future<void> connectBluetooth(String address, bool connect) async {}
}
