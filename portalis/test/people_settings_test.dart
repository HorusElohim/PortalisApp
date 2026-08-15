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
            buildNexusCollection(id: 1, name: 'Iceland trip', members: Uint32List.fromList(const [7])),
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
  });

  group('settings', () {
    testWidgets('the Nexus service is configurable from the first screen',
        (tester) async {
      await tester.binding.setSurfaceSize(const Size(390, 2400));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      AppControllers.settings.debugSeed(buildEngineSettings());
      await tester.pumpWidget(const MaterialApp(home: SettingsScreen()));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      expect(find.text('NEXUS SERVICE'), findsOneWidget);
      expect(find.text('Connection'), findsOneWidget);
      expect(
        find.text('Not configured'),
        findsOneWidget,
        reason: 'nothing set up says so, rather than "ready to connect"',
      );

      // Opening it offers the local service, so running one on this machine
      // is a single paste of its Node ID.
      await tester.tap(find.text('Connection'));
      await tester.pumpAndSettle();
      expect(find.text(defaultDirectAddress), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

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
