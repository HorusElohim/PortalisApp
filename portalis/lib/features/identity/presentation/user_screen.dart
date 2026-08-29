import 'dart:async';

import 'package:flutter/material.dart';

import '../../../app/app_controllers.dart';
import '../../../nexus/domain/app_state.dart';
import '../../../design/design.dart';
import '../../settings/presentation/device_profile_section.dart';
import '../../../shell/navigation.dart';
import '../../settings/presentation/formats_screen.dart';

/// The local device identity, backend-owned activity, and the people who
/// receive from it.
///
/// User is deliberately separate from Settings: identity answers "who am
/// I?" while Settings answers "how does the engine behave?". Every activity
/// figure here is a direct render of [AppUserSummary] — the backend's own
/// durable ledger — never a Flutter-side sum of the current snapshot; see
/// ADR-0011.
class UserScreen extends StatefulWidget {
  const UserScreen({super.key, this.embedded = false});

  final bool embedded;

  @override
  State<UserScreen> createState() => _UserScreenState();
}

class _UserScreenState extends State<UserScreen> {
  bool _showFormats = false;
  AppUserSummary? _summary;
  String? _summaryError;
  Timer? _poll;

  @override
  void initState() {
    super.initState();
    AppControllers.identity.load();
    _loadSummary();
    // The current run's counters move while this screen is open — same
    // cadence Storage already polls its own backend-computed figures at.
    _poll = Timer.periodic(const Duration(seconds: 2), (_) => _loadSummary());
    // The engine can start, or a test can seed it, after this widget has
    // already mounted (it lives in an IndexedStack alongside every other
    // tab, so its initState runs before a caller has a chance to seed
    // anything). Retrying once the engine actually has something to say
    // covers that ordering without polling harder than the timer above.
    AppControllers.engine.addListener(_onEngineChanged);
  }

  void _onEngineChanged() {
    if (_summary == null) _loadSummary();
  }

  @override
  void dispose() {
    _poll?.cancel();
    AppControllers.engine.removeListener(_onEngineChanged);
    super.dispose();
  }

  Future<void> _loadSummary() async {
    try {
      final summary = await AppControllers.engine.userSummary();
      if (!mounted) return;
      setState(() {
        _summary = summary;
        _summaryError = null;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() => _summaryError = '$error');
    }
  }

  Future<void> _rename() async {
    final profile = AppControllers.identity.info;
    final result = await promptForText(
      context,
      title: 'Your name',
      initialValue: profile?.nickname,
      helper: 'This is how you appear to collaborators.',
    );
    if (result == null || result.isEmpty || !mounted) return;
    try {
      await AppControllers.identity.rename(result);
    } catch (error) {
      if (mounted) showToast(context, 'Couldn\'t rename: $error');
    }
  }

  Future<void> _clearActivity() async {
    final confirmed = await confirmAction(
      context,
      title: 'Clear activity history?',
      message: 'Resets session and lifetime totals on this device. Your '
          'identity, collections, and settings are never touched.',
      confirmLabel: 'Clear',
      destructive: true,
    );
    if (!confirmed || !mounted) return;
    try {
      await AppControllers.engine.clearUserActivity();
      await _loadSummary();
      if (mounted) showToast(context, 'Activity history cleared');
    } catch (error) {
      if (mounted) showToast(context, 'Couldn\'t clear activity: $error');
    }
  }

  void _openFormats() => openNestedScreen(
        context,
        embedded: widget.embedded,
        showInPlace: () => setState(() => _showFormats = true),
        push: (_) => const FormatsScreen(),
      );

  @override
  Widget build(BuildContext context) {
    if (_showFormats) {
      return FormatsScreen(
        embedded: widget.embedded,
        onBack: () => setState(() => _showFormats = false),
      );
    }

    return AppScreen(
      title: 'User',
      embedded: widget.embedded,
      width: ScreenWidth.full,
      body: ListenableBuilder(
        listenable: Listenable.merge([
          AppControllers.identity,
          AppControllers.engine,
        ]),
        builder: (context, _) {
          final identity = AppControllers.identity;
          return RefreshIndicator(
            onRefresh: _loadSummary,
            child: SingleChildScrollView(
              physics: const AlwaysScrollableScrollPhysics(),
              child: DeviceProfileSection(
                profile: identity.info,
                identityError: identity.lastError,
                summary: _summary,
                summaryError: _summaryError,
                people: AppControllers.engine.state?.contacts.length ?? 0,
                collections:
                    AppControllers.engine.state?.collections.length ?? 0,
                onRename: identity.info == null ? null : _rename,
                onOpenPeople: () =>
                    AppNavigation.tab.value = AppNavigation.peopleTab,
                onOpenFormats: _openFormats,
                onClearActivity: _clearActivity,
              ),
            ),
          );
        },
      ),
    );
  }
}
