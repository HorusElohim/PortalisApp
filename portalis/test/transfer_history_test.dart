import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/collections/domain/transfer_history.dart';

void main() {
  test('keeps timestamped transfer samples at the backend poll cadence', () {
    final start = DateTime(2026, 8, 5, 12, 0);
    final history = TransferHistory(startedAt: start);

    expect(
      history.record(
        at: start,
        downBytesPerSecond: 1,
        upBytesPerSecond: 0,
        progress: 0.1,
      ),
      isTrue,
    );
    expect(
      history.record(
        at: start.add(const Duration(milliseconds: 500)),
        downBytesPerSecond: 2,
        upBytesPerSecond: 0,
        progress: 0.2,
      ),
      isTrue,
    );
    expect(
      history.record(
        at: start.add(const Duration(seconds: 1)),
        downBytesPerSecond: 3,
        upBytesPerSecond: 25000,
        progress: 0.3,
      ),
      isTrue,
    );

    expect(history.samples, hasLength(3));
    expect(history.samples.last.downBytesPerSecond, 3);
  });

  test('restores the graph and completion marker after a restart', () {
    final start = DateTime(2026, 8, 5, 12, 0);
    final sample = TransferSample(
      at: start.add(const Duration(seconds: 4)),
      downBytesPerSecond: 525000,
      upBytesPerSecond: 37500,
      progress: 0.8,
    );
    final history = TransferHistory.restore(
      startedAt: start,
      samples: [sample],
      completedAt: start.add(const Duration(minutes: 2)),
    );

    expect(history.startedAt, start);
    expect(history.samples.single.at, sample.at);
    expect(history.samples.single.downBytesPerSecond, 525000);
    expect(history.completedAt, start.add(const Duration(minutes: 2)));
  });

  test('retains the core\'s full thirty-minute transfer ring', () {
    final start = DateTime(2026, 8, 5, 12, 0);
    final history = TransferHistory.restore(
      startedAt: start,
      samples: List.generate(
        3601,
        (index) => TransferSample(
          at: start.add(Duration(milliseconds: 500 * index)),
          downBytesPerSecond: index,
          upBytesPerSecond: 0,
          progress: index / 3600,
        ),
      ),
    );

    expect(history.samples, hasLength(3600));
    expect(history.samples.first.downBytesPerSecond, 1);
    expect(history.samples.last.downBytesPerSecond, 3600);
  });
}
