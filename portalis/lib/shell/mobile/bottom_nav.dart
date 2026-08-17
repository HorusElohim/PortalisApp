import 'package:flutter/material.dart';

import '../../design/design.dart';
import '../../design/theme.dart';
import '../navigation.dart';

/// The four-destination bottom bar.
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
  Widget build(BuildContext context) => Container(
        decoration: BoxDecoration(
          color: AppColors.surfaceDeep,
          border: Border(top: BorderSide(color: AppColors.border)),
        ),
        child: SafeArea(
          top: false,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(8, 8, 8, 8),
            child: Row(
              children: [
                for (var i = 0; i < items.length; i++)
                  Expanded(
                    child: InkWell(
                      key: Key('navTab$i'),
                      borderRadius: BorderRadius.circular(AppRadius.inner),
                      onTap: () =>
                          i == 0 ? AppNavigation.goHome() : onSelected(i),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(vertical: 6),
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            if (items[i].icon == null)
                              Opacity(
                                opacity: i == index ? 1 : 0.45,
                                child: const PortalisLogo(size: 24),
                              )
                            else
                              Icon(
                                items[i].icon,
                                size: 24,
                                color: i == index
                                    ? AppColors.text
                                    : AppColors.textGhost,
                              ),
                            const SizedBox(height: 6),
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
      );
}
