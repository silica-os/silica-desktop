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

  test('a context menu opens with its first row under the pointer', () {
    final request = PopupMenuRequest(
      serial: 1,
      appId: '',
      popUp: false,
      anchor: const Rect.fromLTWH(300, 400, 0, 0),
      selected: 0,
      rows: rows,
    );
    expect(popupMenuTopLeft(request), const Offset(300, 389));
  });

  test('a pull-down opens below its control', () {
    final request = PopupMenuRequest(
      serial: 1,
      appId: '',
      popUp: false,
      anchor: const Rect.fromLTWH(100, 200, 80, 24),
      selected: 0,
      rows: rows,
    );
    expect(popupMenuTopLeft(request), const Offset(100, 228));
  });

  test('a pop-up button aligns the selected row on the control', () {
    final items = [
      PopupRow(kind: PopupRowKind.item, id: 1, label: 'A'),
      PopupRow(kind: PopupRowKind.separator),
      PopupRow(kind: PopupRowKind.item, id: 2, label: 'B'),
    ];
    final request = PopupMenuRequest(
      serial: 1,
      appId: '',
      popUp: true,
      anchor: const Rect.fromLTWH(100, 200, 80, 24),
      selected: 2,
      rows: items,
    );
    final top = popupMenuTopLeft(request);
    // 4 padding + 24 + 8 above the selected row: its centre is on the control's.
    expect(top.dy + 4 + 24 + 8 + 12, 212);
    expect(top.dx, 100 - 5 - 4);
  });
}
