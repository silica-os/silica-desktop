// Dock geometry and magnification, from plan/reference/dock.md (macOS 27, light appearance,
// sliders of the recordings: size 0.384, magnification 0.429). Everything in logical points,
// with the origin at the top left of the Dock surface (as wide as the screen, with its
// bottom edge on the screen edge).

import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/animation.dart' show Cubic;
import 'package:flutter/foundation.dart' show immutable;

import 'package:silica_ui/silica_ui.dart';

/// Pitch between cells: 64 pt (Amalfi, kit `Dock/Shelf/Default`: tiles 64 wide, gap 0).
const double cellPitch = 64;

/// The scale of the measured distances and amplitudes (magnification profile, lift, hops;
/// `motion.md` rule 3), from the 56 pt pitch they were first rescaled to; the timings are not.
const double amalfiScale = 56 / 63;

/// Width of the separator cell: 28 pt (kit `Dock/Separator/Line`: 28 x 80, the 1 pt line at
/// x 14, y 10, 60 tall), with no gap around it.
const double separatorCell = 28;

/// Shelf height: 80 pt (kit `Dock/Shelf/Default`, 620 x 80 at the default size).
const double backgroundHeight = 80;

/// Distance between the background and the bottom edge of the screen: 4 pt.
const double bottomGap = 4;

/// Corner radius of the shelf: 24 (kit `Dock/Shelf/Default`).
const double backgroundRadius = 24;

/// Margin between the cells and the shelf edge, at the sides: 8 pt (kit: 8 + 9 tiles of 64 + the
/// 28 pt separator + 8 = 620).
const double sidePadding = 8;

/// Distance from the top of the shelf to the icon canvas: 10 pt (kit tile: the 60 pt canvas is
/// at x 2, y 10 of the 64 x 80 tile).
const double iconTop = 10;

/// The side of the icon the badges were measured on (48 pt, with the canvas 4 pt below the top
/// of the old tile); the badges scale from it.
const double iconSize = 48;
const double badgeIconTop = 4;

/// Separator: a 1 pt line, 10 pt from the top and bottom of the shelf (kit `Dock/Separator`).
const double separatorHeight = 60;

/// Indicator of open apps: a 4 pt disc at y 72-76 of the tile (kit `Dock/Indicator/Dot`),
/// its centre 6 pt above the shelf's bottom edge.
const double indicatorSize = 4;
const double indicatorAbove = 6;

/// Canvas side of an app icon at rest and its limits (dock.json `tileSize`, Settings).
const double defaultCanvas = 60;
const double minCanvas = 16;
const double maxCanvas = 128;

/// The magnified canvas when dock.json has none (Settings: 65).
const double defaultMagnifiedCanvas = 65;

/// Whole points, inside [minCanvas, maxCanvas].
double clampCanvas(num value) => value.isFinite
    ? value.clamp(minCanvas, maxCanvas).roundToDouble()
    : defaultCanvas;

/// The active zone starts 5 pt above the background.
const double activeZoneAbove = 5;

/// Fall-off of the magnification: 1 under the pointer, a cosine down to 0 at 3 tiles
/// (`pitch` each) from it. The timing of entering and leaving ([enterCurve]) is measured.
// Assumption: not measured (motion.md): the shape of the fall-off.
double profile(double distance, [double pitch = cellPitch]) {
  final d = distance.abs() / (3 * pitch);
  return d >= 1 ? 0 : (1 + math.cos(math.pi * d)) / 2;
}

/// The geometry of one Dock from the side of the icon canvas (whole points, 16-128; the
/// default is 60). Everything scales by k = canvas / 60 around the kit's values (Dock masters,
/// Amalfi), rounded to whole points: tile `canvas + 4k` x `canvas + 20k`, the canvas at (2k, 10k),
/// no gap, padding 8k, radius 24k, separator 28k wide with its line 14k from the left and 10k
/// from the top and bottom, dot 4k with its centre 6k above the shelf's bottom, label offset 12k.
@immutable
class DockMetrics {
  const DockMetrics([this.canvas = defaultCanvas]);

  /// The side of the whole 1024 canvas of an app icon, in points.
  final double canvas;

  static const DockMetrics standard = DockMetrics();

  double get k => canvas / defaultCanvas;

  /// The margin around the canvas scales with it (2k at the sides, 10k above and below),
  /// rounded once per margin: the same relative place at every size, also while resizing.
  double get _marginX => (2 * k).roundToDouble();
  double get _marginY => (10 * k).roundToDouble();
  double get tileWidth => canvas + 2 * _marginX;
  double get tileHeight => canvas + 2 * _marginY;
  double get gap => 0;

  /// Distance between the centres of two tiles.
  double get pitch => tileWidth + gap;

  /// The shelf: its height is the tile's.
  double get backgroundHeight => tileHeight;
  double get shelfPadding => (8 * k).roundToDouble();

  /// Margin between the first or last cell and the shelf edge.
  double get sidePadding => shelfPadding;
  double get radius => (24 * k).roundToDouble();

  /// The canvas in its tile: its distance from the tile's left edge and from the shelf's top.
  /// Centred in the tile: the icon and the dot, which is centred in the tile too, stay on one
  /// axis at every size.
  double get iconLeft => _marginX;
  double get iconTop => _marginY;
  double get separatorWidth => (28 * k).roundToDouble();

  /// The separator cell: the item itself.
  double get separatorCell => separatorWidth;

  /// The separator's line: 10k from the top and bottom, 14k from the left.
  double get separatorInset => (10 * k).roundToDouble();
  double get separatorHeight => tileHeight - 2 * separatorInset;
  double get separatorLine => (14 * k).roundToDouble();
  double get dot => math.max(3, (4 * k).roundToDouble());

  /// The dot's centre above the shelf's bottom.
  double get dotAbove => (6 * k).roundToDouble();
  double get labelOffset => (12 * k).roundToDouble();

  /// Gap between the top of the icon's slot (a canvas of `side`, magnified or not) and the
  /// tip of the label or of the menu's arrow: the label ends [labelClearance] above where the
  /// icon's drawing starts (824 of a 1024 grid, centred), not above the shelf. Negative: the
  /// tip is that much below the top of the slot.
  double labelGapFor(double side) =>
      labelClearance - side * (1 - 824 / 1024) / 2;

  /// How far above the icon's drawing the label's bottom edge is.
  double get labelClearance => 8 * k;

  @override
  bool operator ==(Object other) =>
      other is DockMetrics && other.canvas == canvas;

  @override
  int get hashCode => canvas.hashCode;
}

/// The canvas of the icon at `distance` from the pointer, growing from the resting canvas to
/// `magnified` (cosine, over 3 tiles each way).
double magnifiedCanvas(DockMetrics m, double magnified, double distance) =>
    m.canvas + math.max(0, magnified - m.canvas) * profile(distance, m.pitch);

/// Curve of entering and leaving the active zone (equal within 0.01: here the mean of
/// the two), 3 cycles at maximum magnification: progress every 10 ms over ~160 ms.
const List<double> enterCurve = [
  0,
  0.001,
  0.017,
  0.067,
  0.112,
  0.206,
  0.294,
  0.386,
  0.481,
  0.578,
  0.672,
  0.759,
  0.837,
  0.902,
  0.955,
  0.988,
  1.0,
];

/// The entrance progress at a fraction `t` (0–1) of the duration.
double enterProgress(double t) {
  final x = t.clamp(0.0, 1.0) * (enterCurve.length - 1);
  final i = x.floor().clamp(0, enterCurve.length - 2);
  return enterCurve[i] + (enterCurve[i + 1] - enterCurve[i]) * (x - i);
}

/// Entrance duration: ~60 ms for a small growth, ~160 ms for a large one; it grows with the
/// ratio between the magnified and the resting canvas (1.08 to 2.1 and over).
Duration enterDuration(double ratio) {
  final span = (ratio - 1.08) / (2.1 - 1.08);
  final ms = 60 + (160 - 60) * span.clamp(0.0, 1.0);
  return Duration(microseconds: (ms * 1000).round());
}

/// A Dock item: an app (or folder, trash) or the separator.
/// Bumped when a Dock folder reports a change, so its icons are asked again.
int dockIconRevision = 0;

enum ItemKind { app, separator }

class Item {
  const Item.app(
    this.label, {
    this.running = false,
    this.iconPath,
    this.layered,
    this.command,
    this.windows = const [],
    this.id,
    this.pinned = false,
    this.folder,
    this.openFiles,
    this.trash = false,
    this.action,
    this.fixed = false,
    this.missing = false,
  }) : kind = ItemKind.app,
       minimizedWindow = null,
       thumbnail = null;

  /// A minimized window (D-08): its thumbnail with the app icon in the corner.
  const Item.window(
    this.label, {
    required String window,
    this.thumbnail,
    this.iconPath,
    this.layered,
  }) : kind = ItemKind.app,
       running = false,
       command = null,
       windows = const [],
       id = null,
       pinned = false,
       folder = null,
       openFiles = null,
       trash = false,
       action = null,
       fixed = false,
       missing = false,
       minimizedWindow = window;
  const Item.separator()
    : kind = ItemKind.separator,
      label = '',
      running = false,
      iconPath = null,
      layered = null,
      command = null,
      windows = const [],
      id = null,
      pinned = false,
      folder = null,
      openFiles = null,
      trash = false,
      action = null,
      fixed = false,
      missing = false,
      minimizedWindow = null,
      thumbnail = null;

  final ItemKind kind;
  final String label;
  final bool running;

  /// The icon file (SVG or PNG); null: placeholder drawn in code.
  final String? iconPath;

  /// The layered silica-icons icon, preferred to [iconPath].
  final LayeredIcon? layered;

  /// The command to launch it (Exec without field codes), if it is an app.
  final String? command;

  /// The open windows of the app (identifiers of Ossido's list).
  final List<String> windows;

  /// The id of the .desktop entry (null for folders and trash).
  final String? id;

  /// Pinned in the Dock (it stays even when closed).
  final bool pinned;

  /// A folder shown as a stack (D-06): its path.
  final String? folder;

  /// Exec with field codes, to open the files dropped on the app (null: it takes none).
  final String? openFiles;

  /// The Trash: dropped files end up inside it.
  final bool trash;

  /// A shell command (engine.execute id) run on a click instead of launching [command]: the
  /// Apps item opens the Launcher.
  final String? action;

  /// Always first in the Dock, neither dragged nor removed: the file manager (#141).
  final bool fixed;

  /// A pinned app that is no longer installed (moved to the Trash, or its volume ejected): the
  /// question mark until it is found again (silica-desktop#138).
  final bool missing;

  /// Accepts files dragged from another app (D-05, MU-40).
  bool get acceptsFiles =>
      trash ||
      folder != null ||
      (openFiles != null && RegExp('%[fFuU]').hasMatch(openFiles!));

  /// The minimized window this item stands for (null for apps and folders).
  final String? minimizedWindow;

  /// Its thumbnail, if it has already arrived from Ossido.
  final ui.Image? thumbnail;

  /// The icon to draw; null: placeholder with the initial.
  IconSource? get icon => switch ((layered, iconPath)) {
    _ when folder != null => ServiceSource(
      Uri.file(folder!),
      isFolder: true,
      revision: dockIconRevision,
    ),
    (final LayeredIcon l, _) => LayeredSource(l),
    (null, final String path) => FlatSource(path),
    _ => null,
  };

  double get restWidth => restWidthIn(DockMetrics.standard);

  /// The width of its cell at rest.
  double restWidthIn(DockMetrics m) =>
      kind == ItemKind.separator ? m.separatorCell : m.pitch;

  /// A folder, the Trash or another place: its icon is drawn on the place canvas, whose body
  /// is as large as the canvas of an app (SilicaIcon.besideApps).
  bool get isPlace =>
      trash ||
      folder != null ||
      (layered != null && layered!.kind != IconKind.app);
}

/// Launch of an app that is not kept in the Dock (dock.md, DOCK-ANIM, measured): the item is
/// inserted before the separator at 10 % of its size with its centre fixed; the cell grows
/// 6.3 to 63 pt in ~350 ms along cubic-bezier(0.38, 0, 0.64, 1) and the neighbours each move
/// half the width. The icon is drawn at the cell's scale with opacity (scale - 0.2) / 0.8.
const double launchStartScale = 0.1;
const Duration launchDuration = Duration(milliseconds: 350);
const Cubic launchCurve = Cubic(0.38, 0, 0.64, 1);

/// The 17 pt rise of the hop is the same animation (same curve); the fall takes ~350 ms,
/// quadratic ease-in, with no settle and no squash; one hop is ~705 ms. With magnification the
/// peak is ~18 pt.
const double launchHopPeak = 17 * amalfiScale;
const Duration launchFallDuration = Duration(milliseconds: 350);
const Duration launchHopDuration = Duration(milliseconds: 705);

/// The running dot fades in linearly over ~450 ms (415-480 measured), starting ~255 ms in
/// (230-325 measured) and ending at the landing.
const Duration launchDotStart = Duration(milliseconds: 255);
const Duration launchDotFade = Duration(milliseconds: 450);

/// Quit: the cell goes 63 to 0 pt in ~480 ms (470-490) along cubic-bezier(0.38, 0.05, 0.61,
/// 1); the icon shrinks in place with opacity = scale squared; the dot fades out over ~370 ms
/// (350-390) from the start.
const Duration quitDuration = Duration(milliseconds: 480);
const Cubic quitCurve = Cubic(0.38, 0.05, 0.61, 1);
const Duration quitDotFade = Duration(milliseconds: 370);

/// Height of the launch hop as a fraction of its peak, `ms` after the start.
double launchHop(double ms) {
  if (ms <= 0) return 0;
  final rise = launchDuration.inMilliseconds;
  if (ms < rise) return launchCurve.transform(ms / rise);
  final fall = launchFallDuration.inMilliseconds;
  final u = (ms - rise) / fall;
  return u >= 1 ? 0 : 1 - u * u;
}

/// The Dock layout at one moment: cells, background, icons.
class Layout {
  Layout._(
    this.cells,
    this.scales,
    this.background,
    this.presence,
    this.metrics,
  );

  /// The size the layout was computed for.
  final DockMetrics metrics;

  /// How present every item is (1 normally; 0..1 while its slot opens or closes, see
  /// [launchDuration]): it scales the width of the cell, the icon and its opacity.
  final List<double> presence;

  /// Rectangle (x, width) of every cell, in the order of the items.
  final List<(double, double)> cells;

  /// Scale of every cell (1 at rest; the separator stays 1).
  final List<double> scales;

  /// The background: left, top, width, height.
  final (double, double, double, double) background;

  /// Center of cell `i`.
  double center(int i) => cells[i].$1 + cells[i].$2 / 2;
}

/// Computes the layout for a surface `width` wide and `height` tall, with the pointer at
/// `pointerX` (null: outside the active zone) and the magnification `amount` (0–1, the
/// progress of the entrance) up to the canvas `magnified` (null: no magnification).
Layout layout(
  List<Item> items, {
  required double width,
  required double height,
  double? pointerX,
  double amount = 0,
  double? magnified,
  DockMetrics metrics = DockMetrics.standard,
  double Function(Item item)? presenceOf,
}) {
  final presence = [
    for (final item in items) presenceOf?.call(item).clamp(0.0, 1.0) ?? 1.0,
  ];
  // The resting width of a cell, shrunk while its slot is opening or closing.
  double rest(int i) => items[i].restWidthIn(metrics) * presence[i];
  var restTotal = 0.0;
  for (var i = 0; i < items.length; i++) {
    restTotal += rest(i);
  }
  final restLeft = (width - restTotal) / 2;
  // Resting centers, from which the distance is measured.
  final restCenters = <double>[];
  var x = restLeft;
  for (var i = 0; i < items.length; i++) {
    restCenters.add(x + rest(i) / 2);
    x += rest(i);
  }
  final scales = [
    for (var i = 0; i < items.length; i++)
      if (items[i].kind == ItemKind.separator ||
          pointerX == null ||
          magnified == null)
        1.0
      else
        1 +
            (magnifiedCanvas(metrics, magnified, pointerX - restCenters[i]) /
                        metrics.canvas -
                    1) *
                amount,
  ];
  // How much wider a cell is than at rest: its tile grows with its canvas, the gap does not.
  double grown(int i) => (scales[i] - 1) * metrics.canvas * presence[i];
  // Lateral expansion: the point under the pointer stays still. Every cell gives its growth
  // to the side it is on relative to the pointer; the one under the pointer splits it
  // according to the pointer's position in the resting cell.
  var leftGrowth = 0.0;
  for (var i = 0; i < items.length; i++) {
    final extra = grown(i);
    if (extra == 0 || pointerX == null) continue;
    final cellLeft = restCenters[i] - rest(i) / 2;
    final fraction = ((pointerX - cellLeft) / rest(i)).clamp(0.0, 1.0);
    leftGrowth += extra * fraction;
  }
  final cells = <(double, double)>[];
  x = restLeft - leftGrowth;
  for (var i = 0; i < items.length; i++) {
    final w = rest(i) + grown(i);
    cells.add((x, w));
    x += w;
  }
  final first = cells.first.$1;
  final last = cells.last.$1 + cells.last.$2;
  final top = height - bottomGap - metrics.backgroundHeight;
  return Layout._(
    cells,
    scales,
    (
      first - metrics.sidePadding,
      top,
      last - first + 2 * metrics.sidePadding,
      metrics.backgroundHeight,
    ),
    presence,
    metrics,
  );
}

/// The canvas square of the icon of cell `i`: left, top, side. Magnified, the bottom stays
/// where it was at rest and the icon overflows upward (the shelf keeps its height).
(double, double, double) iconRect(Layout l, int i, double height) {
  final m = l.metrics;
  final side = m.canvas * l.scales[i] * l.presence[i];
  final bottom = height - bottomGap - m.backgroundHeight + m.iconTop + m.canvas;
  return (l.center(i) - side / 2, bottom - side, side);
}

/// The 824 body of an app is inset in its canvas by this fraction of the side, per side.
const double bodyInset = (1 - 824 / 1024) / 2;

/// Where to put a `SilicaIcon(side: …, besideApps: true)` for the canvas square `side` at
/// (0, 0): an app's body is inset in its canvas; a place (folder, Trash, stack) has a body
/// as large as that canvas, its bottom on the bottom of the app bodies, so it spills over the
/// tile on every side (unclipped).
(double, double, double) iconBody(double side, {required bool place}) {
  if (!place) {
    final body = side * (1 - 2 * bodyInset);
    return (side * bodyInset, side * bodyInset, body);
  }
  return (0, 0, side);
}

/// The pointer is in the active zone: inside the resting background horizontally (x 816 no,
/// x 820 yes with the background from 816.5), from 5 pt above the background downwards.
bool inActiveZone(Layout rest, double x, double y, double height) {
  final top =
      height - bottomGap - rest.metrics.backgroundHeight - activeZoneAbove;
  final (left, _, w, _) = rest.background;
  return y >= top && x >= left && x <= left + w;
}

/// Leaving upwards the zone stays active while the pointer is in the magnified cell
/// (hysteresis, dock.md D-01): up to the top of its icon minus this margin.
const double activeZoneHold = 6;

/// Stays in the zone: horizontally as on entering (the resting list), vertically up to the
/// top of the magnified cell under the pointer.
bool staysInActiveZone(
  Layout rest,
  Layout current,
  List<Item> items,
  double x,
  double y,
  double height,
) {
  final (left, _, w, _) = rest.background;
  if (x < left || x > left + w) return false;
  if (inActiveZone(rest, x, y, height)) return true;
  for (var i = 0; i < current.cells.length; i++) {
    final (cellLeft, cellWidth) = current.cells[i];
    if (x >= cellLeft && x < cellLeft + cellWidth) {
      final (_, top, _) = iconRect(current, i, height);
      return y >= top - activeZoneHold;
    }
  }
  return false;
}

/// Label (D-12): it appears on entering (full in ~40 ms), jumps immediately from one icon to
/// the next, on exit it stays ~40 ms and fades in ~250 ms.
const Duration labelAppear = Duration(milliseconds: 40);
const Duration labelHold = Duration(milliseconds: 40);
const Duration labelVanish = Duration(milliseconds: 250);

/// Launch bounce (misc.md): height in pt above rest on the reference's 63 pt Dock, a jump every
/// ~0.7 s, until the app opens a window (at most `bounceLimit`). The shape is the measured
/// parabolic one (samples every ~13-80 ms; rise ~0.3 s, fall ~0.35-0.4 s, no squash, no
/// rebound), not a spring; only the heights are rescaled to the Amalfi size ([amalfiScale]).
const List<(double, double)> bounceCurve = [
  (0, 0),
  (27, 3),
  (40, 5),
  (53, 6),
  (80, 8),
  (93, 9),
  (133, 11),
  (160, 13),
  (213, 14),
  (293, 17),
  (440, 14),
  (533, 11),
  (587, 7),
  (613, 6),
  (653, 3),
  (693, 0),
];
const double bouncePeriod = 693;
const double bouncePeak = 17 * amalfiScale;
const Duration bounceLimit = Duration(seconds: 10);

/// The item that bounces when [item] is clicked (dock.md D-02/D-03, silica-desktop#210): an
/// app that is not running bounces itself until its first window; a running app, and the
/// Apps action, never. A place (the Trash, a folder) is opened by the file manager: if that
/// is running nothing bounces, otherwise the bounce is the file manager's own tile's.
Item? launchBounceTarget(Item item, List<Item> items) {
  if (item.action != null) return null;
  if (item.isPlace || item.trash || item.folder != null) {
    final manager = items.where((i) => i.fixed).firstOrNull;
    if (item.command == null || manager == null || manager.running) return null;
    return manager.command == null ? null : manager;
  }
  return !item.running && item.command != null ? item : null;
}

/// Bounce height `ms` after the start (repeated).
double bounceHeight(double ms) {
  if (ms <= 0) return 0;
  final t = ms % bouncePeriod;
  for (var i = 1; i < bounceCurve.length; i++) {
    final (t1, h1) = bounceCurve[i];
    if (t <= t1) {
      final (t0, h0) = bounceCurve[i - 1];
      return amalfiScale * (h0 + (h1 - h0) * (t - t0) / (t1 - t0));
    }
  }
  return 0;
}

/// Attention request (dock.md › SH-DK-30): three decaying hops (56, 23, 8 pt lasting ~466,
/// 320, 214 ms), then ~1 s still, repeated every 2 s until the app is activated.
const List<(double, double)> attentionHops = [
  (466, 56 * amalfiScale),
  (320, 23 * amalfiScale),
  (214, 8 * amalfiScale),
];
const double attentionPeriod = 2000;

/// Attention height `ms` after the request.
double attentionHeight(double ms) {
  if (ms <= 0) return 0;
  var t = ms % attentionPeriod;
  for (final (duration, height) in attentionHops) {
    if (t < duration) {
      final x = t / duration;
      // A hop: up and down, symmetric.
      return 4 * height * x * (1 - x);
    }
    t -= duration;
  }
  return 0;
}

/// Badge (dock.md › SH-DK-29): a red capsule 23 pt tall, its top 10 pt below the cell top,
/// its right edge 3 pt inside the cell; 23 pt wide for one or two characters, then 26, 33, 39.
const double badgeHeight = 23;
const double badgeTop = 10;
const double badgeRightInset = 3;
const ui.Color badgeFill = ui.Color(0xFFF23F34);

/// The badge width for `text` (measured widths; beyond five characters ~6.5 pt each).
double badgeWidth(String text) => switch (text.length) {
  <= 2 => 23,
  3 => 26,
  4 => 33,
  5 => 39,
  final n => 39 + (n - 5) * 6.5,
};

/// Download progress (SH-DK-30): a 59 × 4 pt outlined capsule in place of the indicator dot,
/// filling from the left in the dot's color.
const double progressWidth = 59;
const double progressHeight = 4;

/// The cell under the pointer, if there is an app.
/// The cell under `x`, separator included.
int? cellAt(Layout l, double x) {
  for (var i = 0; i < l.cells.length; i++) {
    final (left, w) = l.cells[i];
    if (x >= left && x < left + w) return i;
  }
  return null;
}

int? itemAt(Layout l, List<Item> items, double x) {
  for (var i = 0; i < l.cells.length; i++) {
    final (left, w) = l.cells[i];
    if (x >= left && x < left + w) {
      return items[i].kind == ItemKind.app ? i : null;
    }
  }
  return null;
}
