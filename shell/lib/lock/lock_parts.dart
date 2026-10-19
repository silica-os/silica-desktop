// The parts of the lock screen (kit page "Lock Screen", masters `Lock Screen/...`): the
// appearance, the Glass/Lock material, the password field and its states, the help button, the
// glass clock with its minute roll, the user block, the hint, the collapsed banners and the
// optional power row. Look and measures from docs/design/amalfi/lock-screen.md; behaviour and
// timings from docs/spec/lock.md (L-01). The status items are the shared Status Bar
// (`status/status_items.dart`).

import 'dart:async';
import 'dart:typed_data' show Float64List;
import 'dart:ui' as ui;

import 'package:flutter/services.dart' show LogicalKeyboardKey, TextInputAction;
import 'package:flutter/widgets.dart';
import 'package:intl/intl.dart' show DateFormat;
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../notifications/model.dart';
import '../notifications/notifications.dart' show AppIcon;
import 'lock_layout.dart';

/// What the lock screen draws with: the appearance and the accessibility settings. The dark
/// appearance is the measured one; the light one, Increase contrast and Reduce transparency are
/// Amalfi decisions (provisional).
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

  Brightness get brightness => dark ? Brightness.dark : Brightness.light;

  AmalfiColors get colors => AmalfiColors.of(brightness);

  /// The Status Bar's treatment (Increase contrast first, as the system's).
  AmalfiStatusBarLegibility get legibility => increaseContrast
      ? AmalfiStatusBarLegibility.increaseContrast
      : reduceTransparency
      ? AmalfiStatusBarLegibility.reduceTransparency
      : AmalfiStatusBarLegibility.standard;

  /// Text straight on the wallpaper (name, hint, labels): on-photo white, the dark
  /// appearance's `Text/Primary` (#F4F5F7) in both; solid white under Increase contrast.
  Color get onPhoto => increaseContrast
      ? const Color(0xFFFFFFFF)
      : AmalfiColors.dark.textPrimary;

  /// The help address: `Dark/Text/Secondary` in dark, white 80 % in light.
  Color get onPhotoSecondary =>
      dark ? AmalfiColors.dark.textSecondary : const Color(0xCCFFFFFF);

  /// The labels of the action buttons: on-photo white, pure white in light (the kit's).
  Color get onPhotoLabel => dark ? onPhoto : const Color(0xFFFFFFFF);

  /// `shadows.onPhoto` under every text drawn on the photo in the light appearance.
  List<Shadow>? get onPhotoShadows => dark ? null : AmalfiShadows.onPhoto;

  /// The 1 pt rim of Increase contrast around text on the photo (name, date).
  Color? get onPhotoRim => increaseContrast ? const Color(0x66000000) : null;

  /// What is drawn inside the glass (dots, caret, glyphs): `Text/Primary` of the appearance.
  Color get glassInk => colors.textPrimary;
  Color get glassInkSecondary => colors.textSecondary;

  /// The `Glass/Lock` treatment in effect: null for the frosted glass, else the opaque variant.
  AmalfiGlassVariant? get glassVariant => increaseContrast
      ? AmalfiMaterials.lock.increaseContrast(brightness)
      : reduceTransparency
      ? AmalfiMaterials.lock.reduceTransparency(brightness)
      : null;
}

/// The colour matrix that multiplies the saturation by [s] (Rec. 709 luma weights) and the
/// brightness by [gain].
List<double> lockToneMatrix(double s, [double gain = 1]) {
  const r = 0.2126, g = 0.7152, b = 0.0722;
  final i = 1 - s;
  return [
    (r * i + s) * gain, g * i * gain, b * i * gain, 0, 0, //
    r * i * gain, (g * i + s) * gain, b * i * gain, 0, 0, //
    r * i * gain, g * i * gain, (b * i + s) * gain, 0, 0, //
    0, 0, 0, 1, 0,
  ];
}

/// `Glass/Lock` (tokens.json › materials): the wallpaper blurred (20) and saturated (1.1)
/// behind white 28 % (dark) or 60 % (light), a 1 pt edge inside and a rim light along the top;
/// no refraction, no shadow, and its blur never animates. Increase contrast and Reduce
/// transparency draw the material's opaque variants. Flutter draws it: the wallpaper is in the
/// same view (the lock surface is a view of its own).
class LockGlass extends StatelessWidget {
  const LockGlass({
    super.key,
    required this.look,
    this.shape = const CircleBorder(),
    this.edge,
    this.child,
  });

  final LockLook look;

  /// The outline: a circle, or a capsule (`AmalfiSmoothBorder` of half the height).
  final ShapeBorder shape;

  /// The edge colour when it is not the material's (the reduce-motion error flash).
  final Color? edge;
  final Widget? child;

  @override
  Widget build(BuildContext context) {
    final m = AmalfiMaterials.lock;
    final b = look.brightness;
    final variant = look.glassVariant;
    final face = CustomPaint(
      painter: LockGlassFacePainter(
        shape: shape,
        fill: variant?.fill ?? m.tint(b) ?? const Color(0x00000000),
        rim: edge ?? variant?.rimInside ?? m.rimInside(b),
        rimLight: variant == null ? m.rimLight(b) : variant.rimLight,
      ),
      child: child,
    );
    if (variant != null) return face;
    return Stack(
      fit: StackFit.passthrough,
      children: [
        Positioned.fill(
          child: ClipPath(
            clipper: ShapeBorderClipper(shape: shape),
            child: BackdropFilter(
              filter: ui.ImageFilter.compose(
                outer: ColorFilter.matrix(lockToneMatrix(m.saturation)),
                inner: ui.ImageFilter.blur(
                  sigmaX: m.backgroundBlur,
                  sigmaY: m.backgroundBlur,
                  tileMode: TileMode.clamp,
                ),
              ),
              child: const SizedBox.expand(),
            ),
          ),
        ),
        face,
      ],
    );
  }
}

/// The face of `Glass/Lock` (and of the avatar disc): the fill, the rim light (an inner
/// shadow 0, 1, blur 1) and the 1 pt edge inside.
class LockGlassFacePainter extends CustomPainter {
  const LockGlassFacePainter({
    required this.shape,
    required this.fill,
    required this.rim,
    required this.rimLight,
  });

  final ShapeBorder shape;
  final Color fill;
  final Color rim;
  final Color? rimLight;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final outline = shape.getOuterPath(rect);
    canvas.drawPath(outline, Paint()..color = fill);
    canvas.save();
    canvas.clipPath(outline);
    if (rimLight case final light?) {
      canvas.drawPath(
        Path()
          ..fillType = PathFillType.evenOdd
          ..addRect(rect.inflate(4))
          ..addPath(outline, const Offset(0, 1)),
        Paint()
          ..color = light
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 0.5),
      );
    }
    canvas.drawPath(
      outline,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2
        ..color = rim,
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(LockGlassFacePainter old) =>
      old.shape != shape ||
      old.fill != fill ||
      old.rim != rim ||
      old.rimLight != rimLight;
}

/// A button of the lock screen: a hover and press overlay (`fade.quick` in, 60 ms out) over
/// [overlay] (a circle of that side at the top centre; null: the whole button), a pointer,
/// Tab focus and Return/Space.
class LockButton extends StatefulWidget {
  const LockButton({
    super.key,
    required this.onTap,
    required this.label,
    required this.child,
    this.overlay,
    this.overlayColor = const Color(0x1FFFFFFF),
  });

  final VoidCallback onTap;
  final String label;
  final Widget child;
  final double? overlay;
  final Color overlayColor;

  @override
  State<LockButton> createState() => _LockButtonState();
}

class _LockButtonState extends State<LockButton> {
  bool _hover = false;
  bool _down = false;
  bool _focused = false;

  @override
  Widget build(BuildContext context) {
    final on = _hover || _down || _focused;
    return Semantics(
      button: true,
      label: widget.label,
      excludeSemantics: true,
      onTap: widget.onTap,
      child: FocusableActionDetector(
        mouseCursor: SystemMouseCursors.click,
        onShowHoverHighlight: (v) => setState(() => _hover = v),
        onShowFocusHighlight: (v) => setState(() => _focused = v),
        // Return and Space press the focused button (the lock view has no app shortcuts).
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.enter): ActivateIntent(),
          SingleActivator(LogicalKeyboardKey.numpadEnter): ActivateIntent(),
          SingleActivator(LogicalKeyboardKey.space): ActivateIntent(),
        },
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
                        duration: on ? fadeQuick : fadeOut,
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            color: widget.overlayColor,
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

/// The help button (kit `Lock Screen/Help Button/*`): a 32 `Glass/Lock` circle, `?` in
/// `Title 3` (16/22, 600) `Text/Primary`, centred.
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
  Widget build(BuildContext context) => SizedBox.square(
    dimension: LockMetrics.help,
    child: LockButton(
      onTap: onTap,
      label: l10nOf(context).lockHelpButton,
      child: LockGlass(
        look: look,
        edge: edge,
        child: Center(
          child: Text(
            '?',
            key: const ValueKey('lock help glyph'),
            textAlign: TextAlign.center,
            style: AmalfiType.title3(color: look.glassInk),
          ),
        ),
      ),
    ),
  );
}

/// The password field (kit `Lock Screen/Password Field/<State>`): 208 × 32 and never another
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
  void didUpdateWidget(LockPasswordField old) {
    super.didUpdateWidget(old);
    if (old.focusNode != widget.focusNode) {
      old.focusNode.removeListener(_focusChanged);
      widget.focusNode.addListener(_focusChanged);
    }
    if (old.controller != widget.controller) {
      old.controller.removeListener(_typed);
      widget.controller.addListener(_typed);
    }
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
    final colors = look.colors;
    final l = l10nOf(context);
    final count = widget.controller.text.runes.length;
    final focused = widget.focusNode.hasFocus;
    final state = widget.state;
    final verifying = state == LockFieldState.verifying;
    return SizedBox.fromSize(
      size: LockMetrics.fieldSize,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: widget.focusNode.requestFocus,
        child: LockGlass(
          look: look,
          shape: const AmalfiSmoothBorder(radius: LockFieldMetrics.radius),
          edge: widget.edge,
          child: Stack(
            children: [
              // The input itself: obscured, nothing of it is seen.
              Positioned(
                left: LockFieldMetrics.caretX,
                top: LockFieldMetrics.caretY,
                width: LockFieldMetrics.submitX - LockFieldMetrics.caretX - 4,
                height: LockFieldMetrics.caretHeight,
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
                    key: const ValueKey('lock field ink'),
                    painter: LockFieldPainter(
                      count: count,
                      dot: look.glassInk,
                      caret:
                          focused &&
                              _caret &&
                              !verifying &&
                              state != LockFieldState.wrong
                          ? look.glassInk
                          : null,
                      selection: state == LockFieldState.wrong
                          ? colors.lineFocus
                          : null,
                    ),
                  ),
                ),
              ),
              if (count == 0 && !verifying)
                Positioned(
                  left: LockFieldMetrics.placeholderX,
                  top: LockFieldMetrics.placeholderY,
                  width: 160,
                  child: IgnorePointer(
                    child: Text(
                      widget.placeholder,
                      key: const ValueKey('lock placeholder'),
                      maxLines: 1,
                      softWrap: false,
                      overflow: TextOverflow.fade,
                      style: AmalfiType.body(color: look.glassInkSecondary),
                    ),
                  ),
                ),
              if (verifying)
                const Positioned(
                  left: LockFieldMetrics.spinnerX,
                  top: LockFieldMetrics.spinnerY,
                  child: AmalfiSpinner(
                    key: ValueKey('lock spinner'),
                    size: AmalfiProgressSize.small,
                  ),
                )
              else
                Positioned(
                  left: LockFieldMetrics.submitX,
                  top: LockFieldMetrics.submitY,
                  width: LockFieldMetrics.submit,
                  height: LockFieldMetrics.submit,
                  // The submit comes with the first character (`fade.quick`) and goes in
                  // 60 ms when the field is emptied.
                  child: AnimatedOpacity(
                    key: const ValueKey('lock submit fade'),
                    opacity: count > 0 ? 1 : 0,
                    duration: count > 0 ? fadeQuick : fadeOut,
                    child: IgnorePointer(
                      ignoring: count == 0,
                      // Not in the Tab order (field → help → input source): Return in the
                      // field submits.
                      child: ExcludeFocus(
                        child: LockButton(
                          key: const ValueKey('lock submit'),
                          onTap: widget.onSubmit,
                          label: l.lockSubmit,
                          overlayColor: const Color(0x29FFFFFF),
                          child: DecoratedBox(
                            key: const ValueKey('lock submit disc'),
                            decoration: BoxDecoration(
                              shape: BoxShape.circle,
                              color: colors.accentAmalfi,
                            ),
                            child: Center(
                              child: AmalfiSymbol(
                                'arrow-right',
                                size: LockFieldMetrics.submitGlyph,
                                color: colors.textOnAccent,
                              ),
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

  /// The caret's colour; null: it is in its off phase, hidden, or the field has no focus.
  final Color? caret;

  /// The selection capsule's colour (`Line/Focus`) after a failure; null: none.
  final Color? selection;

  @override
  void paint(Canvas canvas, Size size) {
    final shown = LockFieldMetrics.shown(count);
    if (selection case final color? when count > 0) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          LockFieldMetrics.selection(count),
          const Radius.circular(LockFieldMetrics.caretHeight / 2),
        ),
        Paint()..color = color,
      );
    }
    final paint = Paint()..color = dot;
    for (var i = 0; i < shown; i++) {
      canvas.drawOval(
        Rect.fromLTWH(
          LockFieldMetrics.dotLeft(i),
          LockFieldMetrics.dotY,
          LockFieldMetrics.dot,
          LockFieldMetrics.dot,
        ),
        paint,
      );
    }
    if (caret case final color?) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          LockFieldMetrics.caret(count),
          const Radius.circular(1),
        ),
        Paint()..color = color,
      );
    }
  }

  @override
  bool shouldRepaint(LockFieldPainter old) =>
      old.count != count ||
      old.dot != dot ||
      old.caret != caret ||
      old.selection != selection;
}

// ---- Clock --------------------------------------------------------------------------------

/// Text on the photo: [style] in [color], with the light appearance's `shadows.onPhoto` and,
/// under Increase contrast, a 1 pt black 40 % rim outside the glyphs ([rimWidth] for others).
class LockPhotoText extends StatelessWidget {
  const LockPhotoText(
    this.text, {
    super.key,
    required this.style,
    required this.look,
    this.color,
    this.maxLines = 1,
    this.textAlign = TextAlign.center,
    this.rimWidth = 1,
  });

  final String text;
  final TextStyle style;
  final LockLook look;

  /// The ink; null: on-photo white.
  final Color? color;
  final int maxLines;
  final TextAlign textAlign;
  final double rimWidth;

  @override
  Widget build(BuildContext context) {
    final ink = style.copyWith(
      color: color ?? look.onPhoto,
      shadows: look.onPhotoShadows,
    );
    Widget line(TextStyle s, {Key? key}) => Text(
      text,
      key: key,
      maxLines: maxLines,
      softWrap: maxLines > 1,
      overflow: TextOverflow.ellipsis,
      textAlign: textAlign,
      style: s,
    );
    final rim = look.onPhotoRim;
    if (rim == null) return line(ink);
    return Stack(
      alignment: Alignment.center,
      children: [
        // A 2 × width stroke under the solid fill: only its outer half shows.
        ExcludeSemantics(
          child: line(
            style.copyWith(
              foreground: Paint()
                ..style = PaintingStyle.stroke
                ..strokeWidth = rimWidth * 2
                ..strokeJoin = StrokeJoin.round
                ..color = rim,
            ),
          ),
        ),
        line(ink),
      ],
    );
  }
}

/// The date (`Lock Date` 32/40, 600): white 84 % (92 % under Reduce transparency, solid white
/// with a 1 pt rim under Increase contrast), with the light appearance's on-photo shadow.
class LockDate extends StatelessWidget {
  const LockDate({super.key, required this.date, required this.look});

  final String date;
  final LockLook look;

  @override
  Widget build(BuildContext context) => LockPhotoText(
    date,
    look: look,
    style: AmalfiType.lockDate(),
    color: look.increaseContrast
        ? const Color(0xFFFFFFFF)
        : look.reduceTransparency
        ? AmalfiGlyphMaterials.clock.reduceTransparencyFill
        : const Color(0xD6FFFFFF),
  );
}

/// The text style of the clock: `Lock Clock` (Inter 200/200 Semibold, tracking −6, tabular).
TextStyle get lockClockStyle => AmalfiType.lockClock();

/// One string of the clock in `Glass/Clock` (tokens.json): the wallpaper under the glyphs,
/// blurred (8), saturated (1.3) and brightened (1.3), under a white 76 % fill, a 1 pt white 80 %
/// line inside the outline and a 1 pt black 32 % rim outside it. Reduce transparency: white
/// 92 % and the rim, no glass. Increase contrast: solid white with a 2 pt black 40 % rim (and
/// the on-photo shadow in light). The text starts [inset] from the box's left edge, so that ink
/// running past the advance (tracking −6) keeps its glass. The edge refraction and the specular
/// highlights of the token are not drawn yet (provisional: the glass shader works on shapes,
/// not glyphs).
class LockGlyphGlass extends StatelessWidget {
  const LockGlyphGlass(
    this.text, {
    super.key,
    required this.look,
    this.inset = 0,
    this.advances,
  });

  final String text;
  final LockLook look;
  final double inset;

  /// The advance of each character of [text], to draw the whole string in one glass (one
  /// backdrop blur and one mask for the clock instead of one per digit, which the virtio-gpu
  /// Venus ring of the dev VM does not always keep up with, silica-desktop#253): each
  /// character starts where the previous one's advance ends. Null: [text] is one run.
  final List<double>? advances;

  @override
  Widget build(BuildContext context) {
    final m = AmalfiGlyphMaterials.clock;
    final style = lockClockStyle;
    final steps = advances;
    Widget layer(TextStyle s) {
      if (steps == null) {
        return Positioned(
          left: inset,
          top: 0,
          child: Text(text, style: s, softWrap: false, maxLines: 1),
        );
      }
      final chars = text.characters.toList();
      var x = inset;
      final parts = <Widget>[];
      for (var i = 0; i < chars.length; i++) {
        parts.add(
          Positioned(
            left: x,
            top: 0,
            child: Text(chars[i], style: s, softWrap: false, maxLines: 1),
          ),
        );
        x += i < steps.length ? steps[i] : 0;
      }
      return Positioned.fill(
        child: Stack(clipBehavior: Clip.none, children: parts),
      );
    }

    Paint stroke(double width, Color color) => Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = width * 2
      ..strokeJoin = StrokeJoin.round
      ..color = color;
    if (look.increaseContrast) {
      return Stack(
        clipBehavior: Clip.none,
        children: [
          layer(
            style.copyWith(
              foreground: stroke(
                m.increaseContrastRimWidth,
                m.increaseContrastRim,
              ),
              shadows: look.onPhotoShadows,
            ),
          ),
          layer(style.copyWith(color: m.increaseContrastFill)),
        ],
      );
    }
    final rim = layer(
      style.copyWith(
        foreground: stroke(m.rimOutsideWidth, m.rimOutside(look.brightness)),
      ),
    );
    if (look.reduceTransparency) {
      return Stack(
        clipBehavior: Clip.none,
        children: [
          rim,
          layer(style.copyWith(color: m.reduceTransparencyFill)),
        ],
      );
    }
    return Stack(
      clipBehavior: Clip.none,
      children: [
        rim,
        Positioned.fill(
          child: _GlyphMask(
            text: text,
            style: style,
            inset: inset,
            advances: steps,
            child: Stack(
              children: [
                // Off the mask's edges (silica-desktop#250): see [_GlyphMask.edge].
                Positioned.fill(
                  left: _GlyphMask.edge,
                  top: _GlyphMask.edge,
                  right: _GlyphMask.edge,
                  bottom: _GlyphMask.edge,
                  child: ClipRect(
                    child: BackdropFilter(
                      filter: ui.ImageFilter.compose(
                        outer: ColorFilter.matrix(
                          lockToneMatrix(m.saturation, m.brightness),
                        ),
                        inner: ui.ImageFilter.blur(
                          sigmaX: m.backgroundBlur,
                          sigmaY: m.backgroundBlur,
                          tileMode: TileMode.clamp,
                        ),
                      ),
                      child: ColoredBox(color: m.fill),
                    ),
                  ),
                ),
                layer(
                  style.copyWith(
                    foreground: stroke(m.lineInsideWidth, m.lineInside),
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

/// Keeps [child] only inside the glyphs of [text] (destination-in with an image of the glyphs,
/// drawn at the display's resolution): the glass clipped to the digits.
///
/// The child must leave [edge] empty around it. The mask is applied over the layer's bounds as
/// an anti-aliased rectangle: where a bound falls inside a device pixel (a cell at a fractional
/// x at 1.5×, a digit rolling by fractions of a point) that pixel is only partly covered, and
/// destination-in keeps the uncovered part of the child unmasked, a one-pixel line of glass
/// down the whole box (silica-desktop#250).
class _GlyphMask extends StatefulWidget {
  /// The empty margin the child leaves inside the mask's bounds: more than one device pixel
  /// at any scale the lock screen runs at, and inside the cell's bleed, where no ink falls.
  static const double edge = 2;

  const _GlyphMask({
    required this.text,
    required this.style,
    required this.inset,
    this.advances,
    required this.child,
  });

  final String text;
  final TextStyle style;
  final double inset;
  final List<double>? advances;
  final Widget child;

  @override
  State<_GlyphMask> createState() => _GlyphMaskState();
}

class _GlyphMaskState extends State<_GlyphMask> {
  ui.Image? _mask;
  Object? _maskFor;

  @override
  void dispose() {
    _mask?.dispose();
    super.dispose();
  }

  ui.Image? _maskOf(Size size, double dpr) {
    final key = (
      widget.text,
      widget.style,
      widget.inset,
      Object.hashAll(widget.advances ?? const <double>[]),
      size,
      dpr,
    );
    if (key == _maskFor) return _mask;
    final width = (size.width * dpr).ceil();
    final height = (size.height * dpr).ceil();
    if (width <= 0 || height <= 0 || !size.isFinite) return null;
    final recorder = ui.PictureRecorder();
    final canvas = Canvas(recorder)..scale(dpr);
    final white = widget.style.copyWith(color: const Color(0xFFFFFFFF));
    final steps = widget.advances;
    final chars = steps == null
        ? [widget.text]
        : widget.text.characters.toList();
    var x = widget.inset;
    for (var i = 0; i < chars.length; i++) {
      final painter = TextPainter(
        text: TextSpan(text: chars[i], style: white),
        textDirection: TextDirection.ltr,
        maxLines: 1,
      )..layout();
      painter.paint(canvas, Offset(x, 0));
      painter.dispose();
      if (steps != null) x += i < steps.length ? steps[i] : 0;
    }
    final picture = recorder.endRecording();
    _mask?.dispose();
    _mask = picture.toImageSync(width, height);
    picture.dispose();
    _maskFor = key;
    return _mask;
  }

  @override
  Widget build(BuildContext context) {
    final dpr = MediaQuery.maybeDevicePixelRatioOf(context) ?? 1;
    return LayoutBuilder(
      builder: (context, constraints) {
        final mask = _maskOf(constraints.biggest, dpr);
        // Without a mask (no room) nothing of the glass shows.
        if (mask == null) return const SizedBox.shrink();
        final scale = 1 / dpr;
        return ShaderMask(
          blendMode: BlendMode.dstIn,
          shaderCallback: (_) => ImageShader(
            mask,
            TileMode.decal,
            TileMode.decal,
            Float64List.fromList([
              scale, 0, 0, 0, //
              0, scale, 0, 0, //
              0, 0, 1, 0, //
              0, 0, 0, 1,
            ]),
          ),
          child: widget.child,
        );
      },
    );
  }
}

/// The distance from the top of the 200 pt time box to the top of its digits, from the font.
double digitInset() {
  final p = TextPainter(
    text: TextSpan(text: '0', style: lockClockStyle),
    textDirection: TextDirection.ltr,
  )..layout();
  final inset =
      p.computeDistanceToActualBaseline(TextBaseline.alphabetic) -
      LockMetrics.digitHeight;
  p.dispose();
  return inset.isFinite && inset > 0 ? inset : 0;
}

double _advance(String text) {
  final p = TextPainter(
    text: TextSpan(text: text, style: lockClockStyle),
    textDirection: TextDirection.ltr,
  )..layout();
  final w = p.width;
  p.dispose();
  return w;
}

/// The time (kit `Lock Clock` in `Glass/Clock`), centred in the 960 × 200 time box. A minute
/// change rolls only the digits that change, upwards, ~900 ms decelerating: the old digit goes
/// up by the digit height and fades out, the new one comes up from below and fades in, through
/// a soft mask 24 pt beyond the top and the bottom of the digit box; the ones digit starts
/// 60 ms after the tens. Reduce motion: a `fade.quick` cross-fade. The cells have the tabular
/// width, so that the other digits never move.
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
  late final double _inset = digitInset();

  /// The room around a cell's glyphs that the glass may use (tracking −6 lets the ink run past
  /// the advance).
  static const double _bleed = 24;

  /// The progress 0–1 of the roll of the cell of digit [index] (counting digits only).
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
    final digitTop = _inset;
    final digitBottom = _inset + LockMetrics.digitHeight;
    return Semantics(
      label: time,
      excludeSemantics: true,
      child: AnimatedBuilder(
        animation: _roll,
        builder: (context, _) {
          var digit = 0;
          final cells = <Widget>[];
          final steps = <double>[];
          final chars = time.characters.toList();
          final rolling = _roll.isAnimating && _old.length == time.length;
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
            steps.add(width);
            Widget glyph(String s, {double dy = 0, double opacity = 1}) =>
                Positioned(
                  left: -_bleed,
                  top: dy,
                  width: width + 2 * _bleed,
                  height: LockMetrics.timeHeight,
                  child: Opacity(
                    opacity: opacity,
                    child: LockGlyphGlass(s, look: look, inset: _bleed),
                  ),
                );
            Widget inner;
            if (!rolling) {
              // At rest the glass of all the digits is drawn once, under the row.
              inner = const SizedBox.shrink();
            } else if (!changing) {
              inner = Stack(clipBehavior: Clip.none, children: [glyph(c)]);
            } else {
              final p = _progress(_roll.value, index);
              if (look.reduceMotion) {
                inner = Stack(
                  clipBehavior: Clip.none,
                  children: [
                    glyph(old[i], opacity: 1 - p),
                    glyph(c, opacity: p),
                  ],
                );
              } else {
                final e = minuteRollCurve.transform(p);
                const travel = LockMetrics.digitHeight;
                const h = LockMetrics.timeHeight;
                inner = ShaderMask(
                  key: ValueKey('lock roll mask $i'),
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
                      ((digitTop - minuteRollMask) / h).clamp(0.0, 1.0),
                      (digitTop / h).clamp(0.0, 1.0),
                      (digitBottom / h).clamp(0.0, 1.0),
                      ((digitBottom + minuteRollMask) / h).clamp(0.0, 1.0),
                    ],
                  ),
                  child: Stack(
                    clipBehavior: Clip.none,
                    children: [
                      glyph(old[i], dy: -travel * e, opacity: 1 - e),
                      glyph(c, dy: travel * (1 - e), opacity: e),
                    ],
                  ),
                );
              }
            }
            cells.add(
              SizedBox(
                key: ValueKey('lock digit $i'),
                width: width,
                height: LockMetrics.timeHeight,
                child: inner,
              ),
            );
          }
          final row = Row(mainAxisSize: MainAxisSize.min, children: cells);
          if (rolling) return row;
          final total = steps.fold<double>(0, (a, b) => a + b);
          return Stack(
            clipBehavior: Clip.none,
            children: [
              row,
              Positioned(
                left: -_bleed,
                top: 0,
                width: total + 2 * _bleed,
                height: LockMetrics.timeHeight,
                child: LockGlyphGlass(
                  time,
                  look: look,
                  inset: _bleed,
                  advances: steps,
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

// ---- User ---------------------------------------------------------------------------------

/// The avatar (kit `Lock Screen/User/*`): a 72 `Accent/Amalfi` disc with a 1 pt white 24 % edge
/// inside (60 % under Increase contrast) and a white 22 % rim light, and `person-fill` 40
/// `Text/On Accent` centred. No pictures are kept yet; the lock glyph when the user is not
/// shown.
class LockAvatar extends StatelessWidget {
  const LockAvatar({super.key, required this.look, required this.showUser});

  final LockLook look;
  final bool showUser;

  @override
  Widget build(BuildContext context) {
    final colors = look.colors;
    return SizedBox.square(
      dimension: LockMetrics.avatar,
      child: CustomPaint(
        key: const ValueKey('lock avatar disc'),
        painter: LockGlassFacePainter(
          shape: const CircleBorder(),
          fill: colors.accentAmalfi,
          rim: look.increaseContrast
              ? const Color(0x99FFFFFF)
              : const Color(0x3DFFFFFF),
          rimLight: const Color(0x38FFFFFF),
        ),
        child: Center(
          child: AmalfiSymbol(
            showUser ? 'person-fill' : 'lock',
            size: 40,
            color: colors.textOnAccent,
          ),
        ),
      ),
    );
  }
}

// ---- Banners ------------------------------------------------------------------------------

/// A collapsed banner (kit `Lock Screen/Notification/Collapsed`): 360 × 60, `Glass/Regular`
/// radius 18, the app's icon 36 at 12, the app name (`Headline`) at x 56, "N notifications"
/// (`Callout`, secondary) under it, the time (`Caption`, tertiary) at the right. The body is
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
                widget.look.reduceMotion
                    ? 0
                    : (LockMetrics.bannerSize.width + 16) * bannerSlideLeft(ms),
                0,
              ),
              child: child,
            ),
          );
        },
        child: SizedBox.fromSize(
          size: LockMetrics.bannerSize,
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
                  left: 56,
                  top: 12,
                  width: 240,
                  child: Text(
                    app,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AmalfiType.headline(color: colors.textPrimary),
                  ),
                ),
                Positioned(
                  left: 56,
                  top: 30,
                  width: 240,
                  child: Text(
                    count,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AmalfiType.callout(color: colors.textSecondary),
                  ),
                ),
                Positioned(
                  left: 300,
                  top: 12,
                  width: 48,
                  child: Text(
                    time,
                    maxLines: 1,
                    textAlign: TextAlign.right,
                    overflow: TextOverflow.fade,
                    softWrap: false,
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

/// One action of the power row (kit `Lock Screen/Action Button`): 80 × 72, a 48 `Glass/Lock`
/// circle at x 16, a 24 glyph in `Text/Primary` at its centre, the label (`Callout` 12) on the
/// photo 8 below.
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
  Widget build(BuildContext context) => SizedBox(
    width: 80,
    height: 72,
    child: LockButton(
      onTap: onTap,
      label: label,
      overlay: 48,
      child: Stack(
        children: [
          Positioned(
            left: 16,
            top: 0,
            width: 48,
            height: 48,
            child: LockGlass(
              look: look,
              child: Center(
                child: AmalfiSymbol(symbol, size: 24, color: look.glassInk),
              ),
            ),
          ),
          Positioned(
            left: 0,
            top: 56,
            width: 80,
            height: 16,
            child: LockPhotoText(
              label,
              look: look,
              color: look.onPhotoLabel,
              style: AmalfiType.callout(),
            ),
          ),
        ],
      ),
    ),
  );
}
