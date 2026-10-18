import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/dock/dock.dart' show shellQuote;
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_shell/screenshot/files.dart';
import 'package:silica_shell/screenshot/screenshot.dart';
import 'package:silica_shell/screenshot/selection.dart';
import 'package:silica_shell/screenshot/services.dart';
import 'package:silica_shell/screenshot/toolbar.dart'
    show
        ShotGlass,
        ShotToolbar,
        actionWidth,
        captureModesX,
        controlHeight,
        recordModesX,
        tipAboveBar,
        toolbarActionSpace;
import 'package:silica_shell/system/desktop_entries.dart';
import 'package:silica_shell/system/glass_profile.dart';
import 'package:silica_shell/system/shadowed_glass.dart';
import 'package:silica_ui/silica_ui.dart';

import 'glass_backend_probe.dart';

/// The system, recorded.
class FakeServices implements ScreenshotServices {
  final executed = <String>[];
  final params = <Map<String, Object?>?>[];
  final launched = <String>[];
  final sounds = <String>[];
  final copied = <String>[];
  final dragged = <String>[];
  Offset? hotspot;
  bool dragTaken = true;
  final drags = StreamController<String>.broadcast();
  String? folder;

  @override
  Future<bool> execute(String id, [Map<String, Object?>? p]) async {
    executed.add(id);
    params.add(p);
    return true;
  }

  /// The commands other than the settings and the veil.
  List<String> get captures => [
    for (final id in executed)
      if (id != 'screenshot.settings' && id != 'screenshot.window_veil') id,
  ];

  /// The parameters of the last capture command.
  Map<String, Object?>? get lastCapture {
    for (var i = executed.length - 1; i >= 0; i--) {
      if (executed[i] != 'screenshot.settings' &&
          executed[i] != 'screenshot.window_veil') {
        return params[i];
      }
    }
    return null;
  }

  @override
  void launch(String command) => launched.add(command);

  @override
  void playSound(String name) => sounds.add(name);

  @override
  Future<bool> copyImage(String path) async {
    copied.add(path);
    return true;
  }

  @override
  Future<bool> startDrag(
    String path, {
    ui.Image? icon,
    ui.Offset? hotspot,
  }) async {
    dragged.add(path);
    this.hotspot = hotspot;
    return dragTaken;
  }

  @override
  Stream<String> get dragFinished => drags.stream;

  @override
  Future<String?> chooseFolder({required String title, String? start}) async =>
      folder;
}

/// The centre of a mode button of the bar: [index] 0 screen, 1 window, 2 selection.
Offset modeAt(WidgetTester tester, int index, {bool record = false}) {
  final bar = tester.getRect(find.byType(ShotToolbar));
  return bar.topLeft +
      Offset((record ? recordModesX : captureModesX) + 4 + 32 * index + 16, 24);
}

void main() {
  late FakeServices services;
  late Directory dir;
  late Directory desk;
  late Directory docs;
  late SilicaSettings store;
  var seq = 1000;

  setUp(() {
    store = SilicaSettings.withValues({});
    services = FakeServices();
    dir = Directory.systemTemp.createTempSync('istantanee');
    desk = Directory('${dir.path}/Scrivania')..createSync();
    docs = Directory('${dir.path}/Documenti')..createSync();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('silica/layer', JSONMethodCodec()),
          (call) async => true,
        );
  });

  tearDown(() => dir.deleteSync(recursive: true));

  Future<void> send(WidgetTester tester, String method, Object? args) =>
      tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        'silica/shell',
        const JSONMethodCodec().encodeMethodCall(MethodCall(method, args)),
        (_) {},
      );

  /// A capture announced by Ossido: a temporary file of the given size.
  Future<File> announce(
    WidgetTester tester, {
    double width = 2560,
    double height = 1440,
    bool clipboard = false,
  }) async {
    final temp = File('${dir.path}/tmp-${seq++}.png')
      ..writeAsBytesSync([0, 1, 2]);
    await send(tester, 'broadcast', {
      'topic': 'screenshot',
      'value': jsonEncode({
        'path': temp.path,
        'width': width,
        'height': height,
        'clipboard': clipboard,
        'seq': seq,
      }),
    });
    await tester.pump();
    return temp;
  }

  Future<void> pumpLayer(
    WidgetTester tester, {
    Rect? Function(Offset)? windowAt,
    Map<String, DesktopEntry> entries = const {},
    Map<String, String>? env,
  }) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: compositorScope(
          ScreenshotLayer(
            folder: desk.path,
            documents: docs.path,
            cache: '${dir.path}/cache',
            now: () => DateTime(2026, 10, 14, 4, 12, 33),
            windowAt: windowAt,
            services: services,
            settings: store,
            entries: entries,
            env: env ?? {'HOME': dir.path},
          ),
        ),
      ),
    );
  }

  ScreenshotSettings savedSettings() => ScreenshotSettings.of(store);

  const deskName = 'Screenshot 2026-10-14 at 04.12.33.png';

  // --- Files and settings -----------------------------------------------------------------

  test('the file name, localized, with a number if it already exists', () {
    final t = DateTime(2026, 10, 14, 4, 12, 33);
    expect(screenshotName(t, (_) => false), deskName);
    expect(
      screenshotName(t, (n) => !n.contains('(')),
      'Screenshot 2026-10-14 at 04.12.33 (2).png',
    );
    expect(
      screenshotName(
        t,
        (_) => false,
        texts: lookupAppLocalizations(const Locale('it')),
      ),
      'Istantanea schermo 2026-10-14 alle 04.12.33.png',
    );
  });

  test('the settings survive a round trip and hostile values', () {
    final s = ScreenshotSettings(
      destination: const ToFolder('/tmp/x'),
      timer: 10,
      showThumbnail: false,
      rememberSelection: false,
      showPointer: true,
      mode: ToolbarChoice.window,
      selection: const Rect.fromLTWH(1, 2, 3, 4),
      barCenter: const Offset(5, 6),
      recentFolders: ['/tmp/x'],
    );
    final back = ScreenshotSettings.of(SilicaSettings.withValues(s.toValues()));
    expect(back.destination, const ToFolder('/tmp/x'));
    expect(back.timer, 10);
    expect(back.showThumbnail, isFalse);
    expect(back.rememberSelection, isFalse);
    expect(back.showPointer, isTrue);
    expect(back.mode, ToolbarChoice.window);
    expect(back.selection, const Rect.fromLTWH(1, 2, 3, 4));
    expect(back.barCenter, const Offset(5, 6));
    expect(back.recentFolders, ['/tmp/x']);
    final hostile = ScreenshotSettings.of(
      SilicaSettings.withValues({
        'screenshot.destination': 42,
        'screenshot.timer': 7,
        'screenshot.thumbnail': 'yes',
        'screenshot.selection': [1, 2, -3, 4],
        'screenshot.bar': [1],
        'screenshot.recentFolders': ['relative', '/ok'],
      }),
    );
    expect(hostile.destination, const ToDesktop());
    expect(hostile.timer, 0);
    expect(hostile.showThumbnail, isTrue);
    expect(hostile.selection, isNull);
    expect(hostile.barCenter, isNull);
    expect(hostile.recentFolders, ['/ok']);
    // Other folders: newest first, at most three.
    final r = ScreenshotSettings();
    for (final f in ['/a', '/b', '/c', '/d', '/b']) {
      r.chooseFolder(f);
    }
    expect(r.recentFolders, ['/b', '/d', '/c']);
    expect(r.destination, const ToFolder('/b'));
  });

  test('the default image viewer comes from mimeapps.list', () {
    const list = '''
[Added Associations]
image/png=other.desktop;
[Default Applications]
image/jpeg=jpeg.desktop
image/png=missing.desktop;org.example.Viewer.desktop;
''';
    expect(
      defaultInMimeapps(list, 'image/png', (id) => id != 'missing'),
      'org.example.Viewer',
    );
    expect(defaultInMimeapps(list, 'image/gif', (_) => true), isNull);
    final config = Directory('${dir.path}/config')..createSync();
    File('${config.path}/mimeapps.list').writeAsStringSync(list);
    expect(
      defaultAppFor(
        'image/png',
        env: {'HOME': dir.path, 'XDG_CONFIG_HOME': config.path},
        exists: (id) => id == 'org.example.Viewer',
      ),
      'org.example.Viewer',
    );
  });

  test('a live selection: Shift keeps a side, Option grows from the center, '
      'Space moves it', () {
    final live = LiveSelection(const Offset(100, 100));
    expect(
      live.update(const Offset(200, 150), shift: false, option: false),
      const Rect.fromLTRB(100, 100, 200, 150),
    );
    // Shift, then a move mostly to the right: the height stays.
    live.update(const Offset(230, 152), shift: true, option: false);
    expect(
      live.update(const Offset(260, 170), shift: true, option: false),
      const Rect.fromLTRB(100, 100, 260, 150),
    );
    // Option: centered on the start.
    expect(
      live.update(const Offset(150, 130), shift: false, option: true),
      const Rect.fromLTRB(50, 70, 150, 130),
    );
    // Space: the whole selection follows the pointer.
    live.moving = const Offset(150, 130);
    expect(
      live.update(const Offset(160, 140), shift: false, option: false),
      const Rect.fromLTRB(110, 110, 160, 140),
    );
  });

  // --- The thumbnail ------------------------------------------------------------------------

  testWidgets(
    'the thumbnail at the bottom right, the sound, then the file on the Desktop',
    (tester) async {
      await pumpLayer(tester);
      final temp = await announce(tester);
      await tester.pump(const Duration(milliseconds: 500));
      expect(services.sounds, ['screen-capture']);
      // The 256 × 144 image with the 4 pt frame, 12 pt from the edges.
      final thumb = tester.getRect(find.byKey(const ValueKey('thumbnail')));
      expect(thumb.size, const Size(264, 152));
      expect(thumb.right, 1280 - 12);
      expect(thumb.bottom, 800 - 12);
      // White frame of radius 8 (kit `Thumbnail shadow`) with the level 2 shadow.
      final frame = tester.widget<DecoratedBox>(
        find.byKey(const ValueKey('thumbnail frame')),
      );
      final decoration = frame.decoration as AmalfiShapeDecoration;
      expect(decoration.color, AmalfiColors.light.textOnAccent);
      expect(
        (decoration.shape as AmalfiSmoothBorder).radii.first,
        thumbnailRadius,
      );
      expect(decoration.shadows, AmalfiShadows.light.level2);
      expect(find.byType(Transform), findsNothing);
      // It stays ~5.4 s after the fade-in, then leaves in ~250 ms and the file goes to the
      // Desktop.
      await tester.pump(const Duration(seconds: 5));
      expect(temp.existsSync(), isTrue);
      await tester.pump(const Duration(milliseconds: 700));
      expect(temp.existsSync(), isTrue, reason: 'still there at 5.9 s');
      for (var i = 0; i < 10; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
      expect(temp.existsSync(), isFalse);
      expect(File('${desk.path}/$deskName').existsSync(), isTrue);
      expect(find.byKey(const ValueKey('thumbnail')), findsNothing);
    },
  );

  testWidgets('a clipboard capture is copied, with no thumbnail and no file', (
    tester,
  ) async {
    await pumpLayer(tester);
    final temp = await announce(tester, clipboard: true);
    await tester.pump();
    expect(services.copied, [temp.path]);
    expect(services.sounds, ['screen-capture']);
    expect(find.byKey(const ValueKey('thumbnail')), findsNothing);
    expect(temp.existsSync(), isFalse);
    expect(desk.listSync(), isEmpty);
  });

  testWidgets('a click on the thumbnail saves the file and opens it', (
    tester,
  ) async {
    await pumpLayer(tester);
    await announce(tester);
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(find.byKey(const ValueKey('thumbnail')));
    await tester.pump();
    final saved = '${desk.path}/$deskName';
    expect(File(saved).existsSync(), isTrue);
    expect(services.launched.single, contains("gio open '$saved'"));
    expect(find.byKey(const ValueKey('thumbnail')), findsNothing);
  });

  testWidgets('the thumbnail menu: Delete removes the capture', (tester) async {
    await pumpLayer(
      tester,
      entries: {
        fileManagerId: const DesktopEntry(
          id: fileManagerId,
          name: 'Mustache',
          exec: 'mustache',
        ),
      },
    );
    final temp = await announce(tester);
    await tester.pump(const Duration(milliseconds: 500));
    final thumb = find.byKey(const ValueKey('thumbnail'));
    await tester.tap(thumb, buttons: kSecondaryButton);
    await tester.pump();
    expect(find.byType(AmalfiMenuList), findsOneWidget);
    // Whole on the screen, though the thumbnail touches the right edge.
    expect(
      tester.getRect(find.byType(AmalfiMenuList)).right,
      lessThanOrEqualTo(1280 - 8),
    );
    for (final item in [
      'Save to Desktop',
      'Save to Documents',
      'Save to Clipboard',
      'Show in Mustache',
      'Delete',
      'Close',
    ]) {
      expect(find.text(item), findsOneWidget, reason: item);
    }
    // The menu keeps the thumbnail past its time.
    await tester.pump(const Duration(seconds: 7));
    expect(thumb, findsOneWidget);
    await tester.tap(find.text('Delete'));
    await tester.pump(const Duration(milliseconds: 500));
    expect(thumb, findsNothing);
    expect(temp.existsSync(), isFalse);
    expect(desk.listSync(), isEmpty);
    await tester.pump(const Duration(seconds: 6));
    expect(desk.listSync(), isEmpty);
    // No file anywhere, no sound but the shutter at the capture.
    expect(
      Directory('${dir.path}/cache').existsSync() &&
          Directory('${dir.path}/cache').listSync().isNotEmpty,
      isFalse,
    );
    expect(services.sounds, ['screen-capture']);
    expect(services.copied, isEmpty);
  });

  DesktopEntry app(String id, String name) =>
      DesktopEntry(id: id, name: name, exec: id);

  /// The labels of the open menu, from top to bottom.
  List<String> menuLabels(WidgetTester tester) {
    final texts = find.descendant(
      of: find.byType(AmalfiMenuList),
      matching: find.byType(Text),
    );
    final found = [
      for (final e in texts.evaluate())
        (
          (e.widget as Text).data ?? '',
          tester.getTopLeft(find.byWidget(e.widget)).dy,
        ),
    ]..sort((a, b) => a.$2.compareTo(b.$2));
    return [for (final f in found) f.$1];
  }

  Future<void> openThumbnailMenu(WidgetTester tester) async {
    await announce(tester);
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(
      find.byKey(const ValueKey('thumbnail')),
      buttons: kSecondaryButton,
    );
    await tester.pump();
  }

  testWidgets(
    'the thumbnail menu: the items in the measured order, only the apps there are',
    (tester) async {
      // Mail, an image viewer and Photos, and the file manager.
      final config = Directory('${dir.path}/config')..createSync();
      File(
        '${config.path}/mimeapps.list',
      ).writeAsStringSync('[Default Applications]\nimage/png=viewer.desktop\n');
      await pumpLayer(
        tester,
        env: {'HOME': dir.path, 'XDG_CONFIG_HOME': config.path},
        entries: {
          fileManagerId: app(fileManagerId, 'Mustache'),
          shotMailId: app(shotMailId, 'Mail'),
          'viewer': app('viewer', 'Viewer'),
          shotPhotosId: app(shotPhotosId, 'Photos'),
        },
      );
      await openThumbnailMenu(tester);
      expect(menuLabels(tester), [
        'Save to Desktop',
        'Save to Documents',
        'Save to Clipboard',
        'Open in Mail',
        'Open in Viewer',
        'Open in Photos',
        'Show in Mustache',
        'Delete',
        'Close',
      ]);
      // Markup is not there: the editor does not exist yet (#154).
      expect(find.textContaining('Markup'), findsNothing);
      await tester.tap(find.text('Open in Mail'));
      await tester.pump(const Duration(milliseconds: 500));
      final cached = '${dir.path}/cache/$deskName';
      expect(File(cached).existsSync(), isTrue);
      expect(
        services.launched.single,
        'dev.frascella.Mail ${shellQuote(cached)}',
      );
      expect(desk.listSync(), isEmpty);
    },
  );

  testWidgets('the thumbnail menu hides the apps Silica does not have', (
    tester,
  ) async {
    await pumpLayer(
      tester,
      entries: {shotPhotosId: app(shotPhotosId, 'Photos')},
    );
    await openThumbnailMenu(tester);
    expect(menuLabels(tester), [
      'Save to Desktop',
      'Save to Documents',
      'Save to Clipboard',
      'Open in Photos',
      'Delete',
      'Close',
    ]);
    await tester.tap(find.text('Close'));
    await tester.pump(const Duration(milliseconds: 500));
    await pumpLayer(tester);
    await openThumbnailMenu(tester);
    expect(menuLabels(tester), [
      'Save to Desktop',
      'Save to Documents',
      'Save to Clipboard',
      'Delete',
      'Close',
    ]);
  });

  testWidgets('the time stops while the menu is open and goes on from there', (
    tester,
  ) async {
    await pumpLayer(tester);
    await announce(tester);
    final thumb = find.byKey(const ValueKey('thumbnail'));
    // 3 s of the 5.78 s (fade-in and stay), then the menu.
    await tester.pump(const Duration(seconds: 3));
    await tester.tap(thumb, buttons: kSecondaryButton);
    await tester.pump();
    await tester.pump(const Duration(seconds: 30));
    expect(thumb, findsOneWidget);
    // Closed without a choice: the 2.78 s left, not the whole time again.
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.byType(AmalfiMenuList), findsNothing);
    await tester.pump(const Duration(seconds: 2));
    expect(thumb, findsOneWidget);
    await tester.pump(const Duration(seconds: 1));
    await tester.pump(const Duration(milliseconds: 300));
    await tester.pump(const Duration(milliseconds: 300));
    expect(thumb, findsNothing);
    expect(File('${desk.path}/$deskName').existsSync(), isTrue);
  });

  test('the measured timings of the thumbnail', () {
    expect(thumbnailSettle, const Duration(milliseconds: 380));
    expect(thumbnailHold, const Duration(milliseconds: 5400));
    expect(thumbnailLeave, const Duration(milliseconds: 250));
  });

  test('the drag image: 128 pt on the long side, held up and to the left', () {
    expect(thumbnailDragSize(const Size(2560, 1440)), const Size(128, 72));
    expect(thumbnailDragSize(const Size(300, 150)), const Size(128, 64));
    expect(thumbnailDragSize(const Size(100, 400)), const Size(32, 128));
    expect(thumbnailDragSize(Size.zero), Size.zero);
    expect(thumbnailDragHotspot(const Size(128, 64)), const Offset(52, 20));
  });

  testWidgets(
    'dark appearance: the frame stays white with the stronger shadow',
    (tester) async {
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(tester.platformDispatcher.clearAllTestValues);
      await pumpLayer(tester);
      await announce(tester);
      await tester.pump(const Duration(milliseconds: 500));
      final frame = tester.widget<DecoratedBox>(
        find.byKey(const ValueKey('thumbnail frame')),
      );
      final decoration = frame.decoration as AmalfiShapeDecoration;
      expect(decoration.color, AmalfiColors.dark.textOnAccent);
      expect(decoration.shadows, AmalfiShadows.dark.level2);
      expect(
        tester.getRect(find.byKey(const ValueKey('thumbnail'))).size,
        const Size(264, 152),
      );
    },
  );

  testWidgets('the shutter plays once per capture', (tester) async {
    await pumpLayer(tester);
    final temp = File('${dir.path}/once.png')..writeAsBytesSync([0, 1, 2]);
    final message = jsonEncode({
      'path': temp.path,
      'width': 100,
      'height': 50,
      'clipboard': false,
      'seq': 7,
    });
    for (var i = 0; i < 2; i++) {
      await send(tester, 'broadcast', {
        'topic': 'screenshot',
        'value': message,
      });
      await tester.pump();
    }
    expect(services.sounds, ['screen-capture']);
    await announce(tester);
    expect(services.sounds, ['screen-capture', 'screen-capture']);
  });

  testWidgets('the thumbnail menu: Save to Documents, Show in the file manager', (
    tester,
  ) async {
    await pumpLayer(
      tester,
      entries: {
        fileManagerId: const DesktopEntry(
          id: fileManagerId,
          name: 'Mustache',
          exec: 'mustache',
        ),
      },
    );
    await announce(tester);
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(
      find.byKey(const ValueKey('thumbnail')),
      buttons: kSecondaryButton,
    );
    await tester.pump();
    await tester.tap(find.text('Save to Documents'));
    await tester.pump(const Duration(milliseconds: 500));
    expect(File('${docs.path}/$deskName').existsSync(), isTrue);
    expect(desk.listSync(), isEmpty);

    // The next one, shown in the file manager: saved first, then its folder opened.
    await announce(tester);
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(
      find.byKey(const ValueKey('thumbnail')),
      buttons: kSecondaryButton,
    );
    await tester.pump();
    await tester.tap(find.text('Show in Mustache'));
    await tester.pump(const Duration(milliseconds: 500));
    expect(File('${desk.path}/$deskName').existsSync(), isTrue);
    expect(services.launched.last, "mustache '${desk.path}'");
  });

  testWidgets('dragging the thumbnail: the image follows, a drop moves it', (
    tester,
  ) async {
    await pumpLayer(tester);
    final temp = await announce(tester, width: 300, height: 150);
    await tester.pump(const Duration(milliseconds: 500));
    final thumb = find.byKey(const ValueKey('thumbnail'));
    final at = tester.getTopLeft(thumb) + const Offset(40, 30);
    final mouse = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
    await mouse.moveBy(const Offset(-30, -20));
    await tester.pump();
    // The file exists only for the drag, in the cache: nothing on the Desktop.
    final cached = '${dir.path}/cache/$deskName';
    expect(temp.existsSync(), isFalse);
    expect(File(cached).existsSync(), isTrue);
    expect(desk.listSync(), isEmpty);
    expect(services.dragged, [cached]);
    // The image is held with the pointer up and to the left of its center (128 × 64).
    expect(services.hotspot, const Offset(52, 20));
    expect(thumb, findsNothing);
    // Dropped nowhere: it comes back and goes to the Desktop when it leaves.
    services.drags.add('none');
    await tester.pump();
    await tester.pump();
    expect(thumb, findsOneWidget);
    await mouse.up();
    await tester.pump();

    // Dropped into a folder or an app: the target has its copy, this one is gone, and
    // nothing is left on the Desktop.
    final again = await tester.startGesture(
      tester.getCenter(thumb),
      kind: PointerDeviceKind.mouse,
    );
    await again.moveBy(const Offset(-40, 0));
    await tester.pump();
    expect(services.dragged, [cached, cached]);
    services.drags.add('copy');
    await tester.pump();
    await again.up();
    await tester.pump(const Duration(seconds: 7));
    expect(thumb, findsNothing);
    expect(File(cached).existsSync(), isFalse);
    expect(desk.listSync(), isEmpty);
    expect(Directory('${dir.path}/cache').listSync(), isEmpty);
  });

  testWidgets('a drag dropped nowhere still saves the capture when it leaves', (
    tester,
  ) async {
    await pumpLayer(tester);
    await announce(tester);
    await tester.pump(const Duration(milliseconds: 500));
    final thumb = find.byKey(const ValueKey('thumbnail'));
    final drag = await tester.startGesture(
      tester.getCenter(thumb),
      kind: PointerDeviceKind.mouse,
    );
    await drag.moveBy(const Offset(-40, 0));
    await tester.pump();
    services.drags.add('none');
    await tester.pump();
    await tester.pump();
    // The pointer is somewhere else by now (it would keep the thumbnail).
    await drag.moveTo(const Offset(10, 10));
    await drag.up();
    await tester.pump();
    await tester.pump(const Duration(seconds: 7));
    await tester.pump(const Duration(milliseconds: 300));
    await tester.pump(const Duration(milliseconds: 300));
    expect(thumb, findsNothing);
    expect(File('${desk.path}/$deskName').existsSync(), isTrue);
  });

  testWidgets('a two-finger swipe to the right sends the thumbnail away', (
    tester,
  ) async {
    await pumpLayer(tester);
    await announce(tester);
    await tester.pump(const Duration(milliseconds: 500));
    final thumb = find.byKey(const ValueKey('thumbnail'));
    final pad = TestPointer(1, PointerDeviceKind.trackpad);
    final center = tester.getCenter(thumb);
    await tester.sendEventToBinding(pad.panZoomStart(center));
    await tester.sendEventToBinding(
      pad.panZoomUpdate(center, pan: const Offset(25, 2)),
    );
    await tester.sendEventToBinding(
      pad.panZoomUpdate(center, pan: const Offset(50, 3)),
    );
    await tester.sendEventToBinding(pad.panZoomEnd());
    await tester.pump();
    // Saved at once, while it is still leaving.
    expect(thumb, findsOneWidget);
    expect(File('${desk.path}/$deskName').existsSync(), isTrue);
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(thumb, findsNothing);
    expect(services.dragged, isEmpty, reason: 'a swipe is not a drag');
    expect(File('${desk.path}/$deskName').existsSync(), isTrue);
  });

  testWidgets('the pointer on the thumbnail keeps it', (tester) async {
    await pumpLayer(tester);
    await announce(tester);
    await tester.pump(const Duration(milliseconds: 500));
    final thumb = find.byKey(const ValueKey('thumbnail'));
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: tester.getCenter(thumb));
    await tester.pump();
    await tester.pump(const Duration(seconds: 8));
    expect(thumb, findsOneWidget);
    await mouse.moveTo(const Offset(10, 10));
    await tester.pump();
    await tester.pump(const Duration(seconds: 6));
    await tester.pump(const Duration(milliseconds: 400));
    expect(thumb, findsNothing);
    await mouse.removePointer();
  });

  // --- ⇧⌘4 ----------------------------------------------------------------------------------

  testWidgets('⇧⌘4: drag a region; Space: a window; Esc cancels', (
    tester,
  ) async {
    await pumpLayer(tester);
    await send(tester, 'command', 'screenshot.selection');
    await tester.pump();
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(100, 100));
    await mouse.moveTo(const Offset(100, 100));
    await tester.pump();
    expect(find.byKey(const ValueKey('crosshair')), findsOneWidget);
    expect(find.text('100\n100'), findsOneWidget);
    await mouse.down(const Offset(100, 100));
    await mouse.moveTo(const Offset(300, 250));
    await tester.pump();
    expect(
      tester.getRect(find.byKey(const ValueKey('selection'))),
      const Rect.fromLTRB(100, 100, 300, 250),
    );
    // While dragging: the pointer's point in the chip, the size in its own capsule.
    expect(find.text('300\n250'), findsOneWidget);
    expect(find.text('200 × 150'), findsOneWidget);
    await mouse.up();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(services.captures, ['screenshot.region']);
    expect(services.lastCapture, {
      'x': 100.0,
      'y': 100.0,
      'width': 200.0,
      'height': 150.0,
      'pointer': false,
    });

    // Space: the window under the pointer, with its shadow, the desktop as the screen.
    await send(tester, 'command', 'screenshot.selection');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.space);
    await mouse.moveTo(const Offset(400, 300));
    await tester.pump();
    expect(find.byKey(const ValueKey('camera')), findsOneWidget);
    await mouse.down(const Offset(400, 300));
    await mouse.up();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(services.captures.last, 'screenshot.window_at');
    expect(services.lastCapture, {
      'x': 400.0,
      'y': 300.0,
      'shadow': true,
      'or_screen': true,
      'pointer': false,
    });

    // Esc: nothing.
    await send(tester, 'command', 'screenshot.selection');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(find.byKey(const ValueKey('crosshair')), findsNothing);
    // Window mode asked Ossido for the veil on the window under the pointer, then off.
    expect(
      services.executed.where((id) => id == 'screenshot.window_veil'),
      hasLength(2),
    );
    expect(services.captures, hasLength(2));
    await mouse.removePointer();
  });

  testWidgets('⌃⇧⌘4 sends the region to the clipboard', (tester) async {
    await pumpLayer(tester);
    await send(tester, 'command', 'screenshot.selection_clipboard');
    await tester.pump();
    final mouse = await tester.startGesture(
      const Offset(10, 10),
      kind: PointerDeviceKind.mouse,
    );
    await mouse.moveTo(const Offset(60, 50));
    await mouse.up();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(services.lastCapture?['clipboard'], isTrue);
  });

  testWidgets(
    'Space: light blue veil on the window rectangle, or on everything',
    (tester) async {
      const window = Rect.fromLTWH(200, 150, 500, 350);
      await pumpLayer(
        tester,
        windowAt: (p) => window.contains(p) ? window : null,
      );
      await send(tester, 'command', 'screenshot.selection');
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.space);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(400, 300));
      await mouse.moveTo(const Offset(400, 300));
      await tester.pump();
      final veil = find.byKey(const ValueKey('veil'));
      expect(tester.getRect(veil), window);
      expect(
        tester.widget<ColoredBox>(veil).color,
        AmalfiColors.light.accentAmalfi.withValues(alpha: 0.5),
      );
      // On the desktop: the whole screen.
      await mouse.moveTo(const Offset(1000, 700));
      await tester.pump();
      expect(tester.getRect(veil), const Rect.fromLTWH(0, 0, 1280, 800));
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pump();
      await mouse.removePointer();
    },
  );

  // --- ⇧⌘5 ----------------------------------------------------------------------------------

  testWidgets('⇧⌘5: the bar above the Dock; Capture with the full screen', (
    tester,
  ) async {
    await pumpLayer(tester);
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    final bar = tester.getRect(find.byType(ShotToolbar));
    // Kit `Capture Bar`: 496 × 48, 8 pt above the Dock (60 pt shelf, 16 pt from the bottom).
    expect(bar.size, const Size(496, 48));
    expect(bar.bottom, 800 - toolbarAboveDock);
    expect(bar.center.dx, 640);
    // The controls at the kit's places (`Bar Capture Screen`).
    void near(Finder f, Rect expected) {
      final r = tester.getRect(f).shift(-bar.topLeft);
      expect(r.left, closeTo(expected.left, 0.01));
      expect(r.top, closeTo(expected.top, 0.01));
      expect(r.width, closeTo(expected.width, 0.01));
      expect(r.height, closeTo(expected.height, 0.01));
    }

    near(
      find.byKey(const ValueKey('close bar')),
      const Rect.fromLTWH(8, 6, 36, 36),
    );
    final modes = find.byType(AmalfiSegmentedControl);
    expect(modes, findsNWidgets(2));
    near(modes.at(0), const Rect.fromLTWH(44, 8, 104, 32));
    near(modes.at(1), const Rect.fromLTWH(164, 8, 104, 32));
    near(
      find.byKey(const ValueKey('options')),
      const Rect.fromLTWH(284, 8, 96, 32),
    );
    near(
      find.byKey(const ValueKey('capture fill')),
      const Rect.fromLTWH(384, 8, 104, 32),
    );
    // The bar is Glass/Regular of the `control` kind with a capsule radius of 24.
    final glass = tester.widget<ProfiledGlass>(
      find
          .descendant(
            of: find.byType(ShotToolbar),
            matching: find.byType(ProfiledGlass),
          )
          .first,
    );
    expect(glass.surface, ShellGlassProfile.control);
    expect(glass.radius, 24);
    expect(glass.material, GlassMaterial.regular);
    // The chosen mode is on the pill (the selection by default); the record control is disabled.
    expect(tester.widget<AmalfiSegmentedControl>(modes.at(0)).selected, {2});
    expect(
      tester.widget<AmalfiSegmentedControl>(modes.at(1)).onChanged,
      isNull,
    );
    await tester.tapAt(modeAt(tester, 0));
    await tester.pump();
    expect(savedSettings().mode, ToolbarChoice.screen);
    expect(find.byKey(const ValueKey('screen tint')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('capture')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(services.captures, ['screenshot.screen']);
  });

  testWidgets(
    '⇧⌘5: the bar appears and goes at once, with no motion of its own',
    (tester) async {
      await pumpLayer(tester);
      await send(tester, 'command', 'screenshot.toolbar');
      await tester.pump();
      final close = find.byKey(const ValueKey('close bar'));
      final at = tester.getRect(close);
      await tester.pumpAndSettle();
      expect(tester.getRect(close), at);
      await tester.tapAt(tester.getCenter(close));
      await tester.pump();
      expect(find.byType(ShotToolbar), findsNothing);
    },
  );

  testWidgets('⇧⌘5: the close button is the toolbar icon button, 36 pt', (
    tester,
  ) async {
    await pumpLayer(tester);
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    expect(find.byType(AmalfiToolbarButton), findsOneWidget);
    final close = tester.getRect(find.byKey(const ValueKey('close bar')));
    expect(close.size, const Size(36, 36));
    final bar = tester.getRect(find.byType(ShotToolbar));
    expect(close.center.dy - bar.top, closeTo(24, 0.01));
  });

  testWidgets(
    '⇧⌘5: only the six mode buttons have a tooltip, with no delay (#96)',
    (tester) async {
      await pumpLayer(tester);
      await send(tester, 'command', 'screenshot.toolbar');
      await tester.pumpAndSettle();
      final tip = find.byKey(const ValueKey('bar tooltip'));
      final bar = tester.getRect(find.byType(ShotToolbar));
      final modes = [
        (modeAt(tester, 0), 'Capture Entire Screen'),
        (modeAt(tester, 1), 'Capture Selected Window'),
        (modeAt(tester, 2), 'Capture Selected Portion'),
        (modeAt(tester, 0, record: true), 'Record Entire Screen'),
        (modeAt(tester, 1, record: true), 'Record Selected Window'),
        (modeAt(tester, 2, record: true), 'Record Selected Portion'),
      ];
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(5, 5));
      expect(tip, findsNothing);
      for (final (center, value) in modes) {
        await mouse.moveTo(center);
        // In the first frame, not after a delay.
        await tester.pump();
        expect(
          find.descendant(of: tip, matching: find.text(value)),
          findsOneWidget,
          reason: value,
        );
        // The Amalfi tooltip (Thick, radius 8, 4 × 8 padding), centred on the button, its
        // bottom 8 pt over the bar.
        final box = tester.getRect(
          find.byKey(const ValueKey('bar tooltip box')),
        );
        expect(box.center.dx, closeTo(center.dx, 0.5), reason: value);
        expect(box.bottom, closeTo(bar.top - tipAboveBar, 0.5), reason: value);
        final text = tester.getRect(
          find.descendant(of: tip, matching: find.text(value)),
        );
        expect(box.width, closeTo(text.width + 16, 0.5), reason: value);
      }
      // It stays after the pointer leaves the bar, for good.
      await mouse.moveTo(const Offset(5, 5));
      await tester.pump(const Duration(seconds: 5));
      expect(find.text('Record Selected Portion'), findsOneWidget);
      // Close, Options and the action button have none.
      for (final key in ['close bar', 'options', 'capture']) {
        await mouse.moveTo(tester.getCenter(find.byKey(ValueKey(key))));
        await tester.pump(const Duration(seconds: 3));
        // The last mode's tooltip is still there, nothing was added.
        expect(tip, findsOneWidget, reason: key);
        expect(find.text('Close'), findsNothing, reason: key);
        expect(
          find.descendant(of: tip, matching: find.text('Options')),
          findsNothing,
          reason: key,
        );
      }
      // The next mode replaces it.
      await mouse.moveTo(modeAt(tester, 0));
      await tester.pump();
      expect(tip, findsOneWidget);
      expect(find.text('Capture Entire Screen'), findsOneWidget);
      expect(find.text('Record Selected Portion'), findsNothing);
      await mouse.removePointer();
    },
  );

  test('⇧⌘5: the tooltips are translated into Italian', () {
    final it = lookupAppLocalizations(const Locale('it'));
    expect(it.shotTipCaptureScreen, 'Acquisisci l\'intero schermo');
    expect(it.shotTipCaptureWindow, 'Acquisisci la finestra selezionata');
    expect(it.shotTipCaptureSelection, 'Acquisisci la porzione selezionata');
    expect(it.shotTipRecordScreen, 'Registra l\'intero schermo');
    expect(it.shotTipRecordWindow, 'Registra la finestra selezionata');
    expect(it.shotTipRecordSelection, 'Registra la porzione selezionata');
  });

  testWidgets(
    '⇧⌘5: the action button is the Large primary push button, 8 pt from the bar',
    (tester) async {
      await pumpLayer(tester);
      await send(tester, 'command', 'screenshot.toolbar');
      await tester.pumpAndSettle();
      final bar = tester.getRect(find.byType(ShotToolbar));
      final fill = find.byKey(const ValueKey('capture fill'));
      final rect = tester.getRect(fill);
      expect(rect.size, Size(actionWidth, controlHeight));
      expect(rect.top - bar.top, closeTo(8, 0.01));
      expect(bar.bottom - rect.bottom, closeTo(8, 0.01));
      expect(bar.right - rect.right, closeTo(8, 0.01));
      final button = tester.widget<AmalfiPushButton>(
        find.descendant(of: fill, matching: find.byType(AmalfiPushButton)),
      );
      expect(button.variant, AmalfiPushButtonVariant.primary);
      expect(button.size, AmalfiControlSize.large);
      expect(button.label, 'Capture');
    },
  );

  testWidgets(
    'window mode: the action button hides while the pointer is on the bar, the bar shrinks 108 pt',
    (tester) async {
      await pumpLayer(tester);
      await send(tester, 'command', 'screenshot.toolbar');
      await tester.pumpAndSettle();
      final full = tester.getRect(find.byType(ShotToolbar));
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(5, 5));
      await mouse.moveTo(modeAt(tester, 1));
      await tester.pump();
      await tester.tapAt(modeAt(tester, 1));
      await tester.pump();
      // Screen and selection modes keep the button; window mode hides it.
      var bar = tester.getRect(find.byType(ShotToolbar));
      expect(bar.width, full.width);
      // Starts ~60 ms after the pointer reaches the bar; the right edge slides in over 450 ms.
      await tester.pump(const Duration(milliseconds: 50));
      expect(tester.getRect(find.byType(ShotToolbar)).width, full.width);
      await tester.pump(const Duration(milliseconds: 250));
      bar = tester.getRect(find.byType(ShotToolbar));
      expect(bar.width, lessThan(full.width));
      expect(bar.width, greaterThan(full.width - toolbarActionSpace));
      // The button is already fading.
      expect(find.byKey(const ValueKey('capture')), findsOneWidget);
      await tester.pump(const Duration(milliseconds: 300));
      bar = tester.getRect(find.byType(ShotToolbar));
      expect(bar.width, full.width - toolbarActionSpace);
      expect(bar.left, full.left);
      expect(find.byKey(const ValueKey('capture')), findsNothing);
      // The pointer leaves: it comes back with the mirror animation.
      await mouse.moveTo(const Offset(5, 5));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(
        tester.getRect(find.byType(ShotToolbar)).width,
        greaterThan(full.width - toolbarActionSpace),
      );
      await tester.pump(const Duration(milliseconds: 300));
      bar = tester.getRect(find.byType(ShotToolbar));
      expect(bar, full);
      expect(find.byKey(const ValueKey('capture')), findsOneWidget);
      await mouse.removePointer();
    },
  );

  /// The Options menu (the pull-down's own `AmalfiMenu`) and the glass region it declares.
  Finder optionsMenu() => find.byType(AmalfiMenu);

  testWidgets('Options: the pull-down menu is on Ossido glass, above or below', (
    tester,
  ) async {
    await pumpLayer(tester);
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    expect(find.byType(AmalfiPullDownButton), findsOneWidget);
    var button = tester.getRect(find.byKey(const ValueKey('options')));
    await tester.tap(find.byKey(const ValueKey('options')));
    await tester.pump(const Duration(milliseconds: 300));
    var menu = tester.getRect(optionsMenu());
    // No room below the bar (it sits at the bottom): 4 pt above the button, its left edge on
    // the button's, at least 200 wide (silica_ui's pull-down).
    expect(menu.left, button.left);
    expect(menu.bottom, button.top - AmalfiPopUpButtonMetrics.menuDistance);
    expect(menu.width, greaterThanOrEqualTo(200));
    // Thick glass of the `menu` kind with radius 12, declared by the menu itself; Flutter
    // paints no tint of its own.
    final glass = tester.widget<Glass>(
      find.descendant(of: optionsMenu(), matching: find.byType(Glass)).first,
    );
    expect(glass.profile, GlassProfile.menu);
    expect(glass.radius, 12);
    expect(glass.material, GlassMaterial.thick);
    expectGlassRegion(tester, 'menu');
    expectNoFlutterTint(tester);
    // Bar dragged to the top: there is room below, the menu opens below the button.
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 500));
    expect(optionsMenu(), findsNothing);
    final bar = tester.getRect(find.byType(ShotToolbar));
    await tester.dragFrom(
      bar.topLeft + const Offset(282, 24),
      const Offset(0, -700),
    );
    await tester.pump();
    button = tester.getRect(find.byKey(const ValueKey('options')));
    await tester.tap(find.byKey(const ValueKey('options')));
    await tester.pump(const Duration(milliseconds: 300));
    menu = tester.getRect(optionsMenu());
    expect(menu.top, button.bottom + AmalfiPopUpButtonMetrics.menuDistance);
  });

  testWidgets(
    'Options: a press on the selection (or its handles) closes the menu',
    (tester) async {
      await pumpLayer(tester);
      await send(tester, 'command', 'screenshot.toolbar');
      await tester.pumpAndSettle();
      for (final press in const [
        Offset(640, 300), // inside the selection
        Offset(100, 100), // outside it: a new one
        Offset(320, 150), // its top left handle
      ]) {
        await tester.tap(find.byKey(const ValueKey('options')));
        await tester.pump(const Duration(milliseconds: 300));
        expect(optionsMenu(), findsOneWidget);
        // Going back to editing: the press closes the menu (and edits the selection as usual).
        final gesture = await tester.startGesture(press);
        await tester.pump(const Duration(milliseconds: 300));
        await gesture.up();
        await tester.pump(const Duration(milliseconds: 500));
        expect(optionsMenu(), findsNothing, reason: 'press at $press');
      }
    },
  );

  testWidgets(
    'window mode: the camera is a glass of its own and hides over the bar and the menu',
    (tester) async {
      await pumpLayer(tester);
      await send(tester, 'command', 'screenshot.toolbar');
      await tester.pumpAndSettle();
      await tester.tapAt(modeAt(tester, 1));
      await tester.pump(const Duration(milliseconds: 600));
      final camera = find.byKey(const ValueKey('camera'));
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: const Offset(300, 200));
      await mouse.moveTo(const Offset(300, 200));
      await tester.pump();
      expect(camera, findsOneWidget);

      // Never merged with the bar: another glass group (Ossido merges within a group).
      int groupOf(Finder within) => tester
          .widget<ShadowedGlass>(
            find
                .descendant(of: within, matching: find.byType(ShadowedGlass))
                .first,
          )
          .group;
      expect(groupOf(camera), ShotGlass.camera);
      expect(groupOf(find.byType(ShotToolbar)), ShotGlass.bar);
      expect(ShotGlass.camera, lessThan(ShotGlass.bar));

      // On the bar: no camera.
      final bar = tester.getRect(find.byType(ShotToolbar));
      await mouse.moveTo(bar.center);
      await tester.pump();
      expect(camera, findsNothing);
      await mouse.moveTo(const Offset(300, 200));
      await tester.pump();
      expect(camera, findsOneWidget);

      // On the open Options menu: no camera either; off it, the camera is back.
      await tester.tap(find.byKey(const ValueKey('options')));
      await tester.pump(const Duration(milliseconds: 300));
      final menu = tester.getRect(optionsMenu());
      await mouse.moveTo(menu.center);
      await tester.pump();
      expect(camera, findsNothing);
      await mouse.moveTo(const Offset(300, 200));
      await tester.pump();
      expect(camera, findsOneWidget);
      await mouse.removePointer();
    },
  );

  testWidgets(
    'Options over the selection: the dim, the border and the handles leave the menu bare',
    (tester) async {
      await pumpLayer(tester);
      await send(tester, 'command', 'screenshot.toolbar');
      await tester.pumpAndSettle();
      List<RRect> holes() =>
          tester.widget<SelectionEditor>(find.byType(SelectionEditor)).holes;
      expect(holes(), isEmpty);
      await tester.tap(find.byKey(const ValueKey('options')));
      await tester.pump(const Duration(milliseconds: 300));
      await tester.pump();
      // The menu's glass is the compositor's, behind this layer: nothing this layer paints
      // may cover it, or the dim would darken it and the selection's border cut it.
      final menu = tester.getRect(optionsMenu());
      expect(holes().single.outerRect, menu);
      expect(holes().single.tlRadius.x, AmalfiMenuMetrics.radius);
      // Closed: the dim is whole again.
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pump(const Duration(milliseconds: 500));
      await tester.pump();
      expect(holes(), isEmpty);
    },
  );

  testWidgets('⇧⌘5 selection: adjusted, remembered, captured', (tester) async {
    await pumpLayer(tester);
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    // The default mode is the selection: half the screen, centered.
    expect(find.byKey(const ValueKey('selection editor')), findsOneWidget);
    // Drag its inside: it moves.
    await tester.dragFrom(const Offset(640, 300), const Offset(-100, -50));
    await tester.pump();
    expect(savedSettings().selection, const Rect.fromLTWH(220, 150, 640, 400));
    // A handle: the bottom-right corner grows.
    await tester.dragFrom(const Offset(860, 550), const Offset(20, 10));
    await tester.pump();
    expect(savedSettings().selection, const Rect.fromLTWH(220, 150, 660, 410));
    await tester.tap(find.byKey(const ValueKey('capture')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(services.lastCapture, {
      'x': 220.0,
      'y': 150.0,
      'width': 660.0,
      'height': 410.0,
      'pointer': false,
    });
    // Opened again: the same selection.
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('capture')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(services.lastCapture?['width'], 660.0);
  });

  testWidgets('⇧⌘5: the bar is moved by its background and stays there', (
    tester,
  ) async {
    await pumpLayer(tester);
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    final before = tester.getRect(find.byType(ShotToolbar));
    // Between the second separator and Options there is only background.
    await tester.dragFrom(
      before.topLeft + const Offset(282, 24),
      const Offset(-200, -300),
    );
    await tester.pump();
    final after = tester.getRect(find.byType(ShotToolbar));
    expect(after.topLeft, before.topLeft + const Offset(-200, -300));
    expect(savedSettings().barCenter, after.center);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    expect(tester.getRect(find.byType(ShotToolbar)).center, after.center);
  });

  testWidgets('Options: the menu above the button; every item works', (
    tester,
  ) async {
    await pumpLayer(
      tester,
      entries: {
        fileManagerId: const DesktopEntry(
          id: fileManagerId,
          name: 'Mustache',
          exec: 'mustache',
        ),
      },
    );
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    await tester.tapAt(modeAt(tester, 0));
    await tester.pump();
    final button = tester.getRect(find.byKey(const ValueKey('options')));
    Future<void> choose(String item) async {
      await tester.tap(find.byKey(const ValueKey('options')));
      await tester.pump();
      await tester.tap(find.text(item));
      await tester.pump(const Duration(milliseconds: 500));
      await tester.pump(const Duration(milliseconds: 500));
    }

    await tester.tap(find.byKey(const ValueKey('options')));
    await tester.pump();
    final menu = tester.getRect(optionsMenu());
    // Above the button, with its left edge on the button.
    expect(menu.bottom, lessThan(button.top));
    expect(menu.left, closeTo(button.left, 1));
    for (final item in [
      'Save to',
      'Desktop',
      'Documents',
      'Clipboard',
      'Other Folder…',
      'Timer',
      'None',
      '5 seconds',
      '10 seconds',
      'Show Floating Thumbnail',
      'Remember Last Selection',
      'Show Mouse Pointer',
    ]) {
      expect(find.text(item), findsOneWidget, reason: item);
    }
    // The capture-mode menu, in the measured order (recording is not available: #153).
    expect(menuLabels(tester), [
      'Save to',
      'Desktop',
      'Documents',
      'Clipboard',
      'Other Folder…',
      'Timer',
      'None',
      '5 seconds',
      '10 seconds',
      'Options',
      'Show Floating Thumbnail',
      'Remember Last Selection',
      'Capture',
      'Show Mouse Pointer',
    ]);
    // Esc closes the menu, the bar stays.
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.byType(AmalfiMenuList), findsNothing);
    expect(find.byKey(const ValueKey('options')), findsOneWidget);

    // The pointer: Ossido learns it, and the captures ask for it.
    await choose('Show Mouse Pointer');
    expect(savedSettings().showPointer, isTrue);
    expect(services.executed.last, 'screenshot.settings');
    expect(services.params.last, {'pointer': true});

    // Documents: the next capture goes there.
    await choose('Documents');
    expect(savedSettings().destination, const ToDocuments());

    // Other folder: chosen in the file manager, then first among the places.
    services.folder = dir.path;
    await choose('Other Folder…');
    expect(savedSettings().destination, ToFolder(dir.path));
    expect(find.byKey(const ValueKey('options')), findsOneWidget);
    await choose('Documents');

    // The timer: a countdown, then the capture.
    await choose('5 seconds');
    expect(savedSettings().timer, 5);
    await tester.tap(find.byKey(const ValueKey('capture')));
    await tester.pump();
    expect(find.byKey(const ValueKey('countdown')), findsOneWidget);
    expect(find.text('5'), findsOneWidget);
    await tester.pump(const Duration(seconds: 1));
    expect(find.text('4'), findsOneWidget);
    expect(services.captures, isEmpty);
    await tester.pump(const Duration(seconds: 5));
    await tester.pump(const Duration(milliseconds: 100));
    expect(services.captures, ['screenshot.screen']);
    expect(services.lastCapture, {'pointer': true});
    expect(find.byKey(const ValueKey('countdown')), findsNothing);

    // Without a thumbnail the file goes straight into its place.
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    await choose('Show Floating Thumbnail');
    expect(savedSettings().showThumbnail, isFalse);
    await choose('Remember Last Selection');
    expect(savedSettings().rememberSelection, isFalse);
    final temp = await announce(tester);
    expect(find.byKey(const ValueKey('thumbnail')), findsNothing);
    expect(temp.existsSync(), isFalse);
    expect(File('${docs.path}/$deskName').existsSync(), isTrue);
    // A second Esc closes the bar (it fades first).
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('options')), findsNothing);
  });

  testWidgets('to the clipboard: copied when the thumbnail leaves', (
    tester,
  ) async {
    store = SilicaSettings.withValues({'screenshot.destination': 'clipboard'});
    await pumpLayer(tester);
    final temp = await announce(tester);
    await tester.pump(const Duration(seconds: 6));
    await tester.pump(const Duration(milliseconds: 300));
    expect(services.copied, [temp.path]);
    expect(temp.existsSync(), isFalse);
    expect(desk.listSync(), isEmpty);
  });

  testWidgets('to the image viewer: opened with it from the cache', (
    tester,
  ) async {
    final config = Directory('${dir.path}/config')..createSync();
    File(
      '${config.path}/mimeapps.list',
    ).writeAsStringSync('[Default Applications]\nimage/png=viewer.desktop\n');
    store = SilicaSettings.withValues({'screenshot.destination': 'app:viewer'});
    await pumpLayer(
      tester,
      env: {'HOME': dir.path, 'XDG_CONFIG_HOME': config.path},
      entries: {
        'viewer': const DesktopEntry(
          id: 'viewer',
          name: 'Viewer',
          exec: 'viewer',
          rawExec: 'viewer %U',
        ),
      },
    );
    await send(tester, 'command', 'screenshot.toolbar');
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('options')));
    await tester.pump();
    expect(find.text('Viewer'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 500));
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    await announce(tester);
    await tester.pump(const Duration(seconds: 6));
    await tester.pump(const Duration(milliseconds: 300));
    final cached = '${dir.path}/cache/$deskName';
    expect(File(cached).existsSync(), isTrue);
    expect(services.launched.single, "viewer '${Uri.file(cached)}'");
  });
}
