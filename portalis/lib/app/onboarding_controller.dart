import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Whether the person has completed (or explicitly skipped) the first-run
/// introduction — see `onboarding_screen.dart`.
///
/// A singleton read once at startup, matching [ThemeController]: the flag
/// has to be known before the first frame decides what to show, and reading
/// it fresh from disk on every rebuild would be both slower and pointless
/// once it is known for the life of the process.
class OnboardingController {
  OnboardingController._();

  static final instance = OnboardingController._();

  /// A fresh, unloaded instance — for tests that need to observe [load]
  /// from a genuinely empty state rather than the process-wide [instance],
  /// which stays `_loaded` for the rest of the run once anything reads it.
  @visibleForTesting
  factory OnboardingController.forTesting() => OnboardingController._();

  static const _prefsKey = 'app.onboarding.completed.v1';

  bool _completed = false;
  bool get completed => _completed;

  bool _loaded = false;
  bool get loaded => _loaded;

  /// Reads the persisted flag. Awaited before the first frame decides
  /// whether to show onboarding, the same way [ThemeController.load] is
  /// awaited before the first frame paints a theme.
  Future<void> load() async {
    if (_loaded) return;
    final preferences = await SharedPreferences.getInstance();
    _completed = preferences.getBool(_prefsKey) ?? false;
    _loaded = true;
  }

  /// Marks onboarding done — reached the end, or tapped Skip. Either way, a
  /// person who has seen it once should never see it pushed on them again;
  /// it stays reachable on demand from Settings for anyone who wants a
  /// refresher.
  Future<void> complete() async {
    if (_completed) return;
    _completed = true;
    final preferences = await SharedPreferences.getInstance();
    await preferences.setBool(_prefsKey, true);
  }

  /// Marks the singleton complete without touching storage — widget tests
  /// pump [MyApp] directly (see `test/test_support.dart`'s `pumpApp`) and
  /// need the shell visible immediately, not gated behind onboarding they
  /// are not testing.
  @visibleForTesting
  void markCompletedForTesting() {
    _completed = true;
    _loaded = true;
  }
}
