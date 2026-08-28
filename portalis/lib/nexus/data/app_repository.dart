import 'dart:typed_data';

import '../bridge/portalis_api.dart' as bridge;
import '../domain/app_state.dart';

/// The native contract the application consumes.
///
/// An interface rather than the bridge functions themselves, because that is
/// what lets a widget test substitute the engine — every test in this project
/// implements this rather than faking FFI. The projection types it returns are
/// the generated ones: mirroring them by hand bought nothing and could lose a
/// field in silence. See `domain/app_state.dart`.
abstract interface class AppRepository {
  Future<void> start();
  Future<void> stop();
  Future<void> setActive(bool active);
  Stream<AppSnapshot> watchStates();
  Stream<AppDetail?> watchDetail(int? collection);
  Future<String?> shareUri(int collection);

  /// One collection's readings, as they are recorded.
  ///
  /// Arrives as the rows a subscriber has not seen yet, not as the whole ring
  /// — the history only grows at the end, and re-sending all of it to append
  /// one row was thirty kilobytes a second for a screen already showing it.
  /// Whoever subscribes accumulates.
  Stream<Uint8List> watchHistory(int collection);

  /// Every live swarm connection, across all collections.
  ///
  /// A call rather than a stream field: peers change every poll, and carrying
  /// them in the snapshot would rewrite every collection list once a second
  /// for one screen's benefit.
  Future<List<AppCollectionPeer>> peers();
  Future<List<AppPeoplePeer>> peoplePeers();
  Future<List<AppPeerHistory>> peerHistory(int collection);
  Future<AppAccepted> send(EngineCommand command);
}

class FrbAppRepository implements AppRepository {
  const FrbAppRepository();

  @override
  Future<void> start() => bridge.start();

  @override
  Future<void> stop() => bridge.stop();

  @override
  Future<void> setActive(bool active) => bridge.setActive(active: active);

  @override
  Stream<AppSnapshot> watchStates() => bridge.watchStates();

  @override
  Stream<AppDetail?> watchDetail(int? collection) =>
      bridge.watchDetail(collection: collection);

  @override
  Future<String?> shareUri(int collection) =>
      bridge.shareUri(collection: collection);

  @override
  Stream<Uint8List> watchHistory(int collection) =>
      bridge.watchHistory(collection: collection);

  @override
  Future<List<AppCollectionPeer>> peers() => bridge.peers();

  @override
  Future<List<AppPeoplePeer>> peoplePeers() => bridge.peoplePeers();

  @override
  Future<List<AppPeerHistory>> peerHistory(int collection) =>
      bridge.peerHistory(collection: collection);

  @override
  Future<AppAccepted> send(EngineCommand command) =>
      bridge.send(command: command.toBridge());
}
