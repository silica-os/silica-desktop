// The Dock's folder stacks (plan/reference/dock-stacks.md, misc.md "Folder stack").
//
// Automatic view: fan up to 15 files, grid beyond (the real threshold is between 11 and 40,
// not measured).
// - Fan (DOCK-FAN): from the bottom, 64 pt icons with their centres 64 pt apart vertically,
//   each rotated clockwise ~1.22° more than the one below (~1.6° for the first) and shifted
//   right by 64 · tan of its angle from the previous centre, so the column bends more and more
//   to the right; 28 pt label capsules on the left, rotated with their icon, 5.5 pt from its
//   frame; the first item's frame ends 7 pt above the Dock area; at most 15 files, the
//   sixteenth place opens the folder (a flat 58 pt circle). Opening ~240 ms (the column comes
//   out of the icon and stretches, then the labels), closing ~210 ms with Esc, ~300 ms with the
//   close button.
// - Grid: a panel with the title and 128 pt cells, ⌈√(n + 1)⌉ columns, margins 17 / 16 at
//   the sides, 32 at the top (title), 11 at the bottom, radius 25, a ~24 x 8 pt tip towards the
//   icon, its apex 5 pt above the Dock; it scales up from the tip in ~240 ms, closes in ~150 ms.
// A click on a file closes the stack and opens it.

import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../faro/faro.dart' show OuterShadow;
import '../l10n/l10n.dart';
import '../system/glass_profile.dart';
import '../system/silica_symbol.dart';
import 'dock.dart' show DockLabel, pressedShade;
import 'model.dart' show defaultCanvas, dockIconRevision;
import 'glass_groups.dart';
import 'stack_settings.dart';

const int fanMax = 15;

/// The fan's icons (their bounding box grows with the rotation: 64 · (cos θ + sin θ)).
const double fanIcon = 64;

/// Vertical distance between consecutive centres.
const double fanPitch = 64;

/// The first item's rotation and the growth per item (degrees, clockwise).
const double fanFirstAngle = 1.6;
const double fanAngleStep = 1.22;

/// The first item's frame ends this far above the Dock area.
const double fanAboveZone = 7;

/// The label capsules: height, side padding around the 13 pt name, distance from the icon's
/// frame (its bounding box) along the row.
const double fanLabelHeight = 24;
const double fanLabelPadding = 8;
const double fanLabelGap = 5.5;

/// The fan's "open folder" item: a flat circle, its outline and the glyph (a forward curved
/// arrow, ~28 x 17 pt), light and dark; pressed, only the glyph dims (214 -> 140 in dark).
const double fanFolderCircle = 58;
const Color fanFolderFill = Color(0xFFBCBDC0);
const Color fanFolderFillDark = Color(0xFF595A5D);
const Color fanFolderOutline = Color(0xFF666666);
const Color fanFolderOutlineDark = Color(0xFF000000);
const Color fanFolderGlyph = Color(0xFF3E3E3E);
const Color fanFolderGlyphDark = Color(0xFFD6D6D6);
const double fanFolderPressedGlyph = 140 / 214;

const double gridCell = 128;

/// The icon in the cell: folder 92 × 74 19 pt from the top (a 92 box from y 10).
const double gridIcon = 92;
const double gridIconTop = 10;

/// The icon canvas in the cell (silica-icons 0.2: a file's side is the canvas): 100 pt from
/// y 6, so the folder (0.922 × 0.734 of the canvas, top at 132/1024) is again 92 × 74 at 19 pt.
const double gridIconCanvas = 100;
const double gridCanvasTop = 6;

/// The label: one line, capitals from y 109 (baseline at ~118), at most ~104 wide.
const double gridLabelBaseline = 118;
const double gridLabelWidth = 104;

/// The file names under the icons: Caption; the colour comes from the grid's
/// [DefaultTextStyle] (`Text/Primary` of the appearance).
TextStyle get gridLabelStyle => AmalfiType.caption();

/// The tip towards the icon (DOCK-FAN): ~24 pt at the base, 8 pt deep from the panel's edge to
/// the apex (5 pt above the Dock), 45° flanks with small fillets and a rounded apex; Ossido fits
/// its outline to this rectangle.
const double gridTipBase = AmalfiPopoverMetrics.arrowHalfBase * 2;
const double gridTipDepth = AmalfiPopoverMetrics.arrowDepth;

const double gridLeft = 17;

/// 16 pt to the edge plus 1 of rim: 2 columns make 290 pt, 4 make 546.
const double gridRight = 17;
const double gridTop = 32;
const double gridBottom = 11;

/// The panel is a popover on Glass/Thick: radius 16, arrow 20 wide and 8 deep (kit `Popovers`,
/// `AmalfiPopoverMetrics`); the cell geometry below is the Dock's own (derived, not in the kit).
const double gridRadius = AmalfiPopoverMetrics.radius;
const double gridTipHeight = gridTipDepth;
const double gridTipAboveDock = 8;

const Duration stackOpen = Duration(milliseconds: 240);
const Duration fanClose = Duration(milliseconds: 210);
const Duration gridClose = Duration(milliseconds: 150);

/// Released on the close button, the stack closes in ~300 ms (fan and grid).
const Duration stackCloseByButton = Duration(milliseconds: 300);

bool _dark(BuildContext context) =>
    MediaQuery.maybePlatformBrightnessOf(context) == Brightness.dark;

enum StackView { fan, grid }

/// Automatic view for `count` files.
StackView automaticView(int count) =>
    count <= fanMax ? StackView.fan : StackView.grid;

/// The files of the folder without the hidden ones, in the chosen order: by name; by date from
/// the most recent (like download stacks); by kind (extension, then name). On Linux the "date
/// added" is the status change (ctime: moving or copying a file into the folder updates it)
/// and there is no reliable creation date: the modification is used.
List<FileSystemEntity> stackEntries(
  String path, {
  StackSort sort = StackSort.name,
}) {
  try {
    final list = Directory(path)
        .listSync()
        .where(
          (e) => !e.uri.pathSegments
              .where((s) => s.isNotEmpty)
              .last
              .startsWith('.'),
        )
        .toList();
    int byName(FileSystemEntity a, FileSystemEntity b) =>
        naturalCompare(fileName(a.path), fileName(b.path));
    final stats = {for (final e in list) e.path: e.statSync()};
    DateTime when(FileSystemEntity e) {
      final stat = stats[e.path];
      if (stat == null) return DateTime(0);
      return sort == StackSort.added ? stat.changed : stat.modified;
    }

    String extension(FileSystemEntity e) {
      if (e is Directory) return '';
      final name = fileName(e.path);
      final dot = name.lastIndexOf('.');
      return dot <= 0 ? '' : name.substring(dot + 1).toLowerCase();
    }

    list.sort(
      (a, b) => switch (sort) {
        StackSort.name => byName(a, b),
        StackSort.kind => () {
          final c = extension(a).compareTo(extension(b));
          return c != 0 ? c : byName(a, b);
        }(),
        _ => () {
          final c = when(b).compareTo(when(a));
          return c != 0 ? c : byName(a, b);
        }(),
      },
    );
    return list;
  } on FileSystemException {
    return const [];
  }
}

/// "Natural" comparison: digits count as numbers (document-2 before document-10),
/// case-insensitive.
int naturalCompare(String a, String b) {
  final parts = RegExp(r'\d+|\D+');
  final pa = [for (final m in parts.allMatches(a.toLowerCase())) m[0] ?? ''];
  final pb = [for (final m in parts.allMatches(b.toLowerCase())) m[0] ?? ''];
  for (var i = 0; i < pa.length && i < pb.length; i++) {
    final na = int.tryParse(pa[i]);
    final nb = int.tryParse(pb[i]);
    final c = na != null && nb != null
        ? na.compareTo(nb)
        : pa[i].compareTo(pb[i]);
    if (c != 0) return c;
  }
  return pa.length.compareTo(pb.length);
}

String fileName(String path) {
  final parts = path.split('/').where((p) => p.isNotEmpty);
  return parts.isEmpty ? path : parts.last;
}

/// The icon of a file or a folder: the system composes it (`Thumbnails1.Icon`: kind or handling
/// app, folder colour and symbol, special folders). No badges in the Dock (icon-format.md).
IconSource? fileIcon(FileSystemEntity entry) => ServiceSource(
  Uri.file(entry.path),
  isFolder: entry is Directory,
  revision: dockIconRevision,
);

/// The fan geometry for item `i` (from the bottom): the icon's centre, relative to the centre
/// of the stack icon in x and to the top of the Dock area in y; its side ([fanIcon]); its
/// rotation (radians, clockwise); its frame, the bounding box of the rotated icon.
({Offset center, double side, double angle, double frame}) fanSlot(int i) {
  double angle(int k) => (fanFirstAngle + fanAngleStep * k) * math.pi / 180;
  double frame(int k) => fanIcon * (math.cos(angle(k)) + math.sin(angle(k)));
  // Each centre sits on the tangent of the one below, 64 · tan θ further right: the first from
  // the stack icon's centre (+1.8 pt), the next ones with the angle between the two items
  // (the measured steps 2, 4, 5, 7, 8, 9, 11, 13, 13, 15 pt lie between their two angles).
  var x = fanPitch * math.tan(angle(0));
  for (var k = 1; k <= i; k++) {
    x += fanPitch * math.tan((angle(k - 1) + angle(k)) / 2);
  }
  return (
    center: Offset(x, -fanAboveZone - frame(0) / 2 - fanPitch * i),
    side: fanIcon,
    angle: angle(i),
    frame: frame(i),
  );
}

/// Grid columns and rows for `count` files (+1 to open the folder).
(int, int) gridShape(int count) {
  final cells = count + 1;
  final columns = math.sqrt(cells).ceil().clamp(1, 12);
  return (columns, (cells / columns).ceil());
}

Size gridPanelSize(int count, double maxHeight) {
  final (columns, rows) = gridShape(count);
  final height = gridTop + rows * gridCell + gridBottom;
  return Size(
    gridLeft + columns * gridCell + gridRight,
    // Never a reversed range: with little room the minimum row wins.
    height.clamp(
      gridTop + gridCell + gridBottom,
      math.max(gridTop + gridCell + gridBottom, maxHeight),
    ),
  );
}

/// Lets a stack item be dragged out (SH-DK-27): past [stackDragThreshold] the stack closes and
/// the drag of the file goes on as a system drag (onto the desktop, a window, the Dock).
class StackDragSource extends StatefulWidget {
  const StackDragSource({
    super.key,
    required this.path,
    required this.onDrag,
    required this.child,
  });

  final String path;

  /// Called once, when the pointer has moved past the threshold.
  final ValueChanged<String>? onDrag;
  final Widget child;

  @override
  State<StackDragSource> createState() => _StackDragSourceState();
}

const double stackDragThreshold = 5;

class _StackDragSourceState extends State<StackDragSource> {
  Offset? _from;

  @override
  Widget build(BuildContext context) => Listener(
    onPointerDown: (e) => _from = e.position,
    onPointerMove: (e) {
      final from = _from;
      if (from == null || (e.position - from).distance < stackDragThreshold) {
        return;
      }
      _from = null;
      widget.onDrag?.call(widget.path);
    },
    onPointerUp: (_) => _from = null,
    onPointerCancel: (_) => _from = null,
    child: widget.child,
  );
}

/// The fan above the icon: `anchor` is the center of the icon in x and the top of the Dock
/// area in y; `progress` 0–1 opens and closes.
///
/// Every frame of the opening only moves, turns and fades the items: their contents (icon,
/// label with its glass) are built once per stack and kept as the same widgets, each behind
/// its own repaint boundary, so a frame neither resolves icons nor measures names again.
class FanStack extends StatefulWidget {
  const FanStack({
    super.key,
    required this.entries,
    required this.anchor,
    required this.progress,
    required this.onOpen,
    required this.onOpenFolder,
    this.onDragOut,
  });

  final List<FileSystemEntity> entries;
  final Offset anchor;
  final double progress;
  final ValueChanged<FileSystemEntity> onOpen;
  final VoidCallback onOpenFolder;

  /// A file or folder of the stack was dragged out: the Dock closes the stack and starts the
  /// system drag.
  final ValueChanged<String>? onDragOut;

  @override
  State<FanStack> createState() => _FanStackState();
}

class _FanStackState extends State<FanStack> {
  /// While the stack moves its contents are pictures taken once ([SnapshotWidget]): a frame
  /// of the opening or closing draws images, not the icons' vector layers and blurred ground
  /// shadows again at every new position and scale. At rest they are drawn live.
  final SnapshotController _snapshot = SnapshotController();

  @override
  void dispose() {
    _snapshot.dispose();
    super.dispose();
  }

  /// Per slot: the label and the icon, built once for these entries and this appearance.
  List<(Widget, Widget)> _items = const [];
  Object? _itemsKey;

  List<(Widget, Widget)> _contents(BuildContext context) {
    final dark = _dark(context);
    final l10n = l10nOf(context);
    final text = AmalfiTheme.of(context).colors.textPrimary;
    final key = Object.hash(identityHashCode(widget.entries), dark, l10n, text);
    if (key == _itemsKey) return _items;
    _itemsKey = key;
    final shown = widget.entries.take(fanMax).toList();
    final more = widget.entries.length - shown.length;
    return _items = [
      for (var i = 0; i <= shown.length; i++)
        () {
          final side = fanSlot(i).side;
          final entry = i < shown.length ? shown[i] : null;
          final label = entry == null
              ? (more > 0 ? l10n.stackMoreInFolder(more) : l10n.openFolder)
              : fileName(entry.path);
          final icon = entry == null ? null : fileIcon(entry);
          return (
            RepaintBoundary(
              child: SnapshotWidget(
                controller: _snapshot,
                child: _label(label, text, i),
              ),
            ),
            RepaintBoundary(
              child: SnapshotWidget(
                controller: _snapshot,
                child: SizedBox(
                  key: ValueKey('fan icon $i'),
                  width: side,
                  height: side,
                  child: entry == null
                      ? FanFolderButton(side: side, dark: dark)
                      : icon == null
                      ? const SizedBox()
                      : SilicaIcon(icon, side: side, dark: false),
                ),
              ),
            ),
          );
        }(),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final items = _contents(context);
    final shown = math.min(widget.entries.length, fanMax);
    final progress = widget.progress;
    _snapshot.allowSnapshotting = progress < 1;
    final fade = progress.clamp(0.0, 1.0);
    final labels = fade * ((progress - 0.7) / 0.3).clamp(0.0, 1.0);
    final children = <Widget>[];
    for (var i = 0; i <= shown && i < items.length; i++) {
      final slot = fanSlot(i);
      // The column comes out of the icon: the pitch grows with the progress.
      final center =
          widget.anchor +
          Offset(slot.center.dx * progress, slot.center.dy * progress);
      final side = slot.side;
      final entry = i < shown ? widget.entries[i] : null;
      final (label, icon) = items[i];
      children.add(
        Positioned(
          left: center.dx - side / 2 - 400,
          top: center.dy - side / 2,
          width: side + 400,
          height: side,
          child: Transform.rotate(
            angle: slot.angle * progress,
            alignment: Alignment(1 - side / (side + 400), 0),
            child: StackDragSource(
              path: entry?.path ?? '',
              onDrag: entry == null ? null : widget.onDragOut,
              child: GestureDetector(
                behavior: HitTestBehavior.translucent,
                // The current callbacks, not those of the build that made the contents.
                onTap: () => entry == null
                    ? widget.onOpenFolder()
                    : widget.onOpen(entry),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    // Each item fades on its own: a single layer over the whole fan would be
                    // as large as the column on every frame.
                    Opacity(opacity: labels, child: label),
                    // 5.5 pt from the icon's frame, the bounding box of the rotated icon.
                    SizedBox(width: fanLabelGap + (slot.frame - side) / 2),
                    Opacity(opacity: fade, child: icon),
                  ],
                ),
              ),
            ),
          ),
        ),
      );
    }
    return Stack(clipBehavior: Clip.none, children: children);
  }

  /// The item name: a capsule of the Dock label's glass (`thick`), [fanLabelHeight] tall, as
  /// wide as the 13 pt name plus [fanLabelPadding] at each end (at most 360 pt), without the
  /// tip and without a shadow, in a glass group of its own.
  Widget _label(String text, Color color, int index) {
    final style = AmalfiType.callout(color: color);
    final painter = TextPainter(
      text: TextSpan(text: text, style: style),
      textDirection: TextDirection.ltr,
      maxLines: 1,
    )..layout();
    final width = math.min(
      360.0,
      painter.width.ceilToDouble() + 2 * fanLabelPadding,
    );
    painter.dispose();
    return SizedBox(
      width: width,
      height: fanLabelHeight,
      child: DockLabel(
        text: text,
        style: style.copyWith(overflow: TextOverflow.ellipsis),
        height: fanLabelHeight,
        shadow: false,
        group: DockGlass.fanLabel(index),
      ),
    );
  }
}

/// The fan's "open folder" item (dock-stacks.md, DOCK-FAN): a flat [fanFolderCircle] circle
/// with a 1 pt outline and the forward curved arrow, centred in the `side` slot. No hover;
/// while pressed only the glyph dims.
class FanFolderButton extends StatefulWidget {
  const FanFolderButton({super.key, required this.side, this.dark = false});

  final double side;
  final bool dark;

  @override
  State<FanFolderButton> createState() => _FanFolderButtonState();
}

class _FanFolderButtonState extends State<FanFolderButton> {
  bool _pressed = false;

  void _press(bool pressed) {
    if (pressed != _pressed) setState(() => _pressed = pressed);
  }

  @override
  Widget build(BuildContext context) {
    final dark = widget.dark;
    final glyph = dark ? fanFolderGlyphDark : fanFolderGlyph;
    return Listener(
      onPointerDown: (_) => _press(true),
      onPointerUp: (_) => _press(false),
      onPointerCancel: (_) => _press(false),
      child: Center(
        child: Container(
          width: fanFolderCircle,
          height: fanFolderCircle,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: dark ? fanFolderFillDark : fanFolderFill,
            border: Border.all(
              color: dark ? fanFolderOutlineDark : fanFolderOutline,
            ),
          ),
          alignment: Alignment.center,
          child: FolderArrow(
            color: _pressed
                ? glyph.withValues(alpha: fanFolderPressedGlyph)
                : glyph,
          ),
        ),
      ),
    );
  }
}

/// The forward curved arrow of the "open folder" items: silica-icons' `reply` mirrored,
/// its 16.25 x 12 drawing scaled to the measured ~28 pt width.
class FolderArrow extends StatelessWidget {
  const FolderArrow({super.key, required this.color, this.width = 28});

  final Color color;

  /// The drawing's width (its box is wider: the symbol fills 16.25 / 24 of it).
  final double width;

  static const String symbol = 'reply';

  @override
  Widget build(BuildContext context) => Transform.flip(
    flipX: true,
    child: SilicaSymbol(symbol, size: width * 24 / 16.25, color: color),
  );
}

/// The silica-icons "Stack Close" badge (`badges.json` > `dock` > `stack-close`): the PNG
/// rendered for the light or the dark appearance, or null if silica-icons has none.
String? stackCloseBadgePath({required bool dark, Map<String, String>? env}) {
  for (final base in silicaIconDirs(env ?? Platform.environment)) {
    try {
      final manifest = jsonDecode(
        File('$base/badges/badges.json').readAsStringSync(),
      );
      final dock = manifest is Map ? manifest['dock'] : null;
      final badge = dock is Map ? dock['stack-close'] : null;
      final rendered = badge is Map ? badge['rendered'] : null;
      final name = rendered is Map ? rendered[dark ? 'dark' : 'default'] : null;
      if (name is! String || name.contains('/') || name.contains('..')) {
        continue;
      }
      final file = File('$base/badges/$name');
      if (file.existsSync()) return file.path;
    } on FileSystemException {
      continue;
    } on FormatException {
      continue;
    }
  }
  return null;
}

/// What the stack's Dock icon becomes while it is open (fan or grid): the silica-icons
/// "Stack Close" badge on the folder icon's canvas (`side`), not a shell-drawn cross. Pressed,
/// it darkens like the other icons.
class StackCloseButton extends StatelessWidget {
  const StackCloseButton({
    super.key,
    required this.side,
    required this.dark,
    this.pressed = false,
    this.env,
  });

  final double side;
  final bool dark;
  final bool pressed;

  /// The environment for finding silica-icons (tests).
  final Map<String, String>? env;

  @override
  Widget build(BuildContext context) {
    final path = stackCloseBadgePath(dark: dark, env: env);
    if (path == null) return SizedBox.square(dimension: side);
    final ratio = MediaQuery.maybeDevicePixelRatioOf(context) ?? 1;
    var decoded = 64;
    while (decoded < side * ratio && decoded < 1024) {
      decoded *= 2;
    }
    final ImageProvider file = FileImage(File(path));
    final image = Image(
      image: decoded < 1024
          ? ResizeImage(file, width: decoded, height: decoded)
          : file,
      width: side,
      height: side,
      fit: BoxFit.fill,
      filterQuality: FilterQuality.medium,
      gaplessPlayback: true,
      excludeFromSemantics: true,
      errorBuilder: (_, _, _) => SizedBox.square(dimension: side),
    );
    return pressed
        ? ColorFiltered(
            colorFilter: const ColorFilter.mode(
              pressedShade,
              BlendMode.srcATop,
            ),
            child: image,
          )
        : image;
  }
}

/// The grid's "open folder" cell (dock-stacks.md, DOCK-FAN): a stroked circle, no fill, with the
/// forward curved arrow. Measured in dark (white); light `estimated` with the fan item's glyph
/// colour. Not glass: nothing to merge with the panel or the menu.
class OpenFolderButton extends StatelessWidget {
  const OpenFolderButton({super.key, required this.side, this.dark = false});

  /// The box of the item (the circle is [diameterRatio] of it).
  final double side;
  final bool dark;

  static const double diameterRatio = 0.7;

  /// The circle's stroke (`provisional`, not measured).
  static const double stroke = 1.5;

  @override
  Widget build(BuildContext context) {
    final color = dark ? const Color(0xFFFFFFFF) : fanFolderGlyph;
    return Center(
      child: Container(
        width: side * diameterRatio,
        height: side * diameterRatio,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          border: Border.all(color: color, width: stroke),
        ),
        alignment: Alignment.center,
        child: FolderArrow(color: color, width: side * diameterRatio * 0.45),
      ),
    );
  }
}

/// The grid: a panel with the title, above the icon with the tip pointing at it.
/// `tip` is the apex of the tip (5 pt above the Dock).
///
/// Like the fan, the panel's contents (title, cells, names truncated to fit) are built once
/// per stack and kept behind a repaint boundary: a frame of the opening only scales and fades.
class GridStack extends StatefulWidget {
  const GridStack({
    super.key,
    required this.title,
    required this.entries,
    required this.rect,
    required this.tipX,
    required this.progress,
    required this.onOpen,
    required this.onOpenFolder,
    this.onDragOut,
  });

  final String title;
  final List<FileSystemEntity> entries;

  /// The panel body in the area.
  final Rect rect;

  /// The apex of the tip, from the left edge of the panel.
  final double tipX;
  final double progress;
  final ValueChanged<FileSystemEntity> onOpen;
  final VoidCallback onOpenFolder;

  /// A file or folder of the stack was dragged out: the Dock closes the stack and starts the
  /// system drag.
  final ValueChanged<String>? onDragOut;

  @override
  State<GridStack> createState() => _GridStackState();
}

class _GridStackState extends State<GridStack> {
  /// While the stack moves its contents are pictures taken once ([SnapshotWidget]): a frame
  /// of the opening or closing draws images, not the icons' vector layers and blurred ground
  /// shadows again at every new position and scale. At rest they are drawn live.
  final SnapshotController _snapshot = SnapshotController();

  @override
  void dispose() {
    _snapshot.dispose();
    super.dispose();
  }

  Widget? _content;
  Object? _contentKey;

  Widget _contents(BuildContext context) {
    final dark = _dark(context);
    final l10n = l10nOf(context);
    final key = Object.hash(
      identityHashCode(widget.entries),
      widget.title,
      dark,
      l10n,
    );
    final cached = _content;
    if (cached != null && key == _contentKey) return cached;
    _contentKey = key;
    return _content = RepaintBoundary(
      child: SnapshotWidget(
        controller: _snapshot,
        child: DefaultTextStyle.merge(
          style: TextStyle(color: AmalfiTheme.of(context).colors.textPrimary),
          child: Stack(
            children: [
              Positioned(
                left: 0,
                right: 0,
                top: 9,
                child: Text(
                  widget.title,
                  textAlign: TextAlign.center,
                  style: AmalfiType.callout(
                    color: AmalfiTheme.of(context).colors.textPrimary,
                  ).copyWith(fontWeight: FontWeight.w600),
                ),
              ),
              Positioned(
                left: gridLeft,
                right: gridRight,
                top: gridTop,
                bottom: gridBottom,
                child: AmalfiScrollbar(
                  child: SingleChildScrollView(
                    child: Wrap(
                      children: [
                        for (final entry in widget.entries) _cell(entry),
                        _openCell(l10n.openFolder, dark),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final rect = widget.rect;
    final tipX = widget.tipX;
    final p = widget.progress.clamp(0.0, 1.0);
    _snapshot.allowSnapshotting = widget.progress < 1;
    return Positioned.fromRect(
      rect: rect,
      child: Transform.scale(
        scale: 0.2 + 0.8 * p,
        alignment: Alignment(
          (tipX / rect.width) * 2 - 1,
          1 + 2 * gridTipHeight / rect.height,
        ),
        child: Opacity(
          opacity: p,
          child: OuterShadow(
            radius: gridRadius,
            blurRadius: 30,
            offset: const Offset(0, 8),
            child: ProfiledGlass(
              profile: ShellGlassProfile.panel,
              radius: gridRadius,
              material: GlassMaterial.thick,
              group: DockGlass.gridPanel,
              // The measured grid tip (gridTipBase x gridTipDepth): Ossido fits its outline to
              // this rectangle, the body edge 8 pt from the apex.
              tip: GlassTip(
                x: tipX.clamp(
                  gridRadius + gridTipBase / 2,
                  math.max(
                    gridRadius + gridTipBase / 2,
                    rect.width - gridRadius - gridTipBase / 2,
                  ),
                ),
                width: gridTipBase,
                height: gridTipDepth,
              ),
              child: _contents(context),
            ),
          ),
        ),
      ),
    );
  }

  /// A 128 cell (misc.md › Folder stack): the icon of ~92 centered with the top of the
  /// folder at 19 pt (box from y 10), the name on one line truncated in the middle with the
  /// capitals from y 109, `#212122`.
  Widget _cell(FileSystemEntity entry) {
    final icon = fileIcon(entry);
    return StackDragSource(
      path: entry.path,
      onDrag: widget.onDragOut,
      child: GestureDetector(
        // The current callback, not the one of the build that made the cell.
        onTap: () => widget.onOpen(entry),
        child: SizedBox(
          width: gridCell,
          height: gridCell,
          child: Stack(
            children: [
              Positioned(
                key: ValueKey('icon ${entry.path}'),
                left: (gridCell - gridIconCanvas) / 2,
                top: gridCanvasTop,
                width: gridIconCanvas,
                height: gridIconCanvas,
                child: icon == null
                    ? const SizedBox.shrink()
                    : SilicaIcon(icon, side: gridIconCanvas, dark: false),
              ),
              _name(
                middleEllipsis(
                  fileName(entry.path),
                  gridLabelStyle,
                  gridLabelWidth,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _name(String text) => Positioned(
    left: (gridCell - gridLabelWidth) / 2,
    width: gridLabelWidth,
    top: 0,
    child: Baseline(
      baseline: gridLabelBaseline,
      baselineType: TextBaseline.alphabetic,
      child: SizedBox(
        width: gridLabelWidth,
        child: Text(
          text,
          textAlign: TextAlign.center,
          maxLines: 1,
          softWrap: false,
          style: gridLabelStyle,
        ),
      ),
    ),
  );

  Widget _openCell(String label, bool dark) => GestureDetector(
    behavior: HitTestBehavior.opaque,
    onTap: () => widget.onOpenFolder(),
    child: SizedBox(
      width: gridCell,
      height: gridCell,
      child: Stack(
        children: [
          Positioned(
            left: (gridCell - gridIcon) / 2,
            top: gridIconTop,
            width: gridIcon,
            height: gridIcon,
            child: OpenFolderButton(side: gridIcon, dark: dark),
          ),
          _name(label),
        ],
      ),
    ),
  );
}

/// The open stack under a Dock menu (the menu opened on the stack's own icon). Ossido draws all
/// the glass of the Dock's surface under all of its content, so the menu's glass alone cannot
/// cover the stack's icons and names: the stack's content is cut out where the menu's panels
/// (`menu`, in the stack's coordinates) and its tip lie, and the menu's glass group, above the
/// stack's (glass_groups.dart), covers the stack's glass there. Without a menu nothing is cut.
class StackUnderMenu extends StatelessWidget {
  const StackUnderMenu({super.key, required this.menu, required this.child});

  /// The menu's panels and its tip.
  final List<Rect> menu;
  final Widget child;

  @override
  Widget build(BuildContext context) => ClipPath(
    // The same widgets with or without the menu: the stack keeps its state.
    clipBehavior: menu.isEmpty ? Clip.none : Clip.antiAlias,
    clipper: MenuHoleClipper(menu),
    child: child,
  );
}

/// Everything but the menu's panels (with the menus' corner radius) and its tip.
class MenuHoleClipper extends CustomClipper<Path> {
  const MenuHoleClipper(this.holes);

  final List<Rect> holes;

  @override
  Path getClip(Size size) {
    // The stack reaches outside its box (the fan's top items above the surface): far enough.
    final all = Path()..addRect((Offset.zero & size).inflate(4096));
    if (holes.isEmpty) return all;
    final cut = Path();
    for (final hole in holes) {
      cut.addRRect(
        RRect.fromRectAndRadius(
          hole,
          const Radius.circular(AmalfiMenuMetrics.radius),
        ),
      );
    }
    return Path.combine(PathOperation.difference, all, cut);
  }

  @override
  bool shouldReclip(MenuHoleClipper old) => !_same(old.holes, holes);

  static bool _same(List<Rect> a, List<Rect> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
}

/// The "stack" icon of a folder in the Dock (dock-stacks.md › The icon in the Dock): at most
/// three items of the folder, the first in front, the others behind and higher, no rotation,
/// no extra shadow. Each icon is drawn on a 50 pt canvas of the 64 × 80 tile (never the full
/// 60), at x = 7 and y = 17, 15, 13; everything scales by k = canvas / 60 and rests on whole
/// points.
class PileIcon extends StatelessWidget {
  const PileIcon({super.key, required this.sources, required this.side});

  /// From the first (in front) to the third.
  final List<IconSource> sources;

  /// The icon canvas of the tile (60 at the default size).
  final double side;

  /// The canvas of one item, its left edge and the top of the front, second and third, in
  /// tile points at the default size.
  static const double itemCanvas = 50;
  static const double itemLeft = 7;
  static const List<double> itemTops = [17, 15, 13];

  /// The frames of `count` items (at most 3, front first) in tile coordinates, for the icon
  /// canvas `canvas`. `rounded` snaps them to whole points (the resting Dock); magnified they
  /// scale smoothly.
  static List<Rect> frames(double canvas, int count, {bool rounded = true}) {
    final k = canvas / defaultCanvas;
    double snap(double v) => rounded ? v.roundToDouble() : v;
    final edge = snap(itemCanvas * k);
    return [
      for (var i = 0; i < math.min(count, 3); i++)
        Rect.fromLTWH(snap(itemLeft * k), snap(itemTops[i] * k), edge, edge),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final k = side / defaultCanvas;
    final rounded = side == side.roundToDouble();
    final origin = rounded
        ? Offset((2 * k).roundToDouble(), (10 * k).roundToDouble())
        : Offset(2 * k, 10 * k);
    final frames = PileIcon.frames(side, sources.length, rounded: rounded);
    return SizedBox(
      width: side,
      height: side,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          for (var i = frames.length - 1; i >= 0; i--)
            Positioned.fromRect(
              rect: frames[i].shift(-origin),
              child: SilicaIcon(sources[i], side: frames[i].width),
            ),
        ],
      ),
    );
  }
}

/// Truncates in the middle with "…" so that `text` fits in `width` (the grid labels).
String middleEllipsis(String text, TextStyle style, double width) {
  double measure(String s) {
    final painter = TextPainter(
      text: TextSpan(text: s, style: style),
      textDirection: TextDirection.ltr,
      maxLines: 1,
    )..layout();
    final w = painter.width;
    painter.dispose();
    return w;
  }

  if (measure(text) <= width) return text;
  final runes = text.runes.toList();
  var lo = 0;
  var hi = runes.length;
  var best = '…';
  while (lo <= hi) {
    final keep = (lo + hi) ~/ 2;
    final head = (keep + 1) ~/ 2;
    final tail = keep - head;
    final candidate =
        '${String.fromCharCodes(runes.take(head)).trimRight()}…'
        '${String.fromCharCodes(runes.skip(runes.length - tail)).trimLeft()}';
    if (measure(candidate) <= width) {
      best = candidate;
      lo = keep + 1;
    } else {
      hi = keep - 1;
    }
  }
  return best;
}
