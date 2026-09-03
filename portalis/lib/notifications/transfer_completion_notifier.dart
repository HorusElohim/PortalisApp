import 'dart:async';

import '../nexus/domain/app_state.dart';

/// Delivers a native alert after a receiver-side transfer finishes.
abstract interface class TransferCompletionNotifier {
  Future<void> showCompleted({required int id, required String name});
}

/// Forwards Rust's own completion events to a platform notification.
///
/// Rust owns the completion edge — see `AppTransferCompleted` and
/// `nexus::core::transfers::follow_transfers`, the one place a completion is
/// ever decided. This observer used to infer completion itself by diffing
/// successive `AppSnapshot`s (comparing `completedAt` against what it saw
/// last time, and treating the first snapshot after startup as a baseline
/// rather than a new event so a restart did not replay every historical
/// completion). That entire baseline/diff mechanism is gone: the backend
/// only ever emits the event at the moment a transfer actually finishes, so
/// there is nothing left here to infer.
class TransferCompletionObserver {
  TransferCompletionObserver(this._notifier);

  final TransferCompletionNotifier _notifier;
  StreamSubscription<AppTransferCompleted>? _subscription;

  /// Starts forwarding completions from [completions] until [stop] is
  /// called. Safe to call once; a second call is a no-op rather than a
  /// second subscription.
  void start(Stream<AppTransferCompleted> completions) {
    _subscription ??= completions.listen(
      (event) => unawaited(
        _notifier
            .showCompleted(id: event.collection, name: event.name)
            .catchError((_) {}),
      ),
      // A stream failure must not crash the app; the next successful
      // reconnect (if the underlying bridge stream retries) resumes
      // notifying normally.
      onError: (_) {},
    );
  }

  Future<void> stop() async {
    await _subscription?.cancel();
    _subscription = null;
  }
}
