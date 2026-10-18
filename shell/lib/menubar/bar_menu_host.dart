// The menus the bar opens (app menus, the logo menu, status menus, the desktop's context menu)
// on silica_ui's Amalfi menu (`AmalfiMenu`: Glass/Thick, radius 12, highlight Accent radius 7).
//
// The menu model stays the one the apps publish (`MenuEntry` of silica_ui, built in
// app_menus.dart, logo_menu.dart…): `amalfiEntries` turns it into the Amalfi rows. This host
// does what the bar needs around the menu: it puts it below its title (or at the pointer),
// fades it in and out with `fade.instant` (motion.md: no scale), reports the rectangles of the
// menu and of an open submenu (the bar's input region and Ossido's "press outside" depend
// on them), and takes ← and → to the neighbouring title.

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../system/popup_menu_model.dart';

/// The Amalfi rows of a menu of the shell's model. A custom row (`MenuCustom`) has no Amalfi
/// counterpart and is left out: the menus of the bar have none.
List<AmalfiMenuEntry> amalfiEntries(List<MenuEntry> entries) => [
  for (final entry in entries)
    switch (entry) {
      MenuAction() => AmalfiMenuAction(
        entry.label,
        shortcut: entry.shortcut,
        icon: _icon(entry.icon),
        checked: entry.checked,
        enabled: entry.enabled,
        onSelected: entry.onSelected,
        submenu: entry.submenu == null ? null : amalfiEntries(entry.submenu!),
      ),
      MenuDivider() => const AmalfiMenuDivider(),
      MenuSection() => AmalfiMenuSection(entry.label),
      MenuCustom() => null,
    },
].whereType<AmalfiMenuEntry>().toList();

/// The row's icon, which the model draws in the colour it is given: the text colour the row
/// gives its `IconTheme`.
Widget? _icon(Widget Function(Color color)? icon) => icon == null
    ? null
    : Builder(
        builder: (context) =>
            icon(IconTheme.of(context).color ?? const Color(0xFF000000)),
      );

/// Where the menu goes: its top left corner at the anchor, moved inside the surface when it
/// would leave it.
class _MenuLayout extends SingleChildLayoutDelegate {
  const _MenuLayout(this.anchor, {this.flip = false});

  final Offset anchor;

  /// Past the right edge the menu opens to the left of the anchor (a context menu at the
  /// pointer) instead of sliding in.
  final bool flip;

  static const double margin = 8;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      BoxConstraints(
        maxWidth: constraints.maxWidth,
        maxHeight: (constraints.maxHeight - margin).clamp(0, double.infinity),
      );

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    double fit(double v, double extent, double room) =>
        (v + extent > room ? room - extent : v).clamp(0, double.infinity);
    final x = flip && anchor.dx + childSize.width > size.width
        ? anchor.dx - childSize.width
        : anchor.dx;
    return Offset(
      fit(x, childSize.width, size.width),
      fit(anchor.dy, childSize.height, size.height - margin),
    );
  }

  @override
  bool shouldRelayout(_MenuLayout old) =>
      old.anchor != anchor || old.flip != flip;
}

/// Where the menu of a control goes (`PopupMenuLayout`): the measured placement, cut to the
/// output with the rows scrolled when it does not fit.
class _ControlMenuLayout extends SingleChildLayoutDelegate {
  const _ControlMenuLayout(this.control);

  final PopupMenuLayout control;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) {
    final height = control.place(constraints.biggest, 0).height;
    return BoxConstraints(
      maxWidth: (constraints.maxWidth - 2 * AmalfiPopUpMenuMetrics.screenMargin)
          .clamp(0, double.infinity),
      minHeight: height,
      maxHeight: height,
    );
  }

  @override
  Offset getPositionForChild(Size size, Size childSize) =>
      control.place(size, childSize.width).topLeft;

  @override
  bool shouldRelayout(_ControlMenuLayout old) => old.control != control;
}

/// A menu of the bar, shown inside the bar's surface (which covers the screen while a menu is
/// open).
class BarMenuHost extends StatefulWidget {
  const BarMenuHost({
    super.key,
    required this.entries,
    required this.anchor,
    this.entryKey,
    this.highlightFirst = false,
    this.flip = false,
    this.control,
    this.onDismissed,
    this.onNext,
    this.onPrevious,
    this.onRectsChanged,
  });

  /// The rows, top to bottom.
  final List<AmalfiMenuEntry> entries;

  /// The top left corner of the menu.
  final Offset anchor;

  /// Changes when the menu is another one (the next title): the rows start again, the fade
  /// does not.
  final Object? entryKey;

  /// Whether the first row is highlighted at once (opened with the keyboard).
  final bool highlightFirst;

  /// Whether the menu opens to the left of the anchor when it does not fit on the right.
  final bool flip;

  /// The menu of a pop-up button or a pull-down: placed by the measured rules over (or
  /// below) the control instead of at [anchor]; it appears in one frame, fades out ease-out
  /// and closes 150 ms after a choice (docs/spec/menus.md).
  final PopupMenuLayout? control;

  /// The menu is gone: after a choice, Esc, or a press outside, once it has faded out.
  final VoidCallback? onDismissed;

  /// → and ←: the menu of the next or the previous title (null: the keys do nothing).
  final VoidCallback? onNext;
  final VoidCallback? onPrevious;

  /// The rectangles of the menu and of its open submenu, in the surface, when they change.
  final ValueChanged<List<Rect>>? onRectsChanged;

  @override
  State<BarMenuHost> createState() => BarMenuHostState();
}

class BarMenuHostState extends State<BarMenuHost>
    with SingleTickerProviderStateMixin {
  /// Measured (menubar.md "Menus"): the menu opens in one frame, with no animation (the
  /// value starts at 1), and fades out linearly over ~190 ms after a choice or Esc.
  static const Duration fadeOut = Duration(milliseconds: 190);
  late final AnimationController _fade = AnimationController(
    vsync: this,
    duration: fadeOut,
    value: 1,
  );
  late final Animation<double> _opacity = widget.control == null
      ? _fade
      : CurvedAnimation(parent: _fade, curve: AmalfiPopUpMenuMetrics.fadeCurve);
  final GlobalKey _overlayKey = GlobalKey();
  late final OverlayEntry _entry = OverlayEntry(builder: _buildMenu);
  GlobalKey _menuKey = GlobalKey();
  List<Rect> _submenus = const [];
  List<Rect> _rects = const [];
  bool _closing = false;
  bool _disposed = false;

  /// The menu's rectangle (and the submenu's) as last seen, in the surface.
  List<Rect> get rects => _rects;

  @override
  void initState() {
    super.initState();
    HardwareKeyboard.instance.addHandler(_key);
    WidgetsBinding.instance.addPostFrameCallback((_) => _scan());
  }

  @override
  void didUpdateWidget(BarMenuHost old) {
    super.didUpdateWidget(old);
    if (old.entryKey != widget.entryKey) {
      _menuKey = GlobalKey();
      _submenus = const [];
    }
    _entry.markNeedsBuild();
    WidgetsBinding.instance.addPostFrameCallback((_) => _scan());
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_key);
    _disposed = true;
    _fade.dispose();
    super.dispose();
  }

  /// Fades the menu out, then tells the bar. Called for a press outside, Esc or a choice.
  void close() {
    if (_closing || !mounted) return;
    _closing = true;
    _fade.reverse().whenCompleteOrCancel(() {
      // A fade cut short by the menu leaving the tree must not report to a bar that is gone.
      if (!_disposed) widget.onDismissed?.call();
    });
  }

  /// A press at `p` (in the surface): outside the menu and its submenu it closes them.
  void pointerDown(Offset p) {
    _scan();
    if (!_rects.any((r) => r.inflate(1).contains(p))) close();
  }

  bool get closing => _closing;

  /// The menu's rectangle and its open submenus' (reported by `onSubmenusChanged`, which
  /// the menu calls when one opens, closes or moves), in the surface.
  void _scan() {
    if (!mounted) return;
    final box = _menuKey.currentContext?.findRenderObject();
    final rects = <Rect>[
      if (box is RenderBox && box.attached && box.hasSize)
        box.localToGlobal(Offset.zero) & box.size,
      ..._submenus,
    ];
    if (listEquals(rects, _rects)) return;
    _rects = rects;
    if (rects.isNotEmpty) widget.onRectsChanged?.call(rects);
  }

  /// Whether the highlighted row of the menu opens a submenu.
  bool _highlightOpensSubmenu() {
    final root = _overlayKey.currentContext;
    if (root is! Element) return false;
    var opens = false;
    void visit(Element e) {
      final w = e.widget;
      if (w is AmalfiMenuItem && w.highlighted && w.submenu) opens = true;
      e.visitChildren(visit);
    }

    root.visitChildren(visit);
    return opens;
  }

  /// ← and →: the neighbouring title, unless a submenu is open or the highlighted row opens
  /// one (the menu takes the key then).
  bool _key(KeyEvent event) {
    if (_closing || event is KeyUpEvent) return false;
    final left = event.logicalKey == LogicalKeyboardKey.arrowLeft;
    final right = event.logicalKey == LogicalKeyboardKey.arrowRight;
    if (!left && !right) return false;
    final handler = left ? widget.onPrevious : widget.onNext;
    if (handler == null) return false;
    _scan();
    if (_rects.length > 1) return false;
    if (right && _highlightOpensSubmenu()) return false;
    handler();
    return true;
  }

  Widget _buildMenu(BuildContext context) {
    final control = widget.control;
    if (control == null) {
      return CustomSingleChildLayout(
        delegate: _MenuLayout(widget.anchor, flip: widget.flip),
        child: KeyedSubtree(key: _menuKey, child: _menu()),
      );
    }
    return LayoutBuilder(
      builder: (context, box) {
        final place = control.place(box.biggest, 0);
        return CustomSingleChildLayout(
          delegate: _ControlMenuLayout(control),
          child: KeyedSubtree(
            key: _menuKey,
            child: _menu(control: control, place: place),
          ),
        );
      },
    );
  }

  Widget _menu({PopupMenuLayout? control, AmalfiMenuPlacement? place}) =>
      AmalfiMenu(
        entries: widget.entries,
        autofocus: true,
        highlightFirst: widget.highlightFirst,
        onDismiss: close,
        minWidth: control?.minWidth ?? AmalfiMenuMetrics.minWidth,
        padding: control == null
            ? AmalfiMenuMetrics.padding
            : AmalfiPopUpMenuMetrics.padding,
        itemHeight: control?.rowHeight ?? AmalfiMenuMetrics.itemHeight,
        chooseDelay: control == null
            ? AmalfiMenuMetrics.chooseClose
            : AmalfiPopUpMenuMetrics.chooseFadeDelay,
        initialScrollOffset: place?.scroll ?? 0,
        scrollArrows: control != null,
        onSubmenusChanged: (rects) {
          _submenus = rects;
          _scan();
        },
      );

  @override
  Widget build(BuildContext context) => FadeTransition(
    opacity: _opacity,
    // The glass is Ossido's: under `AmalfiGlassBackendScope(compositor)` each AmalfiMenu (and
    // its submenus, in a group above) declares its own region and paints only its rows.
    child: IgnorePointer(
      ignoring: _closing,
      child: Overlay(key: _overlayKey, initialEntries: [_entry]),
    ),
  );
}
