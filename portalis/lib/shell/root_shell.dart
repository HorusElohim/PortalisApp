import 'package:flutter/material.dart';

import '../app/app_controllers.dart';
import 'base.dart';
import 'desktop/pane.dart';
import 'desktop/layout.dart';
import '../features/collections/presentation/home_screen.dart';
import 'mobile/layout.dart';
import '../features/collections/presentation/collection_share.dart';
import '../features/people/presentation/people_screen.dart';
import '../features/settings/presentation/settings_screen.dart';
import '../features/identity/presentation/user_screen.dart';

/// The single application shell. Its parent state owns navigation and
/// lifecycle; only the arrangement changes when the window crosses the
/// responsive breakpoint.
class RootShell extends AdaptiveShell {
  const RootShell({super.key});

  @override
  State<RootShell> createState() => _RootShellState();
}

class _RootShellState extends AdaptiveShellState<RootShell> {
  @override
  Widget buildCompactLayout(BuildContext context) {
    if (pane == DesktopPane.share) {
      return ShareScreen(
        initialFiles: pendingShareFiles,
        onClose: closeShare,
      );
    }
    return MobileShellLayout(
      index: tab,
      onSelected: selectTab,
      onShare: ([files]) => openShare(files, false),
    );
  }

  @override
  Widget buildWideLayout(BuildContext context) => DesktopShellLayout(
        pane: pane,
        onPane: selectPane,
        liveRate: AppControllers.nexusApp.activity.rateMbps,
        home: Home(
          embedded: true,
          openId: openId,
          onOpen: (id) => openCollection(id, inline: true),
          onShare: ([files]) => openShare(files, true),
        ),
        content: _desktopContent(),
      );


  Widget _desktopContent() => switch (pane) {
        DesktopPane.people => const PeopleScreen(embedded: true),
        DesktopPane.user => const UserScreen(embedded: true),
        DesktopPane.settings => const SettingsScreen(embedded: true),
        DesktopPane.share => ShareScreen(
            key: ValueKey(pendingShareFiles),
            initialFiles: pendingShareFiles,
            onClose: closeShare,
          ),
        DesktopPane.home => const SizedBox.shrink(),
      };
}
