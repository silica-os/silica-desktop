// Test app for the Silica embedder: known colors, so that the tests read the
// screenshot.
//
// - Every tap toggles the background between [idle] and [tapped].
// - Typing "word" in the text field makes the background [typed] (text input).
// - Esc clears taps and text (key events, through HardwareKeyboard).
// - The square at the top left tells the devicePixelRatio: [hiDpi] from 2 up,
//   otherwise [loDpi].
// - The square at the bottom left turns [hovered] once the pointer was over the main view: it
//   entered (a Wayland enter with no motion after it) or hovered (silica-desktop#242).
// - At the top right a glass area ([glassSize], silica-glass-v1): the opaque background
//   covers it, it is for the tests that read `glass.list`.
// - F2 opens a second window ([secondTitle], [secondSize], silica-desktop#146) drawn in its
//   own view, all [second] ([typed] once its own field holds the magic word): a tap in it
//   closes it, and so does its close button (the app answers `closeRequested` and closes it
//   itself).
// - F3 exports the main window for another app (`exportHandle`, xdg-foreign) and shows the
//   identifier it got ("wayland:<handle>") as the window title, for the tests to read.
// - F4 opens a popup ([popupSize], [popup], silica-desktop#4) from the main window's right
//   edge, so it extends past the window: an `xdg_popup` with a grab in its own view. A tap in
//   it closes it, and so does a press outside it (Ossido's `popup_done`).
// - `--close=MODE` (a Dart argument, silica-desktop#192) sets how the main window's close
//   request is answered: `none` listens on no `silica/window` at all (an app on an older
//   silica_ui), `silent` listens but never answers it (a hung handler), `ask` says it is
//   asking (`closeAsking`) and closes the window at the next tap, as a confirmation would;
//   `draw` does the same without `closeAsking`, drawing [typed] meanwhile (a confirmation
//   from silica_ui before 0.38).
//   Without it the app closes the main window itself at once.
// - `--empty` (a Dart argument) draws nothing at all in the main view: its scene is empty from
//   the first frame, as the hidden Dock's at session start (silica-desktop#242).
import 'dart:async';
import 'dart:ui' show FlutterView;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

const Color idle = Color(0xFF3366CC);
const Color tapped = Color(0xFFCC6633);
const Color typed = Color(0xFF33CC66);
const Color loDpi = Color(0xFFFF00FF);
const Color hiDpi = Color(0xFF00FF00);
const double probeSize = 16;
const String magicWord = 'word';
const Size glassSize = Size(40, 20);
const Color second = Color(0xFF8833AA);
const String secondTitle = 'Second';
const Size secondSize = Size(200, 120);
const Color popup = Color(0xFFEE2288);
const Color hovered = Color(0xFFFFCC00);
const Size popupSize = Size(120, 80);

/// The `silica/window` channel of the embedder (several windows).
const MethodChannel windowChannel = MethodChannel(
  'silica/window',
  JSONMethodCodec(),
);

/// How the main window's close request is answered (`--close=MODE`).
String closeMode = '';

/// The main view draws nothing at all (`--empty`): an empty scene from the first frame.
bool empty = false;

void main(List<String> args) {
  for (final arg in args) {
    if (arg.startsWith('--close='))
      closeMode = arg.substring('--close='.length);
    if (arg == '--empty') empty = true;
  }
  runApp(empty ? const SizedBox.shrink() : const DemoApp());
}

class DemoApp extends StatefulWidget {
  const DemoApp({super.key});

  @override
  State<DemoApp> createState() => _DemoAppState();
}

class _DemoAppState extends State<DemoApp> {
  int taps = 0;

  /// Whether the pointer was seen over the main view (it entered or hovered).
  bool hoverSeen = false;

  void _sawPointer() {
    if (!hoverSeen) setState(() => hoverSeen = true);
  }

  final TextEditingController text = TextEditingController();
  final FocusNode textFocus = FocusNode();

  /// The second window's view, while it is open.
  FlutterView? secondView;

  /// The popup's view, while it is open.
  FlutterView? popupView;

  /// The answer to a main window close request being asked (`--close=ask`).
  Completer<void>? asking;

  @override
  void initState() {
    super.initState();
    text.addListener(() => setState(() {}));
    HardwareKeyboard.instance.addHandler(_onKey);
    if (closeMode != 'none') windowChannel.setMethodCallHandler(_onWindowCall);
  }

  Future<Object?> _onWindowCall(MethodCall call) async {
    final args = call.arguments;
    final id = args is Map ? args['viewId'] : null;
    switch (call.method) {
      case 'closeRequested':
        if (id == 0 && closeMode == 'silent') {
          // Never answered.
          await Completer<void>().future;
        }
        if (id == 0 && (closeMode == 'ask' || closeMode == 'draw')) {
          if (closeMode == 'ask') {
            await windowChannel.invokeMethod<Object?>('closeAsking', {
              'viewId': id,
              'request': args is Map ? args['request'] : null,
            });
          }
          final answer = Completer<void>();
          setState(() => asking = answer);
          await answer.future;
        }
        // The app closes the window itself (it could ask to save first).
        await _closeSecond(id);
        return true;
      case 'closeProbe':
        // True: this app says closeAsking when it asks (not `draw`, an older silica_ui).
        return closeMode == 'silent' || closeMode == 'ask';
      case 'windowClosed':
        if (id == secondView?.viewId) setState(() => secondView = null);
        if (id == popupView?.viewId) setState(() => popupView = null);
    }
    return null;
  }

  Future<void> _openSecond() async {
    if (secondView != null) return;
    final id = await windowChannel.invokeMethod<Object?>('openWindow', {
      'title': secondTitle,
      'width': secondSize.width,
      'height': secondSize.height,
      'minWidth': 150,
      'minHeight': 100,
      'parent': 0,
    });
    if (id is! int || !mounted) return;
    final views = WidgetsBinding.instance.platformDispatcher.views;
    setState(() {
      for (final view in views) {
        if (view.viewId == id) secondView = view;
      }
    });
  }

  Future<void> _openPopup() async {
    if (popupView != null) return;
    final size =
        View.of(context).physicalSize / View.of(context).devicePixelRatio;
    final id = await windowChannel.invokeMethod<Object?>('openPopup', {
      'parent': 0,
      'width': popupSize.width,
      'height': popupSize.height,
      // A point on the window's right edge, half way down: the popup hangs from it, past
      // the window.
      'anchorRect': [size.width - 20, size.height / 2, 1, 1],
      'adjust': ['slideX', 'slideY'],
    });
    // {viewId, x, y, width, height}: where Ossido put the popup.
    final viewId = id is Map ? id['viewId'] : null;
    if (viewId is! int || !mounted) return;
    final views = WidgetsBinding.instance.platformDispatcher.views;
    setState(() {
      for (final view in views) {
        if (view.viewId == viewId) popupView = view;
      }
    });
  }

  Future<void> _exportWindow() async {
    final handle = await windowChannel.invokeMethod<Object?>('exportHandle');
    if (handle is! String) return;
    await windowChannel.invokeMethod<Object?>('setTitle', {'title': handle});
  }

  Future<void> _closeSecond(Object? id) async {
    if (id == null) return;
    await windowChannel.invokeMethod<Object?>('closeWindow', {'viewId': id});
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_onKey);
    text.dispose();
    textFocus.dispose();
    super.dispose();
  }

  bool _onKey(KeyEvent event) {
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.escape) {
      setState(() => taps = 0);
      text.clear();
      return true;
    }
    if (event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.f2) {
      _openSecond();
      return true;
    }
    if (event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.f3) {
      _exportWindow();
      return true;
    }
    if (event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.f4) {
      _openPopup();
      return true;
    }
    return false;
  }

  Color get background {
    if (text.text == magicWord || (asking != null && closeMode == 'draw')) {
      return typed;
    }
    return taps.isEven ? idle : tapped;
  }

  @override
  Widget build(BuildContext context) {
    final view = secondView;
    final menu = popupView;
    return ViewAnchor(
      view: view == null && menu == null
          ? null
          : ViewCollection(
              views: [
                if (view != null)
                  View(
                    view: view,
                    child: SecondWindow(onTap: () => _closeSecond(view.viewId)),
                  ),
                if (menu != null)
                  View(
                    view: menu,
                    child: GestureDetector(
                      behavior: HitTestBehavior.opaque,
                      onTap: () => _closeSecond(menu.viewId),
                      child: const ColoredBox(color: popup),
                    ),
                  ),
              ],
            ),
      child: _main(context),
    );
  }

  Widget _main(BuildContext context) {
    const white = Color(0xFFFFFFFF);
    return Directionality(
      textDirection: TextDirection.ltr,
      child: MouseRegion(
        onEnter: (_) => _sawPointer(),
        onHover: (_) => _sawPointer(),
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: () {
            final answer = asking;
            if (answer != null) {
              setState(() => asking = null);
              answer.complete();
              return;
            }
            setState(() => taps++);
          },
          child: ColoredBox(
            color: background,
            child: Stack(
              children: [
                Positioned(
                  left: 0,
                  top: 0,
                  width: probeSize,
                  height: probeSize,
                  child: ColoredBox(
                    color: MediaQuery.devicePixelRatioOf(context) >= 2
                        ? hiDpi
                        : loDpi,
                  ),
                ),
                Positioned(
                  left: 0,
                  bottom: 0,
                  width: probeSize,
                  height: probeSize,
                  child: ColoredBox(color: hoverSeen ? hovered : background),
                ),
                Positioned(
                  right: 0,
                  top: 0,
                  width: glassSize.width,
                  height: glassSize.height,
                  child: const Glass(radius: 6, child: SizedBox.expand()),
                ),
                Center(
                  child: Text(
                    'Silica · $taps',
                    style: const TextStyle(fontSize: 32, color: white),
                  ),
                ),
                Positioned(
                  left: 24,
                  right: 24,
                  bottom: 16,
                  child: EditableText(
                    controller: text,
                    focusNode: textFocus,
                    autofocus: true,
                    onTapOutside: (_) {},
                    style: const TextStyle(fontSize: 20, color: white),
                    cursorColor: white,
                    backgroundCursorColor: const Color(0xFF000000),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The second window's content: all [second], [typed] once its own field holds the magic
/// word (the keyboard reaches the field of the window that has it); a tap calls [onTap].
class SecondWindow extends StatefulWidget {
  const SecondWindow({super.key, required this.onTap});

  final VoidCallback onTap;

  @override
  State<SecondWindow> createState() => _SecondWindowState();
}

class _SecondWindowState extends State<SecondWindow> {
  final TextEditingController text = TextEditingController();
  final FocusNode focus = FocusNode();

  @override
  void initState() {
    super.initState();
    text.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    text.dispose();
    focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Directionality(
    textDirection: TextDirection.ltr,
    child: GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: widget.onTap,
      child: ColoredBox(
        color: text.text == magicWord ? typed : second,
        child: Align(
          alignment: Alignment.bottomCenter,
          child: EditableText(
            controller: text,
            focusNode: focus,
            autofocus: true,
            style: const TextStyle(fontSize: 14, color: Color(0xFFFFFFFF)),
            cursorColor: const Color(0xFFFFFFFF),
            backgroundCursorColor: const Color(0xFF000000),
          ),
        ),
      ),
    ),
  );
}
