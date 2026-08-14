import 'dart:async';

import 'package:flutter/foundation.dart';

import '../../features/collections/domain/collection.dart';
import '../../features/collections/domain/peer_observation.dart';
import '../../features/collections/domain/picked_file.dart';
import '../../features/collections/domain/transfer_history.dart';
import '../../features/collections/presentation/source.dart';
import '../application/app_controller.dart';
import '../domain/app_state.dart';
import 'collection_view.dart';

/// Pauses or resumes one collection.
///
/// "Restart" resumes — the same thing it meant against the legacy backend's
/// `restart_collection`, not a retry of a stalled transfer. A free function
/// rather than a method on [EngineCollectionSource] because [Home]'s inline
/// row commands need the exact same dispatch without paying for a source's
/// subscription just to fire one command.
Future<void> sendSetPaused(
  AppController controller,
  int collectionId, {
  required bool paused,
}) =>
    controller.send(
      EngineCommand(kind: 'setPaused', collection: collectionId, paused: paused),
    );

/// Deletes one collection, optionally taking its downloaded files with it.
Future<void> sendDeleteCollection(
  AppController controller,
  int collectionId, {
  required bool deleteFiles,
}) =>
    controller.send(
      EngineCommand(
        kind: 'deleteCollection',
        collection: collectionId,
        deleteFiles: deleteFiles,
      ),
    );

/// Feeds [CollectionDetail] from the Nexus core instead of the legacy
/// collections controller.
///
/// Subscribes once, for as long as this source lives, to the one per-
/// collection detail stream Nexus offers — entries, peers, the piece map, the
/// transfer history — and merges each reading with the controller's own
/// list-level state. [CollectionDetail] never sees any of that; through
/// [resolve] it only ever reads the same plain [Collection] the legacy
/// controller has always handed it.
///
/// Owns exactly one subscription, for exactly one collection, for exactly the
/// lifetime of the screen that constructed it — [dispose] must be called
/// once that screen is gone. [CollectionDetail]'s own state does this: the
/// contract every [CollectionSource] makes is that its owner calls `dispose`
/// exactly once, so this source does not also call it on itself.
class EngineCollectionSource extends CollectionSource with ChangeNotifier {
  EngineCollectionSource({required this.controller, required this.collectionId}) {
    controller.addListener(notifyListeners);
    _detailSubscription =
        controller.watchDetail(collectionId).listen((detail) {
      _detail = detail;
      notifyListeners();
    });
  }

  final AppController controller;
  final int collectionId;

  AppDetail? _detail;
  late final StreamSubscription<AppDetail?> _detailSubscription;

  @override
  Listenable get listenable => this;

  AppCollection? get _nexusCollection => controller.state?.collections
      .where((item) => item.id == collectionId)
      .firstOrNull;

  @override
  Collection resolve(Collection seed) {
    final collection = _nexusCollection;
    if (collection == null) return seed;
    return collectionView(
      collection: collection,
      detail: _detail,
      contacts: controller.state?.contacts ?? const [],
    );
  }

  @override
  TransferHistory? historyFor(String id) => transferHistory(_detail);

  @override
  List<PeerObservation> peerHistoryFor(String id) {
    final collection = _nexusCollection;
    if (collection == null) return const [];
    return peerObservations(collection: collection, detail: _detail);
  }

  @override
  Future<void> addMedia(String id, String label, List<PickedFile> files) =>
      controller.send(
        EngineCommand(
          kind: 'addMedia',
          collection: collectionId,
          label: label,
          files: [
            for (final file in files)
              AppSourceFile(
                name: file.name,
                path: file.path,
                bytes: BigInt.from(file.lengthBytes),
              ),
          ],
        ),
      );

  @override
  Future<int> fetchMedia(String id) async =>
      throw const SourceUnsupported('Fetching is not wired to Nexus yet.');

  /// Restart resumes — the same thing it meant against the legacy backend's
  /// `restart_collection`, not a retry of a stalled transfer.
  @override
  Future<void> restart(String id) =>
      sendSetPaused(controller, collectionId, paused: false);

  @override
  Future<void> pause(String id) =>
      sendSetPaused(controller, collectionId, paused: true);

  /// Only a torrent import has files to choose between: a collection this
  /// device published owns all of them, and there is nothing to fetch.
  @override
  bool get supportsSelection => _nexusCollection?.nature == 'Torrent';

  /// The same command whether or not the download has started. The core
  /// records the choice and its worker states it to the engine — beginning a
  /// download the first time, revising a running one afterwards.
  @override
  Future<void> setSelection(String id, Set<int> entries) => controller.send(
        EngineCommand(
          kind: 'downloadSelection',
          collection: collectionId,
          entries: entries.toList()..sort(),
        ),
      );

  @override
  Future<void> delete(String id) =>
      sendDeleteCollection(controller, collectionId, deleteFiles: false);

  @override
  Future<void> deleteWithFiles(String id) =>
      sendDeleteCollection(controller, collectionId, deleteFiles: true);

  // No showInvite override: Nexus has no invite-code concept, so this falls
  // through to the default's `if (code == null) return;` — the same no-op a
  // legacy shared collection with no code yet would already show. Contact-
  // based sharing needs a picker that does not exist; said here rather than
  // built badly.

  @override
  void dispose() {
    controller.removeListener(notifyListeners);
    unawaited(_detailSubscription.cancel());
    super.dispose();
  }
}
