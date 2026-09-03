import 'dart:async';

import 'package:portalis/notifications/transfer_completion_notifier.dart';

import 'test_support.dart';

void main() {
  test('completion observer forwards each typed Rust completion once',
      () async {
    final notifier = _RecordingNotifier();
    final observer = TransferCompletionObserver(notifier);
    final completions = StreamController<AppTransferCompleted>();
    observer.start(completions.stream);

    completions.add(
      const AppTransferCompleted(collection: 7, name: 'Northern lights'),
    );
    await completions.close();

    expect(notifier.completed, [(7, 'Northern lights')]);
    await observer.stop();
  });

  test('completion observer stops forwarding after cancellation', () async {
    final notifier = _RecordingNotifier();
    final observer = TransferCompletionObserver(notifier);
    final completions = StreamController<AppTransferCompleted>();
    observer.start(completions.stream);

    await observer.stop();
    completions.add(
      const AppTransferCompleted(collection: 7, name: 'Northern lights'),
    );
    await completions.close();

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
