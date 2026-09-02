import '../nexus/domain/app_state.dart';

/// Delivers a native alert after a receiver-side transfer finishes.
abstract interface class TransferCompletionNotifier {
  Future<void> showCompleted({required int id, required String name});
}

/// Observes complete engine snapshots without replaying historical completions.
///
/// A completion timestamp is durable, so the first state after startup is a
/// baseline, not a new event. Later changes are the engine's exact completion
/// edge and are the only ones that may notify a person.
class TransferCompletionObserver {
  TransferCompletionObserver(this._notifier);

  final TransferCompletionNotifier _notifier;
  Map<int, BigInt> _completed = const {};
  bool _seeded = false;
  Future<void> _tail = Future.value();

  Future<void> observe(AppSnapshot snapshot) {
    final next = _tail.then((_) => _observe(snapshot));
    // A platform notification failure must not prevent a later completion
    // from being observed, nor interfere with the engine state subscription.
    _tail = next.catchError((_) {});
    return next;
  }

  Future<void> _observe(AppSnapshot snapshot) async {
    final current = <int, BigInt>{
      for (final collection in snapshot.collections)
        if (collection.role == AppCollectionRole.member &&
            collection.completedAt != null)
          collection.id: collection.completedAt!,
    };
    if (_seeded) {
      for (final collection in snapshot.collections) {
        final completedAt = current[collection.id];
        if (completedAt != null && _completed[collection.id] != completedAt) {
          await _notifier.showCompleted(
              id: collection.id, name: collection.name);
        }
      }
    }
    _completed = current;
    _seeded = true;
  }
}
