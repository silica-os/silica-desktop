import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/dock/dock_menu.dart';
import 'package:silica_shell/dock/model.dart';
import 'package:silica_ui/silica_ui.dart';
import 'package:silica_shell/main.dart';
import 'package:silica_shell/system/glass_profile.dart';
import 'package:silica_shell/system/layer_control.dart';
import 'package:silica_shell/system/shell_channel.dart';

void main() {
  test(
    'only an app that is launching bounces; places use the file manager (#210)',
    () {
      Item manager({bool running = false}) =>
          Item.app('Files', command: 'files', running: running, fixed: true);
      const trash = Item.app('Trash', command: 'files trash:///', trash: true);
      const folder = Item.app('Downloads', command: 'files /d', folder: '/d');
      const app = Item.app('Editor', command: 'editor');
      final closed = [manager(), app, trash, folder];
      final open = [manager(running: true), app, trash, folder];
      // A place never bounces itself; the closed file manager does it for it.
      expect(launchBounceTarget(trash, closed)?.label, 'Files');
      expect(launchBounceTarget(folder, closed)?.label, 'Files');
      // With the file manager running: no bounce at all.
      expect(launchBounceTarget(trash, open), isNull);
      expect(launchBounceTarget(folder, open), isNull);
      // Apps: only while launching; the Apps action and running apps never.
      expect(launchBounceTarget(app, closed), same(app));
      expect(
        launchBounceTarget(
          const Item.app('Editor', command: 'editor', running: true),
          closed,
        ),
        isNull,
      );
      expect(
        launchBounceTarget(
          const Item.app('Apps', command: 'x', action: 'launcher.toggle'),
          closed,
        ),
        isNull,
      );
    },
  );

  testWidgets(
    'the pointer magnifies, shows the label and on exit the Dock goes back to rest',
    (tester) async {
      tester.view.physicalSize = const Size(2560, 200);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(const ShellComponent(component: 'dock-demo'));

      final rest = layout(placeholderItems, width: 2560, height: 200);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(0, 0));
      await mouse.moveTo(Offset(rest.center(6), 155));
      // ~60 ms of entrance.
      await tester.pump(const Duration(milliseconds: 30));
      await tester.pump(const Duration(milliseconds: 40));
      expect(find.text('Photos'), findsOneWidget);

      // Hysteresis: just above the entry threshold but still on the magnified icon, it stays.
      final zoneTop = 200 - bottomGap - backgroundHeight - activeZoneAbove;
      await mouse.moveTo(Offset(rest.center(6), zoneTop - 3));
      await tester.pump(const Duration(milliseconds: 30));
      expect(find.text('Photos'), findsOneWidget);

      // On exit the label stays ~40 ms, then fades in ~250 ms.
      await mouse.moveTo(const Offset(1280, 10));
      await tester.pump(const Duration(milliseconds: 30));
      expect(find.text('Photos'), findsOneWidget);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 50));
      expect(find.text('Photos'), findsNothing);
    },
  );

  test('the label ends 8 pt above the icon drawing, magnified or not (#221)', () {
    const m = DockMetrics();
    // Kit Dock/Shelf/With Label: the tip is 2 pt above the 64 pt slot (the drawing starts
    // at 824/1024 of it, about 6 pt in).
    expect(m.labelGapFor(64), closeTo(8 - 6.25, 1e-9));
    // Magnified: the gap follows the larger slot's own drawing inset.
    expect(m.labelGapFor(128), closeTo(8 - 12.5, 1e-9));
    // The drawing's top minus the tip is always the 8 pt clearance.
    for (final side in [16.0, 64.0, 100.0]) {
      expect(
        side * (1 - 824 / 1024) / 2 + m.labelGapFor(side),
        closeTo(8, 1e-9),
      );
    }
  });

  test(
    'the launch bounce is the measured curve (misc.md), scaled to the Amalfi size',
    () {
      // Reference samples (pt above rest, ms from the start of a jump), heights x 56 / 63.
      const samples = {
        0: 0.0,
        40: 5.0,
        80: 8.0,
        133: 11.0,
        160: 13.0,
        213: 14.0,
        293: 17.0,
        440: 14.0,
        533: 11.0,
        587: 7.0,
        653: 3.0,
        693: 0.0,
      };
      for (final MapEntry(:key, :value) in samples.entries) {
        expect(
          bounceHeight(key.toDouble()),
          closeTo(value * amalfiScale, 1e-9),
          reason: '$key ms',
        );
      }
      expect(bouncePeak, closeTo(17 * 56 / 63, 1e-9));
      expect(bounceHeight(293), bouncePeak);
      // A jump every 0.7 s: the same height one period later.
      expect(bounceHeight(293 + 693), closeTo(bouncePeak, 1e-9));
      expect(bounceHeight(693), 0);
      // Never below rest, no rebound.
      for (var ms = 0.0; ms < 2000; ms += 5) {
        expect(bounceHeight(ms), greaterThanOrEqualTo(0));
      }
    },
  );

  testWidgets('right click and long press open the menu; the click activates', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 300);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final log = <String>[];
    const items = [
      Item.app('Terminal', command: 'term'),
      Item.app('Note', command: 'note'),
    ];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: items,
          onActivate: (item) => log.add('attiva ${item.label}'),
          menuFor: (item) => [
            MenuAction(
              'Apri ${item.label}',
              onSelected: () => log.add('menu ${item.label}'),
            ),
          ],
        ),
      ),
    );
    final rest = layout(items, width: 1280, height: 300);
    final icon = Offset(rest.center(0), 300 - bottomGap - backgroundHeight / 2);
    // Click: activates.
    await tester.tapAt(icon);
    await tester.pump(const Duration(milliseconds: 50));
    expect(log, ['attiva Terminal']);
    // Right click: the menu above the icon, the tip of its 6 pt arrow where the label's is
    // (anchored to the icon, #248).
    await tester.tapAt(icon, buttons: kSecondaryButton);
    await tester.pump();
    expect(find.text('Apri Terminal'), findsOneWidget);
    final body = tester.getRect(find.byKey(dockMenuSurface));
    final m = DockMetrics(defaultCanvas);
    final (_, iconTop, side) = iconRect(rest, 0, 300);
    expect(body.bottom, closeTo(iconTop - m.labelGapFor(side) - 6, 0.5));
    // Centred on the item unless the screen's edge is in the way (8 pt margin).
    expect(
      body.left,
      closeTo(math.max(8, rest.center(0) - body.width / 2), 0.5),
    );
    // Click outside: it closes without choosing.
    await tester.tapAt(const Offset(10, 10));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.text('Apri Terminal'), findsNothing);
    // Long press on the second icon: the menu appears after ~530 ms.
    final second = Offset(rest.center(1), icon.dy);
    final gesture = await tester.startGesture(second);
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.text('Apri Note'), findsNothing);
    await tester.pump(const Duration(milliseconds: 110));
    expect(find.text('Apri Note'), findsOneWidget);
    // Dragging onto the entry and releasing chooses it.
    await gesture.moveTo(tester.getCenter(find.text('Apri Note')));
    await gesture.up();
    await tester.pump(const Duration(milliseconds: 150));
    expect(log, ['attiva Terminal', 'menu Note']);
  });

  testWidgets(
    'the menu is anchored to the magnified icon and freezes the magnification',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 300);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      const items = [
        Item.app('Terminal', command: 'term'),
        Item.app('Note', command: 'note'),
        Item.app('Mail', command: 'mail'),
      ];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(
            items: items,
            magnified: 100,
            menuFor: (item) => [MenuAction('Open')],
          ),
        ),
      );
      final rest = layout(items, width: 1280, height: 300);
      final y = 300 - bottomGap - backgroundHeight / 2;
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset(rest.center(0), y));
      addTearDown(mouse.removePointer);
      await mouse.moveTo(Offset(rest.center(0), y));
      for (var i = 0; i < 20; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      await tester.tapAt(Offset(rest.center(0), y), buttons: kSecondaryButton);
      await tester.pump(const Duration(milliseconds: 100));
      final body = tester.getRect(find.byKey(dockMenuSurface));
      final restTip = iconRect(rest, 0, 300).$2 - 6;
      // The magnified icon is bigger and higher: the menu sits above the resting position.
      expect(body.bottom, lessThan(restTip - 6));
      // Moving over the other icons while the menu is open changes nothing.
      await mouse.moveTo(Offset(rest.center(2), y));
      for (var i = 0; i < 10; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(tester.getRect(find.byKey(dockMenuSurface)), body);
    },
  );

  group('the label and the menu of an icon', () {
    late TestGesture mouse;
    late Offset icon;
    late Offset other;
    late List<String> log;

    Future<void> open(WidgetTester tester) async {
      tester.view.physicalSize = const Size(1280, 300);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      log = [];
      const items = [
        Item.app('Terminal', command: 'term'),
        Item.app('Note', command: 'note'),
      ];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(
            items: items,
            onActivate: (item) => log.add('activate ${item.label}'),
            menuFor: (item) => [MenuAction('Open ${item.label}')],
          ),
        ),
      );
      final rest = layout(items, width: 1280, height: 300);
      icon = Offset(rest.center(0), 300 - bottomGap - backgroundHeight / 2);
      other = Offset(rest.center(1), icon.dy);
      mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(0, 0));
      await mouse.moveTo(icon);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 100));
      expect(find.text('Terminal'), findsOneWidget);
    }

    testWidgets('a click and a short press leave the label', (tester) async {
      await open(tester);
      final press = await tester.startGesture(
        icon,
        kind: PointerDeviceKind.mouse,
      );
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('Terminal'), findsOneWidget);
      await press.up();
      await tester.pump(const Duration(milliseconds: 50));
      expect(find.text('Terminal'), findsOneWidget);
      expect(log, ['activate Terminal']);
      await mouse.removePointer();
    });

    testWidgets('a long press hides the label and opens the menu at ~530 ms', (
      tester,
    ) async {
      await open(tester);
      final press = await tester.startGesture(
        icon,
        kind: PointerDeviceKind.mouse,
      );
      await tester.pump(const Duration(milliseconds: 500));
      expect(find.text('Terminal'), findsOneWidget);
      expect(find.text('Open Terminal'), findsNothing);
      await tester.pump(const Duration(milliseconds: 40));
      expect(find.text('Terminal'), findsNothing);
      expect(find.text('Open Terminal'), findsOneWidget);
      await press.up();
      await tester.pump();
      await mouse.removePointer();
    });

    testWidgets(
      'a right click hides the label at once and it stays hidden until the '
      'pointer leaves the icon',
      (tester) async {
        await open(tester);
        await tester.tapAt(icon, buttons: kSecondaryButton);
        await tester.pump();
        expect(find.text('Terminal'), findsNothing);
        expect(find.text('Open Terminal'), findsOneWidget);
        // Closed with the pointer still on the icon: the label does not come back.
        await tester.tapAt(const Offset(10, 10));
        await tester.pump(const Duration(milliseconds: 100));
        await tester.pump(const Duration(milliseconds: 300));
        expect(find.text('Open Terminal'), findsNothing);
        await mouse.moveTo(icon + const Offset(1, 0));
        await tester.pump(const Duration(milliseconds: 100));
        expect(find.text('Terminal'), findsNothing);
        // Over another icon, and back: the label returns.
        await mouse.moveTo(other);
        await tester.pump(const Duration(milliseconds: 100));
        expect(find.text('Note'), findsOneWidget);
        await mouse.moveTo(icon);
        await tester.pump(const Duration(milliseconds: 100));
        expect(find.text('Terminal'), findsOneWidget);
        await mouse.removePointer();
      },
    );
  });

  testWidgets('the open submenu is part of the input region', (tester) async {
    tester.view.physicalSize = const Size(1280, 300);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final regions = <List<Rect>>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      LayerControl.channel,
      (call) async {
        if (call.method == 'setInputRegion') {
          final rects = (call.arguments as Map)['rects'] as List?;
          regions.add([
            for (final r in rects ?? const [])
              Rect.fromLTWH(
                (r[0] as num).toDouble(),
                (r[1] as num).toDouble(),
                (r[2] as num).toDouble(),
                (r[3] as num).toDouble(),
              ),
          ]);
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        LayerControl.channel,
        null,
      ),
    );
    final log = <String>[];
    const items = [Item.app('Note', command: 'note')];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: items,
          menuFor: (item) => [
            MenuAction(
              'Options',
              submenu: [
                MenuAction(
                  'Remove from Dock',
                  onSelected: () => log.add('via'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
    final rest = layout(items, width: 1280, height: 300);
    final icon = Offset(rest.center(0), 300 - bottomGap - backgroundHeight / 2);
    await tester.tapAt(icon, buttons: kSecondaryButton);
    await tester.pump();
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: icon);
    await mouse.moveTo(tester.getCenter(find.text('Options')));
    await tester.pump(const Duration(milliseconds: 350));
    await tester.pump();
    await tester.pump();
    final item = tester.getCenter(find.text('Remove from Dock'));
    expect(regions.last.any((r) => r.contains(item)), isTrue);
  });

  testWidgets(
    'an older Ossido (no transients): with the menu open the Dock takes the whole screen',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 300);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final regions = <List<Object?>>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        LayerControl.channel,
        (call) async {
          if (call.method == 'outputSize')
            return {'width': 1280, 'height': 800};
          if (call.method == 'setInputRegion') {
            regions.add((call.arguments as Map)['rects'] as List<Object?>);
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          LayerControl.channel,
          null,
        ),
      );
      const items = [Item.app('Note', command: 'note')];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(items: items, menuFor: (item) => [MenuAction('Open')]),
        ),
      );
      await tester.pump();
      final rest = layout(items, width: 1280, height: 300);
      final icon = Offset(
        rest.center(0),
        300 - bottomGap - backgroundHeight / 2,
      );
      await tester.tapAt(icon, buttons: kSecondaryButton);
      await tester.pump();
      await tester.pump();
      // A single rectangle, as tall as the screen: the click on the desktop closes the menu.
      expect(regions.last, [
        [0, 0, 1280, 800],
      ]);
    },
  );

  testWidgets(
    'a Dock menu is a transient: the region stays small, a press outside closes it (#149)',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 300);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final layer = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        LayerControl.channel,
        (call) async {
          layer.add(call);
          if (call.method == 'outputSize') {
            return {'width': 1280, 'height': 800};
          }
          return call.method == 'setTransient' ? true : null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          LayerControl.channel,
          null,
        ),
      );
      // The app that was active when the menu opens: its windows get only an activation.
      ShellChannel.instance.toplevels.value = const [
        Toplevel(id: 'w1', title: 'Notes', appId: 'dev.frascella.Note'),
      ];
      ShellChannel.instance.focused.value = 'w1';
      addTearDown(() {
        ShellChannel.instance.toplevels.value = const [];
        ShellChannel.instance.focused.value = '';
      });
      const items = [Item.app('Note', command: 'note')];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(items: items, menuFor: (item) => [MenuAction('Open')]),
        ),
      );
      await tester.pump();
      expect(layer.any((c) => c.method == 'setTransient'), isFalse);
      final rest = layout(items, width: 1280, height: 300);
      final icon = Offset(
        rest.center(0),
        300 - bottomGap - backgroundHeight / 2,
      );
      await tester.tapAt(icon, buttons: kSecondaryButton);
      await tester.pump();
      await tester.pump();
      Rect rect(Object? r) {
        final v = r as List;
        return Rect.fromLTWH(
          (v[0] as num).toDouble(),
          (v[1] as num).toDouble(),
          (v[2] as num).toDouble(),
          (v[3] as num).toDouble(),
        );
      }

      // The Dock menu's rules (INPUT-01 (c)), and the Dock plus the menu as the
      // rectangles: not the screen.
      final transient = layer.lastWhere((c) => c.method == 'setTransient');
      final args = transient.arguments as Map;
      expect(args['owner'], 'dev.frascella.Note');
      expect(args['ownerRule'], 'activate');
      expect(args['otherRule'], 'pass');
      final rects = [for (final r in args['rects'] as List) rect(r)];
      expect(rects, hasLength(2));
      expect(rects.any((r) => r.contains(icon)), isTrue);
      expect(
        rects.any((r) => r.contains(tester.getCenter(find.text('Open')))),
        isTrue,
      );
      expect(rects.every((r) => r.height < 400), isTrue);
      final region = layer.lastWhere((c) => c.method == 'setInputRegion');
      expect((region.arguments as Map)['rects'], args['rects']);

      // Ossido: a press outside ended it.
      await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        'silica/shell',
        const JSONMethodCodec().encodeMethodCall(
          const MethodCall('transientDismissed'),
        ),
        (_) {},
      );
      await tester.pump();
      await tester.pump();
      expect(find.text('Open'), findsNothing);
      expect(layer.any((c) => c.method == 'clearTransient'), isTrue);
      final closed = layer.lastWhere((c) => c.method == 'setInputRegion');
      expect(((closed.arguments as Map)['rects'] as List), hasLength(1));
    },
  );

  testWidgets(
    'dragging reorders; up high "Remove" appears and the icon is removed',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final orders = <String>[];
      const items = [
        Item.app('Uno', command: 'uno', pinned: true),
        Item.app('Due', command: 'due', pinned: true),
        Item.app('Tre', command: 'tre', pinned: true),
        Item.separator(),
        Item.app('Trash', command: 'cestino'),
      ];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(
            items: items,
            onReorder: (order, dropped) => orders.add(
              '${[for (final i in order) i.label].join(' ')} | ${dropped?.label}',
            ),
          ),
        ),
      );
      final rest = layout(items, width: 1280, height: 400);
      final y = 400 - bottomGap - backgroundHeight / 2;
      // Uno after Due.
      var gesture = await tester.startGesture(Offset(rest.center(0), y));
      await gesture.moveBy(const Offset(10, 0));
      await tester.pump();
      await gesture.moveTo(Offset(rest.center(1) + 20, y));
      await tester.pump();
      await gesture.up();
      await tester.pump(const Duration(milliseconds: 300));
      expect(orders, ['Due Uno Tre | Uno']);
      // Tre up high, more than 130 pt above the grab: "Remove", and on release it disappears.
      gesture = await tester.startGesture(Offset(rest.center(2), y));
      await gesture.moveBy(const Offset(0, -10));
      await tester.pump();
      await gesture.moveTo(Offset(rest.center(2), y - 140));
      await tester.pump();
      expect(find.text('Remove'), findsOneWidget);
      await gesture.up();
      await tester.pump(const Duration(milliseconds: 100));
      expect(orders.last, 'Uno Due | null');
      await tester.pump(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 50));
      expect(find.text('Remove'), findsNothing);
    },
  );

  testWidgets('the fixed file manager is not dragged, nothing goes before it', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final orders = <String>[];
    const items = [
      Item.app('Files', command: 'files', pinned: true, fixed: true),
      Item.app('Due', command: 'due', pinned: true),
      Item.app('Tre', command: 'tre', pinned: true),
      Item.separator(),
      Item.app('Trash', command: 'cestino'),
    ];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: items,
          onReorder: (order, dropped) => orders.add(
            '${[for (final i in order) i.label].join(' ')} | ${dropped?.label}',
          ),
        ),
      ),
    );
    final rest = layout(items, width: 1280, height: 400);
    final y = 400 - bottomGap - backgroundHeight / 2;
    // Dragging the file manager does nothing, also up high.
    var gesture = await tester.startGesture(Offset(rest.center(0), y));
    await gesture.moveBy(const Offset(10, 0));
    await tester.pump();
    await gesture.moveTo(Offset(rest.center(0), y - 140));
    await tester.pump();
    expect(find.text('Remove'), findsNothing);
    await gesture.up();
    await tester.pump(const Duration(milliseconds: 600));
    expect(orders, isEmpty);
    // Tre dropped at the far left lands after it.
    gesture = await tester.startGesture(Offset(rest.center(2), y));
    await gesture.moveBy(const Offset(-10, 0));
    await tester.pump();
    await gesture.moveTo(Offset(rest.center(0) - 20, y));
    await tester.pump();
    await gesture.up();
    await tester.pump(const Duration(milliseconds: 300));
    expect(orders, ['Files Tre Due | Tre']);
  });

  testWidgets(
    'right click on the separator: its menu; the left click nothing',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 300);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final log = <String>[];
      const items = [
        Item.app('Terminal', command: 'term'),
        Item.separator(),
        Item.app('Trash', command: 'trash'),
      ];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(
            items: items,
            onActivate: (item) => log.add('attiva ${item.label}'),
            menuFor: (item) => item.kind == ItemKind.separator
                ? const [MenuAction('Show recent apps')]
                : const [],
          ),
        ),
      );
      final rest = layout(items, width: 1280, height: 300);
      final separator = Offset(
        rest.center(1),
        300 - bottomGap - backgroundHeight / 2,
      );
      await tester.tapAt(separator);
      await tester.pump(const Duration(milliseconds: 50));
      expect(log, isEmpty);
      expect(find.byKey(dockMenuSurface), findsNothing);
      await tester.tapAt(separator, buttons: kSecondaryButton);
      await tester.pump();
      expect(find.text('Show recent apps'), findsOneWidget);
    },
  );

  testWidgets(
    'files from another app: the cell below darkens, the drop goes to the item',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 300);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final dropped = <String>[];
      const items = [
        Item.app('Terminal', command: 'term'),
        Item.app('Testo', command: 'testo', openFiles: 'testo %F'),
        Item.separator(),
        Item.app('Trash', command: 'xdg-open trash:///', trash: true),
      ];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(
            items: items,
            onDropFiles: (item, paths) =>
                dropped.add('${item.label}: ${paths.join(',')}'),
          ),
        ),
      );
      Future<void> send(String method, [Object? args]) async {
        await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
          SilicaDrag.channel.name,
          const JSONMethodCodec().encodeMethodCall(MethodCall(method, args)),
          (_) {},
        );
        await tester.pump(const Duration(milliseconds: 100));
      }

      final rest = layout(items, width: 1280, height: 300);
      final y = 300 - bottomGap - backgroundHeight / 2;
      // On Terminal (takes no files) nothing; on the Trash yes.
      await send('enter', {'x': rest.center(0), 'y': y});
      await send('motion', {'x': rest.center(0), 'y': y});
      await send('drop', {
        'x': rest.center(0),
        'y': y,
        'uris': ['file:///tmp/a.txt'],
      });
      expect(dropped, isEmpty);
      await send('enter', {'x': rest.center(3), 'y': y});
      final current = layout(
        items,
        width: 1280,
        height: 300,
        pointerX: rest.center(3),
        amount: 1,
      );
      await send('motion', {'x': current.center(3), 'y': y});
      await send('drop', {
        'x': current.center(3),
        'y': y,
        'uris': ['file:///tmp/a.txt', 'file:///tmp/b%20c.txt'],
      });
      expect(dropped, ['Trash: /tmp/a.txt,/tmp/b c.txt']);
      await tester.pump(const Duration(seconds: 1));
    },
  );

  testWidgets(
    'at session start the Dock waits below the screen, then slides up (provisional)',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 200);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        const Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(items: placeholderItems, revealAtStart: true),
        ),
      );
      final glass = find.byType(ProfiledGlass);
      const restTop = 200 - bottomGap - backgroundHeight;
      // The first frame: nothing of the Dock on screen, not even its glass.
      expect(tester.getTopLeft(glass).dy, greaterThanOrEqualTo(200));
      // Nothing to decode (placeholder icons): it comes up right after, from below.
      await tester.pump(const Duration(milliseconds: 1));
      await tester.pump(sessionReveal ~/ 2);
      final halfway = tester.getTopLeft(glass).dy;
      expect(halfway, greaterThan(restTop));
      expect(halfway, lessThan(200));
      await tester.pump(sessionReveal);
      expect(tester.getTopLeft(glass).dy, closeTo(restTop, 0.01));
    },
  );
}
