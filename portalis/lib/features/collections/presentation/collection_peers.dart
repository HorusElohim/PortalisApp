import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/peer_chip.dart';
import '../../../theme.dart';
import '../domain/collection.dart';
import '../domain/peer_observation.dart';
import 'collection_presentation.dart';

/// Presents identified collaborators and anonymous torrent observations as one
/// peer surface while keeping their identity guarantees visually distinct.
class CollectionPeers extends StatelessWidget {
  const CollectionPeers({
    super.key,
    required this.collection,
    this.peerHistory = const [],
  });

  final Collection collection;
  final List<PeerObservation> peerHistory;

  @override
  Widget build(BuildContext context) {
    final namedPeers = collection.collaborators.take(6).toList();
    final torrentPeers = _torrentPeers;
    final total = collection.collaborators.length + torrentPeers.length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionLabel('PEERS - $total'),
        const SizedBox(height: 7),
        if (torrentPeers.isNotEmpty && collection.totalBytes > 0) ...[
          _CollectionTransferProgress(collection: collection),
          const SizedBox(height: 10),
        ],
        if (total == 0)
          Text(
            collection.livePeers == 0
                ? 'No peers connected right now'
                : '${collection.peersLabel} connected - addresses unavailable',
            style:
                monoLabel(size: 10, color: AppColors.textDim, letterSpacing: 0),
          )
        else
          Wrap(
            spacing: 8,
            runSpacing: 7,
            children: [
              for (final collaborator in namedPeers)
                _NamedPeer(collaborator: collaborator),
              if (collection.collaborators.length > namedPeers.length)
                PeerChip(
                  label:
                      '+${collection.collaborators.length - namedPeers.length} more',
                ),
              for (final peer in torrentPeers)
                _AnonymousPeer(
                  peer: peer,
                  active: collection.torrentPeers.contains(peer.address),
                ),
            ],
          ),
      ],
    );
  }

  List<PeerObservation> get _torrentPeers {
    if (peerHistory.isNotEmpty) return peerHistory;
    final now = DateTime.now();
    return [
      for (final address in collection.torrentPeers)
        PeerObservation(
          collectionId: collection.id,
          collectionName: collection.name,
          address: address,
          lastSeen: now,
        ),
    ];
  }
}

class _NamedPeer extends StatelessWidget {
  const _NamedPeer({required this.collaborator});

  final Collaborator collaborator;

  @override
  Widget build(BuildContext context) => PeerChip(
        label: collaborator.name,
        leading: Avatar(initials: collaborator.initials, size: 20),
      );
}

class _AnonymousPeer extends StatelessWidget {
  const _AnonymousPeer({
    required this.peer,
    required this.active,
  });

  final PeerObservation peer;
  final bool active;

  @override
  Widget build(BuildContext context) => PeerChip(
        label: peer.address,
        detail: formatLastSeen(peer.lastSeen),
        color: active ? AppColors.ember : rememberedPeerColor(peer.address),
      );
}

/// Bytes received by this device for the collection, not an invented split by
/// peer. BitTorrent addresses identify a current connection but do not expose
/// a durable, trustworthy contribution ledger after it leaves.
class _CollectionTransferProgress extends StatelessWidget {
  const _CollectionTransferProgress({required this.collection});

  final Collection collection;

  @override
  Widget build(BuildContext context) {
    final color = collection.hue;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(
              'COLLECTION TRANSFER',
              style: monoLabel(
                size: 9,
                color: AppColors.textDim,
                letterSpacing: 0.35,
              ),
            ),
            const Spacer(),
            Text(
              formatProgressPercent(collection.progress),
              style: monoLabel(
                size: 10,
                color: color,
                weight: FontWeight.w700,
              ),
            ),
          ],
        ),
        const SizedBox(height: 5),
        ClipRRect(
          borderRadius: BorderRadius.circular(AppRadius.pill),
          child: LinearProgressIndicator(
            key: const Key('collectionPeerTransferProgress'),
            value: collection.progress.clamp(0.0, 1.0).toDouble(),
            minHeight: 4,
            backgroundColor: AppColors.borderStrong,
            valueColor: AlwaysStoppedAnimation(color),
          ),
        ),
        const SizedBox(height: 5),
        Text(
          '${formatBytes(collection.downloadedBytes)} of ${formatBytes(collection.totalBytes)} received on this device',
          style: monoLabel(size: 9, color: AppColors.textDim),
        ),
      ],
    );
  }
}
