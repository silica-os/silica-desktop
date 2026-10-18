// Dock folders (#235, #234): they reorder among themselves, the Trash removes what is dropped
// on it, the plate slides with the icons, and a new folder never leaves an empty slot.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/dock/model.dart';
import 'package:silica_ui/silica_ui.dart';

const _items = [
  Item.app('Uno', command: 'uno', pinned: true),
  Item.app('Due', command: 'due', pinned: true),
  Item.separator(),
  Item.app('Docs', command: 'x', folder: '/nonexistent-docs'),
  Item.app('Music', command: 'x', folder: '/nonexistent-music'),
  Item.app('Pics', command: 'x', folder: '/nonexistent-pics'),
  Item.app('Trash', command: 'trash', trash: true),
];

void main() {
  const height = 400.0;
  final y = height - bottomGap - backgroundHeight / 2;

  Future<List<String>> pump(
    WidgetTester tester, {
    List<Item> items = _items,
    List<String>? apps,
    List<List<String>>? folders,
  }) async {
    tester.view.physicalSize = const Size(1280, height);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final log = <String>[];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: items,
          onReorder: (order, dropped) =>
              log.add('apps ${[for (final i in order) i.label].join(' ')}'),
          onReorderFolders: (order) => log.add(
            'folders ${[for (final f in order) f.split('-').last].join(' ')}',
          ),
        ),
      ),
    );
    return log;
  }

  double plate(WidgetTester tester) => tester
      .getSize(
        find
            .byWidgetPredicate(
              (w) => w is Positioned && w.height == backgroundHeight,
            )
            .first,
      )
      .width;

  testWidgets(
    'folders reorder among themselves and never cross the separator',
    (tester) async {
      final log = await pump(tester);
      final rest = layout(_items, width: 1280, height: height);
      // Docs after Music.
      var g = await tester.startGesture(Offset(rest.center(3), y));
      await g.moveBy(const Offset(10, 0));
      await tester.pump();
      await g.moveTo(Offset(rest.center(4) + 20, y));
      await tester.pump();
      await g.up();
      await tester.pump(const Duration(milliseconds: 400));
      expect(log, ['folders music docs pics']);
      // Pics dragged far to the left, over the apps: first among the folders.
      g = await tester.startGesture(Offset(rest.center(5), y));
      await g.moveBy(const Offset(-10, 0));
      await tester.pump();
      await g.moveTo(Offset(rest.center(0), y));
      await tester.pump();
      await g.up();
      await tester.pump(const Duration(milliseconds: 400));
      expect(log.last, 'folders pics docs music');
      expect(log.any((e) => e.startsWith('apps')), isFalse);
    },
  );

  testWidgets('a folder dropped on the Trash is removed, with its label', (
    tester,
  ) async {
    final log = await pump(tester);
    final rest = layout(_items, width: 1280, height: height);
    final g = await tester.startGesture(Offset(rest.center(4), y));
    await g.moveBy(const Offset(10, 0));
    await tester.pump();
    await g.moveTo(Offset(rest.center(6), y));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.text('Remove from Dock'), findsOneWidget);
    await g.up();
    await tester.pump(const Duration(milliseconds: 100));
    expect(log, ['folders docs pics']);
    await tester.pump(const Duration(seconds: 1));
    expect(find.text('Remove from Dock'), findsNothing);
  });

  testWidgets('an app dropped on the Trash is unpinned; elsewhere no label', (
    tester,
  ) async {
    final log = await pump(tester);
    final rest = layout(_items, width: 1280, height: height);
    final g = await tester.startGesture(Offset(rest.center(0), y));
    await g.moveBy(const Offset(10, 0));
    await tester.pump();
    await g.moveTo(Offset(rest.center(3), y));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.text('Remove from Dock'), findsNothing);
    await g.moveTo(Offset(rest.center(6), y));
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.text('Remove from Dock'), findsOneWidget);
    await g.up();
    await tester.pump(const Duration(milliseconds: 100));
    expect(log, ['apps Due']);
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('the plate slides with the icons when one is dragged out', (
    tester,
  ) async {
    await pump(tester);
    final rest = layout(_items, width: 1280, height: height);
    final full = plate(tester);
    final g = await tester.startGesture(Offset(rest.center(0), y));
    await g.moveBy(const Offset(0, -10));
    await tester.pump(const Duration(milliseconds: 400));
    expect(plate(tester), closeTo(full, 0.5));
    // Up high: the gap closes, the plate narrows by a cell over the slide, not at once.
    await g.moveTo(Offset(rest.center(0), y - 140));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    final mid = plate(tester);
    expect(mid, lessThan(full));
    expect(mid, greaterThan(full - cellPitch + 1));
    await tester.pump(const Duration(milliseconds: 400));
    expect(plate(tester), closeTo(full - cellPitch, 0.5));
    await g.up();
    await tester.pump(const Duration(seconds: 1));
  });

  testWidgets('a folder dragged in gets a gap at the Trash edge too', (
    tester,
  ) async {
    await pump(tester);
    final rest = layout(_items, width: 1280, height: height);
    Future<void> send(String method, [Object? args]) async {
      await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        SilicaDrag.channel.name,
        const JSONMethodCodec().encodeMethodCall(MethodCall(method, args)),
        (_) {},
      );
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pump(const Duration(milliseconds: 400));
    }

    final before = plate(tester);
    final x = rest.center(6) - rest.cells[6].$2 / 2 + 3;
    await send('enter', {'x': x, 'y': y});
    await send('motion', {'x': x, 'y': y});
    expect(plate(tester), greaterThan(before + 20));
    await send('leave');
    expect(plate(tester), closeTo(before, 0.5));
  });

  group('the glyph of a folder stands until its icon is drawn', () {
    // A 1 x 1 PNG.
    final png = base64Decode(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==',
    );
    bool glyph(Widget w) => w is SilicaIcon && w.source is FlatSource;
    bool drawn(WidgetTester tester) => find
        .byType(RawImage)
        .evaluate()
        .any((e) => (e.widget as RawImage).image != null);

    testWidgets('an icon that never arrives keeps the glyph', (tester) async {
      final dir = Directory.systemTemp.createTempSync('glyph');
      addTearDown(() => dir.deleteSync(recursive: true));
      final icon = '${dir.path}/folder.svg';
      File(icon).writeAsStringSync(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8"/></svg>',
      );
      final never = Completer<List<Object?>>();
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SilicaThumbnails.channel,
        (_) => never.future,
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SilicaThumbnails.channel,
          null,
        ),
      );
      await pump(
        tester,
        items: [
          ..._items.take(3),
          Item.app(
            'New',
            command: 'x',
            folder: '${dir.path}/new',
            iconPath: icon,
          ),
          ..._items.skip(6),
        ],
      );
      for (var i = 0; i < 20; i++) {
        await tester.pump(const Duration(seconds: 1));
      }
      expect(find.byWidgetPredicate(glyph), findsOneWidget);
      // Nothing polls: no frame is scheduled and no timer is left pending.
      expect(tester.binding.transientCallbackCount, 0);
      expect(tester.binding.hasScheduledFrame, isFalse);
      expect(tester.hasRunningAnimations, isFalse);
    });

    testWidgets('it swaps to the image once drawn, with no blank frame', (
      tester,
    ) async {
      final dir = Directory.systemTemp.createTempSync('glyph');
      addTearDown(() => dir.deleteSync(recursive: true));
      final icon = '${dir.path}/folder.svg';
      File(icon).writeAsStringSync(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8"/></svg>',
      );
      final composed = '${dir.path}/composed.png';
      File(composed).writeAsBytesSync(png);
      final reply = Completer<List<Object?>>();
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SilicaThumbnails.channel,
        (_) => reply.future,
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SilicaThumbnails.channel,
          null,
        ),
      );
      await pump(
        tester,
        items: [
          ..._items.take(3),
          Item.app(
            'New',
            command: 'x',
            folder: '${dir.path}/new',
            iconPath: icon,
          ),
          ..._items.skip(6),
        ],
      );
      await tester.pump(const Duration(milliseconds: 200));
      expect(find.byWidgetPredicate(glyph), findsOneWidget);
      expect(drawn(tester), isFalse);
      reply.complete([composed, false]);
      await tester.runAsync(() async {
        for (var i = 0; i < 200 && !drawn(tester); i++) {
          await Future<void>.delayed(const Duration(milliseconds: 25));
          await tester.pump(const Duration(milliseconds: 25));
          // Never a frame with neither the glyph nor the composed image.
          expect(
            find.byWidgetPredicate(glyph).evaluate().isNotEmpty ||
                drawn(tester),
            isTrue,
          );
        }
      });
      await tester.pump();
      expect(drawn(tester), isTrue);
      expect(find.byWidgetPredicate(glyph), findsNothing);
    });
  });
}
