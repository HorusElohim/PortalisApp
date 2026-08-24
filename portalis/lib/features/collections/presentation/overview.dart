import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/theme.dart';
import '../domain/collection.dart';
import '../domain/peer_observation.dart';
import '../domain/transfer_history.dart';
import 'commands.dart';
import 'peers.dart';
import 'peer_color.dart';

/// Live collection facts and actions, independent of navigation and commands.
class CollectionOverview extends StatelessWidget {
  const CollectionOverview({
    super.key,
    required this.collection,
    required this.busy,
    required this.onCommand,
    this.history,
    this.showCommands = true,
    this.editing = false,
    this.paused = false,
    this.showTitle = true,
    required this.onAddMedia,
    required this.onFetch,
    this.onShareQr,
    this.peerHistory = const [],
  });

  final Collection collection;
  final bool busy;
  final ValueChanged<CollectionCommand> onCommand;
  final TransferHistory? history;
  final bool showCommands;

  final bool editing;

  /// Which half of the start/stop pair the command bar offers.
  final bool paused;
  final bool showTitle;
  final VoidCallback onAddMedia;
  final VoidCallback onFetch;
  final VoidCallback? onShareQr;
  final List<PeerObservation> peerHistory;

  @override
  Widget build(BuildContext context) {
    final transferHistory = [
      for (final sample in history?.samples ?? const <TransferSample>[])
        TransferPoint(
          at: sample.at,
          downBytesPerSecond: sample.downBytesPerSecond,
          upBytesPerSecond: sample.upBytesPerSecond,
        ),
    ];
    final commandBusy = busy;
    final commandBar = CollectionCommandBar(
      busy: commandBusy,
      onCommand: onCommand,
      editing: editing,
      paused: paused,
      trailingActions: [
        if (onShareQr != null)
          OutlineActionButton(
            key: const Key('collectionShareQr'),
            label: 'Share QR',
            icon: Icons.qr_code_2_outlined,
            compact: true,
            tooltip: 'Show a QR code to import this collection',
            onTap: commandBusy ? null : onShareQr,
          ),
        if (collection.pendingMedia > 0)
          PillButton(
            label: 'Fetch ${collection.pendingMedia}',
            onTap: commandBusy ? null : onFetch,
          ),
      ],
    );
    final hasTransfer = collection.totalBytes > 0 ||
        collection.downBytesPerSecond > 0 ||
        collection.upBytesPerSecond > 0 ||
        transferHistory.isNotEmpty;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _CollectionControls(
          collection: collection,
        ),
        if (showTitle) ...[
          Text(
            collection.name,
            style: displayText(size: 23, weight: FontWeight.w700),
          ),
          const SizedBox(height: 8),
          Text(
            collection.isShared
                ? 'Shared collection - ${collection.subtitle}'
                : 'Torrent - ${collection.subtitle}',
            style: monoLabel(
              size: 12,
              color: AppColors.textDim,
              letterSpacing: 0,
            ),
          ),
          const SizedBox(height: 6),
          CopiesIndicator(
            color: collection.hue,
            label: collection.copiesLabel,
            fontSize: 13,
          ),
        ],
        if (hasTransfer) ...[
          const SizedBox(height: 10),
          TransferPanel(
            progress: collection.progress,
            downloadedBytes: collection.downloadedBytes,
            totalBytes: collection.totalBytes,
            downBytesPerSecond: collection.downBytesPerSecond,
            upBytesPerSecond: collection.upBytesPerSecond,
            sourceReading: collection.isSourceReading,
            history: transferHistory,
            // The core's own moments, not the span of surviving readings.
            startedAt: collection.startedAt ?? history?.startedAt,
            completedAt: collection.completedAt,
            livePeers: collection.livePeers,
            etaLabel: collection.etaLabel,
            color: collection.hue,
          ),
        ],
        if (showCommands) ...[
          const SizedBox(height: 14),
          commandBar,
          const SizedBox(height: 12),
        ],
        ...[
          _CollectionIdentifiers(collection: collection),
          const SizedBox(height: 14),
          CollectionPeers(
            collection: collection,
            peerHistory: peerHistory,
          ),
          const SizedBox(height: 14),
        ],
      ],
    );
  }
}

class _CollectionControls extends StatelessWidget {
  const _CollectionControls({required this.collection});

  final Collection collection;

  @override
  Widget build(BuildContext context) {
    final admins = 0;
    return Row(
      children: [
        if (admins > 0)
          Text(
            '$admins admin${admins == 1 ? '' : 's'}',
            style:
                monoLabel(size: 10, color: AppColors.textDim, letterSpacing: 0),
          ),
        const Spacer(),
      ],
    );
  }
}

class _CollectionIdentifiers extends StatelessWidget {
  const _CollectionIdentifiers({required this.collection});

  final Collection collection;

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
              value: collection.state.isEmpty ? 'Unknown' : collection.state,
            ),
            if (collection.uploadedBytes > 0)
              InfoRow(
                label: 'Uploaded',
                value: formatBytesPrecise(collection.uploadedBytes),
              ),
            // Only once there is one: a draft has never been published, and
            // "revision 0" would read as a version rather than as none.
            if (collection.revision > 0)
              InfoRow(
                label: 'Revision',
                value: '${collection.revision}',
                monospace: true,
              ),
            InfoRow(
              label: collection.isShared ? 'Collection id' : 'Local handle',
              value: collection.id,
              monospace: true,
              copyable: true,
            ),
          ],
        ),
      );
}
