import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/gestures.dart' show PointerDeviceKind, kSecondaryButton;
import 'package:flutter/services.dart' show LogicalKeyboardKey;
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/dock/dock_menu.dart';
import 'package:silica_shell/dock/glass_groups.dart';
import 'package:silica_shell/dock/model.dart';
import 'package:silica_shell/dock/stack.dart';
import 'package:silica_shell/dock/stack_settings.dart';
import 'package:silica_shell/system/layer_control.dart';
import 'package:silica_shell/system/silica_symbol.dart';
import 'package:silica_ui/silica_ui.dart';

// The Dock stacks (#162-#164): the Downloads folder end to end, the fan's spacing and glass
// labels, the grid's glass tip, the glass close and open-folder controls.
/// Any glass, the shell's `ProfiledGlass` included (`find.byType` matches the exact type).
final anyGlass = find.byWidgetPredicate((w) => w is Glass);

void main() {
  setUp(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(GlassRegistry.channel, (call) async => true);
  });

  late Directory dir;
  late List<String> opened;
  late Offset stackIcon;
  late double secondX;

  /// The menu of the open stack's own icon (right click on it).
  Future<void> openMenu(WidgetTester tester) async {
    await tester.tapAt(stackIcon, buttons: kSecondaryButton);
    for (var i = 0; i < 4; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
  }

  /// The glass regions the Dock sends to Ossido, as (group, material, profile).
  List<(int, String, Object?)> regions() => [
    for (final r in GlassRegistry.instance.regions)
      ((r['group'] as int?) ?? 0, r['material'] as String, r['profile']),
  ];

  Future<void> openStack(
    WidgetTester tester,
    int files, {
    StackSettings settings = const StackSettings(),
    void Function(Directory)? prepare,
    List<Item> others = const [],
  }) async {
    dir = Directory.systemTemp.createTempSync('Downloads');
    addTearDown(() => dir.deleteSync(recursive: true));
    for (var i = 1; i <= files; i++) {
      File('${dir.path}/documento-$i.txt').writeAsStringSync('x');
    }
    prepare?.call(dir);
    tester.view.physicalSize = const Size(2560, 1440);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    opened = [];
    final items = [
      Item.app('Downloads', command: 'x', folder: dir.path),
      ...others,
    ];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: items,
          onOpenPath: opened.add,
          stackSettingsFor: (_) => settings,
          menuFor: (_) => [
            MenuAction('Options', onSelected: () {}),
            MenuAction('Open', onSelected: () {}),
          ],
        ),
      ),
    );
    final rest = layout(items, width: 2560, height: 1440);
    secondX = items.length > 1 ? rest.center(1) : 0;
    stackIcon = Offset(rest.center(0), 1440 - bottomGap - backgroundHeight / 2);
    await tester.tapAt(stackIcon);
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 60));
    }
  }

  testWidgets('Downloads: opens and lists its files in the configured order', (
    tester,
  ) async {
    await openStack(
      tester,
      0,
      settings: const StackSettings(sort: StackSort.kind),
      prepare: (d) {
        for (final name in ['b.zip', 'a.txt', 'c.md', '.hidden']) {
          File('${d.path}/$name').writeAsStringSync('x');
        }
        Directory('${d.path}/folder').createSync();
      },
    );
    // By kind: the folder, then md, txt, zip; the hidden file is not listed.
    final names = ['folder', 'c.md', 'a.txt', 'b.zip'];
    final ys = [for (final n in names) tester.getCenter(find.text(n)).dy];
    // The first is at the bottom, next to the Dock.
    expect(ys, orderedEquals([...ys]..sort((a, b) => b.compareTo(a))));
    expect(find.text('.hidden'), findsNothing);
    expect(find.byType(FanStack), findsOneWidget);
    await tester.tap(find.text('c.md'));
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(opened, ['${dir.path}/c.md']);
  });

  testWidgets('fan: 24 pt glass labels, rotated with their icon, 5.5 pt away', (
    tester,
  ) async {
    await openStack(tester, 4);
    final labels = find.byType(DockLabel);
    // Four files and "Open folder".
    expect(labels, findsNWidgets(5));
    // Their glass is the Dock label's and the menus': thick, no tip.
    final glasses = tester
        .renderObjectList<RenderGlass>(
          find.descendant(of: labels, matching: anyGlass),
        )
        .toList();
    expect(glasses, hasLength(5));
    for (final glass in glasses) {
      expect(glass.material, DockLabel.material);
      expect(glass.tip, isNull);
    }
    // 28 pt, no shadow, to the left of their icon: 5.5 pt from the icon's frame (the
    // bounding box of the rotated 64 pt icon) along the row.
    expect(tester.getSize(find.byType(DockLabel).first).height, fanLabelHeight);
    for (final label in tester.widgetList<DockLabel>(labels)) {
      expect(label.shadow, isFalse);
      expect(label.height, fanLabelHeight);
    }
    final icon = find.byKey(const ValueKey('fan icon 0'));
    expect(tester.getSize(icon), const Size.square(fanIcon));
    // In the item's row (rotated as a whole): label, gap, icon.
    final row = tester.widget<Row>(
      find.ancestor(of: icon, matching: find.byType(Row)).first,
    );
    final gap = row.children[1] as SizedBox;
    expect(
      gap.width,
      closeTo(fanLabelGap + (fanSlot(0).frame - fanIcon) / 2, 1e-9),
    );
  });

  testWidgets('fan: each label glass declares its tilt, not an upright box', (
    tester,
  ) async {
    await openStack(tester, 4);
    // Fully open.
    for (var i = 0; i < 10; i++) {
      await tester.pump(const Duration(milliseconds: 60));
    }
    final byGroup = {
      for (final r in GlassRegistry.instance.regions)
        (r['group'] as int?) ?? 0: r,
    };
    // Four files and "Open folder", from the bottom: 1.63° then 1.23° more per item.
    for (var i = 0; i < 5; i++) {
      final region = byGroup[DockGlass.fanLabel(i)];
      expect(region, isNotNull, reason: 'label $i');
      final frame = region!['frame'] as Map<String, Object>?;
      expect(frame, isNotNull, reason: 'label $i sends no rotation');
      final t = (frame!['transform'] as List<Object>).cast<double>();
      final degrees = math.atan2(t[1], t[0]) * 180 / math.pi;
      expect(
        degrees,
        closeTo(fanFirstAngle + fanAngleStep * i, 1e-6),
        reason: 'label $i',
      );
      // Its own capsule, before the turn: 24 pt tall, radius 8 (the tooltip's), the width the
      // label's.
      expect(frame['height'], fanLabelHeight);
      expect(frame['radius'], 8);
      final width = frame['width']! as double;
      expect(width, greaterThan(fanLabelHeight));
      // Where the transform puts the capsule's centre is the label's centre on screen.
      final label = find.byType(DockLabel).at(i);
      final glass = tester.renderObject<RenderGlass>(
        find.descendant(of: label, matching: anyGlass),
      );
      final centre = glass.localToGlobal(glass.size.center(Offset.zero));
      final (cx, cy) = (width / 2, fanLabelHeight / 2);
      expect(t[0] * cx + t[2] * cy + t[4], closeTo(centre.dx, 1e-6));
      expect(t[1] * cx + t[3] * cy + t[5], closeTo(centre.dy, 1e-6));
    }
    // The Dock itself is upright: no frame.
    expect(byGroup[DockGlass.dock]!.containsKey('frame'), isFalse);
  });

  testWidgets('close button: the silica-icons Stack Close badge', (
    tester,
  ) async {
    await openStack(tester, 3);
    final button = find.byType(StackCloseButton);
    expect(button, findsOneWidget);
    // The folder icon's canvas: a square of the icon side, at rest.
    final tile = tester.getRect(button);
    expect(tile.width, tile.height);
    expect(tile.width, closeTo(defaultCanvas, 0.5));
    // Not compositor glass, no shell-drawn cross: neither a chevron symbol nor an app icon.
    expect(find.descendant(of: button, matching: anyGlass), findsNothing);
    expect(
      find.descendant(of: button, matching: find.byType(SilicaSymbol)),
      findsNothing,
    );
    expect(
      find.descendant(of: button, matching: find.byType(SilicaIcon)),
      findsNothing,
    );
    await tester.tap(button);
    // Closed by its button in ~300 ms.
    for (var i = 0; i < 8; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(find.byType(FanStack), findsNothing);
    expect(find.byType(StackCloseButton), findsNothing);
  });

  test('close badge: badges.json > dock > stack-close, light and dark', () {
    final root = Directory.systemTemp.createTempSync('badges');
    addTearDown(() => root.deleteSync(recursive: true));
    Directory('${root.path}/badges').createSync(recursive: true);
    File('${root.path}/badges/badges.json').writeAsStringSync(
      '{"dock":{"stack-close":{"file":"stack-close.svg",'
      '"dark":"stack-close-dark.svg","rendered":{"default":"stack-close.png",'
      '"dark":"stack-close-dark.png"}}}}',
    );
    final env = {'SILICA_ICONS_DIR': root.path, 'HOME': root.path};
    // Without the PNG there is no badge.
    expect(stackCloseBadgePath(dark: false, env: env), isNull);
    File('${root.path}/badges/stack-close.png').writeAsBytesSync([0]);
    File('${root.path}/badges/stack-close-dark.png').writeAsBytesSync([0]);
    expect(
      stackCloseBadgePath(dark: false, env: env),
      '${root.path}/badges/stack-close.png',
    );
    expect(
      stackCloseBadgePath(dark: true, env: env),
      '${root.path}/badges/stack-close-dark.png',
    );
  });

  testWidgets('glass groups: every surface of the open grid is its own', (
    tester,
  ) async {
    await openStack(
      tester,
      5,
      settings: const StackSettings(content: StackContent.grid),
    );
    final groups = regions();
    // Dock and panel (body and tip); the open-folder cell is a stroked circle, not glass.
    expect(groups.where((r) => r.$1 == DockGlass.dock), hasLength(1));
    expect(groups.where((r) => r.$1 == DockGlass.gridPanel), hasLength(2));
    expect(groups.map((r) => r.$1).toSet(), hasLength(2));
    expect(
      find.descendant(of: find.byType(OpenFolderButton), matching: anyGlass),
      findsNothing,
    );
    // From the bottom: the Dock, the panel.
    expect(DockGlass.dock, lessThan(DockGlass.gridPanel));
    // Profiles (#168): the panel is a panel, the button a control.
    expect(
      groups.where((r) => r.$1 == DockGlass.gridPanel).map((r) => r.$3),
      everyElement('panel'),
    );
  });

  testWidgets('glass groups: each fan label on its own, the button flat', (
    tester,
  ) async {
    await openStack(tester, 4);
    final labels = tester
        .renderObjectList<RenderGlass>(
          find.descendant(of: find.byType(DockLabel), matching: anyGlass),
        )
        .map((g) => g.group)
        .toList();
    expect(labels, hasLength(5));
    expect(labels.toSet(), hasLength(5));
    expect(labels, everyElement(lessThan(0)));
    expect(
      find.descendant(of: find.byType(FanFolderButton), matching: anyGlass),
      findsNothing,
    );
  });

  for (final view in [StackContent.grid, StackContent.fan]) {
    testWidgets(
      'a menu over the open ${view.name}: its own glass, on top, the stack cut under it',
      (tester) async {
        await openStack(tester, 5, settings: StackSettings(content: view));
        await openMenu(tester);
        expect(find.byKey(dockMenuSurface), findsOneWidget);
        // The stack stays open under the menu.
        expect(
          find.byType(view == StackContent.grid ? GridStack : FanStack),
          findsOneWidget,
        );
        final menu = tester.renderObject<RenderGlass>(
          find.descendant(of: find.byKey(dockMenuSurface), matching: anyGlass),
        );
        // The menu's group is above every glass of the stack and of the Dock: no merging,
        // the menu on top.
        final others = regions().where((r) => r.$1 != menu.group).toList();
        expect(others, isNotEmpty);
        for (final r in others) {
          expect(r.$1, lessThan(menu.group));
        }
        // The menu is painted after the stack, and the stack's content is cut where the menu
        // lies (its glass alone cannot cover the stack's icons).
        final cut = tester.widget<StackUnderMenu>(find.byType(StackUnderMenu));
        final panel = tester.getRect(find.byKey(dockMenuSurface));
        expect(cut.menu, isNotEmpty);
        final clip = MenuHoleClipper(cut.menu).getClip(const Size(2560, 1440));
        expect(clip.contains(panel.center), isFalse);
        expect(clip.contains(panel.topLeft - const Offset(20, 20)), isTrue);
        // Escape closes the menu first; the stack stays.
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
        for (var i = 0; i < 6; i++) {
          await tester.pump(const Duration(milliseconds: 50));
        }
        expect(find.byKey(dockMenuSurface), findsNothing);
        expect(
          find.byType(view == StackContent.grid ? GridStack : FanStack),
          findsOneWidget,
        );
        expect(
          tester.widget<StackUnderMenu>(find.byType(StackUnderMenu)).menu,
          isEmpty,
        );
        // A second Escape closes the stack.
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
        for (var i = 0; i < 6; i++) {
          await tester.pump(const Duration(milliseconds: 50));
        }
        expect(find.byType(GridStack), findsNothing);
        expect(find.byType(FanStack), findsNothing);
      },
    );
  }

  testWidgets('opening: a frame moves the items, it does not rebuild them', (
    tester,
  ) async {
    dir = Directory.systemTemp.createTempSync('Downloads');
    addTearDown(() => dir.deleteSync(recursive: true));
    for (var i = 1; i <= 6; i++) {
      File('${dir.path}/documento-$i.txt').writeAsStringSync('x');
    }
    tester.view.physicalSize = const Size(2560, 1440);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    var dockBuilds = 0;
    final items = [Item.app('Downloads', command: 'x', folder: dir.path)];
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Dock(
          items: items,
          onOpenPath: (_) {},
          stackSettingsFor: (_) {
            dockBuilds++;
            return const StackSettings();
          },
        ),
      ),
    );
    final rest = layout(items, width: 2560, height: 1440);
    await tester.tapAt(
      Offset(rest.center(0), 1440 - bottomGap - backgroundHeight / 2),
    );
    await tester.pump(const Duration(milliseconds: 16));
    final icon = tester.widget(find.byKey(const ValueKey('fan icon 2')));
    final label = tester.widget(find.byType(DockLabel).at(2));
    final before = dockBuilds;
    for (var i = 0; i < 8; i++) {
      await tester.pump(const Duration(milliseconds: 16));
    }
    // The same widgets: neither the icons nor the names are resolved again.
    expect(
      identical(tester.widget(find.byKey(const ValueKey('fan icon 2'))), icon),
      isTrue,
    );
    expect(
      identical(tester.widget(find.byType(DockLabel).at(2)), label),
      isTrue,
    );
    // And the Dock itself (it asks the stack settings in its build) is not rebuilt per frame.
    expect(dockBuilds - before, lessThan(3));
  });

  testWidgets(
    'open folder (fan): a flat 58 pt circle, pressed only the glyph dims, it opens the folder',
    (tester) async {
      await openStack(tester, 3);
      final button = find.byType(FanFolderButton);
      expect(button, findsOneWidget);
      final circle = find.descendant(
        of: button,
        matching: find.byType(Container),
      );
      expect(tester.getSize(circle), const Size.square(fanFolderCircle));
      final box = tester.widget<Container>(circle).decoration! as BoxDecoration;
      expect(box.shape, BoxShape.circle);
      expect(box.color, fanFolderFill);
      Color glyph() => tester
          .widget<SilicaSymbol>(
            find.descendant(of: button, matching: find.byType(SilicaSymbol)),
          )
          .color;
      expect(
        tester
            .widget<SilicaSymbol>(
              find.descendant(of: button, matching: find.byType(SilicaSymbol)),
            )
            .name,
        FolderArrow.symbol,
      );
      expect(glyph(), fanFolderGlyph);
      final gesture = await tester.startGesture(tester.getCenter(button));
      await tester.pump();
      expect(glyph().a, closeTo(fanFolderPressedGlyph, 0.01));
      // The circle does not change.
      expect(
        (tester.widget<Container>(circle).decoration! as BoxDecoration).color,
        fanFolderFill,
      );
      await gesture.up();
      for (var i = 0; i < 6; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(opened, [dir.path]);
    },
  );

  testWidgets('grid: the open-folder cell is a stroked circle, not glass', (
    tester,
  ) async {
    await openStack(
      tester,
      5,
      settings: const StackSettings(content: StackContent.grid),
    );
    final button = find.byType(OpenFolderButton);
    expect(button, findsOneWidget);
    expect(find.descendant(of: button, matching: anyGlass), findsNothing);
    final box =
        tester
                .widget<Container>(
                  find.descendant(of: button, matching: find.byType(Container)),
                )
                .decoration!
            as BoxDecoration;
    expect(box.color, isNull);
    expect(box.border, isNotNull);
    await tester.tap(button);
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(opened, [dir.path]);
  });

  testWidgets(
    'grid: thick panel glass, the tip is the popover arrow, 20 x 8 pt',
    (tester) async {
      await openStack(
        tester,
        40,
        settings: const StackSettings(content: StackContent.grid),
      );
      final panel = tester
          .renderObjectList<RenderGlass>(
            find.descendant(of: find.byType(GridStack), matching: anyGlass),
          )
          .first;
      expect(panel.material, GlassMaterial.thick);
      expect(panel.tip!.edge, TipEdge.bottom);
      expect(panel.tip!.width, 20);
      expect(panel.tip!.height, 8);
    },
  );

  testWidgets(
    'close button: pressed it darkens, released the stack closes in 187 ms',
    (tester) async {
      await openStack(tester, 3);
      final button = find.byType(StackCloseButton);
      expect(tester.widget<StackCloseButton>(button).pressed, isFalse);
      final gesture = await tester.startGesture(tester.getCenter(button));
      await tester.pump();
      expect(tester.widget<StackCloseButton>(button).pressed, isTrue);
      await gesture.up();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 150));
      // Still closing at 150 ms (187 ms from the release).
      expect(find.byType(FanStack), findsOneWidget);
      await tester.pump(const Duration(milliseconds: 60));
      await tester.pump();
      expect(find.byType(FanStack), findsNothing);
    },
  );

  testWidgets(
    'the stack shows once the surface grows, even short of the screen (menu bar zone)',
    (tester) async {
      // Ossido keeps the Dock below the menu bar's exclusive zone: asked for the screen's
      // 800 pt, the surface gets 770.
      final heights = <double>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        LayerControl.channel,
        (call) async {
          if (call.method == 'outputSize') {
            return {'width': 1280, 'height': 800};
          }
          if (call.method == 'setSize') {
            heights.add(((call.arguments as Map)['height'] as num).toDouble());
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
      dir = Directory.systemTemp.createTempSync('Downloads');
      addTearDown(() => dir.deleteSync(recursive: true));
      for (var i = 1; i <= 3; i++) {
        File('${dir.path}/documento-$i.txt').writeAsStringSync('x');
      }
      tester.view.physicalSize = const Size(1280, 200);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final items = [Item.app('Downloads', command: 'x', folder: dir.path)];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(
            items: items,
            onOpenPath: (_) {},
            stackSettingsFor: (_) => const StackSettings(),
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 50));
      final rest = layout(items, width: 1280, height: 200);
      await tester.tapAt(
        Offset(rest.center(0), 200 - bottomGap - backgroundHeight / 2),
      );
      for (var i = 0; i < 3; i++) {
        await tester.pump(const Duration(milliseconds: 30));
      }
      // The compositor answers with 770 instead of 800.
      tester.view.physicalSize = const Size(1280, 770);
      for (var i = 0; i < 8; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(find.byType(FanStack), findsOneWidget);
      expect(find.byType(DockLabel), findsNWidgets(4));
    },
  );

  test('a silica-icons symbol is read from symbols/<weight>', () {
    final root = Directory.systemTemp.createTempSync('icons');
    addTearDown(() => root.deleteSync(recursive: true));
    Directory('${root.path}/symbols/regular').createSync(recursive: true);
    File('${root.path}/symbols/regular/chevron-down.svg').writeAsStringSync(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"/>',
    );
    final env = {'SILICA_ICONS_DIR': root.path, 'HOME': root.path};
    expect(symbolSvg('chevron-down', env: env), contains('<svg'));
    expect(symbolSvg('missing', env: env), isNull);
    expect(symbolSvg('../x', env: env), isNull);
  });

  for (final content in [StackContent.fan, StackContent.grid]) {
    testWidgets('hover label: none for the open stack, others below it '
        '(${content.name})', (tester) async {
      await openStack(
        tester,
        3,
        settings: StackSettings(content: content),
        others: [Item.app('Notes', command: 'notes')],
      );
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(5, 5));
      // The open stack's own item: no label.
      await mouse.moveTo(stackIcon);
      for (var i = 0; i < 4; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(
        find.widgetWithText(DockLabel, 'Downloads'),
        findsNothing,
        reason: 'no hover label for the open stack',
      );
      // Another item: its label shows, in a glass group below the stack's.
      final other = Offset(secondX, stackIcon.dy);
      await mouse.moveTo(other);
      for (var i = 0; i < 4; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(find.widgetWithText(DockLabel, 'Notes'), findsOneWidget);
      final groups = regions().map((r) => r.$1).toSet();
      expect(groups, contains(DockGlass.label));
      // The stack's own glass is above the label's group.
      expect(
        groups.where((g) => g > DockGlass.label),
        isNotEmpty,
        reason: 'the stack draws above the hover label',
      );
    });
  }

  test('glass order: Dock, hover label, stack panel, fan labels', () {
    expect(DockGlass.dock, lessThan(DockGlass.label));
    expect(DockGlass.label, lessThan(DockGlass.gridPanel));
    expect(DockGlass.gridPanel, lessThan(DockGlass.fanLabel(0)));
    expect(DockGlass.label, lessThan(DockGlass.fanLabel(0)));
    expect(DockGlass.fanLabel(1000), lessThan(0));
  });
}
