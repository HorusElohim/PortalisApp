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

    expect(find.text('DOWNLOAD SESSION'), findsOneWidget);
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

    expect(find.text('TRANSFER SPEED'), findsOneWidget);
    expect(find.textContaining('LIVE ·'), findsOneWidget);
    expect(find.text('now 2.5 MB/s · peak 4.0 MB/s'), findsOneWidget);
    expect(find.text('now 400 KB/s · peak 400 KB/s'), findsOneWidget);
    expect(find.text('LATEST'), findsOneWidget);
    expect(tester.takeException(), isNull);
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

    expect(find.text('DOWNLOAD'), findsOneWidget);
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
    expect(find.text('DOWNLOAD SESSION'), findsNothing);
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

    expect(find.text('DOWNLOAD'), findsOneWidget);
    expect(find.text('UPLOAD'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
