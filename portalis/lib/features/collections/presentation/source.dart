import 'package:flutter/material.dart';

import '../domain/collection.dart';
import '../domain/peer_observation.dart';
import '../domain/picked_file.dart';
import '../domain/transfer_history.dart';

/// Where a [CollectionDetail] gets its live data, and where its commands go.
///
/// [CollectionDetail] is one piece of code no matter what backs it. Every
/// rendering and interaction decision lives there; a source only answers
/// "what is the collection right now" and "where does this command land".
/// Introduced because the alternative — a second screen that happens to look
/// the same — is two implementations that will drift the first time either
/// one changes.
abstract class CollectionSource {
  const CollectionSource();

  /// Notifies whenever [resolve]'s answer, or either history, might have
  /// changed.
  Listenable get listenable;

  /// The live collection for [seed]'s id, or [seed] itself if this source
  /// currently has nothing newer to say.
  Collection resolve(Collection seed);

  TransferHistory? historyFor(String id);
  List<PeerObservation> peerHistoryFor(String id);

  Future<void> addMedia(String id, String label, List<PickedFile> files);
  Future<int> fetchMedia(String id);
  Future<void> restart(String id);
  Future<void> pause(String id);
  Future<void> delete(String id);
  Future<void> deleteWithFiles(String id);

  /// Whether the files of this collection can be chosen individually.
  ///
  /// False for a collection that owns its files — there is nothing to choose,
  /// and offering a checkbox that cannot mean anything is worse than offering
  /// none. Where it is true, [setSelection] is what a choice goes through.
  bool get supportsSelection => false;

  /// Records which entries the collection should fetch, by [MediaItem.entryId].
  ///
  /// A statement of intent, not an action: what the engine does about it is
  /// the backend's business, and the answer arrives back through [resolve]
  /// like every other fact. Sources that do not support selection never have
  /// this called, so the default simply says so.
  Future<void> setSelection(String id, Set<int> entries) async =>
      throw const SourceUnsupported(
        'this collection has no files to choose between',
      );

  /// Replaces the QR-code invite dialog for a source with no invite-code
  /// concept of its own. `null` keeps the default: the dialog every source
  /// showed before a second one existed, driven by [Collection.inviteCode].
  Future<void> Function(BuildContext context, Collection collection)?
      get showInvite => null;

  /// Called once, when the [CollectionDetail] using this source is disposed.
  /// The default needs nothing; a source holding its own subscription
  /// overrides this to cancel it.
  void dispose() {}
}

/// Raised by a source for a command it does not perform yet, so the existing
/// busy/error surfacing (`_run`, a toast of `'$error'`) says why in the
/// source's own words rather than in a generic exception's.
class SourceUnsupported implements Exception {
  const SourceUnsupported(this.message);

  final String message;

  @override
  String toString() => message;
}
