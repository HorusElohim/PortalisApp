import 'dart:async';

import 'package:flutter/material.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

import '../app/app_controllers.dart';
import '../design/design.dart';
import 'navigation.dart';
import 'desktop/pane.dart';
import '../features/collections/presentation/route.dart';

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

  DesktopPane _pane = DesktopPane.home;

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
    // Keeps the screen awake for exactly as long as a transfer is actually
    // moving bytes — a phone going to sleep mid-transfer is a suspended
    // process on iOS/Android, which is a stalled transfer (see
    // `Nexus::reconnect_active`/`set_active` for the other half of that
    // recovery story). `activity.transfers` is the one place the engine
    // already answers "is anything moving right now", so this listens to
    // the same signal every screen reads rather than tracking its own.
    AppControllers.engine.addListener(_syncWakelock);
    _syncWakelock();
  }

  bool _wakelockHeld = false;

  void _syncWakelock() {
    final active = AppControllers.engine.activity.transfers > 0;
    if (active == _wakelockHeld) return;
    _wakelockHeld = active;
    unawaited(WakelockPlus.toggle(enable: active));
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    unawaited(
        AppControllers.engine.setActive(state == AppLifecycleState.resumed));
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

  /// Opens one collection, on its own page.
  ///
  /// One answer for every window size. A wide window used to grow the row in
  /// place instead, which made "open" mean two different things and left the
  /// collection's own controls — edit among them — reachable on one layout
  /// and not the other.
  void openCollection(int id) {
    final collection = AppControllers.engine.state?.collections
        .where((item) => item.id == id)
        .firstOrNull;
    if (collection == null) return;
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => routeFor(collection, AppControllers.engine),
      ),
    );
  }

  @protected
  Widget buildCompactLayout(BuildContext context);

  @protected
  Widget buildWideLayout(BuildContext context);

  @override
  Widget build(BuildContext context) => WindowBuilder(
        builder: (context, window) => ListenableBuilder(
          listenable: AppControllers.engine,
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
    AppControllers.engine.removeListener(_syncWakelock);
    if (_wakelockHeld) {
      unawaited(WakelockPlus.disable());
    }
    unawaited(AppControllers.engine.stop());
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
      };
}
