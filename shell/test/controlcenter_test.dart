import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/controlcenter/controlcenter.dart';
import 'package:silica_shell/controlcenter/detail.dart'
    show
        detailBackStretch,
        detailProgress,
        detailTimeScale,
        detailWidth,
        othersOpacity;
import 'package:silica_shell/controlcenter/edit.dart';
import 'package:silica_shell/controlcenter/modules.dart';
import 'package:silica_shell/system/desktop_entries.dart';
import 'package:silica_shell/system/glass_profile.dart';
import 'package:silica_shell/system/shell_channel.dart';
import 'package:silica_shell/system/system_status.dart';
import 'package:silica_ui/silica_ui.dart';

import 'support/settings.dart';

class FakeSystem extends ValueNotifier<SystemSnapshot> implements SystemSource {
  FakeSystem(super.value);

  final List<String> calls = [];

  @override
  Future<void> setWifi(bool enabled) async {
    calls.add('wifi $enabled');
    value = value.copyWith(wifiEnabled: enabled);
  }

  @override
  Future<void> setBluetooth(bool powered) async => calls.add('bt $powered');

  @override
  Future<void> setBrightness(double v) async =>
      calls.add('brightness ${v.toStringAsFixed(2)}');

  @override
  Future<void> setVolume(double v) async =>
      calls.add('volume ${v.toStringAsFixed(2)}');

  @override
  Future<void> setMuted(bool muted) async => calls.add('muted $muted');

  @override
  Future<void> setDoNotDisturb(bool enabled) async => calls.add('dnd $enabled');

  @override
  Future<void> setNightLight(bool enabled) async {
    calls.add('nightlight $enabled');
    value = value.copyWith(nightLight: enabled);
  }

  List<WifiNetwork> networks = const [
    WifiNetwork(
      ssid: 'Casa',
      strength: 70,
      secure: true,
      known: true,
      active: true,
    ),
    WifiNetwork(ssid: 'Ufficio', strength: 40, secure: true, known: true),
    WifiNetwork(ssid: 'Bar', strength: 90),
  ];

  @override
  Future<List<WifiNetwork>> wifiNetworks() async => networks;

  @override
  Future<void> connectWifi(String ssid) async => calls.add('connect $ssid');

  @override
  Future<List<AudioOutput>> audioOutputs() async => const [
    AudioOutput(id: 74, name: 'Altoparlanti', isDefault: true),
    AudioOutput(id: 62, name: 'Cuffie'),
  ];

  @override
  Future<void> setAudioOutput(int id) async => calls.add('output $id');

  @override
  Future<List<BluetoothDevice>> bluetoothDevices() async => const [
    BluetoothDevice(
      address: 'AA:01',
      name: 'Headphones',
      icon: 'audio-headset',
      connected: true,
    ),
    BluetoothDevice(address: 'AA:02', name: 'Mouse', icon: 'input-mouse'),
  ];

  @override
  Future<void> connectBluetooth(String address, bool connect) async =>
      calls.add('bt-connect $address $connect');
}

const SystemSnapshot laptop = SystemSnapshot(
  wifiAvailable: true,
  wifiEnabled: true,
  wifiSsid: 'Casa',
  bluetoothAvailable: true,
  bluetoothPowered: true,
  batteryPresent: true,
  batteryPercent: 80,
  batteryCharging: true,
  brightnessAvailable: true,
  brightness: 0.5,
  audioAvailable: true,
  volume: 0.46,
);

/// Chooses a menu item: a click on it, then the blink and the fade.
Future<void> choose(WidgetTester tester, String label) async {
  await tester.tapAt(tester.getCenter(find.text(label)));
  for (var i = 0; i < 6; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

Future<void> sendCommand(WidgetTester tester, String id) =>
    tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      'silica/shell',
      const JSONMethodCodec().encodeMethodCall(MethodCall('command', id)),
      (_) {},
    );

/// No arrangement file: the default modules, nothing saved.
const Map<String, String> noConfig = {
  "XDG_CONFIG_HOME": "/nonexistent/silica-test",
};

void main() {
  test(
    'the grid follows the measurements: columns at 0 and 152, rows at a 76 pitch',
    () {
      // The reference's default (INV-A2): the 76 pt pitch from the top-left of the stack
      // (Wi-Fi at 2254,42 without the privacy pill).
      final slots = layoutModules(laptop);
      expect(slots['wifi']!.rect, const Rect.fromLTWH(0, 0, 140, 64));
      expect(slots['bluetooth']!.rect, const Rect.fromLTWH(0, 76, 140, 64));
      expect(slots['media']!.rect, const Rect.fromLTWH(152, 0, 140, 140));
      expect(slots['nearby']!.rect, const Rect.fromLTWH(0, 152, 140, 64));
      expect(slots['staging']!.rect, const Rect.fromLTWH(152, 152, 64, 64));
      expect(slots['mirroring']!.rect, const Rect.fromLTWH(228, 152, 64, 64));
      expect(slots['dark']!.rect, const Rect.fromLTWH(0, 228, 64, 64));
      expect(slots['capture']!.rect, const Rect.fromLTWH(76, 228, 64, 64));
      expect(slots['dnd']!.rect, const Rect.fromLTWH(152, 228, 140, 64));
      expect(slots['display']!.rect, const Rect.fromLTWH(0, 304, 292, 64));
      expect(slots['sound']!.rect, const Rect.fromLTWH(0, 380, 292, 64));
      expect(slots['wifi']!.radius, 32);
      expect(slots['staging']!.radius, 32);
      expect(slots['media']!.radius, 36);
      expect(slots['display']!.radius, 28);
      // The battery is not in the default; Display is there without a backlight too
      // (its track then shows no fill, controlcenter.md › sliders).
      expect(slots.containsKey('battery'), isFalse);
      final desktop = layoutModules(const SystemSnapshot(audioAvailable: true));
      expect(desktop['display']!.rect.top, 304);

      // A user's arrangement: modules pack in order at the first free spot; the battery
      // only shows on a machine with one.
      final mine = layoutModules(laptop, const [
        CcModule('dark', ModuleSize.small),
        CcModule('wifi', ModuleSize.medium),
        CcModule('battery', ModuleSize.medium),
        CcModule('capture', ModuleSize.small),
        CcModule('sound', ModuleSize.wide),
      ]);
      expect(mine['dark']!.rect, const Rect.fromLTWH(0, 0, 64, 64));
      expect(mine['wifi']!.rect, const Rect.fromLTWH(76, 0, 140, 64));
      expect(mine['battery']!.rect, const Rect.fromLTWH(0, 76, 140, 64));
      expect(mine['capture']!.rect, const Rect.fromLTWH(228, 0, 64, 64));
      expect(mine['sound']!.rect, const Rect.fromLTWH(0, 152, 292, 64));
      final noBattery = layoutModules(const SystemSnapshot(), const [
        CcModule('battery', ModuleSize.medium),
      ]);
      expect(noBattery, isEmpty);
    },
  );

  test('the panel opens and closes on the measured opacity tables and scale', () {
    // controlcenter.md "Animations": the table's first point is one frame (13 ms) earlier
    // than the spec's t = 0.
    const open = {
      0: 0.09,
      13: 0.16,
      27: 0.25,
      53: 0.44,
      67: 0.53,
      80: 0.64,
      107: 0.74,
      160: 0.91,
      213: 0.97,
      267: 0.99,
    };
    for (final MapEntry(:key, :value) in open.entries) {
      expect(
        openOpacityAt(key + 13.0),
        closeTo(value, 0.012),
        reason: '$key ms',
      );
    }
    expect(openOpacityAt(0), 0);
    expect(openOpacityAt(1000), 1);
    const close = {
      0: 0.92,
      13: 0.86,
      40: 0.61,
      53: 0.49,
      67: 0.38,
      93: 0.24,
      120: 0.14,
      160: 0.07,
      200: 0.02,
      280: 0.0,
    };
    for (final MapEntry(:key, :value) in close.entries) {
      expect(
        closeOpacityAt(key + 13.0),
        closeTo(value, 0.012),
        reason: '$key ms',
      );
    }
    expect(closeOpacityAt(0), 1);
    // Scale: 1.0075 -> 1 in ~107 ms opening, 1 -> 1.0075 in ~120 ms closing; no height spring.
    expect(panelScaleAt(0, opening: true), closeTo(1.0075, 1e-9));
    expect(panelScaleAt(107, opening: true), 1);
    expect(panelScaleAt(500, opening: true), 1);
    expect(panelScaleAt(0, opening: false), 1);
    expect(panelScaleAt(120, opening: false), closeTo(1.0075, 1e-9));
    expect(openDuration, const Duration(milliseconds: 310));
    expect(closeDuration, const Duration(milliseconds: 293));
  });

  test(
    'the detail morph is the measured slow-fast-slow curve, no overshoot',
    () {
      // controlcenter.md "Module to detail", Display (time scale 1).
      const p = {
        0: 0.0,
        13: 0.08,
        26: 0.19,
        40: 0.31,
        66: 0.54,
        80: 0.63,
        120: 0.82,
        173: 0.94,
        253: 0.99,
        333: 1.0,
      };
      for (final MapEntry(:key, :value) in p.entries) {
        expect(
          detailProgress(key.toDouble(), 1),
          closeTo(value, 0.01),
          reason: '$key ms',
        );
      }
      var peak = 0.0;
      for (var ms = 0.0; ms <= 400; ms += 1) {
        peak = peak > detailProgress(ms, 1) ? peak : detailProgress(ms, 1);
      }
      expect(peak, 1);
      // Nearer modules take 0.65 of the time (Wi-Fi ~215 ms), Display 1.
      expect(detailTimeScale(0, 400), closeTo(0.65, 1e-9));
      expect(detailTimeScale(400, 400), 1);
      expect(detailProgress(215, detailTimeScale(0, 400)), closeTo(0.98, 0.02));
      // The other modules: 0.56 at 67 ms, 0.28 at 107 ms, gone at 240 ms.
      expect(othersOpacity(0), 1);
      expect(othersOpacity(67), closeTo(0.56, 1e-9));
      expect(othersOpacity(107), closeTo(0.28, 1e-9));
      expect(othersOpacity(240), 0);
      expect(detailBackStretch, 1.35);
    },
  );

  test(
    'the module hover is the measured curve: 10 % at 50 ms, half at 150, done at 300',
    () {
      const c = HoverCurve();
      expect(hoverDuration, const Duration(milliseconds: 300));
      expect(c.transform(50 / 300), closeTo(0.1, 1e-9));
      expect(c.transform(150 / 300), closeTo(0.5, 1e-9));
      expect(c.transform(1), 1);
    },
  );

  testWidgets('opens from the command, the modules act, Esc closes', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final system = FakeSystem(laptop);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: ControlCenter(env: noConfig, system: system),
      ),
    );
    expect(find.text('Wi-Fi'), findsNothing);

    await sendCommand(tester, 'controlcenter.toggle');
    await tester.pumpAndSettle();
    expect(find.text('Casa'), findsOneWidget);
    // The stack: 14 pt from the right edge, the first module at y 42.
    final wifi = tester.getRect(
      find
          .ancestor(of: find.text('Wi-Fi'), matching: find.byType(Positioned))
          .first,
    );
    expect(wifi.right, lessThanOrEqualTo(1280 - 14 - 152));
    expect(tester.getTopLeft(find.text('Wi-Fi')).dx, 1280 - 14 - 292 + 60);

    // The icon disc turns off; the text opens the detail.
    final wifiText = tester.getTopLeft(find.text('Wi-Fi'));
    await tester.tapAt(wifiText + const Offset(-60 + 16 + 18, 14));
    await tester.pump();
    expect(system.calls, ['wifi false']);
    expect(find.text('Off'), findsWidgets);
    await tester.tapAt(wifiText + const Offset(-60 + 16 + 18, 14));
    await tester.pump();
    expect(system.calls.last, 'wifi true');

    // A tap a quarter along the Sound slider (thumb centres run from 10 to length - 10).
    final sound = tester.getTopLeft(find.text('Sound'));
    // The Sound track is 48 pt shorter: the outputs disc is on the right.
    await tester.tapAt(
      Offset(
        sound.dx -
            16 +
            trackLeft +
            10 +
            0.25 * (trackLength - outputsShift - 20),
        sound.dy - 8 + trackCenter,
      ),
    );
    expect(system.calls.last, 'volume 0.25');
    expect(find.byKey(const ValueKey('audio outputs')), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.text('Wi-Fi'), findsNothing);
  });

  testWidgets(
    'the text opens the Wi-Fi detail; Esc goes back to the Center, then closes',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final system = FakeSystem(laptop);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ControlCenter(env: noConfig, system: system),
        ),
      );
      await sendCommand(tester, 'controlcenter.toggle');
      await tester.pumpAndSettle();
      await tester.tapAt(tester.getCenter(find.text('Wi-Fi')));
      await tester.pumpAndSettle();
      // The panel: 312 pt, 14 left of the stack, 6 above; the known networks.
      expect(find.text('Known networks'), findsOneWidget); // Home and Office
      expect(find.text('Ufficio'), findsOneWidget);
      expect(find.text('Bar'), findsNothing);
      final title = tester.getTopLeft(
        find.byWidgetPredicate(
          (w) => w is Text && w.data == 'Wi-Fi' && w.style?.fontSize == 16,
        ),
      );
      expect(title.dx, 1280 - 14 - 292 - 14 + 16);
      // A known network connects; "Other networks" shows the others.
      await tester.tap(find.text('Ufficio'));
      expect(system.calls.last, 'connect Ufficio');
      await tester.tap(find.text('Other networks'));
      await tester.pumpAndSettle();
      expect(find.text('Bar'), findsOneWidget);
      // Esc: the Center again, with the modules.
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.text('Reti conosciute'), findsNothing);
      expect(find.text('Casa'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.text('Wi-Fi'), findsNothing);
    },
  );

  testWidgets('the Sound title opens the outputs', (tester) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final system = FakeSystem(laptop);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: ControlCenter(env: noConfig, system: system),
      ),
    );
    await sendCommand(tester, 'controlcenter.toggle');
    await tester.pumpAndSettle();
    await tester.tapAt(tester.getCenter(find.text('Sound')));
    await tester.pumpAndSettle();
    expect(find.text('Output'), findsOneWidget);
    await tester.tap(find.text('Cuffie'));
    await tester.pump();
    expect(system.calls.last, 'output 62');
  });

  testWidgets(
    'Display detail: the Night Light tile turns the night light on (#121)',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final system = FakeSystem(laptop);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ControlCenter(env: noConfig, system: system),
        ),
      );
      await sendCommand(tester, 'controlcenter.toggle');
      await tester.pumpAndSettle();
      await tester.tapAt(tester.getCenter(find.text('Display')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Night Light'));
      await tester.pumpAndSettle();
      expect(system.calls.last, 'nightlight true');
      expect(system.value.nightLight, isTrue);
    },
  );

  testWidgets('Bluetooth detail: the paired devices, a click connects', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final system = FakeSystem(laptop);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: ControlCenter(env: noConfig, system: system),
      ),
    );
    await sendCommand(tester, 'controlcenter.toggle');
    await tester.pumpAndSettle();
    await tester.tapAt(tester.getCenter(find.text('Bluetooth')));
    await tester.pumpAndSettle();
    expect(find.text('Headphones'), findsOneWidget);
    await tester.tap(find.text('Mouse'));
    await tester.pump();
    expect(system.calls.last, 'bt-connect AA:02 true');
  });

  testWidgets(
    'Focus: Do Not Disturb on shows the durations; Dark Mode toggles',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final calls = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ShellChannel.channel,
        (call) async {
          calls.add(call);
          return true;
        },
      );
      final system = FakeSystem(laptop.copyWith(doNotDisturb: true));
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ControlCenter(env: noConfig, system: system),
        ),
      );
      await sendCommand(tester, 'controlcenter.toggle');
      await tester.pumpAndSettle();
      expect(find.text('Do not disturb'), findsOneWidget);
      await tester.tapAt(tester.getCenter(find.text('Do not disturb')));
      await tester.pumpAndSettle();
      expect(find.text('Focus on'), findsOneWidget);
      expect(find.text('For 1 hour'), findsOneWidget);
      expect(find.text('Until this evening'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();

      await tester.tap(find.bySemanticsLabel('Dark Mode'));
      await tester.pump();
      expect(
        calls
            .where((c) => c.method == 'execute')
            .map((c) => (c.arguments as Map)['id']),
        contains('system.dark_toggle'),
      );
    },
  );

  testWidgets(
    'open, the Center takes the pointer only on its panel; a press outside ends it (#149)',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final layer = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        const MethodChannel('silica/layer', JSONMethodCodec()),
        (call) async {
          layer.add(call);
          return true;
        },
      );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ControlCenter(system: FakeSystem(laptop)),
        ),
      );
      await sendCommand(tester, 'controlcenter.toggle');
      await tester.pumpAndSettle();
      final transient = layer.lastWhere((c) => c.method == 'setTransient');
      final args = transient.arguments as Map;
      // A system panel (INPUT-01 (e)): nothing outside is blocked.
      expect(args['owner'], '');
      expect(args['ownerRule'], 'pass');
      expect(args['otherRule'], 'pass');
      expect(args['press'], 'pass_through');
      expect(args['scroll'], 'pass_through');
      final rect = (args['rects'] as List).single as List;
      expect(rect[0], greaterThan(900)); // on the right, not the whole screen
      final region = layer.lastWhere((c) => c.method == 'setInputRegion');
      expect((region.arguments as Map)['rects'], isNotNull);

      // Ossido: a press outside ended it.
      await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        'silica/shell',
        const JSONMethodCodec().encodeMethodCall(
          const MethodCall('transientDismissed'),
        ),
        (_) {},
      );
      await tester.pumpAndSettle();
      expect(find.text('Casa'), findsNothing);
      // The bar's toggle that follows the same press does not open it again.
      await sendCommand(tester, 'controlcenter.toggle');
      await tester.pumpAndSettle();
      expect(find.text('Casa'), findsNothing);
    },
  );

  test('"until this evening" ends at 19:00, or at midnight once past it', () {
    expect(untilEvening(DateTime(2026, 10, 14, 15)), const Duration(hours: 4));
    expect(untilEvening(DateTime(2026, 10, 14, 20)), const Duration(hours: 4));
  });

  testWidgets(
    'Wi-Fi status menu: below the icon, Esc closes it and announces it',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final broadcasts = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ShellChannel.channel,
        (call) async {
          if (call.method == 'broadcast') {
            final args = call.arguments as Map;
            broadcasts.add('${args['topic']}=${args['value']}');
          }
          return null;
        },
      );
      final system = FakeSystem(laptop);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ControlCenter(env: noConfig, system: system),
        ),
      );
      Future<void> announce(String value) =>
          tester.binding.defaultBinaryMessenger.handlePlatformMessage(
            'silica/shell',
            const JSONMethodCodec().encodeMethodCall(
              MethodCall('broadcast', {'topic': 'statusmenu', 'value': value}),
            ),
            (_) {},
          );
      await announce('wifi 1200');
      await tester.pump();
      await tester.pump();
      final menu = tester.getRect(
        find.byKey(const ValueKey('wifi status menu')),
      );
      expect(menu.top, statusMenuTop);
      expect(menu.right, 1200 + statusMenuRight);
      expect(menu.width, detailWidth);
      expect(find.text('Wi-Fi'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pump();
      expect(find.byKey(const ValueKey('wifi status menu')), findsNothing);
      expect(broadcasts, contains('statusmenu='));

      // The Focus moon opens the Focus panel the same way (SH-MB-26).
      system.value = system.value.copyWith(doNotDisturb: true);
      await announce('dnd 1100');
      await tester.pump();
      await tester.pump();
      final focus = tester.getRect(
        find.byKey(const ValueKey('dnd status menu')),
      );
      expect(focus.right, 1100 + statusMenuRight);
      expect(find.text('Focus on'), findsOneWidget);
    },
  );

  testWidgets(
    'media module: 30 × 30 buttons at a 40 pitch, glyphs and module hover (CC-04)',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ControlCenter(env: noConfig, system: FakeSystem(laptop)),
        ),
      );
      await sendCommand(tester, 'controlcenter.toggle');
      await tester.pumpAndSettle();
      final previous = tester.getRect(find.byKey(const ValueKey('precedente')));
      final play = tester.getRect(find.byKey(const ValueKey('riproduci')));
      final next = tester.getRect(find.byKey(const ValueKey('successivo')));
      expect(previous.size, const Size(30, 30));
      expect(play.center.dx - previous.center.dx, 40);
      expect(next.center.dx - play.center.dx, 40);
      // With no players everything is disabled: the glyph at the 35 % of Amalfi.
      double glyphOpacity() => tester
          .widget<Opacity>(
            find.descendant(
              of: find.byKey(const ValueKey('riproduci')),
              matching: find.byType(Opacity),
            ),
          )
          .opacity;
      expect(glyphOpacity(), AmalfiStates.disabledOpacity);
      // The buttons have no hover of their own: the pointer on one gives the module's.
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(play.center);
      await tester.pumpAndSettle();
      expect(glyphOpacity(), AmalfiStates.disabledOpacity);
      expect(hoverLift(tester), closeTo(1, 1e-6));
      await mouse.removePointer();
    },
  );

  testWidgets(
    'Edit Controls: badges remove, the gallery adds, the arrangement is saved',
    (tester) async {
      tester.view.physicalSize = const Size(1600, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await useSettings();
      final system = FakeSystem(laptop);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ControlCenter(env: noConfig, system: system),
        ),
      );
      await sendCommand(tester, 'controlcenter.toggle');
      await tester.pumpAndSettle();

      // The button: 116 × 24, centred under the stack, 16 pt below Sound.
      final button = tester.getRect(
        find
            .ancestor(
              of: find.text('Edit Controls'),
              matching: find.byType(Positioned),
            )
            .first,
      );
      expect(button.size, const Size(116, 24));
      // Sound ends at 42 + 380 + 64.
      expect(button.top, 42 + 380 + 64 + 16);
      expect(button.center.dx, closeTo(1600 - 14 - 146, 0.5));

      await tester.tap(find.text('Edit Controls'));
      await tester.pumpAndSettle();
      // Edit mode: a badge per module, the eight empty slots, the gallery with Done.
      expect(find.byType(RemoveBadge), findsNWidgets(defaultModules.length));
      expect(find.byType(EmptySlot), findsNWidgets(emptySlots));
      expect(find.text('Done'), findsOneWidget);
      // The battery is offered (this machine has one), not yet in the Center.
      expect(find.text('Battery'), findsWidgets);

      // The Wi-Fi badge removes Wi-Fi: Bluetooth moves up and the file is written.
      final badges = find.byType(RemoveBadge);
      await tester.tap(badges.first);
      await tester.pumpAndSettle();
      expect(
        find.byType(RemoveBadge),
        findsNWidgets(defaultModules.length - 1),
      );
      final saved = loadModules();
      expect(saved.map((m) => m.id), isNot(contains('wifi')));

      // A click on a gallery control adds it at the end.
      // The Battery category shows it at the top of the list.
      await tester.tap(find.text('Battery').first);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Battery').last);
      await tester.pumpAndSettle();
      expect(loadModules().last, const CcModule('battery', ModuleSize.small));

      // Done leaves edit mode, the Center stays open.
      await tester.tap(find.text('Done'));
      await tester.pumpAndSettle();
      expect(find.byType(RemoveBadge), findsNothing);
      expect(find.text('Edit Controls'), findsOneWidget);
    },
  );

  testWidgets('the module menu: size and remove; disabled modules do not act', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1600, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await useSettings();
    final system = FakeSystem(laptop);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: ControlCenter(env: noConfig, system: system),
      ),
    );
    await sendCommand(tester, 'controlcenter.toggle');
    await tester.pumpAndSettle();

    // Nearby sharing has no backend yet: dimmed, says so, a click does nothing.
    expect(find.text('Nearby'), findsOneWidget);
    expect(find.text('Not available yet'), findsOneWidget);
    await tester.tap(find.text('Nearby'), warnIfMissed: false);
    await tester.pump();
    expect(system.calls, isEmpty);

    // A secondary click on Dark Mode: the menu with Size and Remove.
    final dark = find.bySemanticsLabel('Dark Mode');
    await tester.tapAt(tester.getCenter(dark), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Size'), findsOneWidget);
    expect(find.text('Copy to Menu Bar'), findsOneWidget);
    await choose(tester, 'Medium');
    await tester.pumpAndSettle();
    expect(
      loadModules().firstWhere((m) => m.id == 'dark').size,
      ModuleSize.medium,
    );
    // Medium: a capsule with the disc and the label.
    expect(find.text('Dark Mode'), findsOneWidget);

    await tester.tapAt(
      tester.getCenter(find.text('Dark Mode')),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await choose(tester, 'Remove');
    expect(find.text('Dark Mode'), findsNothing);
    expect(loadModules().map((m) => m.id), isNot(contains('dark')));
  });

  testWidgets(
    'privacy pill: the stack moves down 28; the footers open Settings at their pane',
    (tester) async {
      tester.view.physicalSize = const Size(1600, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final launched = <String>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ShellChannel.channel,
        (call) async {
          if (call.method == 'launch') {
            launched.add('${(call.arguments as Map)['command']}');
          }
          return true;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ShellChannel.channel,
          null,
        ),
      );
      final system = FakeSystem(
        laptop.copyWith(
          privacy: const [
            PrivacyUse(app: 'Firefox', kind: 'microphone', inUse: false),
          ],
        ),
      );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ControlCenter(
            env: noConfig,
            system: system,
            settingsApp: const DesktopEntry(
              id: 'dev.frascella.Settings',
              name: 'Settings',
              exec: 'silica-app settings',
            ),
          ),
        ),
      );
      await sendCommand(tester, 'controlcenter.toggle');
      await tester.pumpAndSettle();
      expect(find.text('Firefox recently'), findsOneWidget);
      // With the pill Wi-Fi sits at y 70, the pill at 40.
      expect(tester.getTopLeft(find.text('Wi-Fi')).dy, greaterThan(70));
      expect(
        tester
            .getRect(
              find.ancestor(
                of: find.text('Firefox recently'),
                matching: find.byType(ProfiledGlass),
              ),
            )
            .top,
        40,
      );

      // The Wi-Fi detail ends with its settings item, which opens Settings at Wi-Fi.
      await tester.tap(find.text('Casa'), warnIfMissed: false);
      await tester.pumpAndSettle();
      expect(find.text('Wi-Fi Settings…'), findsOneWidget);
      await tester.tap(find.text('Wi-Fi Settings…'));
      await tester.pump();
      expect(launched, ['silica-app settings --dart-arg wifi']);
    },
  );

  testWidgets('hover lifts the whole module with no scale (CC-HOVER)', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: ControlCenter(env: noConfig, system: FakeSystem(laptop)),
      ),
    );
    await sendCommand(tester, 'controlcenter.toggle');
    await tester.pumpAndSettle();
    final wifi = find.text('Wi-Fi');
    final before = tester.getRect(wifi);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    expect(hoverLift(tester), 0);
    await mouse.moveTo(tester.getCenter(wifi));
    await tester.pump();
    // Measured timing: 10 % at 50 ms, half at 150 ms, done at 300 ms.
    await tester.pump(const Duration(milliseconds: 50));
    expect(hoverLift(tester), closeTo(0.1, 0.02));
    await tester.pump(const Duration(milliseconds: 100));
    expect(hoverLift(tester), closeTo(0.5, 0.02));
    await tester.pump(const Duration(milliseconds: 150));
    expect(hoverLift(tester), closeTo(1, 1e-6));
    // The lift is Amalfi's `State/Hover Overlay`, and the module does not move or grow.
    expect(
      AmalfiColors.light.stateHoverOverlay,
      isNot(const Color(0x00000000)),
    );
    expect(tester.getRect(wifi), before);
    expect(find.byKey(const ValueKey('hover scale')), findsNothing);
    // The ">" of a module with a detail fades in with it.
    double chevron() => tester
        .widgetList<Opacity>(find.byKey(const ValueKey('detail chevron')))
        .first
        .opacity;
    expect(chevron(), closeTo(1, 1e-6));
    // Leaving fades out the same way.
    await mouse.moveTo(Offset.zero);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    expect(hoverLift(tester), closeTo(0.9, 0.02));
    await tester.pump(const Duration(milliseconds: 100));
    expect(hoverLift(tester), closeTo(0.5, 0.02));
    await tester.pumpAndSettle();
    expect(hoverLift(tester), 0);
    expect(chevron(), 0);

    // A button inside the module gives the same hover, nothing of its own.
    final dark = find.bySemanticsLabel('Dark Mode');
    await mouse.moveTo(tester.getCenter(dark));
    await tester.pumpAndSettle();
    expect(hoverLift(tester), closeTo(1, 1e-6));
    await mouse.removePointer();
  });
}

/// The strongest hover lift among the modules, as a fraction of the full one (0..1).
double hoverLift(WidgetTester tester) {
  var best = 0.0;
  for (final box in tester.widgetList<DecoratedBox>(
    find.byKey(const ValueKey('module hover')),
  )) {
    final color = (box.decoration as BoxDecoration).color!;
    final full = AmalfiColors.light.stateHoverOverlay.a;
    best = best > color.a / full ? best : color.a / full;
  }
  return best;
}
