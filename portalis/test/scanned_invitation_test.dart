import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/collections/presentation/scanned_invitation.dart';
import 'package:portalis/nexus/bridge/portalis_api.dart';

/// A code produced now, on a network this device shares.
AppInvitation _invitation({
  String name = 'Attic Boxes',
  String owner = "Ada's iPhone",
  int entries = 15,
  bool reachableHere = true,
  Duration age = Duration.zero,
}) =>
    AppInvitation(
      name: name,
      owner: owner,
      entries: entries,
      issuedAtSecs:
          DateTime.utc(2026, 9, 5, 12).subtract(age).millisecondsSinceEpoch ~/
              1000,
      reachableHere: reachableHere,
    );

/// Opens the sheet and returns what it answered.
Future<bool?> _show(WidgetTester tester, AppInvitation invitation) async {
  bool? answer;
  await tester.pumpWidget(MaterialApp(
    home: Builder(
      builder: (context) => ElevatedButton(
        onPressed: () async {
          answer = await confirmScannedInvitation(
            context,
            invitation,
            // Fixed, so an age is a fact about the fixture rather than about
            // when the suite happened to run.
            now: DateTime.utc(2026, 9, 5, 12),
          );
        },
        child: const Text('open'),
      ),
    ),
  ));
  await tester.tap(find.text('open'));
  await tester.pumpAndSettle();
  return answer;
}

void main() {
  group('scanned invitation', () {
    testWidgets('names what was scanned before anything is created',
        (tester) async {
      await _show(tester, _invitation());

      expect(find.byKey(const Key('scannedInvitationName')), findsOneWidget);
      expect(find.text('Attic Boxes'), findsOneWidget);
      expect(find.text("15 items from Ada's iPhone"), findsOneWidget);
      // Nothing is wrong with this code, so nothing is said about it.
      expect(find.byKey(const Key('scannedInvitationWarning')), findsNothing);
    });

    testWidgets('counts one item without pluralising it', (tester) async {
      await _show(tester, _invitation(entries: 1));
      expect(find.text("1 item from Ada's iPhone"), findsOneWidget);
    });

    testWidgets('importing answers true', (tester) async {
      final invitation = _invitation();
      bool? answer;
      await tester.pumpWidget(MaterialApp(
        home: Builder(
          builder: (context) => ElevatedButton(
            onPressed: () async {
              answer = await confirmScannedInvitation(context, invitation,
                  now: DateTime.utc(2026, 9, 5, 12));
            },
            child: const Text('open'),
          ),
        ),
      ));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('scannedInvitationImport')));
      await tester.pumpAndSettle();
      expect(answer, isTrue);
    });

    testWidgets('cancelling answers false, so nothing durable is created',
        (tester) async {
      final invitation = _invitation();
      bool? answer;
      await tester.pumpWidget(MaterialApp(
        home: Builder(
          builder: (context) => ElevatedButton(
            onPressed: () async {
              answer = await confirmScannedInvitation(context, invitation,
                  now: DateTime.utc(2026, 9, 5, 12));
            },
            child: const Text('open'),
          ),
        ),
      ));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('scannedInvitationCancel')));
      await tester.pumpAndSettle();
      expect(answer, isFalse);
    });

    /// The failure this whole envelope exists to explain: the addresses are
    /// honest, they just name a network this device cannot reach, and the
    /// transfer would otherwise stall with nothing said.
    testWidgets('says so when the code names another network', (tester) async {
      await _show(tester, _invitation(reachableHere: false));

      expect(find.byKey(const Key('scannedInvitationWarning')), findsOneWidget);
      expect(
        find.textContaining('different network'),
        findsOneWidget,
        reason: 'the person needs to be told which thing to fix',
      );
      expect(find.textContaining("Ada's iPhone"), findsWidgets);
    });

    testWidgets('mentions a code old enough to have gone stale',
        (tester) async {
      await _show(tester, _invitation(age: const Duration(hours: 3)));

      expect(find.byKey(const Key('scannedInvitationWarning')), findsOneWidget);
      expect(find.textContaining('3 hours old'), findsOneWidget);
    });

    testWidgets('says nothing about a code that is merely a few minutes old',
        (tester) async {
      await _show(tester, _invitation(age: const Duration(minutes: 2)));
      expect(find.byKey(const Key('scannedInvitationWarning')), findsNothing);
    });

    /// Two phones rarely agree to the second. A sender whose clock runs ahead
    /// produces a code with a negative age, which must read as "fresh" rather
    /// than as a warning about a code made moments ago.
    testWidgets('treats a sender whose clock is ahead as fresh',
        (tester) async {
      await _show(tester, _invitation(age: const Duration(hours: -30)));

      expect(find.byKey(const Key('scannedInvitationWarning')), findsNothing);
      expect(find.textContaining('old'), findsNothing);
    });

    /// Being on the wrong network is what actually stops the transfer, so it
    /// is the thing worth saying — a stale code that still works is not.
    testWidgets('prefers the network warning over the staleness one',
        (tester) async {
      await _show(
          tester,
          _invitation(
            reachableHere: false,
            age: const Duration(days: 2),
          ));

      expect(find.textContaining('different network'), findsOneWidget);
      expect(find.textContaining('old'), findsNothing);
    });
  });
}
