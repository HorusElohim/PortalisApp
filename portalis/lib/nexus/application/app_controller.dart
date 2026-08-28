import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';

import '../data/app_repository.dart';
import '../domain/app_state.dart';

/// Owns Portalis's one app-level Nexus state subscription.
class AppController extends ChangeNotifier {
  AppController({required AppRepository repository}) : _repository = repository;

  factory AppController.production() => AppController(
        repository: const FrbAppRepository(),
      );

  final AppRepository _repository;
  StreamSubscription<AppSnapshot>? _subscription;
  Future<void>? _starting;

  AppSnapshot? _state;
  Stream<AppDetail?>? _debugDetails;
  Stream<Uint8List>? _debugHistory;
  List<AppCollectionPeer>? _debugPeers;
  AppSnapshot? get state => _state;

  /// What the engine is doing, derived from the one state this controller
  /// owns. Every piece of chrome reads this rather than counting for itself
  /// — see [EngineActivity] for why that matters.
  EngineActivity get activity {
    final collections = _state?.collections;
    if (collections == null || collections.isEmpty) return EngineActivity.idle;
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
    return EngineActivity(
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

  Future<AppAccepted> send(EngineCommand command) => _repository.send(command);

  Future<String?> shareUri(int collection) => _repository.shareUri(collection);

  /// Selects the one collection whose expensive detail projection is needed.
  /// Widgets consume this stream directly; it deliberately does not become a
  /// second app-level cache beside [state].
  Stream<AppDetail?> watchDetail(int? collection) =>
      _debugDetails ?? _repository.watchDetail(collection);

  /// The readings a subscriber has not seen yet — see [AppRepository].
  Stream<Uint8List> watchHistory(int collection) =>
      _debugHistory ?? _repository.watchHistory(collection);

  /// Every live swarm connection, polled by the one screen that shows them.
  ///
  /// Seeded controllers answer from [debugSeed] rather than reaching the
  /// bridge, for the same reason the history stream does: a widget test must
  /// not discover the native library is missing because something subscribed.
  Future<List<AppCollectionPeer>> peers() async {
    final seeded = _debugPeers;
    if (seeded != null) return seeded;
    try {
      return await _repository.peers();
    } catch (error) {
      // Recorded, not announced. This is a poll for one screen, and notifying
      // every listener from it rebuilds the whole app — including, if the poll
      // began during a build, the widget currently being built.
      lastError = '$error';
      return const [];
    }
  }

  /// Seeds the projection for widgets that exercise app composition without a
  /// native runtime. Production state always arrives through [watchStates].
  Future<List<AppPeoplePeer>> peoplePeers() async {
    final seeded = _debugPeers;
    if (seeded != null) {
      return [
        for (final entry in seeded)
          AppPeoplePeer(
            peer: entry.peer,
            collections: Uint32List.fromList([entry.collection]),
          ),
      ];
    }
    try {
      return await _repository.peoplePeers();
    } catch (error) {
      lastError = '$error';
      return const [];
    }
  }

  Future<List<AppPeerHistory>> peerHistory(int collection) async {
    try {
      return await _repository.peerHistory(collection);
    } catch (error) {
      lastError = '$error';
      return const [];
    }
  }

  /// Seeds the projection for widgets that exercise app composition without a
  /// native runtime. Production state always arrives through [watchStates].
  @visibleForTesting
  void debugSeed(
    AppSnapshot? state, {
    String? error,
    Stream<AppDetail?>? details,
    Stream<Uint8List>? history,
    List<AppCollectionPeer>? peers,
  }) {
    _state = state;
    lastError = error;
    _debugDetails = details;
    // Seeded means offline: a controller standing in for the runtime must not
    // reach the bridge for anything, or a widget test discovers the native
    // library is missing at the moment something happens to subscribe.
    _debugHistory = history ?? const Stream<Uint8List>.empty();
    _debugPeers = peers ?? const [];
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
