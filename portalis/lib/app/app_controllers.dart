import '../design/theme_controller.dart';
import '../nexus/application/app_controller.dart';
import '../features/settings/application/controller.dart';
import '../notifications/local_transfer_completion_notifier.dart';

/// Application-owned controller instances shared by the widget tree.
///
/// Features expose controllers, repositories, and domain models; this is the
/// sole place that chooses their production implementations and lifetime.
abstract final class AppControllers {
  static final notifications = LocalTransferCompletionNotifier();
  static final engine =
      AppController.production(completionNotifier: notifications);
  static final settings = SettingsController.production();
  static final theme = ThemeController.instance;
}
