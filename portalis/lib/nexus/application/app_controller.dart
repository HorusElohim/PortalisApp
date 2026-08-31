import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';

import '../data/app_repository.dart';
import '../domain/app_state.dart';
import '../../notifications/transfer_completion_notifier.dart';

/// Owns Portalis's one app-level Nexus state subscription.
class AppController extends ChangeNotifier {
  AppController({
    required AppRepository repository,
    TransferCompletionNotifier? completionNotifier,
  })  : _repository = repository,
        _completionObserver = completionNotifier == null
            ? null
            : TransferCompletionObserver(completionNotifier);

  factory AppController.production(
          {TransferCompletionNotifier? completionNotifier}) =>
      AppController(
        repository: const FrbAppRepository(),
        completionNotifier: completionNotifier,
      );

  final AppRepository _repository;
  final TransferCompletionObserver? _completionObserver;
  StreamSubscription<AppSnapshot>? _subscription;
  Future<void>? _starting;

  AppSnapshot? _state;
  Stream<AppDetail?>? _debugDetails;
  Stream<Uint8List>? _debugHistory;
  List<AppCollectionPeer>? _debugPeers;
  AppUserSummary? _debugUserSummary;
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
          final observer = _completionObserver;
          if (observer != null) {
            unawaited(observer.observe(state).catchError((_) {}));
          }
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

  /// The local diagnostics log, oldest line first. Never leaves the device
  /// on its own — see [AppRepository.diagnosticsLog].
  Future<String> diagnosticsLog() => _repository.diagnosticsLog();

  Future<void> clearDiagnosticsLog() => _repository.clearDiagnosticsLog();

  Future<String> diagnosticsLogPath() => _repository.diagnosticsLogPath();

  /// Appends one line to the same diagnostics log the native backend
  /// writes to. Used by [runPortalisApp]'s global error handlers, so a
  /// Flutter-side crash lands in the one report a person shares rather than
  /// only on a console nobody is attached to in a release build.
  Future<void> logDiagnostic(String tag, String message) =>
      _repository.logDiagnostic(tag, message);

  /// This device's own locally measured activity: current run, lifetime
  /// counters, library facts, and bounded recent runs. Backend-owned and
  /// on-demand — this controller never aggregates it locally.
  Future<AppUserSummary> userSummary() async {
    final seeded = _debugUserSummary;
    if (seeded != null) return seeded;
    return _repository.userSummary();
  }

  /// Clears only durable device activity and bounded run history. Identity,
  /// collections, and settings are never touched.
  Future<void> clearUserActivity() => _repository.clearUserActivity();

  /// Renames this device through the one canonical path: the backend updates
  /// the persisted identity and the live [state] together, so there is
  /// nothing left here to separately reload or drift out of sync with — see
  /// ADR-0011 decision #11. The next [watchStates] emission (or the debug
  /// seed in a test) carries the new name.
  Future<void> renameDevice(String nickname) =>
      _repository.renameDevice(nickname);

  /// Seeds the projection for widgets that exercise app composition without a
  /// native runtime. Production state always arrives through [watchStates].
  @visibleForTesting
  void debugSeed(
    AppSnapshot? state, {
    String? error,
    Stream<AppDetail?>? details,
    Stream<Uint8List>? history,
    List<AppCollectionPeer>? peers,
    AppUserSummary? userSummary,
  }) {
    _state = state;
    lastError = error;
    _debugDetails = details;
    // Seeded means offline: a controller standing in for the runtime must not
    // reach the bridge for anything, or a widget test discovers the native
    // library is missing at the moment something happens to subscribe.
    _debugHistory = history ?? const Stream<Uint8List>.empty();
    _debugPeers = peers ?? const [];
    _debugUserSummary = userSummary ?? _emptyUserSummary;
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

/// The debug default for a seeded controller: an honest all-zero summary
/// rather than a null that would make every widget's loading state stick
/// forever in a test that never seeds one explicitly.
final _emptyUserSummary = AppUserSummary(
  device: const AppDevice(
    name: 'Portalis',
    handle: null,
    fingerprint: 'test-fingerprint',
    devices: 1,
  ),
  trackedSince: BigInt.zero,
  currentRun: AppAppRun(
    runId: BigInt.one,
    startedAt: BigInt.zero,
    engineRunningNs: BigInt.zero,
    foregroundNs: BigInt.zero,
    networkDownBytes: BigInt.zero,
    networkUpBytes: BigInt.zero,
    completedDownloads: BigInt.zero,
    peakDownBytesPerSecond: 0,
    peakUpBytesPerSecond: 0,
    endReason: 'current',
  ),
  runsStarted: BigInt.one,
  runsCompletedCleanly: BigInt.zero,
  runsInterrupted: BigInt.zero,
  lifetimeEngineRunningNs: BigInt.zero,
  lifetimeForegroundNs: BigInt.zero,
  lifetimeNetworkDownBytes: BigInt.zero,
  lifetimeNetworkUpBytes: BigInt.zero,
  lifetimeCompletedDownloads: BigInt.zero,
  lifetimePeakDownBytesPerSecond: 0,
  lifetimePeakUpBytesPerSecond: 0,
  lastActivityAt: BigInt.zero,
  lastCleanShutdownAt: BigInt.zero,
  collectionsOwned: 0,
  collectionsReceived: 0,
  entriesTotal: 0,
  catalogBytes: BigInt.zero,
  heldBytes: BigInt.zero,
  verifiedContacts: 0,
  unverifiedContacts: 0,
  connectivity: 'LocalOnly',
  recentRuns: const [],
);
