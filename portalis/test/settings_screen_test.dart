import 'test_support.dart';

void main() {
  tearDown(resetTestState);

  Future<void> openSettings(WidgetTester tester) async {
    AppControllers.settings.debugSeed(buildEngineSettings());
    await pumpApp(tester, userSummary: buildUserSummary());
    await tester.tap(find.byKey(const Key('navTab3')));
    await tester.pumpAndSettle();
  }

  group('settings screen', () {
    testWidgets('offers File formats and Clear activity history',
        (tester) async {
      await openSettings(tester);

      final formats = find.text('File formats');
      await tester.dragUntilVisible(
        formats,
        find.byType(SingleChildScrollView),
        const Offset(0, -300),
      );
      expect(formats, findsOneWidget);

      final clear = find.text('Clear activity history');
      await tester.dragUntilVisible(
        clear,
        find.byType(SingleChildScrollView),
        const Offset(0, -300),
      );
      expect(clear, findsOneWidget);
    });

    testWidgets('File formats opens the format registry from Settings',
        (tester) async {
      await openSettings(tester);

      final formats = find.text('File formats');
      await tester.dragUntilVisible(
        formats,
        find.byType(SingleChildScrollView),
        const Offset(0, -300),
      );
      await tester.tap(formats);
      await tester.pumpAndSettle();

      expect(find.text('FILE FORMATS'), findsOneWidget);
    });

    testWidgets('Clear activity history confirms before clearing',
        (tester) async {
      await openSettings(tester);

      final clear = find.text('Clear activity history');
      await tester.dragUntilVisible(
        clear,
        find.byType(SingleChildScrollView),
        const Offset(0, -300),
      );
      await tester.tap(clear);
      await tester.pumpAndSettle();

      expect(find.text('Clear activity history?'), findsOneWidget);
    });
  });
}
