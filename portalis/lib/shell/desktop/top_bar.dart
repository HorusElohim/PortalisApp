import 'package:flutter/material.dart';

import '../../app/app_controllers.dart';
import '../../design/design.dart';
import '../../design/theme.dart';
import '../../nexus/domain/app_state.dart';
import 'event_rail.dart';
import 'identity_chip.dart';
import 'navigation_action.dart';
import 'pane.dart';

/// Desktop chrome shared by every pane: identity, live events, navigation.
///
/// A floating glass dock rather than a bar flush with the window edge — the
/// same [NavDock] shell the mobile bottom bar wears, so the destinations a
/// person crosses the responsive breakpoint with feel like one control that
/// relocated, not two designs that happen to sit in similar places.
class DesktopTopBar extends StatelessWidget {
  const DesktopTopBar({super.key, required this.pane, required this.onPane});

  final DesktopPane pane;
  final ValueChanged<DesktopPane> onPane;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: AppControllers.engine,
        builder: (context, _) {
          final active = AppControllers.engine.activity.isMoving;
          final intensity = Glow.intensityForRate(
              AppControllers.engine.activity.totalBytesPerSecond);
          return SafeArea(
            bottom: false,
            minimum: const EdgeInsets.fromLTRB(18, 14, 18, 0),
            child: NavDock(
              intensity: intensity,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(14, 8, 14, 8),
                child: Row(
                  children: [
                    NavSelection(
                      selected: pane == DesktopPane.home,
                      radius: AppRadius.pill,
                      child: Tooltip(
                        message: 'Home',
                        child: Material(
                          color: Colors.transparent,
                          borderRadius: BorderRadius.circular(AppRadius.pill),
                          child: InkWell(
                            key: const Key('headerHomeButton'),
                            borderRadius: BorderRadius.circular(AppRadius.pill),
                            onTap: () => onPane(DesktopPane.home),
                            child: PortalisLogo(size: 30, energized: active),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 10),
                    DesktopIdentityChip(
                      selected: pane == DesktopPane.user,
                      onTap: () => onPane(DesktopPane.user),
                    ),
                    const SizedBox(width: 18),
                    const Expanded(child: DesktopEventRail()),
                    DesktopNavigationAction(
                      icon: Icons.people_outline,
                      tooltip: 'People',
                      selected: pane == DesktopPane.people,
                      badge: _peopleCount,
                      onTap: () => onPane(DesktopPane.people),
                    ),
                    const SizedBox(width: 4),
                    DesktopNavigationAction(
                      icon: Icons.tune,
                      tooltip: 'Settings',
                      selected: pane == DesktopPane.settings,
                      onTap: () => onPane(DesktopPane.settings),
                    ),
                  ],
                ),
              ),
            ),
          );
        },
      );

  String? get _peopleCount {
    final contacts = AppControllers.engine.state?.contacts ?? const [];
    return contacts.isEmpty ? null : '${contacts.length}';
  }
}
