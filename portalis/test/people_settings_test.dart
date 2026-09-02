import 'dart:typed_data';

import 'test_support.dart';

void main() {
  tearDown(resetTestState);

  group('people', () {
    /// People is contacts. Swarm addresses belong to the transfer that is
    /// moving bytes with them, not to a directory of strangers — see
    /// `PeopleScreen`'s own doc.
    testWidgets('lists contacts and where they appear', (tester) async {
      await tester.binding.setSurfaceSize(desktopSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        const MaterialApp(home: PeopleScreen()),
      );
      AppControllers.engine.debugSeed(
        AppSnapshot(
          device: const AppDevice(
            name: 'Portalis',
            handle: null,
            fingerprint: 'test-fingerprint',
            devices: 1,
          ),
          connectivity: 'LocalOnly',
          contacts: const [
            AppContact(
              id: 7,
              displayName: 'Ana',
              handle: 'ana#7Q2XZ',
              fingerprint: 'ab:cd:ef',
              verified: true,
              friendship: 'Accepted',
              reachable: null,
            ),
          ],
          collections: [
            buildNexusCollection(id: 1, name: 'Iceland trip', members: const [
              AppMember(fingerprint: 'member-root', contact: 7),
            ]),
          ],
          alerts: const [],
        ),
      );
      await tester.pump();

      expect(find.text('Ana'), findsOneWidget);
      expect(find.text('VERIFIED'), findsOneWidget);
      expect(find.text('Iceland trip'), findsOneWidget,
          reason: 'a card says where the person actually appears');
      // The fingerprint is what makes "verified" mean anything, so it is
      // on screen rather than behind a tap.
      expect(find.text('ab:cd:ef'), findsOneWidget);
    });

    testWidgets('says so plainly when nobody is known yet', (tester) async {
      await tester.binding.setSurfaceSize(desktopSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(const MaterialApp(home: PeopleScreen()));
      AppControllers.engine.debugSeed(buildNexusState(const []));
      await tester.pump();

      expect(find.textContaining('Nobody yet'), findsOneWidget);
    });

    /// The figures a person actually wants from a swarm peer are what it has
    /// exchanged and how fast — the only things about it this device measures
    /// rather than takes on trust.
    testWidgets('lists live connections with what each has exchanged',
        (tester) async {
      await tester.binding.setSurfaceSize(desktopSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(const MaterialApp(home: PeopleScreen()));
      AppControllers.engine.debugSeed(
        buildNexusState([
          buildNexusCollection(id: 1, name: 'Iceland trip'),
        ]),
        peers: [
          AppCollectionPeer(
            collection: 1,
            peer: buildPeer(
              address: '203.0.113.5:6881',
              client: 'qBittorrent 4.6',
              downBytes: 4194304,
              upBytes: 1048576,
              downBytesPerSecond: 512000,
            ),
          ),
          AppCollectionPeer(
            collection: 1,
            peer: buildPeer(address: '198.51.100.9:6881'),
          ),
        ],
      );
      await tester.pump();
      await tester.pump();

      expect(find.textContaining('203.0.113.5:6881'), findsOneWidget);
      expect(find.textContaining('Iceland trip'), findsWidgets);
      // People reuses the full collection peer card instead of maintaining a
      // second, less informative connection-card design.
      expect(find.text('qBittorrent 4.6'), findsOneWidget);
      expect(find.text('DOWNLOADED'), findsWidgets);
      expect(find.text('UPLOADED'), findsWidgets);
      expect(find.text('DOWN SPEED'), findsWidgets);
      expect(find.text('UP SPEED'), findsWidgets);
      expect(find.text('512 KB/s'), findsOneWidget);
      // Connections are never presented as verified people.
      expect(find.text('VERIFIED'), findsNothing);
    });

    testWidgets('saved peers show historical peaks and never claim to be live',
        (tester) async {
      await tester.binding.setSurfaceSize(desktopSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(const MaterialApp(home: PeopleScreen()));
      final seen = DateTime.now().subtract(const Duration(minutes: 5));
      AppControllers.engine.debugSeed(
        buildNexusState([
          buildNexusCollection(id: 1, name: 'Iceland trip'),
        ]),
        peoplePeers: [
          AppPeoplePeer(
            peer: AppPeer(
              address: '203.0.113.5:6881',
              client: 'qBittorrent 4.6',
              downBytes: BigInt.from(4194304),
              upBytes: BigInt.from(1048576),
              // A persisted last sample used to turn this card LIVE.
              downBytesPerSecond: 512000,
              upBytesPerSecond: 64000,
            ),
            collections: Uint32List.fromList([1]),
            live: false,
            peakDownBytesPerSecond: 900000,
            peakUpBytesPerSecond: 120000,
            lastSeenAt:
                BigInt.from(seen.microsecondsSinceEpoch) * BigInt.from(1000),
          ),
        ],
      );
      await tester.pump();
      await tester.pump();

      expect(find.text('SWARM HISTORY - 1'), findsOneWidget);
      expect(find.text('LIVE'), findsNothing);
      expect(find.byType(LiveDot), findsNothing);
      expect(find.text('DOWN SPEED'), findsNothing);
      expect(find.text('UP SPEED'), findsNothing);
      expect(find.text('PEAK DOWN'), findsOneWidget);
      expect(find.text('PEAK UP'), findsOneWidget);
      expect(find.text('900 KB/s'), findsOneWidget);
      expect(find.text('120 KB/s'), findsOneWidget);
      expect(find.textContaining('SEEN 5m AGO'), findsOneWidget);
    });
  });

  group('settings', () {
    testWidgets('hides engine internals behind Network & engine',
        (tester) async {
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      AppControllers.settings.debugSeed(buildEngineSettings());
      await tester.pumpWidget(const MaterialApp(home: SettingsScreen()));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      // Advanced rows are not on the first screen.
      expect(find.text('Network & engine'), findsOneWidget);
      expect(find.text('ENGINE EFFICIENCY'), findsOneWidget);
      expect(find.text('Listen ports'), findsNothing);
      expect(tester.takeException(), isNull);
    });

    testWidgets('the advanced screen exposes the construction-time rows',
        (tester) async {
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      AppControllers.settings.debugSeed(buildEngineSettings());
      await tester.pumpWidget(
        const MaterialApp(home: SettingsScreen(advanced: true)),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      expect(find.text('Listen ports'), findsOneWidget);
      expect(find.text('Disable DHT'), findsOneWidget);
      // Profile â€” the old You screen's content â€” is scoped to Basic; the
      // engine internals drill-down has nothing to do with identity.
      expect(find.text('Change name'), findsNothing);
      expect(tester.takeException(), isNull);
    });
  });
}
