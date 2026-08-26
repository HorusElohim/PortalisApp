import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/peer_chip.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/app_state.dart';
import '../domain/collection.dart';
import '../domain/peer_observation.dart';
import 'peer_color.dart';

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
