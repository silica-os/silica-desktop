import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart' show Locale;
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_ui/silica_ui.dart'
    show SilicaDBus, SilicaInstalledApp, SilicaInstalledAppKind;
import 'package:silica_shell/l10n/app_localizations.dart';
import 'package:silica_shell/dock/apps.dart';
import 'package:silica_shell/dock/model.dart';
import 'package:silica_shell/dock/trash_alert.dart';
import 'package:silica_shell/system/desktop_entries.dart';
import 'package:silica_shell/system/shell_channel.dart';

const String filesEntry = '''
[Desktop Entry]
Name=Files
Name[it]=File
Exec=nautilus --new-window %U
Icon=org.gnome.Nautilus
Type=Application
StartupWMClass=org.gnome.Nautilus
Keywords=folder;manager;

[Desktop Action new-window]
Name=New window
Exec=nautilus --new-window
''';

/// The Downloads folder of the Dock is shown with its name on disk.
String downloadsName() =>
    downloadDir(Platform.environment['HOME'] ?? '').split('/').last;

void main() {
  liveEntriesTests();
  test(
    'a .desktop entry: localized name, Exec without codes, actions ignored',
    () {
      final entry = parseDesktopEntry('org.gnome.Nautilus', filesEntry)!;
      expect(entry.name, 'Files');
      // The name in the system language.
      expect(
        parseDesktopEntry('org.gnome.Nautilus', filesEntry, locale: 'it')!.name,
        'File',
      );
      expect(entry.exec, 'nautilus --new-window');
      expect(entry.icon, 'org.gnome.Nautilus');
      expect(entry.keywords, ['folder', 'manager']);
      expect(entry.matchesAppId('org.gnome.Nautilus'), isTrue);
      expect(entry.matchesAppId('nautilus'), isTrue);
      expect(entry.matchesAppId('firefox'), isFalse);
    },
  );

  test('hidden entries or entries without Exec are not apps', () {
    expect(
      parseDesktopEntry(
        'x',
        '[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\nExec=x',
      ),
      isNull,
    );
    expect(
      parseDesktopEntry('x', '[Desktop Entry]\nType=Application\nName=X'),
      isNull,
    );
    expect(
      parseDesktopEntry('x', '[Desktop Entry]\nType=Link\nName=X\nExec=x'),
      isNull,
    );
    expect(stripFieldCodes('app %f --x %% %U'), 'app --x %');
  });

  test('XDG folders: the first one wins on equal ids', () {
    final root = Directory.systemTemp.createTempSync('silica-apps');
    addTearDown(() => root.deleteSync(recursive: true));
    final user = Directory('${root.path}/user/applications')
      ..createSync(recursive: true);
    final system = Directory('${root.path}/system/applications')
      ..createSync(recursive: true);
    File('${user.path}/editor.desktop').writeAsStringSync(
      '[Desktop Entry]\nType=Application\nName=Mio editor\nExec=editor',
    );
    File('${system.path}/editor.desktop').writeAsStringSync(
      '[Desktop Entry]\nType=Application\nName=Editor\nExec=editor',
    );
    File('${system.path}/calc.desktop').writeAsStringSync(
      '[Desktop Entry]\nType=Application\nName=Calcolatrice\nExec=calc',
    );
    final entries = loadDesktopEntries(
      env: {
        'HOME': root.path,
        'XDG_DATA_HOME': '${root.path}/user',
        'XDG_DATA_DIRS': '${root.path}/system',
      },
    );
    expect(entries.keys.toSet(), {'editor', 'calc'});
    expect(entries['editor']!.name, 'Mio editor');
  });

  test('Dock: pinned, then open unpinned, separator, folder and trash', () {
    const files = DesktopEntry(
      id: 'files',
      name: 'File',
      exec: 'files',
      wmClass: 'files',
    );
    const editor = DesktopEntry(id: 'editor', name: 'Editor', exec: 'editor');
    final entries = {'files': files, 'editor': editor};
    final items = dockItems(
      entries,
      ['files'],
      const [
        Toplevel(id: 'a', title: 'Documento', appId: 'editor'),
        Toplevel(id: 'b', title: 'Casa', appId: 'files'),
      ],
    );
    expect(
      [for (final i in items) i.label],
      ['File', 'Editor', '', downloadsName(), 'Trash'],
    );
    expect(items[0].running, isTrue);
    expect(items[0].windows, ['b']);
    expect(items[1].windows, ['a']);
    expect(items[2].kind, ItemKind.separator);
  });

  test('minimized windows go into their app icon with minimizeToApp', () {
    const editor = DesktopEntry(id: 'editor', name: 'Editor', exec: 'editor');
    const windows = [Toplevel(id: 'a', title: 'Documento', appId: 'editor')];
    List<String> labels(bool toApp) => [
      for (final i in dockItems(
        {'editor': editor},
        const [],
        windows,
        minimized: const ['a'],
        minimizeToApp: toApp,
      ))
        i.label,
    ];
    // Separate section: the window's own cell before the Trash.
    expect(labels(false), [
      'Editor',
      '',
      downloadsName(),
      'Documento',
      'Trash',
    ]);
    expect(labels(true), ['Editor', '', downloadsName(), 'Trash']);
  });

  test(
    'recent apps (D-08): second separator, open unpinned and closed, at most 3',
    () {
      DesktopEntry app(String id) => DesktopEntry(id: id, name: id, exec: id);
      final entries = {
        for (final id in ['files', 'editor', 'calc', 'mail', 'foto'])
          id: app(id),
      };
      final items = dockItems(
        entries,
        ['files'],
        const [Toplevel(id: 'a', title: 'Documento', appId: 'editor')],
        recents: ['calc', 'mail', 'foto'],
      );
      expect(
        [for (final i in items) i.label],
        ['files', '', 'editor', 'calc', 'mail', '', downloadsName(), 'Trash'],
      );
      expect(items[3].running, isFalse);
      // Without the option: a single separator, the closed ones are not there.
      final off = dockItems(entries, ['files'], const []);
      expect(
        [for (final i in off) i.label],
        ['files', '', downloadsName(), 'Trash'],
      );
    },
  );

  test('the file manager is first and fixed, whatever the configuration', () {
    const mustache = DesktopEntry(
      id: fileManagerId,
      name: 'Mustache',
      exec: 'mustache',
    );
    const editor = DesktopEntry(id: 'editor', name: 'Editor', exec: 'editor');
    final entries = {fileManagerId: mustache, 'editor': editor};
    // A configuration that lists it later, or not at all.
    for (final pinned in [
      ['editor', fileManagerId],
      ['editor'],
    ]) {
      final items = dockItems(entries, pinned, const []);
      expect([for (final i in items) i.label].take(2), ['Mustache', 'Editor']);
      expect(items.first.fixed, isTrue);
      expect(items.first.pinned, isTrue);
      expect(items[1].fixed, isFalse);
    }
  });

  test('the Apps item follows the file manager and opens the Launcher', () {
    const mustache = DesktopEntry(
      id: fileManagerId,
      name: 'Mustache',
      exec: 'mustache',
    );
    final entries = {fileManagerId: mustache};
    final dir = Directory.systemTemp.createTempSync('dock');
    addTearDown(() => dir.deleteSync(recursive: true));
    final pinned = pinnedIds(entries, env: {'HOME': dir.path});
    expect(pinned.take(2), [fileManagerId, appsItemId]);
    final items = dockItems(entries, pinned, const [], appsLabel: 'Apps');
    final apps = items[1];
    expect(apps.label, 'Apps');
    expect(apps.action, 'launcher.toggle');
    expect(apps.command, isNull);
    expect(apps.pinned, isTrue);
    expect(apps.fixed, isFalse);
    // Removed from the Dock: it is not there.
    final without = dockItems(entries, [fileManagerId], const []);
    expect(without.where((i) => i.id == appsItemId), isEmpty);
  });

  test('the Trash opens the file manager on the Trash', () {
    const mustache = DesktopEntry(
      id: fileManagerId,
      name: 'Mustache',
      exec: '/usr/bin/mustache',
    );
    // The Trash folder, created first (the file manager shows it as the Trash).
    expect(
      trashCommand({fileManagerId: mustache}, env: {'HOME': '/home/a b'}),
      "mkdir -p '/home/a b/.local/share/Trash/files' && "
      "/usr/bin/mustache '/home/a b/.local/share/Trash/files'",
    );
    expect(
      trashFilesDir(env: {'HOME': '/h', 'XDG_DATA_HOME': '/data'}),
      '/data/Trash/files',
    );
    expect(trashCommand(const {}), 'xdg-open trash:///');
    final items = dockItems({fileManagerId: mustache}, const [], const []);
    expect(items.last.trash, isTrue);
    expect(items.last.command, contains('/usr/bin/mustache '));
  });

  test('the pinned apps and the folders keep each other', () {
    final dir = Directory.systemTemp.createTempSync('dock');
    addTearDown(() => dir.deleteSync(recursive: true));
    final env = {'HOME': dir.path};
    savePinned(['files', 'editor'], env: env);
    saveDockOption('folders', ['/tmp'], env: env);
    expect(readDockConfig(env: env)['pinned'], ['files', 'editor']);
    expect(readDockConfig(env: env)['folders'], ['/tmp']);
  });

  test(
    'Exec with files: %f, %F, %u, %U quoted; without codes it takes none',
    () {
      expect(
        execWithFiles('testo %F', ['/tmp/a.txt', "/tmp/l'altro.txt"]),
        "testo '/tmp/a.txt' '/tmp/l'\\''altro.txt'",
      );
      expect(
        execWithFiles('apri %f --nuovo', ['/a', '/b']),
        "apri '/a' --nuovo",
      );
      expect(execWithFiles('web %U', ['/x y']), "web 'file:///x%20y'");
      expect(execWithFiles('web %u %i', ['/x']), "web 'file:///x'");
      expect(execWithFiles('calcolatrice', ['/a']), isNull);
      expect(execWithFiles('cento %%F', ['/a']), isNull);
      expect(execWithFiles('testo %F', []), isNull);
    },
  );

  test('into the Trash by hand: .trashinfo file, equal names numbered', () async {
    final home = Directory.systemTemp.createTempSync('cestino');
    addTearDown(() => home.deleteSync(recursive: true));
    final a = File('${home.path}/a b.txt')..writeAsStringSync('1');
    await moveToTrash(
      [a.path],
      env: {'HOME': home.path},
      now: DateTime(2026, 10, 14, 9, 30),
    );
    final trash = '${home.path}/.local/share/Trash';
    expect(a.existsSync(), isFalse);
    expect(File('$trash/files/a b.txt').readAsStringSync(), '1');
    expect(
      File('$trash/info/a b.txt.trashinfo').readAsStringSync(),
      '[Trash Info]\nPath=${Uri.file(a.path).path}\nDeletionDate=2026-10-14T09:30:00\n',
    );
    File(a.path).writeAsStringSync('2');
    await moveToTrash([a.path], env: {'HOME': home.path});
    expect(File('$trash/files/a b.txt 2').readAsStringSync(), '2');
  });

  test('folders right of the separator come from the configuration', () {
    final dir = Directory.systemTemp.createTempSync('folders');
    addTearDown(() => dir.deleteSync(recursive: true));
    final items = dockItems(
      const {},
      const [],
      const [],
      folders: ['${dir.path}/Projects', '${dir.path}/Music'],
    );
    final folders = [
      for (final i in items)
        if (i.folder != null) i.label,
    ];
    expect(folders, ['Projects', 'Music']);
    final separator = items.indexWhere((i) => i.kind == ItemKind.separator);
    expect(items[separator + 1].label, 'Projects');
    // Opened without xdg-open (not on Silica OS).
    expect(items[separator + 1].command, contains('gio open'));
  });

  test('the file manager menu places: six, sorted, from user-dirs', () {
    final home = Directory.systemTemp.createTempSync('places');
    addTearDown(() => home.deleteSync(recursive: true));
    Directory('${home.path}/Docs').createSync();
    Directory('${home.path}/Downloads').createSync();
    Directory('${home.path}/Desktop').createSync();
    Directory('${home.path}/.config').createSync();
    File(
      '${home.path}/.config/user-dirs.dirs',
    ).writeAsStringSync('XDG_DOCUMENTS_DIR="\$HOME/Docs"\n');
    final places = fileManagerPlaces(
      home.path,
      lookupAppLocalizations(const Locale('en')),
      env: {'HOME': home.path},
    );
    final user = home.path.split('/').last;
    final labels = [for (final (label, _) in places) label];
    expect(labels.length, 6);
    expect(
      labels,
      [...labels]..sort((a, b) => a.toLowerCase().compareTo(b.toLowerCase())),
    );
    final byLabel = {for (final (label, path) in places) label: path};
    expect(byLabel['Documents'], '${home.path}/Docs');
    expect(byLabel['Downloads'], '${home.path}/Downloads');
    expect(byLabel['Desktop'], '${home.path}/Desktop');
    expect(byLabel[user], home.path);
    // Mustache has no Recents view yet: shown disabled.
    expect(byLabel['Recents'], isNull);
  });
}

const String firefoxEntry = '''
[Desktop Entry]
Name=Firefox
Exec=flatpak run org.mozilla.firefox %u
Icon=org.mozilla.firefox
Type=Application
''';

void liveEntriesTests() {
  test('Flatpak exports are searched even when XDG_DATA_DIRS lacks them', () {
    final dirs = appDataDirs({
      'HOME': '/home/u',
      'XDG_DATA_DIRS': '/usr/share:/var/lib/flatpak/exports/share/',
    });
    expect(dirs, contains('/home/u/.local/share/flatpak/exports/share'));
    expect(dirs.where((d) => d.startsWith('/var/lib/flatpak')), hasLength(1));
  });

  test(
    'an app installed while the store runs appears, and removed goes',
    () async {
      final root = Directory.systemTemp.createTempSync('silica-entries');
      addTearDown(() => root.deleteSync(recursive: true));
      final apps = Directory('${root.path}/data/applications')
        ..createSync(recursive: true);
      final store = DesktopEntryStore(
        env: {
          'HOME': root.path,
          'XDG_DATA_HOME': '${root.path}/none',
          'XDG_DATA_DIRS': '${root.path}/data',
        },
        debounce: const Duration(milliseconds: 20),
        rescan: const Duration(milliseconds: 50),
      );
      addTearDown(store.dispose);
      expect(store.entries, isEmpty);
      final changed = Completer<void>();
      store.addListener(() {
        if (!changed.isCompleted) changed.complete();
      });
      File(
        '${apps.path}/org.mozilla.firefox.desktop',
      ).writeAsStringSync(firefoxEntry);
      await changed.future.timeout(const Duration(seconds: 5));
      expect(store.entries['org.mozilla.firefox']?.name, 'Firefox');
      expect(
        store.entries['org.mozilla.firefox']!.matchesAppId(
          'org.mozilla.firefox',
        ),
        isTrue,
      );
    },
  );

  test('the registry\'s apps become entries; hidden ones are left out', () {
    final entries = entriesFromApps([
      const SilicaInstalledApp(
        id: 'dev.example.Viewer',
        name: 'Viewer',
        names: {'it': 'Visore'},
        kind: SilicaInstalledAppKind.bundle,
        path: '/Applications/Viewer.app',
        exec: 'silica-launch /Applications/Viewer.app %U',
        silicaIcon: '/Applications/Viewer.app/Contents/Resources/icon',
        wmClass: 'dev.example.Viewer',
        isNew: true,
      ),
      const SilicaInstalledApp(
        id: 'helper',
        name: 'Helper',
        exec: 'helper',
        noDisplay: true,
      ),
    ], locale: 'it');
    expect(entries.keys, ['dev.example.Viewer']);
    final viewer = entries['dev.example.Viewer']!;
    expect(viewer.name, 'Visore');
    expect(viewer.exec, 'silica-launch /Applications/Viewer.app');
    expect(viewer.rawExec, 'silica-launch /Applications/Viewer.app %U');
    expect(viewer.kind, 'bundle');
    expect(viewer.isNew, isTrue);
    expect(viewer.matchesAppId('dev.example.Viewer'), isTrue);
    expect(
      execWithFiles(viewer.rawExec!, ['/tmp/a b']),
      "silica-launch /Applications/Viewer.app 'file:///tmp/a%20b'",
    );
  });

  test(
    'with the registry running, the store follows it and its signals',
    () async {
      TestWidgetsFlutterBinding.ensureInitialized();
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      var apps = [
        {'id': 'a.One', 'name': 'One', 'kind': 'system', 'exec': 'one'},
      ];
      messenger.setMockMethodCallHandler(SilicaDBus.channel, (call) async {
        if (call.method == 'subscribe') return 1;
        if (call.method == 'unsubscribe') return true;
        final args = call.arguments as Map;
        if (args['method'] == 'List') {
          return [
            [
              for (final a in apps) [a['id'], a['name'], a['kind'], '', a],
            ],
          ];
        }
        return const [];
      });
      addTearDown(
        () => messenger.setMockMethodCallHandler(SilicaDBus.channel, null),
      );
      final root = Directory.systemTemp.createTempSync('silica-service');
      addTearDown(() => root.deleteSync(recursive: true));
      final store = DesktopEntryStore(
        env: {'HOME': root.path, 'XDG_DATA_DIRS': '${root.path}/none'},
        rescan: Duration.zero,
        service: true,
      );
      addTearDown(store.dispose);
      var changed = Completer<void>();
      store.addListener(() {
        if (!changed.isCompleted) changed.complete();
      });
      await changed.future.timeout(const Duration(seconds: 5));
      expect(store.fromService, isTrue);
      expect(store.entries.keys, ['a.One']);
      // An Added signal: the store reads the list again.
      apps = [
        ...apps,
        {'id': 'b.Two', 'name': 'Two', 'kind': 'bundle', 'exec': 'two'},
      ];
      changed = Completer<void>();
      await messenger.handlePlatformMessage(
        'silica/dbus',
        const JSONMethodCodec().encodeMethodCall(
          const MethodCall('signal', {
            'id': 1,
            'sender': ':1.5',
            'path': '/dev/frascella/Silica/Apps1',
            'interface': 'dev.frascella.Silica.Apps1',
            'member': 'Added',
            'args': ['b.Two'],
          }),
        ),
        (_) {},
      );
      await changed.future.timeout(const Duration(seconds: 5));
      expect(store.entries.keys.toSet(), {'a.One', 'b.Two'});
      expect(store.entries['b.Two']!.kind, 'bundle');
    },
  );

  test('a running app that is not pinned gets a Dock item with its entry', () {
    final firefox = parseDesktopEntry('org.mozilla.firefox', firefoxEntry)!;
    final items = dockItems(
      {'org.mozilla.firefox': firefox},
      const [],
      [
        const Toplevel(
          id: '1',
          title: 'Mozilla Firefox',
          appId: 'org.mozilla.firefox',
        ),
      ],
    );
    final item = items.firstWhere((i) => i.id == 'org.mozilla.firefox');
    expect(item.label, 'Firefox');
    expect(item.running, isTrue);
    expect(item.pinned, isFalse);
  });
}
