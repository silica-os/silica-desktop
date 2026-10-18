// Interface sound effects, played by silica-daemon (`dev.frascella.Silica.Sound1`, docs/daemon.md):
// the daemon finds the sound, plays it through PipeWire and honours "Play user interface sound
// effects". Without the daemon (outside Silica, in tests) nothing plays.

import 'package:flutter/services.dart';

import 'dbus.dart';

/// The sounds the shell plays.
abstract final class UiSound {
  /// Items moved to the Trash, by a drop on the Dock's Trash too (one Trash sound; the old
  /// `trash-drag` name stays in the daemon only as an alias).
  static const String trashMove = 'trash-move';

  /// The Trash emptied.
  static const String trashEmpty = 'trash-empty';

  /// A copy or move finished (a drop on a Dock folder).
  static const String fileTransfer = 'file-transfer';

  /// A screenshot taken.
  static const String screenCapture = 'screen-capture';
}

/// Plays the alert sound chosen in Settings › Sound at the alert volume (`PlayAlert`), as a
/// notification arrives; never fails.
Future<void> playAlertSound() async {
  try {
    await DBus.instance.call(
      destination: 'dev.frascella.Silica',
      path: '/dev/frascella/Silica/Sound1',
      interface: 'dev.frascella.Silica.Sound1',
      method: 'PlayAlert',
      signature: 's',
      args: [''],
    );
  } on PlatformException {
    // No daemon, or no such sound: silence.
  } on MissingPluginException {
    // Outside Silica.
  }
}

/// Plays interface sound `name`; never fails.
Future<void> playUiSound(String name) async {
  try {
    await DBus.instance.call(
      destination: 'dev.frascella.Silica',
      path: '/dev/frascella/Silica/Sound1',
      interface: 'dev.frascella.Silica.Sound1',
      method: 'PlaySound',
      signature: 's',
      args: [name],
    );
  } on PlatformException {
    // No daemon, or no such sound: silence.
  } on MissingPluginException {
    // Outside Silica.
  }
}
