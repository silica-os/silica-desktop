// A placeholder for an icon the system composes asynchronously (Thumbnails1.Icon): the plain
// glyph stands until the icon reports that its composed image has been delivered and decoded
// (`SilicaIcon.onComposed`), then the icon takes its place in one frame, with no blank frame
// between them (#234). Event-driven: nothing polls, and an icon that never arrives costs
// nothing.

import 'package:flutter/widgets.dart';

/// Shows [placeholder] until the icon built by [builder] calls the callback it was given.
/// The icon is built from the start, hidden, so its request is already under way.
class PendingIcon extends StatefulWidget {
  const PendingIcon({
    super.key,
    required this.placeholder,
    required this.builder,
  });

  final Widget placeholder;

  /// The icon; it must call the callback once its composed image is drawn.
  final Widget Function(VoidCallback composed) builder;

  @override
  State<PendingIcon> createState() => _PendingIconState();
}

class _PendingIconState extends State<PendingIcon> {
  bool _ready = false;

  void _composed() {
    if (mounted && !_ready) setState(() => _ready = true);
  }

  @override
  Widget build(BuildContext context) {
    // The same tree before and after, so the icon keeps its state when the glyph leaves.
    return Stack(
      clipBehavior: Clip.none,
      children: [
        _ready ? const SizedBox.shrink() : widget.placeholder,
        Opacity(opacity: _ready ? 1 : 0, child: widget.builder(_composed)),
      ],
    );
  }
}
