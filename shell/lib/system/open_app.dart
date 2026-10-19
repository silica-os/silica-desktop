// Opening an app from Faro, the Launcher and the menu bar, as the Dock does: a running app
// is activated and its windows raised, a stopped one is launched.

import 'package:silica_ui/silica_ui.dart' show SilicaApps;

import 'desktop_entries.dart';
import 'shell_channel.dart';

/// The ids of the windows of `entry` to bring forward, in raising order (the last ends on
/// top). Visible windows keep their order with the focused one last; if all are minimized,
/// the most recently minimized one comes back. Empty: the app is not running.
List<String> windowsToRaise(
  DesktopEntry entry, {
  required List<Toplevel> toplevels,
  required List<String> minimized,
  required String focused,
}) {
  final own = [
    for (final t in toplevels)
      if (entry.matchesAppId(t.appId)) t.id,
  ];
  if (own.isEmpty) return const [];
  final visible = [
    for (final id in own)
      if (!minimized.contains(id)) id,
  ];
  if (visible.isEmpty) return [minimized.lastWhere(own.contains)];
  return [
    for (final id in visible)
      if (id != focused) id,
    if (visible.contains(focused)) focused,
  ];
}

/// Opens `entry`: activates its windows if it is running, otherwise launches it.
void openApp(DesktopEntry entry, [ShellChannel? channel]) {
  final shell = channel ?? ShellChannel.instance;
  final windows = windowsToRaise(
    entry,
    toplevels: shell.toplevels.value,
    minimized: shell.minimized.value,
    focused: shell.focused.value,
  );
  if (windows.isEmpty) {
    shell.launch(entry.exec);
    // Launched once, it is no longer new (the Launcher's dot).
    if (entry.isNew) SilicaApps.noteLaunched(entry.id);
    return;
  }
  for (final id in windows) {
    shell.activate(id);
  }
}
