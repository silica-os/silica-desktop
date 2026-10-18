// The geometry, the states and the timings of the lock screen: the Amalfi hand-off
// (docs/design/amalfi/lock-screen.md) over the measurements of docs/spec/lock.md (L-01). Pure
// functions of the screen size, so that the tests can check them without drawing.

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

/// The state of the password field (kit `Lock Screen/Password Field/<State>`).
enum LockFieldState {
  /// Empty and focused: the caret and the placeholder.
  placeholder,

  /// Characters typed: one dot each, and the submit button.
  typing,

  /// The password is being checked: the dots and a spinner in place of the arrow.
  verifying,

  /// The last try failed: the dots stay, selected, and the field shook.
  wrong,
}

/// The stages of a wake from display-off (spec "Wake"): every change is a cut.
enum LockWakeStage {
  /// 0 ms: the wallpaper and the status items only.
  wallpaper,

  /// +150 ms: date, clock, avatar and name, the user block 36 pt higher, no field.
  clock,

  /// +20 ms more: the field and the help button, the user block in place.
  full,
}

/// Positions in pt for a screen of a given size: the top block from the top edge, the bottom
/// block from the bottom edge, all centred on the vertical axis except the help button and the
/// status items (the doc's "Screen layout" table).
abstract final class LockMetrics {
  static const double margin = 16;

  static Rect statusItems(Size s) =>
      Rect.fromLTWH(s.width - margin - 164, 0, 164, 32);

  static Rect clock(Size s) =>
      Rect.fromLTWH((s.width - 960) / 2, 128, 960, 264);

  /// The date's box inside the clock block, top at the block's top (baseline y ~ 167).
  static const double dateHeight = 48;

  /// The digits' top (y ~ 199) and height (cap height of Inter 200).
  static const double digitsTop = 199;
  static const double digitHeight = 145;

  /// The user block; `raised` is the 36 pt higher place of the wake's second stage.
  static Rect userBlock(Size s, {bool raised = false}) => Rect.fromLTWH(
    (s.width - 320) / 2,
    s.height - 258 - (raised ? wakeRise : 0),
    320,
    108,
  );

  static const double wakeRise = 36;

  /// The avatar: 72, at the top of the user block.
  static Rect avatar(Size s, {bool raised = false}) {
    final b = userBlock(s, raised: raised);
    return Rect.fromLTWH(b.left + (320 - 72) / 2, b.top, 72, 72);
  }

  /// The name: 22 tall, 12 below the avatar, the block's width.
  static Rect name(Size s, {bool raised = false}) {
    final b = userBlock(s, raised: raised);
    return Rect.fromLTWH(b.left, b.top + 84, 320, 22);
  }

  /// The password field: 160 x 28, its bottom 108 above the screen edge. Never resized.
  static Rect field(Size s) =>
      Rect.fromLTWH((s.width - 160) / 2, s.height - 136, 160, 28);

  /// The help button: 28, 8 right of the field, same centre; not part of the centring.
  static Rect help(Size s) {
    final f = field(s);
    return Rect.fromLTWH(f.right + 8, f.top, 28, 28);
  }

  /// The hint block of the hint state: 256 wide, its bottom 28 above the field.
  static Rect hint(Size s) =>
      Rect.fromLTWH((s.width - 256) / 2, s.height - 248, 256, 84);

  /// The help address of the hint state: 256 x 16, 12 below the field.
  static Rect helpAddress(Size s) =>
      Rect.fromLTWH((s.width - 256) / 2, s.height - 96, 256, 16);

  /// The banner `index` (0 first): 360 wide, 16 from the right, the first at y 48, 8 apart.
  static Rect banner(Size s, int index) =>
      Rect.fromLTWH(s.width - margin - 360, 48 + index * 68.0, 360, 60);

  /// The optional power row: 288 x 72, 20 below the field.
  static Rect powerRow(Size s) =>
      Rect.fromLTWH((s.width - 288) / 2, s.height - 88, 288, 72);
}

/// Inside the password field (field coordinates, pt).
abstract final class LockFieldMetrics {
  static const double caretX = 12;
  static const double caretWidth = 2;
  static const double caretHeight = 16;
  static const double caretY = 6;
  static const double placeholderX = 18;
  static const double dot = 8;
  static const double dotPitch = 12;
  static const double firstDotX = 16;
  static const double submitRing = 24;
  static const double submitX = 134;
  static const double submitY = 2;
  static const double spinnerX = 138;
  static const double spinnerY = 6;

  /// How many dots fit before the submit button; more keep the last ones in view.
  static const int visibleDots = 9;

  /// The left edge of dot [i].
  static double dotLeft(int i) => firstDotX + dotPitch * i;

  /// The selection of the dots after a failure: x 12 to the end of the [count] dots, 16 tall.
  static Rect selection(int count) {
    final n = math.min(count, visibleDots);
    return Rect.fromLTRB(
      12,
      caretY,
      n == 0 ? 12 : dotLeft(n - 1) + dot,
      caretY + caretHeight,
    );
  }

  /// The caret after [count] dots: 4 after the last one (at 12 without dots).
  static Rect caret(int count) {
    final n = math.min(count, visibleDots);
    final x = n == 0 ? caretX : dotLeft(n - 1) + dot + 4;
    return Rect.fromLTWH(x, caretY, caretWidth, caretHeight);
  }
}

// ---- Timings (spec "Animation timings") ----------------------------------------------------

/// Lock: the clock, date and user block fade in over ~250 ms, almost linear with a soft end
/// (progress ~0.6 at half time).
const Duration lockFade = Duration(milliseconds: 250);
const Curve lockFadeCurve = Cubic(0.4, 0.4, 0.7, 1);

/// The user block starts slightly after the clock (offset unmeasured: an assumption).
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

/// The minute roll: ~900 ms, decelerating; the ones digit starts 60 ms after the tens
/// (assumption). Travel = the digit height, mask 24 pt (assumptions).
const Duration minuteRoll = Duration(milliseconds: 900);
const Duration minuteRollOnesDelay = Duration(milliseconds: 60);
const Curve minuteRollCurve = Cubic(0.2, 0, 0, 1);
const double minuteRollMask = 24;

/// Reduce motion: the roll and the shake's replacement are `fade.quick` (150 ms).
const Duration fadeQuick = Duration(milliseconds: 150);

/// The wrong-password shake: horizontal only, ~400 ms, first to the left (pt, ms).
const Duration lockShake = Duration(milliseconds: 400);
const List<(double, double)> lockShakeKeyframes = [
  (0, 0),
  (31, -10),
  (94, 14),
  (156, -11),
  (219, 10),
  (281, -7),
  (344, 6),
  (400, 0),
];

/// The field's offset in pt [ms] after a failure: linear keyframes would show corners, so
/// each segment is eased in and out (sine-like).
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
