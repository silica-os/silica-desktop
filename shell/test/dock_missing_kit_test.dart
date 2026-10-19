// The Dock's tile of a pinned app that is no longer installed against the Amalfi kit
// (silica-design cbf75649, `Dock/Tile/Missing/Rest/<Light|Dark>`): in the 60 pt icon canvas a
// 48 squircle at (6, 6), with the remembered name for the screen reader.

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/dock.dart';
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_ui/silica_ui.dart';

void main() {
  for (final brightness in Brightness.values) {
    testWidgets('the missing tile ($brightness)', (tester) async {
      await tester.pumpWidget(
        Localizations(
          locale: const Locale('en'),
          delegates: AppLocalizations.localizationsDelegates,
          child: Directionality(
            textDirection: TextDirection.ltr,
            child: AmalfiTheme(
              brightness: brightness,
              child: const Align(
                alignment: Alignment.topLeft,
                child: MissingAppIcon(side: 60, name: 'Lemon'),
              ),
            ),
          ),
        ),
      );
      final tile = find.byKey(const ValueKey('dock missing app'));
      expect(tester.getRect(tile), const Rect.fromLTWH(6, 6, 48, 48));
      final shape =
          (tester
                  .widget<DecoratedBox>(
                    find.descendant(
                      of: tile,
                      matching: find.byType(DecoratedBox),
                    ),
                  )
                  .decoration
              as ShapeDecoration);
      expect(
        shape.color,
        brightness == Brightness.dark
            ? const Color(0xFF3A3C42)
            : const Color(0xFFE6E8EC),
      );
      expect(find.bySemanticsLabel('Lemon, not installed'), findsOneWidget);
      // At another canvas it scales (k = side / 60).
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: AmalfiTheme(
            brightness: brightness,
            child: const Align(
              alignment: Alignment.topLeft,
              child: MissingAppIcon(side: 120),
            ),
          ),
        ),
      );
      expect(tester.getRect(tile), const Rect.fromLTWH(12, 12, 96, 96));
    });
  }
}
