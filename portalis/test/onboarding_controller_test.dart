import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/app/onboarding_controller.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('onboarding controller', () {
    test('complete() writes the same key load() would read back as done',
        () async {
      SharedPreferences.setMockInitialValues({});
      await OnboardingController.instance.complete();

      final preferences = await SharedPreferences.getInstance();
      expect(
        preferences.getBool('app.onboarding.completed.v1'),
        isTrue,
        reason: 'load() reads exactly this key on the next launch',
      );
    });

    test('complete() is idempotent — a second call does not throw or clear it',
        () async {
      SharedPreferences.setMockInitialValues({});
      final controller = OnboardingController.instance;
      await controller.complete();
      await controller.complete();
      expect(controller.completed, isTrue);
    });

    test('load() reflects a flag a previous run already persisted',
        () async {
      SharedPreferences.setMockInitialValues(
        {'app.onboarding.completed.v1': true},
      );
      // A fresh controller instance, not the shared singleton, so this
      // exercises load() from a clean slate exactly as a real cold start
      // would — the singleton above may already be `_loaded` from an
      // earlier test in this file and would short-circuit.
      final fresh = _freshController();
      await fresh.load();
      expect(fresh.completed, isTrue);
    });

    test('load() leaves a fresh install incomplete', () async {
      SharedPreferences.setMockInitialValues({});
      final fresh = _freshController();
      await fresh.load();
      expect(fresh.completed, isFalse);
    });
  });
}

/// [OnboardingController] is a deliberate singleton in production (see its
/// doc), but a test that wants to observe `load()` from a truly empty state
/// needs an instance that has never been loaded before — the private
/// constructor makes that possible only from within this library's own
/// test, via the same class.
OnboardingController _freshController() => OnboardingController.forTesting();
