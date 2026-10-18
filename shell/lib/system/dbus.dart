// The embedder's silica/dbus channel: D-Bus calls and signals with JSON values
// (crates/embedder/src/dbus). The client is silica_ui's SilicaDBus, shared with the library and
// the other widgets of the process: a channel has one signal handler, so every listener goes
// through it.

import 'package:silica_ui/silica_ui.dart'
    show SilicaDBus, SilicaDBusBus, SilicaDBusSignal;

typedef DBus = SilicaDBus;
typedef DBusBus = SilicaDBusBus;
typedef DBusSignal = SilicaDBusSignal;
