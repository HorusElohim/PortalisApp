import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/endpoint_config.dart';

/// The configuration state of the authenticated Nexus service.
class ServiceSection extends StatelessWidget {
  const ServiceSection({
    super.key,
    required this.config,
    required this.connectivity,
    required this.onConfigure,
    required this.onClear,
  });

  final EndpointConfig config;

  /// What the engine reports it can actually reach — see `core::service`.
  /// A configuration is not a connection, and this row used to show one as
  /// though it were the other.
  final String connectivity;
  final VoidCallback onConfigure;
  final VoidCallback onClear;

  /// What the engine says, as a person would put it.
  ///
  /// Read from the engine rather than from whether a Node ID has been typed:
  /// a service that is configured and unreachable is a different situation
  /// from one that was never set up, and the difference is the whole reason
  /// somebody opens this screen.
  ({String label, Color color, String detail}) get _reach {
    if (connectivity.startsWith('Online')) {
      return (
        label: 'Connected',
        color: AppColors.signal,
        detail: 'Authenticated by its Node ID over a direct connection.',
      );
    }
    if (connectivity.startsWith('Degraded')) {
      return (
        label: 'Not reachable',
        color: AppColors.danger,
        detail: 'Configured, but nothing answered. Portalis keeps trying.',
      );
    }
    if (connectivity.startsWith('Connecting')) {
      return (
        label: 'Connecting…',
        color: AppColors.ember,
        detail: 'Dialling the configured service.',
      );
    }
    return config.isConfigured
        ? (
            label: 'Not connected',
            color: AppColors.textFaint,
            detail: 'Configured, and not reaching it yet.',
          )
        : (
            label: 'Not configured',
            color: AppColors.textFaint,
            detail: 'Add a server Node ID to enable online sharing.',
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
            onTap: onConfigure,
          ),
          if (config.isConfigured) ...[
            ValueRow(
              label: 'Direct address',
              // A configured service need not have one: the Node ID is the
              // identity, and the engine can find where it lives.
              value: config.directAddress ?? 'Found automatically',
              subtitle: config.directAddress == null
                  ? 'Located by Node ID, over this network or a signed record.'
                  : 'A route only; changing it does not change the server identity.',
              onTap: onConfigure,
            ),
            ValueRow(
              label: 'Forget Nexus service',
              value: 'Remove',
              subtitle:
                  'This does not change the server or your local identity.',
              onTap: onClear,
            ),
          ],
        ],
      );
}
