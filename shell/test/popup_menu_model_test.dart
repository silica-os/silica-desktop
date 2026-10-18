import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:silica_ui/silica_ui.dart';
import 'package:silica_shell/system/popup_menu_model.dart';

Map<String, Object?> row(
  int kind, {
  int id = 0,
  String label = '',
  int flags = 0,
}) => {'kind': kind, 'id': id, 'label': label, 'flags': flags};

void main() {
  final rows = [
    PopupRow.fromJson(row(PopupRowKind.section, label: 'Edit'))!,
    PopupRow.fromJson({
      ...row(PopupRowKind.item, id: 1, label: 'Copy', flags: 2),
      'key': 'C',
      'modifiers': 1,
    })!,
    PopupRow.fromJson(row(PopupRowKind.separator))!,
    PopupRow.fromJson(row(PopupRowKind.submenuBegin, label: 'Share'))!,
    PopupRow.fromJson(row(PopupRowKind.item, id: 2, label: 'Mail'))!,
    PopupRow.fromJson(row(PopupRowKind.submenuEnd))!,
    PopupRow.fromJson(
      row(PopupRowKind.item, id: 3, label: 'Delete', flags: 9),
    )!,
  ];

  test('rows become Amalfi entries with the choice going back', () {
    final chosen = <int>[];
    final entries = popupEntries(rows, chosen.add);
    expect(entries.length, 5);
    expect(entries[0], isA<AmalfiMenuSection>());
    final copy = entries[1] as AmalfiMenuAction;
    expect(copy.checked, isTrue);
    expect(copy.shortcut, '⌘C');
    expect(entries[2], isA<AmalfiMenuDivider>());
    final share = entries[3] as AmalfiMenuAction;
    expect(share.submenu!.length, 1);
    (share.submenu!.first as AmalfiMenuAction).onSelected!();
    final del = entries[4] as AmalfiMenuAction;
    expect(del.enabled, isFalse);
    expect(del.destructive, isTrue);
    copy.onSelected!();
    expect(chosen, [2, 1]);
  });

  test('a submenu with no rows is dropped and stray ends are ignored', () {
    final entries = popupEntries([
      PopupRow(kind: PopupRowKind.submenuBegin, label: 'Empty'),
      PopupRow(kind: PopupRowKind.submenuEnd),
      PopupRow(kind: PopupRowKind.submenuEnd),
      PopupRow(kind: PopupRowKind.item, id: 1, label: 'A'),
    ], (_) {});
    expect(entries.length, 1);
  });

  test('a request from Ossido is read defensively', () {
    expect(PopupMenuRequest.fromJson(null), isNull);
    expect(PopupMenuRequest.fromJson({'serial': 0}), isNull);
    expect(
      PopupMenuRequest.fromJson({
        'serial': 1,
        'anchor': [1, 2],
        'rows': [],
      }),
      isNull,
    );
    final request = PopupMenuRequest.fromJson({
      'serial': 3,
      'app_id': 'a',
      'kind': 1,
      'anchor': [100, 200, 80, 22],
      'selected': 2,
      'appearance': 'dark',
      'rows': [
        row(PopupRowKind.item, id: 2, label: 'x'),
        'junk',
        {'kind': 99},
      ],
    })!;
    expect(request.popUp, isTrue);
    expect(request.rows.length, 1);
    expect(request.anchor, const Rect.fromLTWH(100, 200, 80, 22));
  });

  PopupMenuRequest request({
    required Rect anchor,
    bool popUp = true,
    int selected = 0,
    int count = 5,
  }) => PopupMenuRequest(
    serial: 1,
    appId: '',
    popUp: popUp,
    anchor: anchor,
    selected: selected,
    rows: [
      for (var i = 1; i <= count; i++)
        PopupRow(kind: PopupRowKind.item, id: i, label: 'Item $i'),
    ],
  );

  const screen = Size(1000, 800);

  test('a context menu opens with its first row under the pointer', () {
    final r = request(
      anchor: const Rect.fromLTWH(300, 400, 0, 0),
      popUp: false,
    );
    expect(popupMenuTopLeft(r), const Offset(300, 389));
    expect(popupMenuLayout(r), isNull);
  });

  group('a pop-up button', () {
    const control = Rect.fromLTWH(300, 300, 120, 28);

    test('puts the selected row on the control: first, middle, last', () {
      for (final id in [1, 3, 5]) {
        final layout = popupMenuLayout(request(anchor: control, selected: id))!;
        final place = layout.place(screen, 150);
        // 4 of padding, 24 per row, the row 2 below the 28 pt control's top.
        expect(place.top + 4 + 24.0 * (id - 1), 302, reason: 'item $id');
        expect(place.left, 288);
        expect(layout.height, 128);
        expect(layout.minWidth, 120);
      }
    });

    test('counts separators and sections, and skips submenu rows', () {
      final r = PopupMenuRequest(
        serial: 1,
        appId: '',
        popUp: true,
        anchor: control,
        selected: 2,
        rows: const [
          PopupRow(kind: PopupRowKind.section, label: 'S'),
          PopupRow(kind: PopupRowKind.separator),
          PopupRow(kind: PopupRowKind.submenuBegin, label: 'More'),
          PopupRow(kind: PopupRowKind.item, id: 9, label: 'Inner'),
          PopupRow(kind: PopupRowKind.submenuEnd),
          PopupRow(kind: PopupRowKind.item, id: 2, label: 'B'),
        ],
      );
      final layout = popupMenuLayout(r)!;
      // 4 + section 24 + separator 8 + submenu 24.
      expect(layout.selectedTop, 60);
      expect(layout.height, 4 + 24 + 8 + 24 + 24 + 4);
    });

    test('no selected row aligns the first', () {
      final layout = popupMenuLayout(request(anchor: control, selected: 0))!;
      expect(layout.first, isTrue);
      expect(layout.place(screen, 150).top, 298);
    });

    test('the first item flips above without room below', () {
      final layout = popupMenuLayout(
        request(anchor: const Rect.fromLTWH(300, 770, 120, 28), selected: 1),
      )!;
      final place = layout.place(screen, 150);
      expect(place.flipped, isTrue);
      expect(place.top + place.height, 770);
    });

    test('a middle item near the bottom scrolls with an arrow strip', () {
      final layout = popupMenuLayout(
        request(anchor: const Rect.fromLTWH(300, 760, 120, 28), selected: 3),
      )!;
      final place = layout.place(screen, 150);
      expect(place.flipped, isFalse);
      expect(place.top + place.height, 794);
      expect(place.arrowBottom, isTrue);
    });

    test('the last item near the top scrolls instead of flipping', () {
      final layout = popupMenuLayout(
        request(anchor: const Rect.fromLTWH(300, 20, 120, 28), selected: 5),
      )!;
      final place = layout.place(screen, 150);
      expect(place.flipped, isFalse);
      expect(place.top, 6);
      expect(place.arrowTop, isTrue);
      expect(place.top - place.scroll + 4 + 24 * 4, 22);
    });

    test(
      'past the right edge it slides left and keeps the row on the control',
      () {
        final layout = popupMenuLayout(
          request(anchor: const Rect.fromLTWH(940, 300, 120, 28), selected: 3),
        )!;
        final place = layout.place(screen, 200);
        expect(place.left, 1000 - 6 - 200);
        expect(place.top + 4 + 48, 302);
      },
    );

    test('small and mini controls use 22 and 19 pt rows', () {
      final small = popupMenuLayout(
        request(anchor: const Rect.fromLTWH(300, 300, 100, 24), selected: 2),
      )!;
      expect(small.rowHeight, 22);
      expect(small.height, 4 + 5 * 22 + 4);
      expect(small.place(screen, 120).left, 289);
      final mini = popupMenuLayout(
        request(anchor: const Rect.fromLTWH(300, 300, 100, 20), selected: 2),
      )!;
      expect(mini.rowHeight, 19);
      // Baselines coincide plus the mini text drop (4): the row is 4.5 below the top.
      expect(mini.place(screen, 120).top + 4 + 19, 304.5);
    });
  });

  group('a pull-down', () {
    const control = Rect.fromLTWH(100, 200, 80, 28);

    test('opens 3 below its control, 5 left of it and 4 narrower', () {
      final layout = popupMenuLayout(request(anchor: control, popUp: false))!;
      final place = layout.place(screen, 76);
      expect(place.top, 231);
      expect(place.left, 95);
      expect(layout.minWidth, 76);
    });

    test('opens above without room below', () {
      final layout = popupMenuLayout(
        request(anchor: const Rect.fromLTWH(100, 760, 80, 28), popUp: false),
      )!;
      final place = layout.place(screen, 76);
      expect(place.flipped, isTrue);
      expect(place.top + place.height, 760);
    });
  });
}
