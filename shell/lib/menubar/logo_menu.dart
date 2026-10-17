// The logo menu at the left of the bar (SH-MB-02, SH-MB-03, SH-MB-04; menubar.md,
// inventory INV-A1): about, settings, recent items, force quit, the power
// items, lock and log out. Icons only on the first two items; with ⌥ held some items
// change: "About" → "System Information", "Restart…" → "Restart", "Shut Down…" → "Shut
// Down", the log out item loses its ellipsis and its shortcut becomes ⌥⇧⌘Q. The menu keeps
// its size.
//
// Sleep, Restart, Shut Down and Log Out go through `power` (power.dart: the confirmation
// card, logind). Outside a real Silica session the power items are disabled, so a nested or
// test session never powers off its host. Lock Screen stays disabled until the lock screen
// exists (SH-LK).

import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../dock/stack.dart' show fileIcon, fileName;
import '../l10n/l10n.dart';
import '../system/desktop_entries.dart';
import '../system/icon_lookup.dart';
import '../system/silica_icons.dart';
import 'power.dart';

/// Recent apps and documents for the "Recent Items" submenu (at most 10 each).
class RecentItems {
  const RecentItems({this.apps = const [], this.documents = const []});

  final List<DesktopEntry> apps;
  final List<String> documents;

  static const int limit = 10;

  /// The documents of `recently-used.xbel` (freedesktop recent files), newest first, only
  /// local files that still exist and were used after the last "Clear Menu".
  static List<String> loadDocuments({
    Map<String, String>? env,
    int limit = RecentItems.limit,
  }) {
    final e = env ?? Platform.environment;
    final home = e['HOME'] ?? '';
    final data = e['XDG_DATA_HOME'] ?? '$home/.local/share';
    final String xml;
    try {
      xml = File('$data/recently-used.xbel').readAsStringSync();
    } on FileSystemException {
      return const [];
    }
    final cleared = _clearedFile(e);
    String? after;
    try {
      after = cleared.readAsStringSync().trim();
    } on FileSystemException {
      after = null;
    }
    return parseRecentlyUsed(
      xml,
      after: after,
    ).where((p) => File(p).existsSync()).take(limit).toList();
  }

  /// Hides the documents used until now (the freedesktop list belongs to every app and is
  /// not changed).
  static void clearDocuments({Map<String, String>? env}) {
    final file = _clearedFile(env ?? Platform.environment);
    try {
      file.parent.createSync(recursive: true);
      file.writeAsStringSync(
        '${DateTime.now().toUtc().toIso8601String().split('.').first}Z',
      );
    } on FileSystemException {
      // Without a writable state the list stays.
    }
  }

  static File _clearedFile(Map<String, String> e) {
    final home = e['HOME'] ?? '';
    final state = e['XDG_STATE_HOME'] ?? '$home/.local/state';
    return File('$state/silica/recent-documents-cleared');
  }
}

/// The local paths of an xbel file, by `modified` (or `visited`) date, newest first; with
/// `after` (an ISO 8601 UTC time) only those used later.
List<String> parseRecentlyUsed(String xml, {String? after}) {
  final bookmark = RegExp(r'<bookmark\b([^>]*)>');
  String? attribute(String attrs, String name) =>
      RegExp('\\b$name="([^"]*)"').firstMatch(attrs)?.group(1);
  final found = <(String, String)>[];
  for (final m in bookmark.allMatches(xml)) {
    final attrs = m.group(1) ?? '';
    final href = attribute(attrs, 'href');
    if (href == null || !href.startsWith('file://')) continue;
    final String path;
    try {
      path = Uri.parse(href.replaceAll('&amp;', '&')).toFilePath();
    } on FormatException {
      continue;
    } on UnsupportedError {
      continue;
    }
    final date =
        attribute(attrs, 'modified') ?? attribute(attrs, 'visited') ?? '';
    if (after != null && date.compareTo(after) <= 0) continue;
    found.add((date, path));
  }
  found.sort((a, b) => b.$1.compareTo(a.$1));
  final seen = <String>{};
  return [
    for (final (_, p) in found)
      if (seen.add(p)) p,
  ];
}

/// The logo menu. `run` runs an Ossido command, `launch` a command line, `open` a file;
/// `option` is true while ⌥ is held; `user` the full name for the log out item.
List<MenuEntry> logoMenu({
  required AppLocalizations l,
  required void Function(String id) run,
  required void Function(DesktopEntry entry) launch,
  required void Function(String path) open,
  required bool option,
  required String user,
  DesktopEntry? settings,
  RecentItems recent = const RecentItems(),
  VoidCallback? clearRecent,
  void Function(PowerAction action, {required bool now})? power,
  bool powerEnabled = false,
}) {
  final hasRecent = recent.apps.isNotEmpty || recent.documents.isNotEmpty;
  return [
    MenuAction(
      option ? l.systemInformation : l.aboutSilica,
      icon: (color) => AmalfiSymbol(
        'desktop',
        size: 16,
        color: color,
        fallback: CustomPaint(painter: _ComputerGlyph(color)),
      ),
      enabled: false,
    ),
    const MenuDivider(),
    MenuAction(
      l.systemSettings,
      icon: (color) => AmalfiSymbol(
        'gear',
        size: 16,
        color: color,
        fallback: CustomPaint(painter: _GearGlyph(color)),
      ),
      enabled: settings != null,
      onSelected: settings == null ? null : () => launch(settings),
    ),
    const MenuDivider(),
    MenuAction(
      l.recentItems,
      submenu: [
        MenuSection(l.recentApplications),
        for (final app in recent.apps)
          MenuAction(
            app.name,
            icon: (_) => _appIcon(app),
            onSelected: () => launch(app),
          ),
        if (recent.apps.isEmpty) MenuAction(l.recentNone, enabled: false),
        const MenuDivider(),
        MenuSection(l.recentDocuments),
        for (final path in recent.documents)
          MenuAction(
            fileName(path),
            icon: (_) => _fileIcon(path),
            onSelected: () => open(path),
          ),
        if (recent.documents.isEmpty) MenuAction(l.recentNone, enabled: false),
        const MenuDivider(),
        MenuAction(
          l.recentClear,
          enabled: hasRecent && clearRecent != null,
          onSelected: clearRecent,
        ),
      ],
    ),
    const MenuDivider(),
    MenuAction(
      l.forceQuitMenu,
      shortcut: '⌥⌘⎋',
      onSelected: () => run('app.force_quit_panel'),
    ),
    const MenuDivider(),
    MenuAction(
      l.sleep,
      enabled: powerEnabled && power != null,
      onSelected: () => power?.call(PowerAction.sleep, now: true),
    ),
    MenuAction(
      option ? l.restartNow : l.restart,
      enabled: powerEnabled && power != null,
      onSelected: () => power?.call(PowerAction.restart, now: option),
    ),
    MenuAction(
      option ? l.shutDownNow : l.shutDown,
      enabled: powerEnabled && power != null,
      onSelected: () => power?.call(PowerAction.shutDown, now: option),
    ),
    const MenuDivider(),
    MenuAction(l.lockScreen, shortcut: '⌃⌘Q', enabled: false),
    MenuAction(
      option ? l.logOutUserNow(user) : l.logOutUser(user),
      shortcut: option ? '⌥⇧⌘Q' : '⇧⌘Q',
      onSelected: power == null
          ? () => run('session.quit')
          : () => power(PowerAction.logOut, now: option),
    ),
  ];
}

/// Colored 16 pt icons in the recent items (menubar.md › Submenu).
const double recentIconSide = 16;

Widget _appIcon(DesktopEntry entry) {
  final layered = silicaIconFor(entry);
  final flat = findIcon(entry.icon);
  final IconSource? source = layered != null
      ? LayeredSource(layered)
      : flat != null
      ? FlatSource(flat)
      : null;
  return SizedBox.square(
    dimension: recentIconSide,
    child: source == null ? null : SilicaIcon(source, side: recentIconSide),
  );
}

Widget _fileIcon(String path) {
  final source = fileIcon(File(path));
  return SizedBox.square(
    dimension: recentIconSide,
    child: source == null ? null : SilicaIcon(source, side: recentIconSide),
  );
}

/// The computer glyph of "About": a flat box with a slot, in the text color.
class _ComputerGlyph extends CustomPainter {
  const _ComputerGlyph(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final c = size.center(Offset.zero);
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromCenter(center: c, width: 14, height: 4),
        const Radius.circular(1.5),
      ),
      Paint()..color = color,
    );
  }

  @override
  bool shouldRepaint(_ComputerGlyph old) => old.color != color;
}

/// The gear glyph of Settings.
class _GearGlyph extends CustomPainter {
  const _GearGlyph(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final c = size.center(Offset.zero);
    final stroke = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.3;
    canvas
      ..drawCircle(c, 4.6, stroke)
      ..drawCircle(c, 1.6, stroke);
    for (var i = 0; i < 8; i++) {
      final a = i * math.pi / 4;
      final d = Offset(math.cos(a), math.sin(a));
      canvas.drawLine(c + d * 4.6, c + d * 6.3, stroke);
    }
  }

  @override
  bool shouldRepaint(_GearGlyph old) => old.color != color;
}
