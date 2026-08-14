import 'dart:async';

import 'package:flutter/material.dart';

import '../app/app_controllers.dart';
import '../design/design.dart';
import '../features/collections/domain/picked_file.dart';
import '../services/navigation.dart';
import 'desktop_pane.dart';
import '../features/collections/presentation/collection_share.dart';
import '../features/nexus/presentation/nexus_collection_detail.dart';

/// The one stateful shell for every window size.
///
/// Lifecycle, selected destination, open collection, and one-shot flows live
/// here. Subclasses only choose how that state is arranged on screen, so
/// resizing a Windows window cannot replace one navigation state with another.
abstract class AdaptiveShell extends StatefulWidget {
  const AdaptiveShell({super.key});
}

abstract class AdaptiveShellState<T extends AdaptiveShell> extends State<T>
    with WidgetsBindingObserver {
  int get tab => AppNavigation.tab.value;
  DesktopPane get pane => _pane;
  int? get openId => _openId;
  List<PickedFile>? get pendingShareFiles => _pendingShareFiles;

  DesktopPane _pane = DesktopPane.home;
  int? _openId;
  List<PickedFile>? _pendingShareFiles;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    AppNavigation.tab.addListener(_onTabChanged);
    // The legacy collections engine is deliberately not started. It restored
    // its own collections into the same torrent session Nexus uses and ran a
    // second listener beside it, so the shell could report a transfer that
    // belonged to no collection Nexus knew about — chrome reading one engine
    // above a list drawn from another. Nexus owns the engine; a second one
    // running quietly is how the interface ends up lying.
    AppControllers.settings.load();
    AppControllers.identity.load();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    unawaited(
        AppControllers.nexusApp.setActive(state == AppLifecycleState.resumed));
  }

  void _onTabChanged() {
    final next = _paneForTab(AppNavigation.tab.value);
    if (next != null && next != _pane && mounted) {
      setState(() => _pane = next);
    } else if (mounted) {
      setState(() {});
    }
  }

  void selectTab(int index) {
    AppNavigation.tab.value = index;
  }

  void selectPane(DesktopPane value) {
    final next = value == _pane ? DesktopPane.home : value;
    if (next == _pane) return;
    setState(() => _pane = next);
    final tab = _tabForPane(next);
    if (tab != null) AppNavigation.tab.value = tab;
  }

  /// Opens one collection.
  ///
  /// `inline: false` pushes it as its own route — the compact layout, and
  /// anywhere else with no list to grow a row into. `inline: true` toggles
  /// which id [openId] names instead: a wide window grows the matching row
  /// into its own detail rather than covering the whole shell, which is what
  /// pushing from an embedded pane would otherwise do.
  void openCollection(int id, {required bool inline}) {
    if (!inline) {
      final collection = AppControllers.nexusApp.state?.collections
          .where((item) => item.id == id)
          .firstOrNull;
      if (collection == null) return;
      Navigator.of(context).push(
        MaterialPageRoute(
          builder: (_) =>
              nexusCollectionScreen(collection, AppControllers.nexusApp),
        ),
      );
      return;
    }
    setState(() => _openId = _openId == id ? null : id);
    selectPane(DesktopPane.home);
  }

  void openShare([List<PickedFile>? initialFiles, bool inline = false]) {
    if (!inline) {
      Navigator.of(context).push(
        MaterialPageRoute(
          builder: (_) => ShareScreen(initialFiles: initialFiles),
        ),
      );
      return;
    }
    setState(() => _pendingShareFiles = initialFiles);
    selectPane(DesktopPane.share);
  }


  void closeShare() {
    setState(() => _pendingShareFiles = null);
    selectPane(DesktopPane.home);
  }


  @protected
  Widget buildCompactLayout(BuildContext context);

  @protected
  Widget buildWideLayout(BuildContext context);

  @override
  Widget build(BuildContext context) => WindowBuilder(
        builder: (context, window) => ListenableBuilder(
          listenable: AppControllers.nexusApp,
          builder: (context, _) => KeyedSubtree(
            // Desktop and compact layouts have incompatible parent chains.
            // Make a breakpoint crossing an explicit replacement rather than
            // letting Flutter attempt to retain dependencies from one layout
            // under the other while MediaQuery is notifying resize listeners.
            // Navigation itself remains above this boundary in this State.
            key: ValueKey(window.isDesktop),
            child: window.isDesktop
                ? buildWideLayout(context)
                : buildCompactLayout(context),
          ),
        ),
      );

  @override
  void dispose() {
    AppNavigation.tab.removeListener(_onTabChanged);
    WidgetsBinding.instance.removeObserver(this);
    unawaited(AppControllers.nexusApp.stop());
    super.dispose();
  }

  static DesktopPane? _paneForTab(int value) => switch (value) {
        0 => DesktopPane.home,
        1 => DesktopPane.user,
        2 => DesktopPane.people,
        3 => DesktopPane.settings,
        _ => null,
      };

  static int? _tabForPane(DesktopPane value) => switch (value) {
        DesktopPane.home => 0,
        DesktopPane.user => 1,
        DesktopPane.people => 2,
        DesktopPane.settings => 3,
        DesktopPane.share => null,
      };
}
