// The shell's texts follow the system language: English by default, Italian when the
// system asks for it (one check per component).

import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/controlcenter/controlcenter.dart';
import 'package:silica_shell/dock/trash_alert.dart';
import 'package:silica_shell/faro/faro.dart';
import 'package:silica_shell/forcequit/force_quit.dart';
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_shell/menubar/menubar.dart';
import 'package:silica_shell/notifications/notifications.dart';
import 'package:silica_shell/screenshot/screenshot.dart';

import 'package:silica_ui/silica_ui.dart';
import 'controlcenter_test.dart' show FakeSystem, laptop;
import 'notifications_test.dart' show FakeSource;

/// `child` in Italian, as when the system language is `it_IT`.
Widget italian(Widget child) => ShellLocalizations(
  locales: const [Locale('it', 'IT')],
  child: Directionality(textDirection: TextDirection.ltr, child: child),
);

Future<void> command(WidgetTester tester, String id) =>
    tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      'silica/shell',
      const JSONMethodCodec().encodeMethodCall(MethodCall('command', id)),
      (_) {},
    );

void main() {
  test('the system languages resolve to Italian or fall back to English', () {
    expect(resolveLocale(const [Locale('it', 'IT')]), const Locale('it'));
    expect(
      resolveLocale(const [Locale('de'), Locale('it')]),
      const Locale('it'),
    );
    expect(resolveLocale(const [Locale('fr')]), const Locale('en'));
    expect(resolveLocale(const []), const Locale('en'));
  });

  testWidgets('menu bar in Italian: menus and clock', (tester) async {
    await tester.pumpWidget(
      italian(MenuBar(now: DateTime(2026, 10, 12, 22, 34))),
    );
    await tester.pump();
    // Edit is hidden while it has no items (no dead menus); Help is there.
    expect(find.text('Aiuto'), findsOneWidget);
    expect(find.text('Finestra'), findsOneWidget);
    expect(find.text('Lun 12 ott  22:34'), findsOneWidget);
  });

  testWidgets('Faro in Italian', (tester) async {
    await tester.pumpWidget(italian(const Faro(entries: {}, startOpen: true)));
    await tester.pumpAndSettle();
    expect(find.text('Cerca con Faro'), findsOneWidget);
  });

  testWidgets('Control Center in Italian', (tester) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(italian(ControlCenter(system: FakeSystem(laptop))));
    await command(tester, 'controlcenter.toggle');
    await tester.pumpAndSettle();
    expect(find.text('Non ancora disponibile'), findsOneWidget);
    expect(find.text('Modifica controlli'), findsOneWidget);
    expect(find.text('Focus'), findsOneWidget);
    expect(find.text('Suono'), findsOneWidget);
  });

  testWidgets('Notification Center in Italian', (tester) async {
    tester.view.physicalSize = const Size(1280, 686);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(italian(NotificationsLayer(source: FakeSource())));
    await command(tester, 'notifications.toggle');
    await tester.pumpAndSettle();
    // Empty: the title and the text, nothing to clear.
    expect(find.text('Notifiche'), findsOneWidget);
    expect(find.text('Nessuna notifica recente'), findsOneWidget);
    expect(find.text('Cancella tutto'), findsNothing);
  });

  testWidgets('Empty Trash alert in Italian', (tester) async {
    await tester.pumpWidget(
      italian(TrashAlert(onEmpty: () {}, onCancel: () {})),
    );
    expect(find.text('Svuota il Cestino'), findsOneWidget);
    expect(find.text('Annulla'), findsOneWidget);
  });

  testWidgets('force quit in Italian', (tester) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(italian(const ForceQuitLayer(entries: {})));
    await command(tester, 'app.force_quit_panel');
    await tester.pump();
    expect(find.text('Termina le app'), findsOneWidget);
    expect(find.text('Termina'), findsOneWidget);
  });

  testWidgets('screenshot bar in Italian', (tester) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final dir = Directory.systemTemp.createTempSync('shot-it');
    addTearDown(() => dir.deleteSync(recursive: true));
    await tester.pumpWidget(
      italian(ScreenshotLayer(settings: SilicaSettings.withValues({}))),
    );
    await command(tester, 'screenshot.toolbar');
    await tester.pump();
    expect(find.text('Acquisisci'), findsOneWidget);
    expect(find.text('Opzioni'), findsOneWidget);
    expect(
      screenshotName(
        DateTime(2026, 10, 14, 4, 12, 33),
        (_) => false,
        texts: lookupAppLocalizations(const Locale('it')),
      ),
      'Istantanea schermo 2026-10-14 alle 04.12.33.png',
    );
  });
}
