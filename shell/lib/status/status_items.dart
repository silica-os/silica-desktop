// The data side of the shared Status Bar (silica_ui `AmalfiStatusBar`,
// docs/design/amalfi/status-bar.md): what the network, battery and input source items show,
// from the system's state, for the menu bar and the lock screen alike. Both build their items
// here, so that an item has the same look, size and value in both contexts.

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:intl/intl.dart' show NumberFormat;
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../menubar/status_icons.dart';
import '../system/shell_channel.dart';
import '../system/system_status.dart';

/// The battery's percentage as the locale writes it (`84%`, `84 %`), whole numbers.
String batteryPercentage(double percent, String locale) {
  final value = (percent.isFinite ? percent : 0).clamp(0, 100).round() / 100;
  try {
    return NumberFormat.percentPattern(locale).format(value);
  } on ArgumentError {
    return NumberFormat.percentPattern('en').format(value);
  }
}

/// The network item: the wired glyph with a cable and no Wi-Fi, else Wi-Fi by its strength
/// (`wifi-slash` when off or not connected); null when there is neither.
Widget? networkStatusItem(
  AppLocalizations l,
  SystemSnapshot s, {
  Key? boxKey,
  bool active = false,
  VoidCallback? onPressed,
}) {
  if (s.wired && s.wifiSsid.isEmpty) {
    return AmalfiStatusItem(
      key: const ValueKey('status wired'),
      boxKey: boxKey,
      label: l.statusEthernet,
      width: AmalfiStatusBarMetrics.glyphItemWidth,
      child: const Center(child: StatusIcon(kind: StatusKind.wired)),
    );
  }
  if (!s.wifiAvailable) return null;
  final off = !s.wifiEnabled || s.wifiSsid.isEmpty;
  return AmalfiStatusItem(
    key: const ValueKey('status wifi'),
    boxKey: boxKey,
    label: l.statusWifi,
    value: !s.wifiEnabled
        ? l.statusWifiOff
        : s.wifiSsid.isEmpty
        ? l.statusWifiNotConnected
        : l.statusWifiConnected(s.wifiSsid),
    active: active,
    onPressed: onPressed,
    width: AmalfiStatusBarMetrics.glyphItemWidth,
    child: Center(
      child: StatusIcon(
        kind: StatusKind.network,
        level: s.wifiStrength / 100,
        dim: off,
      ),
    ),
  );
}

/// The battery item: the percentage (when [percentage]) right-aligned before the glyph; null
/// without a battery.
Widget? batteryStatusItem(
  AppLocalizations l,
  SystemSnapshot s, {
  required bool percentage,
  Key? boxKey,
  bool active = false,
  VoidCallback? onPressed,
}) {
  if (!s.batteryPresent) return null;
  final text = batteryPercentage(s.batteryPercent, l.localeName);
  final glyph = StatusIcon(
    kind: StatusKind.battery,
    level: s.batteryPercent / 100,
    charging: s.batteryCharging,
  );
  return AmalfiStatusItem(
    key: const ValueKey('status battery'),
    boxKey: boxKey,
    label: l.statusBattery,
    value: s.batteryCharging
        ? l.statusBatteryCharging(s.batteryPercent.round())
        : l.statusBatteryLevel(s.batteryPercent.round()),
    active: active,
    onPressed: onPressed,
    width: percentage ? null : AmalfiStatusBarMetrics.glyphItemWidth,
    child: percentage
        ? Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              AmalfiStatusLabel(
                text,
                alignment: Alignment.centerRight,
                tabular: true,
              ),
              const SizedBox(width: AmalfiStatusBarMetrics.labelGap),
              glyph,
            ],
          )
        : Center(child: glyph),
  );
}

/// The input source item: the active source's name and the keyboard glyph; null with fewer
/// than two sources (there is nothing to choose).
Widget? inputSourceStatusItem(
  AppLocalizations l,
  InputSourceState sources, {
  Key? boxKey,
  bool active = false,
  VoidCallback? onPressed,
}) {
  if (sources.names.length < 2) return null;
  return AmalfiStatusInputSourceItem(
    key: const ValueKey('status input source'),
    boxKey: boxKey,
    source: inputSourceName(sources.active, l.localeName),
    label: l.statusInputSource,
    active: active,
    onPressed: onPressed,
  );
}

/// The input sources Ossido announces (`keyboard.sources`, `keyboard.source`).
@immutable
class InputSourceState {
  const InputSourceState({this.names = const [], this.index = 0});

  /// From the shell states: the xkb names, comma-separated, and the active index.
  factory InputSourceState.of(Map<String, String> states) {
    final names = [
      for (final n in (states['keyboard.sources'] ?? '').split(','))
        if (n.trim().isNotEmpty) n.trim(),
    ];
    final index = int.tryParse(states['keyboard.source'] ?? '') ?? 0;
    return InputSourceState(
      names: names,
      index: names.isEmpty ? 0 : index.clamp(0, names.length - 1),
    );
  }

  /// The sources' xkb names (`us`, `it`, `de(nodeadkeys)`), in order; empty with fewer than
  /// two.
  final List<String> names;

  /// The active one's index.
  final int index;

  /// The active source's xkb name; empty without sources.
  String get active => names.isEmpty ? '' : names[index];

  @override
  bool operator ==(Object other) =>
      other is InputSourceState &&
      other.index == index &&
      listEquals(other.names, names);

  @override
  int get hashCode => Object.hash(index, Object.hashAll(names));
}

/// The input sources, live from the shell states; [select] makes one the active one.
class InputSources extends ValueNotifier<InputSourceState> {
  InputSources({ShellChannel? channel})
    : _channel = channel ?? ShellChannel.instance,
      super(const InputSourceState()) {
    _channel.states.addListener(_changed);
    _changed();
  }

  final ShellChannel _channel;

  void _changed() => value = InputSourceState.of(_channel.states.value);

  /// Asks Ossido to make the source at [index] the active one (`keyboard.select_source`).
  Future<bool> select(int index) =>
      _channel.execute('keyboard.select_source', {'index': index});

  @override
  void dispose() {
    _channel.states.removeListener(_changed);
    super.dispose();
  }
}

/// The name of an xkb input source in the interface language: `it` is "Italian" ("Italiano"),
/// `us(intl)` "U.S. (intl)"; a layout without a name here shows its code in capitals.
String inputSourceName(String xkb, String locale) {
  final open = xkb.indexOf('(');
  final layout = open < 0 ? xkb : xkb.substring(0, open);
  final variant = open < 0
      ? ''
      : xkb.substring(open + 1, xkb.endsWith(')') ? xkb.length - 1 : null);
  final names = _layoutNames[layout];
  final italian = locale.toLowerCase().startsWith('it');
  final name = names == null
      ? layout.toUpperCase()
      : italian
      ? names.$2
      : names.$1;
  return variant.isEmpty ? name : '$name ($variant)';
}

/// The common xkb layouts: English and Italian names.
const Map<String, (String, String)> _layoutNames = {
  'us': ('U.S.', 'Statunitense'),
  'gb': ('British', 'Britannica'),
  'it': ('Italian', 'Italiano'),
  'de': ('German', 'Tedesco'),
  'fr': ('French', 'Francese'),
  'es': ('Spanish', 'Spagnolo'),
  'pt': ('Portuguese', 'Portoghese'),
  'br': ('Brazilian', 'Brasiliano'),
  'ch': ('Swiss', 'Svizzero'),
  'at': ('Austrian', 'Austriaco'),
  'be': ('Belgian', 'Belga'),
  'nl': ('Dutch', 'Olandese'),
  'se': ('Swedish', 'Svedese'),
  'no': ('Norwegian', 'Norvegese'),
  'dk': ('Danish', 'Danese'),
  'fi': ('Finnish', 'Finlandese'),
  'is': ('Icelandic', 'Islandese'),
  'ie': ('Irish', 'Irlandese'),
  'pl': ('Polish', 'Polacco'),
  'cz': ('Czech', 'Ceco'),
  'sk': ('Slovak', 'Slovacco'),
  'si': ('Slovenian', 'Sloveno'),
  'hr': ('Croatian', 'Croato'),
  'hu': ('Hungarian', 'Ungherese'),
  'ro': ('Romanian', 'Rumeno'),
  'bg': ('Bulgarian', 'Bulgaro'),
  'gr': ('Greek', 'Greco'),
  'ru': ('Russian', 'Russo'),
  'ua': ('Ukrainian', 'Ucraino'),
  'tr': ('Turkish', 'Turco'),
  'il': ('Hebrew', 'Ebraico'),
  'ara': ('Arabic', 'Arabo'),
  'ir': ('Persian', 'Persiano'),
  'in': ('Indian', 'Indiano'),
  'jp': ('Japanese', 'Giapponese'),
  'kr': ('Korean', 'Coreano'),
  'cn': ('Chinese', 'Cinese'),
  'tw': ('Taiwanese', 'Taiwanese'),
  'th': ('Thai', 'Thailandese'),
  'vn': ('Vietnamese', 'Vietnamita'),
  'ca': ('Canadian', 'Canadese'),
  'latam': ('Latin American', 'Latinoamericano'),
};

/// The input menu's rows: one per source, the active one checked; choosing one makes it the
/// active source.
List<MenuEntry> inputSourceMenu(InputSources sources, String locale) {
  final state = sources.value;
  return [
    for (final (i, name) in state.names.indexed)
      MenuAction(
        inputSourceName(name, locale),
        checked: i == state.index,
        onSelected: () => sources.select(i),
      ),
  ];
}
