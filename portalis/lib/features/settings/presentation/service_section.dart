import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/endpoint_config.dart';

/// Whether this device is reaching the Nexus service.
///
/// Reporting only. The service is not a choice a person makes: it is fixed for
/// everyone and compiled into the build, so there is nothing here to set. What
/// remains worth showing is whether it is being reached, because that changes
/// on its own and explains why sharing is or is not working.
class ServiceSection extends StatelessWidget {
  const ServiceSection({
    super.key,
    required this.config,
    required this.connectivity,
  });

  final EndpointConfig config;

  /// What the engine reports it can actually reach — see `core::service`.
  /// A configuration is not a connection, and this row used to show one as
  /// though it were the other.
  final String connectivity;

  /// What the engine says, as a person would put it.
  ///
  /// Read from the engine rather than from whether a service exists: one that
  /// is present and unreachable is a different situation from one this build
  /// never had, and the difference is the whole reason somebody looks.
  ({String label, Color color, String detail}) get _reach {
    if (connectivity.startsWith('Online')) {
      // The engine reports the path it actually has, and it can change on a
      // connection that never dropped. Claiming "direct" regardless would be
      // the same class of untruth this row was built to stop telling.
      final relayed = connectivity.contains('Relayed');
      return (
        label: 'Connected',
        color: AppColors.signal,
        detail: relayed
            ? 'Authenticated by its Node ID, reached through a relay.'
            : 'Authenticated by its Node ID over a direct connection.',
      );
    }
    if (connectivity.startsWith('Degraded')) {
      return (
        label: 'Not reachable',
        color: AppColors.danger,
        detail: 'Nothing answered. Portalis keeps trying.',
      );
    }
    if (connectivity.startsWith('Connecting')) {
      return (
        label: 'Connecting…',
        color: AppColors.ember,
        detail: 'Reaching the Portalis service.',
      );
    }
    return config.isConfigured
        ? (
            label: 'Not connected',
            color: AppColors.textFaint,
            detail: 'Not reaching the service yet.',
          )
        : (
            label: 'Unavailable',
            color: AppColors.textFaint,
            detail: 'This build ships with no service, so sharing is local only.',
          );
  }

  @override
  Widget build(BuildContext context) => SettingsSection(
        label: 'NEXUS SERVICE',
        children: [
          ValueRow(
            label: 'Connection',
            value: _reach.label,
            valueColor: _reach.color,
            subtitle: _reach.detail,
          ),
        ],
      );
}
