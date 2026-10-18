// The Control Center's modules: which ones, in which order and at which size, packed into
// the measured grid (docs/spec/controlcenter.md, CC-00, SH-CC-01, INV-A2).
//
// The grid is four 64 pt cells across (292 pt: 4 × 64 + 3 × 12), with a 76 pt pitch in
// both directions. A small module is one cell (a 64 pt round tile), a medium one two cells
// across (a 140 × 64 capsule), a large one two by two (140 × 140), a wide one the whole row
// (292 × 64, Display and Sound). Modules are placed in order, each at the first free spot
// reading row by row, which reproduces the reference's default layout.
//
// The user's arrangement is the `controlcenter.modules` key of the Settings service, a list of
// `id:size` (`wifi:medium`). Edit mode (Edit Controls) and the module menu change it.

import 'dart:async';
import 'dart:ui';

import 'package:flutter/foundation.dart';
import 'package:silica_ui/silica_ui.dart';

import '../system/system_status.dart';

/// Cell side and pitch of the grid.
const double cell = 64;
const double pitch = 76;
const int gridColumns = 4;

enum ModuleSize {
  small(1, 1),
  medium(2, 1),
  large(2, 2),
  wide(4, 1);

  const ModuleSize(this.columns, this.rows);
  final int columns;
  final int rows;

  Size get extent =>
      Size(columns * cell + (columns - 1) * 12, rows * cell + (rows - 1) * 12);
}

@immutable
class CcModule {
  const CcModule(this.id, this.size);

  final String id;
  final ModuleSize size;

  CcModule withSize(ModuleSize size) => CcModule(id, size);

  @override
  bool operator ==(Object other) =>
      other is CcModule && other.id == id && other.size == size;

  @override
  int get hashCode => Object.hash(id, size);

  @override
  String toString() => '$id:${size.name}';
}

/// The modules Silica offers and the sizes each one can take (the module menu's Size group).
/// The order is the gallery's.
const Map<String, List<ModuleSize>> moduleSizes = {
  'wifi': [ModuleSize.medium],
  'bluetooth': [ModuleSize.medium],
  'nearby': [ModuleSize.medium],
  // The compact 140 × 64 Now playing is not measured: large only for now.
  'media': [ModuleSize.large],
  'dnd': [ModuleSize.medium],
  'dark': [ModuleSize.small, ModuleSize.medium],
  'capture': [ModuleSize.small, ModuleSize.medium],
  'staging': [ModuleSize.small, ModuleSize.medium],
  'mirroring': [ModuleSize.small, ModuleSize.medium],
  'battery': [ModuleSize.small, ModuleSize.medium],
  'display': [ModuleSize.wide],
  'keyboard': [ModuleSize.wide],
  'sound': [ModuleSize.wide],
};

/// The reference's default arrangement (INV-A2 grid): Wi-Fi and Bluetooth on the left with
/// Now playing beside them, nearby sharing with the window staging and mirroring tiles, Dark
/// Mode and Screen Capture with Focus, then Display and Sound.
const List<CcModule> defaultModules = [
  CcModule('wifi', ModuleSize.medium),
  CcModule('media', ModuleSize.large),
  CcModule('bluetooth', ModuleSize.medium),
  CcModule('nearby', ModuleSize.medium),
  CcModule('staging', ModuleSize.small),
  CcModule('mirroring', ModuleSize.small),
  CcModule('dark', ModuleSize.small),
  CcModule('capture', ModuleSize.small),
  CcModule('dnd', ModuleSize.medium),
  CcModule('display', ModuleSize.wide),
  CcModule('sound', ModuleSize.wide),
];

/// Places [modules] in order on the four-cell grid: each at the first free spot, row by row.
/// Returns each module's frame relative to the stack's top-left corner.
Map<String, Rect> packModules(List<CcModule> modules) {
  final taken = <(int, int)>{};
  final frames = <String, Rect>{};
  bool fits(int col, int row, ModuleSize size) {
    if (col + size.columns > gridColumns) return false;
    for (var dc = 0; dc < size.columns; dc++) {
      for (var dr = 0; dr < size.rows; dr++) {
        if (taken.contains((col + dc, row + dr))) return false;
      }
    }
    return true;
  }

  for (final m in modules) {
    if (frames.containsKey(m.id)) continue;
    for (var row = 0; ; row++) {
      final col = [
        for (var c = 0; c < gridColumns; c++)
          if (fits(c, row, m.size)) c,
      ].firstOrNull;
      if (col == null) continue;
      for (var dc = 0; dc < m.size.columns; dc++) {
        for (var dr = 0; dr < m.size.rows; dr++) {
          taken.add((col + dc, row + dr));
        }
      }
      frames[m.id] = Offset(col * pitch, row * pitch) & m.size.extent;
      break;
    }
  }
  return frames;
}

/// Whether module [id] can show on this machine: the battery only with a battery, the
/// keyboard backlight only with a backlit keyboard.
bool moduleAvailable(String id, SystemSnapshot s) => switch (id) {
  'battery' => s.batteryPresent,
  'keyboard' => s.keyboardAvailable,
  _ => true,
};

/// The modules that can be shown on this machine.
List<CcModule> presentModules(List<CcModule> modules, SystemSnapshot s) => [
  for (final m in modules)
    if (moduleAvailable(m.id, s)) m,
];

/// The settings key of the arrangement: a list of `id:size`. Empty is the default
/// arrangement; `none` stands for an arrangement without modules.
const String modulesKey = 'controlcenter.modules';

/// The user's arrangement, or the default one. Unknown ids and sizes a module can't take
/// are dropped.
List<CcModule> loadModules([SilicaSettings? settings]) {
  final list = (settings ?? SilicaSettings.instance).getTexts(modulesKey);
  if (list.isEmpty) return defaultModules;
  final modules = <CcModule>[];
  for (final item in list) {
    final colon = item.indexOf(':');
    final id = colon < 0 ? item : item.substring(0, colon);
    final sizes = moduleSizes[id];
    if (sizes == null) continue;
    final size = ModuleSize.values
        .where((s) => colon >= 0 && s.name == item.substring(colon + 1))
        .firstOrNull;
    modules.add(
      CcModule(id, size != null && sizes.contains(size) ? size : sizes.first),
    );
  }
  return modules;
}

/// Saves the arrangement.
void saveModules(List<CcModule> modules, [SilicaSettings? settings]) {
  unawaited(
    (settings ?? SilicaSettings.instance).set(modulesKey, [
      for (final m in modules) '${m.id}:${m.size.name}',
      if (modules.isEmpty) 'none',
    ]),
  );
}
