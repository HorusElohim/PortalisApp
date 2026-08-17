import 'test_support.dart';

void main() {
  tearDown(resetTestState);

  group('adding a collection', () {
    /// The New Share page is gone: choosing what to put in a collection is a
    /// sheet, and the collection itself is where naming and adjusting happen.
    /// So this is the whole of "create" now — one question with a few answers.
    testWidgets('asks what to add and offers each way of answering',
        (tester) async {
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));

      late BuildContext sheetContext;
      await tester.pumpWidget(MaterialApp(
        home: Builder(builder: (context) {
          sheetContext = context;
          return const Scaffold(body: SizedBox.expand());
        }),
      ));

      unawaited(showAddSourcesSheet(sheetContext));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      // Files is always offered — a device that cannot share its own files
      // can still fetch a torrent, and that is what the entry is for.
      expect(find.byKey(const Key('addFiles')), findsOneWidget);
      expect(find.byKey(const Key('addMagnet')), findsOneWidget);
      expect(
        find.text('A .torrent here is fetched, not shared'),
        findsOneWidget,
        reason: 'the one place a person could be surprised says so',
      );
      expect(tester.takeException(), isNull);
    });

    /// Backing out is not the same as adding nothing: the sheet answers with
    /// null, and no collection is created for somebody who changed their mind.
    testWidgets('answers with nothing when dismissed', (tester) async {
      await tester.binding.setSurfaceSize(phoneSize);
      addTearDown(() => tester.binding.setSurfaceSize(null));

      late BuildContext sheetContext;
      await tester.pumpWidget(MaterialApp(
        home: Builder(builder: (context) {
          sheetContext = context;
          return const Scaffold(body: SizedBox.expand());
        }),
      ));

      ChosenSources? answer;
      var settled = false;
      unawaited(showAddSourcesSheet(sheetContext).then((value) {
        answer = value;
        settled = true;
      }));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      Navigator.of(sheetContext).pop();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 400));

      expect(settled, isTrue);
      expect(answer, isNull);
      expect(tester.takeException(), isNull);
    });
  });

  group('draft names', () {
    test('every default is distinct and says nothing about the content', () {
      expect(draftNames.toSet().length, draftNames.length);
      expect(draftNames.length, greaterThanOrEqualTo(50));
      expect(
        draftNames.every((name) => name.trim() == name && name.isNotEmpty),
        isTrue,
      );
      expect(draftNames, contains(randomDraftName()));
    });
  });
}
