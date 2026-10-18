// Screenshots (screenshot-ui.md, docs/spec/screenshot-ui.md): the thumbnail after every
// capture, the ⇧⌘4 crosshair (with Space: a window) and the ⇧⌘5 bar with its Options menu.
//
// - Ossido captures (`screenshot.screen`, `screenshot.region`, `screenshot.window_at`; ⇧⌘3
//   and ⌃⇧⌘3 run there directly), writes a temporary PNG and announces it with the broadcast
//   "screenshot" (JSON: path, width, height, clipboard, seq). The shell plays the shutter
//   sound (silica-daemon), then shows the thumbnail or sends the capture to its place.
// - Where captures go, the timer, the thumbnail, the pointer, the bar's mode, selection and
//   position: `settings.dart` (the `screenshot.*` keys of the Settings service). ⇧⌘3 and ⇧⌘4 follow
//   the same choices.
// - The file is named "Screenshot <date> at <time>.png" (localized) from the capture time,
//   and lands in its place when the thumbnail leaves, or at once on a swipe to the right.
// - Dragging the thumbnail is a Wayland drag and drop of a file kept only for the drag (in the
//   cache): a folder, the desktop or an app that takes it gets the image, and nothing stays
//   in the capture's own place (SCREENSHOT-2, #87).
// - Clicking the thumbnail opens the image with the default app: there is no Markup editor
//   yet (#154), so the thumbnail menu has no Markup entry and the image viewer opens on a
//   click. Mail and Photos are offered only where their apps are installed.

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../dock/apps.dart' show openPathCommand;
import '../dock/dock.dart' show shellQuote;
import '../l10n/l10n.dart';
import '../menubar/menubar.dart' show barHeight;
import '../system/desktop_entries.dart';
import '../system/layer_arbiter.dart';
import '../system/layer_control.dart';
import '../system/shell_channel.dart';
import 'files.dart';
import 'menu.dart';
import 'selection.dart';
import 'services.dart';
import 'settings.dart';
import 'thumbnail.dart';
import 'toolbar.dart';

export 'files.dart' show screenshotFolder, screenshotName, documentsFolder;
export 'settings.dart';
export 'thumbnail.dart'
    show
        thumbnailImage,
        thumbnailBorder,
        thumbnailMargin,
        thumbnailRadius,
        thumbnailSettle,
        thumbnailHold,
        thumbnailLeave,
        thumbnailDragSide,
        thumbnailDragSize,
        thumbnailDragHotspot;
export 'toolbar.dart' show toolbarSize, toolbarAboveDock;

/// The apps the thumbnail menu and the Options menu offer when they are installed (the image
/// viewer is the default handler of PNG files, shown as the equivalent of Preview).
const String shotMailId = 'dev.frascella.Mail';
const String shotPhotosId = 'dev.frascella.Photos';

/// The countdown of the timer (PROVISIONAL: not measured): a dark rounded square with the
/// seconds left, at the center of what will be captured.
const Size countdownSize = Size(96, 96);

enum _Mode { none, selection, window, toolbar, countdown }

class ScreenshotLayer extends StatefulWidget {
  const ScreenshotLayer({
    super.key,
    this.folder,
    this.documents,
    this.cache,
    this.now,
    this.windowAt,
    this.services,
    this.settings,
    this.entries = const {},
    this.env,
  });

  /// The rectangle of the window under a point (frame without shadow), for the window mode
  /// veil; null or no window: the whole screen is veiled (the desktop). Without it Ossido
  /// draws the veil.
  final Rect? Function(Offset point)? windowAt;

  /// The Desktop and Documents folders (tests); null: the user's.
  final String? folder;
  final String? documents;

  /// Where captures for the clipboard or an app get a file (tests).
  final String? cache;

  /// The capture time (tests).
  final DateTime Function()? now;

  /// The system (tests replace it).
  final ScreenshotServices? services;

  /// The settings service (tests); null: the system's.
  final SilicaSettings? settings;

  /// The installed apps: the image viewer, the file manager.
  final Map<String, DesktopEntry> entries;

  /// The environment for the default image viewer (tests).
  final Map<String, String>? env;

  @override
  State<ScreenshotLayer> createState() => ScreenshotLayerState();
}

/// A capture announced by Ossido, until it reaches its place.
class _Shot {
  _Shot(this.temp, this.size, this.time, this.destination);

  final String temp;
  final Size size;
  final DateTime time;
  final ShotDestination destination;

  /// Where the file is now, once placed (a drag places it early).
  String? placed;

  /// It reached its place, was dropped somewhere or deleted: nothing more to do.
  bool done = false;

  /// Being dragged out: the thumbnail is hidden.
  bool dragging = false;

  /// The image of the drag, decoded when the thumbnail appears.
  ui.Image? dragImage;

  String get file => placed ?? temp;
}

class ScreenshotLayerState extends State<ScreenshotLayer>
    with TickerProviderStateMixin {
  /// The arbiter of this component's layer surface (#201).
  late final LayerArbiter _arbiter;

  late final ScreenshotServices _services =
      widget.services ?? SystemScreenshotServices(entries: widget.entries);
  late final SilicaSettings _store = widget.settings ?? SilicaSettings.instance;
  late ScreenshotSettings _settings = ScreenshotSettings.of(_store);

  StreamSubscription<String>? _commands;
  StreamSubscription<String>? _drags;
  final FocusNode _focus = FocusNode();
  _Mode _mode = _Mode.none;
  Offset? _pointer;

  /// ⇧⌘4: the selection being drawn, and whether this run goes to the clipboard.
  LiveSelection? _live;
  bool _toClipboard = false;

  /// ⇧⌘5: the mode, the selection being adjusted, the bar's center, the Options menu.
  ToolbarChoice _choice = ToolbarChoice.selection;
  Rect? _selection;
  Offset? _barCenter;

  /// The Options menu's items, made again only when a setting changes (a new list at every
  /// rebuild would reset an open menu).
  List<AmalfiMenuEntry>? _optionsEntries;

  /// The thumbnail menu's items, made when it opens.
  List<AmalfiMenuEntry> _menuEntries = const [];

  /// The timer: seconds left and its ticker.
  int _countdown = 0;
  Rect? _countdownTarget;
  Timer? _ticker;

  /// The thumbnail.
  _Shot? _shot;
  late final AnimationController _settle = AnimationController(
    vsync: this,
    duration: thumbnailSettle,
  )..addListener(() => setState(() {}));
  late final AnimationController _leave = AnimationController(
    vsync: this,
    duration: thumbnailLeave,
  )..addListener(() => setState(() {}));

  /// The time the thumbnail stays (from the start of the fade-in); stopped while its menu is
  /// open or the pointer is on it, and it goes on from where it stopped.
  late final AnimationController _dwell =
      AnimationController(
        vsync: this,
        duration: thumbnailSettle + thumbnailHold,
      )..addStatusListener((status) {
        if (status == AnimationStatus.completed) _dismiss();
      });
  bool _hovered = false;

  /// The thumbnail's menu, open at this point.
  Offset? _menuAt;
  int _lastSeq = -1;
  Size _lastSize = Size.zero;

  /// The app that opens images (the default image viewer), if any.
  late final DesktopEntry? _viewer = _findViewer();

  @override
  void initState() {
    super.initState();
    _arbiter = LayerArbiter.of(LayerControl.of(context));
    _commands = ShellChannel.instance.commands.listen((id) {
      switch (id) {
        case 'screenshot.selection':
          _startCrosshair(clipboard: false);
        case 'screenshot.selection_clipboard':
          _startCrosshair(clipboard: true);
        case 'screenshot.toolbar':
          _openToolbar();
      }
    });
    _drags = _services.dragFinished.listen(_dragFinished);
    ShellChannel.instance.states.addListener(_statesChanged);
    _pushSettings();
  }

  @override
  void dispose() {
    _arbiter.remove(this);
    _commands?.cancel();
    _drags?.cancel();
    ShellChannel.instance.states.removeListener(_statesChanged);
    _dwell.dispose();
    _ticker?.cancel();
    _settle.dispose();
    _leave.dispose();
    _focus.dispose();
    _shot?.dragImage?.dispose();
    super.dispose();
  }

  // --- Settings -------------------------------------------------------------------------

  DesktopEntry? _findViewer() {
    final entries = widget.entries;
    final id = defaultAppFor(
      'image/png',
      env: widget.env,
      exists: entries.containsKey,
    );
    if (id != null) return entries[id];
    for (final entry in entries.values) {
      if (entry.mimeTypes.contains('image/png') && entry.id != fileManagerId) {
        return entry;
      }
    }
    return null;
  }

  void _changeSettings(void Function(ScreenshotSettings s) change) {
    setState(() {
      change(_settings);
      _optionsEntries = null;
    });
    _settings.save(_store);
    _pushSettings();
  }

  /// Ossido draws the pointer into ⇧⌘3 captures itself: it learns the setting.
  void _pushSettings() {
    _services.execute('screenshot.settings', {
      'pointer': _settings.showPointer,
    });
  }

  String get _desktop => widget.folder ?? screenshotFolder(env: widget.env);
  String get _documents => widget.documents ?? documentsFolder(env: widget.env);
  String get _cache => widget.cache ?? screenshotCache(env: widget.env);

  /// The folder of a destination (null: clipboard and apps have none).
  String? _folderOf(ShotDestination to) => switch (to) {
    ToDesktop() => _desktop,
    ToDocuments() => _documents,
    ToFolder(:final path) => Directory(path).existsSync() ? path : _desktop,
    ToClipboard() || ToApp() => null,
  };

  // --- Modes (crosshair, window, bar, timer) --------------------------------------------

  void _startCrosshair({required bool clipboard}) {
    _toClipboard = clipboard;
    _enter(_Mode.selection);
  }

  void _openToolbar() {
    _toClipboard = false;
    _choice = _settings.mode;
    _selection = null;
    _barCenter = _settings.barCenter;
    _enter(_Mode.toolbar);
  }

  void _enter(_Mode mode) {
    _ticker?.cancel();
    setState(() {
      _mode = mode;
      _live = null;
    });
    _syncLayer();
    _syncVeil();
    WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  /// The bar goes away (× or Esc) at once.
  void _closeBar() => _cancel();

  void _cancel() {
    _ticker?.cancel();
    setState(() {
      _mode = _Mode.none;
      _live = null;
    });
    _syncLayer();
    _syncVeil();
  }

  bool get _windowMode =>
      _mode == _Mode.window ||
      (_mode == _Mode.toolbar && _choice == ToolbarChoice.window);

  /// In window mode Ossido veils the frame of the window under the pointer (K-05: it knows
  /// the frames, the shell does not). A shell given `windowAt` draws the veil itself.
  bool _compositorVeil = false;
  bool _veilAsked = false;

  Future<void> _syncVeil() async {
    final on = _windowMode && widget.windowAt == null;
    if (on == _veilAsked) return;
    _veilAsked = on;
    final done = await _services.execute('screenshot.window_veil', {
      'enabled': on,
    });
    if (!mounted) return;
    setState(() => _compositorVeil = on && done);
  }

  /// The input region: everything with a tool or the thumbnail's menu open; the thumbnail
  /// alone otherwise (none while it is dragged); nothing while the timer runs, so the apps
  /// can be used before the capture.
  void _syncLayer() {
    final shot = _shot;
    final thumb = shot == null || shot.dragging
        ? null
        : thumbnailRect(shot.size, _lastSize);
    final LayerWish wish;
    if ((_mode != _Mode.none && _mode != _Mode.countdown) || _menuAt != null) {
      wish = const LayerWish.all();
    } else if (thumb != null && !thumb.isEmpty) {
      wish = LayerWish.rects([thumb]);
    } else {
      wish = const LayerWish.none();
    }
    _arbiter.set(this, wish);
  }

  /// Hides the tools, counts down the timer if asked (from the bar), waits for Ossido to
  /// redraw the screen without them, then captures.
  Future<void> _capture(
    String id,
    Map<String, Object?> params, {
    bool timed = false,
    Rect? target,
  }) async {
    final clipboard = _toClipboard;
    final seconds = timed ? _settings.timer : 0;
    if (seconds > 0) {
      _ticker?.cancel();
      setState(() {
        _mode = _Mode.countdown;
        _live = null;
        _countdown = seconds;
        _countdownTarget = target;
      });
      _syncLayer();
      _syncVeil();
      final finished = Completer<bool>();
      _ticker = Timer.periodic(const Duration(seconds: 1), (t) {
        if (!mounted || _mode != _Mode.countdown) {
          t.cancel();
          if (!finished.isCompleted) finished.complete(false);
          return;
        }
        if (_countdown <= 1) {
          t.cancel();
          if (!finished.isCompleted) finished.complete(true);
          return;
        }
        setState(() => _countdown--);
      });
      if (!await finished.future || !mounted) return;
    }
    _cancel();
    await SchedulerBinding.instance.endOfFrame;
    await Future<void>.delayed(const Duration(milliseconds: 60));
    await _services.execute(id, {
      ...params,
      'pointer': _settings.showPointer,
      if (clipboard) 'clipboard': true,
    });
  }

  KeyEventResult _key(FocusNode node, KeyEvent event) {
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.space && _live != null) {
      // Space with the button held moves the selection.
      if (event is KeyDownEvent) _live!.moving = _pointer;
      if (event is KeyUpEvent) _live!.moving = null;
      return KeyEventResult.handled;
    }
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    if (key == LogicalKeyboardKey.escape) {
      // The Options menu, when open, takes Esc itself (it has the focus).
      _closeBar();
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.space &&
        (_mode == _Mode.selection || _mode == _Mode.window)) {
      setState(() {
        _mode = _mode == _Mode.selection ? _Mode.window : _Mode.selection;
      });
      _syncVeil();
      return KeyEventResult.handled;
    }
    if ((key == LogicalKeyboardKey.enter ||
            key == LogicalKeyboardKey.numpadEnter) &&
        _mode == _Mode.toolbar) {
      _action();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  bool get _shift => HardwareKeyboard.instance.isShiftPressed;
  bool get _option => HardwareKeyboard.instance.isAltPressed;

  void _pointerDown(Offset at) {
    _pointer = at;
    if (_mode == _Mode.selection) setState(() => _live = LiveSelection(at));
  }

  void _pointerMove(Offset at) {
    setState(() {
      _pointer = at;
      _live?.update(at, shift: _shift, option: _option);
    });
  }

  void _pointerUp(Offset at) {
    switch (_mode) {
      case _Mode.selection:
        final live = _live;
        if (live == null) return;
        live.update(at, shift: _shift, option: _option);
        final rect = live.rect(option: _option);
        // A click without dragging captures nothing.
        if (rect.width < 4 || rect.height < 4) {
          setState(() => _live = null);
          return;
        }
        _captureRegion(rect);
      case _Mode.window:
        _captureWindow(at);
      case _Mode.toolbar:
      case _Mode.none:
      case _Mode.countdown:
        break;
    }
  }

  void _captureRegion(Rect rect, {bool timed = false}) => _capture(
    'screenshot.region',
    {'x': rect.left, 'y': rect.top, 'width': rect.width, 'height': rect.height},
    timed: timed,
    target: rect,
  );

  /// The window under the point with its shadow (Option: without), or the whole screen
  /// over the desktop.
  void _captureWindow(Offset at, {bool timed = false}) => _capture(
    'screenshot.window_at',
    {'x': at.dx, 'y': at.dy, 'shadow': !_option, 'or_screen': true},
    timed: timed,
  );

  /// The bar's action button (or Enter).
  void _action() {
    switch (_choice) {
      case ToolbarChoice.screen:
        _capture('screenshot.screen', {}, timed: true);
      case ToolbarChoice.window:
        if (_pointer case final p?) _captureWindow(p, timed: true);
      case ToolbarChoice.selection:
        final rect = _selection;
        if (rect == null) return;
        _changeSettings((s) => s.selection = rect);
        _captureRegion(rect, timed: true);
    }
  }

  void _choose(ToolbarChoice choice) {
    setState(() {
      _choice = choice;
    });
    _changeSettings((s) => s.mode = choice);
    _syncVeil();
  }

  // --- The capture arrives ------------------------------------------------------------------

  void _statesChanged() {
    final value = ShellChannel.instance.states.value['screenshot'] ?? '';
    if (value.isEmpty) return;
    final Object? json;
    try {
      json = jsonDecode(value);
    } on FormatException {
      return;
    }
    if (json is! Map) return;
    final path = json['path'], w = json['width'], h = json['height'];
    final seq = json['seq'];
    if (path is! String || w is! num || h is! num || w <= 0 || h <= 0) return;
    if (seq is int) {
      if (seq == _lastSeq) return;
      _lastSeq = seq;
    }
    _services.playSound(captureSound);
    if (json['clipboard'] == true) {
      // ⌃: straight to the clipboard, no thumbnail, no file.
      _toClipboardNow(path);
      return;
    }
    final shot = _Shot(
      path,
      Size(w.toDouble(), h.toDouble()),
      widget.now?.call() ?? DateTime.now(),
      _settings.destination,
    );
    // A thumbnail still showing: its capture goes to its place right away.
    final previous = _shot;
    if (previous != null) _finish(previous);
    _dwell.stop();
    if (!_settings.showThumbnail) {
      setState(() => _shot = null);
      _syncLayer();
      _finish(shot);
      return;
    }
    setState(() {
      _shot = shot;
      _menuAt = null;
      _hovered = false;
    });
    _leave.value = 0;
    _settle.forward(from: 0);
    _restartHold(thumbnailSettle + thumbnailHold);
    _syncLayer();
    thumbnailDragImage(path, shot.size).then((image) {
      if (!mounted || shot.done) {
        image?.dispose();
        return;
      }
      shot.dragImage = image;
    });
  }

  Future<void> _toClipboardNow(String path) async {
    await _services.copyImage(path);
    try {
      File(path).deleteSync();
    } on FileSystemException {
      // Already gone.
    }
  }

  void _restartHold(Duration after) {
    _dwell
      ..stop()
      ..duration = after
      ..forward(from: 0);
  }

  /// The time is up: the thumbnail slides off to the right, then the capture goes to its
  /// place. The pointer on it or its menu keep it.
  Future<void> _dismiss() async {
    final shot = _shot;
    if (shot == null || shot.dragging || _hovered || _menuAt != null) return;
    await _slideAway(shot);
  }

  Future<void> _slideAway(_Shot shot) async {
    _dwell.stop();
    await _leave.forward(from: 0);
    if (!mounted || _shot != shot) return;
    _removeThumbnail();
    _finish(shot);
  }

  void _removeThumbnail() {
    final shot = _shot;
    _dwell.stop();
    setState(() {
      _shot = null;
      _menuAt = null;
      _hovered = false;
    });
    _syncLayer();
    if (shot != null && shot.done) {
      shot.dragImage?.dispose();
      shot.dragImage = null;
    }
  }

  /// Moves the capture into `folder` under its name; the new path (null if it failed).
  String? _place(_Shot shot, String folder) {
    final placed = shot.placed;
    if (placed != null && File(placed).parent.path == folder) return placed;
    try {
      Directory(folder).createSync(recursive: true);
    } on FileSystemException {
      return null;
    }
    final name = screenshotName(
      shot.time,
      (n) => File('$folder/$n').existsSync(),
      texts: mounted ? l10nOf(context) : null,
    );
    final target = '$folder/$name';
    if (!moveFile(shot.file, target)) return null;
    shot.placed = target;
    return target;
  }

  /// Sends the capture to `to` (its own destination by default); then, if asked, opens it
  /// or shows it in the file manager.
  Future<void> _finish(
    _Shot shot, {
    ShotDestination? to,
    bool open = false,
    bool reveal = false,
  }) async {
    if (shot.done) return;
    shot.done = true;
    final destination = to ?? shot.destination;
    String? path;
    switch (destination) {
      case ToClipboard():
        await _services.copyImage(shot.file);
        if (open || reveal) {
          path = _place(shot, _cache);
        } else {
          try {
            File(shot.file).deleteSync();
          } on FileSystemException {
            // Already gone.
          }
        }
      case ToApp(:final id):
        path = _place(shot, _cache);
        final app = widget.entries[id] ?? _viewer;
        if (path != null && app != null && !open) {
          _services.launch(
            execWithFiles(app.rawExec ?? app.exec, [path]) ??
                '${app.exec} ${shellQuote(path)}',
          );
        }
      case ToDesktop() || ToDocuments() || ToFolder():
        final folder = _folderOf(destination) ?? _desktop;
        path = _place(shot, folder);
    }
    if (path == null) return;
    if (open) {
      final quoted = shellQuote(path);
      _services.launch('gio open $quoted 2>/dev/null || xdg-open $quoted');
    }
    if (reveal) {
      _services.launch(openPathCommand(widget.entries, File(path).parent.path));
    }
  }

  // --- The thumbnail's gestures -------------------------------------------------------------

  /// A click: the image opens (with the default app: there is no Markup editor yet, #154).
  void _openShot() {
    final shot = _shot;
    if (shot == null) return;
    _removeThumbnail();
    _finish(shot, open: true);
  }

  void _hover(bool on) {
    _hovered = on;
    final shot = _shot;
    if (shot == null || shot.dragging) return;
    if (on) {
      _dwell.stop();
    } else if (_menuAt == null) {
      // PROVISIONAL: the full time again once the pointer leaves.
      _restartHold(thumbnailHold);
    }
  }

  /// Two fingers to the right: the capture is saved at once, and the thumbnail goes away.
  void _swipe() {
    final shot = _shot;
    if (shot == null || shot.dragging) return;
    _finish(shot);
    _slideAway(shot);
  }

  /// A press that moves: the image is dragged out as a real drag and drop of a file kept in
  /// the cache for the drag. The thumbnail hides; a frameless image, 128 pt on its long
  /// side, follows the pointer (SCREENSHOT-2).
  Future<void> _dragOut() async {
    final shot = _shot;
    if (shot == null || shot.done || shot.dragging) return;
    _dwell.stop();
    final path = _place(shot, _cache);
    if (path == null) return;
    setState(() => shot.dragging = true);
    _syncLayer();
    final started = await _services.startDrag(
      path,
      icon: shot.dragImage,
      hotspot: thumbnailDragHotspot(thumbnailDragSize(shot.size)),
    );
    if (!mounted || started) return;
    _dragEnded(shot, dropped: false);
  }

  void _dragFinished(String action) {
    final shot = _shot;
    if (shot == null || !shot.dragging) return;
    _dragEnded(shot, dropped: action != 'none');
  }

  void _dragEnded(_Shot shot, {required bool dropped}) {
    if (dropped) {
      // Taken by the target (a folder, the desktop, an app): it has its own copy, so this
      // file goes. The capture is moved there, not saved here as well.
      shot.done = true;
      _removeThumbnail();
      try {
        File(shot.file).deleteSync();
      } on FileSystemException {
        // The target moved it.
      }
      return;
    }
    // Dropped nowhere: the thumbnail comes back, and the capture goes to its place when it
    // leaves.
    setState(() => shot.dragging = false);
    _syncLayer();
    _restartHold(thumbnailHold);
  }

  void _openMenu(Offset at) {
    if (_shot == null) return;
    _dwell.stop();
    setState(() {
      _menuAt = at;
      _menuEntries = _thumbnailMenu();
    });
    _syncLayer();
  }

  void _menuClosed() {
    if (_menuAt == null) return;
    setState(() => _menuAt = null);
    _syncLayer();
    // The time goes on from where the menu stopped it.
    if (_shot != null && !_hovered) _dwell.forward();
  }

  /// A choice of the thumbnail's menu: the thumbnail goes away and the capture goes `to`.
  void _menuFinish({
    ShotDestination? to,
    bool reveal = false,
    bool delete = false,
  }) {
    final shot = _shot;
    if (shot == null) return;
    _menuAt = null;
    _removeThumbnail();
    if (delete) {
      shot.done = true;
      try {
        File(shot.file).deleteSync();
      } on FileSystemException {
        // Already gone.
      }
      return;
    }
    _finish(shot, to: to, reveal: reveal);
  }

  /// The apps the thumbnail and the Options menu can hand an image to, in the order of the
  /// reference (Mail, the image viewer as Preview, Photos); only the ones that exist.
  List<DesktopEntry> _imageApps() {
    final viewer = _viewer;
    return [
      if (widget.entries[shotMailId] case final mail?) mail,
      if (viewer != null) viewer,
      if (widget.entries[shotPhotosId] case final photos?
          when photos.id != viewer?.id)
        photos,
    ];
  }

  /// The thumbnail's menu (SCREENSHOT-2): save to Desktop, Documents, Clipboard | open in
  /// Mail, the image viewer, Photos | show in the file manager, Delete | Close. Markup is
  /// left out until the editor exists (#154).
  List<AmalfiMenuEntry> _thumbnailMenu() {
    final l = l10nOf(context);
    final files = widget.entries[fileManagerId];
    final apps = _imageApps();
    return [
      AmalfiMenuAction(
        l.shotSaveToDesktop,
        onSelected: () => _menuFinish(to: const ToDesktop()),
      ),
      AmalfiMenuAction(
        l.shotSaveToDocuments,
        onSelected: () => _menuFinish(to: const ToDocuments()),
      ),
      AmalfiMenuAction(
        l.shotSaveToClipboard,
        onSelected: () => _menuFinish(to: const ToClipboard()),
      ),
      if (apps.isNotEmpty) const AmalfiMenuDivider(),
      for (final app in apps)
        AmalfiMenuAction(
          l.shotOpenWith(app.name),
          onSelected: () => _menuFinish(to: ToApp(app.id)),
        ),
      const AmalfiMenuDivider(),
      if (files != null)
        AmalfiMenuAction(
          l.shotShowIn(files.name),
          onSelected: () => _menuFinish(reveal: true),
        ),
      AmalfiMenuAction(
        l.shotDelete,
        onSelected: () => _menuFinish(delete: true),
      ),
      const AmalfiMenuDivider(),
      AmalfiMenuAction(l.shotClose, onSelected: () => _menuFinish()),
    ];
  }

  // --- The bar's Options menu -------------------------------------------------------------

  Future<void> _chooseOtherFolder() async {
    final l = l10nOf(context);
    // The chooser is a window: the bar steps aside while it is open, then comes back.
    _cancel();
    final current = _folderOf(_settings.destination) ?? _desktop;
    final chosen = await _services.chooseFolder(
      title: l.shotChooseFolder,
      start: current,
    );
    if (!mounted) return;
    if (chosen != null) _changeSettings((s) => s.chooseFolder(chosen));
    _openToolbar();
  }

  List<AmalfiMenuEntry> _optionsMenu() {
    final l = l10nOf(context);
    final to = _settings.destination;
    String base(String path) => path.split('/').where((p) => p.isNotEmpty).last;
    void destination(ShotDestination d) =>
        _changeSettings((s) => s.destination = d);
    return [
      AmalfiMenuSection(l.saveTo),
      AmalfiMenuAction(
        l.shotDesktop,
        checked: to is ToDesktop,
        onSelected: () => destination(const ToDesktop()),
      ),
      AmalfiMenuAction(
        l.shotDocuments,
        checked: to is ToDocuments,
        onSelected: () => destination(const ToDocuments()),
      ),
      AmalfiMenuAction(
        l.shotClipboard,
        checked: to is ToClipboard,
        onSelected: () => destination(const ToClipboard()),
      ),
      for (final app in _imageApps())
        AmalfiMenuAction(
          app.name,
          checked: to == ToApp(app.id),
          onSelected: () => destination(ToApp(app.id)),
        ),
      for (final folder in _settings.recentFolders)
        AmalfiMenuAction(
          base(folder),
          checked: to == ToFolder(folder),
          onSelected: () => _changeSettings((s) => s.chooseFolder(folder)),
        ),
      if (widget.entries.containsKey(fileManagerId))
        AmalfiMenuAction(l.shotOtherFolder, onSelected: _chooseOtherFolder),
      const AmalfiMenuDivider(),
      AmalfiMenuSection(l.delay),
      for (final seconds in timerChoices)
        AmalfiMenuAction(
          seconds == 0 ? l.delayNone : l.delaySeconds(seconds),
          checked: _settings.timer == seconds,
          onSelected: () => _changeSettings((s) => s.timer = seconds),
        ),
      const AmalfiMenuDivider(),
      AmalfiMenuSection(l.options),
      AmalfiMenuAction(
        l.thumbnailAfterCapture,
        checked: _settings.showThumbnail,
        onSelected: () =>
            _changeSettings((s) => s.showThumbnail = !s.showThumbnail),
      ),
      AmalfiMenuAction(
        l.shotKeepSelection,
        checked: _settings.rememberSelection,
        onSelected: () =>
            _changeSettings((s) => s.rememberSelection = !s.rememberSelection),
      ),
      const AmalfiMenuDivider(),
      AmalfiMenuSection(l.shotCaptureSection),
      AmalfiMenuAction(
        l.shotIncludePointer,
        checked: _settings.showPointer,
        onSelected: () =>
            _changeSettings((s) => s.showPointer = !s.showPointer),
      ),
    ];
  }

  // --- Drawing ------------------------------------------------------------------------------

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final screen = box.biggest;
      if (screen != _lastSize) {
        _lastSize = screen;
        // The input region depends on the size (the thumbnail's place).
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) _syncLayer();
        });
      }
      final children = <Widget>[];
      if (_shot case final shot? when !shot.dragging) {
        final rect = thumbnailRect(shot.size, screen);
        // It appears in place with a fade (slowing down), then leaves to the right
        // accelerating.
        final settle = Curves.easeOut.transform(_settle.value);
        final dx =
            Curves.easeIn.transform(_leave.value) *
            (thumbnailMargin + rect.width + 30);
        children.add(
          Positioned.fromRect(
            rect: rect.shift(Offset(dx, 0)),
            child: Opacity(
              opacity: settle,
              child: ThumbnailView(
                key: const ValueKey('thumbnail'),
                file: shot.file,
                onOpen: _openShot,
                onMenu: _openMenu,
                onDragOut: _dragOut,
                onSwipe: _swipe,
                onHover: _hover,
              ),
            ),
          ),
        );
      }
      if (_mode == _Mode.countdown) children.add(_countdownView(screen));
      if (_mode != _Mode.none && _mode != _Mode.countdown) {
        children.add(Positioned.fill(child: _overlay(screen)));
      }
      if (_menuAt case final at?) {
        children.add(
          Positioned.fill(
            child: ShotMenu(
              key: const ValueKey('thumbnail menu'),
              entries: _menuEntries,
              // At the pointer: the menu moves left when the thumbnail is at the right edge.
              anchor: at,
              upwards: true,
              top: barHeight,
              // Drawn by Flutter: it covers the thumbnail (Ossido's glass would sit behind
              // the whole layer, thumbnail included).
              onDismissed: _menuClosed,
            ),
          ),
        );
      }
      return Stack(children: children);
    },
  );

  Widget _countdownView(Size screen) {
    final target = _countdownTarget ?? Offset.zero & screen;
    return Positioned.fromRect(
      rect: Rect.fromCenter(
        center: target.center,
        width: countdownSize.width,
        height: countdownSize.height,
      ),
      child: IgnorePointer(
        child: Container(
          key: const ValueKey('countdown'),
          decoration: BoxDecoration(
            color: const Color(0x99000000),
            borderRadius: BorderRadius.circular(20),
          ),
          alignment: Alignment.center,
          child: Text(
            '$_countdown',
            style: uiText(
              size: 48,
              color: const Color(0xFFFFFFFF),
            ).copyWith(fontWeight: FontWeight.w300),
          ),
        ),
      ),
    );
  }

  Rect _barRect(Size screen) {
    final center =
        _barCenter ??
        Offset(
          screen.width / 2,
          screen.height - toolbarAboveDock - toolbarSize.height / 2,
        );
    final rect = Rect.fromCenter(
      center: center,
      width: toolbarSize.width,
      height: toolbarSize.height,
    );
    // Always whole on the screen.
    final dx = rect.left < 0
        ? -rect.left
        : rect.right > screen.width
        ? screen.width - rect.right
        : 0.0;
    final dy = rect.top < 0
        ? -rect.top
        : rect.bottom > screen.height
        ? screen.height - rect.bottom
        : 0.0;
    return rect.shift(Offset(dx, dy));
  }

  /// Where the Options menu is while it is open (the pull-down's own [AmalfiMenu], in this
  /// layer's overlay), on the layer; null when it is closed.
  Rect? _openMenuRect() {
    Rect? found;
    void visit(Element element) {
      if (found != null) return;
      if (element.widget is AmalfiMenu) {
        final box = element.renderObject;
        if (box is RenderBox && box.attached && box.hasSize) {
          found = box.localToGlobal(Offset.zero) & box.size;
        }
        return;
      }
      element.visitChildren(visit);
    }

    context.visitChildElements(visit);
    return found;
  }

  /// Whether the pointer is over the Options menu, and where the menu is: worked out after
  /// each frame while the bar is up (the menu opens and closes under a resting pointer).
  bool _overMenu = false;
  Rect? _menuRect;

  bool _menuHolds(Offset pointer) =>
      _openMenuRect()?.contains(pointer) ?? false;

  bool _syncing = false;

  /// Looks at the Options menu after every frame while the bar is up: the pull-down opens
  /// and closes it on its own, without building this layer again.
  void _watchMenu() {
    if (_syncing) return;
    _syncing = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _syncing = false;
      if (!mounted) return;
      _syncMenu();
      if (_mode == _Mode.toolbar) _watchMenu();
    });
  }

  void _syncMenu() {
    final pointer = _pointer;
    final menu = _mode == _Mode.toolbar ? _openMenuRect() : null;
    final over = menu != null && pointer != null && menu.contains(pointer);
    if (over != _overMenu || menu != _menuRect) {
      setState(() {
        _overMenu = over;
        _menuRect = menu;
      });
    }
  }

  Widget _overlay(Size screen) {
    final theme = AmalfiTheme.of(context);
    final toolbar = _mode == _Mode.toolbar;
    final bar = toolbar ? _barRect(screen) : null;
    // The camera is hidden while the pointer is over the bar or over the Options menu.
    if (toolbar) _watchMenu();
    final overBar =
        bar != null &&
        _pointer != null &&
        (bar.contains(_pointer!) || _overMenu);
    final camera = _windowMode || (toolbar && _choice == ToolbarChoice.screen);
    final cursor = overBar || (toolbar && _choice == ToolbarChoice.selection)
        ? MouseCursor.defer
        : SystemMouseCursors.none;
    // An overlay of its own for the widgets of the layer that need one, and the surface that
    // tells the Options menu (and the other widgets with a `TapRegion`) of a press outside
    // them: the menu closes when the user goes back to editing the selection (#220).
    return TapRegionSurface(
      // The pointer is followed over the Options menu too (it is an entry of the overlay,
      // not below the region of the layer's content).
      child: MouseRegion(
        onHover: (e) => setState(() {
          _pointer = e.localPosition;
          _overMenu = toolbar && _menuHolds(e.localPosition);
        }),
        child: Overlay.wrap(
          child: Focus(
            focusNode: _focus,
            onKeyEvent: _key,
            child: MouseRegion(
              cursor: cursor,
              child: Listener(
                behavior: HitTestBehavior.opaque,
                onPointerDown: (e) {
                  if (!toolbar) _pointerDown(e.localPosition);
                },
                onPointerMove: (e) => _pointerMove(e.localPosition),
                onPointerUp: (e) {
                  if (!toolbar) _pointerUp(e.localPosition);
                },
                child: Stack(
                  children: [
                    // ⇧⌘5, screen or window: a click on the screen captures.
                    if (toolbar && _choice != ToolbarChoice.selection)
                      Positioned.fill(
                        child: GestureDetector(
                          key: const ValueKey('capture area'),
                          behavior: HitTestBehavior.opaque,
                          onTapUp: (d) => _choice == ToolbarChoice.screen
                              ? _capture('screenshot.screen', {}, timed: true)
                              : _captureWindow(d.localPosition, timed: true),
                        ),
                      )
                    else
                      const Positioned.fill(child: SizedBox.expand()),
                    if (toolbar && _choice == ToolbarChoice.selection)
                      Positioned.fill(
                        child: SelectionEditor(
                          rect: _selection ??= _initialSelection(screen),
                          // The glass of the Options menu over the selection is the
                          // compositor's, behind this layer: the dim leaves it bare.
                          holes: [
                            if (_menuRect case final menu?)
                              RRect.fromRectAndRadius(
                                menu,
                                const Radius.circular(AmalfiMenuMetrics.radius),
                              ),
                          ],
                          onChanged: (r) => setState(() => _selection = r),
                          onCommitted: (r) {
                            if (_settings.rememberSelection) {
                              _changeSettings((s) => s.selection = r);
                            }
                          },
                        ),
                      ),
                    if (_mode == _Mode.selection)
                      if (_live case final live?)
                        Positioned.fill(
                          child: SelectionFrame(
                            rect: live.rect(option: _option),
                          ),
                        ),
                    if (_pointer case final p? when _mode == _Mode.selection)
                      ..._crosshair(p),
                    // The target is tinted `Accent/Amalfi` at 50 % (kit `Window tint`, `Screen tint`):
                    // the window under the pointer, or the whole screen when the desktop is the target.
                    if (toolbar && _choice == ToolbarChoice.screen)
                      Positioned.fill(
                        child: IgnorePointer(
                          child: ColoredBox(
                            key: const ValueKey('screen tint'),
                            color: targetTint(theme),
                          ),
                        ),
                      ),
                    if (_pointer case final p?
                        when _windowMode && !_compositorVeil && !overBar)
                      Positioned.fromRect(
                        rect: widget.windowAt?.call(p) ?? Offset.zero & screen,
                        child: IgnorePointer(
                          child: ColoredBox(
                            key: const ValueKey('veil'),
                            color: targetTint(theme),
                          ),
                        ),
                      ),
                    if (_pointer case final p? when camera && !overBar)
                      Positioned(
                        left: p.dx - 16,
                        top: p.dy - 16,
                        child: const IgnorePointer(
                          child: ShotCamera(key: ValueKey('camera')),
                        ),
                      ),
                    if (bar != null) ..._toolbar(bar),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Rect _initialSelection(Size screen) {
    final remembered = _settings.selection;
    if (_settings.rememberSelection && remembered != null) {
      return fitSelection(remembered, screen);
    }
    return defaultSelection(screen);
  }

  /// The crosshair (kit `Pointers/Crosshair`, 24 × 24) and, 20 pt to its right, the chip with
  /// the point on two lines (kit `Coordinates capsule`, 8 pt above its center).
  List<Widget> _crosshair(Offset p) {
    // The kit draws the crosshair in the same ink in both appearances, on its white outline.
    final ink = AmalfiColors.light.textPrimary;
    return [
      Positioned(
        left: p.dx - 12,
        top: p.dy - 12,
        child: IgnorePointer(
          child: CustomPaint(
            key: const ValueKey('crosshair'),
            size: const Size(24, 24),
            painter: CrosshairPainter(ink),
          ),
        ),
      ),
      Positioned(
        left: p.dx + 20,
        top: p.dy - 8,
        child: IgnorePointer(
          child: CrosshairLabel(text: '${p.dx.round()}\n${p.dy.round()}'),
        ),
      ),
    ];
  }

  List<Widget> _toolbar(Rect bar) {
    final l = l10nOf(context);
    // In window mode the target is the window under the pointer: none while the pointer is
    // on the bar. The bar's right 85 pt (the action button's space) do not count, or the
    // bar would shrink and grow again under a pointer resting there.
    final narrow = Rect.fromLTRB(
      bar.left,
      bar.top,
      bar.right - toolbarActionSpace,
      bar.bottom,
    );
    final overBar = _pointer != null && narrow.contains(_pointer!);
    return [
      Positioned(
        // Keyed: the bar keeps its state (tooltip, animation) while the children before it
        // come and go.
        key: const ValueKey('shot toolbar'),
        left: bar.left,
        top: bar.top,
        child: ShotToolbar(
          choice: _choice,
          optionsLabel: l.options,
          optionsEntries: _optionsEntries ??= _optionsMenu(),
          actionLabel: l.capture,
          closeLabel: l.shotClose,
          tips: ShotBarTips(
            capture: [
              l.shotTipCaptureScreen,
              l.shotTipCaptureWindow,
              l.shotTipCaptureSelection,
            ],
            record: [
              l.shotTipRecordScreen,
              l.shotTipRecordWindow,
              l.shotTipRecordSelection,
            ],
          ),
          showAction: !(_choice == ToolbarChoice.window && overBar),
          onChoose: _choose,
          onClose: _closeBar,
          onAction: _action,
          onMove: (d) =>
              setState(() => _barCenter = _barRect(_lastSize).center + d),
          onMoved: () => _changeSettings((s) => s.barCenter = _barCenter),
        ),
      ),
    ];
  }
}
