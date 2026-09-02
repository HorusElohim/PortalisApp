import 'package:portalis/notifications/transfer_completion_notifier.dart';

import 'test_support.dart';

void main() {
  test('completion observer alerts once when a live transfer completes',
      () async {
    final notifier = _RecordingNotifier();
    final observer = TransferCompletionObserver(notifier);

    await observer.observe(_snapshot(completedAt: null));
    await observer.observe(_snapshot(completedAt: BigInt.from(42)));
    await observer.observe(_snapshot(completedAt: BigInt.from(42)));

    expect(notifier.completed, [(7, 'Northern lights')]);
  });

  test('completion observer never alerts for a completion restored at startup',
      () async {
    final notifier = _RecordingNotifier();
    final observer = TransferCompletionObserver(notifier);

    await observer.observe(_snapshot(completedAt: BigInt.from(42)));

    expect(notifier.completed, isEmpty);
  });
}

class _RecordingNotifier implements TransferCompletionNotifier {
  final completed = <(int, String)>[];

  @override
  Future<void> showCompleted({required int id, required String name}) async {
    completed.add((id, name));
  }
}

AppSnapshot _snapshot({required BigInt? completedAt}) => AppSnapshot(
      device: const AppDevice(
        name: 'Portalis',
        handle: null,
        fingerprint: 'test',
        devices: 1,
      ),
      connectivity: 'LocalOnly',
      contacts: const [],
      collections: [
        buildNexusCollection(
          id: 7,
          name: 'Northern lights',
          nature: 'Torrent',
          role: 'Member',
          status: completedAt == null ? 'Downloading' : 'Available',
          entries: 1,
          totalBytes: 100,
          onDiskBytes: 100,
          completedAt: completedAt,
        ),
      ],
      alerts: const [],
      activity: const AppActivity(
        transfers: 0,
        downBytesPerSecond: 0,
        upBytesPerSecond: 0,
        peers: 0,
      ),
    );
