import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/dock/model.dart';
import 'package:silica_shell/dock/options.dart';
import 'package:silica_shell/system/glass_profile.dart';
import 'package:silica_ui/silica_ui.dart';

import 'support/settings.dart';

void main() {
  test('dock options: defaults, then the values Settings sets', () async {
    final backend = await useSettings();
    final settings = SilicaSettings.instance;
    expect(DockOptions.of(settings), const DockOptions());
    backend.emit('dock.autohide', true);
    backend.emit('dock.position', 'left');
    backend.emit('dock.minimizeEffect', 'scale');
    backend.emit('dock.indicators', false);
    final o = DockOptions.of(settings);
    expect(o.autohide, isTrue);
    expect(o.position, DockPosition.left);
    expect(o.minimizeEffect, MinimizeEffect.scale);
    expect(o.indicators, isFalse);
    expect(o.magnify, isFalse);
    // A broken value falls back to the default.
    backend.emit('dock.position', 'top');
    expect(DockOptions.of(settings).position, DockPosition.bottom);
  });

  test('the watcher follows a Changed signal at once', () async {
    final backend = await useSettings();
    final watcher = DockOptionsWatcher();
    addTearDown(watcher.dispose);
    expect(watcher.value.value.autohide, isFalse);
    var seen = 0;
    watcher.value.addListener(() => seen++);
    backend.emit('dock.autohide', true);
    expect(watcher.value.value.autohide, isTrue);
    backend.emit('dock.tileSize', 122);
    expect(watcher.value.value.tileSize, 122);
    expect(seen, 2);
    // Setting from the Dock itself shows at once too.
    setDockOption('position', 'right');
    expect(watcher.value.value.position, DockPosition.right);
  });

  testWidgets(
    'auto-hide: only the last row reveals it, after ~190 ms; leaving hides it',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 300);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      const items = [
        Item.app('Terminal', command: 'term'),
        Item.app('Note', command: 'note'),
      ];
      await tester.pumpWidget(
        const Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(items: items, autohide: true),
        ),
      );
      double glassTop() => tester.getRect(find.byType(ProfiledGlass)).top;
      final shownTop = 300 - bottomGap - backgroundHeight;
      // Hidden below the screen.
      expect(glassTop(), greaterThanOrEqualTo(300));

      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(640, 100));
      // A row above the last one does nothing.
      await mouse.moveTo(const Offset(640, 297));
      await tester.pump(const Duration(milliseconds: 500));
      expect(glassTop(), greaterThanOrEqualTo(300));
      // The last row: nothing for ~190 ms, then it slides up in ~190 ms.
      await mouse.moveTo(const Offset(100, 299.5));
      await tester.pump(const Duration(milliseconds: 150));
      expect(glassTop(), greaterThanOrEqualTo(300));
      await tester.pump(const Duration(milliseconds: 60));
      await tester.pump(const Duration(milliseconds: 200));
      expect(glassTop(), closeTo(shownTop, 0.5));
      // Leaving upward: down again in ~175 ms, with no delay.
      await mouse.moveTo(const Offset(640, 50));
      await tester.pump(const Duration(milliseconds: 20));
      await tester.pump(const Duration(milliseconds: 180));
      expect(glassTop(), greaterThanOrEqualTo(300));
    },
  );

  testWidgets(
    'auto-hide: a pointer that arrives on the last row and stays still reveals it (#242)',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 300);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      const items = [Item.app('Terminal', command: 'term')];
      await tester.pumpWidget(
        const Directionality(
          textDirection: TextDirection.ltr,
          child: Dock(items: items, autohide: true),
        ),
      );
      double glassTop() => tester.getRect(find.byType(ProfiledGlass)).top;
      expect(glassTop(), greaterThanOrEqualTo(300));
      // The compositor's enter on the 1 px strip: the pointer is added there, with no motion
      // after it (a warp, a mouse pushed against the edge).
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      addTearDown(mouse.removePointer);
      await mouse.addPointer(location: const Offset(900, 299.5));
      await tester.pump(const Duration(milliseconds: 150));
      expect(glassTop(), greaterThanOrEqualTo(300));
      await tester.pump(const Duration(milliseconds: 60));
      await tester.pump(const Duration(milliseconds: 200));
      expect(glassTop(), closeTo(300 - bottomGap - backgroundHeight, 0.5));
    },
  );

  test('dock.launchAnimation: bounce by default, none turns it off', () async {
    final backend = await useSettings();
    final settings = SilicaSettings.instance;
    expect(DockOptions.of(settings).launchAnimation, isTrue);
    backend.emit('dock.launchAnimation', 'none');
    expect(DockOptions.of(settings).launchAnimation, isFalse);
    backend.emit('dock.launchAnimation', 'bounce');
    expect(DockOptions.of(settings).launchAnimation, isTrue);
    backend.emit('dock.launchAnimation', 7);
    expect(DockOptions.of(settings).launchAnimation, isTrue);
  });
}
