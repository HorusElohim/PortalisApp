import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../identity/domain/device_profile.dart';

/// Device identity, session totals, and identity-related destinations.
class DeviceProfileSection extends StatelessWidget {
  const DeviceProfileSection({
    super.key,
    required this.profile,
    required this.identityError,
    required this.sentBytes,
    required this.receivedBytes,
    required this.people,
    required this.collections,
    required this.onRename,
    required this.onOpenPeople,
    required this.onOpenFormats,
  });

  final DeviceProfile? profile;
  final String? identityError;
  /// Totals, already summed. The screen above owns where they come from —
  /// this only renders them.
  final int sentBytes;
  final int receivedBytes;
  final int people;
  final int collections;
  final VoidCallback? onRename;
  final VoidCallback onOpenPeople;
  final VoidCallback onOpenFormats;

  @override
  Widget build(BuildContext context) {
    final nickname = profile?.nickname ?? '…';
    final initials = profile != null && nickname.isNotEmpty
        ? nickname[0].toUpperCase()
        : '·';
    final sent = sentBytes;
    final received = receivedBytes;

    return Padding(
      padding: const EdgeInsets.fromLTRB(0, 4, 0, 26),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Column(
              children: [
                Avatar(initials: initials, size: 76, primary: true),
                const SizedBox(height: 16),
                CanvasTitle(nickname, size: 30),
                const SizedBox(height: 6),
                Text(
                  profile == null
                      ? (identityError == null
                          ? 'LOADING…'
                          : 'IDENTITY UNAVAILABLE')
                      : 'THIS DEVICE · '
                          '${profile!.deviceId.substring(0, profile!.deviceId.length.clamp(0, 8)).toUpperCase()}',
                  style: monoLabel(size: 10.5, letterSpacing: 0.6),
                ),
                const SizedBox(height: 16),
                PillButton(
                  label: 'Change name',
                  dim: true,
                  onTap: onRename,
                ),
              ],
            ),
          ),
          Padding(
            padding:
                const EdgeInsets.fromLTRB(kScreenGutter, 26, kScreenGutter, 0),
            child: GridView.count(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              crossAxisCount: 2,
              mainAxisSpacing: 10,
              crossAxisSpacing: 10,
              childAspectRatio: 2.0,
              children: [
                _ProfileStat(label: 'SENT · SESSION', value: formatBytes(sent)),
                _ProfileStat(
                  label: 'RECEIVED · SESSION',
                  value: formatBytes(received),
                  highlight: received > 0,
                ),
                _ProfileStat(
                  label: 'COLLECTIONS',
                  value: '$collections',
                ),
                _ProfileStat(
                  label: 'PEOPLE',
                  value: '$people',
                  onTap: onOpenPeople,
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(
              kScreenGutter,
              12,
              kScreenGutter,
              0,
            ),
            child: DestinationRow(
              icon: Icons.shield_outlined,
              iconColor: AppColors.signal,
              title: 'Identity lives on this device',
              subtitle: 'A key pair, no account, no server. Lose the device '
                  'and the identity goes with it.',
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(
              kScreenGutter,
              12,
              kScreenGutter,
              0,
            ),
            child: DestinationRow(
              icon: Icons.category_outlined,
              title: 'File formats',
              subtitle: 'What Portalis can view, and what it converts',
              onTap: onOpenFormats,
            ),
          ),
        ],
      ),
    );
  }
}

class _ProfileStat extends StatelessWidget {
  const _ProfileStat({
    required this.label,
    required this.value,
    this.highlight = false,
    this.onTap,
  });

  final String label;
  final String value;
  final bool highlight;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) => SurfaceCard(
        padding: const EdgeInsets.all(14),
        onTap: onTap,
        child: FittedBox(
          fit: BoxFit.scaleDown,
          alignment: Alignment.centerLeft,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(label, style: monoLabel(size: 9.5)),
                  if (onTap != null) ...[
                    const SizedBox(width: 4),
                    Icon(
                      Icons.chevron_right,
                      size: 13,
                      color: AppColors.textGhost,
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 6),
              Text(
                value,
                style: displayText(
                  size: 24,
                  color: highlight ? AppColors.signal : AppColors.text,
                ),
              ),
            ],
          ),
        ),
      );
}
