import 'dart:io';

import 'package:flutter/gestures.dart' show PointerDeviceKind;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_shell/menubar/desktop_menu.dart';
import 'package:silica_shell/menubar/logo_menu.dart';
import 'package:silica_shell/menubar/menubar.dart';
import 'package:silica_shell/menubar/menubar_config.dart';
import 'package:silica_shell/system/desktop_entries.dart';
import 'package:silica_shell/menubar/status_icons.dart';
import 'package:silica_shell/menubar/window_menu.dart';
import 'package:silica_ui/silica_ui.dart';
import 'package:silica_shell/system/shell_channel.dart';
import 'package:silica_shell/system/system_status.dart';

import 'controlcenter_test.dart' show FakeSystem;

void main() {
  test('the clock as in the bar: day, date, double space, time', () {
    expect(clockText(DateTime(2026, 10, 12, 22, 34)), 'Mon 12 Oct  22:34');
    expect(clockText(DateTime(2026, 1, 4, 9, 5)), 'Sun 4 Jan  09:05');
    // In Italian as the bar has always shown it.
    expect(
      clockText(DateTime(2026, 10, 12, 22, 34), 'it'),
      'Lun 12 ott  22:34',
    );
    expect(clockText(DateTime(2026, 1, 4, 9, 5), 'it'), 'Dom 4 gen  09:05');
  });

  test('the clock options of Settings: 12 hours, seconds, weekday, date', () {
    final t = DateTime(2026, 10, 12, 22, 34, 7);
    String text(ClockStyle style, [String locale = 'en']) =>
        clockText(t, locale, style);
    expect(text(const ClockStyle(hour24: false)), 'Mon 12 Oct  10:34 PM');
    expect(
      clockText(
        DateTime(2026, 10, 12, 0, 5),
        'en',
        const ClockStyle(hour24: false),
      ),
      'Mon 12 Oct  12:05 AM',
    );
    expect(text(const ClockStyle(seconds: true)), 'Mon 12 Oct  22:34:07');
    expect(text(const ClockStyle(weekday: false)), '12 Oct  22:34');
    expect(text(const ClockStyle(date: ClockDate.never)), 'Mon 22:34');
    expect(
      text(const ClockStyle(date: ClockDate.never, weekday: false)),
      '22:34',
    );
    expect(text(const ClockStyle(date: ClockDate.always)), 'Mon 12 Oct  22:34');
    expect(
      text(const ClockStyle(hour24: true, seconds: true), 'it'),
      'Lun 12 ott  22:34:07',
    );
  });

  test(
    'menubar.json clock and recentItems: valid values, defaults otherwise',
    () {
      final config = MenubarConfig.fromJson({
        'clock': {
          'hour24': false,
          'seconds': true,
          'weekday': false,
          'date': 'never',
        },
        'recentItems': 20,
      });
      expect(
        config.clock,
        const ClockStyle(
          hour24: false,
          seconds: true,
          weekday: false,
          date: ClockDate.never,
        ),
      );
      expect(config.recentItems, 20);
      final odd = MenubarConfig.fromJson({
        'clock': {'hour24': 'yes', 'date': 'sometimes'},
        'recentItems': 7,
      });
      expect(odd.clock, const ClockStyle());
      expect(odd.recentItems, 10);
      expect(MenubarConfig.fromJson({'clock': 3}).clock, const ClockStyle());
    },
  );

  testWidgets(
    'the name of the focused app, in bold, from the .desktop entries',
    (tester) async {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ShellChannel.channel,
        (call) async => switch (call.method) {
          'list' => [
            {'id': 'w1', 'title': 'Casa', 'app_id': 'org.gnome.Nautilus'},
          ],
          'focused' => 'w1',
          _ => null,
        },
      );
      const files = DesktopEntry(
        id: 'org.gnome.Nautilus',
        name: 'File',
        exec: 'nautilus',
      );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(
            entries: const {'org.gnome.Nautilus': files},
            now: DateTime(2026, 10, 12, 22, 34),
          ),
        ),
      );
      await tester.pump();
      await tester.pump();
      final name = tester.widget<Text>(find.text('File').first);
      expect(name.style?.fontWeight, FontWeight.w600);
      expect(find.text('Mon 12 Oct  22:34'), findsOneWidget);
    },
  );

  test('the lit arcs follow the Wi-Fi strength', () {
    expect(wifiArcs(0.9), 3);
    expect(wifiArcs(0.5), 2);
    expect(wifiArcs(0.1), 1);
  });

  test('menubar.json autohide and background', () {
    expect(MenubarConfig.fromJson({}).autohide, MenubarAutohide.fullscreen);
    expect(MenubarConfig.fromJson({}).background, isFalse);
    final config = MenubarConfig.fromJson({
      'autohide': 'always',
      'background': true,
    });
    expect(config.autohide, MenubarAutohide.always);
    expect(config.background, isTrue);
    expect(
      MenubarConfig.fromJson({'autohide': 'sometimes'}).autohide,
      MenubarAutohide.fullscreen,
    );
  });

  group('auto-hide', () {
    // The timers (190 ms) fire and the slide runs: frames, as a ticker starts on the next one.
    Future<void> settle(WidgetTester tester) async {
      for (var i = 0; i < 4; i++) {
        await tester.pump(const Duration(milliseconds: 250));
      }
    }

    Future<ValueNotifier<MenubarConfig>> pumpBar(
      WidgetTester tester,
      MenubarAutohide mode,
    ) async {
      final config = ValueNotifier(MenubarConfig(autohide: mode));
      addTearDown(config.dispose);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(
            config: config,
            system: FakeSystem(const SystemSnapshot()),
            now: DateTime(2026, 10, 13, 9),
          ),
        ),
      );
      await settle(tester);
      return config;
    }

    double top(WidgetTester tester) =>
        tester.getTopLeft(find.byKey(const ValueKey('menubar slide'))).dy;

    testWidgets('never: the bar stays in place', (tester) async {
      ShellChannel.instance.states.value = const {'fullscreen.front': '1'};
      addTearDown(() => ShellChannel.instance.states.value = const {});
      await pumpBar(tester, MenubarAutohide.never);
      expect(top(tester), 0);
    });

    testWidgets('in full screen only: away while the front window is', (
      tester,
    ) async {
      ShellChannel.instance.states.value = const {};
      addTearDown(() => ShellChannel.instance.states.value = const {});
      await pumpBar(tester, MenubarAutohide.fullscreen);
      expect(top(tester), 0);
      ShellChannel.instance.states.value = const {'fullscreen.front': '1'};
      await settle(tester);
      expect(top(tester), -barHeight);
      ShellChannel.instance.states.value = const {'fullscreen.front': '0'};
      await settle(tester);
      expect(top(tester), 0);
    });

    testWidgets('always: the top edge brings it back, leaving sends it away', (
      tester,
    ) async {
      await pumpBar(tester, MenubarAutohide.always);
      expect(top(tester), -barHeight);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      addTearDown(mouse.removePointer);
      await mouse.addPointer(location: const Offset(400, 300));
      await mouse.moveTo(const Offset(400, 0));
      await settle(tester);
      expect(top(tester), 0);
      await mouse.moveTo(const Offset(400, 300));
      await settle(tester);
      expect(top(tester), -barHeight);
    });
  });

  testWidgets('the bar background is drawn only when Settings turns it on', (
    tester,
  ) async {
    final config = ValueNotifier(
      const MenubarConfig(autohide: MenubarAutohide.never),
    );
    addTearDown(config.dispose);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MenuBar(
          config: config,
          system: FakeSystem(const SystemSnapshot()),
          now: DateTime(2026, 10, 13, 9),
        ),
      ),
    );
    Finder background() => find.byKey(const ValueKey('menubar background'));
    expect(background(), findsNothing);
    config.value = const MenubarConfig(
      autohide: MenubarAutohide.never,
      background: true,
    );
    await tester.pump();
    expect(background(), findsOneWidget);
  });

  testWidgets('the Focus moon only while a Focus is on (SH-MB-26)', (
    tester,
  ) async {
    final system = FakeSystem(const SystemSnapshot(wifiAvailable: true));
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MenuBar(system: system, now: DateTime(2026, 10, 13, 9)),
      ),
    );
    bool moon() => tester
        .widgetList<StatusIcon>(find.byType(StatusIcon))
        .any((i) => i.kind == StatusKind.focus);
    expect(moon(), isFalse);
    system.value = system.value.copyWith(doNotDisturb: true);
    await tester.pump();
    expect(moon(), isTrue);
  });

  testWidgets('battery and network from the system state', (tester) async {
    final system = FakeSystem(
      const SystemSnapshot(
        wifiAvailable: true,
        wifiEnabled: true,
        wifiSsid: 'Casa',
        wifiStrength: 79,
        batteryPresent: true,
        batteryPercent: 80,
        batteryCharging: true,
      ),
    );
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MenuBar(system: system, now: DateTime(2026, 10, 13, 9)),
      ),
    );
    StatusIcon icon(StatusKind kind) => tester
        .widgetList<StatusIcon>(find.byType(StatusIcon))
        .firstWhere((i) => i.kind == kind);
    expect(icon(StatusKind.battery).level, 0.8);
    expect(icon(StatusKind.battery).charging, isTrue);
    expect(icon(StatusKind.network).level, 0.79);
    expect(icon(StatusKind.network).dim, isFalse);

    // Wi-Fi off: the icon stays, dimmed; wired: the wired network icon.
    system.value = system.value.copyWith(wifiEnabled: false);
    await tester.pump();
    expect(icon(StatusKind.network).dim, isTrue);
    system.value = const SystemSnapshot(wifiAvailable: true, wired: true);
    await tester.pump();
    expect(
      find.byWidgetPredicate(
        (w) => w is StatusIcon && w.kind == StatusKind.wired,
      ),
      findsOneWidget,
    );
    expect(
      find.byWidgetPredicate(
        (w) => w is StatusIcon && w.kind == StatusKind.battery,
      ),
      findsNothing,
    );
  });

  testWidgets(
    'an open menu takes the pointer only on the bar and itself; a press outside ends it (#149)',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final layer = <MethodCall>[];
      // The frontmost app: the focused window's.
      tester.binding.defaultBinaryMessenger
        ..setMockMethodCallHandler(
          ShellChannel.channel,
          (call) async => switch (call.method) {
            'list' => [
              {'id': 'w1', 'title': 'Notes', 'app_id': 'dev.frascella.Note'},
            ],
            'focused' => 'w1',
            _ => null,
          },
        )
        ..setMockMethodCallHandler(
          const MethodChannel('silica/layer', JSONMethodCodec()),
          (call) async {
            layer.add(call);
            return true;
          },
        );
      addTearDown(() {
        ShellChannel.instance.toplevels.value = const [];
        ShellChannel.instance.focused.value = '';
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(now: DateTime(2026, 10, 12, 22, 34)),
        ),
      );
      await tester.pump();
      await tester.tapAt(tester.getRect(find.text('Window')).center);
      await tester.pump();
      await tester.pump();
      final transient = layer.lastWhere((c) => c.method == 'setTransient');
      // A menu of the frontmost app (INPUT-01 (b)): modal for that app, a click in
      // another app closes it and goes through.
      final args = transient.arguments as Map;
      expect(args['owner'], 'dev.frascella.Note');
      expect(args['ownerRule'], 'modal');
      expect(args['otherRule'], 'pass');
      // An older Ossido (version 6): consumed outside, as before.
      expect(args['press'], 'consume');
      final region = layer.lastWhere((c) => c.method == 'setInputRegion');
      final rects = (region.arguments as Map)['rects'] as List;
      expect(rects.length, 2); // the bar and the menu, not the whole screen

      await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        'silica/shell',
        const JSONMethodCodec().encodeMethodCall(
          const MethodCall('transientDismissed'),
        ),
        (_) {},
      );
      await tester.pumpAndSettle();
      expect(find.text('Minimize'), findsNothing);
      expect(layer.last.method, 'setSize');
    },
  );

  testWidgets(
    'a click that passes to another app closes the menu even as the frontmost app changes (#149)',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final layer = <MethodCall>[];
      tester.binding.defaultBinaryMessenger
        ..setMockMethodCallHandler(
          ShellChannel.channel,
          (call) async => switch (call.method) {
            'list' => [
              {'id': 'w1', 'title': 'Notes', 'app_id': 'dev.frascella.Note'},
            ],
            'focused' => 'w1',
            _ => null,
          },
        )
        ..setMockMethodCallHandler(
          const MethodChannel('silica/layer', JSONMethodCodec()),
          (call) async {
            layer.add(call);
            return true;
          },
        );
      addTearDown(() {
        ShellChannel.instance.toplevels.value = const [];
        ShellChannel.instance.focused.value = '';
        ShellChannel.instance.states.value = const {};
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(now: DateTime(2026, 10, 12, 22, 34)),
        ),
      );
      await tester.pump();
      await tester.tapAt(tester.getRect(find.text('Window')).center);
      await tester.pump();
      await tester.pump();
      expect(find.text('Minimize'), findsOneWidget);

      // Ossido ends the menu, then the same click makes the desktop's owner frontmost:
      // the bar rebuilds for the new app while the menu fades out.
      Future<void> shell(MethodCall call) =>
          tester.binding.defaultBinaryMessenger.handlePlatformMessage(
            'silica/shell',
            const JSONMethodCodec().encodeMethodCall(call),
            (_) {},
          );
      await shell(const MethodCall('transientDismissed'));
      await tester.pump(const Duration(milliseconds: 16));
      await shell(const MethodCall('focused', ''));
      await shell(
        const MethodCall('broadcast', {
          'topic': 'app.frontmost',
          'value': 'dev.frascella.Mustache',
        }),
      );
      await tester.pumpAndSettle();
      expect(find.text('Minimize'), findsNothing);
      expect(layer.any((c) => c.method == 'clearTransient'), isTrue);
      expect(layer.last.method, 'setSize');
    },
  );

  testWidgets('pressing Window opens the menu; the choice runs the command', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final executed = <String>[];
    final layer = <String>[];
    tester.binding.defaultBinaryMessenger
      ..setMockMethodCallHandler(ShellChannel.channel, (call) async {
        if (call.method == 'execute') {
          executed.add('${(call.arguments as Map)['id']}');
        }
        return null;
      })
      ..setMockMethodCallHandler(
        const MethodChannel('silica/layer', JSONMethodCodec()),
        (call) async {
          layer.add(call.method);
          return true;
        },
      );
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MenuBar(now: DateTime(2026, 10, 12, 22, 34)),
      ),
    );
    await tester.pump();
    final finestra = tester.getRect(find.text('Window'));
    await tester.tapAt(finestra.center);
    await tester.pump();
    expect(find.text('Minimize'), findsOneWidget);
    // The surface grows and the keyboard goes to the menu.
    expect(
      layer,
      containsAll(['setSize', 'setInputRegion', 'setKeyboardInteractivity']),
    );
    // The menu body 4 pt left of the item's box (the text is 8 pt in), the label 24 pt
    // from the row's edge (checkmark slot), the rows 4 pt in.
    final riduci = tester.getTopLeft(find.text('Minimize'));
    expect(
      riduci.dx,
      closeTo(finestra.left - itemPadding - menuLeftOfItem + 4 + 24, 1),
    );
    // 8 pt below the bar.
    expect(
      tester.getRect(find.byType(AmalfiMenu)).top,
      barHeight + menuBelowBar,
    );
    await tester.tapAt(tester.getCenter(find.text('Minimize')));
    await tester.pump(const Duration(milliseconds: 150));
    expect(executed, ['window.minimize']);
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.text('Minimize'), findsNothing);
  });

  testWidgets(
    'long press on the green button: the popover below the button runs the arrangement',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final executed = <String>[];
      final layer = <String>[];
      tester.binding.defaultBinaryMessenger
        ..setMockMethodCallHandler(ShellChannel.channel, (call) async {
          if (call.method == 'execute') {
            executed.add('${(call.arguments as Map)['id']}');
          }
          return null;
        })
        ..setMockMethodCallHandler(
          const MethodChannel('silica/layer', JSONMethodCodec()),
          (call) async {
            layer.add(call.method);
            return call.method == 'outputSize'
                ? {'width': 1280, 'height': 800}
                : true;
          },
        );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(now: DateTime(2026, 10, 12, 22, 34)),
        ),
      );
      await tester.pump();
      await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        'silica/shell',
        const JSONMethodCodec().encodeMethodCall(
          MethodCall('windowMenu', {'id': 'w1', 'x': 300, 'y': 200}),
        ),
        (_) {},
      );
      await tester.pump();
      await tester.pump();
      expect(find.text('Half of the screen'), findsOneWidget);
      // The popover below the tip (its arrow's apex), centred on it, once it has grown.
      await tester.pumpAndSettle();
      final panel = tester.getRect(find.byType(AmalfiPopover));
      expect(panel.top, closeTo(200, 0.05));
      expect(panel.left, closeTo(300 - windowPopoverWidth / 2, 0.05));
      expect(panel.width, closeTo(windowPopoverWidth, 0.05));
      // Cells of 54 × 44 with the icon (25 × 20) and the pointer's highlight (46 × 36).
      expect(
        tester.getSize(find.byKey(const ValueKey('window.tile_left'))),
        windowMenuCell,
      );
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(0, 0));
      addTearDown(mouse.removePointer);
      await mouse.moveTo(
        tester.getCenter(find.byKey(const ValueKey('window.tile_left'))),
      );
      await tester.pump();
      final highlight = find.descendant(
        of: find.byKey(const ValueKey('window.tile_left')),
        matching: find.byWidgetPredicate(
          (w) => w is Container && w.decoration != null,
        ),
      );
      expect(tester.getSize(highlight), windowMenuHover);
      // Moving the pointer away does not close it.
      await mouse.moveTo(const Offset(900, 600));
      await tester.pump(const Duration(seconds: 1));
      expect(find.text('Half of the screen'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('window.tile_left')));
      // Blink and fade of the closing (~380 ms).
      for (var i = 0; i < 12; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(executed, ['window.tile_left']);
      expect(find.text('Half of the screen'), findsNothing);
    },
  );

  testWidgets(
    'the item of an open Center has the Accent Soft capsule and its glyph or text in the accent',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ShellChannel.channel,
        (call) async => null,
      );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(now: DateTime(2026, 10, 12, 22, 34)),
        ),
      );
      await tester.pump();
      Future<void> announce(String topic, String value) =>
          tester.binding.defaultBinaryMessenger.handlePlatformMessage(
            'silica/shell',
            const JSONMethodCodec().encodeMethodCall(
              MethodCall('broadcast', {'topic': topic, 'value': value}),
            ),
            (_) {},
          );
      final colors = AmalfiColors.of(Brightness.light);
      // The capsule of the item around `of`: its box and fill.
      (Rect, Color?) capsule(Finder of) {
        final box = find
            .ancestor(of: of, matching: find.byType(DecoratedBox))
            .first;
        final decoration = tester.widget<DecoratedBox>(box).decoration;
        return (
          tester.getRect(box),
          decoration is ShapeDecoration ? decoration.color : null,
        );
      }

      final controls = find.byWidgetPredicate(
        (w) => w is StatusIcon && w.kind == StatusKind.controls,
      );
      final clock = find.text('Mon 12 Oct  22:34');
      expect(capsule(controls).$2, isNull);
      expect(capsule(clock).$2, isNull);
      await announce('controlcenter.open', '1');
      await tester.pump();
      await tester.pump();
      // A status item: 32 × 24, the capsule Accent/Amalfi Soft, the glyph in the accent.
      final (box, fill) = capsule(controls);
      expect(box.size, const Size(statusCell, itemHeight));
      expect(box.top, itemTop);
      expect(fill, const Color(0x66FFFFFF));
      expect(tester.widget<StatusIcon>(controls).color, isNull);
      await announce('notifications.open', '1');
      await tester.pump();
      expect(capsule(clock).$2, const Color(0x66FFFFFF));
      expect(tester.widget<Text>(clock).style?.color, colors.textPrimary);
      await announce('controlcenter.open', '0');
      await announce('notifications.open', '0');
      await tester.pump();
      expect(capsule(controls).$2, isNull);
      expect(capsule(clock).$2, isNull);
    },
  );

  testWidgets(
    'the bar follows the kit: 32 tall, items 28, mark and glyphs 36 wide',
    (tester) async {
      tester.view.physicalSize = const Size(960, 200);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ShellChannel.channel,
        (call) async => null,
      );
      final system = FakeSystem(
        const SystemSnapshot(wifiAvailable: true, wifiSsid: 'Casa'),
      );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(system: system, now: DateTime(2026, 10, 16, 9, 41)),
        ),
      );
      await tester.pump();
      expect(barHeight, 32);
      // The mark's item: x 8, 36 × 28 at y 2 (kit `Silica mark`).
      final mark = find
          .ancestor(
            of: find.byType(SilicaMark),
            matching: find.byType(SizedBox),
          )
          .first;
      expect(tester.getRect(mark), const Rect.fromLTWH(8, 2, 36, 28));
      // The app name follows at x 44, its text 8 pt in (kit `App name`), Headline.
      final name = find.text('Silica');
      expect(tester.getRect(name).left, 52);
      expect(tester.widget<Text>(name).style?.fontWeight, FontWeight.w600);
      // The clock ends 8 + 36 pt before the right edge (its item, then the control centre).
      final clock = find.text('Fri 16 Oct  09:41');
      expect(tester.getRect(clock).right, 960 - 8 - 36 - 8);
      expect(
        tester
            .getRect(
              find.byWidgetPredicate(
                (w) => w is StatusIcon && w.kind == StatusKind.controls,
              ),
            )
            .center
            .dx,
        960 - 8 - 18,
      );
      // The bar paints no material: no glass above its items.
      expect(find.byType(Glass), findsNothing);
    },
  );

  group('logo menu', () {
    List<String> labels(List<MenuEntry> entries) => [
      for (final e in entries)
        switch (e) {
          MenuAction(:final label) => label,
          MenuDivider() => '—',
          _ => '?',
        },
    ];

    List<MenuEntry> menu({bool option = false}) => logoMenu(
      l: englishTexts,
      run: (_) {},
      launch: (_) {},
      open: (_) {},
      option: option,
      user: 'Ada Lovelace',
    );

    test('items in the reference order, power items without the session', () {
      expect(labels(menu()), [
        'About Silica',
        '—',
        'Settings…',
        '—',
        'Recent Items',
        '—',
        'Force Quit…',
        '—',
        'Sleep',
        'Restart…',
        'Shut Down…',
        '—',
        'Lock Screen',
        'Log Out Ada Lovelace…',
      ]);
      final logOut = menu().last as MenuAction;
      expect(logOut.shortcut, '⇧⌘Q');
      expect(logOut.enabled, isTrue);
      expect((menu()[8] as MenuAction).enabled, isFalse);
    });

    test('with Option held: the alternate items, same number of rows', () {
      final alt = labels(menu(option: true));
      expect(alt.first, 'System Information');
      expect(alt, contains('Restart'));
      expect(alt, contains('Shut Down'));
      expect(alt.last, 'Log Out Ada Lovelace');
      expect((menu(option: true).last as MenuAction).shortcut, '⌥⇧⌘Q');
      expect(alt.length, labels(menu()).length);
    });

    test('recent documents from the xbel list, newest first', () {
      const xml = """
<xbel version="1.0">
  <bookmark href="file:///home/u/old.txt" added="x" modified="2026-10-01T10:00:00Z" visited="x"/>
  <bookmark href="file:///home/u/new%20one.pdf" modified="2026-10-14T09:00:00Z"/>
  <bookmark href="https://example.org/" modified="2026-10-14T10:00:00Z"/>
</xbel>""";
      expect(parseRecentlyUsed(xml), [
        '/home/u/new one.pdf',
        '/home/u/old.txt',
      ]);
      expect(parseRecentlyUsed(xml, after: '2026-10-10T00:00:00Z'), [
        '/home/u/new one.pdf',
      ]);
    });

    test('the full name from passwd, else the login', () {
      const passwd =
          'root:x:0:0:root:/root:/bin/bash\n'
          'ada:x:1000:1000:Ada Lovelace,,,:/home/ada:/bin/bash\n'
          'bob:x:1001:1001::/home/bob:/bin/sh\n';
      expect(
        userFullName(env: {'USER': 'ada'}, passwd: passwd),
        'Ada Lovelace',
      );
      expect(userFullName(env: {'USER': 'bob'}, passwd: passwd), 'bob');
    });
  });

  group('desktop menu', () {
    test('the desktop folder from user-dirs, else ~/Desktop', () {
      expect(
        desktopFolder(
          env: {'HOME': '/home/u'},
          userDirs: 'XDG_DESKTOP_DIR="\$HOME/Scrivania"\n',
        ),
        '/home/u/Scrivania',
      );
      expect(
        desktopFolder(env: {'HOME': '/home/u'}, userDirs: ''),
        '/home/u/Desktop',
      );
    });

    test('New Folder picks a free name', () {
      final dir = Directory.systemTemp.createTempSync('desktop');
      addTearDown(() => dir.deleteSync(recursive: true));
      expect(
        newFolder(dir.path, 'untitled folder'),
        '${dir.path}/untitled folder',
      );
      expect(
        newFolder(dir.path, 'untitled folder'),
        '${dir.path}/untitled folder 2',
      );
    });

    test('the items of the reference, the unavailable ones disabled', () {
      final entries = desktopMenu(l: englishTexts, onNewFolder: () {});
      final actions = entries.whereType<MenuAction>().toList();
      expect(
        [for (final a in actions) a.label],
        [
          'New Folder',
          'Paste Item',
          'Get Info',
          'Change Wallpaper…',
          'Edit Widgets…',
          'Use Stacks',
          'Sort By',
          'Show View Options',
        ],
      );
      expect(actions.first.enabled, isTrue);
      expect(actions[3].enabled, isFalse);
      expect(entries.whereType<MenuDivider>(), hasLength(2));
    });

    test(
      'who asked for the menu: the wallpaper or the file manager (#147)',
      () {
        expect(parseDesktopMenu('10 20'), (
          const Offset(10, 20),
          DesktopMenuKind.wallpaper,
        ));
        expect(parseDesktopMenu('10 20 desktop')?.$2, DesktopMenuKind.desktop);
        expect(parseDesktopMenu('10 20 items')?.$2, DesktopMenuKind.items);
        expect(parseDesktopMenu(''), isNull);
        expect(parseDesktopMenu('10 20 other'), isNull);
        expect(parseDesktopMenu('x 20'), isNull);
      },
    );

    test('the owner state, with bad values ignored', () {
      final state = DesktopOwnerState.parse(
        '{"stacks":true,"sort":"kind","selected":2}',
      );
      expect(state.stacks, isTrue);
      expect(state.sort, 'kind');
      expect(state.selected, 2);
      final broken = DesktopOwnerState.parse('{"sort":"color","selected":-1');
      expect(broken.sort, 'none');
      expect(broken.selected, 0);
    });

    test('the file manager desktop menu sends its actions back', () {
      final sent = <String>[];
      final entries = ownerDesktopMenu(
        l: englishTexts,
        state: const DesktopOwnerState(sort: 'name'),
        onAction: sent.add,
      );
      final actions = entries.whereType<MenuAction>().toList();
      MenuAction named(String label) =>
          actions.firstWhere((a) => a.label == label);
      named('New Folder').onSelected?.call();
      named('Use Stacks').onSelected?.call();
      final sort = named('Sort By').submenu!.whereType<MenuAction>().toList();
      expect(sort.firstWhere((a) => a.label == 'Name').checked, isTrue);
      sort.firstWhere((a) => a.label == 'Size').onSelected?.call();
      // Clean Up only with free positions (Sort By › None).
      expect(named('Clean Up').enabled, isFalse);
      expect(named('Paste Item').enabled, isFalse);
      expect(sent, ['newFolder', 'stacks', 'sort:size']);
    });

    test('the owner state carries the shared colour tags and the folder', () {
      final state = DesktopOwnerState.parse(
        '{"selected":1,"tags":["red","blue","pink",3],"folder":true}',
      );
      expect(state.tags, {'red', 'blue'});
      expect(state.folder, isTrue);
      final none = DesktopOwnerState.parse('{"tags":"red"}');
      expect(none.tags, isEmpty);
      expect(none.folder, isFalse);
    });

    test('Customize Folder… replaces Tags… on one folder and sends back', () {
      final sent = <String>[];
      final entries = ownerItemsMenu(
        l: englishTexts,
        state: const DesktopOwnerState(selected: 1, folder: true),
        onAction: sent.add,
      );
      final labels = [for (final a in entries.whereType<MenuAction>()) a.label];
      expect(labels.last, 'Customize Folder…');
      expect(labels, isNot(contains('Tags…')));
      entries.whereType<MenuAction>().last.onSelected?.call();
      expect(sent, ['customize']);
      // A divider, the Tags submenu, then the item.
      final tags = entries.whereType<MenuAction>().firstWhere(
        (a) => a.label == 'Tags',
      );
      expect(entries[entries.indexOf(tags) - 1], isA<MenuDivider>());
      expect(entries.last, isA<MenuAction>());
      expect((entries.last as MenuAction).label, 'Customize Folder…');
    });

    group('the Tags submenu', () {
      List<MenuAction> tagItems(Set<String> assigned, List<String> sent) =>
          (ownerItemsMenu(
                    l: englishTexts,
                    state: DesktopOwnerState(selected: 1, tags: assigned),
                    onAction: sent.add,
                  )
                  .whereType<MenuAction>()
                  .firstWhere((a) => a.label == 'Tags')
                  .submenu!)
              .whereType<MenuAction>()
              .toList();

      test(
        'seven colours in the file manager order, the shared ones checked',
        () {
          final items = tagItems({'red', 'blue'}, []);
          expect(
            [for (final a in items) a.label],
            ['Red', 'Orange', 'Yellow', 'Green', 'Blue', 'Purple', 'Gray'],
          );
          expect(
            [for (final a in items) a.checked],
            [true, false, false, false, true, false, false],
          );
          // Each has its coloured dot.
          expect(items.every((a) => a.icon != null), isTrue);
        },
      );

      test('a choice sends the colour back', () {
        final sent = <String>[];
        final items = tagItems({'red'}, sent);
        items[3].onSelected?.call();
        items[0].onSelected?.call();
        expect(sent, ['tag:green', 'tag:red']);
      });

      test('the colours are localised', () {
        final l = lookupAppLocalizations(const Locale('it'));
        expect(desktopTagName(l, 'red'), 'Rosso');
        expect(desktopTagName(l, 'gray'), 'Grigio');
      });
    });

    test('the menu of the selected icons', () {
      final sent = <String>[];
      final many = ownerItemsMenu(
        l: englishTexts,
        state: const DesktopOwnerState(selected: 2),
        onAction: sent.add,
      ).whereType<MenuAction>().toList();
      expect(
        [for (final a in many) a.label],
        [
          'Open',
          'Move to Trash',
          'Get Info',
          'Rename',
          'Duplicate',
          'Quick Preview',
          // The colours of the tags, in a submenu.
          'Tags',
        ],
      );
      expect(many.last.submenu, hasLength(7));
      // Rename only one at a time.
      expect(many.firstWhere((a) => a.label == 'Rename').enabled, isFalse);
      many.firstWhere((a) => a.label == 'Move to Trash').onSelected?.call();
      expect(sent, ['trash']);
    });

    testWidgets(
      'Move to Trash closes the menu while the desktop announces its new state',
      (tester) async {
        tester.view.physicalSize = const Size(1280, 800);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final layer = <MethodCall>[];
        final sent = <String>[];
        Future<void> shell(MethodCall call) =>
            tester.binding.defaultBinaryMessenger.handlePlatformMessage(
              'silica/shell',
              const JSONMethodCodec().encodeMethodCall(call),
              (_) {},
            );
        Future<void> broadcast(String topic, String value) =>
            shell(MethodCall('broadcast', {'topic': topic, 'value': value}));
        tester.binding.defaultBinaryMessenger
          ..setMockMethodCallHandler(ShellChannel.channel, (call) async {
            if (call.method == 'broadcast') {
              final args = call.arguments as Map;
              sent.add('${args['topic']}=${args['value']}');
            }
            return switch (call.method) {
              'list' => const <Object?>[],
              'focused' => '',
              _ => null,
            };
          })
          ..setMockMethodCallHandler(
            const MethodChannel('silica/layer', JSONMethodCodec()),
            (call) async {
              layer.add(call);
              return switch (call.method) {
                'outputSize' => {'width': 1280, 'height': 800},
                _ => true,
              };
            },
          );
        addTearDown(() {
          ShellChannel.instance.states.value = const {};
        });
        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: MenuBar(now: DateTime(2026, 10, 12, 22, 34)),
          ),
        );
        await tester.pump();
        await broadcast(
          'desktop.state',
          '{"stacks":false,"sort":"none","selected":1}',
        );
        await broadcast('desktopmenu', '400 300 items');
        await tester.pump();
        await tester.pump();
        expect(find.text('Move to Trash'), findsOneWidget);

        await tester.tapAt(tester.getRect(find.text('Move to Trash')).center);
        // The desktop runs the action as the menu starts fading, and announces its new
        // state: the bar rebuilds while the menu fades out.
        for (var i = 0; i < 20; i++) {
          await tester.pump(const Duration(milliseconds: 16));
          if (sent.any((s) => s.startsWith('desktop.action=trash'))) break;
        }
        expect(sent.where((s) => s.startsWith('desktop.action=trash')), [
          'desktop.action=trash 1',
        ]);
        await broadcast(
          'desktop.state',
          '{"stacks":false,"sort":"none","selected":0}',
        );
        await tester.pumpAndSettle();
        expect(find.text('Move to Trash'), findsNothing);
        expect(sent.last, 'desktopmenu=');
        expect(layer.any((c) => c.method == 'clearTransient'), isTrue);
      },
    );

    testWidgets(
      'the item menu shows the colour dots checked from desktop.state and '
      'sends tag and customize back',
      (tester) async {
        tester.view.physicalSize = const Size(1280, 800);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final sent = <String>[];
        Future<void> broadcast(String topic, String value) =>
            tester.binding.defaultBinaryMessenger.handlePlatformMessage(
              'silica/shell',
              const JSONMethodCodec().encodeMethodCall(
                MethodCall('broadcast', {'topic': topic, 'value': value}),
              ),
              (_) {},
            );
        tester.binding.defaultBinaryMessenger
          ..setMockMethodCallHandler(ShellChannel.channel, (call) async {
            if (call.method == 'broadcast') {
              final args = call.arguments as Map;
              sent.add('${args['topic']}=${args['value']}');
            }
            return switch (call.method) {
              'list' => const <Object?>[],
              'focused' => '',
              _ => null,
            };
          })
          ..setMockMethodCallHandler(
            const MethodChannel('silica/layer', JSONMethodCodec()),
            (call) async => switch (call.method) {
              'outputSize' => {'width': 1280, 'height': 800},
              _ => true,
            },
          );
        addTearDown(() {
          ShellChannel.instance.states.value = const {};
        });
        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: MenuBar(now: DateTime(2026, 10, 12, 22, 34)),
          ),
        );
        await tester.pump();
        await broadcast(
          'desktop.state',
          '{"stacks":false,"sort":"none","selected":1,'
              '"tags":["green"],"folder":true}',
        );
        await broadcast('desktopmenu', '400 100 items');
        await tester.pump();
        await tester.pump();
        expect(find.text('Tags'), findsOneWidget);
        expect(find.text('Tags…'), findsNothing);
        // Tags opens a submenu of the colours with a check on the shared ones.
        await tester.tap(find.text('Tags'));
        await tester.pump();
        await tester.pump();
        bool checked(String label) => tester
            .widget<AmalfiMenuItem>(
              find.byWidgetPredicate(
                (w) => w is AmalfiMenuItem && w.label == label,
              ),
            )
            .checked;
        expect(checked('Green'), isTrue);
        expect(checked('Red'), isFalse);

        await tester.tap(find.text('Blue'));
        for (var i = 0; i < 20; i++) {
          await tester.pump(const Duration(milliseconds: 16));
          if (sent.any((s) => s.startsWith('desktop.action=tag:blue'))) break;
        }
        expect(sent.where((s) => s.startsWith('desktop.action=')), [
          'desktop.action=tag:blue 1',
        ]);
        await tester.pumpAndSettle();

        // The menu again: Customize Folder… sends `customize`.
        await broadcast('desktopmenu', '400 100 items');
        await tester.pump();
        await tester.pump();
        await tester.tap(find.text('Customize Folder…'));
        for (var i = 0; i < 20; i++) {
          await tester.pump(const Duration(milliseconds: 16));
          if (sent.any((s) => s.startsWith('desktop.action=customize'))) break;
        }
        expect(sent.where((s) => s.startsWith('desktop.action=')), [
          'desktop.action=tag:blue 1',
          'desktop.action=customize 2',
        ]);
        await tester.pumpAndSettle();
      },
    );

    group('stays on screen', () {
      Future<Rect> openAt(WidgetTester tester, String at) async {
        tester.view.physicalSize = const Size(1280, 800);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        tester.binding.defaultBinaryMessenger
          ..setMockMethodCallHandler(ShellChannel.channel, (call) async {
            return switch (call.method) {
              'list' => const <Object?>[],
              'focused' => '',
              _ => null,
            };
          })
          ..setMockMethodCallHandler(
            const MethodChannel('silica/layer', JSONMethodCodec()),
            (call) async => switch (call.method) {
              'outputSize' => {'width': 1280, 'height': 800},
              _ => true,
            },
          );
        addTearDown(() {
          ShellChannel.instance.states.value = const {};
        });
        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: MenuBar(now: DateTime(2026, 10, 12, 22, 34)),
          ),
        );
        await tester.pump();
        await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
          'silica/shell',
          const JSONMethodCodec().encodeMethodCall(
            MethodCall('broadcast', {'topic': 'desktopmenu', 'value': at}),
          ),
          (_) {},
        );
        await tester.pumpAndSettle();
        final texts = find.descendant(
          of: find.byType(AmalfiMenu),
          matching: find.byType(Text),
        );
        expect(texts, findsWidgets);
        return [
          for (final t in texts.evaluate())
            tester.getRect(find.byWidget(t.widget)),
        ].reduce((a, b) => a.expandToInclude(b));
      }

      testWidgets('opened at the right edge it opens to the left', (
        tester,
      ) async {
        final rect = await openAt(tester, '1275 300 items');
        expect(rect.right, lessThanOrEqualTo(1275));
        expect(rect.left, greaterThanOrEqualTo(0));
      });

      testWidgets('opened near the bottom it shifts up', (tester) async {
        final rect = await openAt(tester, '400 790 items');
        expect(rect.bottom, lessThanOrEqualTo(800));
        expect(rect.top, greaterThanOrEqualTo(barHeight));
      });
    });
  });
}
