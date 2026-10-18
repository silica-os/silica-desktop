import 'dart:io';

import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/apps.dart';
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/dock/model.dart';
import 'package:silica_shell/dock/options.dart';
import 'package:silica_shell/system/glass_profile.dart';

void main() {
  test('dock.json options: defaults, then the values Settings writes', () {
    final dir = Directory.systemTemp.createTempSync('dock-options');
    addTearDown(() => dir.deleteSync(recursive: true));
    final env = {'XDG_CONFIG_HOME': dir.path};
    expect(DockOptions.read(env: env), const DockOptions());
    saveDockOption('autohide', true, env: env);
    saveDockOption('position', 'left', env: env);
    saveDockOption('minimizeEffect', 'scale', env: env);
    saveDockOption('indicators', false, env: env);
    final o = DockOptions.read(env: env);
    expect(o.autohide, isTrue);
    expect(o.position, DockPosition.left);
    expect(o.minimizeEffect, MinimizeEffect.scale);
    expect(o.indicators, isFalse);
    expect(o.magnify, isFalse);
    // A broken value falls back to the default.
    saveDockOption('position', 'top', env: env);
    expect(DockOptions.read(env: env).position, DockPosition.bottom);
  });

  test(
    'the watcher reloads after an atomic write (temp file + rename)',
    () async {
      final dir = Directory.systemTemp.createTempSync('dock-rename');
      addTearDown(() => dir.deleteSync(recursive: true));
      final env = {'XDG_CONFIG_HOME': dir.path};
      final watcher = DockOptionsWatcher(env: env);
      addTearDown(watcher.dispose);
      expect(watcher.value.value.autohide, isFalse);
      File('${dir.path}/silica/dock.json.tmp')
        ..writeAsStringSync('{"autohide": true}')
        ..renameSync('${dir.path}/silica/dock.json');
      await Future<void>.delayed(const Duration(milliseconds: 400));
      expect(watcher.value.value.autohide, isTrue);
    },
  );

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

  test('dock.json launchAnimation: bounce by default, none turns it off', () {
    final dir = Directory.systemTemp.createTempSync('dock-launch');
    addTearDown(() => dir.deleteSync(recursive: true));
    final env = {'XDG_CONFIG_HOME': dir.path};
    expect(DockOptions.read(env: env).launchAnimation, isTrue);
    saveDockOption('launchAnimation', 'none', env: env);
    expect(DockOptions.read(env: env).launchAnimation, isFalse);
    saveDockOption('launchAnimation', 'bounce', env: env);
    expect(DockOptions.read(env: env).launchAnimation, isTrue);
    saveDockOption('launchAnimation', 7, env: env);
    expect(DockOptions.read(env: env).launchAnimation, isTrue);
  });
}
