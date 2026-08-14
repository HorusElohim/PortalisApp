import 'package:flutter/material.dart';

import '../../app/app_controllers.dart';
import '../../design/design.dart';
import '../../features/collections/domain/picked_file.dart';
import 'bottom_nav.dart';
import '../../design/theme.dart';
import '../../features/collections/presentation/home_screen.dart';
import '../../features/people/presentation/screen.dart';
import '../../features/settings/presentation/screen.dart';
import '../../features/identity/presentation/user_screen.dart';

/// Compact arrangement of the shared adaptive shell state.
class MobileShellLayout extends StatelessWidget {
  const MobileShellLayout({
    super.key,
    required this.index,
    required this.onSelected,
    required this.onShare,
  });

  final int index;
  final ValueChanged<int> onSelected;
  final void Function([List<PickedFile>?]) onShare;

  @override
  Widget build(BuildContext context) => Scaffold(
        backgroundColor: AppColors.surfaceDeep,
        body: AmbientBackground(
          intensity: Glow.intensityForRate(AppControllers.engine.activity.rateMbps),
          child: SafeArea(
            bottom: false,
            child: IndexedStack(
              index: index,
              children: [
                TickerMode(
                  enabled: index == 0,
                  child: Home(onShare: onShare),
                ),
                TickerMode(
                  enabled: index == 1,
                  child: const UserScreen(embedded: true),
                ),
                TickerMode(
                  enabled: index == 2,
                  child: const PeopleScreen(embedded: true),
                ),
                TickerMode(
                  enabled: index == 3,
                  child: const SettingsScreen(embedded: true),
                ),
              ],
            ),
          ),
        ),
        bottomNavigationBar: AppBottomNav(
          index: index,
          onSelected: onSelected,
        ),
      );
}
