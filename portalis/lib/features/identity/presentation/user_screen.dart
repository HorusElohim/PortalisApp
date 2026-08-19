import 'package:flutter/material.dart';

import '../../../app/app_controllers.dart';
import '../../../nexus/domain/app_state.dart';
import '../../../design/design.dart';
import '../../settings/presentation/device_profile_section.dart';
import '../../../shell/navigation.dart';
import '../../settings/presentation/formats_screen.dart';

/// The local device identity and the people who receive from it.
///
/// User is deliberately separate from Settings: identity answers "who am I?"
/// while Settings answers "how does the engine behave?".
class UserScreen extends StatefulWidget {
  const UserScreen({super.key, this.embedded = false});

  final bool embedded;

  @override
  State<UserScreen> createState() => _UserScreenState();
}

class _UserScreenState extends State<UserScreen> {
  bool _showFormats = false;

  @override
  void initState() {
    super.initState();
    AppControllers.identity.load();
  }

  /// What this device has sent and is holding, from Nexus.
  int get _totalUploaded => _sum((c) => c.uploadedBytes.toInt());

  int get _totalOnDisk => _sum((c) => c.onDiskBytes.toInt());

  int _sum(int Function(AppCollection) of) =>
      AppControllers.engine.state?.collections
          .fold<int>(0, (total, collection) => total + of(collection)) ??
      0;

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
          return SingleChildScrollView(
            child: DeviceProfileSection(
              profile: identity.info,
              identityError: identity.lastError,
              sentBytes: _totalUploaded,
              receivedBytes: _totalOnDisk,
              people: AppControllers.engine.state?.contacts.length ?? 0,
              collections: AppControllers.engine.state?.collections.length ?? 0,
              onRename: identity.info == null ? null : _rename,
              onOpenPeople: () =>
                  AppNavigation.tab.value = AppNavigation.peopleTab,
              onOpenFormats: _openFormats,
            ),
          );
        },
      ),
    );
  }
}
