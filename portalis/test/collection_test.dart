import 'dart:typed_data';
import 'test_support.dart';
import 'package:portalis/app/collection_link.dart';
import 'package:portalis/nexus/data/collection_view.dart';
import 'package:portalis/features/collections/presentation/share_qr.dart';

import 'package:portalis/features/collections/domain/picked_file.dart';
import 'package:portalis/features/collections/domain/transfer_history.dart';
import 'package:portalis/features/collections/presentation/source.dart';

import 'package:portalis/features/collections/domain/peer_observation.dart';
import 'package:portalis/features/collections/presentation/peers.dart';
import 'package:portalis/features/collections/presentation/peer_color.dart';

/// A source that answers with exactly what it was given.
///
/// The collection screen is one widget over a [CollectionSource]; these tests
/// are about what it *draws*, so the simplest possible source keeps them
/// about that rather than about whichever engine is behind it.
class _FixedSource extends CollectionSource with ChangeNotifier {
  _FixedSource(this.collection, {this.qrUri});

  Collection collection;
  final String? qrUri;
  final commands = <String>[];
  final shareRequests = <String>[];

  @override
  Listenable get listenable => this;

  @override
  Collection resolve(Collection seed) => collection;

  @override
  TransferHistory? historyFor(String id) => null;

  @override
  List<PeerObservation> peerHistoryFor(String id) => const [];

  @override
  Future<void> addMedia(
          String id, String label, List<PickedFile> files) async =>
      commands.add('addMedia');

  @override
  Future<int> fetchMedia(String id) async {
    commands.add('fetch');
    return 0;
  }

  @override
  Future<void> restart(String id) async => commands.add('restart');

  @override
  Future<void> pause(String id) async => commands.add('pause');

  @override
  Future<void> delete(String id) async => commands.add('delete');

  @override
  Future<void> deleteWithFiles(String id) async =>
      commands.add('deleteWithFiles');

  @override
  Future<String?> shareUri(String id) async {
    shareRequests.add(id);
    return qrUri;
  }
}

void main() {
  tearDown(resetTestState);

  group('collection', () {
    test('peer age keeps only the compact time unit', () {
      final now = DateTime(2026, 8, 11, 12);

      expect(
        formatLastSeen(now.subtract(const Duration(seconds: 4)), now: now),
        '4s',
      );
      expect(
        formatLastSeen(now.subtract(const Duration(minutes: 2)), now: now),
        '2m',
      );
      expect(
        formatLastSeen(now.subtract(const Duration(hours: 3)), now: now),
        '3h',
      );
    });

    testWidgets('a torrent labels its process-local handle honestly',
        (tester) async {
      final collection = buildCollection(nature: 'Torrent');

      await tester.pumpWidget(
        MaterialApp(
          home: CollectionScreen(
            collection: collection,
            source: _FixedSource(collection),
          ),
        ),
      );
      await tester.pump();

      expect(find.text('Local handle'), findsOneWidget);
      expect(find.text('Info hash'), findsNothing);
    });

    testWidgets('active and disconnected peers have honest distinct colours',
        (tester) async {
      final observedAt = DateTime.now().subtract(const Duration(seconds: 12));
      final collection = buildCollection(
        nature: 'Torrent',
        totalBytes: 1000,
        downloadedBytes: 250,
        torrentPeers: const ['203.0.113.5:6881'],
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: CollectionPeers(
              collection: collection,
              peerHistory: [
                PeerObservation(
                  collectionId: collection.id,
                  collectionName: collection.name,
                  address: '203.0.113.5:6881',
                  lastSeen: observedAt,
                ),
                PeerObservation(
                  collectionId: collection.id,
                  collectionName: collection.name,
                  address: '198.51.100.9:6881',
                  lastSeen: observedAt,
                ),
              ],
            ),
          ),
        ),
      );

      final active = tester.widget<Text>(find.text('203.0.113.5:6881'));
      final disconnected = tester.widget<Text>(find.text('198.51.100.9:6881'));
      expect(active.style?.color, AppColors.ember);
      expect(
        disconnected.style?.color,
        rememberedPeerColor('198.51.100.9:6881'),
      );
      expect(disconnected.style?.color, isNot(AppColors.textFaint));
      expect(find.textContaining(RegExp(r'^1[2-3]s$')), findsNWidgets(2));
      expect(find.textContaining('seen'), findsNothing);
      expect(find.textContaining('ago'), findsNothing);
      expect(
        find.byKey(const Key('collectionPeerTransferProgress')),
        findsNothing,
        reason: 'progress belongs to the transfer panel, not the peer list',
      );
      expect(find.text('COLLECTION TRANSFER'), findsNothing);
      expect(find.text('250 B of 1 KB received on this device'), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('a plain torrent offers no invite or add-media',
        (tester) async {
      // A torrent's contents are fixed by its info-hash and it has no invite
      // secret, so those actions must not appear â€” they would be dead
      // buttons.
      final collection = buildCollection(
        name: 'Some Torrent',
        nature: 'Torrent',
        status: 'Downloading',
      );
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
            home: CollectionScreen(
                collection: collection, source: _FixedSource(collection))),
      );
      await tester.pump();

      expect(find.text('Some Torrent'), findsOneWidget);
      expect(find.text('Invite'), findsNothing);
      expect(find.text('ï¼‹ Add media'), findsNothing);
      expect(find.text('Sync'), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('a completed collection offers a QR sharing action',
        (tester) async {
      final collection = buildCollection(
        name: 'Iceland trip',
        status: 'Available',
        entries: [buildEntry(label: 'waterfall.jpg', bytes: 42)],
      );
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          home: CollectionScreen(
            collection: collection,
            source: _FixedSource(collection),
          ),
        ),
      );
      await tester.pump();

      expect(find.byKey(const Key('collectionShareQr')), findsOneWidget);
      expect(find.text('Share QR'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('a paused collection with content still offers QR sharing',
        (tester) async {
      final collection = buildCollection(
        name: 'Iceland trip',
        status: 'Paused',
        entries: [buildEntry(label: 'waterfall.jpg', bytes: 42)],
      );
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          home: CollectionScreen(
            collection: collection,
            source: _FixedSource(collection),
          ),
        ),
      );
      await tester.pump();

      expect(find.byKey(const Key('collectionShareQr')), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets(
        'a restarted published collection keeps QR sharing while entries rehydrate',
        (tester) async {
      // The durable revision and torrent identity are already present after a
      // restart, while the inexpensive list projection may briefly report no
      // entries until the detail tier is rehydrated.
      final collection = buildCollection(
        name: 'Reopened trip',
        status: 'Downloading',
      );
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          home: CollectionScreen(
            collection: collection,
            source: _FixedSource(collection),
          ),
        ),
      );
      await tester.pump();

      expect(find.byKey(const Key('collectionShareQr')), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('sharing a collection renders its magnet URI as a QR code',
        (tester) async {
      const uri =
          'magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567';
      final collection = buildCollection(
        name: 'Iceland trip',
        status: 'Available',
        entries: [buildEntry(label: 'waterfall.jpg', bytes: 42)],
      );
      final source = _FixedSource(collection, qrUri: uri);
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
            home: CollectionScreen(collection: collection, source: source)),
      );
      await tester.pump();

      await tester.tap(find.byKey(const Key('collectionShareQr')));
      await pumpTransition(tester);

      expect(source.shareRequests, [collection.id]);
      expect(find.byKey(const Key('collectionShareQrDialog')), findsOneWidget);
      expect(find.byKey(const Key('collectionShareQrCode')), findsOneWidget);
      expect(
        tester
            .widget<CollectionShareQrCode>(
              find.byKey(const Key('collectionShareQrCode')),
            )
            .uri,
        collectionShareLink(uri),
      );
      expect(find.text('Scan to import Iceland trip'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('deletion is one command with collection-or-files choices',
        (tester) async {
      final collection = buildCollection(name: 'Trip archive');
      await tester.binding.setSurfaceSize(const Size(390, 1000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
            home: CollectionScreen(
                collection: collection, source: _FixedSource(collection))),
      );
      await tester.pump();

      expect(find.text('Forget'), findsNothing);
      expect(find.text('Delete files'), findsNothing);
      await tester.tap(find.byKey(const Key('collectionCommanddelete')));
      await pumpTransition(tester);

      expect(find.text('Delete "Trip archive"?'), findsOneWidget);
      expect(find.text('Cancel'), findsOneWidget);
      expect(find.byKey(const Key('deleteCollectionOnly')), findsOneWidget);
      expect(
        find.byKey(const Key('deleteCollectionWithFiles')),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    });

    testWidgets('the viewer carries its own details and stays live',
        (tester) async {
      // Reading a file's size used to cost two taps and a screen transition,
      // and the screen it landed on held a snapshot taken when the tile was
      // tapped â€” so the numbers stopped moving exactly when they mattered.
      const media = MediaItem(
        label: 'clip.mp4',
        entryLabel: 'Beach day',
        localPath: 'C:/Media/clip.mp4',
        sizeBytes: 1000,
        downloadedBytes: 400,
        progress: 0.4,
      );
      final collection = buildCollection(
        status: 'Downloading',
        entries: [
          buildEntry(
            label: media.label,
            bytes: 1000,
            downloadedBytes: 400,
            path: 'C:/Media/clip.mp4',
          ),
        ],
        totalBytes: 1000,
        downloadedBytes: 400,
        downBytesPerSecond: 2,
        livePeers: 3,
      );
      final source = _FixedSource(collection);
      await tester.binding.setSurfaceSize(const Size(390, 1000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(MaterialApp(
        home: MediaViewerScreen(
          collection: collection,
          media: media,
          source: source,
        ),
      ));
      await tester.pump();

      // On screen without asking: how much of it is here.
      expect(find.textContaining('400 B of 1 KB'), findsOneWidget);
      expect(find.textContaining('3 peers'), findsOneWidget);

      // Details are part of the viewer itself; no second tap or route is
      // needed to inspect the media metadata.
      expect(find.text('File path'), findsOneWidget);
      expect(find.byType(MediaViewerScreen), findsOneWidget);

      // And it follows its source rather than the arguments it was built
      // with: the same property the controller cache used to provide, now
      // expressed through the seam every collection screen shares.
      source.collection = buildCollection(
        status: 'Downloading',
        entries: [
          buildEntry(
            label: 'clip.mp4',
            bytes: 1000,
            downloadedBytes: 900,
            available: false,
          ),
        ],
        torrentPeers: const ['203.0.113.5:6881'],
        totalBytes: 1000,
        downloadedBytes: 900,
        downBytesPerSecond: 2,
        livePeers: 3,
      );
      source.notifyListeners();
      await tester.pump();

      expect(find.textContaining('900 B of 1 KB'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('identifiers are a disclosure, not a destination',
        (tester) async {
      // They were a pushed screen whose only remaining content was a type, a
      // state and an id â€” everything on it that moved is now on the screen
      // itself.
      final collection = buildCollection(status: 'Available');
      await tester.binding.setSurfaceSize(const Size(390, 1000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(MaterialApp(
        home: CollectionScreen(
            collection: collection, source: _FixedSource(collection)),
      ));
      await tester.pump();

      expect(find.text('Collection id'), findsOneWidget);
      expect(find.byTooltip('Details'), findsNothing);
      expect(find.byType(CollectionScreen), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('shows anonymous torrent peer addresses in the collection',
        (tester) async {
      final collection = buildCollection(
        nature: 'Torrent',
        torrentPeers: const ['203.0.113.5:6881'],
        livePeers: 1,
      );
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
            home: CollectionScreen(
                collection: collection, source: _FixedSource(collection))),
      );
      await tester.pump();

      expect(find.text('203.0.113.5:6881'), findsOneWidget);
      expect(find.text('PEERS - 1'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    test('reports how far along it is, not merely whether it finished', () {
      final view = collectionView(
        collection: buildNexusCollection(id: 1, status: 'Downloading'),
        detail: AppDetail(
          id: 1,
          entries: [
            AppEntry(
              id: 1,
              label: 'episode.mkv',
              bytes: BigInt.from(1000),
              selected: true,
              available: false,
              downloadedBytes: BigInt.from(450),
            ),
            AppEntry(
              id: 2,
              label: 'extras.mkv',
              bytes: BigInt.from(1000),
              selected: true,
              available: true,
              downloadedBytes: BigInt.from(1000),
              path: '/downloads/extras.mkv',
            ),
          ],
          pieces: Uint8List(0),
          peers: const [],
        ),
        contacts: const [],
      );

      final partial = view.media.first;
      expect(partial.progress, closeTo(0.45, 0.001));
      expect(partial.downloadedBytes, 450);
      expect(partial.fetched, isTrue, reason: 'bytes are arriving');
      // A torrent's pieces land out of order, so a path to a half-written file
      // would preview as breakage rather than as an honest placeholder.
      expect(partial.localPath, isNull);
      expect(partial.isReady, isFalse);

      final whole = view.media.last;
      expect(whole.progress, 1);
      expect(whole.localPath, '/downloads/extras.mkv');
      expect(whole.isReady, isTrue);
    });

    test('a file nothing has arrived for claims nothing', () {
      final view = collectionView(
        collection: buildNexusCollection(id: 1, status: 'Downloading'),
        detail: AppDetail(
          id: 1,
          entries: [
            AppEntry(
              id: 1,
              label: 'queued.mkv',
              bytes: BigInt.from(1000),
              selected: true,
              available: false,
              downloadedBytes: BigInt.zero,
            ),
          ],
          pieces: Uint8List(0),
          peers: const [],
        ),
        contacts: const [],
      );

      expect(view.media.single.progress, 0);
      expect(view.media.single.fetched, isFalse);
    });
  });
}
