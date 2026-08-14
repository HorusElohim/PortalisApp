import 'dart:async';

import 'package:flutter/foundation.dart';

import '../data/nexus_app_repository.dart';
import '../domain/nexus_app_state.dart';

/// Owns Portalis's one app-level Nexus state subscription.
class NexusAppController extends ChangeNotifier {
  NexusAppController({required NexusAppRepository repository})
      : _repository = repository;

  factory NexusAppController.production() => NexusAppController(
        repository: const FrbNexusAppRepository(),
      );

  final NexusAppRepository _repository;
  StreamSubscription<NexusAppState>? _subscription;
  Future<void>? _starting;

  NexusAppState? _state;
  Stream<NexusDetail?>? _debugDetails;
  NexusAppState? get state => _state;

  /// What the engine is doing, derived from the one state this controller
  /// owns. Every piece of chrome reads this rather than counting for itself
  /// — see [NexusActivity] for why that matters.
  NexusActivity get activity {
    final collections = _state?.collections;
    if (collections == null || collections.isEmpty) return NexusActivity.idle;
    var transfers = 0;
    var down = 0;
    var up = 0;
    var peers = 0;
    for (final collection in collections) {
      final transfer = collection.transfer;
      if (transfer == null) continue;
      transfers++;
      down += transfer.downBytesPerSecond;
      up += transfer.upBytesPerSecond;
      peers += transfer.peers;
    }
    return NexusActivity(
      transfers: transfers,
      downBytesPerSecond: down,
      upBytesPerSecond: up,
      peers: peers,
    );
  }
  String? lastError;
  bool get started => _starting != null;

  /// Opens the runtime and subscribes once. Repeated starts share the same
  /// future, which keeps hot reload and shell remounts from duplicating a
  /// native stream.
  Future<void> start() => _starting ??= _start();

  Future<void> _start() async {
    try {
      await _repository.start();
      _subscription = _repository.watchStates().listen(
        (state) {
          _state = state;
          lastError = null;
          notifyListeners();
        },
        onError: (Object error, StackTrace _) {
          lastError = '$error';
          notifyListeners();
        },
      );
    } catch (error) {
      lastError = '$error';
      _starting = null;
      notifyListeners();
      rethrow;
    }
  }

  Future<void> setActive(bool active) async {
    try {
      await _repository.setActive(active);
    } catch (error) {
      lastError = '$error';
      notifyListeners();
    }
  }

  Future<NexusAccepted> send(NexusCommand command) => _repository.send(command);

  /// Selects the one collection whose expensive detail projection is needed.
  /// Widgets consume this stream directly; it deliberately does not become a
  /// second app-level cache beside [state].
  Stream<NexusDetail?> watchDetail(int? collection) =>
      _debugDetails ?? _repository.watchDetail(collection);

  /// Seeds the projection for widgets that exercise app composition without a
  /// native runtime. Production state always arrives through [watchStates].
  @visibleForTesting
  void debugSeed(
    NexusAppState? state, {
    String? error,
    Stream<NexusDetail?>? details,
  }) {
    _state = state;
    lastError = error;
    _debugDetails = details;
    notifyListeners();
  }

  Future<void> stop() async {
    final wasStarted = _starting != null || _subscription != null;
    final subscription = _subscription;
    _subscription = null;
    await subscription?.cancel();
    if (!wasStarted) return;
    await _repository.stop();
    _starting = null;
  }

  @override
  void dispose() {
    unawaited(stop());
    super.dispose();
  }
}
