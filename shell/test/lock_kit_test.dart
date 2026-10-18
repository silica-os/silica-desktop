// The lock screen against the Amalfi kit (silica-design 1f8b28df, `kit/symbols/Lock Screen/*` and
// `Status Bar/Bar/Lock Screen/Default`): the frames, colours and text styles of
// `test/fixtures/kit/lock/*.json` are the binding numbers. The screen is pumped at the kit's
// 1920 × 1200, in both appearances, and with Increase contrast and Reduce transparency.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/l10n/app_localizations.dart';
import 'package:silica_shell/lock/lock_parts.dart';
import 'package:silica_shell/lock/lock_screen.dart';
import 'package:silica_shell/menubar/bar_menu_host.dart';
import 'package:silica_shell/notifications/model.dart';
import 'package:silica_shell/status/status_items.dart';
import 'package:silica_shell/system/shell_channel.dart';
import 'package:silica_shell/system/system_status.dart';
import 'package:silica_ui/silica_ui.dart';

import 'lock_test.dart'
    show FakeLock, FakeNotifications, FakeSystem, pump, fixedNow;

const Size screen = Size(1920, 1200);

Map<String, dynamic> kit(String name) =>
    jsonDecode(File('test/fixtures/kit/lock/$name.json').readAsStringSync())
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

Map<String, dynamic> layer(Map<String, dynamic> node, String path) {
  var n = node;
  for (final name in path.split('/')) {
    n = (n['children'] as List).cast<Map<String, dynamic>>().singleWhere(
      (c) => c['name'] == name,
    );
  }
  return n;
}

/// A `#rrggbbaa` colour of the export.
Color hex(String s) {
  final v = int.parse(s.substring(1), radix: 16);
  return Color(((v & 0xFF) << 24) | (v >> 8));
}

Color fillOf(Map<String, dynamic> node) =>
    hex(((node['style'] as Map)['fills'] as List).first['color'] as String);

Color borderOf(Map<String, dynamic> node, [int i = 0]) =>
    hex(((node['style'] as Map)['borders'] as List)[i]['color'] as String);

Color innerShadowOf(Map<String, dynamic> node) => hex(
  ((node['style'] as Map)['innerShadows'] as List).first['color'] as String,
);

Color fontColorOf(Map<String, dynamic> node) =>
    hex((node['font'] as Map)['color'] as String);

double fontSizeOf(Map<String, dynamic> node) =>
    ((node['font'] as Map)['size'] as num).toDouble();

Rect rectOf(WidgetTester tester, String key) =>
    tester.getRect(find.byKey(ValueKey(key)));

Rect relative(Rect r, Rect to) => r.shift(-to.topLeft);

LockGlassFacePainter faceUnder(WidgetTester tester, Finder of) => tester
    .widgetList<CustomPaint>(
      find.descendant(of: of, matching: find.byType(CustomPaint)),
    )
    .map((p) => p.painter)
    .whereType<LockGlassFacePainter>()
    .first;

void expectInk(TextStyle? style, Map<String, dynamic> node) {
  expect(style?.color, fontColorOf(node), reason: node['name'] as String);
  expect(style?.fontSize, fontSizeOf(node), reason: node['name'] as String);
}

const LockLook dark = LockLook();
const LockLook light = LockLook(dark: false);

Future<void> pumpKit(
  WidgetTester tester, {
  LockLook look = dark,
  SystemSource? system,
  Map<String, Object?> values = const {},
  FakeNotifications? notifications,
}) async {
  await pump(
    tester,
    FakeLock(),
    size: screen,
    look: look,
    system: system,
    values: values,
    notifications: notifications,
  );
  await tester.pumpAndSettle();
}

Future<void> typeAndFail(WidgetTester tester, String password) async {
  await tester.enterText(find.byKey(const ValueKey('lock password')), password);
  await tester.testTextInput.receiveAction(TextInputAction.done);
  await tester.pumpAndSettle();
}

void main() {
  for (final (name, look) in const [('Dark', dark), ('Light', light)]) {
    final a = name.toLowerCase();
    group('kit $name', () {
      testWidgets('Clock/Standard: the date and the glass time', (
        tester,
      ) async {
        final master = kit('clock-standard-$a');
        await pumpKit(tester, look: look);
        final block = rectOf(tester, 'lock clock block');
        expect(block, LockMetrics.clock(screen));
        expect(block.size, frameOf(master).size);
        final date = layer(master, 'Date');
        final time = layer(master, 'Time');
        expect(relative(rectOf(tester, 'lock date box'), block), frameOf(date));
        expect(relative(rectOf(tester, 'lock time box'), block), frameOf(time));
        // The date: Lock Date 32/40 Semibold, white 84 %, the on-photo shadow in light.
        final dateText = tester.widget<Text>(
          find.descendant(
            of: find.byKey(const ValueKey('lock date')),
            matching: find.byType(Text),
          ),
        );
        expectInk(dateText.style, date);
        expect(dateText.style?.fontWeight, FontWeight.w600);
        expect(dateText.style?.height, 40 / 32);
        expect(
          dateText.style?.shadows,
          look.dark ? isNull : AmalfiShadows.onPhoto,
        );
        // The time: Lock Clock 200 Semibold, tracking −6, tabular, in Glass/Clock.
        expect(lockClockStyle.fontSize, fontSizeOf(time));
        expect(lockClockStyle.fontWeight, FontWeight.w600);
        expect(lockClockStyle.letterSpacing, (time['font'] as Map)['kerning']);
        expect(lockClockStyle.fontFeatures, const [FontFeature('tnum')]);
        final glass = AmalfiGlyphMaterials.clock;
        expect(glass.fill, fontColorOf(time));
        expect(glass.lineInside, borderOf(time, 0));
        expect(glass.rimOutside(look.brightness), borderOf(time, 1));
        final blur = ((time['style'] as Map)['blurs'] as List).first as Map;
        expect(glass.backgroundBlur, blur['radius']);
        expect(glass.saturation, blur['saturation']);
        expect(glass.brightness, blur['brightness']);
        // The glass digits are drawn: the backdrop under the glyph mask.
        expect(
          find.descendant(
            of: find.byKey(const ValueKey('lock time')),
            matching: find.byType(BackdropFilter),
          ),
          findsNWidgets(5),
        );
        // The digits: tops at about y 169, baseline about 314 (lock-screen.md), 145 tall.
        final top = block.top + LockMetrics.timeTop + digitInset();
        expect(top, closeTo(169, 4));
        expect(top + LockMetrics.digitHeight, closeTo(314, 4));
      });

      testWidgets('User/Default: the Accent avatar and the name', (
        tester,
      ) async {
        final master = kit('user-default-$a');
        await pumpKit(tester, look: look);
        final block = rectOf(tester, 'lock user block');
        expect(block.size, frameOf(master).size);
        final avatar = layer(master, 'Avatar');
        final name = layer(master, 'Name');
        expect(relative(rectOf(tester, 'lock avatar'), block), frameOf(avatar));
        expect(relative(rectOf(tester, 'lock user'), block), frameOf(name));
        final disc = faceUnder(
          tester,
          find.byKey(const ValueKey('lock avatar')),
        );
        expect(disc.fill, fillOf(avatar));
        expect(disc.rim, borderOf(avatar));
        expect(disc.rimLight, innerShadowOf(avatar));
        final glyph = tester.widget<AmalfiSymbol>(
          find.descendant(
            of: find.byKey(const ValueKey('lock avatar')),
            matching: find.byType(AmalfiSymbol),
          ),
        );
        final glyphLayer = layer(master, 'Avatar glyph');
        expect(glyph.name, glyphLayer['text']);
        expect(glyph.size, fontSizeOf(glyphLayer));
        expect(glyph.color, fontColorOf(glyphLayer));
        final text = tester.widget<Text>(find.text('Anna Rossi'));
        expectInk(text.style, name);
        expect(text.style?.fontWeight, FontWeight.w600);
        expect(text.style?.height, 22 / 16);
        expect(text.style?.shadows, look.dark ? isNull : AmalfiShadows.onPhoto);
      });

      testWidgets('Password Field: Placeholder, Typing, Verifying, Wrong', (
        tester,
      ) async {
        final placeholder = kit('password-field-placeholder-$a');
        final typing = kit('password-field-typing-$a');
        final verifying = kit('password-field-verifying-$a');
        final wrong = kit('password-field-wrong-$a');
        final source = FakeLock();
        await pump(tester, source, size: screen, look: look);
        await tester.pumpAndSettle();
        final field = rectOf(tester, 'lock password');
        expect(field, LockMetrics.field(screen));
        expect(field.size, frameOf(placeholder).size);
        // The glass: the material's fill, edge and rim light, a capsule of radius 16.
        final background = layer(placeholder, 'Background');
        final face = faceUnder(
          tester,
          find.byKey(const ValueKey('lock password')),
        );
        expect(face.fill, fillOf(background));
        expect(face.rim, borderOf(background));
        expect(face.rimLight, innerShadowOf(background));
        expect(
          face.shape,
          const AmalfiSmoothBorder(radius: LockFieldMetrics.radius),
        );
        // Placeholder: the caret at 16, 8 and `Enter Password` in Body, secondary, at 22, 7.
        final caret = layer(placeholder, 'Caret');
        expect(LockFieldMetrics.caret(0), frameOf(caret));
        final text = layer(placeholder, 'Placeholder');
        expect(
          relative(rectOf(tester, 'lock placeholder'), field).topLeft,
          frameOf(text).topLeft,
        );
        expect(rectOf(tester, 'lock placeholder').height, frameOf(text).height);
        final label = tester.widget<Text>(
          find.byKey(const ValueKey('lock placeholder')),
        );
        expect(label.data, text['text']);
        expectInk(label.style, text);
        LockFieldPainter ink() =>
            tester
                    .widget<CustomPaint>(
                      find.byKey(const ValueKey('lock field ink')),
                    )
                    .painter!
                as LockFieldPainter;
        expect(ink().caret, fillOf(caret));

        // Typing: six dots, the caret after them, the Accent submit.
        source.gate = null;
        await tester.enterText(
          find.byKey(const ValueKey('lock password')),
          'abcdef',
        );
        await tester.pumpAndSettle();
        final dots = layer(typing, 'Dots');
        for (var i = 0; i < 6; i++) {
          final dot = frameOf(layer(typing, 'Dots/Dot ${i + 1}'));
          expect(
            Rect.fromLTWH(
              LockFieldMetrics.dotLeft(i),
              LockFieldMetrics.dotY,
              LockFieldMetrics.dot,
              LockFieldMetrics.dot,
            ),
            dot.shift(frameOf(dots).topLeft),
          );
        }
        expect(ink().dot, fillOf(layer(typing, 'Dots/Dot 1')));
        expect(LockFieldMetrics.caret(6), frameOf(layer(typing, 'Caret')));
        final submit = layer(typing, 'Submit');
        expect(
          relative(rectOf(tester, 'lock submit disc'), field),
          frameOf(submit),
        );
        final disc =
            tester
                    .widget<DecoratedBox>(
                      find.byKey(const ValueKey('lock submit disc')),
                    )
                    .decoration
                as BoxDecoration;
        expect(disc.color, fillOf(submit));
        final arrow = layer(typing, 'Submit glyph');
        final arrowRect = relative(
          tester.getRect(
            find.descendant(
              of: find.byKey(const ValueKey('lock submit disc')),
              matching: find.byType(AmalfiSymbol),
            ),
          ),
          field,
        );
        expect(arrowRect.topLeft, frameOf(arrow).topLeft);
        expect(arrowRect.width, frameOf(arrow).width);

        // Verifying: the spinner in the submit's place.
        source.gate = Completer<void>();
        await tester.testTextInput.receiveAction(TextInputAction.done);
        await tester.pump();
        expect(
          relative(rectOf(tester, 'lock spinner'), field),
          frameOf(layer(verifying, 'Spinner')),
        );
        expect(find.byKey(const ValueKey('lock submit')), findsNothing);
        source.gate!.complete();
        await tester.pumpAndSettle();

        // Wrong: the dots stay, selected (Line/Focus, radius 8, 10 to 90), the submit stays.
        final selection = layer(wrong, 'Selection');
        expect(LockFieldMetrics.selection(6), frameOf(selection));
        expect(ink().selection, fillOf(selection));
        expect(ink().caret, isNull);
        expect(find.byKey(const ValueKey('lock submit')), findsOneWidget);
      });

      testWidgets('Help Button/Rest: a 32 glass circle and the ?', (
        tester,
      ) async {
        final master = kit('help-button-rest-$a');
        await pumpKit(tester, look: look);
        final help = rectOf(tester, 'lock help');
        expect(help, LockMetrics.helpButton(screen));
        expect(help.size, frameOf(master).size);
        final background = layer(master, 'Background');
        final face = faceUnder(tester, find.byKey(const ValueKey('lock help')));
        expect(face.fill, fillOf(background));
        expect(face.rim, borderOf(background));
        expect(face.shape, const CircleBorder());
        final mark = layer(master, 'Question mark');
        final glyph = tester.widget<Text>(
          find.byKey(const ValueKey('lock help glyph')),
        );
        expect(glyph.data, mark['text']);
        expectInk(glyph.style, mark);
        expect(glyph.style?.fontWeight, FontWeight.w600);
        expect(
          relative(rectOf(tester, 'lock help glyph'), help).top,
          frameOf(mark).top,
        );
      });

      testWidgets('Hint and Help Address after the second failure', (
        tester,
      ) async {
        final hint = layer(kit('hint-default-$a'), 'Hint');
        final address = layer(kit('help-address-rest-$a'), 'Address');
        await pumpKit(
          tester,
          look: look,
          values: {
            'lockscreen.passwordHint': 'My cat',
            'lockscreen.helpAddress': 'help.example.org/password',
          },
        );
        await typeAndFail(tester, 'one');
        await typeAndFail(tester, 'two');
        final text = tester.widget<Text>(find.text('My cat'));
        expectInk(text.style, hint);
        expect(text.style?.fontWeight, FontWeight.w600);
        expect(text.style?.height, 18 / 13);
        final link = tester.widget<Text>(
          find.text('help.example.org/password'),
        );
        expectInk(link.style, address);
        expect(
          rectOf(tester, 'lock help address'),
          LockMetrics.helpAddress(screen),
        );
      });

      testWidgets('Notification/Collapsed: the app and the count', (
        tester,
      ) async {
        final master = kit('notification-collapsed-$a');
        final n = ShellNotification(
          id: 1,
          appName: 'Mail',
          summary: 'Subject',
          body: 'Body',
          desktopEntry: 'mail',
          timestamp: fixedNow,
        );
        await pumpKit(
          tester,
          look: look,
          notifications: FakeNotifications({1: n}),
        );
        final banner = rectOf(tester, 'lock banner mail');
        expect(banner, LockMetrics.banner(screen, 0));
        expect(banner.size, frameOf(master).size);
        Rect textRect(String s) =>
            relative(tester.getRect(find.text(s)), banner);
        expect(textRect('Mail').topLeft, frameOf(layer(master, 'App')).topLeft);
        expect(
          textRect('1 notification').topLeft,
          frameOf(layer(master, 'Count')).topLeft,
        );
        expect(
          tester.widget<Text>(find.text('Mail')).style?.color,
          fontColorOf(layer(master, 'App')),
        );
        expect(
          tester.widget<Text>(find.text('1 notification')).style?.color,
          fontColorOf(layer(master, 'Count')),
        );
      });

      testWidgets('Power Buttons and Action Button (provisional)', (
        tester,
      ) async {
        final row = kit('power-buttons-provisional-$a');
        final button = kit('action-button-rest-$a');
        await pumpKit(
          tester,
          look: look,
          values: {'lockscreen.showPowerButtons': true},
        );
        final power = rectOf(tester, 'lock power');
        expect(power.size, frameOf(row).size);
        final buttons = tester
            .widgetList(find.byType(LockActionButton))
            .toList();
        expect(buttons, hasLength(3));
        for (final (i, name) in const [
          'Sleep',
          'Restart',
          'Shut Down',
        ].indexed) {
          final r = relative(
            tester.getRect(find.byType(LockActionButton).at(i)),
            power,
          );
          expect(r, frameOf(layer(row, name)), reason: name);
        }
        final shutDown = tester.getRect(find.byType(LockActionButton).at(2));
        final label = relative(
          tester.getRect(find.text('Shut Down')),
          shutDown,
        );
        expect(label.top, frameOf(layer(button, 'Label')).top);
        expect(
          tester.widget<Text>(find.text('Shut Down').last).style?.color,
          fontColorOf(layer(button, 'Label')),
        );
        final face = faceUnder(tester, find.byType(LockActionButton).at(2));
        expect(face.fill, fillOf(layer(button, 'Background')));
      });

      testWidgets('Status Bar/Bar/Lock Screen/Default: where the menu bar has it', (
        tester,
      ) async {
        final master = kit('status-bar-lock-screen-default-$a');
        ShellChannel.instance.states.value = const {
          'keyboard.sources': 'it,us',
          'keyboard.source': '0',
        };
        await pumpKit(
          tester,
          look: look,
          system: FakeSystem(
            const SystemSnapshot(
              wifiAvailable: true,
              wifiEnabled: true,
              wifiSsid: 'home',
              wifiStrength: 100,
              batteryPresent: true,
              batteryPercent: 84,
            ),
          ),
        );
        final bar = rectOf(tester, 'lock status bar');
        // x 1724 at 1920: the right edge 8 from the screen's, the kit's 188 × 32.
        expect(bar, Rect.fromLTWH(1724, 0, frameOf(master).width, 32));
        for (final (layerName, key) in const [
          ('Input Source', 'status input source'),
          ('Wi-Fi', 'status wifi'),
          ('Battery', 'status battery'),
        ]) {
          final kitFrame = frameOf(layer(master, layerName));
          final r = relative(rectOf(tester, key), bar);
          expect(r.left, kitFrame.left, reason: layerName);
          expect(r.width, kitFrame.width, reason: layerName);
          // The hit area is the bar's full height; the item's box is 28 at y 2.
          expect(r.height, 32);
        }
        expect(find.text('Italian'), findsOneWidget);
        expect(find.text('84%'), findsOneWidget);
        // On-photo white in both appearances, the on-photo shadow in light.
        final style = AmalfiStatusBarStyle.of(
          tester.element(find.text('Italian')),
        );
        expect(style.ink, const Color(0xFFF4F5F7));
        expect(style.shadows, look.dark ? isEmpty : AmalfiShadows.onPhoto);
        expect(find.byType(AmalfiStatusBarPlate), findsNothing);
        expect(
          tester
              .widget<AmalfiStatusBar>(find.byType(AmalfiStatusBar))
              .placement,
          AmalfiStatusBarPlacement.lockScreen,
        );
      });
    });
  }

  testWidgets('the light appearance adds the scrim; the dark one does not', (
    tester,
  ) async {
    await pumpKit(tester, look: light);
    final scrim = tester.widget<DecoratedBox>(
      find.byKey(const ValueKey('lock scrim')),
    );
    expect(
      (scrim.decoration as BoxDecoration).gradient,
      AmalfiScrims.lockLight.gradient,
    );
    await pumpKit(tester);
    expect(find.byKey(const ValueKey('lock scrim')), findsNothing);
  });

  testWidgets('Increase contrast: opaque glass, solid clock with a 2 pt rim, '
      'the Status Bar on its plate', (tester) async {
    const look = LockLook(increaseContrast: true);
    await pumpKit(
      tester,
      look: look,
      system: FakeSystem(const SystemSnapshot(wifiAvailable: true)),
    );
    final face = faceUnder(tester, find.byKey(const ValueKey('lock password')));
    final variant = AmalfiMaterials.lock.increaseContrast(Brightness.dark)!;
    expect(face.fill, variant.fill);
    expect(face.rim, variant.rimInside);
    expect(face.rimLight, isNull);
    // No backdrop under the field: the variant is opaque.
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('lock password')),
        matching: find.byType(BackdropFilter),
      ),
      findsNothing,
    );
    // The clock: solid white over a 2 pt black 40 % rim, no glass.
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('lock time')),
        matching: find.byType(BackdropFilter),
      ),
      findsNothing,
    );
    final strokes = tester
        .widgetList<Text>(
          find.descendant(
            of: find.byKey(const ValueKey('lock time')),
            matching: find.byType(Text),
          ),
        )
        .map((t) => t.style?.foreground)
        .whereType<Paint>()
        .toList();
    expect(strokes.first.strokeWidth, 4);
    expect(strokes.first.color.toARGB32(), 0x66000000);
    expect(find.byType(AmalfiStatusBarPlate), findsOneWidget);
    // The avatar's edge is white 60 %.
    final disc = faceUnder(tester, find.byKey(const ValueKey('lock avatar')));
    expect(disc.rim, const Color(0x99FFFFFF));
  });

  testWidgets(
    'Reduce transparency: the opaque Glass/Lock and the white 92 % clock',
    (tester) async {
      const look = LockLook(reduceTransparency: true);
      await pumpKit(tester, look: look);
      final face = faceUnder(
        tester,
        find.byKey(const ValueKey('lock password')),
      );
      expect(face.fill, const Color(0xFF5B5C5E));
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('lock time')),
          matching: find.byType(BackdropFilter),
        ),
        findsNothing,
      );
      final fills = tester
          .widgetList<Text>(
            find.descendant(
              of: find.byKey(const ValueKey('lock time')),
              matching: find.byType(Text),
            ),
          )
          .map((t) => t.style?.color)
          .whereType<Color>()
          .toSet();
      expect(fills, {AmalfiGlyphMaterials.clock.reduceTransparencyFill});
    },
  );

  group('input source', () {
    testWidgets('hidden with one source; the name and the menu with two', (
      tester,
    ) async {
      final executed = <Object?>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        ShellChannel.channel,
        (call) async {
          if (call.method == 'execute') executed.add(call.arguments);
          return true;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          ShellChannel.channel,
          null,
        ),
      );
      await pumpKit(tester);
      expect(find.byKey(const ValueKey('status input source')), findsNothing);
      ShellChannel.instance.states.value = const {
        'keyboard.sources': 'us,it',
        'keyboard.source': '0',
      };
      await tester.pump();
      expect(find.text('U.S.'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('status input source')));
      await tester.pumpAndSettle();
      expect(find.byType(BarMenuHost), findsOneWidget);
      // The menu hangs from the item, like a menu bar menu.
      final item = rectOf(tester, 'status input source');
      final menu = tester.getRect(find.byType(AmalfiMenu));
      expect(menu.top, 40);
      expect(menu.left, lessThanOrEqualTo(item.left - 4));
      expect(menu.right, lessThanOrEqualTo(screen.width));
      expect(find.text('Italian'), findsOneWidget);
      await tester.tap(find.text('Italian'));
      // The chosen row blinks, then the menu runs it and closes.
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpAndSettle();
      expect(executed, [
        {
          'id': 'keyboard.select_source',
          'params': {'index': 1},
        },
      ]);
      expect(find.byType(BarMenuHost), findsNothing);
      // The field has the keyboard again.
      final input = tester.widget<EditableText>(
        find.descendant(
          of: find.byKey(const ValueKey('lock password')),
          matching: find.byType(EditableText),
        ),
      );
      expect(input.focusNode.hasFocus, isTrue);
    });

    testWidgets('a click outside closes the menu', (tester) async {
      ShellChannel.instance.states.value = const {
        'keyboard.sources': 'us,it',
        'keyboard.source': '1',
      };
      await pumpKit(tester);
      expect(find.text('Italian'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('status input source')));
      await tester.pumpAndSettle();
      expect(find.byType(BarMenuHost), findsOneWidget);
      await tester.tapAt(const Offset(400, 600));
      await tester.pumpAndSettle();
      expect(find.byType(BarMenuHost), findsNothing);
    });

    test('the names of the sources, in the interface language', () {
      expect(inputSourceName('it', 'en'), 'Italian');
      expect(inputSourceName('it', 'it'), 'Italiano');
      expect(inputSourceName('us(intl)', 'en'), 'U.S. (intl)');
      expect(inputSourceName('xx', 'en'), 'XX');
      expect(
        InputSourceState.of(const {
          'keyboard.sources': 'us,de(nodeadkeys)',
          'keyboard.source': '5',
        }),
        const InputSourceState(names: ['us', 'de(nodeadkeys)'], index: 1),
      );
      expect(InputSourceState.of(const {}).names, isEmpty);
    });
  });

  testWidgets('Tab goes field, help button, input source', (tester) async {
    ShellChannel.instance.states.value = const {
      'keyboard.sources': 'us,it',
      'keyboard.source': '0',
    };
    await pumpKit(tester);
    final input = tester.widget<EditableText>(
      find.descendant(
        of: find.byKey(const ValueKey('lock password')),
        matching: find.byType(EditableText),
      ),
    );
    expect(input.focusNode.hasFocus, isTrue);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
    // ignore: avoid_print
    final help = Focus.of(
      tester.element(find.byKey(const ValueKey('lock help glyph'))),
    );
    expect(help.hasFocus, isTrue);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'lock input source');
    // Focused, the item shows its capsule, and Return opens the menu.
    expect(
      tester
          .widget<AmalfiStatusInputSourceItem>(
            find.byType(AmalfiStatusInputSourceItem),
          )
          .active,
      isTrue,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(find.byType(BarMenuHost), findsOneWidget);
  });

  testWidgets('the shared rule: an item has the same size and x from the right '
      'in the menu bar and on the lock screen', (tester) async {
    const snapshot = SystemSnapshot(
      wifiAvailable: true,
      wifiEnabled: true,
      wifiSsid: 'home',
      wifiStrength: 100,
      batteryPresent: true,
      batteryPercent: 84,
    );
    Future<(Rect, Rect)> rects(AmalfiStatusBarPlacement placement) async {
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: AmalfiTheme(
            brightness: Brightness.dark,
            child: Align(
              alignment: Alignment.topRight,
              child: Builder(
                builder: (context) {
                  final l = lookupAppLocalizations(const Locale('en'));
                  return AmalfiStatusBar(
                    placement: placement,
                    items: [
                      networkStatusItem(l, snapshot)!,
                      batteryStatusItem(l, snapshot, percentage: true)!,
                    ],
                  );
                },
              ),
            ),
          ),
        ),
      );
      return (rectOf(tester, 'status wifi'), rectOf(tester, 'status battery'));
    }

    final menuBar = await rects(AmalfiStatusBarPlacement.menuBar);
    final lockScreen = await rects(AmalfiStatusBarPlacement.lockScreen);
    expect(lockScreen, menuBar);
  });
}
