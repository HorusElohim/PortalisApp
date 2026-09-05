import 'package:flutter/material.dart';

import '../../../design/theme.dart';
import '../../../nexus/bridge/portalis_api.dart';

/// How old a code may be before it is worth saying so.
///
/// Peer addresses are only true of the network the sharing device was on when
/// the code was produced, and the ordinary failure is a code left on screen —
/// or screenshotted — after that device moved. Fifteen minutes is long enough
/// that a code held up across a table is never questioned, and short enough
/// that yesterday's screenshot is.
const _staleAfter = Duration(minutes: 15);

/// Confirms a scanned invitation before it becomes a durable collection.
///
/// A magnet gave the receiver nothing to look at until the swarm replied, so
/// the only way to find out what had been scanned — or that it could not work
/// here — was to import it and wait. The invitation already carries the
/// sender's description, so this asks the one question worth asking while
/// there is still nothing to undo.
///
/// Returns true when the person chose to import. Everything shown is the
/// sending device's own claim, carried in a code this device just read off a
/// screen: it is safe to display, and it is not evidence.
Future<bool> confirmScannedInvitation(
  BuildContext context,
  AppInvitation invitation, {
  DateTime? now,
}) async {
  final age = _ageOf(invitation, now ?? DateTime.now());
  final stale = age > _staleAfter;
  final warning = !invitation.reachableHere
      ? 'This code was made on a different network. Join the same Wi-Fi as '
          '${invitation.owner}, then scan again.'
      : stale
          ? 'This code is ${_spoken(age)} old. If the transfer does not '
              'start, ask for a fresh one.'
          : null;

  final confirmed = await showModalBottomSheet<bool>(
    context: context,
    backgroundColor: AppColors.surface,
    shape: const RoundedRectangleBorder(
      borderRadius: BorderRadius.vertical(top: Radius.circular(AppRadius.card)),
    ),
    builder: (sheetContext) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 20, 24, 12),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              invitation.name,
              key: const Key('scannedInvitationName'),
              textAlign: TextAlign.center,
              style: displayText(size: 20, weight: FontWeight.w700),
            ),
            const SizedBox(height: 6),
            Text(
              '${_items(invitation.entries)} from ${invitation.owner}',
              key: const Key('scannedInvitationSummary'),
              textAlign: TextAlign.center,
              style: AppText.secondary(color: AppColors.textDim),
            ),
            if (warning != null) ...[
              const SizedBox(height: 16),
              _Warning(
                message: warning,
                blocking: !invitation.reachableHere,
              ),
            ],
            const SizedBox(height: 20),
            FilledButton(
              key: const Key('scannedInvitationImport'),
              onPressed: () => Navigator.of(sheetContext).pop(true),
              child: const Text('Download'),
            ),
            TextButton(
              key: const Key('scannedInvitationCancel'),
              onPressed: () => Navigator.of(sheetContext).pop(false),
              child: const Text('Cancel'),
            ),
          ],
        ),
      ),
    ),
  );
  return confirmed ?? false;
}

/// The sender's clock against this device's.
///
/// Deliberately *not* clamped at zero. A negative age — the sender's clock
/// running ahead — is never greater than the staleness threshold, so it can
/// only ever mean "not stale", which is the honest answer for a code that was
/// just made. Clamping it would add a branch no rendering path can reach.
Duration _ageOf(AppInvitation invitation, DateTime now) {
  final issued = DateTime.fromMillisecondsSinceEpoch(
    invitation.issuedAtSecs * 1000,
    isUtc: true,
  );
  return now.toUtc().difference(issued);
}

String _items(int count) => count == 1 ? '1 item' : '$count items';

String _spoken(Duration age) {
  if (age.inDays >= 1) return '${age.inDays} day${age.inDays == 1 ? '' : 's'}';
  if (age.inHours >= 1) {
    return '${age.inHours} hour${age.inHours == 1 ? '' : 's'}';
  }
  return '${age.inMinutes} minute${age.inMinutes == 1 ? '' : 's'}';
}

class _Warning extends StatelessWidget {
  const _Warning({required this.message, required this.blocking});

  final String message;

  /// Whether this names something that will actually prevent the transfer,
  /// rather than something merely worth knowing.
  final bool blocking;

  @override
  Widget build(BuildContext context) {
    final colour = blocking ? AppColors.danger : AppColors.textDim;
    return Container(
      key: const Key('scannedInvitationWarning'),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: colour.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(AppRadius.card),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            blocking ? Icons.wifi_off_outlined : Icons.schedule_outlined,
            size: 18,
            color: colour,
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Text(message, style: AppText.secondary(color: colour)),
          ),
        ],
      ),
    );
  }
}
