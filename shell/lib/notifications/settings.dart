// Per-app notification settings ($XDG_CONFIG_HOME/silica/notifications.json, written by the
// Settings app's Notifications pane):
//
//   {"previews": "always" | "unlocked" | "never", "grouping": "automatic" | "byApp" | "off",
//    "apps": {"<desktop entry or app name>": {"style": "banners" | "alerts" | "none",
//      "previews": "default" | "always" | "unlocked" | "never", "grouping": "auto" | "app" |
//      "off", "showInCenter": true}}}
//
// - banners (default): the banner leaves by itself after ~5 s;
// - alerts: the banner stays until the user acts on it (SH-NC-05; the reference style was not
//   measured: it behaves like a critical notification, with the × always visible,
//   provisional);
// - none: no banner, the notification goes straight to the Center.

import 'dart:convert';
import 'dart:io';

import 'model.dart';

enum NotificationStyle { banners, alerts, none }

/// Whether the text of a notification shows. Silica has no lock screen to hide it on, so
/// `unlocked` shows it like `always`.
enum PreviewMode { always, unlocked, never }

/// How the Centre stacks an app's notifications: `auto` and `app` fold them into one stack,
/// `off` lists each one.
enum GroupingMode { auto, app, off }

class NotificationSettings {
  const NotificationSettings([
    this.styles = const {},
    this.previews = PreviewMode.always,
    this.appPreviews = const {},
    this.groupings = const {},
    this.hiddenInCenter = const {},
    this.grouping = GroupingMode.auto,
    this.mutedApps = const {},
  ]);

  /// The apps that play no sound (`sound: false`).
  final Set<String> mutedApps;

  /// Whether a notification of this app plays the alert sound (the default).
  bool soundOf(ShellNotification n) =>
      !(mutedApps.contains(n.group) || mutedApps.contains(n.appName));

  /// Style by group key (desktop entry or app name).
  final Map<String, NotificationStyle> styles;

  /// The default for every app (`previews`), and the apps that chose their own.
  final PreviewMode previews;
  final Map<String, PreviewMode> appPreviews;

  /// The Centre's grouping by default (`grouping`: automatic, byApp, off) and per app.
  final GroupingMode grouping;
  final Map<String, GroupingMode> groupings;

  /// The apps whose notifications are not kept in the Centre (`showInCenter: false`).
  final Set<String> hiddenInCenter;

  T? _of<T>(Map<String, T> map, ShellNotification n) =>
      map[n.group] ?? map[n.appName];

  /// The notification as it is shown: without its text when previews are off for its app.
  ShellNotification present(ShellNotification n) {
    final mode = _of(appPreviews, n) ?? previews;
    return mode == PreviewMode.never && n.body.isNotEmpty ? n.withoutBody() : n;
  }

  /// Whether the Centre keeps the notification.
  bool inCenter(ShellNotification n) =>
      !(hiddenInCenter.contains(n.group) || hiddenInCenter.contains(n.appName));

  GroupingMode groupingOf(ShellNotification n) => _of(groupings, n) ?? grouping;

  NotificationStyle styleOf(ShellNotification n) =>
      styles[n.group] ?? styles[n.appName] ?? NotificationStyle.banners;

  static File file({Map<String, String>? env}) {
    final e = env ?? Platform.environment;
    final config = e['XDG_CONFIG_HOME'] ?? '${e['HOME'] ?? ''}/.config';
    return File('$config/silica/notifications.json');
  }

  /// The settings on disk; the defaults when the file is missing or unreadable.
  static NotificationSettings read({Map<String, String>? env}) {
    try {
      final f = file(env: env);
      if (!f.existsSync()) return const NotificationSettings();
      return parse(f.readAsStringSync());
    } on FileSystemException {
      return const NotificationSettings();
    }
  }

  static NotificationSettings parse(String text) {
    Object? json;
    try {
      json = jsonDecode(text);
    } on FormatException {
      return const NotificationSettings();
    }
    if (json is! Map) return const NotificationSettings();
    final apps = json['apps'] is Map ? json['apps'] as Map : const {};
    Map<String, T> perApp<T>(T? Function(Object?) read, String field) => {
      for (final MapEntry(:key, :value) in apps.entries)
        if (key is String && value is Map)
          if (read(value[field]) case final parsed?) key: parsed,
    };
    return NotificationSettings(
      perApp(_style, 'style'),
      _preview(json['previews']) ?? PreviewMode.always,
      perApp(_preview, 'previews'),
      perApp(_grouping, 'grouping'),
      {
        for (final MapEntry(:key, :value) in apps.entries)
          if (key is String && value is Map && value['showInCenter'] == false)
            key,
      },
      _grouping(json['grouping']) ?? GroupingMode.auto,
      {
        for (final MapEntry(:key, :value) in apps.entries)
          if (key is String && value is Map && value['sound'] == false) key,
      },
    );
  }

  static PreviewMode? _preview(Object? name) => switch (name) {
    'always' => PreviewMode.always,
    'unlocked' => PreviewMode.unlocked,
    'never' => PreviewMode.never,
    _ => null,
  };

  static GroupingMode? _grouping(Object? name) => switch (name) {
    'auto' || 'automatic' => GroupingMode.auto,
    'app' || 'byApp' => GroupingMode.app,
    'off' => GroupingMode.off,
    _ => null,
  };

  static NotificationStyle? _style(Object? name) => switch (name) {
    'banners' => NotificationStyle.banners,
    'alerts' => NotificationStyle.alerts,
    'none' => NotificationStyle.none,
    _ => null,
  };
}
