import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:portalis/design/formatters.dart';
import 'package:portalis/design/theme.dart';

/// A point in the transfer history shown by [TransferGraph].
class TransferPoint {
  const TransferPoint({
    required this.at,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
  });

  final DateTime at;
  final int downBytesPerSecond;
  final int upBytesPerSecond;
}

/// A real per-torrent receive/upload history sampled by the collections core.
class TransferGraph extends StatelessWidget {
  TransferGraph({
    super.key,
    required this.progress,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    this.sourceReading = false,
    this.seeding = false,
    this.history = const [],
    this.startedAt,
    this.completedAt,
    this.showHeader = true,
    Color? color,
  }) : color = color ?? AppColors.signal;

  final double progress;
  final int downBytesPerSecond;
  final int upBytesPerSecond;
  final bool sourceReading;
  final bool seeding;
  final List<TransferPoint> history;
  final DateTime? startedAt;
  final DateTime? completedAt;
  final bool showHeader;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final graph = _TransferGraphState.from(
      progress: progress,
      downBytesPerSecond: downBytesPerSecond,
      upBytesPerSecond: upBytesPerSecond,
      sourceReading: sourceReading,
      seeding: seeding,
      history: history,
      startedAt: startedAt,
      completedAt: completedAt,
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (showHeader) ...[
          _TransferGraphHeading(graph: graph, color: color),
          const SizedBox(height: 10),
        ],
        if (graph.maxRate <= 0)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 12),
            child: Row(
              children: [
                Container(width: 26, height: 2, color: color),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    graph.complete
                        ? (graph.sourceReading
                            ? 'No non-zero source-read samples were recorded.'
                            : 'No non-zero receive-session samples were recorded.')
                        : 'Waiting for the first non-zero receive sample.',
                    style: monoLabel(size: 10, color: AppColors.textDim),
                  ),
                ),
              ],
            ),
          )
        else
          Semantics(
            label: 'Transfer speed on a logarithmic scale from '
                '${_dateTimeLabel(graph.start)} to '
                '${_dateTimeLabel(graph.end)}. Peak ${graph.sourceReading ? 'source-read' : 'receive'} '
                '${formatRate(graph.peakDownload)}${graph.hasUpload ? ', peak upload ${formatRate(graph.peakUpload)}' : ''}.',
            child: SizedBox(
              height: 86,
              width: double.infinity,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  SizedBox(
                    width: 66,
                    child: _RateAxis(
                      maxRate: graph.maxRate,
                      minPositiveRate: graph.minPositiveRate,
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: CustomPaint(
                      painter: _TransferGraphPainter(
                        history: graph.points,
                        startedAt: graph.start,
                        endedAt: graph.end,
                        maxRate: graph.maxRate,
                        minPositiveRate: graph.minPositiveRate,
                        color: color,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        const SizedBox(height: 7),
        LayoutBuilder(
          builder: (context, constraints) {
            final compact = constraints.maxWidth < 520;
            return Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: _TimelineLabel(
                    title: 'START',
                    value: graph.start,
                    compact: compact,
                  ),
                ),
                Expanded(
                  child: _TimelineLabel(
                    title: graph.complete
                        ? 'END'
                        : (graph.active ? 'LATEST' : 'LAST RECORDED'),
                    value: graph.end,
                    compact: compact,
                    alignEnd: true,
                  ),
                ),
              ],
            );
          },
        ),
      ],
    );
  }

  String _dateTimeLabel(DateTime value) =>
      '${value.day.toString().padLeft(2, '0')}/'
      '${value.month.toString().padLeft(2, '0')}/'
      '${value.year} '
      '${value.hour.toString().padLeft(2, '0')}:'
      '${value.minute.toString().padLeft(2, '0')}:'
      '${value.second.toString().padLeft(2, '0')}';
}

/// The live speed facts that can sit beside a collection identity while the
/// chart itself remains below. [TransferGraph] shows the same heading by
/// default when it is used on its own.
class TransferGraphHeader extends StatelessWidget {
  TransferGraphHeader({
    super.key,
    required this.progress,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    this.sourceReading = false,
    this.seeding = false,
    this.history = const [],
    this.startedAt,
    this.completedAt,
    Color? color,
  }) : color = color ?? AppColors.signal;

  final double progress;
  final int downBytesPerSecond;
  final int upBytesPerSecond;
  final bool sourceReading;
  final bool seeding;
  final List<TransferPoint> history;
  final DateTime? startedAt;
  final DateTime? completedAt;
  final Color color;

  @override
  Widget build(BuildContext context) => _TransferGraphHeading(
        graph: _TransferGraphState.from(
          progress: progress,
          downBytesPerSecond: downBytesPerSecond,
          upBytesPerSecond: upBytesPerSecond,
          sourceReading: sourceReading,
          seeding: seeding,
          history: history,
          startedAt: startedAt,
          completedAt: completedAt,
        ),
        color: color,
      );
}

class _TransferGraphHeading extends StatelessWidget {
  const _TransferGraphHeading({required this.graph, required this.color});

  final _TransferGraphState graph;
  final Color color;

  @override
  Widget build(BuildContext context) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text(
                graph.sourceReading
                    ? 'SOURCE VERIFICATION'
                    : (graph.seeding
                        ? 'SEEDING SPEED'
                        : (graph.active
                            ? 'RECEIVING SPEED'
                            : (graph.complete
                                ? 'RECEIVE SESSION'
                                : 'RECEIVE HISTORY'))),
                style: monoLabel(
                  size: 10,
                  color: AppColors.textDim,
                  weight: FontWeight.w700,
                ),
              ),
              const Spacer(),
              Text(
                graph.complete
                    ? 'COMPLETED IN ${graph.spanLabel}'
                    : (graph.active
                        ? 'LIVE · ${graph.spanLabel}'
                        : 'LAST RECORDED · ${graph.spanLabel}'),
                style: monoLabel(size: 10, color: AppColors.textGhost),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _SeriesSummary(
                color: color,
                label: graph.sourceReading
                    ? 'VERIFYING'
                    : (graph.seeding ? 'SEEDING' : 'RECEIVING'),
                value: graph.active
                    ? 'now ${formatRate(graph.downBytesPerSecond)} · peak ${formatRate(graph.peakDownload)}'
                    : 'peak ${formatRate(graph.peakDownload)}',
              ),
              if (graph.hasUpload)
                _SeriesSummary(
                  color: AppColors.signalSoft,
                  label: 'UPLOAD',
                  value: graph.active && graph.upBytesPerSecond > 0
                      ? 'now ${formatRate(graph.upBytesPerSecond)} · peak ${formatRate(graph.peakUpload)}'
                      : 'peak ${formatRate(graph.peakUpload)}',
                ),
            ],
          ),
        ],
      );
}

class _TransferGraphState {
  const _TransferGraphState({
    required this.complete,
    required this.active,
    required this.sourceReading,
    required this.seeding,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    required this.points,
    required this.start,
    required this.end,
    required this.completedAt,
    required this.peakDownload,
    required this.peakUpload,
    required this.minPositiveRate,
  });

  factory _TransferGraphState.from({
    required double progress,
    required int downBytesPerSecond,
    required int upBytesPerSecond,
    required List<TransferPoint> history,
    required bool sourceReading,
    required bool seeding,
    DateTime? startedAt,
    DateTime? completedAt,
  }) {
    final now = DateTime.now();
    // A completed download remains completed even when it predates persisted
    // transfer metadata. In that case the final sample is the best truthful
    // end time available; treating it as "latest" suggests a live transfer.
    final complete = progress >= 1.0;
    // A paused or disconnected incomplete transfer still has useful history,
    // but it must not gain a synthetic "now" point on every rebuild. That
    // stretches its time axis until the real line is visually compressed away.
    final active =
        !complete && (downBytesPerSecond > 0 || upBytesPerSecond > 0);
    final current = TransferPoint(
      at: now,
      downBytesPerSecond: downBytesPerSecond,
      upBytesPerSecond: upBytesPerSecond,
    );
    final points = history.isEmpty
        ? [current]
        : active
            ? [...history, current]
            : history;
    final start = startedAt ?? points.first.at;
    final lastSampleAt = points.last.at;
    final requestedEnd = completedAt ?? lastSampleAt;
    final end =
        requestedEnd.isBefore(lastSampleAt) ? lastSampleAt : requestedEnd;
    final peakDownload = points.fold<int>(
      0,
      (peak, point) => math.max(peak, point.downBytesPerSecond),
    );
    final peakUpload = points.fold<int>(
      0,
      (peak, point) => math.max(peak, point.upBytesPerSecond),
    );
    final positiveRates = <int>[
      for (final point in points) ...[
        if (point.downBytesPerSecond > 0) point.downBytesPerSecond,
        if (point.upBytesPerSecond > 0) point.upBytesPerSecond,
      ],
    ];
    final minPositiveRate =
        positiveRates.isEmpty ? 0 : positiveRates.reduce(math.min);
    return _TransferGraphState(
      complete: complete,
      active: active,
      sourceReading: sourceReading,
      seeding: seeding,
      downBytesPerSecond: downBytesPerSecond,
      upBytesPerSecond: upBytesPerSecond,
      points: points,
      start: start,
      end: end,
      completedAt: completedAt,
      peakDownload: peakDownload,
      peakUpload: peakUpload,
      minPositiveRate: minPositiveRate,
    );
  }

  final bool complete;
  final bool active;
  final bool sourceReading;
  final bool seeding;
  final int downBytesPerSecond;
  final int upBytesPerSecond;
  final List<TransferPoint> points;
  final DateTime start;
  final DateTime end;

  /// When the core recorded this as finished, where it has.
  final DateTime? completedAt;
  final int peakDownload;
  final int peakUpload;
  final int minPositiveRate;

  int get maxRate => math.max(peakDownload, peakUpload);
  bool get hasUpload => peakUpload > 0 || upBytesPerSecond > 0;

  /// How long it took, as the core recorded it.
  ///
  /// [end] is stretched to cover the last reading so the axis holds every
  /// point it draws. That makes it the right edge of a chart and the wrong
  /// answer for a duration: a collection that kept reporting after it finished
  /// would read as having taken longer than it did. When the core says when it
  /// completed, that is the answer.
  String get spanLabel => _durationLabel(
        (completedAt ?? end).difference(start),
      );
}

String _durationLabel(Duration duration) {
  final seconds = math.max(0, duration.inSeconds);
  final hours = seconds ~/ 3600;
  final minutes = (seconds % 3600) ~/ 60;
  if (hours > 0) return '${hours}h ${minutes}m';
  if (minutes > 0) return '${minutes}m ${seconds % 60}s';
  return '${seconds}s';
}

class _TimelineLabel extends StatelessWidget {
  const _TimelineLabel({
    required this.title,
    required this.value,
    this.compact = false,
    this.alignEnd = false,
  });

  final String title;
  final DateTime value;
  final bool compact;
  final bool alignEnd;

  @override
  Widget build(BuildContext context) => Column(
        crossAxisAlignment:
            alignEnd ? CrossAxisAlignment.end : CrossAxisAlignment.start,
        children: [
          Text(title, style: monoLabel(size: 8, color: AppColors.textGhost)),
          const SizedBox(height: 2),
          Text(
            compact ? _compactDateTimeLabel(value) : _dateTimeLabel(value),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: alignEnd ? TextAlign.end : TextAlign.start,
            style: monoLabel(size: 9, color: AppColors.textDim),
          ),
        ],
      );

  String _compactDateTimeLabel(DateTime value) =>
      '${value.day.toString().padLeft(2, '0')}/'
      '${value.month.toString().padLeft(2, '0')}  '
      '${value.hour.toString().padLeft(2, '0')}:'
      '${value.minute.toString().padLeft(2, '0')}:'
      '${value.second.toString().padLeft(2, '0')}';

  String _dateTimeLabel(DateTime value) =>
      '${value.day.toString().padLeft(2, '0')}/'
      '${value.month.toString().padLeft(2, '0')}/'
      '${value.year}  '
      '${value.hour.toString().padLeft(2, '0')}:'
      '${value.minute.toString().padLeft(2, '0')}:'
      '${value.second.toString().padLeft(2, '0')}';
}

class _SeriesSummary extends StatelessWidget {
  const _SeriesSummary({
    required this.color,
    required this.label,
    required this.value,
  });

  final Color color;
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 7,
                  height: 7,
                  decoration:
                      BoxDecoration(shape: BoxShape.circle, color: color),
                ),
                const SizedBox(width: 6),
                Text(
                  label,
                  style: monoLabel(
                    size: 10,
                    color: color,
                    weight: FontWeight.w700,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              value,
              style: monoLabel(
                size: 13,
                color: AppColors.text,
                letterSpacing: 0,
              ),
            ),
          ],
        ),
      );
}

class _RateAxis extends StatelessWidget {
  const _RateAxis({
    required this.maxRate,
    required this.minPositiveRate,
  });

  final int maxRate;
  final int minPositiveRate;

  @override
  Widget build(BuildContext context) => Column(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Text(
            formatRate(maxRate),
            style: monoLabel(size: 8, color: AppColors.textDim),
          ),
          Text(
            formatRate(
              _inverseLogarithmicRate(
                0.5,
                maxRate: maxRate,
                minPositiveRate: minPositiveRate,
              ),
            ),
            style: monoLabel(size: 8, color: AppColors.textGhost),
          ),
          Text(
            formatRate(0),
            style: monoLabel(size: 8, color: AppColors.textGhost),
          ),
        ],
      );
}

class _TransferGraphPainter extends CustomPainter {
  const _TransferGraphPainter({
    required this.history,
    required this.startedAt,
    required this.endedAt,
    required this.maxRate,
    required this.minPositiveRate,
    required this.color,
  });

  final List<TransferPoint> history;
  final DateTime startedAt;
  final DateTime endedAt;
  final int maxRate;
  final int minPositiveRate;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    const top = 4.0;
    final bottom = size.height - 4;
    final grid = Paint()
      ..color = AppColors.border
      ..strokeWidth = 1;
    for (final y in [top, (top + bottom) / 2, bottom]) {
      canvas.drawLine(Offset(0, y), Offset(size.width, y), grid);
    }

    final duration = endedAt.difference(startedAt).inMicroseconds;
    final span = duration <= 0 ? 1 : duration;

    if (history.any((point) => point.downBytesPerSecond > 0)) {
      _drawSeries(
        canvas,
        size,
        history,
        maxRate,
        span,
        startedAt,
        top,
        bottom,
        color,
        (point) => point.downBytesPerSecond,
        fill: true,
      );
    }
    if (history.any((point) => point.upBytesPerSecond > 0)) {
      _drawSeries(
        canvas,
        size,
        history,
        maxRate,
        span,
        startedAt,
        top,
        bottom,
        AppColors.signalSoft,
        (point) => point.upBytesPerSecond,
      );
    }
  }

  void _drawSeries(
    Canvas canvas,
    Size size,
    List<TransferPoint> points,
    int scale,
    int span,
    DateTime start,
    double top,
    double bottom,
    Color color,
    int Function(TransferPoint point) rate, {
    bool fill = false,
  }) {
    final path = Path();
    final offsets = <Offset>[];
    for (var index = 0; index < points.length; index++) {
      final point = points[index];
      final elapsed = point.at.difference(start).inMicroseconds;
      final x = points.length == 1
          ? size.width
          : (size.width * elapsed / span).clamp(0.0, size.width);
      final normalized = _logarithmicRate(
        rate(point),
        maxRate: scale,
        minPositiveRate: minPositiveRate,
      );
      final y = bottom - normalized * (bottom - top);
      offsets.add(Offset(x, y));
      if (index == 0) {
        path.moveTo(x, y);
      } else {
        path.lineTo(x, y);
      }
    }

    if (fill && offsets.isNotEmpty) {
      final area = Path()
        ..moveTo(offsets.first.dx, bottom)
        ..lineTo(offsets.first.dx, offsets.first.dy);
      for (final point in offsets.skip(1)) {
        area.lineTo(point.dx, point.dy);
      }
      area
        ..lineTo(offsets.last.dx, bottom)
        ..close();
      canvas.drawPath(
        area,
        Paint()
          ..shader = LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [
              color.withValues(alpha: 0.16),
              color.withValues(alpha: 0.01),
            ],
          ).createShader(Rect.fromLTWH(0, top, size.width, bottom - top)),
      );
    }

    final paint = Paint()
      ..color = color.withValues(alpha: 0.86)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.2
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    canvas.drawPath(path, paint);

    final last = points.last;
    final lastElapsed = last.at.difference(start).inMicroseconds;
    final lastX = points.length == 1
        ? size.width
        : (size.width * lastElapsed / span).clamp(0.0, size.width);
    final lastRate = _logarithmicRate(
      rate(last),
      maxRate: scale,
      minPositiveRate: minPositiveRate,
    );
    final lastY = bottom - lastRate * (bottom - top);
    canvas.drawCircle(
      Offset(lastX, lastY),
      3,
      Paint()..color = color,
    );
  }

  @override
  bool shouldRepaint(_TransferGraphPainter oldDelegate) =>
      oldDelegate.history != history ||
      oldDelegate.startedAt != startedAt ||
      oldDelegate.endedAt != endedAt ||
      oldDelegate.maxRate != maxRate ||
      oldDelegate.minPositiveRate != minPositiveRate ||
      oldDelegate.color != color;
}

double _logarithmicRate(
  int rate, {
  required int maxRate,
  required int minPositiveRate,
}) {
  if (rate <= 0 || maxRate <= 0 || minPositiveRate <= 0) return 0;
  final denominator = math.log(1 + maxRate / minPositiveRate);
  if (denominator <= 0) return 1;
  // A small positive floor keeps a genuinely active direction visible even
  // when the other direction is orders of magnitude faster.
  return (math.log(1 + rate / minPositiveRate) / denominator)
      .clamp(0.06, 1.0)
      .toDouble();
}

int _inverseLogarithmicRate(
  double normalized, {
  required int maxRate,
  required int minPositiveRate,
}) {
  if (maxRate <= 0 || minPositiveRate <= 0) return 0;
  final logarithmicMax = math.log(1 + maxRate / minPositiveRate);
  return (minPositiveRate * (math.exp(logarithmicMax * normalized) - 1))
      .round();
}
