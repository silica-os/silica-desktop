// The screen savers (Settings › Screen Saver): full-screen animations that start after the
// idle time of screensaver.json and end with any input (Ossido tells the shell with the state
// `screensaver.active`, the id of the saver). Drawn from the Amalfi palette; each moves slowly
// so that no pixel stays lit.

import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

/// The savers the shell draws, in the order Settings lists them. Ossido accepts the same ids.
const List<String> screenSaverIds = ['drift', 'clock', 'colors'];

/// The saver with this id; null for an unknown one.
Widget? screenSaver(String id, {DateTime Function()? now}) => switch (id) {
  'drift' => const DriftSaver(),
  'clock' => ClockSaver(now: now),
  'colors' => const ColorsSaver(),
  _ => null,
};

/// A slow animation: [builder] gets the time in seconds since it began.
class _Slow extends StatefulWidget {
  const _Slow({required this.builder});

  final Widget Function(BuildContext context, double seconds) builder;

  @override
  State<_Slow> createState() => _SlowState();
}

class _SlowState extends State<_Slow> with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(hours: 1),
  )..repeat();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _controller,
    builder: (context, _) => widget.builder(
      context,
      _controller.lastElapsedDuration?.inMicroseconds == null
          ? 0
          : _controller.lastElapsedDuration!.inMicroseconds / 1e6,
    ),
  );
}

/// Drift: soft blobs of the palette's sea, lavender and bougainvillea wandering on black.
class DriftSaver extends StatelessWidget {
  const DriftSaver({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = AmalfiColors.dark;
    return _Slow(
      builder: (context, t) => CustomPaint(
        painter: DriftPainter(t, [
          colors.paletteMare,
          colors.paletteLavanda,
          colors.paletteBouganville,
        ]),
        child: const SizedBox.expand(),
      ),
    );
  }
}

class DriftPainter extends CustomPainter {
  DriftPainter(this.seconds, this.colors);

  final double seconds;
  final List<Color> colors;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(
      Offset.zero & size,
      Paint()..color = const Color(0xFF000000),
    );
    final side = math.min(size.width, size.height);
    for (var i = 0; i < colors.length; i++) {
      final speed = 0.05 + 0.017 * i;
      final x = size.width * (0.5 + 0.32 * math.sin(seconds * speed + i * 2.1));
      final y =
          size.height * (0.5 + 0.3 * math.cos(seconds * speed * 0.8 + i * 1.3));
      final radius = side * (0.42 + 0.06 * math.sin(seconds * 0.07 + i));
      final paint = Paint()
        ..shader = RadialGradient(
          colors: [
            colors[i].withValues(alpha: 0.75),
            colors[i].withValues(alpha: 0),
          ],
        ).createShader(Rect.fromCircle(center: Offset(x, y), radius: radius));
      canvas.drawCircle(Offset(x, y), radius, paint);
    }
  }

  @override
  bool shouldRepaint(DriftPainter old) => old.seconds != seconds;
}

/// Clock: the time, large, drifting slowly over the screen.
class ClockSaver extends StatelessWidget {
  const ClockSaver({super.key, this.now});

  /// The time to show; the system's by default.
  final DateTime Function()? now;

  @override
  Widget build(BuildContext context) {
    final colors = AmalfiColors.dark;
    return ColoredBox(
      color: const Color(0xFF000000),
      child: _Slow(
        builder: (context, t) {
          final time = (now ?? DateTime.now)();
          final text =
              '${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
          return Align(
            alignment: Alignment(
              0.6 * math.sin(t * 0.03),
              0.5 * math.cos(t * 0.021),
            ),
            child: Text(
              text,
              key: const ValueKey('saver time'),
              style: AmalfiType.numeral104(color: colors.textPrimary),
            ),
          );
        },
      ),
    );
  }
}

/// Colours: the whole screen slowly fading through the palette.
class ColorsSaver extends StatelessWidget {
  const ColorsSaver({super.key});

  @override
  Widget build(BuildContext context) {
    final c = AmalfiColors.dark;
    final palette = [
      c.paletteMare,
      c.paletteLavanda,
      c.paletteBouganville,
      c.paletteTerracotta,
      c.paletteArancia,
      c.paletteUlivo,
    ];
    return _Slow(
      builder: (context, t) {
        // One colour every 20 s, blended with the next.
        final at = (t / 20) % palette.length;
        final i = at.floor() % palette.length;
        final from = palette[i];
        final to = palette[(i + 1) % palette.length];
        final color = Color.lerp(from, to, at - at.floorToDouble()) ?? from;
        return ColoredBox(
          color: Color.lerp(const Color(0xFF000000), color, 0.85) ?? color,
          child: const SizedBox.expand(),
        );
      },
    );
  }
}
