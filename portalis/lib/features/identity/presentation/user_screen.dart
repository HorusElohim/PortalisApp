import 'dart:async';

import 'package:flutter/material.dart';

import '../../../app/app_controllers.dart';
import '../../../nexus/domain/app_state.dart';
import '../../../design/design.dart';
import '../../settings/presentation/device_profile_section.dart';
import '../../../shell/navigation.dart';

/// The local device identity, backend-owned activity, and the people who
/// receive from it.
///
/// User is deliberately separate from Settings: identity answers "who am
/// I?" while Settings answers "how does the engine behave?". Identity is
/// read from [AppSnapshot.device] — the same live projection every other
/// screen reads — rather than a second identity path; renaming goes through
/// [AppController.renameDevice], which updates the persisted identity and
/// this snapshot together (ADR-0011 decision #11). Every activity figure
/// here is a direct render of [AppUserSummary] — the backend's own durable
/// ledger — never a Flutter-side sum of the current snapshot.
///
/// File formats and clearing activity history live in Settings now, not
/// here — they are engine/device configuration, not identity.
class UserScreen extends StatefulWidget {
  const UserScreen({super.key, this.embedded = false});

  final bool embedded;

  @override
  State<UserScreen> createState() => _UserScreenState();
}

class _UserScreenState extends State<UserScreen> with PollingState {
  AppUserSummary? _summary;
  String? _summaryError;

  /// Contacts *and* swarm connections — the same two tiers the People
  /// screen counts (see its own doc, and [_loadPeopleCount] here). A
  /// contact-only count reads as "0 people" for anybody who has only ever
  /// exchanged with anonymous swarm peers, which is the common case.
  int _peopleCount = 0;

  @override
  void initState() {
    super.initState();
    // The current run's counters, and who's connected, both move while
    // this screen is open — same cadence Storage already polls its own
    // backend-computed figures at.
    startPolling();
    // The engine can start, or a test can seed it, after this widget has
    // already mounted (it lives in an IndexedStack alongside every other
    // tab, so its initState runs before a caller has a chance to seed
    // anything). Retried on every change rather than guarded, matching
    // PeopleScreen's own listener — a connection appearing or leaving is
    // exactly the kind of engine change this should react to promptly.
    AppControllers.engine.addListener(_onEngineChanged);
  }

  @override
  void onPoll() {
    _loadSummary();
    _loadPeopleCount();
  }

  void _onEngineChanged() {
    if (_summary == null) _loadSummary();
    _loadPeopleCount();
  }

  @override
  void dispose() {
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

  Future<void> _loadPeopleCount() async {
    final contacts = AppControllers.engine.state?.contacts.length ?? 0;
    final peers = await AppControllers.engine.peoplePeers();
    if (!mounted) return;
    setState(() => _peopleCount = contacts + peers.length);
  }

  Future<void> _rename() async {
    final device = AppControllers.engine.state?.device;
    final result = await promptForText(
      context,
      title: 'Your name',
      initialValue: device?.name,
      helper: 'This is how you appear to collaborators.',
    );
    if (result == null || result.isEmpty || !mounted) return;
    try {
      await AppControllers.engine.renameDevice(result);
    } catch (error) {
      if (mounted) showToast(context, 'Couldn\'t rename: $error');
    }
  }

  @override
  Widget build(BuildContext context) {
    return AppScreen(
      title: 'User',
      embedded: widget.embedded,
      width: ScreenWidth.full,
      body: ListenableBuilder(
        listenable: AppControllers.engine,
        builder: (context, _) {
          final snapshot = AppControllers.engine.state;
          return RefreshIndicator(
            onRefresh: _loadSummary,
            child: SingleChildScrollView(
              physics: const AlwaysScrollableScrollPhysics(),
              child: DeviceProfileSection(
                device: snapshot?.device,
                identityError: AppControllers.engine.lastError,
                summary: _summary,
                summaryError: _summaryError,
                people: _peopleCount,
                collections: snapshot?.collections.length ?? 0,
                onRename: snapshot?.device == null ? null : _rename,
                onOpenPeople: () =>
                    AppNavigation.tab.value = AppNavigation.peopleTab,
              ),
            ),
          );
        },
      ),
    );
  }
}
