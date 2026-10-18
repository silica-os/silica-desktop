// Acceptance: every key a Settings control writes makes the shell component that applies it
// react live, with no restart and no file in between (docs/spec/shell-config.md). The keys
// that only Ossido applies (lockscreen.passwordDelay, screensaver.*) are not here;
// lockscreen.showUser is in lock_test.dart, the Control Center and Screenshot keys (written by
// the shell itself) in controlcenter_test.dart and screenshot_test.dart.

import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/notifications/settings.dart';
import 'package:silica_shell/wallpaper/wallpaper.dart';
import 'package:silica_shell/dock/options.dart';
import 'package:silica_shell/menubar/menubar.dart';
import 'package:silica_shell/menubar/menubar_config.dart';
import 'package:silica_shell/menubar/status_icons.dart';
import 'package:silica_shell/system/system_status.dart';

import 'controlcenter_test.dart' show FakeSystem;
import 'support/settings.dart';

void main() {
  group('Dock', () {
    testWidgets('every dock.* key changes the options', (tester) async {
      final backend = await useSettings();
      final watcher = DockOptionsWatcher();
      addTearDown(watcher.dispose);
      DockOptions o() => watcher.value.value;
      final cases = <String, (Object, bool Function())>{
        'dock.magnify': (true, () => o().magnify),
        'dock.recents': (true, () => o().recents),
        'dock.autohide': (true, () => o().autohide),
        'dock.position': ('right', () => o().position == DockPosition.right),
        'dock.minimizeEffect': (
          'scale',
          () => o().minimizeEffect.name == 'scale',
        ),
        'dock.indicators': (false, () => !o().indicators),
        'dock.tileSize': (100, () => o().tileSize == 100),
        'dock.magnifiedSize': (110, () => o().magnifiedSize == 110),
        'dock.launchAnimation': ('none', () => !o().launchAnimation),
        'dock.minimizeToApp': (true, () => o().minimizeToApp),
      };
      expect(
        cases.keys.map((k) => k.substring(5)),
        unorderedEquals(dockOptionKeys),
      );
      for (final MapEntry(:key, :value) in cases.entries) {
        expect(value.$2(), isFalse, reason: '$key before');
        backend.emit(key, value.$1);
        expect(value.$2(), isTrue, reason: '$key after');
      }
    });
  });

  group('Menu bar', () {
    testWidgets('every menubar.* key changes the configuration', (
      tester,
    ) async {
      final backend = await useSettings();
      final watcher = MenubarConfigWatcher();
      addTearDown(watcher.dispose);
      MenubarConfig c() => watcher.value;
      final cases = <String, (Object, bool Function())>{
        'menubar.sound': ('always', () => c().sound == SoundItem.always),
        'menubar.bluetooth': (true, () => c().bluetooth),
        'menubar.battery': (false, () => !c().battery),
        'menubar.batteryPercent': (true, () => c().batteryPercent),
        'menubar.focus': (true, () => c().focus),
        'menubar.clock.hour24': (false, () => !c().clock.hour24),
        'menubar.clock.seconds': (true, () => c().clock.seconds),
        'menubar.clock.weekday': (false, () => !c().clock.weekday),
        'menubar.clock.date': (
          'never',
          () => c().clock.date == ClockDate.never,
        ),
        'menubar.recentItems': (20, () => c().recentItems == 20),
        'menubar.autohide': (
          'always',
          () => c().autohide == MenubarAutohide.always,
        ),
        'menubar.background': (true, () => c().background),
      };
      expect(cases.keys, unorderedEquals(menubarKeys));
      for (final MapEntry(:key, :value) in cases.entries) {
        expect(value.$2(), isFalse, reason: '$key before');
        backend.emit(key, value.$1);
        expect(value.$2(), isTrue, reason: '$key after');
      }
    });

    testWidgets('the bar on screen follows the service: items and clock', (
      tester,
    ) async {
      final backend = await useSettings();
      final system = FakeSystem(
        const SystemSnapshot(
          wifiAvailable: true,
          bluetoothAvailable: true,
          bluetoothPowered: true,
          batteryPresent: true,
          batteryPercent: 64,
        ),
      );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(system: system, now: DateTime(2026, 10, 13, 9, 5, 7)),
        ),
      );
      bool shown(StatusKind kind) => tester
          .widgetList<StatusIcon>(find.byType(StatusIcon))
          .any((i) => i.kind == kind);
      expect(shown(StatusKind.bluetooth), isFalse);
      expect(find.text('64%'), findsNothing);
      backend.emit('menubar.bluetooth', true);
      backend.emit('menubar.batteryPercent', true);
      await tester.pump();
      expect(shown(StatusKind.bluetooth), isTrue);
      expect(find.text('64%'), findsOneWidget);
      backend.emit('menubar.battery', false);
      await tester.pump();
      expect(shown(StatusKind.battery), isFalse);
    });
  });

  group('Wallpaper', () {
    testWidgets('image, colour, Amalfi id and fit change the layer', (
      tester,
    ) async {
      final backend = await useSettings();
      await tester.pumpWidget(
        const Directionality(
          textDirection: TextDirection.ltr,
          child: WallpaperLayer(
            env: {'HOME': '/nonexistent', 'XDG_DATA_DIRS': '/nonexistent'},
          ),
        ),
      );
      Color? color() => tester
          .widgetList<ColoredBox>(find.byType(ColoredBox))
          .map((b) => b.color)
          .firstOrNull;
      backend.emit('wallpaper.color', '#102030');
      await tester.pump();
      expect(color(), const Color(0xFF102030));
      // An image that exists wins over the colour; the fit applies to it.
      final file = File(
        '${Directory.systemTemp.createTempSync('wp').path}/a.png',
      )..writeAsBytesSync([0]);
      addTearDown(() => file.parent.deleteSync(recursive: true));
      backend.emit('wallpaper.image', file.path);
      backend.emit('wallpaper.fit', 'tile');
      await tester.pump();
      final image = tester.widget<Image>(find.byType(Image));
      expect(image.repeat, ImageRepeat.repeat);
      expect((image.image as FileImage).file.path, file.path);
      backend.emit('wallpaper.fit', 'stretch');
      await tester.pump();
      expect(tester.widget<Image>(find.byType(Image)).fit, BoxFit.fill);
      // Cleared again (what Settings does choosing a colour): the colour is back.
      backend.emit('wallpaper.image', '');
      await tester.pump();
      expect(find.byType(Image), findsNothing);
      expect(color(), const Color(0xFF102030));
      await tester.pumpWidget(const SizedBox());
    });
  });

  group('Notifications', () {
    test('every notifications.* key changes what the next one does', () async {
      final backend = await useSettings();
      NotificationSettings now() => NotificationSettings.read();
      backend.emit('notifications.previews', 'never');
      expect(now().previews, PreviewMode.never);
      backend.emit('notifications.grouping', 'off');
      expect(now().grouping, GroupingMode.off);
      const app = 'notifications.apps.org.x.Mail';
      backend.emit('$app.style', 'none');
      expect(now().styles, {'org.x.Mail': NotificationStyle.none});
      backend.emit('$app.previews', 'always');
      expect(now().appPreviews, {'org.x.Mail': PreviewMode.always});
      backend.emit('$app.grouping', 'app');
      expect(now().groupings, {'org.x.Mail': GroupingMode.app});
      backend.emit('$app.showInCenter', false);
      expect(now().hiddenInCenter, {'org.x.Mail'});
      backend.emit('$app.sound', false);
      expect(now().mutedApps, {'org.x.Mail'});
      // Reset by Settings: the daemon sends the default value.
      backend.emit('$app.showInCenter', true);
      backend.emit('$app.sound', true);
      backend.emit('$app.previews', 'default');
      expect(now().hiddenInCenter, isEmpty);
      expect(now().mutedApps, isEmpty);
      expect(now().appPreviews, isEmpty);
    });
  });
}
