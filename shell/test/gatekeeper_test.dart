import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
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

  test('the texts say where the app came from and why it was blocked', () {
    final l10n = englishTexts;
    final fromImage = gatekeeperTexts(
      l10n,
      request(1, 'confirm', origin: 'file:///home/u/Downloads/Hello%201.0.sxv'),
    );
    expect(fromImage.body, [
      'It comes from the disk image “Hello 1.0”.',
      'Developer: Example Ltd (identified).',
    ]);
    // Cancel is the default of a confirmation.
    expect(fromImage.buttons.first, ('cancel', 'Cancel', true));
    final fromSite = gatekeeperTexts(
      l10n,
      request(1, 'confirm', origin: 'https://apps.example.org/get'),
    );
    expect(fromSite.body.first, 'It was downloaded from apps.example.org.');
    final saved = gatekeeperTexts(
      l10n,
      request(1, 'confirm', agent: 'org.mozilla.firefox'),
    );
    expect(saved.body.first, 'It was saved by org.mozilla.firefox.');
    final unsigned = gatekeeperTexts(
      l10n,
      request(2, 'block', reason: 'unsigned', status: 'unsigned'),
    );
    expect(unsigned.title, '“Hello” Not Opened');
    expect(unsigned.body, hasLength(2));
    final tampered = gatekeeperTexts(
      l10n,
      request(2, 'block', reason: 'tampered'),
    );
    // No Open Anyway for a modified app.
    expect(tampered.body, hasLength(1));
    final malware = gatekeeperTexts(l10n, request(3, 'malware'));
    expect(malware.buttons.firstWhere((b) => b.$3).$1, 'trash');
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
