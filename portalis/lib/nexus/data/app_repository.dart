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
  Future<AppAccepted> createCollection(String name, List<AppSourceFile> files);
  Future<AppAccepted> addMedia(
    int collection,
    String label,
    List<AppSourceFile> files,
  );
  Future<AppAccepted> renameCollection(int collection, String name);
  Future<AppAccepted> deleteCollection(int collection, bool deleteFiles);
  Future<AppAccepted> setCollectionPaused(int collection, bool paused);
  Future<AppAccepted> publishDraft(int collection);
  Future<AppAccepted> importTorrent(String source);
  Future<AppAccepted> downloadSelection(int collection, List<int> entries);

  /// The local diagnostics log — see `rust/backend/src/nexus/diagnostics.rs`.
  /// Never transmitted anywhere on its own; a person's own choice to share
  /// or clear it.
  Future<String> diagnosticsLog();
  Future<void> clearDiagnosticsLog();
  Future<String> diagnosticsLogPath();
  Future<void> logDiagnostic(String tag, String message);

  /// This device's own locally measured activity: current run, lifetime
  /// counters, library facts, and bounded recent runs. Backend-owned and
  /// on-demand — Flutter renders it, it never aggregates it locally.
  Future<AppUserSummary> userSummary();

  /// Clears only durable device activity and bounded run history. Identity,
  /// collections, and settings are never touched.
  Future<void> clearUserActivity();

  /// Renames this device. Updates the persisted identity and the live
  /// [AppSnapshot.device] together — see ADR-0011 decision #11.
  Future<void> renameDevice(String nickname);

  /// A typed fact each time a receiver-side transfer completes — see
  /// [AppTransferCompleted] for why this replaces diffing successive
  /// snapshots (ADR-0016).
  Stream<AppTransferCompleted> watchTransferCompletions();
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
  Future<AppAccepted> createCollection(
    String name,
    List<AppSourceFile> files,
  ) =>
      bridge.createCollection(name: name, files: files);

  @override
  Future<AppAccepted> addMedia(
    int collection,
    String label,
    List<AppSourceFile> files,
  ) =>
      bridge.addMedia(collection: collection, label: label, files: files);

  @override
  Future<AppAccepted> renameCollection(int collection, String name) =>
      bridge.renameCollection(collection: collection, name: name);

  @override
  Future<AppAccepted> deleteCollection(int collection, bool deleteFiles) =>
      bridge.deleteCollection(
        collection: collection,
        deleteFiles: deleteFiles,
      );

  @override
  Future<AppAccepted> setCollectionPaused(int collection, bool paused) =>
      bridge.setCollectionPaused(collection: collection, paused: paused);

  @override
  Future<AppAccepted> publishDraft(int collection) =>
      bridge.publishDraft(collection: collection);

  @override
  Future<AppAccepted> importTorrent(String source) =>
      bridge.importTorrent(source: source);

  @override
  Future<AppAccepted> downloadSelection(int collection, List<int> entries) =>
      bridge.downloadSelection(collection: collection, entries: entries);

  @override
  Future<String> diagnosticsLog() => bridge.diagnosticsLog();

  @override
  Future<void> clearDiagnosticsLog() => bridge.clearDiagnosticsLog();

  @override
  Future<String> diagnosticsLogPath() => bridge.diagnosticsLogPath();

  @override
  Future<void> logDiagnostic(String tag, String message) =>
      bridge.logDiagnostic(tag: tag, message: message);

  @override
  Future<AppUserSummary> userSummary() => bridge.userSummary();

  @override
  Future<void> clearUserActivity() => bridge.clearUserActivity();

  @override
  Future<void> renameDevice(String nickname) =>
      bridge.renameDevice(nickname: nickname);

  @override
  Stream<AppTransferCompleted> watchTransferCompletions() =>
      bridge.watchTransferCompletions();
}
