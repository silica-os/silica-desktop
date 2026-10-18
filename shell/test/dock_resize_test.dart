// Resizing the Dock by dragging the separator, and the sizes dock.json carries.

import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/dock/model.dart';
import 'package:silica_shell/dock/options.dart';
import 'package:silica_ui/silica_ui.dart';

import 'support/settings.dart';

const _items = [
  Item.app('A', command: 'a'),
  Item.app('B', command: 'b'),
  Item.separator(),
  Item.app('C', command: 'c'),
];

void main() {
  test(
    'dock sizes: defaults, clamped to 16-128, magnified from the size up',
    () async {
      final backend = await useSettings();
      final settings = SilicaSettings.instance;
      final defaults = DockOptions.of(settings);
      expect(defaults.tileSize, 60);
      expect(defaults.magnify, isFalse);
      backend.emit('dock.tileSize', 200);
      expect(DockOptions.of(settings).tileSize, 128);
      backend.emit('dock.tileSize', 3);
      expect(DockOptions.of(settings).tileSize, 16);
      backend.emit('dock.tileSize', 64);
      backend.emit('dock.magnifiedSize', 40);
      expect(DockOptions.of(settings).magnifiedSize, 64);
      backend.emit('dock.magnifiedSize', 500);
      expect(DockOptions.of(settings).magnifiedSize, 128);
      backend.emit('dock.magnify', true);
      expect(DockOptions.of(settings).magnify, isTrue);
    },
  );

  group('separator drag', () {
    late List<double> saved;

    Future<(TestGesture, Offset)> setUp(WidgetTester tester) async {
      await useSettings();
      tester.view.physicalSize = const Size(1280, 400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      saved = [];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(
            items: _items,
            onResize: (canvas) {
              saved.add(canvas);
              setDockOption('tileSize', canvas.round());
            },
          ),
        ),
      );
      final rest = layout(_items, width: 1280, height: 400);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(0, 0));
      return (mouse, Offset(rest.center(2), 400 - bottomGap - 30));
    }

    double shelfHeight(WidgetTester tester) => tester
        .widgetList<Positioned>(find.byType(Positioned))
        .firstWhere(
          (p) => p.width != null && p.height != null && p.height! >= 22,
        )
        .height!;

    testWidgets('up is bigger, 1 pt per point, whole points, kept on release', (
      tester,
    ) async {
      final (mouse, start) = await setUp(tester);
      expect(shelfHeight(tester), 80);

      // Over the separator: the ns-resize pointer.
      await mouse.moveTo(start);
      await tester.pump();
      expect(
        tester.binding.mouseTracker.debugDeviceActiveCursor(1),
        SystemMouseCursors.resizeUpDown,
      );

      await mouse.down(start);
      await mouse.moveTo(start - const Offset(0, 10));
      await tester.pump();
      // Canvas 70: the shelf is canvas + 20k, at once (no animation).
      expect(shelfHeight(tester), 94);
      // Continuous but snapped to whole points.
      await mouse.moveTo(start - const Offset(0, 17.4));
      await tester.pump();
      expect(shelfHeight(tester), 103);
      // Nothing is saved before the release.
      expect(saved, isEmpty);
      expect(DockOptions.of(SilicaSettings.instance).tileSize, 60);
      // Down again, below the starting size.
      await mouse.moveTo(start + const Offset(0, 8));
      await tester.pump();
      expect(shelfHeight(tester), 70);
      await mouse.moveTo(start - const Offset(0, 20));
      await tester.pump();
      await mouse.up();
      await tester.pump();
      expect(saved, [80]);
      expect(DockOptions.of(SilicaSettings.instance).tileSize, 80);
      expect(shelfHeight(tester), 106);
    });

    testWidgets('clamped to 16 and 128', (tester) async {
      final (mouse, start) = await setUp(tester);
      await mouse.down(start);
      await mouse.moveTo(start - const Offset(0, 300));
      await tester.pump();
      expect(shelfHeight(tester), 170);
      await mouse.moveTo(start + const Offset(0, 300));
      await tester.pump();
      expect(shelfHeight(tester), 22);
      await mouse.up();
      await tester.pump();
      expect(saved, [16]);
    });

    testWidgets('a click without moving changes nothing', (tester) async {
      final (mouse, start) = await setUp(tester);
      await mouse.down(start);
      await mouse.up();
      await tester.pump();
      expect(saved, isEmpty);
      expect(shelfHeight(tester), 80);
    });
  });

  testWidgets('the Dock follows its canvas', (tester) async {
    tester.view.physicalSize = const Size(1280, 400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    for (final canvas in [16.0, 48.0, 128.0]) {
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(items: _items, canvas: canvas),
        ),
      );
      final m = DockMetrics(canvas);
      final glass = tester
          .widgetList<Positioned>(find.byType(Positioned))
          .firstWhere((p) => p.height == m.backgroundHeight);
      final expected = 3 * m.pitch + m.separatorCell + 2 * m.sidePadding;
      expect(glass.width, closeTo(expected, 0.01));
    }
  });
}
