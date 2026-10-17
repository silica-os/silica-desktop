import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/apps.dart' show downloadDir;
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/dock/dock_menu.dart';
import 'package:silica_shell/dock/model.dart';
import 'package:silica_shell/dock/stack.dart';
import 'package:silica_shell/dock/stack_settings.dart';
import 'package:silica_shell/system/glass_profile.dart';
import 'package:silica_ui/silica_ui.dart';

void main() {
  setUp(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(GlassRegistry.channel, (call) async => true);
  });

  test('the fan follows the measurements (DOCK-FAN)', () {
    // Stack item centre x 1672.2, Dock top edge 1356 (the Dock area 7 pt higher), 10 files and
    // "open in Finder" (dock-stacks.md, DOCK-FAN): measured icon centres and AX sides.
    const centres = [
      (1674.0, 1311.0, 65.8),
      (1676.0, 1247.0, 67.1),
      (1680.0, 1183.0, 68.4),
      (1685.0, 1119.0, 69.6),
      (1692.0, 1055.0, 70.9),
      (1700.0, 991.0, 72.1),
      (1709.0, 927.0, 73.2),
      (1720.0, 863.0, 74.4),
      (1733.0, 799.0, 75.5),
      (1746.0, 735.0, 76.5),
      (1761.0, 671.0, 77.6),
    ];
    for (var i = 0; i < centres.length; i++) {
      final s = fanSlot(i);
      final (x, y, frame) = centres[i];
      expect(1672.2 + s.center.dx, closeTo(x, 1.6), reason: 'x of $i');
      expect(1356 - 5 + s.center.dy, closeTo(y, 0.6), reason: 'y of $i');
      expect(s.frame, closeTo(frame, 0.2), reason: 'frame of $i');
      expect(s.side, fanIcon);
    }
    // ~1.6° for the first, ~1.22° more per item.
    expect(fanSlot(0).angle * 180 / math.pi, closeTo(1.6, 0.01));
    expect(
      (fanSlot(9).angle - fanSlot(8).angle) * 180 / math.pi,
      closeTo(1.22, 0.01),
    );
    // Centres 64 pt apart vertically.
    expect(fanSlot(4).center.dy - fanSlot(5).center.dy, closeTo(64, 1e-9));
  });

  test('automatic view and grid shape', () {
    expect(automaticView(10), StackView.fan);
    expect(automaticView(40), StackView.grid);
    expect(gridShape(3), (2, 2));
    expect(gridShape(10), (4, 3));
    expect(gridShape(40), (7, 6));
    expect(gridPanelSize(3, 2000).width, 290);
    expect(gridPanelSize(10, 2000).width, 546);
    // Surface still low: no exceptions, the minimum row.
    expect(gridPanelSize(40, 71).height, 32 + 128 + 11);
  });

  test('natural order: documento-2 before documento-10', () {
    final names = ['documento-10.txt', 'Documento-2.txt', 'documento-1.txt'];
    names.sort(naturalCompare);
    expect(names, ['documento-1.txt', 'Documento-2.txt', 'documento-10.txt']);
  });

  test('a path with quotes stays a single argument', () {
    expect(shellQuote("/tmp/l'uno.txt"), "'/tmp/l'\\''uno.txt'");
  });

  testWidgets(
    'a click on the folder opens the fan; a click on a file opens it',
    (tester) async {
      final dir = Directory.systemTemp.createTempSync('pila');
      addTearDown(() => dir.deleteSync(recursive: true));
      for (final name in ['b.txt', 'a.txt', '.nascosto']) {
        File('${dir.path}/$name').writeAsStringSync('x');
      }
      tester.view.physicalSize = const Size(1280, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final opened = <String>[];
      final items = [Item.app('Stack', command: 'x', folder: dir.path)];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(items: items, onOpenPath: opened.add),
        ),
      );
      final rest = layout(items, width: 1280, height: 1000);
      await tester.tapAt(
        Offset(rest.center(0), 1000 - bottomGap - backgroundHeight / 2),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      // By name, without the hidden ones, and "Open folder" on top.
      expect(find.text('a.txt'), findsOneWidget);
      expect(find.text('b.txt'), findsOneWidget);
      expect(find.text('.nascosto'), findsNothing);
      expect(find.text('Open folder'), findsOneWidget);
      expect(
        tester.getCenter(find.text('a.txt')).dy,
        greaterThan(tester.getCenter(find.text('b.txt')).dy),
      );
      await tester.tap(find.text('b.txt'));
      // The stack closes in ~210 ms, over several frames.
      for (var i = 0; i < 6; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(opened, ['${dir.path}/b.txt']);
      expect(find.text('a.txt'), findsNothing);
    },
  );

  testWidgets('with 40 files the stack is a grid with the title', (
    tester,
  ) async {
    final dir = Directory.systemTemp.createTempSync('griglia');
    addTearDown(() => dir.deleteSync(recursive: true));
    for (var i = 1; i <= 40; i++) {
      File('${dir.path}/documento-$i.txt').writeAsStringSync('x');
    }
    tester.view.physicalSize = const Size(2560, 1440);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final items = [Item.app('Download', command: 'x', folder: dir.path)];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(items: items, onOpenPath: (_) {}),
      ),
    );
    final rest = layout(items, width: 2560, height: 1440);
    await tester.tapAt(
      Offset(rest.center(0), 1440 - bottomGap - backgroundHeight / 2),
    );
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 60));
    }
    expect(find.text('documento-1.txt'), findsOneWidget);
    expect(find.text('Download'), findsOneWidget);
    // 7 columns: 7 × 128 + 34 = 930 pt.
    final panel = tester.getRect(
      find
          .descendant(
            of: find.byType(GridStack),
            matching: find.byType(ProfiledGlass),
          )
          .first,
    );
    expect(panel.width, closeTo(930, 1));
  });

  testWidgets('grid cells: 92 icon from y 10, name on one line', (
    tester,
  ) async {
    final dir = Directory.systemTemp.createTempSync('griglia');
    addTearDown(() => dir.deleteSync(recursive: true));
    for (var i = 1; i <= 20; i++) {
      File(
        '${dir.path}/documento con un nome molto lungo $i.txt',
      ).writeAsStringSync('x');
    }
    tester.view.physicalSize = const Size(2560, 1440);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final items = [Item.app('Download', command: 'x', folder: dir.path)];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: items,
          onOpenPath: (_) {},
          stackSettingsFor: (_) =>
              const StackSettings(content: StackContent.grid),
        ),
      ),
    );
    final rest = layout(items, width: 2560, height: 1440);
    await tester.tapAt(
      Offset(rest.center(0), 1440 - bottomGap - backgroundHeight / 2),
    );
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 60));
    }
    final label = find.textContaining('documento');
    expect(label, findsWidgets);
    final text = tester.widget<Text>(label.first);
    // One line, truncated in the middle: the end of the name stays.
    expect(text.maxLines, 1);
    expect(text.data, contains('…'));
    expect(text.data, endsWith('.txt'));
    final icon = tester.getRect(
      find.byKey(
        ValueKey('icon ${dir.path}/documento con un nome molto lungo 1.txt'),
      ),
    );
    expect(icon.width, gridIconCanvas);
  });

  testWidgets('List view: a menu with the files, Options and Open', (
    tester,
  ) async {
    final dir = Directory.systemTemp.createTempSync('elenco');
    addTearDown(() => dir.deleteSync(recursive: true));
    for (final name in ['b.txt', 'a.txt']) {
      File('${dir.path}/$name').writeAsStringSync('x');
    }
    tester.view.physicalSize = const Size(1280, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final items = [Item.app('Stack', command: 'x', folder: dir.path)];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: items,
          onOpenPath: (_) {},
          stackSettingsFor: (_) =>
              const StackSettings(content: StackContent.list),
          stackListMenu: (item, entries) => [
            for (final e in entries) MenuAction(fileName(e.path)),
            const MenuDivider(),
            const MenuAction('Options'),
          ],
        ),
      ),
    );
    final rest = layout(items, width: 1280, height: 1000);
    await tester.tapAt(
      Offset(rest.center(0), 1000 - bottomGap - backgroundHeight / 2),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byKey(dockMenuSurface), findsOneWidget);
    expect(
      tester.getTopLeft(find.text('a.txt')).dy,
      lessThan(tester.getTopLeft(find.text('b.txt')).dy),
    );
    expect(find.text('Options'), findsOneWidget);
    // No fan.
    expect(find.text('Open folder'), findsNothing);
  });

  testWidgets('shown as a stack: the icon is the first three files', (
    tester,
  ) async {
    final dir = Directory.systemTemp.createTempSync('pila');
    addTearDown(() => dir.deleteSync(recursive: true));
    for (final name in ['a.txt', 'b.txt', 'c.txt', 'd.txt']) {
      File('${dir.path}/$name').writeAsStringSync('x');
    }
    tester.view.physicalSize = const Size(1280, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    Widget dock(StackDisplay display) => Directionality(
      textDirection: TextDirection.ltr,
      child: Dock(
        items: [Item.app('Stack', command: 'x', folder: dir.path)],
        onOpenPath: (_) {},
        stackSettingsFor: (_) => StackSettings(display: display),
      ),
    );
    await tester.pumpWidget(dock(StackDisplay.stack));
    await tester.pump();
    final pile = find.byType(PileIcon);
    expect(pile, findsOneWidget);
    expect(tester.widget<PileIcon>(pile).sources, hasLength(3));
    await tester.pumpWidget(dock(StackDisplay.folder));
    await tester.pump();
    expect(find.byType(PileIcon), findsNothing);
  });

  test('stack order: by date from the most recent, by kind', () {
    final dir = Directory.systemTemp.createTempSync('ordine');
    addTearDown(() => dir.deleteSync(recursive: true));
    final old = File('${dir.path}/b.txt')..writeAsStringSync('x');
    old.setLastModifiedSync(DateTime(2020));
    File('${dir.path}/a.md').writeAsStringSync('x');
    File('${dir.path}/c.zip').writeAsStringSync('x');
    List<String> names(StackSort sort) => [
      for (final e in stackEntries(dir.path, sort: sort)) fileName(e.path),
    ];
    expect(names(StackSort.name), ['a.md', 'b.txt', 'c.zip']);
    expect(names(StackSort.modified).last, 'b.txt');
    expect(names(StackSort.kind), ['a.md', 'b.txt', 'c.zip']);
  });

  test('stack settings saved by path', () {
    final dir = Directory.systemTemp.createTempSync('config');
    addTearDown(() => dir.deleteSync(recursive: true));
    final env = {'XDG_CONFIG_HOME': dir.path, 'HOME': dir.path};
    expect(loadStackSettings(env: env), isEmpty);
    saveStackSettings({
      '/home/io/Download': const StackSettings(
        sort: StackSort.added,
        display: StackDisplay.folder,
        content: StackContent.list,
      ),
    }, env: env);
    final back = loadStackSettings(env: env)['/home/io/Download'];
    expect(back?.sort, StackSort.added);
    expect(back?.display, StackDisplay.folder);
    expect(back?.content, StackContent.list);
  });

  test('the Download folder from xdg-user-dirs in XDG_CONFIG_HOME', () {
    final dir = Directory.systemTemp.createTempSync('dirs');
    addTearDown(() => dir.deleteSync(recursive: true));
    Directory('${dir.path}/Scaricamenti').createSync();
    File('${dir.path}/cfg/user-dirs.dirs')
      ..createSync(recursive: true)
      ..writeAsStringSync('XDG_DOWNLOAD_DIR="\$HOME/Scaricamenti"\n');
    expect(
      downloadDir(dir.path, env: {'XDG_CONFIG_HOME': '${dir.path}/cfg'}),
      '${dir.path}/Scaricamenti',
    );
  });

  test('pile geometry: 50 pt canvases at x 7, y 17 / 15 / 13, scaled by k', () {
    // c = 60 (k = 1): the exact frames of the design session.
    expect(PileIcon.frames(60, 3), const [
      Rect.fromLTWH(7, 17, 50, 50),
      Rect.fromLTWH(7, 15, 50, 50),
      Rect.fromLTWH(7, 13, 50, 50),
    ]);
    // At most three, front first.
    expect(PileIcon.frames(60, 5), hasLength(3));
    expect(PileIcon.frames(60, 1), const [Rect.fromLTWH(7, 17, 50, 50)]);
    // c = 16 (k = 4/15) and c = 128 (k = 32/15): every number scaled, whole points.
    expect(PileIcon.frames(16, 3), const [
      Rect.fromLTWH(2, 5, 13, 13),
      Rect.fromLTWH(2, 4, 13, 13),
      Rect.fromLTWH(2, 3, 13, 13),
    ]);
    expect(PileIcon.frames(128, 3), const [
      Rect.fromLTWH(15, 36, 107, 107),
      Rect.fromLTWH(15, 32, 107, 107),
      Rect.fromLTWH(15, 28, 107, 107),
    ]);
  });

  for (final canvas in [16.0, 60.0, 128.0]) {
    testWidgets(
      'pile icon: the first item in front, the rest behind (c = $canvas)',
      (tester) async {
        final metrics = DockMetrics(canvas);
        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: Align(
              alignment: Alignment.topLeft,
              child: PileIcon(
                side: canvas,
                sources: const [
                  FlatSource('/nonexistent/a.png'),
                  FlatSource('/nonexistent/b.png'),
                  FlatSource('/nonexistent/c.png'),
                ],
              ),
            ),
          ),
        );
        final icons = find.byType(SilicaIcon);
        expect(icons, findsNWidgets(3));
        // The pile's frames are in the coordinates of its own canvas origin, (2k, 10k) rounded
        // (the pile does not use the tile's centred left).
        final origin = tester.getTopLeft(find.byType(PileIcon));
        final frames = PileIcon.frames(canvas, 3);
        final sources = tester
            .widgetList<SilicaIcon>(icons)
            .map((w) => (w.source as FlatSource).path)
            .toList();
        // Painted back to front: the first source is the last widget, in front.
        expect(sources, [
          '/nonexistent/c.png',
          '/nonexistent/b.png',
          '/nonexistent/a.png',
        ]);
        for (var i = 0; i < 3; i++) {
          final rect = tester.getRect(icons.at(2 - i));
          final tile = rect.shift(
            Offset(
                  (2 * canvas / defaultCanvas).roundToDouble(),
                  metrics.iconTop,
                ) -
                origin,
          );
          expect(tile, frames[i], reason: 'item $i');
        }
      },
    );
  }

  testWidgets(
    'empty folder and "Show as Folder": the folder icon, not a pile',
    (tester) async {
      final empty = Directory.systemTemp.createTempSync('vuota');
      final full = Directory.systemTemp.createTempSync('piena');
      addTearDown(() {
        empty.deleteSync(recursive: true);
        full.deleteSync(recursive: true);
      });
      File('${full.path}/a.txt').writeAsStringSync('x');
      tester.view.physicalSize = const Size(1280, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      Widget dock(String folder, StackDisplay display) => Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: [Item.app('Stack', command: folder, folder: folder)],
          onOpenPath: (_) {},
          stackSettingsFor: (_) => StackSettings(display: display),
        ),
      );
      await tester.pumpWidget(dock(empty.path, StackDisplay.stack));
      await tester.pump();
      expect(find.byType(PileIcon), findsNothing);
      await tester.pumpWidget(dock(full.path, StackDisplay.folder));
      await tester.pump();
      expect(find.byType(PileIcon), findsNothing);
    },
  );

  test('Downloads stacks by Date Added unless configured', () {
    final home = Directory.systemTemp.createTempSync('casa');
    addTearDown(() => home.deleteSync(recursive: true));
    Directory('${home.path}/Downloads').createSync();
    final env = {'HOME': home.path, 'XDG_CONFIG_HOME': '${home.path}/cfg'};
    expect(
      defaultStackSettings('${home.path}/Downloads', env: env).sort,
      StackSort.added,
    );
    expect(
      defaultStackSettings('${home.path}/Documents', env: env).sort,
      StackSort.name,
    );
  });
}
