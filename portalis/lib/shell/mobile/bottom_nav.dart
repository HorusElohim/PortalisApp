import 'package:flutter/material.dart';

import '../../app/app_controllers.dart';
import '../../design/design.dart';
import '../../design/theme.dart';
import '../../nexus/domain/app_state.dart';
import '../navigation.dart';

/// The four-destination bottom bar.
///
/// A floating glass dock rather than a bar flush with the screen edge — the
/// same [NavDock] shell the desktop top bar wears, so crossing the
/// responsive breakpoint relocates one control instead of swapping it for a
/// visually unrelated one.
class AppBottomNav extends StatelessWidget {
  const AppBottomNav({
    super.key,
    required this.index,
    required this.onSelected,
  });

  final int index;
  final ValueChanged<int> onSelected;

  static const items = <({IconData? icon, String label})>[
    (icon: null, label: 'Home'),
    (icon: Icons.person_outline, label: 'User'),
    (icon: Icons.people_outline, label: 'People'),
    (icon: Icons.tune, label: 'Settings'),
  ];

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: AppControllers.engine,
        builder: (context, _) {
          final intensity = Glow.intensityForRate(
              AppControllers.engine.activity.totalBytesPerSecond);
          return SafeArea(
            top: false,
            minimum: const EdgeInsets.fromLTRB(16, 0, 16, 12),
            child: NavDock(
              intensity: intensity,
              radius: AppRadius.pill,
              child: Material(
                color: Colors.transparent,
                child: Padding(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
                  child: Row(
                    children: [
                      for (var i = 0; i < items.length; i++)
                        Expanded(
                          child: InkWell(
                            key: Key('navTab$i'),
                            borderRadius: BorderRadius.circular(AppRadius.pill),
                            onTap: () =>
                                i == 0 ? AppNavigation.goHome() : onSelected(i),
                            child: NavSelection(
                              selected: i == index,
                              padding: const EdgeInsets.symmetric(
                                  horizontal: 4, vertical: 8),
                              radius: AppRadius.pill,
                              child: Column(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  if (items[i].icon == null)
                                    Opacity(
                                      opacity: i == index ? 1 : 0.45,
                                      child: const PortalisLogo(size: 22),
                                    )
                                  else
                                    Icon(
                                      items[i].icon,
                                      size: 22,
                                      color: i == index
                                          ? AppColors.signal
                                          : AppColors.textGhost,
                                    ),
                                  const SizedBox(height: 4),
                                  Text(
                                    items[i].label,
                                    style: AppText.caption(
                                      color: i == index
                                          ? AppColors.text
                                          : AppColors.textGhost,
                                      weight: i == index
                                          ? FontWeight.w600
                                          : FontWeight.w400,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          );
        },
      );
}
