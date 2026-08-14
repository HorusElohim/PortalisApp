import 'package:flutter/material.dart';

import '../theme.dart';

/// A fixed-height, drag-to-resize frame around a media grid.
///
/// The grid itself still sizes to its own content — a large collection could
/// otherwise push the whole card, and the list, far down the page. Scrolling
/// inside a height the person controls themselves is the same trade every
/// other resizable panel makes.
class ResizableMediaPreview extends StatefulWidget {
  const ResizableMediaPreview({super.key, required this.child});

  final Widget child;

  @override
  State<ResizableMediaPreview> createState() => _ResizableMediaPreviewState();
}

class _ResizableMediaPreviewState extends State<ResizableMediaPreview> {
  static const double _minHeight = 180;
  static const double _maxHeight = 480;
  static const double _defaultHeight = 220;

  double _height = _defaultHeight;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: _height,
          child: SingleChildScrollView(child: widget.child),
        ),
        MouseRegion(
          cursor: SystemMouseCursors.resizeUpDown,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onVerticalDragUpdate: (details) {
              setState(() {
                _height =
                    (_height + details.delta.dy).clamp(_minHeight, _maxHeight);
              });
            },
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 6),
              child: Center(
                child: Container(
                  width: 36,
                  height: 4,
                  decoration: BoxDecoration(
                    color: AppColors.borderStrong,
                    borderRadius: BorderRadius.circular(AppRadius.pill),
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}
