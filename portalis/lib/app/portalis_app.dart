import 'package:flutter/material.dart';

import '../design/design.dart';
import '../design/theme_controller.dart';
import '../shell/root_shell.dart';
import '../shell/navigation.dart';
import '../design/theme.dart';
import 'onboarding_controller.dart';
import 'onboarding_screen.dart';

/// Flutter composition. Native startup lives in the bootstrap module.
class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: ThemeController.instance,
        builder: (context, _) => MaterialApp(
          // Keyed by the active theme so a switch remounts the whole tree
          // instead of rebuilding in place: many widgets below embed
          // AppColors.* inside `const` constructors, and Flutter skips
          // rebuilding an unchanged const instance on an in-place update —
          // only a fresh mount is guaranteed to repaint every one of them.
          key: ValueKey(ThemeController.instance.id),
          title: 'Portalis',
          debugShowCheckedModeBanner: false,
          theme: AppTheme.current,
          home: const _AppHome(),
          navigatorKey: AppNavigation.navigatorKey,
          navigatorObservers: [AppNavigation.observer],
          // The undo shortcut went with "forget all remembered peers": Nexus
          // records no peer history, so there is nothing left to undo.
          builder: (context, child) => ToastScope(
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => FocusManager.instance.primaryFocus?.unfocus(),
              child: child ?? const SizedBox.shrink(),
            ),
          ),
        ),
      );
}

/// Shows the first-run introduction ahead of the shell exactly once — see
/// [OnboardingController]. A `StatefulWidget` rather than deciding this in
/// [MyApp.build] because completing onboarding has to swap the visible
/// screen without touching the theme-keyed `MaterialApp` above it.
class _AppHome extends StatefulWidget {
  const _AppHome();

  @override
  State<_AppHome> createState() => _AppHomeState();
}

class _AppHomeState extends State<_AppHome> {
  bool _showOnboarding = !OnboardingController.instance.completed;

  void _completeOnboarding() {
    OnboardingController.instance.complete();
    if (mounted) setState(() => _showOnboarding = false);
  }

  @override
  Widget build(BuildContext context) => _showOnboarding
      ? OnboardingScreen(onDone: _completeOnboarding)
      : const RootShell();
}
