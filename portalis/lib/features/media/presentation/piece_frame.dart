import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../domain/item.dart';

/// Decodes the backend's packed 64-bucket visual progress shape into sparse
/// perimeter ranges. Invalid or unavailable data returns an empty list so the
/// caller can fall back to its aggregate percentage.
List<PerimeterSegment> progressSegmentsForBuckets(List<int> packed) {
  const bucketCount = 64;
  if (packed.length != 16) return const [];

  final segments = <PerimeterSegment>[];
  int? state;
  var start = 0;
  for (var bucket = 0; bucket <= bucketCount; bucket++) {
    final next = bucket == bucketCount
        ? 0
        : (packed[bucket ~/ 4] >> ((bucket % 4) * 2)) & 3;
    if (next == state) continue;
    if ((state == 1 || state == 2) && bucket > start) {
      segments.add(PerimeterSegment(
        start: start / bucketCount,
        extent: (bucket - start) / bucketCount,
        active: state == 2,
        workerCount: state == 2 ? 1 : 0,
      ));
    }
    state = next;
    start = bucket;
  }
  return segments;
}

/// Paints only progress state supplied by the backend. When older/unavailable
/// telemetry has no buckets, the existing aggregate perimeter remains as the
/// honest fallback.
class MediaPieceFrame extends StatefulWidget {
  const MediaPieceFrame({
    super.key,
    required this.media,
    required this.color,
    required this.borderRadius,
    required this.child,
  });

  final MediaItem media;
  final Color color;
  final BorderRadius borderRadius;
  final Widget child;

  @override
  State<MediaPieceFrame> createState() => _MediaPieceFrameState();
}

class _MediaPieceFrameState extends State<MediaPieceFrame>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pulse = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
    value: 1,
  );

  bool get _hasWorkers => progressSegmentsForBuckets(
        widget.media.progressBuckets,
      ).any((segment) => segment.active);

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _syncPulse();
  }

  @override
  void didUpdateWidget(covariant MediaPieceFrame oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncPulse();
  }

  void _syncPulse() {
    final shouldAnimate =
        _hasWorkers && !MediaQuery.disableAnimationsOf(context);
    if (shouldAnimate && !_pulse.isAnimating) {
      _pulse.repeat(reverse: true);
    } else if (!shouldAnimate && _pulse.isAnimating) {
      _pulse
        ..stop()
        ..value = 1;
    }
  }

  @override
  void dispose() {
    _pulse.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final size = widget.media.sizeBytes;
    final bucketSegments = progressSegmentsForBuckets(
      widget.media.progressBuckets,
    );
    final segments = size <= 0 ? const <PerimeterSegment>[] : bucketSegments;
    return AnimatedBuilder(
      animation: _pulse,
      builder: (context, child) => PerimeterProgress(
        progress: widget.media.progress,
        color: widget.color,
        activeColor: Color.lerp(
          AppColors.signalDim,
          AppColors.signalSoft,
          _pulse.value,
        ),
        borderRadius: widget.borderRadius,
        segments: segments,
        child: child!,
      ),
      child: widget.child,
    );
  }
}
