import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/wallpaper/amalfi_wallpapers.dart';
import 'package:silica_shell/wallpaper/wallpaper.dart';
import 'package:silica_ui/silica_ui.dart';

import 'support/settings.dart';

/// An environment without the Amalfi set (the developer's machine may have it).
const Map<String, String> noSet = {
  'HOME': '/nonexistent',
  'XDG_DATA_DIRS': '/nonexistent',
};

void main() {
  test('the wallpaper choice from the settings', () {
    expect(chooseWallpaper(env: noSet), isA<GeneratedWallpaper>());
    final color = chooseWallpaper(color: '#7a7b80', env: noSet);
    expect((color as ColorWallpaper).color, const Color(0xFF7A7B80));
    // A colour that is not one: the default.
    expect(
      chooseWallpaper(color: 'red', env: noSet),
      isA<GeneratedWallpaper>(),
    );
    // An image that does not exist: the default.
    expect(
      chooseWallpaper(image: '/non/esiste.png', env: noSet),
      isA<GeneratedWallpaper>(),
    );
    final file = File(
      '${Directory.systemTemp.createTempSync().path}/sfondo.png',
    )..writeAsBytesSync([0]);
    expect(
      chooseWallpaper(image: file.path, color: '#101010', env: noSet),
      isA<ImageWallpaper>(),
    );
  });

  testWidgets('the generated wallpaper draws', (tester) async {
    await tester.pumpWidget(const Wallpaper(choice: GeneratedWallpaper()));
    expect(find.byType(CustomPaint), findsWidgets);
  });

  test('the Silica wallpaper follows the appearance', () {
    const light = SilicaWallpaperPainter(dark: false);
    const dark = SilicaWallpaperPainter(dark: true);
    expect(light.shouldRepaint(dark), isTrue);
    expect(
      light.shouldRepaint(const SilicaWallpaperPainter(dark: false)),
      isFalse,
    );
  });

  group('the Amalfi set', () {
    late Directory root;
    late Map<String, String> env;
    setUp(() {
      root = Directory.systemTemp.createTempSync('silica-wallpapers');
      final set = Directory('${root.path}/amalfi')..createSync();
      for (final id in ['positano-1', 'ravello']) {
        Directory('${set.path}/$id').createSync();
        File('${set.path}/$id/light.png').writeAsBytesSync([1]);
        File('${set.path}/$id/dark.png').writeAsBytesSync([2]);
      }
      Directory('${set.path}/previews').createSync();
      File('${set.path}/previews/ravello-light.jpg').writeAsBytesSync([3]);
      File('${set.path}/wallpapers.json').writeAsStringSync(
        '{"wallpapers":['
        '{"id":"positano-1","title":"Positano 1","variants":{"light":{"file":"positano-1/light.png"},"dark":{"file":"positano-1/dark.png"}}},'
        '{"id":"ravello","title":"Ravello","variants":{"light":{"file":"ravello/light.png"},"dark":{"file":"ravello/dark.png"}}}]}',
      );
      env = {...noSet, 'SILICA_WALLPAPERS': root.path};
    });
    tearDown(() => root.deleteSync(recursive: true));

    test('lists ids, titles, images and previews', () {
      final list = loadAmalfiWallpapers(env: env);
      expect(list.map((w) => w.id), ['positano-1', 'ravello']);
      expect(list.last.title, 'Ravello');
      expect(list.last.lightPreview, endsWith('previews/ravello-light.jpg'));
      expect(list.last.darkPreview, isNull);
      expect(list.last.image(dark: true), endsWith('ravello/dark.png'));
    });

    test(
      'Ravello is the default; a choice by id; an unknown id falls back',
      () {
        WallpaperChoice choose({String id = '', String color = ''}) =>
            chooseWallpaper(id: id, color: color, env: env);
        expect((choose() as AmalfiWallpaper).info.id, 'ravello');
        expect(
          (choose(id: 'positano-1') as AmalfiWallpaper).info.id,
          'positano-1',
        );
        expect((choose(id: 'nope') as AmalfiWallpaper).info.id, 'ravello');
        // Colours and images of the user still win when chosen.
        expect(choose(color: '#101010'), isA<ColorWallpaper>());
      },
    );

    testWidgets('light and dark images, aspect-filled and centred', (
      tester,
    ) async {
      final choice = chooseWallpaper(env: env);
      for (final brightness in [Brightness.light, Brightness.dark]) {
        await tester.pumpWidget(
          MediaQuery(
            data: MediaQueryData(platformBrightness: brightness),
            child: Directionality(
              textDirection: TextDirection.ltr,
              child: Wallpaper(choice: choice),
            ),
          ),
        );
        final image = tester.widget<Image>(find.byType(Image));
        expect(image.fit, BoxFit.cover);
        expect(image.alignment, Alignment.center);
        expect(
          ((image.image as FileImage).file.path),
          endsWith(brightness == Brightness.dark ? 'dark.png' : 'light.png'),
        );
      }
    });
  });

  test(
    'wallpaper.fit: fill by default, the five modes, junk falls back',
    () async {
      final backend = await useSettings();
      final settings = SilicaSettings.instance;
      expect(wallpaperFitOf(settings), WallpaperFit.fill);
      for (final fit in WallpaperFit.values) {
        backend.emit('wallpaper.fit', fit.name);
        expect(wallpaperFitOf(settings), fit);
      }
      backend.emit('wallpaper.fit', 'zoom');
      expect(wallpaperFitOf(settings), WallpaperFit.fill);
    },
  );

  testWidgets('an image is drawn as the fit says', (tester) async {
    Future<Image> pumpFit(WallpaperFit fit) async {
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Wallpaper(choice: const ImageWallpaper('/none.png'), fit: fit),
        ),
      );
      return tester.widget<Image>(find.byType(Image));
    }

    expect((await pumpFit(WallpaperFit.fill)).fit, BoxFit.cover);
    expect((await pumpFit(WallpaperFit.fit)).fit, BoxFit.contain);
    expect((await pumpFit(WallpaperFit.stretch)).fit, BoxFit.fill);
    expect((await pumpFit(WallpaperFit.center)).fit, BoxFit.none);
    final tiled = await pumpFit(WallpaperFit.tile);
    expect(tiled.repeat, ImageRepeat.repeat);
  });

  testWidgets('the layer draws again when Settings changes the wallpaper', (
    tester,
  ) async {
    final backend = await useSettings();
    await tester.pumpWidget(
      const Directionality(
        textDirection: TextDirection.ltr,
        child: WallpaperLayer(env: noSet),
      ),
    );
    expect(find.byType(CustomPaint), findsWidgets);
    backend.emit('wallpaper.color', '#102030');
    backend.emit('wallpaper.fit', 'tile');
    await tester.pump();
    final box = tester.widget<ColoredBox>(find.byType(ColoredBox).first);
    expect(box.color, const Color(0xFF102030));
    // A set from the app itself shows at once too.
    await SilicaSettings.instance.set('wallpaper.color', '#203040');
    await tester.pump();
    expect(
      tester.widget<ColoredBox>(find.byType(ColoredBox).first).color,
      const Color(0xFF203040),
    );
    await tester.pumpWidget(const SizedBox());
  });
}
