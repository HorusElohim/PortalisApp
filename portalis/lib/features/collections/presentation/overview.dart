import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/app_state.dart';
import 'commands.dart';
import 'peers.dart';

/// Live generated collection facts and actions, independent of navigation.
class CollectionOverview extends StatelessWidget {
  const CollectionOverview({
    super.key,
    required this.collection,
    required this.detail,
    required this.readings,
    required this.contacts,
    required this.busy,
    required this.onCommand,
    this.showCommands = true,
    this.editing = false,
    this.paused = false,
    this.showTitle = true,
  });

  final AppCollection collection;
  final AppDetail? detail;
  final List<Reading> readings;
  final List<AppContact> contacts;
  final bool busy;
  final ValueChanged<CollectionCommand> onCommand;
  final bool showCommands;
  final bool editing;
  final bool paused;
  final bool showTitle;

  @override
  Widget build(BuildContext context) {
    final history = [
      for (final reading in readings)
        TransferPoint(
          at: reading.at,
          downBytesPerSecond: reading.downBytesPerSecond,
          upBytesPerSecond: reading.upBytesPerSecond,
        ),
    ];
    final livePeers = collection.livePeersFor(detail);
    final hasTransfer = collection.totalBytesInt > 0 ||
        collection.downBytesPerSecond > 0 ||
        collection.upBytesPerSecond > 0 ||
        history.isNotEmpty;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const _CollectionControls(),
        if (showTitle) ...[
          Text(
            collection.name,
            style: displayText(size: 23, weight: FontWeight.w700),
          ),
          const SizedBox(height: 8),
          Text(
            collection.isShared
                ? 'Shared collection - ${collection.subtitleFor(detail)}'
                : 'Torrent - ${collection.subtitleFor(detail)}',
            style: monoLabel(
              size: 12,
              color: AppColors.textDim,
              letterSpacing: 0,
            ),
          ),
          const SizedBox(height: 6),
          CopiesIndicator(
            color: collection.hue,
            label: collection.copiesLabelFor(detail),
            fontSize: 13,
          ),
        ],
        if (hasTransfer) ...[
          const SizedBox(height: 10),
          TransferPanel(
            progress:
                collection.progressFor(readings.isEmpty ? null : readings.last),
            downloadedBytes: collection.downloadedBytesInt,
            totalBytes: collection.totalBytesInt,
            downBytesPerSecond: collection.downBytesPerSecond,
            upBytesPerSecond: collection.upBytesPerSecond,
            history: history,
            startedAt: collection.startedAtMoment,
            completedAt: collection.completedAtMoment,
            livePeers: livePeers,
            etaLabel: collection.etaLabel,
            color: collection.hue,
          ),
        ],
        if (showCommands) ...[
          const SizedBox(height: 14),
          CollectionCommandBar(
            busy: busy,
            onCommand: onCommand,
            editing: editing,
            paused: paused,
          ),
          const SizedBox(height: 12),
        ],
        _CollectionIdentifiers(collection: collection),
        const SizedBox(height: 14),
        CollectionPeers(
          collection: collection,
          detail: detail,
          contacts: contacts,
        ),
        const SizedBox(height: 14),
      ],
    );
  }
}

class _CollectionControls extends StatelessWidget {
  const _CollectionControls();

  @override
  Widget build(BuildContext context) => const SizedBox.shrink();
}

class _CollectionIdentifiers extends StatelessWidget {
  const _CollectionIdentifiers({required this.collection});

  final AppCollection collection;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(top: 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            InfoRow(
              label: 'Type',
              value: collection.isShared
                  ? 'Shared collection - invite-based, can grow'
                  : 'Torrent - fixed contents',
            ),
            InfoRow(
              label: 'State',
              value: collection.status.isEmpty ? 'Unknown' : collection.status,
            ),
            if (collection.uploadedBytesInt > 0)
              InfoRow(
                label: 'Uploaded',
                value: formatBytesPrecise(collection.uploadedBytesInt),
              ),
            if (collection.revisionInt > 0)
              InfoRow(
                label: 'Revision',
                value: '${collection.revisionInt}',
                monospace: true,
              ),
            InfoRow(
              label: collection.isShared ? 'Collection id' : 'Info hash',
              value: collection.stringId,
              monospace: true,
              copyable: true,
            ),
          ],
        ),
      );
}
