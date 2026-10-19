import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_ui/silica_ui.dart';
import 'package:silica_shell/faro/faro.dart' show backProgress;
import 'package:silica_shell/faro/launcher.dart';
import 'package:silica_shell/system/desktop_entries.dart';

DesktopEntry app(
  String id,
  String name, [
  List<String> categories = const [],
]) => DesktopEntry(id: id, name: name, exec: id, categories: categories);

final entries = {
  for (final e in [
    app('calc', 'Calcolatrice', ['Utility', 'Calculator']),
    app('cal', 'Calendar', ['Office', 'Calendar']),
    app('term', 'Terminal', ['System', 'TerminalEmulator']),
    app('gimp', 'Disegno', ['Graphics']),
    app('chess', 'Scacchi', ['Game']),
    app('mail', 'Mail', ['Network', 'Email']),
    app('docs', 'Lettore', ['Viewer']),
    app('odd', 'Strana'),
    for (var i = 0; i < 20; i++) app('x$i', 'App $i', ['Utility']),
  ])
    e.id: e,
};

/// A second view of the test engine, which has only one: it has its own id.
class _PopupView extends TestFlutterView {
  _PopupView(TestFlutterView base, this.id)
    : super(
        view: base,
        platformDispatcher: base.platformDispatcher,
        display: base.display,
      );

  final int id;

  @override
  int get viewId => id;
}

void main() {
  test('freedesktop categories end up in the seven of the launcher', () {
    expect(categoryOf(entries['calc']!), 'utilities');
    expect(categoryOf(entries['term']!), 'utilities');
    expect(categoryOf(entries['cal']!), 'productivity');
    expect(categoryOf(entries['gimp']!), 'creativity');
    expect(categoryOf(entries['chess']!), 'entertainment');
    expect(categoryOf(entries['mail']!), 'social');
    expect(categoryOf(entries['docs']!), 'reading');
    expect(categoryOf(entries['odd']!), 'other');
    expect(categoryNames, hasLength(7));
  });

  test(
    'search: over all apps the best first, in the category alphabetically',
    () {
      final all = launcherApps(entries.values, query: 'cal');
      expect(
        [for (final e in all.take(2)) e.name],
        ['Calcolatrice', 'Calendar'],
      );
      final utility = launcherApps(entries.values, category: 'utilities');
      expect(utility.first.name, 'App 0');
      expect(utility.every((e) => categoryOf(e) == 'utilities'), isTrue);
      expect(
        [
          for (final e in launcherApps(
            entries.values,
            category: 'utilities',
            query: 'cal',
          ))
            e.name,
        ],
        ['Calcolatrice'],
      );
    },
  );

  test('the file manager is in the grid, like Finder in Applications', () {
    const manager = DesktopEntry(
      id: fileManagerId,
      name: 'Mustache',
      exec: 'mustache',
      categories: ['System', 'FileManager'],
    );
    final all = launcherApps([...entries.values, manager]);
    expect(all.where((e) => e.id == fileManagerId), hasLength(1));
    expect(launcherApps([manager], query: 'must'), [manager]);
  });

  test('grid: recents, line and list at the kit distances', () {
    const layout = GridLayout(recents: 7, apps: 30);
    // Browsing: the area starts under the capsules (y 96). Recents' icons at y 124, the
    // line at 216, the list's icons at 236, 104 between rows (kit `Launcher`).
    expect(layout.areaTop, 96);
    expect(layout.recentCell(2), const Offset(28 + 2 * 112, 124 - 96));
    expect(layout.separatorY, 216 - 96);
    expect(layout.cell(0).dy, 236 - 96);
    expect(layout.cell(7).dy, 340 - 96);
    expect(layout.cell(8).dx, 28 + 112);
    // Without recents the first cell is at y 124; with a category or a query at y 88
    // (`Launcher Category`), the area from y 57.
    expect(const GridLayout(recents: 0, apps: 3).cell(0).dy, 124 - 96);
    const category = GridLayout(recents: 0, apps: 3, browsing: false);
    expect(category.areaTop, 57);
    expect(category.cell(0).dy, 88 - 57);
  });

  test('list: recents in cells of 116, line at 208, rows of 56 from 220', () {
    const layout = GridLayout(recents: 5, apps: 6, list: true);
    expect(layout.recentCell(1), const Offset(20 + 116, 108 - 96));
    expect(layout.separatorY, 208 - 96);
    expect(layout.rowY(0), 220 - 96);
    expect(layout.rowY(1) - layout.rowY(0), 56);
  });

  test('the panel is 844 x 580 in grid mode and 640 x 560 in list mode', () {
    expect(launcherSize(list: false), const Size(844, 580));
    expect(launcherSize(list: true), const Size(640, 560));
  });

  test('recent apps are remembered from the most recent, at most seven', () {
    final dir = Directory.systemTemp.createTempSync('recenti');
    addTearDown(() => dir.deleteSync(recursive: true));
    final recents = RecentApps(env: {'HOME': dir.path});
    expect(recents.load(), isEmpty);
    for (var i = 0; i < 9; i++) {
      recents.add('a$i');
    }
    recents.add('a3');
    expect(recents.load(), ['a3', 'a8', 'a7', 'a6', 'a5', 'a4', 'a2']);
  });

  testWidgets(
    'capsules 28 tall at y 68, 8 pt apart; the category becomes a token',
    (tester) async {
      final key = GlobalKey<LauncherState>();
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Align(
            alignment: Alignment.topLeft,
            child: Launcher(
              key: key,
              entries: entries,
              recents: const ['term', 'calc'],
              onLaunch: (_) {},
              onBack: () {},
            ),
          ),
        ),
      );
      Rect capsule(String name) => tester.getRect(
        find
            .ancestor(of: find.text(name), matching: find.byType(Container))
            .first,
      );
      final first = capsule('Productivity and finance');
      final utility = capsule('Utilities');
      expect(first.left, 20);
      expect(first.top, 68);
      expect(first.height, 28);
      expect(utility.left - first.right, 8);
      expect(find.text('Applications'), findsOneWidget);
      await tester.tap(find.text('Utilities'));
      await tester.pump(const Duration(milliseconds: 300));
      expect(key.currentState!.category, 'utilities');
      expect(find.text('Applications'), findsNothing);
      // The capsules disappear, the token stays.
      expect(find.text('Social'), findsNothing);
      expect(find.text('Utilities'), findsOneWidget);
      final token = tester.getRect(
        find.byKey(const ValueKey('launcher category token')),
      );
      expect(token.left, 56);
      expect(token.height, 32);
    },
  );

  testWidgets('a newly installed app has the dot before its name', (
    tester,
  ) async {
    final withNew = {
      ...entries,
      'fresh': const DesktopEntry(
        id: 'fresh',
        name: 'Fresh',
        exec: 'fresh',
        categories: ['Utility'],
        isNew: true,
      ),
    };
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Align(
          alignment: Alignment.topLeft,
          child: Launcher(
            entries: withNew,
            recents: const [],
            onLaunch: (_) {},
            onBack: () {},
          ),
        ),
      ),
    );
    await tester.pump();
    final dot = find.byKey(const ValueKey('launcher-new-dot'));
    expect(dot, findsOneWidget);
    // Left of the name, in the same row.
    final name = tester.getRect(find.text('Fresh'));
    final at = tester.getRect(dot);
    expect(at.right, lessThanOrEqualTo(name.left));
    expect((at.center.dy - name.center.dy).abs(), lessThan(2));
    // Kit `Apps/Faro/Launcher Item/New`: an 8 pt dot, 4 before the name.
    expect(at.size, const Size(8, 8));
    expect(name.left - at.right, 4);
    expect(newDotSize, 8);
    // Read as "<name>, new".
    expect(find.bySemanticsLabel('Fresh, new'), findsOneWidget);
  });

  testWidgets(
    'grid: Down chooses the first icon, arrows move it, Return opens (PA-60)',
    (tester) async {
      final launched = <String>[];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Align(
            alignment: Alignment.topLeft,
            child: Launcher(
              entries: entries,
              recents: const [],
              onLaunch: (e) => launched.add(e.id),
              onBack: () {},
            ),
          ),
        ),
      );
      await tester.pump();
      final all = launcherApps(entries.values);
      // Left does nothing before a choice; Return with none opens nothing.
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      expect(launched, isEmpty);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
      expect(find.text('${all[0].name} — Open'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      expect(launched, [all[1 + gridColumns].id]);
    },
  );

  testWidgets('the "…" menu switches between the grid and the list', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1200, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    var list = false;
    late StateSetter set;
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: StatefulBuilder(
          builder: (context, setState) {
            set = setState;
            return Align(
              alignment: Alignment.topLeft,
              child: Launcher(
                entries: entries,
                recents: const [],
                list: list,
                onListChanged: (value) => set(() => list = value),
                onLaunch: (_) {},
                onBack: () {},
              ),
            );
          },
        ),
      ),
    );
    expect(tester.getSize(find.byType(Launcher)), const Size(844, 580));
    await tester.tap(find.byKey(const ValueKey('launcher more')));
    await tester.pump();
    expect(find.byKey(const ValueKey('launcher view menu')), findsOneWidget);
    await tester.tap(find.text('as List'));
    await tester.pump(const Duration(milliseconds: 300));
    expect(list, isTrue);
    expect(tester.getSize(find.byType(Launcher)), const Size(640, 560));
    expect(find.byKey(const ValueKey('launcher view menu')), findsNothing);
  });

  testWidgets(
    'the view menu is a popup surface above the layer when the system offers it',
    (tester) async {
      // Ossido draws the glass of a menu below its own surface's pixels: in the Faro layer
      // the panel would cover it (silica-desktop#222).
      final calls = <MethodCall>[];
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(
        GlassRegistry.channel,
        (_) async => true,
      );
      messenger.setMockMethodCallHandler(SilicaWindow.channel, (call) async {
        calls.add(call);
        return switch (call.method) {
          'popupsAvailable' => true,
          'openPopup' => {
            'viewId': 7,
            'x': 700.0,
            'y': 44.0,
            'width': 180.0,
            'height': 60.0,
          },
          _ => null,
        };
      });
      SilicaWindows.reset();
      SilicaPopups.debugViewResolver = (id) => _PopupView(tester.view, id);
      addTearDown(() {
        messenger.setMockMethodCallHandler(SilicaWindow.channel, null);
        messenger.setMockMethodCallHandler(GlassRegistry.channel, null);
        SilicaWindows.reset();
      });
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Align(
            alignment: Alignment.topLeft,
            child: Launcher(
              entries: entries,
              recents: const [],
              onListChanged: (_) {},
              onLaunch: (_) {},
              onBack: () {},
            ),
          ),
        ),
      );
      await SilicaPopups.probe();
      await tester.tap(find.byKey(const ValueKey('launcher more')));
      for (var i = 0; i < 5; i++) {
        await tester.pump();
      }
      final opened = calls.where((call) => call.method == 'openPopup').toList();
      expect(opened, hasLength(1));
      final args = opened.single.arguments as Map;
      // Hanging from the menu's top right corner: 20 from the right edge, 44 from the top.
      expect(args['anchorRect'], [824.0, 44.0, 1.0, 1.0]);
      expect(args['anchor'], 'top-right');
      expect(args['gravity'], 'bottom-left');
      expect(args['parent'], 0);
      // Built in the popup's view, not in the layer.
      expect(find.byKey(const ValueKey('launcher view menu')), findsOneWidget);
    },
  );

  test('inline completion: only if the first result starts with the text', () {
    final results = launcherApps(entries.values, query: 'cal');
    expect(completionFor('cal', results), 'colatrice');
    expect(completionFor('Cal', results), 'colatrice');
    expect(
      completionFor('lat', launcherApps(entries.values, query: 'lat')),
      isNull,
    );
    expect(completionFor('', results), isNull);
  });

  test('the return to Faro follows the measured height', () {
    expect(backProgress(0), 0);
    // 577 → 234 pt at 106.7 ms: 343 / 520 (the measured curve, kept).
    expect(backProgress(106.7), closeTo(343 / 520, 0.001));
    expect(backProgress(300), 1);
  });

  testWidgets(
    'typing completes the name and puts the ring on the first result',
    (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Align(
            alignment: Alignment.topLeft,
            child: Launcher(
              entries: entries,
              recents: const [],
              onLaunch: (_) {},
              onBack: () {},
            ),
          ),
        ),
      );
      await tester.enterText(find.byType(EditableText), 'cal');
      await tester.pump();
      expect(find.text('colatrice — Open'), findsOneWidget);
      // The ring: a 4 pt border around the 60 pt icon of the first result.
      final ring = find.byWidgetPredicate(
        (w) =>
            w is DecoratedBox &&
            w.decoration is BoxDecoration &&
            (w.decoration as BoxDecoration).border != null,
      );
      expect(ring, findsOneWidget);
      expect(tester.getSize(ring), const Size(68, 68));
    },
  );
}
