// The Settings service in memory for the shell's tests.

import 'package:flutter_test/flutter_test.dart';
import 'package:silica_ui/silica_ui.dart';

/// Makes [SilicaSettings.instance] a store in memory with [values] and returns it.
Future<MemorySettingsBackend> useSettings([
  Map<String, Object?> values = const {},
]) async {
  final backend = MemorySettingsBackend(values);
  final settings = SilicaSettings.useBackend(backend);
  addTearDown(() {
    settings.dispose();
    // The next test starts from the defaults, not from this one's values.
    SilicaSettings.useBackend(MemorySettingsBackend());
  });
  await settings.refresh();
  return backend;
}
