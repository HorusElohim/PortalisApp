import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../identity/domain/device_profile.dart';
import '../../../nexus/domain/app_state.dart';

/// Device identity, backend-owned activity, and identity-related
/// destinations.
///
/// Every figure here comes from [AppUserSummary] — the backend's own durable
/// ledger — rather than being summed from the current collection snapshot.
/// `null` means the summary has not loaded yet or the runtime is not
/// started; the section renders a loading state rather than zeros.
class DeviceProfileSection extends StatelessWidget {
  const DeviceProfileSection({
    super.key,
    required this.profile,
    required this.identityError,
    required this.summary,
    required this.summaryError,
    required this.people,
    required this.collections,
    required this.onRename,
    required this.onOpenPeople,
    required this.onOpenFormats,
    required this.onClearActivity,
  });

  final DeviceProfile? profile;
  final String? identityError;

  /// The backend's own activity ledger. `null` while loading or unavailable.
  final AppUserSummary? summary;
  final String? summaryError;

  final int people;
  final int collections;
  final VoidCallback? onRename;
  final VoidCallback onOpenPeople;
  final VoidCallback onOpenFormats;
  final VoidCallback onClearActivity;

  @override
  Widget build(BuildContext context) {
    final nickname = profile?.nickname ?? '…';
    final initials = profile != null && nickname.isNotEmpty
        ? nickname[0].toUpperCase()
        : '·';
    final trackedSince = summary == null
        ? null
        : formatTrackedSince(summary!.trackedSince.toInt());

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
                if (trackedSince != null) ...[
                  const SizedBox(height: 4),
                  Text(
                    'TRACKED SINCE $trackedSince'.toUpperCase(),
                    style: monoLabel(size: 9.5, color: AppColors.textFaint),
                  ),
                ],
                const SizedBox(height: 16),
                PillButton(
                  label: 'Change name',
                  dim: true,
                  onTap: onRename,
                ),
              ],
            ),
          ),
          if (summaryError != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(
                kScreenGutter,
                18,
                kScreenGutter,
                0,
              ),
              child: Text(
                'Couldn\'t read activity: $summaryError',
                style: AppText.caption(color: AppColors.danger),
              ),
            ),
          _Section(
            title: 'CURRENT SESSION',
            child: GridView.count(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              crossAxisCount: 2,
              mainAxisSpacing: 10,
              crossAxisSpacing: 10,
              childAspectRatio: 2.0,
              children: [
                _ProfileStat(
                  label: 'RUNNING',
                  value: summary == null
                      ? '…'
                      : formatNanosDuration(
                          summary!.currentRun.engineRunningNs.toInt()),
                ),
                _ProfileStat(
                  label: 'RECEIVED',
                  value: summary == null
                      ? '…'
                      : formatBytes(
                          summary!.currentRun.networkDownBytes.toInt()),
                ),
                _ProfileStat(
                  label: 'SENT',
                  value: summary == null
                      ? '…'
                      : formatBytes(
                          summary!.currentRun.networkUpBytes.toInt()),
                ),
                _ProfileStat(
                  label: 'COMPLETED',
                  value: summary == null
                      ? '…'
                      : plural(
                          summary!.currentRun.completedDownloads.toInt(),
                          'download'),
                ),
              ],
            ),
          ),
          _Section(
            title: 'LIFETIME',
            child: GridView.count(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              crossAxisCount: 2,
              mainAxisSpacing: 10,
              crossAxisSpacing: 10,
              childAspectRatio: 2.0,
              children: [
                _ProfileStat(
                  label: 'TOTAL RECEIVED',
                  value: summary == null
                      ? '…'
                      : formatBytes(
                          summary!.lifetimeNetworkDownBytes.toInt()),
                  highlight: summary != null &&
                      summary!.lifetimeNetworkDownBytes > BigInt.zero,
                ),
                _ProfileStat(
                  label: 'TOTAL SENT',
                  value: summary == null
                      ? '…'
                      : formatBytes(summary!.lifetimeNetworkUpBytes.toInt()),
                ),
                _ProfileStat(
                  label: 'SESSIONS',
                  value: summary == null ? '…' : '${summary!.runsStarted}',
                ),
                _ProfileStat(
                  label: 'ACTIVE TIME',
                  value: summary == null
                      ? '…'
                      : formatNanosDuration(
                          summary!.lifetimeForegroundNs.toInt()),
                ),
              ],
            ),
          ),
          _Section(
            title: 'LIBRARY',
            child: GridView.count(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              crossAxisCount: 2,
              mainAxisSpacing: 10,
              crossAxisSpacing: 10,
              childAspectRatio: 2.0,
              children: [
                _ProfileStat(
                  label: 'COLLECTIONS',
                  value: '$collections',
                ),
                _ProfileStat(
                  label: 'PEOPLE',
                  value: '$people',
                  onTap: onOpenPeople,
                ),
                _ProfileStat(
                  label: 'HELD LOCALLY',
                  value: summary == null
                      ? '…'
                      : formatBytes(summary!.heldBytes.toInt()),
                ),
                _ProfileStat(
                  label: 'CATALOG SIZE',
                  value: summary == null
                      ? '…'
                      : formatBytes(summary!.catalogBytes.toInt()),
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
          Padding(
            padding: const EdgeInsets.fromLTRB(
              kScreenGutter,
              12,
              kScreenGutter,
              0,
            ),
            child: DestinationRow(
              icon: Icons.history_toggle_off,
              title: 'Clear activity history',
              subtitle: 'Resets session and lifetime totals on this device. '
                  'Never touches your identity or collections.',
              onTap: onClearActivity,
            ),
          ),
        ],
      ),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section({required this.title, required this.child});

  final String title;
  final Widget child;

  @override
  Widget build(BuildContext context) => Padding(
        padding:
            const EdgeInsets.fromLTRB(kScreenGutter, 22, kScreenGutter, 0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SectionLabel(title),
            const SizedBox(height: 10),
            child,
          ],
        ),
      );
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
