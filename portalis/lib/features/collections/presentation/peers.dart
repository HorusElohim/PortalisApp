import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/peer_chip.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/app_state.dart';
import '../domain/collection.dart';
import '../domain/peer_observation.dart';
import 'peer_color.dart';

/// Presents identified collaborators and anonymous swarm connections as one
/// peer surface while keeping their identity guarantees visually distinct.
///
/// Collaborators are chips: a name is all there is to say about somebody this
/// collection is shared with, because neither engine attributes bytes to a
/// signed identity. Connections are rows, because there *is* more to say —
/// what each has sent and received, and how fast — and those figures are this
/// device's own measurements rather than anybody's claim.
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
    final connections = _connections;
    final total = collection.collaborators.length + connections.length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionLabel('PEERS - $total'),
        const SizedBox(height: 7),
        if (total == 0)
          Text(
            collection.livePeers == 0
                ? 'No peers connected right now'
                : '${collection.peersLabel} connected - addresses unavailable',
            style:
                monoLabel(size: 10, color: AppColors.textDim, letterSpacing: 0),
          )
        else ...[
          if (namedPeers.isNotEmpty) ...[
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
              ],
            ),
            if (connections.isNotEmpty) const SizedBox(height: 10),
          ],
          for (final peer in connections)
            Padding(
              padding: const EdgeInsets.only(bottom: 6),
              child: PeerRow(peer: peer),
            ),
        ],
      ],
    );
  }

  /// The live connections, busiest first, so the peer actually carrying the
  /// transfer is the one at the top rather than whichever connected first.
  List<PeerObservation> get _connections {
    final peers = peerHistory.isNotEmpty
        ? [...peerHistory]
        : [
            for (final peer in collection.torrentPeers)
              PeerObservation(
                collectionId: collection.id,
                collectionName: collection.name,
                address: peer.address,
                lastSeen: DateTime.now(),
                client: peer.client,
                downBytes: peer.downBytes.toInt(),
                upBytes: peer.upBytes.toInt(),
                downBytesPerSecond: peer.downBytesPerSecond,
                upBytesPerSecond: peer.upBytesPerSecond,
              ),
          ];
    peers.sort((a, b) {
      final moving = (b.downBytesPerSecond + b.upBytesPerSecond)
          .compareTo(a.downBytesPerSecond + a.upBytesPerSecond);
      if (moving != 0) return moving;
      return (b.downBytes + b.upBytes).compareTo(a.downBytes + a.upBytes);
    });
    return peers;
  }
}

class _NamedPeer extends StatelessWidget {
  const _NamedPeer({required this.collaborator});

  final AppContact collaborator;

  @override
  Widget build(BuildContext context) => PeerChip(
        label: collaborator.displayName,
        leading: Avatar(
          initials: collaborator.displayName.isEmpty
              ? '?'
              : collaborator.displayName[0].toUpperCase(),
          size: 20,
        ),
      );
}

/// One swarm connection on a collection page.
///
/// The address leads because it is the only part this device can vouch for;
/// the client name follows as something the peer reported about itself. The
/// figures on the right are measurements.
class PeerRow extends StatelessWidget {
  const PeerRow({super.key, required this.peer});

  final PeerObservation peer;

  @override
  Widget build(BuildContext context) {
    final color =
        peer.isMoving ? AppColors.ember : rememberedPeerColor(peer.address);
    return Row(
      children: [
        Container(
          width: 6,
          height: 6,
          decoration: BoxDecoration(color: color, shape: BoxShape.circle),
        ),
        const SizedBox(width: 9),
        Expanded(
          child: Text(
            peer.client == null
                ? peer.address
                : '${peer.address}  ·  reports ${peer.client}',
            overflow: TextOverflow.ellipsis,
            // The address carries the same colour as the dot: active peers
            // read as ember, settled ones keep their own stable identity
            // colour. Distinguishable at a glance without reading the figures.
            style: monoLabel(size: 10, color: color, letterSpacing: 0),
          ),
        ),
        const SizedBox(width: 8),
        Text(
          _figures,
          style: monoLabel(
            size: 10,
            color: peer.isMoving ? AppColors.textDim : AppColors.textFaint,
            letterSpacing: 0,
          ),
        ),
      ],
    );
  }

  /// Rates while something is moving, totals once it is not — a connected peer
  /// that has gone quiet says so rather than showing a rate that has stopped
  /// being true.
  String get _figures {
    if (peer.isMoving) {
      return [
        if (peer.downBytesPerSecond > 0)
          '↓ ${formatRate(peer.downBytesPerSecond)}',
        if (peer.upBytesPerSecond > 0) '↑ ${formatRate(peer.upBytesPerSecond)}',
      ].join('  ');
    }
    if (!peer.hasExchanged) return 'idle';
    return '↓ ${formatBytes(peer.downBytes)}  ↑ ${formatBytes(peer.upBytes)}';
  }
}
