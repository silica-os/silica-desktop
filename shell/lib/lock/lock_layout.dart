// The geometry, the states and the timings of the lock screen: the Amalfi hand-off
// (docs/design/amalfi/lock-screen.md, kit page "Lock Screen") over the measurements of
// docs/spec/lock.md (L-01). Pure functions of the screen size, so that the tests can check them
// without drawing.

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

/// The state of the password field (kit `Lock Screen/Password Field/<State>`).
enum LockFieldState {
  /// Empty and focused: the caret and the placeholder.
  placeholder,

  /// Characters typed: one dot each, the caret and the submit button.
  typing,

  /// The password is being checked: the dots and a spinner in place of the submit.
  verifying,

  /// The last try failed: the dots stay, selected, and the field shook.
  wrong,
}

/// The stages of a wake from display-off (spec "Wake"): every change is a cut.
enum LockWakeStage {
  /// 0 ms: the wallpaper and the Status Bar only.
  wallpaper,

  /// +150 ms: date, clock, avatar and name, the user block 48 pt higher, no field.
  clock,

  /// +20 ms more: the field and the help button, the user block in place.
  full,
}

/// Positions in pt for a screen of a given size (the doc's "Screen layout" table): the top
/// block hangs from the top edge, the bottom block stands on the bottom edge, everything is
/// centred on the vertical axis except the help button and the Status Bar. Every frame is on
/// the 4 pt grid on even screen sizes.
abstract final class LockMetrics {
  /// The Status Bar's trailing edge from the right edge: the menu bar's padding, so nothing
  /// moves when the screen locks.
  static const double statusTrailing = 8;
  static const double statusHeight = 32;

  /// The clock block: 960 × 240, its top 104 from the top edge.
  static const double clockTop = 104;
  static const Size clockSize = Size(960, 240);

  /// The date's box at the top of the clock block (`Lock Date` 32/40).
  static const double dateHeight = 40;

  /// The time's box below it (`Lock Clock` 200/200).
  static const double timeTop = 40;
  static const double timeHeight = 200;

  /// The digits' height: the cap height of Inter 200 (>= the measured 142).
  static const double digitHeight = 145;

  static Rect clock(Size s) => Rect.fromLTWH(
    (s.width - clockSize.width) / 2,
    clockTop,
    clockSize.width,
    clockSize.height,
  );

  /// The user block: 320 × 108, its top 284 above the bottom edge; `raised` is the 48 pt
  /// higher place of the wake's second stage.
  static const Size userSize = Size(320, 108);
  static const double userFromBottom = 284;
  static const double wakeRise = 48;

  static Rect userBlock(Size s, {bool raised = false}) => Rect.fromLTWH(
    (s.width - userSize.width) / 2,
    s.height - userFromBottom - (raised ? wakeRise : 0),
    userSize.width,
    userSize.height,
  );

  /// The avatar: 72, at the top centre of the user block.
  static const double avatar = 72;

  static Rect avatarIn(Size s, {bool raised = false}) {
    final b = userBlock(s, raised: raised);
    return Rect.fromLTWH(
      b.left + (b.width - avatar) / 2,
      b.top,
      avatar,
      avatar,
    );
  }

  /// The name: a 320 × 22 box (`Title 3` 16/22), 12 below the avatar.
  static const double nameTop = 84;
  static const double nameHeight = 22;

  static Rect name(Size s, {bool raised = false}) {
    final b = userBlock(s, raised: raised);
    return Rect.fromLTWH(b.left, b.top + nameTop, b.width, nameHeight);
  }

  /// The password field: 208 × 32, its bottom 128 above the bottom edge. Never resized.
  static const Size fieldSize = Size(208, 32);
  static const double fieldFromBottom = 160;

  static Rect field(Size s) => Rect.fromLTWH(
    (s.width - fieldSize.width) / 2,
    s.height - fieldFromBottom,
    fieldSize.width,
    fieldSize.height,
  );

  /// The help button: 32, 8 right of the field, the same top; not part of the centring.
  static const double help = 32;
  static const double helpGap = 8;

  static Rect helpButton(Size s) {
    final f = field(s);
    return Rect.fromLTWH(f.right + helpGap, f.top, help, help);
  }

  /// The hint block of the hint state: 320 × 68, its bottom 24 above the field.
  static const Size hintSize = Size(320, 68);
  static const double hintGap = 24;

  static Rect hint(Size s) => Rect.fromLTWH(
    (s.width - hintSize.width) / 2,
    field(s).top - hintGap - hintSize.height,
    hintSize.width,
    hintSize.height,
  );

  /// The help address of the hint state: 320 × 16, 12 below the field.
  static const double addressGap = 12;

  static Rect helpAddress(Size s) =>
      Rect.fromLTWH((s.width - 320) / 2, field(s).bottom + addressGap, 320, 16);

  /// The banner `index` (0 first): 360 × 60, 16 from the right, the first 16 under the Status
  /// Bar (y 48), the next 8 below.
  static const Size bannerSize = Size(360, 60);

  static Rect banner(Size s, int index) => Rect.fromLTWH(
    s.width - 16 - bannerSize.width,
    48 + index * (bannerSize.height + 8),
    bannerSize.width,
    bannerSize.height,
  );

  /// The optional power row (provisional): 288 × 72, 32 below the field.
  static const Size powerSize = Size(288, 72);

  static Rect powerRow(Size s) => Rect.fromLTWH(
    (s.width - powerSize.width) / 2,
    field(s).bottom + 32,
    powerSize.width,
    powerSize.height,
  );
}

/// Inside the password field (field coordinates, pt; kit `Lock Screen/Password Field/*`).
abstract final class LockFieldMetrics {
  static const double radius = 16;

  /// The caret: 2 × 16 at x 16, y 8 when empty; 4 after the last dot otherwise.
  static const double caretX = 16;
  static const double caretY = 8;
  static const double caretWidth = 2;
  static const double caretHeight = 16;
  static const double caretAfterDot = 4;

  /// The placeholder (`Body` 13/18) at x 22, y 7.
  static const double placeholderX = 22;
  static const double placeholderY = 7;

  /// The dots: 8 pt, first at x 16, pitch 12, y 12.
  static const double dot = 8;
  static const double dotPitch = 12;
  static const double firstDotX = 16;
  static const double dotY = 12;

  /// The submit: a 24 disc at x 180, y 4, its arrow 16 at x 184, y 8; the spinner (16) takes
  /// the arrow's place while the password is checked.
  static const double submit = 24;
  static const double submitX = 180;
  static const double submitY = 4;
  static const double submitGlyph = 16;
  static const double spinnerX = 184;
  static const double spinnerY = 8;

  /// The selection after a failure: a capsule 16 tall, radius 8, from x 10 to 6 past the last
  /// dot, at y 8.
  static const double selectionLeft = 10;
  static const double selectionPastDot = 6;

  /// How many dots fit before the submit; more scroll inside the field (it never grows).
  static const int visibleDots = 13;

  /// The left edge of dot [i] (of those shown).
  static double dotLeft(int i) => firstDotX + dotPitch * i;

  /// The dots shown for [count] characters: the last [visibleDots].
  static int shown(int count) => math.min(count, visibleDots);

  /// The selection of [count] dots.
  static Rect selection(int count) {
    final n = shown(count);
    final right = n == 0
        ? selectionLeft + selectionPastDot
        : dotLeft(n - 1) + dot + selectionPastDot;
    return Rect.fromLTRB(selectionLeft, caretY, right, caretY + caretHeight);
  }

  /// The caret after [count] characters.
  static Rect caret(int count) {
    final n = shown(count);
    final x = n == 0 ? caretX : dotLeft(n - 1) + dot + caretAfterDot;
    return Rect.fromLTWH(x, caretY, caretWidth, caretHeight);
  }
}

// ---- Timings (spec "Animation timings"; the Amalfi geometry of lock-screen.md › Motion) ------

/// Lock: the clock, date and user block fade in over ~250 ms, almost linear with a soft end
/// (progress ~0.6 at half time).
const Duration lockFade = Duration(milliseconds: 250);
const Curve lockFadeCurve = Cubic(0.4, 0.4, 0.7, 1);

/// The user block starts slightly after the clock (offset unmeasured: provisional).
const Duration lockUserOffset = Duration(milliseconds: 30);

/// Unlock: the whole screen fades out, decelerating, done at ~350 ms.
const Duration unlockFade = Duration(milliseconds: 350);

/// 1 - e^(-t / 78 ms), cut at 350 ms: 95 % at ~235 ms.
class UnlockCurve extends Curve {
  const UnlockCurve();

  static const double _tau = 78;

  @override
  double transformInternal(double t) =>
      t >= 1 ? 1 : 1 - math.exp(-t * unlockFade.inMilliseconds / _tau);
}

/// Wake: the date, clock, avatar and name come 150 ms after the wallpaper, the field 20 ms
/// after them.
const Duration wakeClockDelay = Duration(milliseconds: 150);
const Duration wakeFieldDelay = Duration(milliseconds: 20);

/// The minute roll: ~900 ms, decelerating; the ones digit starts 60 ms after the tens. Travel =
/// the digit height, a soft mask of 24 at the top and bottom of the digit box (provisional).
const Duration minuteRoll = Duration(milliseconds: 900);
const Duration minuteRollOnesDelay = Duration(milliseconds: 60);
const Curve minuteRollCurve = Cubic(0.2, 0, 0, 1);
const double minuteRollMask = 24;

/// `fade.quick`: the submit's fade in, the hover overlays, the reduce-motion roll.
const Duration fadeQuick = Duration(milliseconds: 150);

/// The fade out of the submit and of the hover overlays.
const Duration fadeOut = Duration(milliseconds: 60);

/// The wrong-password shake: horizontal only, ~400 ms, first to the left. The measured
/// keyframes (−10, +14, −11, +10, −7, +6) scaled by the field's 208 / 160 = 1.3 (ms, pt).
const Duration lockShake = Duration(milliseconds: 400);
const List<(double, double)> lockShakeKeyframes = [
  (0, 0),
  (31, -13),
  (94, 18),
  (156, -14),
  (219, 13),
  (281, -9),
  (344, 8),
  (400, 0),
];

/// The field's offset in pt [ms] after a failure: each segment eased in and out.
double lockShakeOffset(double ms) {
  final k = lockShakeKeyframes;
  if (ms <= k.first.$1 || ms >= k.last.$1) return 0;
  for (var i = 1; i < k.length; i++) {
    if (ms <= k[i].$1) {
      final (t0, v0) = k[i - 1];
      final (t1, v1) = k[i];
      final u = Curves.easeInOutSine.transform((ms - t0) / (t1 - t0));
      return v0 + (v1 - v0) * u;
    }
  }
  return 0;
}

/// The banner's entrance: an exponential slide from the right edge (tau 71 ms, ~400 ms) and a
/// fade of ~210 ms (docs/spec/notifications.md).
const Duration bannerSlide = Duration(milliseconds: 400);
const Duration bannerFade = Duration(milliseconds: 210);

/// The share of the slide left [ms] after the start.
double bannerSlideLeft(double ms) =>
    ms >= bannerSlide.inMilliseconds ? 0 : math.exp(-ms / 71);
