import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/design/design.dart';
import 'package:portalis/features/settings/presentation/diagnostics_screen.dart';

void main() {
  group('diagnostics screen', () {
    /// Widget tests run with no native runtime attached, so a real read
    /// always fails here — which is exactly the path this test pins: the
    /// screen must show that plainly rather than crash.
    testWidgets('shows the screen and fails gracefully without a backend',
        (tester) async {
      await tester.pumpWidget(
        const MaterialApp(home: DiagnosticsScreen()),
      );
      await tester.pump();
      // The one round-trip through the (unavailable) bridge settles here.
      await tester.pump(const Duration(milliseconds: 50));

      expect(find.text('DIAGNOSTICS'), findsOneWidget);
      // A failed read must be visible, not left spinning forever.
      expect(find.byType(CircularProgressIndicator), findsNothing);
    });

    testWidgets('the Share action starts disabled with nothing loaded',
        (tester) async {
      await tester.pumpWidget(
        const MaterialApp(home: DiagnosticsScreen()),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));

      final share = tester.widget<PrimaryActionButton>(
        find.byType(PrimaryActionButton),
      );
      expect(share.onTap, isNull,
          reason: 'nothing to share when the log never loaded');

      final clear = tester.widget<OutlineActionButton>(
        find.byType(OutlineActionButton),
      );
      expect(clear.onTap, isNull,
          reason: 'nothing to clear when the log never loaded');
    });
  });
}
