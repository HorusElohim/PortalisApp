import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/app/onboarding_screen.dart';

void main() {
  group('onboarding screen', () {
    testWidgets('says what Portalis actually does on the first page',
        (tester) async {
      await tester.pumpWidget(
        MaterialApp(home: OnboardingScreen(onDone: () {})),
      );

      expect(find.text('Direct, not central'), findsOneWidget);
      expect(find.textContaining('no company server'), findsOneWidget);
    });

    testWidgets('Skip calls onDone without stepping through every page',
        (tester) async {
      var done = false;
      await tester.pumpWidget(
        MaterialApp(home: OnboardingScreen(onDone: () => done = true)),
      );

      await tester.tap(find.byKey(const Key('onboardingSkip')));
      await tester.pump();

      expect(done, isTrue);
    });

    testWidgets('Next advances through every page and calls onDone at the end',
        (tester) async {
      var done = false;
      await tester.pumpWidget(
        MaterialApp(home: OnboardingScreen(onDone: () => done = true)),
      );

      expect(find.text('Direct, not central'), findsOneWidget);

      // Three "Next" taps cross the four pages defined in the screen; each
      // needs a settle for the PageView's own transition animation.
      for (var i = 0; i < 3; i++) {
        await tester.tap(find.byKey(const Key('onboardingNext')));
        await tester.pumpAndSettle();
      }

      expect(find.text('Your files stay yours'), findsOneWidget,
          reason: 'the last page in the sequence');
      expect(done, isFalse, reason: 'the last page still needs its own tap');

      await tester.tap(find.byKey(const Key('onboardingNext')));
      await tester.pump();

      expect(done, isTrue);
    });
  });
}
