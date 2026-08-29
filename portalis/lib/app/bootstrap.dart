import 'dart:async';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:video_player_media_kit/video_player_media_kit.dart';

import 'collection_link_receiver.dart';
import '../nexus/bridge/bridge.dart';
import '../nexus/bridge/frb_generated.dart';
import '../design/theme_controller.dart';
import '../version.dart';
import 'app_controllers.dart';
import 'onboarding_controller.dart';
import 'portalis_app.dart';

/// Starts the native backend before Flutter renders any screen.
Future<void> runPortalisApp() async {
  WidgetsFlutterBinding.ensureInitialized();
  _installDiagnosticsHandlers();
  try {
    VideoPlayerMediaKit.ensureInitialized(
      android: true,
      iOS: true,
      macOS: true,
      windows: true,
      linux: true,
    );
    await RustLib.init();
    // Errors after this point can reach the shareable diagnostics file, not
    // just the console.
    _backendReady = true;
    final backendVersion = getVersion();
    final compatibility = backendVersion == expectedBackendVersion;
    debugPrint(
      '[startup] backend trust report: '
      'frontend=$portalisVersion backend=$backendVersion '
      'expected_backend=$expectedBackendVersion '
      'compatibility=${compatibility ? "trusted" : "rejected"}',
    );
    if (!compatibility) {
      throw StateError(
        'Native backend compatibility rejected: frontend $portalisVersion '
        'loaded backend $backendVersion, expected backend '
        '$expectedBackendVersion. '
        'Regenerate the bridge and rebuild the native backend together.',
      );
    }
    await AppControllers.engine.start();
    // Awaited so the first frame already paints the persisted theme — a
    // load kicked off after runApp would flash Nature before a stored
    // Future preference lands.
    await ThemeController.instance.load();
    // Same reasoning: the first frame must already know whether onboarding
    // was completed, or a returning person would see it flash before
    // RootShell replaces it.
    await OnboardingController.instance.load();
    runApp(const MyApp());
    startCollectionLinkReceiver();
  } catch (error) {
    runApp(_StartupErrorApp(error: error));
  }
}

class _StartupErrorApp extends StatelessWidget {
  const _StartupErrorApp({required this.error});

  final Object error;

  @override
  Widget build(BuildContext context) => MaterialApp(
        title: 'Portalis startup error',
        home: Scaffold(
          body: Center(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: SelectableText(
                'Portalis could not start.\n\n$error\n\n'
                'From the portalis folder, run:\n'
                'cargo build --release --manifest-path rust/backend/Cargo.toml\n\n'
                'Then restart Flutter. Regenerate the bindings too when a Rust '
                'API or DTO changes.',
              ),
            ),
          ),
        ),
      );
}

/// Whether [RustLib.init] has completed — a diagnostic caught before that
/// (or, on native platforms, one FRB itself cannot marshal — see below) has
/// nowhere durable to go, so it stays on the console only rather than
/// throwing a second error trying to reach a backend that isn't there yet.
bool _backendReady = false;

/// Routes uncaught Flutter and Dart errors into the same local diagnostics
/// log the native backend writes to (`nexus::diagnostics`), so a crash a
/// beta tester hits lands in the one file the Diagnostics screen can share —
/// not only on a console nobody is attached to in a release build.
///
/// Installed before anything else in [runPortalisApp] so it also catches a
/// failure during native startup itself, not just failures once the app is
/// already running.
///
/// Deliberately local-only, matching every other diagnostic in this app:
/// nothing here transmits anywhere by itself.
void _installDiagnosticsHandlers() {
  final previousFlutterOnError = FlutterError.onError;
  FlutterError.onError = (FlutterErrorDetails details) {
    previousFlutterOnError?.call(details);
    _recordDiagnostic('flutter', details.exceptionAsString());
  };
  PlatformDispatcher.instance.onError = (error, stack) {
    _recordDiagnostic('dart', '$error\n$stack');
    // Not handled: still crashes as it would have. This only ensures the
    // crash is recorded first, in the shareable log, before Flutter or the
    // OS does whatever it would do next (in debug, print the red screen; in
    // release, terminate the isolate).
    return false;
  };
}

void _recordDiagnostic(String tag, String message) {
  // debugPrint always, so a debug run still shows it immediately in the
  // console the way it always did — this is additive, not a replacement.
  debugPrint('[$tag] $message');
  if (!_backendReady) return;
  // Best-effort and fire-and-forget: a diagnostic handler that could itself
  // throw or block would risk masking or delaying the very crash it exists
  // to record.
  unawaited(
    AppControllers.engine.logDiagnostic(tag, message).catchError((_) {}),
  );
}
