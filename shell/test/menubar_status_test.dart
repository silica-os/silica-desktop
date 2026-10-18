import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/menubar/menubar.dart';
import 'package:silica_shell/menubar/menubar_config.dart';
import 'package:silica_shell/menubar/status_icons.dart';
import 'package:silica_shell/system/system_status.dart';
import 'package:silica_ui/silica_ui.dart';

import 'controlcenter_test.dart' show FakeSystem;
import 'support/settings.dart';

void main() {
  test('Copy to Menu Bar: setMenubarModule sets the matching keys', () async {
    final backend = await useSettings({'menubar.batteryPercent': true});
    final settings = SilicaSettings.instance;
    expect(menubarShows(MenubarConfig.of(settings), 'bluetooth'), isFalse);
    expect(await setMenubarModule('bluetooth', true), isTrue);
    expect(await setMenubarModule('sound', true), isTrue);
    expect(await setMenubarModule('dnd', true), isTrue);
    final c = MenubarConfig.of(settings);
    expect(c.batteryPercent, isTrue);
    expect([
      for (final m in menubarModules) menubarShows(c, m),
    ], everyElement(isTrue));
    expect(backend.values['menubar.sound'], 'always');
    expect(backend.values['menubar.focus'], true);
    expect(await setMenubarModule('sound', false), isTrue);
    expect(MenubarConfig.of(settings).sound, SoundItem.never);
    expect(await setMenubarModule('display', true), isFalse);
  });

  test('menubar settings: defaults as on the reference, values read', () {
    expect(
      MenubarConfig.of(SilicaSettings.withValues({})),
      const MenubarConfig(),
    );
    expect(const MenubarConfig().bluetooth, isFalse);
    expect(const MenubarConfig().sound, SoundItem.active);
    expect(
      MenubarConfig.of(
        SilicaSettings.withValues({
          'menubar.sound': 'always',
          'menubar.bluetooth': true,
          'menubar.battery': false,
          'menubar.batteryPercent': true,
        }),
      ),
      const MenubarConfig(
        sound: SoundItem.always,
        bluetooth: true,
        battery: false,
        batteryPercent: true,
      ),
    );
  });

  testWidgets('the watcher follows the settings', (tester) async {
    final backend = await useSettings();
    final watcher = MenubarConfigWatcher();
    addTearDown(watcher.dispose);
    expect(watcher.value.background, isFalse);
    backend.emit('menubar.background', true);
    backend.emit('menubar.clock.seconds', true);
    expect(watcher.value.background, isTrue);
    expect(watcher.value.clock.seconds, isTrue);
  });

  testWidgets('Bluetooth and Sound items follow Settings › Menu Bar', (
    tester,
  ) async {
    final system = FakeSystem(
      const SystemSnapshot(
        wifiAvailable: true,
        bluetoothAvailable: true,
        bluetoothPowered: true,
        audioAvailable: true,
        volume: 0.5,
        batteryPresent: true,
        batteryPercent: 64,
      ),
    );
    final config = ValueNotifier(const MenubarConfig());
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MenuBar(
          system: system,
          config: config,
          now: DateTime(2026, 10, 13, 9),
        ),
      ),
    );
    bool shown(StatusKind kind) => tester
        .widgetList<StatusIcon>(find.byType(StatusIcon))
        .any((i) => i.kind == kind);
    // Defaults: no Bluetooth, Sound only while active (nothing plays here).
    expect(shown(StatusKind.bluetooth), isFalse);
    expect(shown(StatusKind.sound), isFalse);
    expect(shown(StatusKind.battery), isTrue);
    expect(find.text('64%'), findsNothing);

    config.value = const MenubarConfig(
      sound: SoundItem.always,
      bluetooth: true,
      batteryPercent: true,
    );
    await tester.pump();
    expect(shown(StatusKind.bluetooth), isTrue);
    expect(shown(StatusKind.sound), isTrue);
    expect(find.text('64%'), findsOneWidget);
    final sound = tester
        .widgetList<StatusIcon>(find.byType(StatusIcon))
        .firstWhere((i) => i.kind == StatusKind.sound);
    expect(sound.level, 0.5);

    config.value = const MenubarConfig(battery: false);
    await tester.pump();
    expect(shown(StatusKind.battery), isFalse);
  });
}
