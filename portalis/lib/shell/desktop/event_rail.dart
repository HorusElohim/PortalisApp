import 'package:flutter/material.dart';

import '../../app/app_controllers.dart';
import '../../design/design.dart';
import '../../design/theme.dart';

/// A quiet rail that appears only while something is actually transferring.
///
/// Reads Nexus, which is the engine. It used to read the legacy collections
/// controller — a second engine polling the same torrent session — and so
/// could report "1 ACTIVE TRANSFER" above a Home showing no collections at
/// all. Status chrome that disagrees with the list beside it is worse than no
/// chrome: it makes a person doubt the thing they can see.
class DesktopEventRail extends StatelessWidget {
  const DesktopEventRail({super.key});

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: AppControllers.engine,
        builder: (context, _) {
          final activity = AppControllers.engine.activity;
          if (!activity.isMoving) return const SizedBox.shrink();

          return Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              LiveDot(color: AppColors.signal, size: 7),
              const SizedBox(width: 8),
              Text(
                plural(activity.transfers, 'active transfer').toUpperCase(),
                style: monoLabel(size: 10, color: AppColors.signal),
              ),
              if (activity.downBytesPerSecond > 0) ...[
                const SizedBox(width: 12),
                Text(
                  '↓ ${formatRate(activity.downBytesPerSecond)}',
                  style: monoLabel(size: 10),
                ),
              ],
              if (activity.upBytesPerSecond > 0) ...[
                const SizedBox(width: 12),
                Text(
                  '↑ ${formatRate(activity.upBytesPerSecond)}',
                  style: monoLabel(size: 10),
                ),
              ],
            ],
          );
        },
      );
}
