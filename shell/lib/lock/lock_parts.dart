// The parts of the lock screen (kit page "Lock Screen", masters `Lock Screen/...`): the Glass/Lock
// material, the password field and its states, the help button, the rolling clock, the status
// items, the collapsed banners and the optional power row. Look and measures from
// docs/design/amalfi/lock-screen.md; behaviour from docs/spec/lock.md (L-01).

import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/services.dart' show TextInputAction;
import 'package:flutter/widgets.dart';
import 'package:intl/intl.dart' show DateFormat;
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../menubar/status_icons.dart';
import '../notifications/model.dart';
import '../notifications/notifications.dart' show AppIcon;
import '../system/system_status.dart';
import 'lock_layout.dart';

/// What the lock screen draws with: the appearance and the accessibility settings. Dark is
/// the measured one; light, increase contrast and reduce transparency are provisional.
class LockLook {
  const LockLook({
    this.dark = true,
    this.increaseContrast = false,
    this.reduceTransparency = false,
    this.reduceMotion = false,
  });

  factory LockLook.of(BuildContext context) {
    final look = SystemLookScope.of(context);
    return LockLook(
      dark:
          (MediaQuery.maybePlatformBrightnessOf(context) ?? Brightness.dark) ==
          Brightness.dark,
      increaseContrast: look.increaseContrast,
      reduceTransparency: look.reduceTransparency,
      reduceMotion:
          look.reduceMotion ||
          (MediaQuery.maybeDisableAnimationsOf(context) ?? false),
    );
  }

  final bool dark;
  final bool increaseContrast;
  final bool reduceTransparency;
  final bool reduceMotion;

  AmalfiColors get colors => dark ? AmalfiColors.dark : AmalfiColors.light;

  /// Text over the wallpaper (name, hint, link, status items, labels): `Primary` in dark,
  /// white with a soft shadow in light.
  Color get ink =>
      dark ? AmalfiColors.dark.textPrimary : const Color(0xFFFFFFFF);

  /// The help address: `Secondary` in dark, white 72 % in light.
  Color get inkSecondary =>
      dark ? AmalfiColors.dark.textSecondary : const Color(0xB8FFFFFF);

  /// What is drawn inside the glass (dots, glyphs, placeholder).
  Color get glassInk => colors.textPrimary;
  Color get glassInkSecondary => colors.textSecondary;

  List<Shadow>? get textShadow => dark
      ? null
      : const [
          Shadow(color: Color(0x2E000000), offset: Offset(0, 1), blurRadius: 4),
        ];

  Color get fill => increaseContrast
      ? const Color(0xF22A2B30)
      : reduceTransparency
      ? Color.alphaBlend(
          dark ? const Color(0x47FFFFFF) : const Color(0x99FFFFFF),
          colors.surfaceWindow,
        )
      : dark
      ? const Color(0x47FFFFFF)
      : const Color(0x99FFFFFF);

  Color get edge => increaseContrast
      ? const Color(0x99FFFFFF)
      : dark
      ? const Color(0x3DFFFFFF)
      : const Color(0x1F000000);

  Color get rimLight =>
      dark ? const Color(0x38FFFFFF) : const Color(0xB3FFFFFF);

  /// The glass blurs the wallpaper behind it (σ 20, saturation 1.1).
  bool get blurs => !increaseContrast && !reduceTransparency;
}

/// The saturation 1.1 of `Glass/Lock` as a colour matrix.
List<double> _saturation(double s) {
  const r = 0.2126, g = 0.7152, b = 0.0722;
  final i = 1 - s;
  return [
    r * i + s, g * i, b * i, 0, 0, //
    r * i, g * i + s, b * i, 0, 0, //
    r * i, g * i, b * i + s, 0, 0, //
    0, 0, 0, 1, 0,
  ];
}

/// `Glass/Lock` (tokens.json › materials): background blur σ 20, saturation 1.1, white 28 %
/// (60 % light), a 1 pt lighter edge inside, a rim light on top, no refraction, no shadow. The
/// blur radius never animates. Over the wallpaper in the same view (the lock surface is a view
/// of its own, Flutter draws it).
class LockGlass extends StatelessWidget {
  const LockGlass({
    super.key,
    required this.look,
    required this.radius,
    this.edge,
    this.child,
  });

  final LockLook look;
  final double radius;

  /// The edge colour when it is not the material's (the reduced-motion error flash).
  final Color? edge;
  final Widget? child;

  @override
  Widget build(BuildContext context) {
    final borderRadius = BorderRadius.circular(radius);
    return ClipRRect(
      borderRadius: borderRadius,
      child: Stack(
        fit: StackFit.expand,
        children: [
          if (look.blurs)
            BackdropFilter(
              filter: ui.ImageFilter.compose(
                outer: ui.ColorFilter.matrix(_saturation(1.1)),
                inner: ui.ImageFilter.blur(sigmaX: 20, sigmaY: 20),
              ),
              child: ColoredBox(color: look.fill),
            )
          else
            ColoredBox(color: look.fill),
          Positioned(
            left: radius,
            right: radius,
            top: 1,
            height: 1,
            child: ColoredBox(color: look.rimLight),
          ),
          DecoratedBox(
            decoration: BoxDecoration(
              borderRadius: borderRadius,
              border: Border.all(color: edge ?? look.edge),
            ),
          ),
          ?child,
        ],
      ),
    );
  }
}

/// A button of the lock screen: hover and press overlay (`fade.quick` in, 60 ms out), a
/// pointer, Tab focus and Return/Space.
class LockButton extends StatefulWidget {
  const LockButton({
    super.key,
    required this.onTap,
    required this.label,
    required this.child,
    this.overlay,
  });

  final VoidCallback onTap;
  final String label;
  final Widget child;

  /// The side of the circle the overlay covers, at the top and centred; null: all of it.
  final double? overlay;

  @override
  State<LockButton> createState() => _LockButtonState();
}

class _LockButtonState extends State<LockButton> {
  bool _hover = false;
  bool _down = false;

  @override
  Widget build(BuildContext context) {
    final on = _hover || _down;
    return Semantics(
      button: true,
      label: widget.label,
      excludeSemantics: true,
      onTap: widget.onTap,
      child: FocusableActionDetector(
        mouseCursor: SystemMouseCursors.click,
        onShowHoverHighlight: (v) => setState(() => _hover = v),
        actions: {
          ActivateIntent: CallbackAction<ActivateIntent>(
            onInvoke: (_) {
              widget.onTap();
              return null;
            },
          ),
        },
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.onTap,
          onTapDown: (_) => setState(() => _down = true),
          onTapUp: (_) => setState(() => _down = false),
          onTapCancel: () => setState(() => _down = false),
          child: Stack(
            fit: StackFit.passthrough,
            children: [
              widget.child,
              Positioned.fill(
                child: IgnorePointer(
                  child: Align(
                    alignment: Alignment.topCenter,
                    child: SizedBox(
                      width: widget.overlay,
                      height: widget.overlay,
                      child: AnimatedOpacity(
                        opacity: on ? 1 : 0,
                        duration: on
                            ? fadeQuick
                            : const Duration(milliseconds: 60),
                        child: const DecoratedBox(
                          decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            color: Color(0x1FFFFFFF),
                          ),
                        ),
                      ),
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
}

/// The help button: a 28 `Glass/Lock` disc holding the `question` glyph (28, its circle draws
/// ~23).
class LockHelpButton extends StatelessWidget {
  const LockHelpButton({
    super.key,
    required this.look,
    required this.onTap,
    this.edge,
  });

  final LockLook look;
  final VoidCallback onTap;
  final Color? edge;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: 28,
      child: LockButton(
        onTap: onTap,
        label: l10nOf(context).lockHelpButton,
        child: LockGlass(
          look: look,
          radius: 14,
          edge: edge,
          child: Center(
            child: AmalfiSymbol(
              'question',
              size: 28,
              color: look.glassInkSecondary,
            ),
          ),
        ),
      ),
    );
  }
}

/// The password field (kit `Lock Screen/Password Field/<State>`): 160 x 28 and never another
/// size. The characters are typed into a real (hidden) text input; what is seen is the dots,
/// the caret, the selection and the submit button.
class LockPasswordField extends StatefulWidget {
  const LockPasswordField({
    super.key,
    required this.controller,
    required this.focusNode,
    required this.look,
    required this.state,
    required this.placeholder,
    required this.onSubmit,
    this.edge,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final LockLook look;
  final LockFieldState state;
  final String placeholder;

  /// Return or the submit button.
  final VoidCallback onSubmit;
  final Color? edge;

  @override
  State<LockPasswordField> createState() => _LockPasswordFieldState();
}

class _LockPasswordFieldState extends State<LockPasswordField> {
  Timer? _blink;
  bool _caret = true;

  @override
  void initState() {
    super.initState();
    widget.focusNode.addListener(_focusChanged);
    widget.controller.addListener(_typed);
    _focusChanged();
  }

  @override
  void dispose() {
    _blink?.cancel();
    widget.focusNode.removeListener(_focusChanged);
    widget.controller.removeListener(_typed);
    super.dispose();
  }

  void _focusChanged() {
    _blink?.cancel();
    _caret = true;
    if (widget.focusNode.hasFocus) {
      _blink = Timer.periodic(const Duration(milliseconds: 530), (_) {
        if (mounted) setState(() => _caret = !_caret);
      });
    }
    if (mounted) setState(() {});
  }

  void _typed() {
    // A key shows the caret at once.
    _caret = true;
    if (mounted) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final look = widget.look;
    final l = l10nOf(context);
    final count = widget.controller.text.runes.length;
    final focused = widget.focusNode.hasFocus;
    final state = widget.state;
    final style = AmalfiType.body(color: look.glassInkSecondary);
    return SizedBox(
      width: 160,
      height: 28,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: widget.focusNode.requestFocus,
        child: LockGlass(
          look: look,
          radius: 14,
          edge: widget.edge,
          child: Stack(
            children: [
              // The input itself: obscured, nothing of it is seen.
              Positioned(
                left: 12,
                top: 6,
                width: 110,
                height: 16,
                child: Semantics(
                  label: l.lockPasswordField,
                  child: EditableText(
                    controller: widget.controller,
                    focusNode: widget.focusNode,
                    style: const TextStyle(
                      color: Color(0x00000000),
                      fontSize: 13,
                    ),
                    cursorColor: const Color(0x00000000),
                    backgroundCursorColor: const Color(0x00000000),
                    selectionColor: const Color(0x00000000),
                    showCursor: false,
                    obscureText: true,
                    autocorrect: false,
                    enableSuggestions: false,
                    maxLines: 1,
                    textInputAction: TextInputAction.done,
                    // Return must not give the focus away (the default for "done"): the
                    // field stays the first responder across a failed try.
                    onEditingComplete: () {},
                    onSubmitted: (_) => widget.onSubmit(),
                  ),
                ),
              ),
              Positioned.fill(
                child: IgnorePointer(
                  child: CustomPaint(
                    painter: LockFieldPainter(
                      count: count,
                      dot: look.glassInk,
                      caret:
                          focused && _caret && state != LockFieldState.verifying
                          ? look.glassInk
                          : null,
                      selection: state == LockFieldState.wrong
                          ? look.colors.lineFocus
                          : null,
                    ),
                  ),
                ),
              ),
              if (count == 0 && state != LockFieldState.verifying)
                Positioned(
                  left: LockFieldMetrics.placeholderX,
                  right: 4,
                  top: 0,
                  bottom: 0,
                  child: IgnorePointer(
                    child: Align(
                      alignment: Alignment.centerLeft,
                      child: Text(
                        widget.placeholder,
                        key: const ValueKey('lock placeholder'),
                        maxLines: 1,
                        overflow: TextOverflow.clip,
                        style: style,
                      ),
                    ),
                  ),
                ),
              if (state == LockFieldState.verifying)
                const Positioned(
                  left: LockFieldMetrics.spinnerX,
                  top: LockFieldMetrics.spinnerY,
                  child: AmalfiSpinner(size: AmalfiProgressSize.small),
                )
              else if (count > 0)
                Positioned(
                  left: LockFieldMetrics.submitX,
                  top: LockFieldMetrics.submitY,
                  child: SizedBox.square(
                    dimension: LockFieldMetrics.submitRing,
                    child: LockButton(
                      key: const ValueKey('lock submit'),
                      onTap: widget.onSubmit,
                      label: l.lockSubmit,
                      child: Stack(
                        children: [
                          CustomPaint(
                            size: const Size.square(24),
                            painter: _RingPainter(look.glassInkSecondary),
                          ),
                          Center(
                            child: AmalfiSymbol(
                              'arrow-right',
                              size: 16,
                              color: look.glassInkSecondary,
                            ),
                          ),
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
}

class _RingPainter extends CustomPainter {
  const _RingPainter(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawCircle(
      size.center(Offset.zero),
      size.shortestSide / 2 - 0.75,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1.5
        ..color = color,
    );
  }

  @override
  bool shouldRepaint(_RingPainter old) => old.color != color;
}

/// The dots, the selection and the caret of the field.
class LockFieldPainter extends CustomPainter {
  const LockFieldPainter({
    required this.count,
    required this.dot,
    this.caret,
    this.selection,
  });

  final int count;
  final Color dot;

  /// The caret's colour; null: it is in its off phase or the field has no focus.
  final Color? caret;

  /// The selection capsule's colour (`Line/Focus`) after a failure; null: none.
  final Color? selection;

  @override
  void paint(Canvas canvas, Size size) {
    final shown = math.min(count, LockFieldMetrics.visibleDots);
    if (selection != null && count > 0) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          LockFieldMetrics.selection(count),
          const Radius.circular(4),
        ),
        Paint()..color = selection!,
      );
    }
    final paint = Paint()..color = dot;
    for (var i = 0; i < shown; i++) {
      canvas.drawCircle(
        Offset(
          LockFieldMetrics.dotLeft(i) + LockFieldMetrics.dot / 2,
          size.height / 2,
        ),
        LockFieldMetrics.dot / 2,
        paint,
      );
    }
    if (caret != null && selection == null) {
      canvas.drawRect(LockFieldMetrics.caret(count), Paint()..color = caret!);
    }
  }

  @override
  bool shouldRepaint(LockFieldPainter old) =>
      old.count != count ||
      old.dot != dot ||
      old.caret != caret ||
      old.selection != selection;
}

// ---- Clock ------------------------------------------------------------------------------

/// The date: `Inter 40 Semibold`, white 84 %.
TextStyle lockDateStyle(LockLook look) => AmalfiType.title1().copyWith(
  fontSize: 40,
  fontWeight: FontWeight.w600,
  height: 1,
  color: look.increaseContrast
      ? const Color(0xFFFFFFFF)
      : const Color(0xD6FFFFFF),
  shadows: look.textShadow,
);

const TextStyle _digits = TextStyle(
  fontFamily: 'Inter',
  package: 'silica_ui',
  fontSize: 200,
  fontWeight: FontWeight.w700,
  height: 1,
  leadingDistribution: TextLeadingDistribution.even,
  fontFeatures: [FontFeature.tabularFigures()],
);

/// The clock digits (`Inter 200 Bold`, tabular): white 78 % with a 1 pt white 90 % line inside
/// and a 1 pt black 24 % rim outside (the backdrop blur through the glyphs is not drawn: the
/// fill is translucent over the wallpaper). Reduce transparency: white 92 %. Increase contrast:
/// solid white, a 2 pt black 40 % rim, no inner line.
class _DigitText extends StatelessWidget {
  const _DigitText(this.text, this.look);

  final String text;
  final LockLook look;

  @override
  Widget build(BuildContext context) {
    final contrast = look.increaseContrast;
    final fill = contrast
        ? const Color(0xFFFFFFFF)
        : look.reduceTransparency
        ? const Color(0xEBFFFFFF)
        : const Color(0xC7FFFFFF);
    final rim = contrast
        ? const Color(0x66000000)
        : look.dark
        ? const Color(0x3D000000)
        : const Color(0x52000000);
    Widget layer(TextStyle s) => Text(text, style: s, softWrap: false);
    return Stack(
      children: [
        layer(
          _digits.copyWith(
            foreground: Paint()
              ..style = PaintingStyle.stroke
              ..strokeWidth = contrast ? 4 : 2
              ..color = rim,
          ),
        ),
        layer(_digits.copyWith(color: fill)),
        if (!contrast)
          layer(
            _digits.copyWith(
              foreground: Paint()
                ..style = PaintingStyle.stroke
                ..strokeWidth = 1
                ..color = const Color(0xE6FFFFFF),
            ),
          ),
      ],
    );
  }
}

/// The distance from the top of a 200 pt line to the top of its digits, from the font itself.
double digitInset(String sample) {
  final p = TextPainter(
    text: TextSpan(text: sample, style: _digits),
    textDirection: TextDirection.ltr,
  )..layout();
  final inset =
      p.computeDistanceToActualBaseline(TextBaseline.alphabetic) -
      LockMetrics.digitHeight;
  p.dispose();
  return inset.isFinite ? math.max(0, inset) : 0;
}

double _advance(String text) {
  final p = TextPainter(
    text: TextSpan(text: text, style: _digits),
    textDirection: TextDirection.ltr,
  )..layout();
  final w = p.width;
  p.dispose();
  return w;
}

/// The time. A minute change rolls only the digits that change, upwards, ~900 ms decelerating:
/// the old digit goes up and fades out, the new one comes up from below and fades in, through
/// a 24 pt vertical fade mask; the ones digit starts 60 ms after the tens. Reduce motion: a
/// 150 ms cross-fade. The cells have the digit height (145) and the tabular width, so that the
/// other digits never move.
class LockClock extends StatefulWidget {
  const LockClock({super.key, required this.time, required this.look});

  final String time;
  final LockLook look;

  @override
  State<LockClock> createState() => LockClockState();
}

class LockClockState extends State<LockClock>
    with SingleTickerProviderStateMixin {
  late final AnimationController _roll = AnimationController(vsync: this);
  String _old = '';
  late final double _inset = digitInset('0');

  /// The progress 0-1 of the roll of a cell, [index] counting the digits of the time.
  double progressOf(int index) => _progress(_roll.value, index);

  /// Whether a roll is running.
  bool get rolling => _roll.isAnimating;

  Duration get _total =>
      widget.look.reduceMotion ? fadeQuick : minuteRoll + minuteRollOnesDelay;

  double _progress(double v, int digitIndex) {
    final total = _total.inMicroseconds;
    final delay = widget.look.reduceMotion || digitIndex.isEven
        ? 0
        : minuteRollOnesDelay.inMicroseconds;
    final length = widget.look.reduceMotion ? total : minuteRoll.inMicroseconds;
    return ((v * total - delay) / length).clamp(0.0, 1.0);
  }

  @override
  void initState() {
    super.initState();
    _old = widget.time;
  }

  @override
  void didUpdateWidget(LockClock old) {
    super.didUpdateWidget(old);
    if (old.time == widget.time) return;
    _old = old.time;
    if (_old.length != widget.time.length) {
      _roll.value = 1;
      return;
    }
    _roll.duration = _total;
    _roll.forward(from: 0);
  }

  @override
  void dispose() {
    _roll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final look = widget.look;
    final time = widget.time;
    return Semantics(
      label: time,
      excludeSemantics: true,
      child: AnimatedBuilder(
        animation: _roll,
        builder: (context, _) {
          var digit = 0;
          final cells = <Widget>[];
          final chars = time.characters.toList();
          final old = _old.characters.toList();
          for (var i = 0; i < chars.length; i++) {
            final c = chars[i];
            final isDigit = RegExp(r'\d').hasMatch(c);
            final index = isDigit ? digit++ : 0;
            final changing =
                isDigit &&
                old.length == chars.length &&
                old[i] != c &&
                _roll.isAnimating;
            final width = _advance(isDigit ? '0' : c);
            Widget cell(String s, {double dy = 0, double opacity = 1}) =>
                Positioned(
                  left: 0,
                  top: -_inset + dy,
                  child: Opacity(opacity: opacity, child: _DigitText(s, look)),
                );
            Widget inner;
            if (!changing) {
              inner = Stack(clipBehavior: Clip.none, children: [cell(c)]);
            } else {
              final p = _progress(_roll.value, index);
              final e = look.reduceMotion ? p : minuteRollCurve.transform(p);
              final travel = look.reduceMotion ? 0.0 : LockMetrics.digitHeight;
              inner = ShaderMask(
                blendMode: BlendMode.dstIn,
                shaderCallback: (r) => ui.Gradient.linear(
                  Offset(0, r.top),
                  Offset(0, r.bottom),
                  const [
                    Color(0x00000000),
                    Color(0xFF000000),
                    Color(0xFF000000),
                    Color(0x00000000),
                  ],
                  [
                    0,
                    minuteRollMask / LockMetrics.digitHeight,
                    1 - minuteRollMask / LockMetrics.digitHeight,
                    1,
                  ],
                ),
                child: Stack(
                  clipBehavior: Clip.none,
                  children: [
                    cell(old[i], dy: -travel * e, opacity: 1 - e),
                    cell(c, dy: travel * (1 - e), opacity: e),
                  ],
                ),
              );
              if (look.reduceMotion)
                inner = Stack(
                  children: [
                    cell(old[i], opacity: 1 - e),
                    cell(c, opacity: e),
                  ],
                );
            }
            cells.add(
              SizedBox(
                key: ValueKey('lock digit $i'),
                width: width,
                height: LockMetrics.digitHeight,
                child: ClipRect(child: inner),
              ),
            );
          }
          return Row(mainAxisSize: MainAxisSize.min, children: cells);
        },
      ),
    );
  }
}

// ---- Status items -----------------------------------------------------------------------

/// The status items at the top right (kit `Lock Screen/Status Items`): Wi-Fi and Battery, 36 x 28
/// each, glyph 20, 4 apart, white over the wallpaper. The input source item is not drawn: the
/// shell has no input source data.
class LockStatusItems extends StatelessWidget {
  const LockStatusItems({super.key, required this.system, required this.look});

  final SystemSource? system;
  final LockLook look;

  @override
  Widget build(BuildContext context) {
    final source = system;
    if (source == null) return const SizedBox.shrink();
    return ValueListenableBuilder<SystemSnapshot>(
      valueListenable: source,
      builder: (context, s, _) {
        final items = <Widget>[
          if (s.wired && s.wifiSsid.isEmpty)
            _item(StatusKind.wired, const ValueKey('lock status wired'))
          else if (s.wifiAvailable)
            _item(
              StatusKind.network,
              const ValueKey('lock status wifi'),
              level: s.wifiStrength / 100,
              dim: !s.wifiEnabled || s.wifiSsid.isEmpty,
            ),
          if (s.batteryPresent)
            _item(
              StatusKind.battery,
              const ValueKey('lock status battery'),
              level: s.batteryPercent / 100,
              charging: s.batteryCharging,
            ),
        ];
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var i = 0; i < items.length; i++) ...[
              if (i > 0) const SizedBox(width: 4),
              items[i],
            ],
          ],
        );
      },
    );
  }

  Widget _item(
    StatusKind kind,
    Key key, {
    double level = 1,
    bool dim = false,
    bool charging = false,
  }) => SizedBox(
    key: key,
    width: 36,
    height: 28,
    child: Center(
      child: StatusIcon(
        kind: kind,
        color: look.ink,
        level: level,
        dim: dim,
        charging: charging,
      ),
    ),
  );
}

// ---- Banners ----------------------------------------------------------------------------

/// A collapsed banner (kit `Lock Screen/Notification/Collapsed`): 360 x 60, `Glass/Regular`
/// radius 18, the app's icon 36 at 12, the app name, "N notifications", the time. The body is
/// never shown. It slides in from the right edge (exponential, tau 71 ms) with a 210 ms fade.
class LockBanner extends StatefulWidget {
  const LockBanner({
    super.key,
    required this.latest,
    required this.count,
    required this.look,
    this.animate = true,
  });

  final ShellNotification latest;
  final int count;
  final LockLook look;
  final bool animate;

  @override
  State<LockBanner> createState() => _LockBannerState();
}

class _LockBannerState extends State<LockBanner>
    with SingleTickerProviderStateMixin {
  late final AnimationController _in = AnimationController(
    vsync: this,
    duration: bannerSlide,
  );

  @override
  void initState() {
    super.initState();
    if (widget.animate) {
      _in.forward();
    } else {
      _in.value = 1;
    }
  }

  @override
  void dispose() {
    _in.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l = l10nOf(context);
    final colors = widget.look.colors;
    final n = widget.latest;
    final time = DateFormat.Hm(l.localeName).format(n.timestamp);
    final app = n.appName.isEmpty ? n.desktopEntry : n.appName;
    final count = l.lockNotificationCount(widget.count);
    return Semantics(
      label: '$app, $count',
      excludeSemantics: true,
      child: AnimatedBuilder(
        animation: _in,
        builder: (context, child) {
          final ms = _in.value * bannerSlide.inMilliseconds;
          return Opacity(
            opacity: widget.look.reduceMotion
                ? 1
                : (ms / bannerFade.inMilliseconds).clamp(0.0, 1.0),
            child: Transform.translate(
              offset: Offset(
                widget.look.reduceMotion ? 0 : 376 * bannerSlideLeft(ms),
                0,
              ),
              child: child,
            ),
          );
        },
        child: SizedBox(
          width: 360,
          height: 60,
          child: AmalfiGlass(
            material: AmalfiMaterials.regular,
            radius: 18,
            child: Stack(
              children: [
                Positioned(
                  left: 12,
                  top: 12,
                  width: 36,
                  height: 36,
                  child: AppIcon(n),
                ),
                Positioned(
                  left: 60,
                  right: 64,
                  top: 10,
                  child: Text(
                    app,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AmalfiType.headline(color: colors.textPrimary),
                  ),
                ),
                Positioned(
                  left: 60,
                  right: 64,
                  top: 30,
                  child: Text(
                    count,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AmalfiType.callout(color: colors.textSecondary),
                  ),
                ),
                Positioned(
                  right: 12,
                  top: 12,
                  child: Text(
                    time,
                    style: AmalfiType.caption(color: colors.textTertiary),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

// ---- Power row (provisional) --------------------------------------------------------------

/// One action of the power row (kit `Lock Screen/Action Button`): a 48 `Glass/Lock` circle,
/// a 24 glyph, the label 12 below. 80 x 72.
class LockActionButton extends StatelessWidget {
  const LockActionButton({
    super.key,
    required this.look,
    required this.symbol,
    required this.label,
    required this.onTap,
  });

  final LockLook look;
  final String symbol;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 80,
      height: 72,
      child: LockButton(
        onTap: onTap,
        label: label,
        overlay: 48,
        child: Column(
          children: [
            SizedBox.square(
              dimension: 48,
              child: LockGlass(
                look: look,
                radius: 24,
                child: Center(
                  child: AmalfiSymbol(symbol, size: 24, color: look.glassInk),
                ),
              ),
            ),
            const SizedBox(height: 4),
            Text(
              label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: AmalfiType.callout(color: look.ink)
                  .copyWith(shadows: look.textShadow),
            ),
          ],
        ),
      ),
    );
  }
}
