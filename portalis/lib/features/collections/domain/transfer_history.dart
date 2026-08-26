/// Download and upload samples collected while a collection is visible to the
/// running transfer engine.
class TransferHistory {
  TransferHistory({required this.startedAt});

  factory TransferHistory.restore({
    required DateTime startedAt,
    required List<TransferSample> samples,
    DateTime? completedAt,
  }) {
    final history = TransferHistory(startedAt: startedAt)
      ..completedAt = completedAt;
    history._samples.addAll(
      samples.length <= _maxSamples
          ? samples
          : samples.sublist(samples.length - _maxSamples),
    );
    return history;
  }

  // The Rust core writes at this cadence; thinning it again in the UI loses
  // half of the persisted timeline before the chart can render it.
  static const _sampleSpacing = Duration(milliseconds: 500);
  // Match the backend's 3,600 samples at 500 ms: a complete thirty-minute
  // graph survives both initial restoration and a long-open detail screen.
  static const _maxSamples = 3600;

  final DateTime startedAt;
  final List<TransferSample> _samples = [];
  DateTime? completedAt;

  List<TransferSample> get samples => List.unmodifiable(_samples);

  bool record({
    required DateTime at,
    required int downBytesPerSecond,
    required int upBytesPerSecond,
    required double progress,
  }) {
    final previous = _samples.isEmpty ? null : _samples.last;
    if (previous != null && at.difference(previous.at) < _sampleSpacing) {
      return false;
    }
    _samples.add(
      TransferSample(
        at: at,
        downBytesPerSecond: downBytesPerSecond,
        upBytesPerSecond: upBytesPerSecond,
        progress: progress,
      ),
    );
    if (_samples.length > _maxSamples) {
      _samples.removeRange(0, _samples.length - _maxSamples);
    }
    return true;
  }
}

class TransferSample {
  const TransferSample({
    required this.at,
    required this.downBytesPerSecond,
    required this.upBytesPerSecond,
    required this.progress,
  });

  final DateTime at;
  final int downBytesPerSecond;
  final int upBytesPerSecond;
  final double progress;
}
