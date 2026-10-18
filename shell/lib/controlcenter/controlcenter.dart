// The Control Center on the Amalfi kit (`Control Center/*`, docs/design/amalfi), with the
// behaviour measured in docs/spec/controlcenter.md.
//
// The panel is a transparent 292 × 484 container anchored at the top right; each module is a
// Glass/Thick tile (toggle modules are capsules, radius = height / 2): two 140 pt columns
// 12 pt apart, 64 pt rows at a 76 pitch, the media module 140 × 140 (radius 36), Display and
// Sound 292 × 64 (radius 28) with an Amalfi slider (and its lens) inside. A module opens into a
// 312 pt wide Thick detail panel (radius 16, `detail.dart`). The glass is drawn by Ossido
// (profile `module`); the content uses the Amalfi tokens through `CcPalette`.
//
// Which modules, in which order and size: `modules.dart` (the default arrangement, or the
// user's from Edit Controls). Modules with a real effect: Wi-Fi, Bluetooth, Focus, Dark Mode,
// Screen Capture, Display, Sound (from silica-daemon and Ossido), Now playing (MPRIS), the
// battery if there is one. Nearby sharing, window staging and screen mirroring are in the
// grid but Silica has no backend for them yet: they show disabled, saying so (#140 and the
// stage view and display issues). Click on the icon disc: on or off; click on the text (or on
// the slider title): the detail panel, where there is one; with the pointer over a module with
// a detail the ">" arrow appears. Esc from the detail goes back to the Center.
//
// Motion (motion.md): the panel opens with `fade.instant` and its height grows with
// `spring.panel` from the top edge; a module turns into its detail with `spring.morph`.
//
// The component keeps running on a full-screen overlay layer: when closed it draws nothing
// and takes neither pointer nor keyboard (silica/layer).

import 'dart:async';
import 'dart:io' show File;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:silica_ui/silica_ui.dart';

import '../l10n/l10n.dart';
import '../menubar/menubar.dart' show barHeight;
import '../menubar/menubar_config.dart';
import '../system/glass_profile.dart';
import '../system/layer_control.dart';
import '../system/media.dart';
import '../system/shadow_image.dart';
import '../system/shell_channel.dart';
import '../system/desktop_entries.dart';
import '../system/system_status.dart';
import 'detail.dart';
import 'glyphs.dart';
import 'edit.dart';
import 'hud.dart';
import 'modules.dart';
import 'palette.dart';

const double stackWidth = 292;
const double columnWidth = 140;
const double gap = 12;
const double rightMargin = 14;
const double stackTop = 42;

/// The privacy pill: 24 pt tall at y 40, 6 pt above the first module, which moves down
/// 28 pt (Wi-Fi at y 70).
const double pillTop = 40;
const double pillHeight = 24;
const double pillShift = 28;
const double rowHeight = 64;
const double rowStep = rowHeight + gap;

/// Radii of the kit's modules: capsules are height / 2, the media module 36, Display and Sound 28.
const double capsuleRadius = 32;
const double mediaRadius = 36;
const double sliderRadius = 28;

/// Toggle module: a 36 pt disc at (16, 14) with a 24 pt glyph, the text from x 60.
const double disc = 36;
const double discInset = 16;
const double discTop = 14;
const double discGlyph = 24;
const double textLeft = 60;

/// The Wi-Fi status menu: 1 pt below the bar, right edge 9 pt beyond the icon.
const double statusMenuTop = 35;
const double statusMenuRight = 9;

/// Sliders (`Module/Slider/*`): the track starts at x 40, 220 pt long (172 with the outputs
/// disc), centred at y 44; the outputs disc is 28 pt at (252, 20).
const double trackLeft = 40;
const double trackLength = 220;
const double trackCenter = 44;
const double outputsShift = 48;
const double outputsLeft = 252;
const double outputsTop = 20;
const double outputsSide = 28;

/// The stack's shadow is `shadows.3` of Glass/Thick: y 12, blur 32 (a Gaussian of σ 16).
const double shadowOffsetY = 12;
const double shadowSigma = 16;

/// Opacity while opening and closing (ms from the first changed frame; the first point is
/// one frame earlier). controlcenter.md, "Animations".
const List<(double, double)> openOpacity = [
  (0, 0),
  (13, 0.09),
  (27, 0.16),
  (40, 0.25),
  (67, 0.44),
  (80, 0.53),
  (93, 0.64),
  (120, 0.74),
  (133, 0.80),
  (147, 0.84),
  (173, 0.91),
  (187, 0.92),
  (200, 0.94),
  (227, 0.97),
  (240, 0.98),
  (280, 0.99),
  (310, 1),
];

const List<(double, double)> closeOpacity = [
  (0, 1),
  (13, 0.92),
  (27, 0.86),
  (53, 0.61),
  (67, 0.49),
  (80, 0.38),
  (107, 0.24),
  (120, 0.19),
  (133, 0.14),
  (160, 0.08),
  (173, 0.07),
  (187, 0.05),
  (213, 0.02),
  (253, 0.01),
  (293, 0),
];

/// Content scale: 1.0075 → 1 in ~107 ms when opening, 1 → 1.0075 in ~120 ms when closing,
/// around the center of the stack.
const double scaleAway = 1.0075;
const double openScaleMs = 107;
const double closeScaleMs = 120;
const Duration openDuration = Duration(milliseconds: 310);
const Duration closeDuration = Duration(milliseconds: 293);

double tableAt(List<(double, double)> table, double ms) {
  if (ms <= table.first.$1) return table.first.$2;
  for (var i = 1; i < table.length; i++) {
    final (t1, a1) = table[i];
    if (ms <= t1) {
      final (t0, a0) = table[i - 1];
      return a0 + (a1 - a0) * (ms - t0) / (t1 - t0);
    }
  }
  return table.last.$2;
}

/// Opacity of the panel [ms] after it started opening (measured table, controlcenter.md).
double openOpacityAt(double ms) => tableAt(openOpacity, ms);

/// Opacity of the panel [ms] after it started closing.
double closeOpacityAt(double ms) => tableAt(closeOpacity, ms);

/// Content scale [ms] after the panel started opening (1.0075 -> 1 in ~107 ms) or closing
/// (1 -> 1.0075 in ~120 ms), around the centre of the stack. Measured; Amalfi changes only the
/// geometry, and a scale factor is the same at any size.
double panelScaleAt(double ms, {required bool opening}) => opening
    ? 1 + (scaleAway - 1) * (1 - (ms / openScaleMs).clamp(0.0, 1.0))
    : 1 + (scaleAway - 1) * (ms / closeScaleMs).clamp(0.0, 1.0);

/// A module: rectangle relative to the stack and radius.
@immutable
class ModuleSlot {
  const ModuleSlot(this.rect, this.radius);
  final Rect rect;
  final double radius;

  @override
  bool operator ==(Object other) =>
      other is ModuleSlot && other.rect == rect && other.radius == radius;

  @override
  int get hashCode => Object.hash(rect, radius);
}

/// The layout of the present modules, from the top (coordinates relative to the stack):
/// [modules] packed into the grid (modules.dart), each with its shape's radius.
Map<String, ModuleSlot> layoutModules(
  SystemSnapshot s, [
  List<CcModule> modules = defaultModules,
]) {
  final present = presentModules(modules, s);
  final frames = packModules(present);
  return {
    for (final m in present)
      if (frames[m.id] case final rect?)
        m.id: ModuleSlot(rect, switch (m.size) {
          ModuleSize.small => rowHeight / 2,
          ModuleSize.medium => capsuleRadius,
          ModuleSize.large => mediaRadius,
          ModuleSize.wide => sliderRadius,
        }),
  };
}

class ControlCenter extends StatefulWidget {
  const ControlCenter({
    super.key,
    required this.system,
    this.startOpen = false,
    this.env,
    this.settingsApp,
  });

  final SystemSource system;
  final bool startOpen;

  /// The environment for the arrangement file (tests); null: the process environment.
  final Map<String, String>? env;

  /// The Settings app (tests); null: looked up among the installed apps.
  final DesktopEntry? settingsApp;

  @override
  State<ControlCenter> createState() => _ControlCenterState();
}

/// The modules with a detail panel.
const Set<String> detailModules = {
  'wifi',
  'media',
  'bluetooth',
  'dnd',
  'sound',
  'display',
};

/// Room around the stack for the detail panel, which sticks out to the left and up.
const Offset detailPad = Offset(-detailLeft, -detailTop);

class _ControlCenterState extends State<ControlCenter>
    with TickerProviderStateMixin {
  /// This component's layer surface (several share one engine, #201).
  late final LayerControl _surface;

  late final AnimationController _anim;

  /// The open detail (or opening or closing) and its transformation (value = ms).
  String? _detail;
  bool _detailClosing = false;
  late final AnimationController _detailAnim = AnimationController(
    vsync: this,
    upperBound: detailTotalMs,
    duration: Duration(milliseconds: detailTotalMs.round()),
  )..addListener(_detailTick);
  DetailData _detailData = const DetailData();
  bool _othersOpen = false;

  /// Do Not Disturb "for an hour" or "until this evening" (provisional: the timer lives in
  /// the shell and does not survive a restart).
  Timer? _dndTimer;
  final FocusNode _focus = FocusNode();
  StreamSubscription<String>? _commands;
  bool _open = false;

  /// The arrangement: the user's, or the reference's default.
  late List<CcModule> _modules = loadModules();
  late final Listenable _modulesKey = SilicaSettings.instance.watch(const [
    modulesKey,
  ]);

  void _modulesChanged() {
    if (mounted) setState(() => _modules = loadModules());
  }

  @override
  void initState() {
    super.initState();
    _surface = LayerControl.of(context);
    _anim = AnimationController(vsync: this)
      ..addListener(() => setState(() {}));
    widget.system.addListener(_changed);
    _modulesKey.addListener(_modulesChanged);
    _commands = ShellChannel.instance.commands.listen((id) {
      if (id == 'controlcenter.toggle') _toggle();
    });
    _dismissed = _surface.transientDismissed.listen(
      (_) => _transientDismissed(),
    );
    ShellChannel.instance.states.addListener(_statesChanged);
    if (widget.startOpen) {
      _show();
    } else {
      _surface.inputRegion(const []);
      _surface.keyboard('none');
    }
  }

  @override
  void dispose() {
    widget.system.removeListener(_changed);
    _modulesKey.removeListener(_modulesChanged);
    ShellChannel.instance.states.removeListener(_statesChanged);
    _commands?.cancel();
    _dismissed?.cancel();
    _anim.dispose();
    _dndTimer?.cancel();
    _detailAnim.dispose();
    _editAnim.dispose();
    _focus.dispose();
    _menuFocus.dispose();
    super.dispose();
  }

  void _changed() => setState(() {});

  // A press on the bar's icon while the Center is open: Ossido first ends the panel
  // (transientDismissed, the press goes through), then the bar asks for the toggle, which
  // must not open it again.
  void _toggle() {
    if (!_open && _dismissedAt != null) {
      final since = DateTime.now().difference(_dismissedAt!);
      if (since < const Duration(milliseconds: 400)) return;
    }
    _open ? _hide() : _show();
  }

  StreamSubscription<void>? _dismissed;
  DateTime? _dismissedAt;

  /// A press outside the open Center or status menu ended it (Ossido, #149).
  void _transientDismissed() {
    if (_wifiMenuRight != null) _closeWifiMenu();
    if (_open) {
      _dismissedAt = DateTime.now();
      _hide();
    }
  }

  /// The visible panel or status menu: the input region and the transient follow it, so
  /// the windows below keep the pointer (#149).
  Rect? _region;

  void _setRegion(Rect rect, TransientPolicy policy) {
    if (rect == _region) return;
    _region = rect;
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      if (!mounted || (!_open && _wifiMenuRight == null)) return;
      final supported = await _surface.transient(rects: [rect], policy: policy);
      if (!mounted || (!_open && _wifiMenuRight == null)) return;
      _surface.inputRegion(supported ? [rect] : null);
    });
  }

  /// The status menu opened from the bar (Wi-Fi, Focus): the right edge of the icon.
  double? _wifiMenuRight;
  String? _statusMenuKey;
  final FocusNode _menuFocus = FocusNode();

  /// The panels the bar's status items open.
  static const Set<String> statusMenus = {
    'wifi',
    'dnd',
    'media',
    'bluetooth',
    'sound',
  };

  /// The bar announces `statusmenu` = "<panel> <right edge of the icon>" or "".
  void _statesChanged() {
    final value = ShellChannel.instance.states.value['statusmenu'] ?? '';
    final parts = value.split(' ');
    final key = parts.length == 2 && statusMenus.contains(parts[0])
        ? parts[0]
        : null;
    final right = key == null ? null : double.tryParse(parts[1]);
    if (right == _wifiMenuRight && key == _statusMenuKey) return;
    if (key == null || right == null) {
      _closeWifiMenu(announce: false);
    } else {
      _openWifiMenu(key, right);
    }
  }

  Future<void> _openWifiMenu(String key, double right) async {
    if (_open) _hide();
    setState(() {
      _wifiMenuRight = right;
      _statusMenuKey = key;
      _detail = key;
      _detailClosing = false;
      _othersOpen = false;
      _detailData = const DetailData();
    });
    _region = null;
    _surface.inputRegion(const []);
    _surface.keyboard('exclusive');
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => _menuFocus.requestFocus(),
    );
    final system = widget.system;
    final data = switch (key) {
      'wifi' => DetailData(networks: await system.wifiNetworks()),
      'sound' => DetailData(outputs: await system.audioOutputs()),
      'bluetooth' => DetailData(devices: await system.bluetoothDevices()),
      _ => null,
    };
    if (data != null &&
        mounted &&
        _wifiMenuRight != null &&
        _statusMenuKey == key) {
      setState(() => _detailData = data);
    }
  }

  void _closeWifiMenu({bool announce = true}) {
    if (_wifiMenuRight == null) return;
    _statusMenuKey = null;
    setState(() {
      _wifiMenuRight = null;
      _detail = null;
    });
    if (announce) ShellChannel.instance.broadcast('statusmenu', '');
    if (!_open) {
      _region = null;
      _surface.clearTransient();
      _surface.keyboard('none');
      _surface.inputRegion(const []);
    }
  }

  /// The Wi-Fi panel as a status menu (menubar.md): 308 pt, 1 pt below the bar (y 31), the
  /// right edge 9 pt beyond the icon's; it opens in one frame.
  Widget _wifiMenu(SystemSnapshot s, double right) {
    final content = _detailContent(s);
    if (content == null) return const SizedBox.shrink();
    final rect = Rect.fromLTWH(
      right + statusMenuRight - detailWidth,
      statusMenuTop,
      detailWidth,
      content.height,
    );
    // A status menu is a system panel (INPUT-01 (f)): clicks, drags and the wheel outside
    // pass; the keys stay here.
    _setRegion(rect.inflate(2), TransientPolicy.panel);
    return Focus(
      focusNode: _menuFocus,
      onKeyEvent: (node, event) {
        if (event is KeyDownEvent &&
            event.logicalKey == LogicalKeyboardKey.escape) {
          _closeWifiMenu();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Stack(
        children: [
          Positioned.fill(
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: _closeWifiMenu,
            ),
          ),
          Positioned.fromRect(
            rect: rect,
            child: DecoratedBox(
              key: ValueKey('${_statusMenuKey ?? ''} status menu'),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(detailRadius),
                boxShadow: [
                  BoxShadow(
                    color: AmalfiTheme.of(context).colors.shadow3,
                    blurRadius: 28,
                    offset: const Offset(0, shadowOffsetY),
                  ),
                ],
              ),
              child: ProfiledGlass(
                profile: ShellGlassProfile.panel,
                radius: detailRadius,
                material: GlassMaterial.thick,
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(detailRadius),
                  child: content.build(),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  void _show() {
    _closeWifiMenu();
    setState(() => _open = true);
    ShellChannel.instance.broadcast('controlcenter.open', '1');
    _region = null;
    _surface.inputRegion(const []);
    _surface.keyboard('exclusive');
    _anim.duration = openDuration;
    _anim.forward(from: 0);
    WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  void _detailTick() {
    // Once the return is over, the detail disappears.
    if (_detailClosing && _detailAnim.isCompleted) {
      _detail = null;
      _detailClosing = false;
    }
    setState(() {});
  }

  Future<void> _openDetail(String key) async {
    setState(() {
      _detail = key;
      _detailClosing = false;
      _othersOpen = false;
      _detailData = const DetailData();
    });
    _detailAnim.forward(from: 0);
    final system = widget.system;
    final data = switch (key) {
      'wifi' => DetailData(networks: await system.wifiNetworks()),
      'sound' => DetailData(outputs: await system.audioOutputs()),
      'bluetooth' => DetailData(devices: await system.bluetoothDevices()),
      _ => const DetailData(),
    };
    if (mounted && _detail == key) setState(() => _detailData = data);
  }

  void _closeDetail() {
    if (_detail == null || _detailClosing) return;
    setState(() => _detailClosing = true);
    _detailAnim.forward(from: 0);
  }

  /// How far the Display module's top is from the panel's: the reference of the morph's time.
  static final double _displayDistance =
      layoutModules(const SystemSnapshot())['display']!.rect.top - detailTop;

  /// How much of module `_detail` has become a panel (0–1).
  double _detailProgress() {
    if (_detail == null) return 0;
    final slot = layoutModules(widget.system.value, _modules)[_detail];
    final scale = detailTimeScale(
      slot == null ? 0 : slot.rect.top - detailTop,
      _displayDistance,
    );
    return _detailClosing
        ? 1 - detailProgress(_detailAnim.value / detailBackStretch, scale)
        : detailProgress(_detailAnim.value, scale);
  }

  void _hide() {
    _detail = null;
    _detailClosing = false;
    _editing = false;
    _moduleMenu = null;
    setState(() => _open = false);
    ShellChannel.instance.broadcast('controlcenter.open', '0');
    _region = null;
    _surface.clearTransient();
    _surface.keyboard('none');
    _surface.inputRegion(const []);
    _anim.duration = closeDuration;
    _anim.forward(from: 0);
  }

  @override
  Widget build(BuildContext context) {
    // Everything in one overlay entry: dragging controls in edit mode needs an Overlay
    // (the shell's components have none), and the entry follows every rebuild.
    _layer.markNeedsBuild();
    return Overlay(initialEntries: [_layer]);
  }

  late final OverlayEntry _layer = OverlayEntry(
    builder: (context) => DefaultTextStyle.merge(
      style: CcPalette.of(context).textStyle,
      child: Stack(
        children: [
          Positioned.fill(child: _center(context)),
          if (_wifiMenuRight case final right?)
            Positioned.fill(child: _wifiMenu(widget.system.value, right)),
          // The volume and brightness HUD: not with the Center open.
          Positioned.fill(
            child: IgnorePointer(
              child: SystemHud(system: widget.system, suppressed: _open),
            ),
          ),
        ],
      ),
    ),
  );

  Widget _center(BuildContext context) {
    final ms =
        _anim.value * (_open ? openDuration : closeDuration).inMilliseconds;
    final opacity = _open ? openOpacityAt(ms) : closeOpacityAt(ms);
    if (!_open && (opacity <= 0 || !_anim.isAnimating)) {
      return const SizedBox.expand();
    }
    final scale = panelScaleAt(ms, opening: _open);
    final snapshot = widget.system.value;
    final slots = layoutModules(snapshot, _modules);
    final height = slots.values
        .map((s) => s.rect.bottom)
        .reduce((a, b) => a > b ? a : b);
    // The detail: rectangle and radius between the module and the panel, and the content.
    final progress = _detailProgress();
    final opening = !_detailClosing;
    final othersAlpha = _detail == null
        ? 1.0
        : opening
        ? othersOpacity(_detailAnim.value)
        : (1 - progress).clamp(0.0, 1.0);
    final content = _detailContent(snapshot);
    final from = slots[_detail];
    final (Rect, double, DetailContent)? detail =
        content == null || from == null
        ? null
        : (
            lerpRect(
              _glassRect(from.rect),
              Rect.fromLTWH(detailLeft, detailTop, detailWidth, content.height),
              progress,
            ),
            from.radius + (detailRadius - from.radius) * progress,
            content,
          );
    final totalHeight = detail == null
        ? height
        : (detail.$1.bottom > height ? detail.$1.bottom : height);
    final shadowSlots = detail != null && progress > 0.5
        ? [ModuleSlot(detail.$1, detail.$2)]
        : slots.values.toList();

    return LayoutBuilder(
      builder: (context, constraints) {
        final left = constraints.maxWidth - rightMargin - stackWidth;
        final top = stackTop + (snapshot.privacy.isEmpty ? 0 : pillShift);
        final origin = Offset(left, top);
        final shadow = _StackShadow(
          origin: origin,
          height: totalHeight,
          slots: shadowSlots,
          color: AmalfiTheme.of(context).colors.shadow3,
        );
        if (_open) {
          _setRegion(
            Rect.fromLTWH(
              left - detailPad.dx,
              top - detailPad.dy,
              detailWidth,
              totalHeight + detailPad.dy,
            ),
            TransientPolicy.panel,
          );
        }
        return Focus(
          focusNode: _focus,
          onKeyEvent: (node, event) {
            if (event is KeyDownEvent &&
                event.logicalKey == LogicalKeyboardKey.escape) {
              // From the detail back to the Center; from the Center (or edit mode, gallery
              // included) it closes.
              _detail != null && !_detailClosing && !_editing
                  ? _closeDetail()
                  : _hide();
              return KeyEventResult.handled;
            }
            return KeyEventResult.ignored;
          },
          child: Stack(
            children: [
              Positioned.fill(
                child: GestureDetector(
                  onTap: _open ? _hide : null,
                  behavior: HitTestBehavior.opaque,
                  // Edit mode dims the screen behind (~12 levels).
                  child: _editing
                      ? Opacity(
                          opacity: opacity,
                          child: const ColoredBox(color: editDim),
                        )
                      : null,
                ),
              ),
              // The shadow is an image (CachedShadow): drawn once per layout, it fades with the
              // panel under Impeller, which cannot fade a blurred draw under a clip.
              Positioned.fill(
                child: RepaintBoundary(
                  child: CachedShadow(
                    cacheKey: shadow.cacheKey,
                    paintShadow: shadow.paintShadow,
                    opacity: opacity,
                    hittable: false,
                  ),
                ),
              ),
              if (snapshot.privacy.firstOrNull case final use?)
                Positioned(
                  left: left,
                  width: stackWidth,
                  top: pillTop,
                  height: pillHeight,
                  child: Opacity(
                    opacity: opacity * othersAlpha,
                    // As tall as the band, as wide as its content, centred.
                    child: Center(
                      child: SizedBox(
                        height: pillHeight,
                        child: _PrivacyPill(
                          use: use,
                          onTap: _settingsAt('privacy'),
                        ),
                      ),
                    ),
                  ),
                ),
              Positioned(
                left: left - detailPad.dx,
                top: top - detailPad.dy,
                width: detailWidth,
                height: totalHeight + detailPad.dy + _belowStack,
                child: Transform.scale(
                  scale: scale,
                  child: Opacity(
                    opacity: opacity,
                    child: Stack(
                      clipBehavior: Clip.none,
                      children: [
                        for (final MapEntry(:key, :value) in slots.entries)
                          if (key != _detail)
                            Positioned.fromRect(
                              rect: _glassRect(value.rect).shift(detailPad),
                              child: Opacity(
                                opacity: othersAlpha,
                                child: ProfiledGlass(
                                  profile: ShellGlassProfile.module,
                                  radius: value.radius,
                                  material: GlassMaterial.thick,
                                  child: const SizedBox.expand(),
                                ),
                              ),
                            ),
                        if (detail case (final rect, final radius, _))
                          Positioned.fromRect(
                            rect: rect.shift(detailPad),
                            child: ProfiledGlass(
                              profile: ShellGlassProfile.module,
                              radius: radius,
                              material: GlassMaterial.thick,
                              child: const SizedBox.expand(),
                            ),
                          ),
                        for (final MapEntry(:key, :value) in slots.entries)
                          Positioned.fromRect(
                            rect: value.rect.shift(detailPad),
                            child: Opacity(
                              opacity: key == _detail
                                  ? (1 - progress * 3).clamp(0.0, 1.0)
                                  : othersAlpha,
                              child: IgnorePointer(
                                ignoring: _detail != null,
                                child: _arranged(key, value, snapshot),
                              ),
                            ),
                          ),
                        // Edit Controls: centred 16 pt under the last module; in edit mode the
                        // remove badges and the eight empty slots instead.
                        if (_detail == null && !_editing)
                          Positioned(
                            left:
                                (stackWidth - editButton.width) / 2 +
                                detailPad.dx,
                            top: height + editButtonGap + detailPad.dy,
                            width: editButton.width,
                            height: editButton.height,
                            child: _EditButton(
                              label: l10nOf(context).ccEditControls,
                              onTap: _startEditing,
                            ),
                          ),
                        if (_editing) ...[
                          for (final MapEntry(:key, :value) in slots.entries)
                            Positioned(
                              left:
                                  value.rect.left - badgeOutside + detailPad.dx,
                              top: value.rect.top - badgeOutside + detailPad.dy,
                              width: badgeSide,
                              height: badgeSide,
                              child: RemoveBadge(
                                label:
                                    '${l10nOf(context).ccRemove} ${_label(key)}',
                                onTap: () => _removeModule(key),
                              ),
                            ),
                          for (var i = 0; i < emptySlots; i++)
                            Positioned(
                              left: (i % 4) * pitch + detailPad.dx,
                              top:
                                  height +
                                  gap +
                                  (i ~/ 4) * pitch +
                                  detailPad.dy,
                              width: cell,
                              height: cell,
                              child: Opacity(
                                opacity: slotsOpacity(
                                  _editAnim.value * editTotalMs,
                                ),
                                child: EmptySlot(onDrop: _placeModule),
                              ),
                            ),
                        ],
                        if (detail case (
                          final rect,
                          final radius,
                          final content,
                        ))
                          Positioned.fromRect(
                            rect: rect.shift(detailPad),
                            child: Opacity(
                              opacity: ((progress - 0.25) / 0.4).clamp(
                                0.0,
                                1.0,
                              ),
                              child: ClipRRect(
                                borderRadius: BorderRadius.circular(radius),
                                child: OverflowBox(
                                  alignment: Alignment.topLeft,
                                  minWidth: detailWidth,
                                  maxWidth: detailWidth,
                                  minHeight: 0,
                                  maxHeight: double.infinity,
                                  child: content.build(),
                                ),
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
              ),
              if (_editing) _gallery(constraints.biggest, opacity),
              if (_moduleMenu case (final key, final at))
                Positioned.fill(
                  child: MenuHost(
                    key: ValueKey('module menu $key $at'),
                    entries: _moduleMenuEntries(key),
                    anchor:
                        at - Offset(menuSize(_moduleMenuEntries(key)).width, 0),
                    top: barHeight,
                    onDismissed: () => setState(() => _moduleMenu = null),
                  ),
                ),
            ],
          ),
        );
      },
    );
  }

  /// Room under the stack: the Edit Controls button, or the empty slots in edit mode.
  double get _belowStack =>
      _editing ? gap + cell + pitch : editButtonGap + editButton.height;

  /// The gallery, centred on the screen (shorter screens: as tall as fits).
  Widget _gallery(Size screen, double opacity) {
    final height = gallerySize.height.clamp(0.0, screen.height - 80);
    final present = {for (final m in _modules) m.id};
    final available = {
      for (final id in moduleSizes.keys)
        if (moduleAvailable(id, widget.system.value)) id,
    };
    return Positioned(
      left: (screen.width - gallerySize.width) / 2,
      top: (screen.height - height) / 2,
      width: gallerySize.width,
      height: height,
      child: Opacity(
        opacity: opacity * galleryOpacity(_editAnim.value * editTotalMs),
        child: DecoratedBox(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(galleryRadius),
            boxShadow: const [
              BoxShadow(
                color: Color(0x40000000),
                blurRadius: 40,
                offset: Offset(0, 16),
              ),
            ],
          ),
          child: ControlGallery(
            present: present,
            available: available,
            preview: _preview,
            label: _label,
            onAdd: _placeModule,
            onDone: _stopEditing,
          ),
        ),
      ),
    );
  }

  /// A module in the stack: the secondary click opens its menu; in edit mode it can be
  /// dragged to another place, and controls dropped on it go before it.
  Widget _arranged(String key, ModuleSlot slot, SystemSnapshot s) {
    final module = GestureDetector(
      onSecondaryTapUp: (d) =>
          setState(() => _moduleMenu = (key, d.globalPosition)),
      child: _module(key, s),
    );
    if (!_editing) return _ModuleHover(radius: slot.radius, child: module);
    return DragTarget<String>(
      onAcceptWithDetails: (d) => _placeModule(d.data, before: key),
      builder: (context, candidates, _) => Draggable<String>(
        data: key,
        feedback: Opacity(
          opacity: 0.85,
          child: SizedBox.fromSize(
            size: slot.rect.size,
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: const Color(0xFF6A6B70),
                borderRadius: BorderRadius.circular(slot.radius),
              ),
              child: IgnorePointer(child: _module(key, s)),
            ),
          ),
        ),
        childWhenDragging: const SizedBox.expand(),
        // In edit mode the modules don't act: they are dragged.
        child: Opacity(
          opacity: candidates.isEmpty ? 1 : 0.6,
          child: IgnorePointer(child: module),
        ),
      ),
    );
  }

  /// The glass of a module: exactly its box (the kit's masters have no overhang).
  Rect _glassRect(Rect r) => r;

  /// The Settings app, if installed (its first argument names the pane).
  late final DesktopEntry? _settingsApp =
      widget.settingsApp ??
      loadDesktopEntries(locale: systemLanguage()).values
          .where((e) => e.id.toLowerCase().endsWith('.settings'))
          .firstOrNull;

  /// Opens Settings at [pane] and closes the Center or the status menu; null without the
  /// app (the footer item is not shown then).
  VoidCallback? _settingsAt(String pane) {
    final app = _settingsApp;
    if (app == null) return null;
    return () {
      _closeWifiMenu();
      if (_open) _hide();
      ShellChannel.instance.launch('${app.exec} --dart-arg $pane');
    };
  }

  /// The content of the open detail.
  DetailContent? _detailContent(SystemSnapshot s) {
    final system = widget.system;
    final l = l10nOf(context);
    return switch (_detail) {
      'wifi' => wifiDetail(
        l: l,
        s: s,
        networks: _detailData.networks,
        othersOpen: _othersOpen,
        onSwitch: system.setWifi,
        onConnect: system.connectWifi,
        onToggleOthers: () => setState(() => _othersOpen = !_othersOpen),
        onSettings: _settingsAt('wifi'),
      ),
      'sound' => soundDetail(
        l: l,
        s: s,
        outputs: _detailData.outputs,
        onVolume: (v) {
          if (s.muted && v > 0) system.setMuted(false);
          system.setVolume(v);
        },
        onOutput: (id) async {
          await system.setAudioOutput(id);
          final outputs = await system.audioOutputs();
          if (mounted)
            setState(() => _detailData = DetailData(outputs: outputs));
        },
        onSettings: _settingsAt('sound'),
      ),
      'display' => displayDetail(
        l: l,
        s: s,
        onBrightness: system.setBrightness,
        dark: MediaQuery.maybePlatformBrightnessOf(context) == Brightness.dark,
        onDark: () => ShellChannel.instance.execute('system.dark_toggle'),
        onNightLight: () => system.setNightLight(!s.nightLight),
        onSettings: _settingsAt('displays'),
      ),
      'bluetooth' => bluetoothDetail(
        l: l,
        s: s,
        devices: _detailData.devices,
        onSwitch: system.setBluetooth,
        onDevice: (d) async {
          await system.connectBluetooth(d.address, !d.connected);
          final devices = await system.bluetoothDevices();
          if (mounted && _detail == 'bluetooth') {
            setState(() => _detailData = DetailData(devices: devices));
          }
        },
        onSettings: _settingsAt('bluetooth'),
      ),
      'media' => DetailContent([(nowPlayingHeight, const _NowPlayingDetail())]),
      'dnd' => focusDetail(
        l: l,
        s: s,
        onDoNotDisturb: (on) {
          _dndTimer?.cancel();
          system.setDoNotDisturb(on);
        },
        onFor: (duration) {
          _dndTimer?.cancel();
          _dndTimer = Timer(duration ?? untilEvening(DateTime.now()), () {
            system.setDoNotDisturb(false);
          });
        },
        onSettings: _settingsAt('focus'),
      ),
      _ => null,
    };
  }

  /// While building a gallery preview: the size to draw it at.
  ModuleSize? _sizeOverride;

  /// Module [key] at [size], for the gallery.
  Widget _preview(String key, ModuleSize size) {
    _sizeOverride = size;
    try {
      return _module(key, widget.system.value);
    } finally {
      _sizeOverride = null;
    }
  }

  /// The control's name, for the gallery and the badges.
  String _label(String key) {
    final l = l10nOf(context);
    return switch (key) {
      'wifi' => 'Wi-Fi',
      'bluetooth' => 'Bluetooth',
      'nearby' => l.ccNearbySharing,
      'media' => l.ccNowPlaying,
      'dnd' => l.ccFocus,
      'dark' => l.ccDarkMode,
      'capture' => l.ccScreenCapture,
      'staging' => l.ccStageView,
      'mirroring' => l.ccScreenMirroring,
      'battery' => l.ccBattery,
      'keyboard' => l.ccKeyboardBrightness,
      'display' => l.ccDisplay,
      _ => l.ccSound,
    };
  }

  // Edit Controls: the arrangement changes and is saved at once.

  void _setModules(List<CcModule> modules) {
    setState(() => _modules = modules);
    saveModules(modules);
  }

  void _removeModule(String key) => _setModules([
    for (final m in _modules)
      if (m.id != key) m,
  ]);

  /// Adds [key] (from the gallery) or moves it (in the Center) before [before], or at the
  /// end.
  void _placeModule(String key, {String? before}) {
    if (key == before) return;
    final existing = _modules.where((m) => m.id == key).firstOrNull;
    final module =
        existing ?? CcModule(key, moduleSizes[key]?.first ?? ModuleSize.small);
    final rest = [
      for (final m in _modules)
        if (m.id != key) m,
    ];
    final at = before == null
        ? rest.length
        : rest.indexWhere((m) => m.id == before);
    rest.insert(at < 0 ? rest.length : at, module);
    _setModules(rest);
  }

  void _resizeModule(String key, ModuleSize size) => _setModules([
    for (final m in _modules) m.id == key ? m.withSize(size) : m,
  ]);

  bool _editing = false;
  late final AnimationController _editAnim = AnimationController(
    vsync: this,
    duration: Duration(milliseconds: editTotalMs.round()),
  )..addListener(() => setState(() {}));

  void _startEditing() {
    _closeDetail();
    setState(() => _editing = true);
    _editAnim.forward(from: 0);
  }

  void _stopEditing() {
    if (!_editing) return;
    setState(() => _editing = false);
    _editAnim.value = 0;
  }

  /// The module menu (secondary click): its top-right corner on the pointer.
  (String, Offset)? _moduleMenu;

  List<MenuEntry> _moduleMenuEntries(String key) {
    final l = l10nOf(context);
    final sizes = moduleSizes[key] ?? const <ModuleSize>[];
    final current = _sizeOf(key);
    MenuAction size(String label, ModuleSize s) => MenuAction(
      label,
      checked: current == s,
      enabled: sizes.contains(s) && sizes.length > 1,
      onSelected: () => _resizeModule(key, s),
    );
    return [
      MenuSection(l.ccSize),
      size(l.ccSizeSmall, ModuleSize.small),
      size(l.ccSizeMedium, ModuleSize.medium),
      size(l.ccSizeLarge, ModuleSize.large),
      const MenuDivider(),
      // Only the controls with a status item can go to the bar; the rest stay disabled.
      if (menubarShows(MenubarConfig.of(SilicaSettings.instance), key))
        MenuAction(
          l.ccRemoveFromMenuBar,
          onSelected: () => setMenubarModule(key, false),
        )
      else
        MenuAction(
          l.ccCopyToMenuBar,
          enabled: menubarModules.contains(key),
          onSelected: () => setMenubarModule(key, true),
        ),
      const MenuDivider(),
      MenuAction(l.ccRemove, onSelected: () => _removeModule(key)),
    ];
  }

  /// The size of module [key] in the arrangement.
  ModuleSize _sizeOf(String key) =>
      _modules.where((m) => m.id == key).firstOrNull?.size ?? ModuleSize.small;

  /// A toggle without a detail: a 64 pt round tile when small, a 140 × 64 capsule (disc and
  /// label) when medium. Without [onTap] it is disabled: Silica can't do it yet.
  Widget _toggleModule(
    String key, {
    required GlyphKind glyph,
    required String label,
    required bool active,
    required VoidCallback? onTap,
  }) {
    final l = l10nOf(context);
    if ((_sizeOverride ?? _sizeOf(key)) == ModuleSize.small) {
      return _RoundToggle(
        glyph: glyph,
        label: label,
        active: active,
        onTap: onTap,
      );
    }
    return _Capsule(
      glyph: glyph,
      title: label,
      subtitle: onTap == null
          ? l.ccNotAvailableYet
          : (active ? l.ccOn : l.ccOff),
      active: active,
      enabled: onTap != null,
      onTap: onTap,
    );
  }

  Widget _module(String key, SystemSnapshot s) {
    final system = widget.system;
    final l = l10nOf(context);
    return switch (key) {
      'wifi' => _Capsule(
        glyph: GlyphKind.wifi,
        title: 'Wi-Fi',
        subtitle: !s.wifiAvailable
            ? l.ccUnavailable
            : !s.wifiEnabled
            ? l.ccOff
            : s.wifiSsid.isNotEmpty
            ? s.wifiSsid
            : s.wired
            ? l.ccWired
            : l.ccNotConnected,
        active: s.wifiAvailable && s.wifiEnabled,
        onTap: s.wifiAvailable ? () => system.setWifi(!s.wifiEnabled) : null,
        onOpen: s.wifiAvailable ? () => _openDetail('wifi') : null,
      ),
      'bluetooth' => _Capsule(
        glyph: GlyphKind.bluetooth,
        title: 'Bluetooth',
        subtitle: !s.bluetoothAvailable
            ? l.ccUnavailable
            : s.bluetoothPowered
            ? l.ccOn
            : l.ccOff,
        active: s.bluetoothAvailable && s.bluetoothPowered,
        onTap: s.bluetoothAvailable
            ? () => system.setBluetooth(!s.bluetoothPowered)
            : null,
        onOpen: s.bluetoothAvailable ? () => _openDetail('bluetooth') : null,
      ),
      // Focus: off it is titled "Focus"; on, "Do Not Disturb / On" with a purple moon
      // on the white disc (SH-CC-06).
      'dnd' => _Capsule(
        glyph: GlyphKind.moon,
        title: s.doNotDisturb ? l.ccDoNotDisturb : l.ccFocus,
        subtitle: s.doNotDisturb ? l.ccDoNotDisturbOn : '',
        active: s.doNotDisturb,
        onTap: () => system.setDoNotDisturb(!s.doNotDisturb),
        onOpen: () => _openDetail('dnd'),
      ),
      'dark' => _toggleModule(
        key,
        glyph: GlyphKind.contrast,
        label: l.ccDarkMode,
        active:
            MediaQuery.maybePlatformBrightnessOf(context) == Brightness.dark,
        onTap: () => ShellChannel.instance.execute('system.dark_toggle'),
      ),
      'capture' => _toggleModule(
        key,
        glyph: GlyphKind.screenCapture,
        label: l.ccScreenCapture,
        active: false,
        onTap: () {
          _hide();
          ShellChannel.instance.execute('screenshot.toolbar');
        },
      ),
      // In the reference's grid, with no backend in Silica yet: shown disabled, saying so
      // (nearby sharing #140; window staging and screen mirroring need Ossido's stage view
      // and a second display).
      'nearby' => _Capsule(
        glyph: GlyphKind.nearby,
        title: l.ccNearby,
        subtitle: l.ccNotAvailableYet,
        active: false,
        enabled: false,
      ),
      'staging' => _toggleModule(
        key,
        glyph: GlyphKind.staging,
        label: l.ccStageView,
        active: false,
        onTap: null,
      ),
      'mirroring' => _toggleModule(
        key,
        glyph: GlyphKind.mirroring,
        label: l.ccScreenMirroring,
        active: false,
        onTap: null,
      ),
      'battery' => _Capsule(
        glyph: GlyphKind.battery,
        glyphLevel: s.batteryPercent / 100,
        title: l.ccBattery,
        subtitle: s.batteryCharging
            ? l.ccBatteryCharging(s.batteryPercent.round())
            : l.ccBatteryLevel(s.batteryPercent.round()),
        active: false,
      ),
      'media' => _Media(onOpen: () => _openDetail('media')),
      // The keyboard backlight (UPower), offered only with a backlit keyboard.
      'keyboard' => _SliderModule(
        title: l.ccKeyboardBrightness,
        value: s.keyboardBrightness,
        enabled: s.keyboardAvailable,
        low: GlyphKind.keyboard,
        high: GlyphKind.keyboard,
        onChanged: (v) => ShellChannel.instance.execute(
          'system.keyboard_brightness',
          {'value': v},
        ),
      ),
      'display' => _SliderModule(
        onOpen: () => _openDetail('display'),
        title: l.ccDisplay,
        value: s.brightness,
        enabled: s.brightnessAvailable,
        low: GlyphKind.sunSmall,
        high: GlyphKind.sunLarge,
        onChanged: system.setBrightness,
      ),
      _ => _SliderModule(
        onOpen: () => _openDetail('sound'),
        onOutputs: () => _openDetail('sound'),
        title: l.ccSound,
        value: s.muted ? 0 : s.volume,
        enabled: s.audioAvailable,
        low: GlyphKind.speakerSmall,
        high: GlyphKind.speakerLarge,
        onChanged: (v) {
          if (s.muted && v > 0) system.setMuted(false);
          system.setVolume(v);
        },
      ),
    };
  }
}

/// A toggle module (`Module/Toggle/*`): 140 × 64 capsule, a 36 pt disc at (16, 14) with a
/// 20 pt glyph, title (Headline) and subtitle (Caption) from x 60, vertically centred.
class _Capsule extends StatelessWidget {
  const _Capsule({
    required this.glyph,
    required this.title,
    required this.subtitle,
    required this.active,
    this.onTap,
    this.onOpen,
    this.glyphLevel = 1,
    this.enabled = true,
  });

  final GlyphKind glyph;

  /// False when Silica can't do it yet: the module is dimmed and does not respond.
  final bool enabled;
  final double glyphLevel;
  final String title;
  final String subtitle;
  final bool active;

  /// The icon disc: on or off.
  final VoidCallback? onTap;

  /// The text: opens the detail; without a detail the text acts like the disc.
  final VoidCallback? onOpen;

  @override
  Widget build(BuildContext context) {
    final widget = this;
    final palette = CcPalette.of(context);
    final hover = ModuleHoverScope.of(context);
    final arrowable = widget.onOpen != null && widget.enabled;
    final capsule = Stack(
      children: [
        // Text: detail (or, without one, the same effect as the disc).
        Positioned.fill(
          child: GestureDetector(
            onTap: widget.onOpen ?? widget.onTap,
            behavior: HitTestBehavior.opaque,
          ),
        ),
        Positioned(
          left: discInset,
          top: discTop,
          width: disc,
          height: disc,
          child: GestureDetector(
            onTap: widget.onTap,
            child: DecoratedBox(
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                // `Accent/Amalfi` when on, `Text/Tertiary` when off; the glyph is
                // `Text/On Accent` on both.
                color: widget.active ? palette.accent : palette.discOff,
              ),
              child: Center(
                child: CcGlyph(
                  widget.glyph,
                  color: palette.onAccent,
                  size: discGlyph,
                  level: widget.glyphLevel,
                ),
              ),
            ),
          ),
        ),
        Positioned(
          left: textLeft,
          right: 8 + (arrowable ? hover.progress * (hoverChevronRight + 2) : 0),
          top: 0,
          bottom: 0,
          child: IgnorePointer(
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  widget.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: AmalfiType.headline(color: palette.text),
                ),
                if (widget.subtitle.isNotEmpty)
                  Text(
                    widget.subtitle,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AmalfiType.caption(color: palette.secondary),
                  ),
              ],
            ),
          ),
        ),
        if (arrowable)
          Positioned(
            right: hoverChevronRight,
            top: (rowHeight - 12) / 2,
            child: IgnorePointer(
              child: Opacity(
                key: const ValueKey('detail chevron'),
                opacity: hover.progress,
                child: Chevron(color: palette.text, right: true),
              ),
            ),
          ),
      ],
    );
    if (widget.enabled) return capsule;
    return Semantics(
      enabled: false,
      child: AbsorbPointer(
        child: Opacity(opacity: disabledOpacity, child: capsule),
      ),
    );
  }
}

/// A module Silica can't drive yet: the whole module at the disabled opacity of Amalfi.
const double disabledOpacity = AmalfiStates.disabledOpacity;

/// "Until this evening": 19:00 today, or the end of the day once past it (provisional).
Duration untilEvening(DateTime now) {
  final evening = DateTime(now.year, now.month, now.day, 19);
  final end = now.isBefore(evening)
      ? evening
      : DateTime(now.year, now.month, now.day + 1);
  return end.difference(now);
}

/// The ">" arrow of modules with a detail, at the right end of the title row: it fades in
/// with the module's hover and the text shortens to make room (CC-HOVER).
const double hoverChevronRight = 16;

/// Hover of a Control Center module (controlcenter.md, CC-HOVER): the whole module lifts with
/// `State/Hover Overlay`, with no scale; inner buttons have no hover of their own. The change
/// starts after ~50 ms (10 %), is half done at ~150 ms and done at ~300 ms, the same in and
/// out and for every module (measured, [HoverCurve]).
const Duration hoverDuration = Duration(milliseconds: 300);

/// 10 % at 50 ms, 50 % at 150 ms, 100 % at 300 ms: a mild ease-out.
class HoverCurve extends Curve {
  const HoverCurve();

  static const List<(double, double)> _points = [
    (0, 0),
    (50 / 300, 0.1),
    (150 / 300, 0.5),
    (1, 1),
  ];

  @override
  double transformInternal(double t) {
    for (var i = 1; i < _points.length; i++) {
      final (x1, y1) = _points[i];
      if (t <= x1) {
        final (x0, y0) = _points[i - 1];
        return y0 + (y1 - y0) * (t - x0) / (x1 - x0);
      }
    }
    return 1;
  }
}

/// What a module's content reads from its wrapper: whether the pointer is on the module and
/// how far the hover has gone (0..1).
@immutable
class ModuleHoverState {
  const ModuleHoverState(this.hovered, this.progress);
  final bool hovered;
  final double progress;
}

class ModuleHoverScope extends InheritedWidget {
  const ModuleHoverScope({
    super.key,
    required this.state,
    required super.child,
  });

  final ModuleHoverState state;

  static ModuleHoverState of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<ModuleHoverScope>()?.state ??
      const ModuleHoverState(false, 0);

  @override
  bool updateShouldNotify(ModuleHoverScope old) =>
      old.state.hovered != state.hovered ||
      old.state.progress != state.progress;
}

/// Wraps a module of the stack: the hover lift over the whole module (its [radius]) and the
/// scope its content reads.
class _ModuleHover extends StatefulWidget {
  const _ModuleHover({required this.radius, required this.child});

  final double radius;
  final Widget child;

  @override
  State<_ModuleHover> createState() => _ModuleHoverState();
}

class _ModuleHoverState extends State<_ModuleHover>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: hoverDuration,
  );
  // Entering follows the curve; leaving is its mirror in time (10 % at 55 ms, half at 150 ms).
  double get _progress {
    const curve = HoverCurve();
    final v = _controller.value;
    return _hover ? curve.transform(v) : curve.flipped.transform(v);
  }

  bool _hover = false;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _set(bool hover) {
    _hover = hover;
    hover ? _controller.forward() : _controller.reverse();
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => _set(true),
      onExit: (_) => _set(false),
      child: AnimatedBuilder(
        animation: _controller,
        child: widget.child,
        builder: (context, child) => ModuleHoverScope(
          state: ModuleHoverState(_hover, _progress),
          child: Stack(
            fit: StackFit.passthrough,
            clipBehavior: Clip.none,
            children: [
              Positioned.fill(
                child: IgnorePointer(
                  child: DecoratedBox(
                    key: const ValueKey('module hover'),
                    decoration: BoxDecoration(
                      color: CcPalette.of(context).hoverLift.withValues(
                        alpha: CcPalette.of(context).hoverLift.a * _progress,
                      ),
                      borderRadius: BorderRadius.circular(widget.radius),
                    ),
                  ),
                ),
              ),
              child!,
            ],
          ),
        ),
      ),
    );
  }
}

/// A 64 pt round toggle (`Module/Tile/*`: Dark Mode, Screen Capture, Stage View, Screen
/// Mirroring): the module itself is the button. Off: a 20 pt glyph in `Text/Primary` on the
/// module glass; on: the whole tile `Accent/Amalfi` with a `Text/On Accent` glyph.
class _RoundToggle extends StatelessWidget {
  const _RoundToggle({
    required this.glyph,
    required this.label,
    required this.active,
    required this.onTap,
  });

  final GlyphKind glyph;
  final String label;
  final bool active;

  /// Null: Silica can't do it yet, the tile is dimmed and does not respond.
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final enabled = onTap != null;
    final palette = CcPalette.of(context);
    final tile = DecoratedBox(
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: active ? palette.accent : null,
      ),
      child: Center(
        child: CcGlyph(
          glyph,
          color: active ? palette.onAccent : palette.text,
          size: discGlyph,
        ),
      ),
    );
    return Semantics(
      button: true,
      enabled: enabled,
      toggled: active,
      label: label,
      child: enabled
          ? GestureDetector(onTap: onTap, child: tile)
          : AbsorbPointer(
              child: Opacity(opacity: disabledOpacity, child: tile),
            ),
    );
  }
}

/// The transport button of a media control: a 20 pt Icon Kit glyph in a 30 pt hit area.
class _TransportButton extends StatelessWidget {
  const _TransportButton({
    super.key,
    required this.symbol,
    required this.enabled,
    required this.onTap,
  });

  final String symbol;
  final bool enabled;
  final VoidCallback? onTap;
  static const double size = 20;

  @override
  Widget build(BuildContext context) {
    final glyph = AmalfiSymbol(
      symbol,
      size: size,
      color: CcPalette.of(context).text,
      fallback: SizedBox.square(dimension: size),
    );
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: enabled ? onTap : null,
      child: Center(
        child: enabled
            ? glyph
            : Opacity(opacity: AmalfiStates.disabledOpacity, child: glyph),
      ),
    );
  }
}

/// The artwork square of the media module and of its detail: the cover, or a
/// `Palette/Terracotta` tile with a music note (`Module/Now Playing/*`).
class _Artwork extends StatelessWidget {
  const _Artwork({required this.url, required this.radius, required this.note});

  final String url;
  final double radius;
  final double note;

  @override
  Widget build(BuildContext context) {
    final file = url.startsWith('file://')
        ? File(Uri.parse(url).toFilePath())
        : null;
    final colors = AmalfiTheme.of(context).colors;
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: file != null && file.existsSync()
          ? Image.file(file, fit: BoxFit.cover)
          : ColoredBox(
              color: colors.paletteTerracotta,
              child: Center(
                child: AmalfiSymbol(
                  'music-note',
                  size: note,
                  color: colors.textOnAccent,
                  fallback: SizedBox.square(dimension: note),
                ),
              ),
            ),
    );
  }
}

/// The media module (`Module/Now Playing/*`): the 40 pt cover at (12, 12), title (Headline)
/// at (12, 64) and artist (Caption) at (12, 84), the three 20 pt transport glyphs at x 20 / 60
/// / 100 and y 104. With nothing playing the buttons are dimmed; with an MPRIS player it shows
/// title and artist and controls them.
class _Media extends StatefulWidget {
  const _Media({this.onOpen});

  /// A click on the rest of the module (cover, title): the Now Playing detail.
  final VoidCallback? onOpen;

  @override
  State<_Media> createState() => _MediaState();
}

const double mediaButtonSide = 30;
const List<double> mediaButtonCenters = [30, 70, 110];
const double mediaButtonCenterY = 114;
const double mediaArtRadius = 8;

class _MediaState extends State<_Media> {
  final MediaPlayers _players = MediaPlayers.instance;

  @override
  void initState() {
    super.initState();
    _players.start();
    _players.addListener(_changed);
  }

  @override
  void dispose() {
    _players.removeListener(_changed);
    super.dispose();
  }

  void _changed() => setState(() {});

  Widget _button({
    required String key,
    required String symbol,
    required double centerX,
    required bool enabled,
    VoidCallback? onTap,
  }) => Positioned(
    left: centerX - mediaButtonSide / 2,
    top: mediaButtonCenterY - mediaButtonSide / 2,
    width: mediaButtonSide,
    height: mediaButtonSide,
    child: _TransportButton(
      key: ValueKey(key),
      symbol: symbol,
      enabled: enabled,
      onTap: onTap,
    ),
  );

  @override
  Widget build(BuildContext context) {
    final now = _players.now;
    final active = now != null;
    final p = CcPalette.of(context);
    return Stack(
      children: [
        Positioned.fill(
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: active ? widget.onOpen : null,
          ),
        ),
        Positioned(
          left: 12,
          top: 12,
          width: 40,
          height: 40,
          child: IgnorePointer(
            child: _Artwork(
              url: now?.artUrl ?? '',
              radius: mediaArtRadius,
              note: 20,
            ),
          ),
        ),
        Positioned(
          left: 12,
          right: 12,
          top: 64,
          child: IgnorePointer(
            child: active
                ? Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        now.title.isEmpty
                            ? l10nOf(context).mediaUntitled
                            : now.title,
                        maxLines: 1,
                        softWrap: false,
                        overflow: TextOverflow.ellipsis,
                        style: AmalfiType.headline(color: p.text),
                      ),
                      if (now.byline.isNotEmpty)
                        Text(
                          now.byline,
                          maxLines: 1,
                          softWrap: false,
                          overflow: TextOverflow.ellipsis,
                          style: AmalfiType.caption(color: p.secondary),
                        ),
                    ],
                  )
                : Text(
                    l10nOf(context).mediaNothingPlaying,
                    maxLines: 1,
                    softWrap: false,
                    overflow: TextOverflow.ellipsis,
                    style: AmalfiType.headline(color: p.text),
                  ),
          ),
        ),
        _button(
          key: 'precedente',
          symbol: 'skip-backward',
          centerX: mediaButtonCenters[0],
          enabled: active && now.canPrevious,
          onTap: active ? _players.previous : null,
        ),
        _button(
          key: 'riproduci',
          symbol: active && now.playing ? 'pause' : 'play',
          centerX: mediaButtonCenters[1],
          enabled: active,
          onTap: active ? _players.playPause : null,
        ),
        _button(
          key: 'successivo',
          symbol: 'skip-forward',
          centerX: mediaButtonCenters[2],
          enabled: active && now.canNext,
          onTap: active ? _players.next : null,
        ),
      ],
    );
  }
}

/// The Now Playing detail (`Detail/Now Playing`): a 312 × 140 panel, the 112 pt cover at
/// (16, 14), title and artist from (140, 16), three 20 pt transport glyphs at y 64 and x 146 /
/// 204 / 260, the 156 × 4 progress track at (140, 104) and the times under it.
const double nowPlayingHeight = 140;
const double nowPlayingArt = 112;
const double nowPlayingArtRadius = 12;
const double nowPlayingTextLeft = 140;
const double nowPlayingButton = 28;
const List<double> nowPlayingButtonCentres = [156, 214, 270];
const double nowPlayingButtonCentreY = 74;
const Rect nowPlayingProgress = Rect.fromLTWH(140, 104, 156, 4);

/// "01:03", "1:02:03".
String mediaTime(Duration d) {
  String two(int n) => n.toString().padLeft(2, '0');
  final h = d.inHours, m = d.inMinutes % 60, s = d.inSeconds % 60;
  return h > 0 ? '$h:${two(m)}:${two(s)}' : '${two(m)}:${two(s)}';
}

class _NowPlayingDetail extends StatefulWidget {
  const _NowPlayingDetail();

  @override
  State<_NowPlayingDetail> createState() => _NowPlayingDetailState();
}

class _NowPlayingDetailState extends State<_NowPlayingDetail> {
  final MediaPlayers _players = MediaPlayers.instance;
  Timer? _tick;

  @override
  void initState() {
    super.initState();
    _players.addListener(_changed);
    // The position moves on while playing: once a second.
    _tick = Timer.periodic(const Duration(seconds: 1), (_) => _changed());
  }

  @override
  void dispose() {
    _tick?.cancel();
    _players.removeListener(_changed);
    super.dispose();
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  Widget _button(String symbol, int index, bool enabled, VoidCallback tap) =>
      Positioned(
        left: nowPlayingButtonCentres[index] - nowPlayingButton / 2,
        top: nowPlayingButtonCentreY - nowPlayingButton / 2,
        width: nowPlayingButton,
        height: nowPlayingButton,
        child: _TransportButton(symbol: symbol, enabled: enabled, onTap: tap),
      );

  @override
  Widget build(BuildContext context) {
    final now = _players.now;
    final p = CcPalette.of(context);
    final length = now?.length ?? Duration.zero;
    final position = now?.positionAt(DateTime.now()) ?? Duration.zero;
    final fraction = length > Duration.zero
        ? (position.inMilliseconds / length.inMilliseconds).clamp(0.0, 1.0)
        : 0.0;
    final timeStyle = AmalfiType.caption(color: p.tertiary);
    return Stack(
      children: [
        Positioned(
          left: 16,
          top: 14,
          width: nowPlayingArt,
          height: nowPlayingArt,
          child: _Artwork(
            url: now?.artUrl ?? '',
            radius: nowPlayingArtRadius,
            note: 24,
          ),
        ),
        Positioned(
          left: nowPlayingTextLeft,
          right: 20,
          top: 16,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                now == null || now.title.isEmpty
                    ? l10nOf(context).mediaUntitled
                    : now.title,
                maxLines: 1,
                softWrap: false,
                overflow: TextOverflow.ellipsis,
                style: AmalfiType.headline(color: p.text),
              ),
              if (now != null && now.byline.isNotEmpty)
                Text(
                  now.byline,
                  maxLines: 1,
                  softWrap: false,
                  overflow: TextOverflow.ellipsis,
                  style: AmalfiType.caption(color: p.secondary),
                ),
            ],
          ),
        ),
        _button(
          'skip-backward',
          0,
          now?.canPrevious ?? false,
          _players.previous,
        ),
        _button(
          now?.playing ?? false ? 'pause' : 'play',
          1,
          now != null,
          _players.playPause,
        ),
        _button('skip-forward', 2, now?.canNext ?? false, _players.next),
        // The read-only slider, no thumb, centred on the 4 pt progress line.
        Positioned.fromRect(
          rect: Rect.fromCenter(
            center: nowPlayingProgress.center,
            width: nowPlayingProgress.width,
            height: 20,
          ),
          child: AmalfiSlider.indicator(
            key: const ValueKey('now playing progress'),
            value: fraction.clamp(0.0, 1.0),
          ),
        ),
        if (length > Duration.zero)
          Positioned(
            left: nowPlayingProgress.left,
            width: nowPlayingProgress.width,
            top: nowPlayingProgress.bottom + 4,
            child: Row(
              children: [
                Text(mediaTime(position), style: timeStyle),
                const Spacer(),
                Text('−${mediaTime(length - position)}', style: timeStyle),
              ],
            ),
          ),
      ],
    );
  }
}

/// A slider module (`Module/Slider/*`): the title (Headline) at (16, 8), a 16 pt glyph at each
/// end of an [AmalfiSlider] (with its lens) whose track runs from x 40 at y 44, and, for
/// Sound, the 28 pt outputs disc at (252, 20) that shortens the track by 48 pt.
class _SliderModule extends StatelessWidget {
  const _SliderModule({
    this.onOpen,
    this.onOutputs,
    required this.title,
    required this.value,
    required this.enabled,
    required this.low,
    required this.high,
    required this.onChanged,
  });

  final String title;
  final double value;
  final bool enabled;
  final GlyphKind low;
  final GlyphKind high;
  final ValueChanged<double> onChanged;

  /// The title opens the detail.
  final VoidCallback? onOpen;

  /// The audio outputs disc (Sound only): opens the detail with the outputs.
  final VoidCallback? onOutputs;

  /// With the outputs disc the track is shorter.
  double get _length =>
      onOutputs == null ? trackLength : trackLength - outputsShift;

  @override
  Widget build(BuildContext context) {
    final p = CcPalette.of(context);
    return Stack(
      children: [
        Positioned(
          left: 16,
          top: 8,
          right: 16,
          height: 24,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: onOpen,
            child: Align(
              alignment: Alignment.topLeft,
              child: Text(
                title,
                maxLines: 1,
                style: AmalfiType.headline(color: p.text),
              ),
            ),
          ),
        ),
        Positioned(
          left: 16,
          top: 36,
          child: CcGlyph(low, color: p.text, size: 16),
        ),
        Positioned(
          left: trackLeft,
          top: trackCenter - 10,
          width: _length,
          height: 20,
          child: AmalfiSlider(
            value: value.clamp(0.0, 1.0),
            onChanged: enabled ? onChanged : null,
            semanticLabel: title,
          ),
        ),
        Positioned(
          left: trackLeft + _length + 8,
          top: 36,
          child: CcGlyph(high, color: p.text, size: 16),
        ),
        if (onOutputs != null)
          Positioned(
            key: const ValueKey('audio outputs'),
            left: outputsLeft,
            top: outputsTop,
            width: outputsSide,
            height: outputsSide,
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: onOutputs,
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: p.discOff,
                  shape: BoxShape.circle,
                ),
                child: Center(
                  child: CcGlyph(
                    GlyphKind.headphones,
                    color: p.onAccent,
                    size: 16,
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

/// The privacy pill (`Module/Privacy Pill`): a 24 pt Glass/Thick capsule centred over the
/// stack; a 16 pt `Palette/Lavanda` rounded square with the microphone or camera at (8, 4),
/// the app in Label from x 32 ("<app> recently" once it stopped). A click opens the privacy
/// settings (provisional: the reference's click was not observed).
class _PrivacyPill extends StatelessWidget {
  const _PrivacyPill({required this.use, required this.onTap});

  final PrivacyUse use;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final l = l10nOf(context);
    final p = CcPalette.of(context);
    final text = use.inUse
        ? l.ccPrivacyNow(use.app)
        : l.ccPrivacyRecent(use.app);
    return Semantics(
      button: onTap != null,
      label: text,
      child: GestureDetector(
        onTap: onTap,
        child: ProfiledGlass(
          profile: ShellGlassProfile.module,
          radius: pillHeight / 2,
          material: GlassMaterial.thick,
          child: Padding(
            padding: const EdgeInsets.only(left: 8, right: 8),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 16,
                  height: 16,
                  decoration: BoxDecoration(
                    color: p.colors.paletteLavanda,
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Center(
                    child: CcGlyph(
                      use.kind == 'camera'
                          ? GlyphKind.camera
                          : GlyphKind.microphone,
                      color: p.onAccent,
                      size: 12,
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 220),
                  child: Text(
                    text,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AmalfiType.label(color: p.text),
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

/// The Edit Controls button (`Module/Edit Button`): a 116 × 24 Glass/Thick capsule (radius
/// 12), centred, 16 pt under the last module, the label in Label.
const Size editButton = Size(116, 24);
const double editButtonGap = 16;

class _EditButton extends StatelessWidget {
  const _EditButton({required this.label, required this.onTap});

  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
    button: true,
    label: label,
    child: GestureDetector(
      onTap: onTap,
      behavior: HitTestBehavior.opaque,
      child: ProfiledGlass(
        profile: ShellGlassProfile.module,
        radius: editButton.height / 2,
        material: GlassMaterial.thick,
        child: Center(
          child: Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: AmalfiType.label(color: CcPalette.of(context).text),
          ),
        ),
      ),
    ),
  );
}

/// The shadow of the module stack: `shadows.3` of Glass/Thick (y 12, blur 32), drawn outside
/// the modules only.
class _StackShadow {
  const _StackShadow({
    required this.origin,
    required this.height,
    required this.slots,
    required this.color,
  });

  final Offset origin;
  final double height;
  final List<ModuleSlot> slots;
  final Color color;

  Object get cacheKey =>
      Object.hash(origin, height, color, Object.hashAll(slots));

  void paintShadow(Canvas canvas, Size size) {
    var modules = Path();
    final shifted = Path();
    for (final slot in slots) {
      final rrect = RRect.fromRectAndRadius(
        slot.rect.shift(origin),
        Radius.circular(slot.radius),
      );
      modules = Path.combine(
        PathOperation.union,
        modules,
        Path()..addRRect(rrect),
      );
      shifted.addRRect(rrect.shift(const Offset(0, shadowOffsetY)));
    }
    canvas
      ..save()
      ..clipPath(
        Path.combine(
          PathOperation.difference,
          Path()..addRect(Offset.zero & size),
          modules,
        ),
      )
      ..drawPath(
        shifted,
        Paint()
          ..color = color
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, shadowSigma),
      )
      ..restore();
  }
}
