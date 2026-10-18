// The frontmost app's own menus in the bar (silica-menu-v1, #25).

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_shell/l10n/l10n.dart';
import 'package:silica_shell/menubar/app_menus.dart';
import 'package:silica_shell/menubar/menubar.dart';
import 'package:silica_shell/system/app_menu_model.dart';
import 'package:silica_shell/system/shell_channel.dart';
import 'package:silica_ui/silica_ui.dart'
    show MenuAction, MenuDivider, MenuEntry;

/// The model as Ossido hands it over: Calculator's menus.
Map<String, Object?> calculatorModel({String scientific = 'Scientific'}) => {
  'menus': [
    {
      'label': 'Calculator',
      'role': 'app',
      'items': [
        {'id': 1, 'label': 'About Calculator', 'role': 'about'},
        {'separator': true},
        {'role': 'services', 'enabled': true},
        {'separator': true},
        {'role': 'hide', 'enabled': true},
        {'role': 'hide_others', 'enabled': true},
        {'role': 'show_all', 'enabled': true},
        {'separator': true},
        {'role': 'quit', 'enabled': true},
      ],
    },
    {
      'label': 'Edit',
      'items': [
        {'role': 'undo', 'enabled': false},
        {'label': 'Note', 'enabled': true},
        {'separator': true},
        {
          'id': 2,
          'label': 'Copy',
          'enabled': true,
          'shortcut': {'key': 'C', 'meta': true},
        },
      ],
    },
    {
      'label': 'View',
      'items': [
        {
          'id': 3,
          'label': 'Basic',
          'enabled': true,
          'checked': true,
          'shortcut': {'key': '1', 'meta': true},
        },
        {
          'id': 4,
          'label': scientific,
          'enabled': true,
          'shortcut': {'key': '2', 'meta': true},
        },
        {
          'label': 'Decimal Places',
          'enabled': true,
          'items': [
            {'id': 5, 'label': '0', 'enabled': true},
            {'id': 6, 'label': '8', 'enabled': true, 'checked': true},
          ],
        },
      ],
    },
    {'label': 'Empty', 'items': <Object?>[]},
    {
      'label': 'Window',
      'role': 'window',
      'items': [
        {
          'role': 'close',
          'enabled': true,
          'shortcut': {'key': 'W', 'meta': true},
        },
      ],
    },
    {'label': 'Help', 'role': 'help', 'items': <Object?>[]},
  ],
};

List<String> labels(List<MenuEntry> entries) => [
  for (final e in entries)
    if (e is MenuAction) e.label else if (e is MenuDivider) '—',
];

MenuAction action(List<MenuEntry> entries, String label) =>
    entries.whereType<MenuAction>().firstWhere((a) => a.label == label);

void main() {
  test('shortcuts are written ⌃⌥⇧⌘ then the key', () {
    expect(const MenuShortcut('m', meta: true, alt: true).text, '⌥⌘M');
    expect(
      const MenuShortcut('Left', ctrl: true, shift: true, meta: true).text,
      '⌃⇧⌘←',
    );
    expect(const MenuShortcut('Return').text, '↩');
    expect(const MenuShortcut('F5', alt: true).text, '⌥F5');
  });

  test('the model is read defensively', () {
    expect(AppMenuState.fromJson(null).hasMenus, isFalse);
    expect(AppMenuState.fromJson('junk').hasMenus, isFalse);
    final state = AppMenuState.fromJson({
      'app_id': 'org.silica.Calculator',
      'serial': 3,
      'model': {
        'menus': [
          7,
          {
            'label': 'View',
            'items': [
              'x',
              {'id': -1, 'label': 'Bad id', 'shortcut': 'no'},
              {
                'label': 'Sub',
                'items': [
                  {'id': 9, 'label': 'Deep'},
                ],
              },
            ],
          },
        ],
      },
    });
    expect(state.hasMenus, isTrue);
    expect(state.menus!.single.label, 'View');
    final items = state.menus!.single.items;
    expect(items.length, 2);
    expect(items.first.id, isNull);
    expect(items.first.shortcut, isNull);
    expect(items.last.items!.single.id, 9);
    // A model announced without a serial is no model.
    expect(
      AppMenuState.fromJson({'serial': 0, 'model': calculatorModel()}).hasMenus,
      isFalse,
    );
  });

  group('composed menus', () {
    final activated = <int>[];
    final ran = <String>[];
    final windows = <String>[];
    List<BarMenu> compose({
      bool systemHide = false,
      List<Toplevel> open = const [],
    }) => composeAppMenus(
      AppMenuState.fromJson({'serial': 1, 'model': calculatorModel()}).menus!,
      appName: 'Calculator',
      windows: open,
      focusedWindow: 'w2',
      actions: AppMenuActions(
        activate: activated.add,
        run: ran.add,
        activateWindow: windows.add,
        activateApp: (id) => windows.add('app:$id'),
        systemHide: systemHide,
      ),
    );

    setUp(() {
      activated.clear();
      ran.clear();
      windows.clear();
    });

    test('Close All takes its label and ⌥⌘W from the system (#177)', () {
      final menus = composeAppMenus(
        AppMenuState.fromJson({
          'serial': 1,
          'model': {
            'menus': [
              {
                'label': 'File',
                'items': [
                  {
                    'id': 9,
                    'label': '',
                    'role': 'close_all',
                    'enabled': true,
                    'shortcut': {'key': 'W', 'meta': true, 'alt': true},
                  },
                ],
              },
            ],
          },
        }).menus!,
        appName: 'Notes',
        windows: const [],
        focusedWindow: '',
        actions: AppMenuActions(activate: activated.add, run: ran.add),
      );
      final item = menus
          .firstWhere((m) => m.title == 'File')
          .entries
          .whereType<MenuAction>()
          .single;
      expect(item.label, englishTexts.closeAll);
      expect(item.enabled, isTrue);
      item.onSelected!();
      expect(activated, [9]);
    });

    test('the bar keeps the app order, drops empty menus, adds Help', () {
      final menus = compose();
      expect(menus.map((m) => m.title), [
        'Calculator',
        'Edit',
        'View',
        'Window',
        'Help',
      ]);
      expect(menus.first.app, isTrue);
    });

    test('the app menu: the app items, Services, Quit run by the system', () {
      final app = compose().first.entries;
      // Hide, Hide Others and Show All need Ossido's hiding: left out without it.
      expect(labels(app), [
        'About Calculator',
        '—',
        englishTexts.services,
        '—',
        englishTexts.quitApp('Calculator'),
      ]);
      action(app, 'About Calculator').onSelected!();
      expect(activated, [1]);
      final services = action(app, englishTexts.services).submenu!;
      expect(action(services, englishTexts.noServices).enabled, isFalse);
      final quit = action(app, englishTexts.quitApp('Calculator'));
      expect(quit.shortcut, '⌘Q');
      quit.onSelected!();
      expect(ran, ['app.quit']);
      final hiding = compose(systemHide: true).first.entries;
      expect(action(hiding, englishTexts.hideApp('Calculator')).shortcut, '⌘H');
      action(hiding, englishTexts.hideOthers).onSelected!();
      expect(ran.last, 'app.hide_others');
    });

    test('items: shortcuts, checkmarks, disabled, submenus', () {
      final menus = compose();
      final edit = menus[1].entries;
      // A system-labelled edit item with nothing to act on, and an item without an id: both
      // disabled, never dead.
      expect(labels(edit), [englishTexts.editUndo, 'Note', '—', 'Copy']);
      expect(action(edit, englishTexts.editUndo).enabled, isFalse);
      expect(action(edit, englishTexts.editUndo).shortcut, '⌘Z');
      expect(action(edit, 'Note').enabled, isFalse);
      expect(action(edit, 'Copy').shortcut, '⌘C');
      final view = menus[2].entries;
      expect(action(view, 'Basic').checked, isTrue);
      expect(action(view, 'Scientific').checked, isFalse);
      final places = action(view, 'Decimal Places').submenu!;
      expect(action(places, '8').checked, isTrue);
      action(places, '0').onSelected!();
      expect(activated, [5]);
    });

    test(
      'the Window menu: the system part, then the app items, then the windows',
      () {
        final window = compose(
          open: const [
            Toplevel(id: 'w1', title: 'Calculator', appId: 'calc'),
            Toplevel(id: 'w2', title: '', appId: 'calc'),
          ],
        )[3].entries;
        expect(labels(window), [
          englishTexts.minimize,
          englishTexts.fill,
          '—',
          englishTexts.moveAndResize,
          englishTexts.fullScreen,
          '—',
          englishTexts.closeWindow,
          '—',
          englishTexts.bringAllToFront,
          '—',
          'Calculator',
          'Calculator',
        ]);
        action(window, englishTexts.closeWindow).onSelected!();
        action(window, englishTexts.minimize).onSelected!();
        action(
          action(window, englishTexts.moveAndResize).submenu!,
          englishTexts.tileLeft,
        ).onSelected!();
        expect(ran, ['window.close', 'window.minimize', 'window.tile_left']);
        // The focused window is checked; choosing a window activates it.
        final list = window.whereType<MenuAction>().toList().sublist(6);
        expect(list.map((a) => a.checked), [false, true]);
        list.first.onSelected!();
        action(window, englishTexts.bringAllToFront).onSelected!();
        // One window alone; Bring All to Front the whole app (silica-ui#101).
        expect(windows, ['w1', 'app:w2']);
      },
    );

    test('Help without the app help: Silica Help', () {
      final help = compose().last.entries;
      expect(labels(help), [englishTexts.silicaHelp]);
    });

    test('an app without a Window menu gets one before Help', () {
      final menus = composeAppMenus(
        const [
          AppMenu(
            label: 'Notes',
            items: [AppMenuItem(id: 1, label: 'New')],
          ),
          AppMenu(
            label: 'Help',
            role: 'help',
            items: [AppMenuItem(id: 2, role: 'help')],
          ),
        ],
        appName: 'Notes',
        actions: AppMenuActions(activate: (_) {}, run: (_) {}),
      );
      expect(menus.map((m) => m.title), ['Notes', 'Notes', 'Window', 'Help']);
      // Without an app menu the bar still offers Quit; a help item takes the app's name.
      expect(labels(menus.first.entries), [englishTexts.quitApp('Notes')]);
      expect(labels(menus.last.entries), [englishTexts.appHelp('Notes')]);
    });
  });

  group('the bar', () {
    late List<MethodCall> calls;
    const codec = JSONMethodCodec();

    Future<void> pumpBar(WidgetTester tester) async {
      tester.view.physicalSize = const Size(1280, 600);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      calls = [];
      tester.binding.defaultBinaryMessenger
        ..setMockMethodCallHandler(ShellChannel.channel, (call) async {
          calls.add(call);
          return switch (call.method) {
            'list' => [
              {'id': 'w1', 'title': 'Calculator', 'app_id': 'calc'},
            ],
            'focused' => 'w1',
            'appMenu' => {
              'app_id': 'calc',
              'serial': 7,
              'model': calculatorModel(),
            },
            _ => true,
          };
        })
        ..setMockMethodCallHandler(
          const MethodChannel('silica/layer', JSONMethodCodec()),
          (call) async => true,
        );
      addTearDown(
        () => ShellChannel.instance.appMenu.value = const AppMenuState(),
      );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: MenuBar(now: DateTime(2026, 10, 15, 9)),
        ),
      );
      await tester.pump();
      await tester.pump();
    }

    testWidgets('shows the app menus; a choice goes back to the app', (
      tester,
    ) async {
      await pumpBar(tester);
      final name = tester.widget<Text>(find.text('Calculator').first);
      expect(name.style?.fontWeight, FontWeight.w600);
      for (final title in ['Edit', 'View', 'Window', 'Help']) {
        expect(find.text(title), findsOneWidget, reason: title);
      }
      // The generic File menu is gone.
      expect(find.text(englishTexts.menuFile), findsNothing);
      await tester.tapAt(tester.getCenter(find.text('View')));
      await tester.pump();
      expect(find.text('⌘2'), findsOneWidget);
      await tester.tapAt(tester.getCenter(find.text('Scientific')));
      await tester.pump(const Duration(milliseconds: 150));
      final chosen = calls.where((c) => c.method == 'activateMenuItem');
      expect(chosen.single.arguments, {'serial': 7, 'id': 4});
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.text('Scientific'), findsNothing);
    });

    testWidgets('the app menu hides the app through Ossido', (tester) async {
      await pumpBar(tester);
      await tester.tapAt(tester.getCenter(find.text('Calculator').first));
      await tester.pump();
      expect(find.text(englishTexts.hideOthers), findsOneWidget);
      expect(find.text(englishTexts.showAll), findsOneWidget);
      await tester.tapAt(
        tester.getCenter(find.text(englishTexts.hideApp('Calculator'))),
      );
      await tester.pump(const Duration(milliseconds: 150));
      final run = calls.where((c) => c.method == 'execute');
      expect((run.single.arguments as Map)['id'], 'app.hide');
      await tester.pump(const Duration(milliseconds: 400));
    });

    testWidgets('updates live and falls back to the generic menus', (
      tester,
    ) async {
      await pumpBar(tester);
      Future<void> send(Object? args) async {
        await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
          ShellChannel.channel.name,
          codec.encodeMethodCall(MethodCall('appMenu', args)),
          (_) {},
        );
        await tester.pump();
      }

      await send({
        'app_id': 'calc',
        'serial': 7,
        'model': calculatorModel(scientific: 'Scientific mode'),
      });
      await tester.tapAt(tester.getCenter(find.text('View')));
      await tester.pump();
      expect(find.text('Scientific mode'), findsOneWidget);
      await tester.tapAt(tester.getCenter(find.text('View')));
      await tester.pump(const Duration(milliseconds: 400));
      // The app takes its menus away: the generic ones come back.
      await send({'app_id': 'calc', 'serial': 0, 'model': null});
      expect(find.text('Edit'), findsNothing);
      expect(find.text(englishTexts.menuFile), findsOneWidget);
    });
  });
}
