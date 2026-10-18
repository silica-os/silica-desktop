// The Silica shell: one component per process, chosen by the first argument of main
// (silica-embedder --dart-arg dock). Today: dock (the real apps), dock-demo (placeholders),
// menubar, wallpaper, faro, notifications, controlcenter, switcher.
//
// `shell` draws every component in one engine (#201): the embedder gives each layer its own
// view (`--view`), named by its namespace, which is the component's name.

import 'dart:io';

import 'package:flutter/scheduler.dart' show timeDilation;
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart'
    show AmalfiGlassBackend, AmalfiGlassBackendScope, SystemLookScope;

import 'auth/auth_dialog.dart';
import 'controlcenter/controlcenter.dart';
import 'l10n/l10n.dart';
import 'dock/dock.dart';
import 'dock/model.dart';
import 'faro/faro.dart';
import 'lock/lock_screen.dart';
import 'menubar/menubar.dart';
import 'notifications/notifications.dart';
import 'notifications/service.dart';
import 'system/desktop_entries.dart';
import 'system/scroll_behavior.dart';
import 'system/shell_channel.dart';
import 'forcequit/force_quit.dart';
import 'screenshot/screenshot.dart';
import 'switcher/switcher.dart';
import 'system/layer_control.dart';
import 'system/shell_views.dart';
import 'system/system_status.dart';
import 'system/media.dart';
import 'wallpaper/wallpaper.dart';

/// Dock placeholders for tests and for comparison with the recordings: 12 apps, separator,
/// folder and trash.
const List<Item> placeholderItems = [
  Item.app('Files', running: true),
  Item.app('Browser', running: true),
  Item.app('Mail'),
  Item.app('Messages'),
  Item.app('Calendar'),
  Item.app('Notes'),
  Item.app('Photos'),
  Item.app('Music'),
  Item.app('Maps'),
  Item.app('Editor'),
  Item.app('Terminal', running: true),
  Item.app('Settings'),
  Item.separator(),
  Item.app('Downloads'),
  Item.app('Trash', trash: true),
];

Future<void> main(List<String> args) async {
  final component = args.isEmpty ? 'dock' : args.first;
  // Debug only: SILICA_TIME_DILATION=10 slows every animation of the shell tenfold, to look
  // at transitions frame by frame (docs/vm.md › Looking at animations).
  final dilation = double.tryParse(
    Platform.environment['SILICA_TIME_DILATION'] ?? '',
  );
  if (dilation != null && dilation > 0) timeDilation = dilation;
  if (component == 'shell') {
    WidgetsFlutterBinding.ensureInitialized();
    final namespaces = await LayerControl.views();
    runWidget(
      ShellViews(
        namespaces: namespaces,
        builder: (namespace) => ShellComponent(component: namespace),
      ),
    );
    return;
  }
  runApp(ShellComponent(component: component));
}

class ShellComponent extends StatelessWidget {
  const ShellComponent({super.key, required this.component});

  final String component;

  @override
  Widget build(BuildContext context) {
    final Widget child = switch (component) {
      'dock-demo' => const Dock(items: placeholderItems),
      // The components that name or list apps follow the installed ones (#198, #200).
      'menubar' => _followsEntries(
        () => MenuBar(
          entries: desktopEntries(),
          onSearch: () => ShellChannel.instance.execute('faro.toggle'),
          onClock: () => ShellChannel.instance.execute('notifications.toggle'),
          onControls: () =>
              ShellChannel.instance.execute('controlcenter.toggle'),
          system: DBusSystem(),
          media: MediaPlayers.instance,
        ),
      ),
      'wallpaper' => const WallpaperLayer(),
      'faro' => _followsEntries(() => Faro(entries: desktopEntries())),
      'notifications' => NotificationsLayer(
        source: DBusNotifications(),
        system: DBusSystem(),
      ),
      'controlcenter' => ControlCenter(system: DBusSystem()),
      // The screen saver (and what asks for the session lock) in an ordinary overlay layer; the
      // lock screen itself is the view of the lock surface the embedder adds while locked.
      'screensaver' => const ScreenSaverLayer(),
      'lock' => LockScreen(source: DBusLock(), showUser: readShowUser()),
      // The app switcher never takes input: the same layer hosts the screenshots.
      'switcher' => _followsEntries(
        () => Stack(
          children: [
            AppSwitcher(entries: desktopEntries()),
            ScreenshotLayer(entries: desktopEntries()),
            ForceQuitLayer(entries: desktopEntries()),
            // The polkit agent's password dialog, above everything else here.
            AuthLayer(source: DBusAuth(), entries: desktopEntries()),
          ],
        ),
      ),
      _ => const LiveDock(),
    };
    // Every layer is a surface of its own with nothing behind its glass in Flutter: Ossido
    // draws the Amalfi glass (menus, popovers, alerts, tooltips). Faro keeps its own backend
    // choice (its subtree is not wrapped here).
    final Widget glass = component == 'faro'
        ? child
        : AmalfiGlassBackendScope(
            backend: AmalfiGlassBackend.compositor,
            child: child,
          );
    return ShellLocalizations(
      child: ScrollConfiguration(
        behavior: const ShellScrollBehavior(),
        // The Appearance choices of Settings (accent, highlight, scroll bars, icon style) follow
        // live, as in every app.
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: SystemLookScope(child: glass),
        ),
      ),
    );
  }

  /// Builds the component again whenever the installed apps change.
  static Widget _followsEntries(Widget Function() build) => ListenableBuilder(
    listenable: desktopEntryStore,
    builder: (context, _) => build(),
  );
}
