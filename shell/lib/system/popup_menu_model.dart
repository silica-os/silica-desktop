// The context menus and pop-up buttons of the apps (silica-desktop#241, docs/spec/menus.md):
// Ossido hands the rows of an app's menu to the shell (`silica_shell_manager_v1.popup_menu`,
// `popupMenu` on silica/shell), the shell draws them with the menu bar's menu widgets and
// sends the choice back. The message comes from another process: it is read defensively.

import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import 'app_menu_model.dart';

/// The kinds of row (`popup_row_kind`).
abstract final class PopupRowKind {
  static const int item = 0;
  static const int separator = 1;
  static const int section = 2;
  static const int submenuBegin = 3;
  static const int submenuEnd = 4;
}

/// Flags of an item.
abstract final class PopupFlags {
  static const int disabled = 1;
  static const int checked = 2;
  static const int mixed = 4;
  static const int destructive = 8;
}

@immutable
class PopupRow {
  const PopupRow({
    required this.kind,
    this.id = 0,
    this.label = '',
    this.flags = 0,
    this.icon = '',
    this.key = '',
    this.modifiers = 0,
  });

  final int kind;
  final int id;
  final String label;
  final int flags;
  final String icon;
  final String key;
  final int modifiers;

  static PopupRow? fromJson(Object? json) {
    if (json is! Map) return null;
    int number(String name) {
      final v = json[name];
      return v is int && v >= 0 ? v : 0;
    }

    String text(String name) {
      final v = json[name];
      return v is String ? v : '';
    }

    final kind = number('kind');
    if (kind > PopupRowKind.submenuEnd) return null;
    return PopupRow(
      kind: kind,
      id: number('id'),
      label: text('label'),
      flags: number('flags'),
      icon: text('icon'),
      key: text('key'),
      modifiers: number('modifiers'),
    );
  }

  /// The shortcut as the menus write it, null without a key.
  String? get shortcut => key.isEmpty
      ? null
      : MenuShortcut(
          key,
          meta: modifiers & 1 != 0,
          ctrl: modifiers & 2 != 0,
          alt: modifiers & 4 != 0,
          shift: modifiers & 8 != 0,
        ).text;
}

/// A menu an app asked for.
@immutable
class PopupMenuRequest {
  const PopupMenuRequest({
    required this.serial,
    required this.appId,
    required this.popUp,
    required this.anchor,
    required this.selected,
    required this.rows,
    this.brightness,
  });

  final int serial;
  final String appId;

  /// A pop-up button's menu (the selected item over the control), else a context menu.
  final bool popUp;

  /// The pointer (0 x 0) or the control, on the output.
  final Rect anchor;
  final int selected;
  final List<PopupRow> rows;
  final Brightness? brightness;

  static PopupMenuRequest? fromJson(Object? json) {
    if (json is! Map) return null;
    final serial = json['serial'];
    final anchor = json['anchor'];
    final rows = json['rows'];
    if (serial is! int || serial <= 0 || anchor is! List || rows is! List) {
      return null;
    }
    if (anchor.length != 4 || anchor.any((v) => v is! num)) return null;
    final a = [for (final v in anchor) (v as num).toDouble()];
    return PopupMenuRequest(
      serial: serial,
      appId: '${json['app_id'] ?? ''}',
      popUp: json['kind'] == 1,
      anchor: Rect.fromLTWH(a[0], a[1], a[2].abs(), a[3].abs()),
      selected: json['selected'] is int ? json['selected'] as int : 0,
      rows: [
        for (final row in rows)
          if (PopupRow.fromJson(row) case final r?) r,
      ],
      brightness: switch (json['appearance']) {
        'dark' => Brightness.dark,
        'light' => Brightness.light,
        _ => null,
      },
    );
  }
}

/// The Amalfi rows of a menu, with `onChoose(id)` for the items (the choice goes to Ossido).
/// A submenu with no usable row is left out; unbalanced ends are ignored.
List<AmalfiMenuEntry> popupEntries(
  List<PopupRow> rows,
  void Function(int id) onChoose,
) {
  var at = 0;
  List<AmalfiMenuEntry> level(int depth) {
    final out = <AmalfiMenuEntry>[];
    while (at < rows.length) {
      final row = rows[at++];
      switch (row.kind) {
        case PopupRowKind.item:
          out.add(
            AmalfiMenuAction(
              row.label,
              shortcut: row.shortcut,
              icon: _icon(row.icon),
              checked: row.flags & (PopupFlags.checked | PopupFlags.mixed) != 0,
              enabled: row.flags & PopupFlags.disabled == 0,
              destructive: row.flags & PopupFlags.destructive != 0,
              onSelected: () => onChoose(row.id),
            ),
          );
        case PopupRowKind.separator:
          out.add(const AmalfiMenuDivider());
        case PopupRowKind.section:
          out.add(AmalfiMenuSection(row.label));
        case PopupRowKind.submenuBegin:
          final children = depth >= 8 ? <AmalfiMenuEntry>[] : level(depth + 1);
          if (children.isNotEmpty) {
            out.add(
              AmalfiMenuAction(
                row.label,
                icon: _icon(row.icon),
                enabled: row.flags & PopupFlags.disabled == 0,
                submenu: children,
              ),
            );
          }
        case PopupRowKind.submenuEnd:
          if (depth > 0) return out;
      }
    }
    return out;
  }

  return level(0);
}

/// A symbol name of the Silica set, or the path of an image file, at 16 pt in the row's
/// colour; null for none.
Widget? _icon(String icon) {
  if (icon.isEmpty) return null;
  if (icon.startsWith('/')) {
    return Image.file(
      File(icon),
      width: 16,
      height: 16,
      errorBuilder: (_, _, _) => const SizedBox.square(dimension: 16),
    );
  }
  return Builder(
    builder: (context) => AmalfiSymbol(
      icon,
      size: 16,
      color: IconTheme.of(context).color ?? const Color(0xFF000000),
      fallback: const SizedBox.square(dimension: 16),
    ),
  );
}

/// The top left corner of the menu on the output.
///
/// A context menu: the first row under the pointer (11 pt above it), as the desktop menu. A
/// pop-up button: the selected top-level row centred on the control (the text baselines
/// line up) and its row edge 5 pt left of the control's (docs/spec/menus.md, provisional).
Offset popupMenuTopLeft(PopupMenuRequest request, {double above = 11}) {
  final a = request.anchor;
  if (!request.popUp) return Offset(a.left, a.top - above);
  const padding = AmalfiMenuMetrics.padding;
  var offset = padding;
  var found = false;
  for (final row in request.rows) {
    if (row.kind == PopupRowKind.item && row.id == request.selected) {
      found = true;
      break;
    }
    if (row.kind == PopupRowKind.separator) {
      offset += AmalfiMenuMetrics.separatorHeight;
    } else if (row.kind == PopupRowKind.submenuBegin) {
      // Skip its rows: they belong to the submenu.
      offset += AmalfiMenuMetrics.itemHeight;
    } else if (row.kind != PopupRowKind.submenuEnd) {
      offset += AmalfiMenuMetrics.itemHeight;
    }
  }
  if (!found) offset = padding;
  return Offset(
    a.left - 5 - padding,
    a.center.dy - offset - AmalfiMenuMetrics.itemHeight / 2,
  );
}
