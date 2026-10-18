// The app launcher: Faro in apps mode, on the Amalfi kit (`Apps/Faro/Launcher`, `Launcher
// Category`, `Launcher List`; docs/design/amalfi/components.md › Launcher). Its own panel,
// Glass/Thick radius 28: 844 × 580 in grid mode, 640 × 560 in list mode, top edge on Faro's
// 30 % line. A 56 pt header (apps glyph or back chevron at x 20, field text at x 56, "…" view
// menu at the right), a separator, a row of seven category capsules, then the grid (7 columns
// of 112 pt, 60 pt icons) or the list (56 pt rows); at the top a row of recent apps separated
// by a line.
//
// A category becomes a token in the field and the grid shows only that one; typing filters
// (inside the category, or over all apps with the best first and a ring around it).
// Esc removes the text, then the category, then goes back to Faro.
//
// Searching all apps, if the first result starts with the text the field completes it
// inline ("cal" + "endar — Open") and shows the app icon on the right.
//
// Keyboard (PA-60): in the grid, Down chooses the first icon, the arrows move the blue ring,
// Return opens it and the field shows "<name> — Open" with the app icon.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../dock/apps.dart' show appsItemId;
import '../system/desktop_entries.dart';
import '../system/icon_lookup.dart';
import '../system/silica_icons.dart';
import 'search.dart';

/// Grid mode: 844 × 580.
const double launcherWidth = 844;
const double launcherHeight = 580;

/// List mode: 640 × 560.
const double launcherListWidth = 640;
const double launcherListHeight = 560;
const double launcherRadius = AmalfiRadii.faroAndLauncher;

/// The glass group of Faro's menus: the panel is group 0, and Ossido merges only glass of one
/// group (a Thick menu in group 0 melts into the Thick panel and shows no glass of its own). A
/// menu's submenus take the next groups.
const int faroMenuGroup = 1;
const double launcherHeader = 56;

/// The panel's size in grid or list mode.
Size launcherSize({required bool list}) => list
    ? const Size(launcherListWidth, launcherListHeight)
    : const Size(launcherWidth, launcherHeight);

/// Side margin of separators and capsules.
const double launcherMargin = 20;

/// Category capsules: 28 tall from y 68 (12 under the separator), 8 apart, text 8 from each
/// edge, radius 8, Callout.
const double categoryTop = 68;
const double categoryHeight = 28;
const double categoryGap = 8;
const double categoryPadding = 8;
const double categoryRadius = 8;

/// Grid: cells 112 wide (the first at x 28), icons 60 at +28 (the kit), labels 16 tall 68 pt
/// under the icon top, 104 between rows. Browsing: the first row's icons at y 124; with a
/// category or a query, at y 88.
const int gridColumns = 7;
const double cellWidth = 112;
const double cellOriginX = 28;
const double gridIcon = 60;
const double gridIconX = 28;
const double gridLabelTop = 68;
const double gridLabelHeight = 16;
const double rowPitch = 104;
const double browsingIconTop = 124;
const double plainIconTop = 88;

/// Under the recents a separator at y 216 (8 under the labels), the list's icons at 236.
const double recentSeparatorY = 216;
const double recentListTop = 236;

/// List mode: 5 recents in cells of 116 (icons at y 108, labels 68 below), the separator at
/// y 208 and the 56 pt rows from y 220 (browsing) or 64.
const int listRecents = 5;
const double listCellWidth = 116;
const double listRecentTop = 108;
const double listSeparatorY = 208;
const double listRowHeight = 56;
const double listRowsTop = 220;
const double listPlainTop = 64;

/// Ring of the first result: 4 pt around the icon, outer radius 17.
const double ringWidth = 4;
const double ringRadius = 17;

/// Category token in the field: 32 tall, radius 10, Title 3.
const double tokenHeight = 32;
const double tokenRadius = 10;

/// Inline completion: a rectangle 32 tall (12 from the top of the header), radius 8.
const double completionHeight = 32;
const double completionRadius = 8;

/// The app that handles the chosen row, at the right of the field: 28 pt.
const double previewSide = 28;

/// The suffix to complete: the rest of the first result's name, if it starts with the typed
/// text (and there is no category). The field shows it followed by "— Open".
String? completionFor(String query, List<DesktopEntry> results) {
  final q = query.trimLeft();
  final first = results.firstOrNull;
  if (q.isEmpty || first == null) return null;
  if (!first.name.toLowerCase().startsWith(q.toLowerCase())) return null;
  return first.name.substring(q.length);
}

/// The colours of Faro and the launcher: Amalfi tokens (`Text/*`, `App/Faro/*`,
/// `Surface/Sunken`, `Line/Separator`), light and dark, resolved from the appearance.
@immutable
class FaroColors {
  const FaroColors._(this.theme);

  final AmalfiThemeData theme;

  AmalfiColors get _t => theme.colors;

  Color get text => _t.textPrimary;
  Color get placeholder => _t.textTertiary;
  Color get subtitle => _t.textSecondary;
  Color get separator => _t.lineSeparator;

  /// The chosen row, the category token, the ring.
  Color get selection => _t.faro.selection;
  Color get pill => _t.faro.pill;
  Color get completion => _t.faro.completion;
  Color get key => _t.faro.key;

  /// Glyph tiles and the answer card.
  Color get sunken => _t.surfaceSunken;

  /// Drop shadow of the panel (`Shadow/3`, level 3).
  BoxShadow get shadow => theme.shadows.level3.first;

  static FaroColors of(BuildContext context) =>
      FaroColors._(AmalfiTheme.of(context));
}

/// The launcher categories (stable ids) and the freedesktop categories they contain, from
/// the first.
const List<(String, List<String>)> launcherCategories = [
  ('productivity', ['Office', 'Finance', 'ProjectManagement']),
  ('utilities', ['Utility', 'System', 'Settings', 'Accessibility']),
  ('creativity', ['Graphics', 'Audio', 'Video', 'AudioVideo', 'Development']),
  ('entertainment', ['Game', 'Player', 'Music']),
  ('social', ['Chat', 'InstantMessaging', 'Email', 'Network']),
  ('reading', ['Education', 'Science', 'News', 'Viewer', 'Documentation']),
];
const String otherCategory = 'other';

List<String> get categoryNames => [
  for (final (name, _) in launcherCategories) name,
  otherCategory,
];

/// The shown name of a category id.
String categoryLabel(AppLocalizations l, String id) => switch (id) {
  'productivity' => l.catProductivity,
  'utilities' => l.catUtilities,
  'creativity' => l.catCreativity,
  'entertainment' => l.catEntertainment,
  'social' => l.catSocial,
  'reading' => l.catReading,
  _ => l.catOther,
};

/// The launcher category of an app: the first that matches, or the other one.
String categoryOf(DesktopEntry entry) {
  for (final (name, members) in launcherCategories) {
    if (entry.categories.any(members.contains)) return name;
  }
  return otherCategory;
}

int _byName(DesktopEntry a, DesktopEntry b) =>
    a.name.toLowerCase().compareTo(b.name.toLowerCase());

/// The grid apps (the file manager included, as Finder is in Applications): alphabetical, in the category if there is one; with text, inside a
/// category it filters alphabetically, over all apps the best goes first.
List<DesktopEntry> launcherApps(
  Iterable<DesktopEntry> entries, {
  String? category,
  String query = '',
}) {
  final inCategory = [
    for (final e in entries)
      if (e.id != appsItemId && (category == null || categoryOf(e) == category))
        e,
  ];
  if (query.trim().isEmpty) return inCategory..sort(_byName);
  final found = searchApps(inCategory, query);
  if (category != null) found.sort(_byName);
  return found;
}

/// The recent apps (ids from the most recent), from ~/.local/state/silica/recent-apps.json.
class RecentApps {
  RecentApps({Map<String, String>? env}) : _env = env ?? Platform.environment;

  final Map<String, String> _env;
  static const int limit = gridColumns;

  File get _file {
    final home = _env['HOME'] ?? '';
    final state = _env['XDG_STATE_HOME'] ?? '$home/.local/state';
    return File('$state/silica/recent-apps.json');
  }

  List<String> load() {
    try {
      final json = jsonDecode(_file.readAsStringSync());
      if (json is List) return [for (final id in json.take(limit)) '$id'];
    } on FileSystemException {
      // Never saved.
    } on FormatException {
      // Broken file: start over.
    }
    return const [];
  }

  void add(String id) {
    final list = [
      id,
      ...load().where((other) => other != id),
    ].take(limit).toList();
    try {
      _file.parent.createSync(recursive: true);
      _file.writeAsStringSync(jsonEncode(list));
    } on FileSystemException {
      // Without a writable state the recents are not remembered.
    }
  }

  File get _viewFile => File('${_file.parent.path}/launcher-view.json');

  /// The Launcher's view, remembered between sessions: true for the list, false for the grid.
  bool loadList() {
    try {
      return jsonDecode(_viewFile.readAsStringSync()) == 'list';
    } on FileSystemException {
      // Never chosen: the grid.
    } on FormatException {
      // Broken file: the grid.
    }
    return false;
  }

  void saveList(bool list) {
    try {
      _viewFile.parent.createSync(recursive: true);
      _viewFile.writeAsStringSync(jsonEncode(list ? 'list' : 'grid'));
    } on FileSystemException {
      // Without a writable state the choice is not remembered.
    }
  }

  /// Forgets the recent apps (logo menu › Recent Items › Clear Menu).
  void clear() {
    try {
      if (_file.existsSync()) _file.deleteSync();
    } on FileSystemException {
      // Nothing to forget.
    }
  }
}

IconSource? iconOf(DesktopEntry entry) {
  if (silicaIconFor(entry) case final layered?) return LayeredSource(layered);
  if (findIcon(entry.icon) case final path?) return FlatSource(path);
  return null;
}

class Launcher extends StatefulWidget {
  const Launcher({
    super.key,
    required this.entries,
    required this.recents,
    required this.onLaunch,
    required this.onBack,
    this.list = false,
    this.onListChanged,
  });

  final Map<String, DesktopEntry> entries;

  /// The ids of the recent apps, from the most recent.
  final List<String> recents;
  final ValueChanged<DesktopEntry> onLaunch;

  /// Esc with no text and no category: back to Faro.
  final VoidCallback onBack;

  /// List mode (640 × 560) instead of the grid (844 × 580).
  final bool list;

  /// The "…" menu chose the grid (false) or the list (true).
  final ValueChanged<bool>? onListChanged;

  @override
  State<Launcher> createState() => LauncherState();
}

class LauncherState extends State<Launcher>
    with SingleTickerProviderStateMixin {
  final TextEditingController _query = TextEditingController();
  final FocusNode _focus = FocusNode();
  final ScrollController _scroll = ScrollController();
  String? _category;

  /// The row chosen in list mode.
  int _selected = 0;

  /// The icon chosen with the keyboard in the grid (PA-60); null until Down is pressed.
  int? _gridSelected;

  /// The layout of the last build, to scroll the keyboard choice into view.
  GridLayout? _layout;

  /// The "…" menu is open, drawn in the layer (where the system offers no popups).
  bool _menu = false;

  /// The "…" menu as a popup surface above the layer (silica-desktop#222): Ossido draws the
  /// glass of a menu below its own surface's pixels, so a menu inside the Faro layer would
  /// have the panel's icons and text on top of its glass.
  SilicaPopupController? _popup;

  /// Where the popup is measured and put: the Launcher has no overlay of its own.
  final GlobalKey<OverlayState> _overlay = GlobalKey<OverlayState>();

  /// The category change: the grid disappears at once and fades back in after a gap
  /// (controller value = ms since the change).
  late final AnimationController _fade = AnimationController(
    vsync: this,
    upperBound: 260,
    value: 260,
    duration: const Duration(milliseconds: 260),
  )..addListener(() => setState(() {}));

  String? get category => _category;

  @override
  void initState() {
    super.initState();
    _query.addListener(
      () => setState(() {
        _selected = 0;
        _gridSelected = null;
      }),
    );
    _scroll.addListener(() => setState(() {}));
    WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
    unawaited(SilicaPopups.probe());
  }

  @override
  void dispose() {
    final popup = _popup;
    _popup = null;
    if (popup != null) unawaited(popup.close());
    _fade.dispose();
    _query.dispose();
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  void chooseCategory(String? name) {
    _fade.forward(from: 0);
    setState(() {
      _category = name;
      _selected = 0;
      _gridSelected = null;
      _query.clear();
    });
    if (_scroll.hasClients) _scroll.jumpTo(0);
  }

  void _closeMenu() {
    if (!_menu) return;
    setState(() => _menu = false);
    _focus.requestFocus();
  }

  List<AmalfiMenuEntry> _menuEntries(bool list) => [
    AmalfiMenuAction(
      l10nOf(context).launcherViewGrid,
      checked: !list,
      onSelected: () => widget.onListChanged?.call(false),
    ),
    AmalfiMenuAction(
      l10nOf(context).launcherViewList,
      checked: list,
      onSelected: () => widget.onListChanged?.call(true),
    ),
  ];

  /// The "…" button: the view menu opens as a popup surface when the system offers it, in
  /// the layer otherwise.
  Future<void> _toggleMenu() async {
    if (_menu) return _closeMenu();
    if (_popup != null) return;
    final box = context.findRenderObject();
    if (SilicaPopups.available == true && box is RenderBox && box.hasSize) {
      // The menu's top right corner at 20 from the right edge, 44 from the top.
      final corner = box.localToGlobal(Offset(box.size.width - 20, 44));
      final list = widget.list;
      final popup = await showAmalfiAsPopup(
        context: context,
        overlay: _overlay.currentState,
        anchor: corner & const Size(1, 1),
        anchorEdge: SilicaPopupEdge.topRight,
        gravity: SilicaPopupEdge.bottomLeft,
        builder: (context, dismiss, placement) => AmalfiMenu(
          key: const ValueKey('launcher view menu'),
          autofocus: true,
          onDismiss: dismiss,
          entries: _menuEntries(list),
        ),
      );
      if (popup != null) {
        if (!mounted) return unawaited(popup.close());
        _popup = popup;
        unawaited(
          popup.closed.whenComplete(() {
            if (_popup != popup) return;
            _popup = null;
            if (mounted) _focus.requestFocus();
          }),
        );
        return;
      }
    }
    if (mounted) setState(() => _menu = true);
  }

  KeyEventResult _key(FocusNode node, KeyEvent event, List<DesktopEntry> apps) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    switch (event.logicalKey) {
      case LogicalKeyboardKey.escape:
        if (_query.text.isNotEmpty) {
          _query.clear();
        } else if (_category != null) {
          chooseCategory(null);
        } else {
          widget.onBack();
        }
      case LogicalKeyboardKey.backspace
          when _query.text.isEmpty && _category != null:
        chooseCategory(null);
      case LogicalKeyboardKey.arrowDown when widget.list && apps.isNotEmpty:
        setState(() => _selected = (_selected + 1).clamp(0, apps.length - 1));
      case LogicalKeyboardKey.arrowUp when widget.list && apps.isNotEmpty:
        setState(() => _selected = (_selected - 1).clamp(0, apps.length - 1));
      case LogicalKeyboardKey.arrowDown when !widget.list && apps.isNotEmpty:
        _moveGrid(apps.length, _gridSelected == null ? 0 : gridColumns);
      case LogicalKeyboardKey.arrowUp
          when !widget.list && _gridSelected != null:
        _moveGrid(apps.length, -gridColumns);
      case LogicalKeyboardKey.arrowRight
          when !widget.list && _gridSelected != null:
        _moveGrid(apps.length, 1);
      case LogicalKeyboardKey.arrowLeft
          when !widget.list && _gridSelected != null:
        _moveGrid(apps.length, -1);
      case LogicalKeyboardKey.enter || LogicalKeyboardKey.numpadEnter:
        if (!widget.list && _gridSelected != null && apps.isNotEmpty) {
          widget.onLaunch(apps[_gridSelected!.clamp(0, apps.length - 1)]);
        } else if (widget.list && apps.isNotEmpty) {
          widget.onLaunch(apps[_selected.clamp(0, apps.length - 1)]);
        } else if (_query.text.trim().isNotEmpty && apps.isNotEmpty) {
          widget.onLaunch(apps.first);
        }
      default:
        return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  /// Moves the grid's keyboard choice by [delta] (a row is [gridColumns]); it stays inside the
  /// apps and the grid scrolls to keep it in view. The first Down chooses the first icon.
  void _moveGrid(int count, int delta) {
    final next = ((_gridSelected ?? 0) + delta).clamp(0, count - 1);
    setState(() => _gridSelected = next);
    if (!_scroll.hasClients) return;
    final layout = _layout;
    if (layout == null) return;
    final top = layout.cell(next).dy;
    final bottom = top + gridLabelTop + gridLabelHeight + 12;
    final view = _scroll.position.viewportDimension;
    final max = _scroll.position.maxScrollExtent;
    if (top - 12 < _scroll.offset) {
      _scroll.jumpTo((top - 12).clamp(0, max));
    } else if (bottom > _scroll.offset + view) {
      _scroll.jumpTo((bottom - view).clamp(0, max));
    }
  }

  /// Grid opacity after a category change (a gap of ~75 ms, then ~155 ms).
  double _gridOpacity() => categoryFade(_fade.value);

  FaroColors get _c => FaroColors.of(context);

  @override
  Widget build(BuildContext context) {
    final list = widget.list;
    final size = launcherSize(list: list);
    final searching = _query.text.trim().isNotEmpty;
    final apps = launcherApps(
      widget.entries.values,
      category: _category,
      query: _query.text,
    );
    final recents = [
      if (!searching && _category == null)
        for (final id in widget.recents.take(list ? listRecents : gridColumns))
          if (widget.entries[id] case final entry?) entry,
    ];
    final browsing = !searching && _category == null;
    final layout = GridLayout(
      recents: recents.length,
      apps: apps.length,
      list: list,
      browsing: browsing,
    );
    _layout = layout;
    final areaTop = layout.areaTop;
    final areaHeight = size.height - areaTop;
    return SizedBox(
      width: size.width,
      height: size.height,
      child: Stack(
        children: [
          Positioned(
            left: 0,
            top: 0,
            right: 0,
            height: launcherHeader,
            child: _header(apps, size.width),
          ),
          Positioned(
            left: launcherMargin,
            right: launcherMargin,
            top: launcherHeader,
            height: 1,
            child: ColoredBox(color: _c.separator),
          ),
          if (browsing)
            Positioned(
              left: launcherMargin,
              top: categoryTop,
              right: launcherMargin,
              height: categoryHeight,
              child: _categories(),
            ),
          Positioned(
            left: 0,
            top: areaTop,
            right: 0,
            height: areaHeight,
            child: Opacity(
              opacity: _gridOpacity(),
              child: _area(
                apps,
                recents,
                layout,
                areaHeight,
                ringIndex: list
                    ? null
                    : _gridSelected ?? (_completion(apps) != null ? 0 : null),
              ),
            ),
          ),
          // With the grid scrolled a second separator appears above the area.
          if (browsing && _scroll.hasClients && _scroll.offset > 0)
            Positioned(
              left: launcherMargin,
              right: launcherMargin,
              top: areaTop - 1,
              height: 1,
              child: ColoredBox(color: _c.separator),
            ),
          // Where the view menu's popup is measured and put (see `_popup`).
          Positioned.fill(
            child: IgnorePointer(child: Overlay(key: _overlay)),
          ),
          if (_menu) ...[
            Positioned.fill(
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: _closeMenu,
              ),
            ),
            Positioned(
              right: 20,
              top: 44,
              child: AmalfiMenu(
                key: const ValueKey('launcher view menu'),
                autofocus: true,
                // Above the panel: Ossido merges only glass of one group, and a menu in the
                // panel's group melts into it (transparent, silica-desktop#209).
                group: faroMenuGroup,
                onDismiss: _closeMenu,
                entries: _menuEntries(list),
              ),
            ),
          ],
        ],
      ),
    );
  }

  String? _completion(List<DesktopEntry> apps) =>
      _category == null ? completionFor(_query.text, apps) : null;

  Widget _header(List<DesktopEntry> apps, double width) {
    final token = _category;
    // A keyboard choice in the grid shows as "<name> — Open" (PA-60); a typed query keeps its
    // own completion.
    final chosen = _gridSelected;
    final choice =
        !widget.list &&
            chosen != null &&
            chosen < apps.length &&
            _query.text.isEmpty
        ? apps[chosen]
        : null;
    final completion = choice?.name ?? _completion(apps);
    final fieldStyle = AmalfiType.field(color: _c.text);
    final typed = completion == null
        ? 0.0
        : (TextPainter(
            text: TextSpan(text: _query.text, style: fieldStyle),
            textDirection: TextDirection.ltr,
          )..layout()).width;
    final preview = completion == null ? null : iconOf(choice ?? apps.first);
    return Focus(
      onKeyEvent: (node, event) => _key(node, event, apps),
      child: Stack(
        children: [
          // The apps glyph goes back to Faro; with a category it is the back chevron.
          Positioned(
            left: 20,
            top: 16,
            width: 24,
            height: 24,
            child: GestureDetector(
              key: const ValueKey('launcher back'),
              behavior: HitTestBehavior.opaque,
              onTap: token == null ? widget.onBack : () => chooseCategory(null),
              child: AmalfiSymbol(
                token == null ? 'square-grid-2x2' : 'chevron-left',
                size: 24,
                color: _c.subtitle,
              ),
            ),
          ),
          Positioned(
            left: 56,
            top: 0,
            bottom: 0,
            right: preview == null ? 64 : 64 + previewSide + 20,
            child: Row(
              children: [
                if (token != null) ...[
                  Container(
                    key: const ValueKey('launcher category token'),
                    height: tokenHeight,
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: _c.selection,
                      borderRadius: BorderRadius.circular(tokenRadius),
                    ),
                    child: Text(
                      categoryLabel(l10nOf(context), token),
                      style: AmalfiType.title3(color: _c.text),
                    ),
                  ),
                  const SizedBox(width: 8),
                ],
                Expanded(
                  child: Stack(
                    alignment: Alignment.centerLeft,
                    children: [
                      if (_query.text.isEmpty && token == null)
                        IgnorePointer(
                          child: Text(
                            l10nOf(context).launcherPlaceholder,
                            maxLines: 1,
                            style: AmalfiType.field(color: _c.placeholder),
                          ),
                        ),
                      if (completion != null)
                        Positioned(
                          left: typed - 2,
                          top: 0,
                          bottom: 0,
                          child: IgnorePointer(
                            child: Center(
                              child: Container(
                                height: completionHeight,
                                padding: const EdgeInsets.only(
                                  left: 4,
                                  right: 1,
                                ),
                                alignment: Alignment.center,
                                decoration: BoxDecoration(
                                  color: _c.completion,
                                  borderRadius: BorderRadius.circular(
                                    completionRadius,
                                  ),
                                ),
                                child: Text(
                                  l10nOf(context).completionOpen(completion),
                                  maxLines: 1,
                                  style: fieldStyle,
                                ),
                              ),
                            ),
                          ),
                        ),
                      EditableText(
                        controller: _query,
                        focusNode: _focus,
                        style: fieldStyle,
                        cursorColor: _c.text,
                        cursorWidth: 2,
                        cursorHeight: 28,
                        backgroundCursorColor: const Color(0x00000000),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
          if (preview != null)
            Positioned(
              right: 64,
              top: 14,
              width: previewSide,
              height: previewSide,
              child: SilicaIcon(preview, side: previewSide),
            ),
          // "…": the view menu (grid or list), 20 pt, 24 from the right edge.
          Positioned(
            left: width - 44,
            top: 18,
            width: 20,
            height: 20,
            child: GestureDetector(
              key: const ValueKey('launcher more'),
              behavior: HitTestBehavior.opaque,
              onTap: _toggleMenu,
              child: AmalfiSymbol('ellipsis', size: 20, color: _c.subtitle),
            ),
          ),
        ],
      ),
    );
  }

  Widget _categories() {
    // Capsules at their natural width; more than fit (other languages, list mode) scroll
    // sideways.
    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final (i, name) in categoryNames.indexed) ...[
            if (i > 0) const SizedBox(width: categoryGap),
            GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => chooseCategory(name),
              child: Container(
                height: categoryHeight,
                padding: const EdgeInsets.symmetric(
                  horizontal: categoryPadding,
                ),
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: _c.pill,
                  borderRadius: BorderRadius.circular(categoryRadius),
                ),
                child: Text(
                  categoryLabel(l10nOf(context), name),
                  maxLines: 1,
                  style: AmalfiType.callout(color: _c.text),
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }

  Widget _area(
    List<DesktopEntry> apps,
    List<DesktopEntry> recents,
    GridLayout layout,
    double areaHeight, {
    required int? ringIndex,
  }) {
    final children = <Widget>[
      for (final (i, entry) in recents.indexed)
        _cell(entry, layout.recentCell(i), ring: false),
      if (recents.isNotEmpty)
        Positioned(
          left: launcherMargin,
          top: layout.separatorY,
          right: launcherMargin,
          height: 1,
          child: ColoredBox(color: _c.separator),
        ),
      for (final (i, entry) in apps.indexed)
        if (widget.list)
          _row(entry, layout.rowY(i), i == _selected, i)
        else
          _cell(entry, layout.cell(i), ring: i == ringIndex),
    ];
    return AmalfiScrollbar(
      child: SingleChildScrollView(
        controller: _scroll,
        child: SizedBox(
          height: layout.height,
          child: Stack(clipBehavior: Clip.none, children: children),
        ),
      ),
    );
  }

  /// A row of the list mode (`Apps/Faro/Row One Image`): icon 32 at x 20, title at x 64,
  /// the chosen row inset 8, radius 16.
  Widget _row(DesktopEntry entry, double top, bool selected, int index) {
    final icon = iconOf(entry);
    return Positioned(
      left: 0,
      right: 0,
      top: top,
      height: listRowHeight,
      child: MouseRegion(
        onEnter: (_) => setState(() => _selected = index),
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: () => widget.onLaunch(entry),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8),
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: selected ? _c.selection : null,
                borderRadius: BorderRadius.circular(16),
              ),
              child: Row(
                children: [
                  const SizedBox(width: 12),
                  SizedBox(
                    width: 32,
                    height: 32,
                    child: icon == null ? null : SilicaIcon(icon, side: 32),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      entry.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: AmalfiType.bodyEmphasis(color: _c.text),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _cell(DesktopEntry entry, Offset at, {required bool ring}) {
    final icon = iconOf(entry);
    final cell = widget.list ? listCellWidth : cellWidth;
    return Positioned(
      left: at.dx,
      top: at.dy,
      width: cell,
      height: gridLabelTop + gridLabelHeight,
      child: GestureDetector(
        // The whole cell: an image icon is not hit-testable by itself, and a click on it
        // would fall through to the backdrop and close the panel without opening the app.
        behavior: HitTestBehavior.opaque,
        onTap: () => widget.onLaunch(entry),
        child: Stack(
          clipBehavior: Clip.none,
          children: [
            if (ring)
              Positioned(
                left: gridIconX - ringWidth,
                top: -ringWidth,
                width: gridIcon + 2 * ringWidth,
                height: gridIcon + 2 * ringWidth,
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    border: Border.all(color: _c.selection, width: ringWidth),
                    borderRadius: BorderRadius.circular(ringRadius),
                  ),
                ),
              ),
            Positioned(
              left: gridIconX,
              top: 0,
              width: gridIcon,
              height: gridIcon,
              child: icon == null
                  ? const SizedBox()
                  : SilicaIcon(icon, side: gridIcon),
            ),
            Positioned(
              left: 0,
              top: gridLabelTop,
              width: cellWidth,
              height: gridLabelHeight,
              child: Text(
                entry.name,
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: AmalfiType.callout(color: _c.text),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Grid opacity `ms` after the category change (measured on the video).
double categoryFade(double ms) {
  const table = [
    (75.0, 0.0),
    (80.0, 0.059),
    (107.0, 0.322),
    (120.0, 0.483),
    (134.0, 0.616),
    (147.0, 0.731),
    (174.0, 0.869),
    (187.0, 0.901),
    (200.0, 0.934),
    (227.0, 0.986),
    (240.0, 0.994),
    (254.0, 1.0),
  ];
  if (ms <= table.first.$1) return 0;
  for (var i = 1; i < table.length; i++) {
    final (t1, a1) = table[i];
    if (ms <= t1) {
      final (t0, a0) = table[i - 1];
      return a0 + (a1 - a0) * (ms - t0) / (t1 - t0);
    }
  }
  return 1;
}

/// Where the cells and rows are, in the scroll area (below the header, or below the
/// category capsules when browsing). All values are those of the kit masters, in panel
/// coordinates, shifted by the area's top.
class GridLayout {
  const GridLayout({
    required this.recents,
    required this.apps,
    this.list = false,
    this.browsing = true,
  });

  final int recents;
  final int apps;
  final bool list;

  /// No category and no text: the capsules show and the recents come first.
  final bool browsing;

  /// The area's top in the panel: below the capsules when browsing, else below the
  /// separator.
  double get areaTop =>
      browsing ? categoryTop + categoryHeight : launcherHeader + 1;

  double get _recentTop => list ? listRecentTop : browsingIconTop;

  /// The separator under the recents.
  double get separatorY => (list ? listSeparatorY : recentSeparatorY) - areaTop;

  /// Top of the first icon of the grid, or of the first row of the list (panel coordinates).
  double get _listTop {
    if (list) {
      return !browsing
          ? listPlainTop
          : recents == 0
          ? _recentTop
          : listRowsTop;
    }
    return !browsing
        ? plainIconTop
        : recents == 0
        ? browsingIconTop
        : recentListTop;
  }

  Offset recentCell(int i) => Offset(
    list ? launcherMargin + i * listCellWidth : cellOriginX + i * cellWidth,
    _recentTop - areaTop,
  );

  Offset cell(int i) => Offset(
    cellOriginX + (i % gridColumns) * cellWidth,
    _listTop - areaTop + (i ~/ gridColumns) * rowPitch,
  );

  double rowY(int i) => _listTop - areaTop + i * listRowHeight;

  double get height {
    if (list) return rowY(apps) + 8;
    final rows = (apps + gridColumns - 1) ~/ gridColumns;
    final last = rows == 0 ? _listTop : _listTop + (rows - 1) * rowPitch;
    return last - areaTop + gridLabelTop + gridLabelHeight + 24;
  }
}
