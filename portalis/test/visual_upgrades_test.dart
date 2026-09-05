// Focused widget tests for the point-2/3/4/5/8 visual upgrades:
// - torrent row tile shows a format-aware icon instead of one static glyph
// - a moving peer connection pulses, an idle one stays static
// - the transfer graph reveals the exact point under a press
// - paused / attention-needing lifecycles get a distinguishable status badge
// - the media grid uses larger tiles once the layout is wide

import 'test_support.dart';
import 'package:portalis/features/collections/presentation/views.dart';
import 'package:portalis/features/collections/presentation/peers.dart';
import 'package:portalis/features/collections/domain/peer_observation.dart';
import 'package:portalis/features/media/presentation/grid.dart';
import 'package:portalis/features/media/presentation/piece_frame.dart';

void main() {
  tearDown(resetTestState);

  test('decodes packed progress buckets into sparse perimeter segments', () {
    final packed = List<int>.filled(16, 0);
    for (var bucket = 0; bucket < 16; bucket++) {
      packed[bucket ~/ 4] |= 1 << ((bucket % 4) * 2);
    }
    for (var bucket = 32; bucket < 48; bucket++) {
      packed[bucket ~/ 4] |= 2 << ((bucket % 4) * 2);
    }

    expect(
      progressSegmentsForBuckets(packed),
      [
        const PerimeterSegment(start: 0, extent: 0.25),
        const PerimeterSegment(
          start: 0.5,
          extent: 0.25,
          active: true,
          workerCount: 1,
        ),
      ],
    );
  });

  group('torrent row tile', () {
    testWidgets('shows a format-aware icon once metadata names video files',
        (tester) async {
      final collection = buildCollection(
        nature: 'Torrent',
        status: 'Downloading',
        entries: [
          buildEntry(label: 'episode1.mp4', bytes: 100),
          buildEntry(label: 'episode2.mkv', bytes: 100),
        ],
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: CollectionRow(collection: collection, onTap: () {}),
          ),
        ),
      );

      expect(find.byIcon(Icons.movie_outlined), findsOneWidget);
      expect(find.byIcon(Icons.download_outlined), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('falls back to a generic glyph before metadata resolves',
        (tester) async {
      final collection = buildCollection(
        nature: 'Torrent',
        status: 'ResolvingMetadata',
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: CollectionRow(collection: collection, onTap: () {}),
          ),
        ),
      );

      expect(find.byIcon(Icons.download_outlined), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  });

  group('peer cards', () {
    testWidgets('a moving connection pulses and an idle one stays static',
        (tester) async {
      final collection = buildCollection(nature: 'Torrent');
      final moving = PeerObservation(
        collectionId: collection.id,
        collectionName: collection.name,
        address: '203.0.113.5:6881',
        lastSeen: DateTime.now(),
        downBytesPerSecond: 512000,
      );
      final idle = PeerObservation(
        collectionId: collection.id,
        collectionName: collection.name,
        address: '198.51.100.9:6881',
        lastSeen: DateTime.now(),
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: CollectionPeers(
              collection: collection,
              peerHistory: [moving, idle],
            ),
          ),
        ),
      );

      // One pulsing dot for the moving connection, none for the idle one.
      expect(find.byType(LiveDot), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  });

  group('transfer graph detail', () {
    testWidgets('pressing the chart reveals the exact point under it',
        (tester) async {
      final start = DateTime(2026, 8, 10, 19, 39, 20);
      final end = start.add(const Duration(seconds: 10));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 900,
              child: TransferGraph(
                progress: 0.5,
                downBytesPerSecond: 0,
                upBytesPerSecond: 0,
                startedAt: start,
                history: [
                  TransferPoint(
                    at: start,
                    downBytesPerSecond: 1000000,
                    upBytesPerSecond: 0,
                  ),
                  TransferPoint(
                    at: end,
                    downBytesPerSecond: 3000000,
                    upBytesPerSecond: 0,
                  ),
                ],
              ),
            ),
          ),
        ),
      );

      expect(find.byKey(const Key('transferGraphTooltip')), findsNothing);

      final chart = find.byKey(const Key('transferGraphChart'));
      expect(chart, findsOneWidget);
      final topLeft = tester.getTopLeft(chart);
      final size = tester.getSize(chart);
      // Press near the right edge, closest to the 3.0 MB/s sample.
      final gesture = await tester.startGesture(
        topLeft + Offset(size.width - 4, size.height / 2),
      );
      await tester.pump();

      expect(find.byKey(const Key('transferGraphTooltip')), findsOneWidget);
      expect(find.text('3.0 MB/s down'), findsOneWidget);

      await gesture.up();
      await tester.pump();
      expect(find.byKey(const Key('transferGraphTooltip')), findsNothing);
      expect(tester.takeException(), isNull);
    });
  });

  group('status badges', () {
    testWidgets('a paused torrent shows a pause glyph on its badge',
        (tester) async {
      final collection = buildCollection(nature: 'Torrent', status: 'Paused');

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: CollectionRow(collection: collection, onTap: () {}),
          ),
        ),
      );

      expect(find.text('PAUSED'), findsOneWidget);
      expect(find.byIcon(Icons.pause), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('a collection needing attention shows a danger badge',
        (tester) async {
      final collection = buildCollection(
        nature: 'Torrent',
        status: 'AccessRemoved',
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: CollectionRow(collection: collection, onTap: () {}),
          ),
        ),
      );

      expect(find.text('ACCESS REMOVED'), findsOneWidget);
      expect(find.byIcon(Icons.error_outline), findsOneWidget);
      final badge = tester.widget<StatusBadge>(find.byType(StatusBadge));
      expect(badge.color, AppColors.danger);
      expect(tester.takeException(), isNull);
    });
  });

  group('media grid density', () {
    testWidgets('tiles grow on a wide layout and stay compact on a narrow one',
        (tester) async {
      const media = [
        MediaItem(label: 'a.jpg', sizeBytes: 10),
        MediaItem(label: 'b.jpg', sizeBytes: 10),
      ];

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 900,
              child: MediaGrid(
                  media: media, color: Colors.teal, onOpenMedia: (_) {}),
            ),
          ),
        ),
      );
      final wideDelegate = tester
          .widget<GridView>(find.byType(GridView))
          .gridDelegate as SliverGridDelegateWithMaxCrossAxisExtent;
      expect(wideDelegate.maxCrossAxisExtent, greaterThan(84));

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 390,
              child: MediaGrid(
                  media: media, color: Colors.teal, onOpenMedia: (_) {}),
            ),
          ),
        ),
      );
      final narrowDelegate = tester
          .widget<GridView>(find.byType(GridView))
          .gridDelegate as SliverGridDelegateWithMaxCrossAxisExtent;
      expect(narrowDelegate.maxCrossAxisExtent, 84);
      expect(tester.takeException(), isNull);
    });
  });
}
