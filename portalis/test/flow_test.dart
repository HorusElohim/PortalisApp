import 'test_support.dart';

void main() {
  tearDown(resetTestState);

  group('flows still surface backend errors', () {
    testWidgets('command bar surfaces magnet backend errors', (tester) async {
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PortalisCommandBar(
              onSearch: (_) {},
              onImportTorrent: (_) async {},
            ),
          ),
        ),
      );
      await tester.pump();

      await tester.enterText(
        find.byKey(const Key('commandBarField')),
        'magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567',
      );
      await tester.pump();
      expect(find.text('ADD TORRENT'), findsOneWidget);

      await tester.tap(find.byKey(const Key('commandBarSubmit')));
      await tester.pump();
      // RustLib isn't initialized, so the add is expected to fail â€” inline
      // error, screen stays open, no uncaught exception.
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.byKey(const Key('commandBarField')), findsOneWidget);
      expect(tester.takeException(), isNull);
    });


    testWidgets('share screen requires a name and files', (tester) async {
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(const MaterialApp(home: ShareScreen()));
      await tester.pump();

      expect(find.text('Nothing added yet'), findsOneWidget);
      await tester.enterText(
          find.byKey(const Key('collectionNameField')), 'Trip to Lisbon');
      await tester.pump();

      final button = tester
          .widget<FilledButton>(find.byKey(const Key('createShareButton')));
      expect(button.onPressed, isNull);
      expect(tester.takeException(), isNull);
    });
  });
}
