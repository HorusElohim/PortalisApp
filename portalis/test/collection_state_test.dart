import 'test_support.dart';
import 'package:portalis/features/collections/domain/collection_state.dart';

/// The backend sends one word per collection and the interface decides what to
/// draw from it. Those words are a shipped contract — see
/// `projection/wire.rs` for the half that produces them — so this pins the
/// half that reads them.
void main() {
  group('collection state contract', () {
    /// Written out literally rather than derived from `wire`, so a rename has
    /// to be made twice on purpose instead of agreeing with itself.
    test('every word the backend sends parses to its own state', () {
      const contract = {
        'Available': CollectionState.available,
        'Seeding': CollectionState.seeding,
        'Paused': CollectionState.paused,
        'Draft': CollectionState.draft,
        'Preparing': CollectionState.preparing,
        'Downloading': CollectionState.downloading,
        'Updating': CollectionState.updating,
        'WaitingForOwner': CollectionState.waitingForOwner,
        'AccessRemoved': CollectionState.accessRemoved,
        'NeedsNewerVersion': CollectionState.needsNewerVersion,
        'CannotVerify': CollectionState.cannotVerify,
        'ConflictingHistory': CollectionState.conflictingHistory,
      };

      contract.forEach((word, state) {
        expect(CollectionState.parse(word), state, reason: word);
        expect(state.wire, word, reason: 'round trip for $word');
      });
    });

    /// Every state except [CollectionState.unknown] must have a word, or the
    /// backend could never produce it.
    test('only the unknown state has no word of its own', () {
      for (final state in CollectionState.values) {
        expect(
          state.wire,
          state == CollectionState.unknown ? isNull : isNotNull,
          reason: '$state',
        );
      }
    });

    /// The bug this replaced. Three sites compared against these two strings,
    /// which the backend has never sent, so every one of them was silently
    /// false — a progress badge that never appeared and an empty-state message
    /// that was always the wrong one.
    test('a word the backend never sends is unknown, not a real state', () {
      expect(CollectionState.parse('downloading'), CollectionState.unknown,
          reason: 'case matters — the backend sends Downloading');
      expect(CollectionState.parse('importing'), CollectionState.unknown,
          reason: 'never a status the backend has emitted');
      expect(CollectionState.parse(''), CollectionState.unknown);
    });

    /// A newer backend saying something this build has never heard of shows
    /// the word itself, which is more use than a placeholder.
    test('an unknown word is still shown to the person', () {
      expect(CollectionState.unknown.label('SomethingNew'), 'SOMETHINGNEW');
      expect(
        CollectionState.waitingForOwner.label('WaitingForOwner'),
        'WAITING FOR OWNER',
      );
    });

    test('nature and role parse the same way', () {
      expect(CollectionNature.parse('Torrent'), CollectionNature.torrent);
      expect(CollectionNature.parse('Native'), CollectionNature.native);
      expect(CollectionNature.parse('torrent'), CollectionNature.unknown);
      expect(CollectionRole.parse('Owner'), CollectionRole.owner);
      expect(CollectionRole.parse('Member'), CollectionRole.member);
      expect(CollectionRole.parse('owner'), CollectionRole.unknown);
    });
  });

  group('collection reads its state through the contract', () {
    test('a downloading collection reports itself as downloading', () {
      final collection = buildCollection(status: 'Downloading');

      expect(collection.isDownloading, isTrue);
      expect(collection.lifecycle, CollectionState.downloading);
      expect(collection.isComplete, isFalse);
    });

    /// An owner seeding its own zero-copy source is complete without ever
    /// having been `Available` — see `status_for` in the backend.
    test('a seeding owner is complete', () {
      final collection = buildCollection(status: 'Seeding');

      expect(collection.isSeeding, isTrue);
      expect(collection.isComplete, isTrue);
    });

    test('a word this build cannot interpret answers no to every question', () {
      final collection = buildCollection(status: 'SomethingNewerBackendsSay');

      expect(collection.lifecycle, CollectionState.unknown);
      expect(collection.isDownloading, isFalse);
      expect(collection.isPaused, isFalse);
      expect(collection.isDraft, isFalse);
      expect(collection.isComplete, isFalse);
      // And it is still legible, rather than being dropped on the floor.
      expect(collection.state, 'SomethingNewerBackendsSay');
    });
  });
}
