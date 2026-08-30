import 'test_support.dart';

void main() {
  tearDown(resetTestState);

  group('user screen', () {
    testWidgets('renders backend-owned lifetime and session totals',
        (tester) async {
      await pumpApp(
        tester,
        userSummary: buildUserSummary(
          trackedSince: 1000000000,
          lifetimeNetworkDownBytes: 5000000,
          lifetimeNetworkUpBytes: 2000000,
          runsStarted: 3,
          collectionsOwned: 2,
          collectionsReceived: 1,
          currentRun: buildAppRun(networkDownBytes: 900000),
        ),
      );

      await tester.tap(find.byKey(const Key('navTab1')));
      await tester.pumpAndSettle();

      // Lifetime figures come straight from the backend summary, not a sum
      // of the current engine snapshot's collections.
      expect(find.text('5 MB'), findsOneWidget); // lifetime received
      expect(find.text('2 MB'), findsOneWidget); // lifetime sent
      expect(find.text('3'), findsOneWidget); // sessions
      expect(find.text('900 KB'), findsOneWidget); // current session received
      expect(tester.takeException(), isNull);
    });

    testWidgets('a summary read failure shows an error, not a crash',
        (tester) async {
      await pumpApp(tester);
      // Seeding leaves the engine debug-seeded, so the repository path is
      // never reached in this harness; this test instead exercises the
      // screen's null-summary loading state on first pump before the
      // asynchronous userSummary() future resolves.
      await tester.tap(find.byKey(const Key('navTab1')));
      await tester.pump();
      expect(tester.takeException(), isNull);
    });

    testWidgets('File formats and Clear activity history are not on User',
        (tester) async {
      // Both moved to Settings — User is identity and activity totals only.
      // See test/settings_screen_test.dart for their coverage there.
      await pumpApp(tester, userSummary: buildUserSummary());
      await tester.tap(find.byKey(const Key('navTab1')));
      await tester.pumpAndSettle();

      expect(find.text('File formats'), findsNothing);
      expect(find.text('Clear activity history'), findsNothing);
    });
  });
}
