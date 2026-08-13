import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../theme.dart';
import '../domain/nexus_endpoint_config.dart';

/// The configuration state of the authenticated Nexus service.
class NexusServiceSection extends StatelessWidget {
  const NexusServiceSection({
    super.key,
    required this.config,
    required this.onConfigure,
    required this.onClear,
  });

  final NexusEndpointConfig config;
  final VoidCallback onConfigure;
  final VoidCallback onClear;

  @override
  Widget build(BuildContext context) => SettingsSection(
        label: 'NEXUS SERVICE',
        children: [
          ValueRow(
            label: 'Connection',
            value: config.isConfigured ? 'Ready to connect' : 'Not configured',
            valueColor:
                config.isConfigured ? AppColors.signal : AppColors.textFaint,
            subtitle: config.isConfigured
                ? 'Iroh will authenticate this server before Portalis signs in.'
                : 'Add a server Node ID and direct address to enable online sharing.',
            onTap: onConfigure,
          ),
          if (config.isConfigured) ...[
            ValueRow(
              label: 'Direct address',
              value: config.directAddress!,
              subtitle:
                  'A route only; changing it does not change the server identity.',
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
