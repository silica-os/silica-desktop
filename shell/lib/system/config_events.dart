// File watching for the JSON configuration files (interim, until the Settings service).

import 'dart:io';

/// Whether [event] concerns the file called [name]. Writers replace a file atomically
/// (temporary file, then a rename over it): the event is then a move whose `path` is the
/// temporary file and whose `destination` is the real one, so both are looked at.
bool eventTouchesFile(FileSystemEvent event, String name) {
  if (event.path.endsWith(name)) return true;
  if (event is FileSystemMoveEvent) {
    return event.destination?.endsWith(name) ?? false;
  }
  return false;
}
