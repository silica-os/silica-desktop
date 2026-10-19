import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_ui/silica_ui.dart';
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_shell/system/desktop_entries.dart';
import 'package:silica_shell/trust/gatekeeper_dialog.dart';

import 'glass_backend_probe.dart';

class FakeGatekeeper implements GatekeeperSource {
  final requests = StreamController<GatekeeperRequest>.broadcast();
  final closes = StreamController<int>.broadcast();
  final answers = <String>[];
  var registered = false;
  List<GatekeeperRequest> waiting = const [];

  @override
  Future<void> register() async => registered = true;
  @override
  Future<List<GatekeeperRequest>> pending() async => waiting;
  @override
  Stream<GatekeeperRequest> get requested => requests.stream;
  @override
  Stream<int> get closed => closes.stream;
  @override
  Future<void> answer(int id, String choice) async =>
      answers.add('$id $choice');
}

GatekeeperRequest request(
  int id,
  String verdict, {
  String reason = '',
  String origin = '',
  String agent = '',
  String status = 'verified',
}) => GatekeeperRequest(
  id: id,
  verdict: verdict,
  info: {
    'id': 'dev.example.Hello',
    'name': 'Hello',
    'path': '/home/u/Applications/Hello.app',
    'developer': 'Example Ltd',
    'developer_status': status,
    'origin': origin,
    'agent': agent,
    'reason': reason,
  },
);

Future<void> pump(WidgetTester tester, FakeGatekeeper source) async {
  tester.view.physicalSize = const Size(1280, 800);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    const MethodChannel('silica/layer', JSONMethodCodec()),
    (call) async => true,
  );
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: compositorScope(
        GatekeeperLayer(
          source: source,
          entries: const {
            'dev.example.Hello': DesktopEntry(
              id: 'dev.example.Hello',
              name: 'Hello',
              exec: 'silica-launch /home/u/Applications/Hello.app',
            ),
          },
        ),
      ),
    ),
  );
  await tester.pump();
}

void main() {
  test('the D-Bus arguments become requests', () {
    final r = GatekeeperRequest.fromArgs([
      3,
      'confirm',
      {'name': 'Hello', 'origin': 'https://example.org/Hello.sxv'},
    ])!;
    expect((r.id, r.verdict, r.name), (3, 'confirm', 'Hello'));
    expect(GatekeeperRequest.fromArgs(['x']), isNull);
    expect(GatekeeperRequest.fromArgs([1, 2, 3]), isNull);
  });

  test('the texts of the four variants (kit Alerts/Gatekeeper)', () {
    final l10n = englishTexts;
    final fromSite = gatekeeperTexts(
      l10n,
      request(1, 'confirm', origin: 'https://lemon.example.org/get'),
    );
    expect(fromSite.message, isNull);
    expect(fromSite.detail, ['lemon.example.org', 'Developer: Example Ltd']);
    // Cancel, then Open: the default comes first.
    expect(fromSite.buttons.map((b) => b.$1), ['cancel', 'open']);
    expect(fromSite.buttons.first.$3, AmalfiPushButtonVariant.primary);
    expect(fromSite.escape, 'cancel');
    // A disk image origin names the image; no origin: the developer line only.
    expect(
      gatekeeperTexts(
        l10n,
        request(
          1,
          'confirm',
          origin: 'file:///home/u/Downloads/Hello%201.0.sxv',
        ),
      ).detail.first,
      'Hello 1.0.sxv',
    );
    expect(
      gatekeeperTexts(
        l10n,
        request(1, 'confirm', status: 'self-signed'),
      ).detail,
      ['Developer: Example Ltd (self-signed)'],
    );
    // Every reason of a block says the same.
    for (final reason in [
      'unsigned',
      'self-signed',
      'tampered',
      'invalid',
      'policy',
    ]) {
      final t = gatekeeperTexts(l10n, request(2, 'block', reason: reason));
      expect(t.title, '“Hello” Not Opened');
      expect(
        t.message,
        'Silica cannot verify that “Hello” is free of malware or has not been modified.',
      );
      expect(t.buttons.map((b) => b.$1), ['cancel', 'trash']);
    }
    final malware = gatekeeperTexts(l10n, request(3, 'malware'));
    expect(malware.buttons.first, (
      'trash',
      'Move to Trash',
      AmalfiPushButtonVariant.destructive,
    ));
    expect(malware.escape, 'cancel');
    final volume = gatekeeperTexts(l10n, request(4, 'volume'));
    expect(volume.title, '“Hello” is running from a disk image.');
    expect(volume.buttons.map((b) => b.$1), ['move', 'cancel']);
  });

  testWidgets('the shell registers, shows the oldest prompt and answers', (
    tester,
  ) async {
    final source = FakeGatekeeper();
    await pump(tester, source);
    expect(source.registered, isTrue);
    source.requests.add(request(5, 'confirm', origin: 'https://example.org/'));
    source.requests.add(request(6, 'block', reason: 'self-signed'));
    await tester.pump();
    await tester.pump();
    expect(find.byKey(const ValueKey('gatekeeper 5')), findsOneWidget);
    expect(
      find.text(
        '“Hello” is an app downloaded from the Internet. Are you sure you want to open it?',
      ),
      findsOneWidget,
    );
    await tester.tap(find.byKey(const ValueKey('gatekeeper open')));
    await tester.pump();
    expect(source.answers, ['5 open']);
    // The next one: Return is Done.
    expect(find.byKey(const ValueKey('gatekeeper 6')), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(source.answers, ['5 open', '6 cancel']);
    expect(find.byKey(const ValueKey('gatekeeper 6')), findsNothing);
    // A prompt that timed out in the daemon goes away by itself.
    source.requests.add(request(7, 'confirm'));
    await tester.pump();
    await tester.pump();
    source.closes.add(7);
    await tester.pump();
    await tester.pump();
    expect(find.byKey(const ValueKey('gatekeeper 7')), findsNothing);
  });
}
