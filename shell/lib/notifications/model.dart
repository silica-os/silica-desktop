// Notifications as silica-daemon sends them (dev.frascella.Silica.Notifications1,
// docs/daemon.md), the Notification Centre groups and the Amalfi motion of banners and panel.

import 'dart:math' as math;

import 'package:flutter/foundation.dart';

/// The action key of the reply field.
const String replyAction = 'inline-reply';

/// A notification. The fields follow the daemon's `(ussssasysssbbti)` structure.
@immutable
class ShellNotification {
  const ShellNotification({
    required this.id,
    required this.appName,
    this.appIcon = '',
    required this.summary,
    this.body = '',
    this.actions = const [],
    this.urgency = 1,
    this.desktopEntry = '',
    this.imagePath = '',
    this.transient = false,
    this.resident = false,
    required this.timestamp,
  });

  final int id;
  final String appName;
  final String appIcon;
  final String summary;
  final String body;

  /// (key, label) pairs, as in the freedesktop specification.
  final List<(String, String)> actions;

  /// 0 low, 1 normal, 2 critical.
  final int urgency;
  final String desktopEntry;
  final String imagePath;

  /// Banner only: it does not stay in the Center.
  final bool transient;
  final bool resident;
  final DateTime timestamp;

  bool get critical => urgency >= 2;

  /// The same notification without its text (Settings › Notifications › previews off).
  ShellNotification withoutBody() => ShellNotification(
    id: id,
    appName: appName,
    appIcon: appIcon,
    summary: summary,
    actions: actions,
    urgency: urgency,
    desktopEntry: desktopEntry,
    imagePath: imagePath,
    transient: transient,
    resident: resident,
    timestamp: timestamp,
  );

  /// The actions to show as buttons: `default` is the click on the notification, and
  /// `inline-reply` is the reply field.
  List<(String, String)> get buttons => [
    for (final action in actions)
      if (action.$1 != 'default' && action.$1 != replyAction) action,
  ];

  /// The app takes a reply typed in the notification (KDE's inline-reply extension).
  bool get canReply => actions.any((a) => a.$1 == replyAction);

  /// The label of the reply action: the field's placeholder.
  String get replyLabel =>
      actions.firstWhere((a) => a.$1 == replyAction, orElse: () => ('', '')).$2;

  bool get hasDefault => actions.any((a) => a.$1 == 'default');

  /// The group key in the Center: the same app.
  String get group => desktopEntry.isNotEmpty ? desktopEntry : appName;

  /// From the D-Bus structure in JSON (a list of 14 fields); null if it does not fit.
  static ShellNotification? fromDBus(Object? json) {
    if (json is! List || json.length < 14) return null;
    final id = json[0];
    if (id is! int) return null;
    String text(int i) => json[i] is String ? json[i] as String : '';
    final flat = json[5] is List
        ? (json[5] as List).whereType<String>()
        : const <String>[];
    final pairs = flat.toList();
    final timestamp = json[12] is int ? json[12] as int : 0;
    return ShellNotification(
      id: id,
      appName: text(1),
      appIcon: text(2),
      summary: text(3),
      body: text(4),
      actions: [
        for (var i = 0; i + 1 < pairs.length; i += 2) (pairs[i], pairs[i + 1]),
      ],
      urgency: json[6] is int ? json[6] as int : 1,
      desktopEntry: text(8),
      imagePath: text(9),
      transient: json[10] == true,
      resident: json[11] == true,
      timestamp: DateTime.fromMillisecondsSinceEpoch(timestamp),
    );
  }
}

/// The notifications of one app in the Center, from the most recent.
@immutable
class NotificationGroup {
  const NotificationGroup(this.key, this.items);

  final String key;
  final List<ShellNotification> items;

  ShellNotification get latest => items.first;
}

/// The Center groups: non-transient notifications, by app, groups ordered by the most
/// recent notification.
List<NotificationGroup> groupNotifications(
  Iterable<ShellNotification> all, {
  bool Function(ShellNotification)? ungrouped,
}) {
  final sorted =
      [
        for (final n in all)
          if (!n.transient) n,
      ]..sort(
        (a, b) => b.timestamp.compareTo(a.timestamp) != 0
            ? b.timestamp.compareTo(a.timestamp)
            : b.id.compareTo(a.id),
      );
  final groups = <String, List<ShellNotification>>{};
  for (final n in sorted) {
    // An app that does not group (Settings › Notifications): one group per notification.
    final key = ungrouped?.call(n) ?? false ? '${n.group}#${n.id}' : n.group;
    groups.putIfAbsent(key, () => []).add(n);
  }
  return [for (final e in groups.entries) NotificationGroup(e.key, e.value)];
}

/// Distances the measurements were taken at (banner 344 + 16 margin, Center 387). Amalfi changes only the
/// geometry: distances and offsets scale by Amalfi / reference, curves and times do not.
const double _refBannerDistance = 344 + 16;
const double _refCenterDistance = 387;

/// Banner entrance (notifications.md, "Banner life cycle"): it slides in from the right
/// from off screen; the remaining distance shrinks by ~0.83 per frame (75 Hz), time
/// constant ~71 ms, arrival within 1 pt in ~400 ms, no overshoot.
const double bannerEnterTau = 71;

/// Starting distance: the whole banner plus the margin, off screen (Amalfi banner 360 pt).
const double bannerEnterDistance = 360 + 16;

/// Entrance duration: until half a point from arrival.
final Duration bannerEnterDuration = Duration(
  milliseconds: (bannerEnterTau * math.log(bannerEnterDistance / 0.5)).ceil(),
);

/// Rightward offset of the banner `ms` after the start of the entrance.
double bannerEnterOffset(double ms) {
  if (ms <= 0) return bannerEnterDistance;
  final offset = bannerEnterDistance * math.exp(-ms / bannerEnterTau);
  return offset < 0.5 ? 0 : offset;
}

/// While entering the banner fades in (~210 ms) and is blurred, sharp in ~215 ms.
const double bannerFadeInMs = 210;
const double bannerFocusMs = 215;

/// Maximum blur (sigma in pt) of the banner entering or leaving: the outline of the content
/// almost disappears in the first frame (maximum gradient 4 over 118).
const double bannerBlurSigma = 8;

/// Opacity and blur of the entrance `ms` after the start.
double bannerEnterOpacity(double ms) => (ms / bannerFadeInMs).clamp(0.0, 1.0);
double bannerEnterBlur(double ms) =>
    bannerBlurSigma * (1 - (ms / bannerFocusMs).clamp(0.0, 1.0));

/// From appearing to the start of the exit: ~5.0 s. Critical notifications stay until the
/// user closes them.
const Duration bannerDuration = Duration(seconds: 5);

/// With the pointer over it the banner does not expire; after the pointer exits, ~1.1 s.
const Duration bannerLinger = Duration(milliseconds: 1100);

/// Exit: to the right, ~180-190 ms, fading out halfway and with growing blur.
const Duration bannerExitDuration = Duration(milliseconds: 190);

/// Measured exit offsets at ~13.3 ms steps (left edge - 2200, pt) on the reference.
const List<double> bannerExitSteps = [
  0, 37, 66, 104, 141, 175, 207, 234, 264, 287, 305, 321, 335, 346, 356, //
];

/// Rightward offset `ms` after the start of the exit (interpolating the measurements, scaled
/// to the Amalfi banner distance).
double bannerExitOffset(double ms) {
  final step = ms / (1000 / 75);
  if (step <= 0) return 0;
  final i = step.floor();
  if (i >= bannerExitSteps.length - 1) return bannerEnterDistance;
  const k = bannerEnterDistance / _refBannerDistance;
  return k *
      (bannerExitSteps[i] +
          (bannerExitSteps[i + 1] - bannerExitSteps[i]) * (step - i));
}

double bannerExitOpacity(double ms) =>
    1 - 0.5 * (ms / bannerExitDuration.inMilliseconds).clamp(0.0, 1.0);
double bannerExitBlur(double ms) =>
    bannerBlurSigma * (ms / bannerExitDuration.inMilliseconds).clamp(0.0, 1.0);

/// Notification Centre opening: it slides in from the right, almost all of it in ~35 ms
/// and then a long tail up to ~235 ms (content edge 2545 -> 2198 -> 2158): the sum of two
/// exponentials, a fast one and a slow one. Amplitudes scale to the Amalfi distance.
const double centerDistance = 380 + 16;
const Duration centerOpenDuration = Duration(milliseconds: 235);

double centerOpenOffset(double ms) {
  if (ms <= 0) return centerDistance;
  const k = centerDistance / _refCenterDistance;
  final x = k * (347 * math.exp(-ms / 12) + 40 * math.exp(-ms / 70));
  return ms >= centerOpenDuration.inMilliseconds || x < 0.5 ? 0 : x;
}

/// Closing (~300 ms, the same with Esc, a click on the clock or a click outside): a slight
/// slow shift, then a fast exit and a tail.
const Duration centerCloseDuration = Duration(milliseconds: 300);
const List<(double, double)> centerCloseSteps = [
  (0, 0),
  (60, 10),
  (120, 37),
  (135, 104),
  (150, 357),
  (200, 380),
  (300, _refCenterDistance),
];

double centerCloseOffset(double ms) {
  if (ms <= 0) return 0;
  const k = centerDistance / _refCenterDistance;
  for (var i = 1; i < centerCloseSteps.length; i++) {
    final (t1, x1) = centerCloseSteps[i];
    if (ms <= t1) {
      final (t0, x0) = centerCloseSteps[i - 1];
      return k * (x0 + (x1 - x0) * (ms - t0) / (t1 - t0));
    }
  }
  return centerDistance;
}
