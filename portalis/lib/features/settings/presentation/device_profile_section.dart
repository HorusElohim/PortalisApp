import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/app_state.dart';

/// Device identity, backend-owned activity, and identity-related
/// destinations.
///
/// Every activity figure here comes from [AppUserSummary] — the backend's
/// own durable ledger — rather than being summed from the current
/// collection snapshot. `null` means the summary has not loaded yet or the
/// runtime is not started; the section renders a loading state rather than
/// zeros. Identity is read from the live [AppDevice] snapshot rather than a
/// separate identity path — see ADR-0011 decision #11.
///
/// Stat cards size themselves to the window rather than a fixed two-column
/// grid: [WindowBuilder.columns] gives more columns as the pane widens, so a
/// desktop-width User page shows compact cards in a row instead of the same
/// phone-sized two-up grid stretched wide with mostly empty space.
class DeviceProfileSection extends StatelessWidget {
  const DeviceProfileSection({
    super.key,
    required this.device,
    required this.identityError,
    required this.summary,
    required this.summaryError,
    required this.people,
    required this.collections,
    required this.onRename,
    required this.onOpenPeople,
  });

  final AppDevice? device;
  final String? identityError;

  /// The backend's own activity ledger. `null` while loading or unavailable.
  final AppUserSummary? summary;
  final String? summaryError;

  final int people;
  final int collections;
  final VoidCallback? onRename;
  final VoidCallback onOpenPeople;

  @override
  Widget build(BuildContext context) {
    final nickname = device?.name ?? '…';
    final initials = device != null && nickname.isNotEmpty
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
                Avatar(initials: initials, size: 64, primary: true),
                const SizedBox(height: 14),
                CanvasTitle(nickname, size: 26),
                const SizedBox(height: 5),
                Text(
                  device == null
                      ? (identityError == null
                          ? 'LOADING…'
                          : 'IDENTITY UNAVAILABLE')
                      : 'THIS DEVICE · '
                          '${device!.fingerprint.substring(0, device!.fingerprint.length.clamp(0, 8)).toUpperCase()}',
                  style: monoLabel(size: 10.5, letterSpacing: 0.6),
                ),
                if (trackedSince != null) ...[
                  const SizedBox(height: 4),
                  Text(
                    'TRACKED SINCE $trackedSince'.toUpperCase(),
                    style: monoLabel(size: 9.5, color: AppColors.textFaint),
                  ),
                ],
                const SizedBox(height: 14),
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
            stats: [
              _Stat(
                label: 'RUNNING',
                value: summary == null
                    ? '…'
                    : formatNanosDuration(
                        summary!.currentRun.engineRunningNs.toInt()),
              ),
              _Stat(
                label: 'RECEIVED',
                value: summary == null
                    ? '…'
                    : formatBytes(
                        summary!.currentRun.networkDownBytes.toInt()),
              ),
              _Stat(
                label: 'SENT',
                value: summary == null
                    ? '…'
                    : formatBytes(summary!.currentRun.networkUpBytes.toInt()),
              ),
              _Stat(
                label: 'COMPLETED',
                value: summary == null
                    ? '…'
                    : plural(
                        summary!.currentRun.completedDownloads.toInt(),
                        'download'),
              ),
            ],
          ),
          _Section(
            title: 'LIFETIME',
            stats: [
              _Stat(
                label: 'TOTAL RECEIVED',
                value: summary == null
                    ? '…'
                    : formatBytes(summary!.lifetimeNetworkDownBytes.toInt()),
                highlight: summary != null &&
                    summary!.lifetimeNetworkDownBytes > BigInt.zero,
              ),
              _Stat(
                label: 'TOTAL SENT',
                value: summary == null
                    ? '…'
                    : formatBytes(summary!.lifetimeNetworkUpBytes.toInt()),
              ),
              _Stat(
                label: 'SESSIONS',
                value: summary == null ? '…' : '${summary!.runsStarted}',
              ),
              _Stat(
                label: 'ACTIVE TIME',
                value: summary == null
                    ? '…'
                    : formatNanosDuration(
                        summary!.lifetimeForegroundNs.toInt()),
              ),
            ],
          ),
          _Section(
            title: 'LIBRARY',
            stats: [
              _Stat(label: 'COLLECTIONS', value: '$collections'),
              _Stat(label: 'PEOPLE', value: '$people', onTap: onOpenPeople),
              _Stat(
                label: 'HELD LOCALLY',
                value: summary == null
                    ? '…'
                    : formatBytes(summary!.heldBytes.toInt()),
              ),
              _Stat(
                label: 'CATALOG SIZE',
                value: summary == null
                    ? '…'
                    : formatBytes(summary!.catalogBytes.toInt()),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section({required this.title, required this.stats});

  final String title;
  final List<_Stat> stats;

  @override
  Widget build(BuildContext context) => Padding(
        padding:
            const EdgeInsets.fromLTRB(kScreenGutter, 20, kScreenGutter, 0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SectionLabel(title),
            const SizedBox(height: 8),
            // More columns as the pane widens, instead of a fixed two-up
            // grid stretched to fill a desktop-width pane — see the class
            // doc on [DeviceProfileSection].
            WindowBuilder(
              builder: (context, window) => GridView.count(
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                crossAxisCount: window.columns(190),
                mainAxisSpacing: 8,
                crossAxisSpacing: 8,
                childAspectRatio: 1.7,
                children: stats,
              ),
            ),
          ],
        ),
      );
}

class _Stat extends StatelessWidget {
  const _Stat({
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
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
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
                  Text(label, style: monoLabel(size: 9)),
                  if (onTap != null) ...[
                    const SizedBox(width: 4),
                    Icon(
                      Icons.chevron_right,
                      size: 12,
                      color: AppColors.textGhost,
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 4),
              Text(
                value,
                style: displayText(
                  size: 19,
                  color: highlight ? AppColors.signal : AppColors.text,
                ),
              ),
            ],
          ),
        ),
      );
}
