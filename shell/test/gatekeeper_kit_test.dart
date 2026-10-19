// The Gatekeeper alerts against the Amalfi kit (silica-design cbf75649,
// `kit/symbols/Alerts/Gatekeeper/<Variant>/Rest/Light`, copied to
// `test/fixtures/kit/gatekeeper/`): the alert's width, the text block's place and width, the
// title's box and every button's frame equal the master's, for the kit's example app "Lemon"
// by "Marta Rossi". Text wraps where the engine's measure says (Confirm's long title takes 4
// lines here, 3 in the kit), so what is under the text block is compared from its bottom.

import 'dart:convert';
import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_shell/trust/gatekeeper_dialog.dart';
import 'package:silica_ui/silica_ui.dart';

Map<String, dynamic> kit(String name) =>
    jsonDecode(
          File('test/fixtures/kit/gatekeeper/$name.json').readAsStringSync(),
        )
        as Map<String, dynamic>;

Rect frameOf(Map<String, dynamic> node) {
  final f = node['frame'] as Map<String, dynamic>;
  return Rect.fromLTWH(
    (f['x'] as num).toDouble(),
    (f['y'] as num).toDouble(),
    (f['w'] as num).toDouble(),
    (f['h'] as num).toDouble(),
  );
}

Map<String, dynamic> child(Map<String, dynamic> node, String name) =>
    (node['children'] as List).cast<Map<String, dynamic>>().singleWhere(
      (c) => c['name'] == name,
    );

GatekeeperRequest lemon(String verdict, {String origin = ''}) =>
    GatekeeperRequest(
      id: 1,
      verdict: verdict,
      info: {
        'id': 'org.example.Lemon',
        'name': 'Lemon',
        'path': '/home/u/Applications/Lemon.app',
        'developer': 'Marta Rossi',
        'developer_status': 'verified',
        'origin': origin,
        'reason': verdict == 'block' ? 'self-signed' : '',
      },
    );

Future<Rect> pumpAlert(WidgetTester tester, GatekeeperRequest r) async {
  tester.view.physicalSize = const Size(800, 600);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    Localizations(
      locale: const Locale('en'),
      delegates: AppLocalizations.localizationsDelegates,
      child: MediaQuery(
        data: const MediaQueryData(size: Size(800, 600)),
        child: AmalfiTheme(
          brightness: Brightness.light,
          child: Directionality(
            textDirection: TextDirection.ltr,
            child: Align(
              alignment: Alignment.topLeft,
              child: GatekeeperAlert(request: r, onChoice: (_) {}),
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pump();
  return tester.getRect(find.byKey(const ValueKey('gatekeeper alert')));
}

void main() {
  const cases = {
    'confirm': ('confirm', {'Open': 'open', 'Cancel': 'cancel'}),
    'blocked': ('block', {'Move to Trash': 'trash', 'Done': 'cancel'}),
    'malware': ('malware', {'Done': 'cancel', 'Move to Trash': 'trash'}),
    'disk-image': (
      'volume',
      {'Move to Applications': 'move', 'Not Now': 'cancel'},
    ),
  };
  for (final MapEntry(key: name, value: (verdict, buttons)) in cases.entries) {
    testWidgets('$name equals its master', (tester) async {
      final master = kit(name);
      final alert = await pumpAlert(
        tester,
        lemon(verdict, origin: 'https://lemon.example.org/Lemon.sxv'),
      );
      Rect local(Finder f) => tester.getRect(f).shift(-alert.topLeft);
      final text = frameOf(child(master, 'Text'));
      final ours = local(find.byKey(const ValueKey('gatekeeper text')));
      // Lines the engine wraps differently from the kit move what is below.
      final shift = ours.height - text.height;
      expect(alert.width, frameOf(master).width);
      expect(alert.height, frameOf(master).height + shift);
      // The text block (240 wide at 20, 96) and the title's box in it.
      expect(ours.topLeft, text.topLeft);
      expect(ours.width, text.width);
      final title = frameOf(child(child(master, 'Text'), 'Title'));
      final titleBox = local(find.byKey(const ValueKey('gatekeeper title')));
      expect(titleBox.topLeft, title.shift(text.topLeft).topLeft);
      expect(titleBox.width, title.width);
      final group = child(master, 'Buttons');
      for (final MapEntry(key: label, value: choice) in buttons.entries) {
        expect(
          local(find.byKey(ValueKey('gatekeeper $choice'))),
          frameOf(
            child(group, label),
          ).shift(frameOf(group).topLeft).translate(0, shift),
          reason: label,
        );
      }
      final badge = find.byKey(const ValueKey('gatekeeper badge'));
      expect(
        badge,
        verdict == 'block' || verdict == 'malware'
            ? findsOneWidget
            : findsNothing,
      );
      if (verdict == 'block') {
        expect(local(badge).left, frameOf(child(master, 'Badge disc')).left);
        expect(local(badge).width, frameOf(child(master, 'Badge disc')).width);
      }
    });
  }
}
