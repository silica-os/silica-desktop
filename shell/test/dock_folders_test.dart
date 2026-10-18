// Dock folders and plate (#235): folders reorder among themselves, the Trash removes what is
// dropped on it, and the plate slides with the icons.

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/dock/model.dart';

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
}
