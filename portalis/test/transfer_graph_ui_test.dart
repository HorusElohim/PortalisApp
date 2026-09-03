import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/design/design.dart';

void main() {
  testWidgets('completed history explains scale, peak, and duration',
      (tester) async {
    final start = DateTime(2026, 8, 10, 19, 39, 20);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 900,
            child: TransferGraph(
              progress: 1,
              downBytesPerSecond: 0,
              upBytesPerSecond: 0,
              startedAt: start,
              completedAt: start.add(const Duration(seconds: 7)),
              history: [
                TransferPoint(
                  at: start,
                  downBytesPerSecond: 1000000,
                  upBytesPerSecond: 0,
                ),
                TransferPoint(
                  at: start.add(const Duration(seconds: 4)),
                  downBytesPerSecond: 5000000,
                  upBytesPerSecond: 62500,
                ),
                TransferPoint(
                  at: start.add(const Duration(seconds: 7)),
                  downBytesPerSecond: 2000000,
                  upBytesPerSecond: 0,
                ),
              ],
            ),
          ),
        ),
      ),
    );

    expect(find.text('RECEIVE SESSION'), findsOneWidget);
    expect(find.text('COMPLETED IN 7s'), findsOneWidget);
    expect(find.text('peak 5.0 MB/s'), findsOneWidget);
    expect(find.text('5.0 MB/s'), findsOneWidget); // top of the y-axis
    expect(find.text('0 B/s'), findsOneWidget);
    expect(find.textContaining('now 0 B/s'), findsNothing);
    expect(find.text('END'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('completed progress uses the final sample as the end time',
      (tester) async {
    final start = DateTime(2026, 8, 10, 19, 39, 20);
    final end = start.add(const Duration(seconds: 7));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TransferGraph(
            progress: 1,
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
                downBytesPerSecond: 0,
                upBytesPerSecond: 0,
              ),
            ],
          ),
        ),
      ),
    );

    expect(find.text('END'), findsOneWidget);
    expect(find.text('LATEST'), findsNothing);
    expect(find.text('COMPLETED IN 7s'), findsOneWidget);
    expect(find.text('10/08/2026  19:39:27'), findsOneWidget);
  });

  testWidgets(
      'post-download upload updates the graph without changing completion',
      (tester) async {
    final start = DateTime(2026, 8, 10, 19, 39, 20);
    final completed = start.add(const Duration(seconds: 7));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TransferGraph(
            progress: 1,
            downBytesPerSecond: 0,
            upBytesPerSecond: 250000,
            startedAt: start,
            completedAt: completed,
            history: [
              TransferPoint(
                at: start,
                downBytesPerSecond: 1000000,
                upBytesPerSecond: 0,
              ),
              TransferPoint(
                at: completed,
                downBytesPerSecond: 0,
                upBytesPerSecond: 0,
              ),
            ],
          ),
        ),
      ),
    );

    expect(find.text('COMPLETED IN 7s'), findsOneWidget);
    expect(find.text('now 250 KB/s · peak 250 KB/s'), findsOneWidget);
    expect(find.text('COMPLETED AT 10/08/2026 19:39:27 · UPLOADING NOW'),
        findsOneWidget);
  });

  testWidgets('live history distinguishes current speed from peak speed',
      (tester) async {
    final start = DateTime.now().subtract(const Duration(seconds: 8));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 900,
            child: TransferGraph(
              progress: 0.6,
              downBytesPerSecond: 2500000,
              upBytesPerSecond: 400000,
              startedAt: start,
              history: [
                TransferPoint(
                  at: start,
                  downBytesPerSecond: 1000000,
                  upBytesPerSecond: 0,
                ),
                TransferPoint(
                  at: start.add(const Duration(seconds: 4)),
                  downBytesPerSecond: 4000000,
                  upBytesPerSecond: 400000,
                ),
              ],
            ),
          ),
        ),
      ),
    );

    expect(find.text('RECEIVING SPEED'), findsOneWidget);
    expect(find.textContaining('LIVE ·'), findsOneWidget);
    expect(find.text('now 2.5 MB/s · peak 4.0 MB/s'), findsOneWidget);
    expect(find.text('now 400 KB/s · peak 400 KB/s'), findsOneWidget);
    expect(find.text('LATEST'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('inactive incomplete history ends at its last recorded sample',
      (tester) async {
    final start = DateTime(2026, 8, 10, 19, 39, 20);
    final lastSample = start.add(const Duration(seconds: 7));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 900,
            child: TransferGraph(
              progress: 0.6,
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
                  at: lastSample,
                  downBytesPerSecond: 0,
                  upBytesPerSecond: 0,
                ),
              ],
            ),
          ),
        ),
      ),
    );

    expect(find.text('RECEIVE HISTORY'), findsOneWidget);
    expect(find.text('LAST RECORDED'), findsOneWidget);
    expect(find.text('10/08/2026  19:39:27'), findsOneWidget);
    expect(find.textContaining('LIVE ·'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
      'live native receive activity is labeled separately from progress',
      (tester) async {
    final start = DateTime.now().subtract(const Duration(seconds: 3));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 900,
            child: TransferGraph(
              progress: 0.6,
              downBytesPerSecond: 2000000,
              upBytesPerSecond: 0,
              startedAt: start,
              history: [
                TransferPoint(
                  at: start,
                  downBytesPerSecond: 1000000,
                  upBytesPerSecond: 0,
                ),
              ],
            ),
          ),
        ),
      ),
    );

    expect(find.text('RECEIVING SPEED'), findsOneWidget);
    expect(find.text('RECEIVING'), findsOneWidget);
    expect(find.textContaining('now 2.0 MB/s'), findsOneWidget);
  });

  testWidgets('either transfer direction keeps a logarithmic timeline visible',
      (tester) async {
    final start = DateTime.now().subtract(const Duration(seconds: 8));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 900,
            child: TransferPanel(
              progress: 0.5,
              downloadedBytes: 500,
              totalBytes: 1000,
              downBytesPerSecond: 100000000,
              upBytesPerSecond: 10000,
              startedAt: start,
            ),
          ),
        ),
      ),
    );

    expect(find.text('RECEIVING'), findsOneWidget);
    expect(find.text('UPLOAD'), findsOneWidget);
    expect(find.text('START'), findsOneWidget);
    expect(find.text('LATEST'), findsOneWidget);
    // With a 10 KB/s–100 MB/s range, the logarithmic midpoint is about
    // 990 KB/s. A linear graph would label this grid line 50 MB/s and flatten
    // the upload into the floor.
    expect(find.text('990 KB/s'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('upload-only activity still renders the temporal graph',
      (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TransferPanel(
            progress: 1,
            downloadedBytes: 1000,
            totalBytes: 1000,
            upBytesPerSecond: 25000,
          ),
        ),
      ),
    );

    expect(find.text('UPLOAD'), findsOneWidget);
    expect(find.text('START'), findsOneWidget);
    expect(find.text('END'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a completed item with no samples does not show an empty chart',
      (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TransferPanel(
            progress: 1,
            downloadedBytes: 276000000,
            totalBytes: 276000000,
          ),
        ),
      ),
    );

    expect(find.text('100%'), findsOneWidget);
    expect(find.text('RECEIVE SESSION'), findsNothing);
  });

  testWidgets('the labeled chart remains readable at phone width',
      (tester) async {
    final start = DateTime(2026, 8, 10, 19, 39, 20);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 340,
            child: TransferGraph(
              progress: 0.5,
              downBytesPerSecond: 1562500,
              upBytesPerSecond: 150000,
              startedAt: start,
              history: [
                TransferPoint(
                  at: start,
                  downBytesPerSecond: 8000000,
                  upBytesPerSecond: 62500,
                ),
              ],
            ),
          ),
        ),
      ),
    );

    expect(find.text('RECEIVING'), findsOneWidget);
    expect(find.text('UPLOAD'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('an active owner names upload activity as seeding',
      (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TransferGraph(
            progress: 1,
            downBytesPerSecond: 0,
            upBytesPerSecond: 1250000,
            seeding: true,
          ),
        ),
      ),
    );

    expect(find.text('SEEDING SPEED'), findsOneWidget);
    expect(find.text('SEEDING'), findsOneWidget);
    expect(find.text('RECEIVING SPEED'), findsNothing);
  });

  group('compressedPositions', () {
    test('a long idle gap does not dominate the axis', () {
      final start = DateTime(2026, 9, 2, 21, 52, 51);
      // A short burst of samples half a second apart, then the app is
      // reopened twelve hours later for one more sample — exactly the
      // reported "closed overnight" shape.
      final points = [
        TransferPoint(
          at: start,
          downBytesPerSecond: 1000000,
          upBytesPerSecond: 0,
        ),
        TransferPoint(
          at: start.add(const Duration(milliseconds: 500)),
          downBytesPerSecond: 30000000,
          upBytesPerSecond: 0,
        ),
        TransferPoint(
          at: start.add(const Duration(seconds: 1)),
          downBytesPerSecond: 5000000,
          upBytesPerSecond: 0,
        ),
        TransferPoint(
          at: start.add(const Duration(hours: 12)),
          downBytesPerSecond: 1000,
          upBytesPerSecond: 0,
        ),
      ];

      final positions = compressedPositions(points);

      expect(positions, hasLength(4));
      expect(positions.first, 0.0);
      expect(positions.last, 1.0);
      // The burst (points 0-2, one real second) must occupy a readable
      // fraction of the axis, not the ~1/43200 a linear twelve-hour axis
      // would give it.
      expect(positions[2], greaterThan(0.3));
    });

    test('gaps under the cap remain linear, matching the uncompressed axis',
        () {
      final start = DateTime(2026, 1, 1);
      final points = [
        TransferPoint(at: start, downBytesPerSecond: 1, upBytesPerSecond: 0),
        TransferPoint(
          at: start.add(const Duration(seconds: 30)),
          downBytesPerSecond: 1,
          upBytesPerSecond: 0,
        ),
        TransferPoint(
          at: start.add(const Duration(seconds: 60)),
          downBytesPerSecond: 1,
          upBytesPerSecond: 0,
        ),
      ];

      final positions = compressedPositions(points);

      expect(positions, [0.0, 0.5, 1.0]);
    });

    test('a single point sits at the right edge', () {
      final points = [
        TransferPoint(
          at: DateTime(2026, 1, 1),
          downBytesPerSecond: 1,
          upBytesPerSecond: 0,
        ),
      ];

      expect(compressedPositions(points), [1.0]);
    });

    test('no points produces no positions', () {
      expect(compressedPositions(const []), isEmpty);
    });
  });

  group('transferChartSeries', () {
    test('splices a stopped/resumed marker pair across an idle gap', () {
      final start = DateTime(2026, 9, 2, 21, 52, 51);
      final lastReal = start.add(const Duration(seconds: 1));
      final resumedAt = start.add(const Duration(hours: 12));
      final points = [
        TransferPoint(
          at: start,
          downBytesPerSecond: 1000000,
          upBytesPerSecond: 0,
        ),
        TransferPoint(
          at: start.add(const Duration(milliseconds: 500)),
          downBytesPerSecond: 30000000,
          upBytesPerSecond: 0,
        ),
        TransferPoint(
          at: lastReal,
          downBytesPerSecond: 5000000,
          upBytesPerSecond: 0,
        ),
        TransferPoint(
          at: resumedAt,
          downBytesPerSecond: 1000,
          upBytesPerSecond: 0,
        ),
      ];

      final series = transferChartSeries(points);

      // Two zero-rate markers inserted between the burst and the reopened
      // session: one at the real last-active timestamp, one at the real
      // resumed timestamp.
      expect(series.points, hasLength(points.length + 2));
      expect(series.positions, hasLength(series.points.length));

      final stopped = series.points[3];
      expect(stopped.idleBoundary, TransferIdleBoundary.stopped);
      expect(stopped.at, lastReal);
      expect(stopped.downBytesPerSecond, 0);
      expect(stopped.upBytesPerSecond, 0);

      final resumed = series.points[4];
      expect(resumed.idleBoundary, TransferIdleBoundary.resumed);
      expect(resumed.at, resumedAt);
      expect(resumed.downBytesPerSecond, 0);
      expect(resumed.upBytesPerSecond, 0);

      // The real samples keep their real rates and carry no boundary flag.
      expect(series.points[2].idleBoundary, TransferIdleBoundary.none);
      expect(series.points[2].downBytesPerSecond, 5000000);
      expect(series.points[5].idleBoundary, TransferIdleBoundary.none);
      expect(series.points[5].downBytesPerSecond, 1000);

      // Both markers sit at their real axis positions — the stopped marker
      // matches the last real sample's compressed x, the resumed marker
      // matches the next real sample's.
      expect(series.positions[3], series.positions[2]);
      expect(series.positions[4], series.positions[5]);
    });

    test('uniformly spaced history gets no markers at all', () {
      final start = DateTime(2026, 1, 1);
      final points = [
        TransferPoint(at: start, downBytesPerSecond: 1, upBytesPerSecond: 0),
        TransferPoint(
          at: start.add(const Duration(seconds: 30)),
          downBytesPerSecond: 1,
          upBytesPerSecond: 0,
        ),
        TransferPoint(
          at: start.add(const Duration(seconds: 60)),
          downBytesPerSecond: 1,
          upBytesPerSecond: 0,
        ),
      ];

      final series = transferChartSeries(points);

      expect(series.points, points);
      expect(series.positions, [0.0, 0.5, 1.0]);
    });

    test('a single point produces no markers', () {
      final points = [
        TransferPoint(
          at: DateTime(2026, 1, 1),
          downBytesPerSecond: 1,
          upBytesPerSecond: 0,
        ),
      ];

      final series = transferChartSeries(points);

      expect(series.points, points);
      expect(series.positions, [1.0]);
    });
  });
}
