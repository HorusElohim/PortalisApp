import 'package:flutter_local_notifications/flutter_local_notifications.dart';

import 'transfer_completion_notifier.dart';

/// Platform notification delivery for receiver-side completion events.
class LocalTransferCompletionNotifier implements TransferCompletionNotifier {
  LocalTransferCompletionNotifier({FlutterLocalNotificationsPlugin? plugin})
      : _plugin = plugin ?? FlutterLocalNotificationsPlugin();

  static const _channelId = 'transfer-completions';
  final FlutterLocalNotificationsPlugin _plugin;

  Future<void> initialize() async {
    const settings = InitializationSettings(
      android: AndroidInitializationSettings('@mipmap/ic_launcher'),
      iOS: DarwinInitializationSettings(
        requestAlertPermission: false,
        requestBadgePermission: false,
        requestSoundPermission: false,
      ),
      macOS: DarwinInitializationSettings(
        requestAlertPermission: false,
        requestBadgePermission: false,
        requestSoundPermission: false,
      ),
      linux: LinuxInitializationSettings(defaultActionName: 'Open'),
      windows: WindowsInitializationSettings(
        appName: 'Portalis',
        appUserModelId: 'com.portalis.app',
        guid: '51f6881e-649e-4e0d-99e7-501e5dd26a39',
      ),
    );
    await _plugin.initialize(settings: settings);
    await _plugin
        .resolvePlatformSpecificImplementation<
            AndroidFlutterLocalNotificationsPlugin>()
        ?.requestNotificationsPermission();
    await _plugin
        .resolvePlatformSpecificImplementation<
            IOSFlutterLocalNotificationsPlugin>()
        ?.requestPermissions(alert: true, badge: true, sound: true);
    await _plugin
        .resolvePlatformSpecificImplementation<
            MacOSFlutterLocalNotificationsPlugin>()
        ?.requestPermissions(alert: true, badge: true, sound: true);
  }

  @override
  Future<void> showCompleted({required int id, required String name}) =>
      _plugin.show(
        id: id,
        title: 'Transfer complete',
        body: '$name is ready on this device.',
        notificationDetails: const NotificationDetails(
          android: AndroidNotificationDetails(
            _channelId,
            'Transfer completions',
            channelDescription: 'Alerts when a received transfer finishes.',
            importance: Importance.high,
            priority: Priority.high,
          ),
          iOS: DarwinNotificationDetails(
            presentAlert: true,
            presentBadge: true,
            presentSound: true,
          ),
          macOS: DarwinNotificationDetails(
            presentAlert: true,
            presentBadge: true,
            presentSound: true,
          ),
          linux: LinuxNotificationDetails(),
          windows: WindowsNotificationDetails(),
        ),
      );
}
