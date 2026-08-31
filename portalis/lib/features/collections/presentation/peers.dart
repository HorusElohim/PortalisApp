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
/// signed identity. Connections are cards, because there *is* more to say —
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
          LayoutBuilder(
            builder: (context, constraints) => GridView.builder(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
                maxCrossAxisExtent: 360,
                crossAxisSpacing: 8,
                mainAxisSpacing: 8,
                mainAxisExtent: 154,
              ),
              itemCount: connections.length,
              itemBuilder: (context, index) =>
                  PeerCard(peer: connections[index]),
            ),
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
/// A card gives the connection a stable visual boundary and makes all four
/// measurements readable together. The client name is still explicitly a
/// report from the peer, not a verified identity.
class PeerCard extends StatelessWidget {
  const PeerCard({super.key, required this.peer, this.contextLabel});

  final PeerObservation peer;
  final String? contextLabel;

  @override
  Widget build(BuildContext context) {
    final color =
        peer.isMoving ? AppColors.ember : rememberedPeerColor(peer.address);
    return SurfaceCard(
      padding: const EdgeInsets.fromLTRB(12, 11, 12, 10),
      borderColor: color.withValues(alpha: peer.isMoving ? 0.5 : 0.28),
      glow: peer.isMoving ? GlowLevel.active : GlowLevel.none,
      glowColor: color,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              // A moving connection pulses so the live/idle split is legible
              // at a glance, not just from the numbers beneath it.
              peer.isMoving
                  ? LiveDot(color: color, size: 8)
                  : Container(
                      width: 8,
                      height: 8,
                      decoration:
                          BoxDecoration(color: color, shape: BoxShape.circle),
                    ),
              const SizedBox(width: 9),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      peer.client ?? 'Unknown client',
                      overflow: TextOverflow.ellipsis,
                      style: displayText(size: 13, weight: FontWeight.w700),
                    ),
                    const SizedBox(height: 3),
                    Text(
                      contextLabel == null
                          ? peer.address
                          : '${peer.address} · $contextLabel',
                      overflow: TextOverflow.ellipsis,
                      style: monoLabel(
                        size: 10,
                        color: AppColors.textDim,
                        letterSpacing: 0,
                      ),
                    ),
                  ],
                ),
              ),
              if (peer.isMoving)
                StatusBadge(label: 'LIVE', color: color, filled: true),
            ],
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                child: _PeerMetric(
                  label: 'DOWNLOADED',
                  value: formatBytesPrecise(peer.downBytes),
                  color: AppColors.signalMuted,
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: _PeerMetric(
                  label: 'UPLOADED',
                  value: formatBytesPrecise(peer.upBytes),
                  color: AppColors.signalMuted,
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              Expanded(
                child: _PeerMetric(
                  label: 'DOWN SPEED',
                  value: formatRate(peer.downBytesPerSecond),
                  color: color,
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: _PeerMetric(
                  label: 'UP SPEED',
                  value: formatRate(peer.upBytesPerSecond),
                  color: color,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _PeerMetric extends StatelessWidget {
  const _PeerMetric({
    required this.label,
    required this.value,
    required this.color,
  });

  final String label;
  final String value;
  final Color color;

  @override
  Widget build(BuildContext context) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            style: monoLabel(size: 9, color: AppColors.textGhost),
          ),
          const SizedBox(height: 4),
          Text(
            value,
            overflow: TextOverflow.ellipsis,
            style: monoLabel(size: 13, color: color, letterSpacing: 0),
          ),
        ],
      );
}
