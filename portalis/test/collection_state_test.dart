import 'test_support.dart';
import 'package:portalis/features/collections/domain/collection_state.dart';

void main() {
  group('typed collection state contract', () {
    test('generated lifecycle values are formatted for people, not parsed', () {
      expect(
        AppCollectionLifecycle.waitingForOwner.label('WaitingForOwner'),
        'WAITING FOR OWNER',
      );
      expect(
        AppCollectionLifecycle.downloading.label('Downloading'),
        'DOWNLOADING',
      );
    });

    test('a downloading collection reads Rust lifecycle and facts directly',
        () {
      final collection = buildCollection(status: 'Downloading');

      expect(collection.lifecycle, AppCollectionLifecycle.downloading);
      expect(collection.isDownloading, isTrue);
      expect(collection.isComplete, isFalse);
      expect(collection.source.facts.moving, isTrue);
    });

    test('a seeding owner is complete from the Rust fact', () {
      final collection = buildCollection(status: 'Seeding');

      expect(collection.lifecycle, AppCollectionLifecycle.seeding);
      expect(collection.isSeeding, isTrue);
      expect(collection.isComplete, isTrue);
      expect(collection.source.facts.complete, isTrue);
    });

    test('metadata preparation and selection capabilities come from Rust', () {
      final resolving = buildCollection(
        nature: 'Torrent',
        status: 'ResolvingMetadata',
      );
      final ready = buildCollection(
        nature: 'Torrent',
        status: 'MetadataReady',
      );

      expect(resolving.isPreparing, isTrue);
      expect(resolving.source.facts.preparing, isTrue);
      expect(resolving.source.capabilities.canSelect, isFalse);
      expect(ready.hasMetadata, isTrue);
      expect(ready.source.capabilities.canSelect, isTrue);
    });
  });
}
