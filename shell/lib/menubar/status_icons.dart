// Bar icons drawn in code (generic shapes, not assets): the Silica mark, the network,
// search (Faro), the Control Center. The status items draw the Silica Symbols glyphs (20 pt) of
// the shared Status Bar (silica_ui `AmalfiStatusGlyph`, docs/design/amalfi/status-bar.md); these
// shapes are what they fall back to when a symbol is missing.

import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

/// The Silica mark ("from sand to glass", silica-brand `exports/mark/silica-mark-16-mono.svg`):
/// the half disc of glass and the three grains of sand, in one colour on the 16 pt grid
/// (ink 14 × 14 pt). The mark is a trademark (silica-brand TRADEMARKS.md): only the shape
/// from the brand, never redrawn.
class SilicaMark extends StatelessWidget {
  const SilicaMark({super.key, required this.color});

  final Color color;

  @override
  Widget build(BuildContext context) =>
      CustomPaint(size: const Size(16, 16), painter: _MarkPainter(color));
}

class _MarkPainter extends CustomPainter {
  _MarkPainter(this.color);

  final Color color;

  /// The mark on its 16-unit grid, as in the brand's 16 pt mono export.
  static Path markPath() => Path()
    // M8 1 A7 7 0 0 1 8 15 Z: the right half of a disc of radius 7.
    ..moveTo(8, 1)
    ..arcToPoint(const Offset(8, 15), radius: const Radius.circular(7))
    ..close()
    ..addRRect(RRect.fromLTRBR(4, 4, 7, 7, const Radius.circular(0.9)))
    ..addRRect(RRect.fromLTRBR(4, 9, 7, 12, const Radius.circular(0.9)))
    ..addRRect(RRect.fromLTRBR(1, 7, 3, 9, const Radius.circular(0.6)));

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 16, size.height / 16);
    canvas.drawPath(markPath(), Paint()..color = color);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_MarkPainter old) => old.color != color;
}

enum StatusKind {
  network,
  wired,
  battery,
  search,
  controls,
  focus,
  nowPlaying,
  sound,
  bluetooth,
}

/// The Silica Symbols glyph of a status item, by what it shows (status-bar.md › Items): the
/// state is the glyph (`wifi-slash`, `volume-slash`), never a dimmed ink.
String statusSymbol(
  StatusKind kind, {
  double level = 1,
  bool dim = false,
  bool charging = false,
}) => switch (kind) {
  StatusKind.network =>
    dim
        ? 'wifi-slash'
        : level < 0.1
        ? 'wifi-0'
        : switch (wifiArcs(level)) {
            3 => 'wifi',
            2 => 'wifi-2',
            _ => 'wifi-1',
          },
  StatusKind.wired => 'ethernet',
  StatusKind.battery => AmalfiStatusBatteryItem.symbolFor(
    level,
    charging: charging,
  ),
  StatusKind.search => 'search',
  StatusKind.controls => 'control-center',
  StatusKind.focus => 'moon',
  StatusKind.nowPlaying => 'play-circle',
  StatusKind.sound =>
    dim || level <= 0
        ? 'volume-slash'
        : level < 0.34
        ? 'volume-0'
        : level < 0.67
        ? 'volume-1'
        : 'volume-2',
  StatusKind.bluetooth => dim ? 'bluetooth-slash' : 'bluetooth',
};

/// The side of a status glyph in the bar.
const double statusGlyph = 20;

class StatusIcon extends StatelessWidget {
  const StatusIcon({
    super.key,
    required this.kind,
    this.color,
    this.level = 1,
    this.dim = false,
    this.charging = false,
  });

  final StatusKind kind;

  /// The glyph's colour; null: the ink of the Status Bar it is in (`AmalfiStatusBarStyle`).
  final Color? color;

  /// Wi-Fi strength or battery charge, 0–1.
  final double level;

  /// Off (Wi-Fi disabled, Bluetooth off, muted).
  final bool dim;

  /// Battery charging.
  final bool charging;

  @override
  Widget build(BuildContext context) {
    final override = color;
    final ink = override ?? AmalfiStatusBarStyle.of(context).ink;
    final symbol = statusSymbol(
      kind,
      level: level,
      dim: dim,
      charging: charging,
    );
    // Without the symbol: the shapes drawn in code.
    final fallback = CustomPaint(
      painter: _StatusPainter(
        kind,
        ink,
        level: level,
        dim: dim,
        charging: charging,
      ),
    );
    if (override == null) {
      return AmalfiStatusGlyph(symbol, fallback: fallback);
    }
    return SizedBox.square(
      dimension: statusGlyph,
      child: AmalfiSymbol(
        symbol,
        size: statusGlyph,
        color: override,
        fallback: fallback,
      ),
    );
  }
}

/// How many Wi-Fi arcs are lit for a strength 0–1 (the dot always is).
int wifiArcs(double level) => level >= 0.67
    ? 3
    : level >= 0.34
    ? 2
    : 1;

class _StatusPainter extends CustomPainter {
  _StatusPainter(
    this.kind,
    this.color, {
    this.level = 1,
    this.dim = false,
    this.charging = false,
  });

  final StatusKind kind;
  final Color color;
  final double level;
  final bool dim;
  final bool charging;

  @override
  void paint(Canvas canvas, Size size) {
    final c = size.center(Offset.zero);
    final stroke = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.6
      ..strokeCap = StrokeCap.round;
    final fill = Paint()..color = color;
    switch (kind) {
      case StatusKind.network:
        // Three arcs and a dot, ~17 × 12 pt; the arcs beyond the strength at 35 %.
        final base = c + const Offset(0, 5);
        final faint = color.withValues(alpha: color.a * 0.35);
        final lit = dim ? 0 : wifiArcs(level);
        final radii = [4.5, 8.0, 11.5];
        for (var i = 0; i < radii.length; i++) {
          canvas.drawArc(
            Rect.fromCircle(center: base, radius: radii[i]),
            -math.pi * 0.75,
            math.pi * 0.5,
            false,
            Paint.from(stroke)..color = i < lit ? color : faint,
          );
        }
        canvas.drawCircle(base, 1.4, dim ? (Paint()..color = faint) : fill);
      case StatusKind.wired:
        // Wired network: three connected boxes, ~16 × 12 pt.
        final link = stroke..strokeWidth = 1.4;
        final top = Rect.fromCenter(
          center: c - const Offset(0, 3.5),
          width: 6,
          height: 5,
        );
        canvas.drawRect(top, link);
        for (final dx in [-5.5, 5.5]) {
          canvas.drawRect(
            Rect.fromCenter(center: c + Offset(dx, 4), width: 5, height: 4),
            link,
          );
          canvas.drawLine(c + Offset(dx, 2), c + Offset(dx, 0.5), link);
        }
        canvas.drawLine(
          c + const Offset(-5.5, 0.5),
          c + const Offset(5.5, 0.5),
          link,
        );
        canvas.drawLine(
          c + const Offset(0, -1),
          c + const Offset(0, 0.5),
          link,
        );
      case StatusKind.battery:
        // Battery ~25 × 12 pt with the charge fill (provisional sizes: the reference is a
        // desktop, without a battery).
        final body = RRect.fromRectAndRadius(
          Rect.fromCenter(
            center: c - const Offset(1.5, 0),
            width: 23,
            height: 11.5,
          ),
          const Radius.circular(3.5),
        );
        canvas.drawRRect(body, Paint.from(stroke)..strokeWidth = 1.1);
        final inner = body.outerRect.deflate(2);
        canvas.saveLayer(body.outerRect.inflate(2), Paint());
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            Rect.fromLTWH(
              inner.left,
              inner.top,
              inner.width * level.clamp(0, 1),
              inner.height,
            ),
            const Radius.circular(1.5),
          ),
          fill,
        );
        if (charging) {
          final bolt = Path()
            ..moveTo(c.dx + 0.5, c.dy - 6)
            ..lineTo(c.dx - 4, c.dy + 0.8)
            ..lineTo(c.dx - 0.8, c.dy + 0.8)
            ..lineTo(c.dx - 2, c.dy + 6)
            ..lineTo(c.dx + 2.5, c.dy - 0.8)
            ..lineTo(c.dx - 0.6, c.dy - 0.8)
            ..close();
          canvas.drawPath(bolt, Paint()..blendMode = BlendMode.clear);
          canvas.drawPath(bolt, Paint.from(stroke)..strokeWidth = 0.9);
        }
        canvas.restore();
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            Rect.fromLTWH(body.right + 1, c.dy - 2, 1.6, 4),
            const Radius.circular(0.8),
          ),
          fill,
        );
      case StatusKind.search:
        // Lens: 13 × 13 pt.
        canvas.drawCircle(c + const Offset(-1.5, -1.5), 4.6, stroke);
        canvas.drawLine(
          c + const Offset(1.8, 1.8),
          c + const Offset(5.5, 5.5),
          stroke,
        );
      case StatusKind.focus:
        // A Focus is on: the crescent moon, ~13 pt (SH-MB-26).
        final moon = Path.combine(
          PathOperation.difference,
          Path()..addOval(Rect.fromCircle(center: c, radius: 6.5)),
          Path()..addOval(
            Rect.fromCircle(center: c + const Offset(4, -3.2), radius: 5.6),
          ),
        );
        canvas.drawPath(moon, fill);
      case StatusKind.nowPlaying:
        // Media playing: a play triangle in a ring, ~15 pt (SH-MB-33).
        canvas
          ..drawCircle(c, 7, stroke..strokeWidth = 1.3)
          ..drawPath(
            Path()
              ..moveTo(c.dx - 2, c.dy - 3.5)
              ..lineTo(c.dx + 3.5, c.dy)
              ..lineTo(c.dx - 2, c.dy + 3.5)
              ..close(),
            fill,
          );
      case StatusKind.sound:
        // A speaker and up to three waves for the volume (`level`; muted: no waves, a
        // slash), ~18 × 14 pt. Provisional: drawn after the Control Center's glyph.
        final body = Path()
          ..moveTo(c.dx - 8, c.dy - 2.5)
          ..lineTo(c.dx - 5, c.dy - 2.5)
          ..lineTo(c.dx - 1, c.dy - 6)
          ..lineTo(c.dx - 1, c.dy + 6)
          ..lineTo(c.dx - 5, c.dy + 2.5)
          ..lineTo(c.dx - 8, c.dy + 2.5)
          ..close();
        canvas.drawPath(body, fill);
        if (dim) {
          canvas.drawLine(
            c + const Offset(2, -4),
            c + const Offset(8, 4),
            stroke..strokeWidth = 1.4,
          );
          break;
        }
        final waves = level <= 0
            ? 0
            : level < 0.34
            ? 1
            : level < 0.67
            ? 2
            : 3;
        for (var i = 0; i < waves; i++) {
          canvas.drawArc(
            Rect.fromCircle(
              center: c - const Offset(1, 0),
              radius: 3.5 + i * 3,
            ),
            -math.pi * 0.3,
            math.pi * 0.6,
            false,
            stroke..strokeWidth = 1.4,
          );
        }
      case StatusKind.bluetooth:
        // The runic B, ~8 × 15 pt; dimmed when Bluetooth is off.
        final ink = Paint.from(stroke)
          ..strokeWidth = 1.4
          ..strokeJoin = StrokeJoin.round
          ..color = dim ? color.withValues(alpha: color.a * 0.35) : color;
        final rune = Path()
          ..moveTo(c.dx - 3.8, c.dy - 3.6)
          ..lineTo(c.dx + 3.6, c.dy + 3.6)
          ..lineTo(c.dx, c.dy + 7.2)
          ..lineTo(c.dx, c.dy - 7.2)
          ..lineTo(c.dx + 3.6, c.dy - 3.6)
          ..lineTo(c.dx - 3.8, c.dy + 3.6);
        canvas.drawPath(rune, ink);
      case StatusKind.controls:
        // Two stacked switches: 14 × 14 pt.
        for (final (dy, knobLeft) in [(-3.5, true), (3.5, false)]) {
          final pill = RRect.fromRectAndRadius(
            Rect.fromCenter(center: c + Offset(0, dy), width: 14, height: 6),
            const Radius.circular(3),
          );
          canvas.drawRRect(pill, stroke..strokeWidth = 1.3);
          canvas.drawCircle(c + Offset(knobLeft ? -4 : 4, dy), 1.8, fill);
        }
    }
  }

  @override
  bool shouldRepaint(_StatusPainter old) =>
      old.kind != kind ||
      old.color != color ||
      old.level != level ||
      old.dim != dim ||
      old.charging != charging;
}
