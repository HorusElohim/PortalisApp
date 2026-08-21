import 'package:flutter/material.dart';

import '../../../design/design.dart';
import '../../../design/peer_chip.dart';
import '../../../design/theme.dart';
import '../../../nexus/domain/app_state.dart';

/// Presents generated collaborators and anonymous live swarm addresses.
class CollectionPeers extends StatelessWidget {
  const CollectionPeers({
    super.key,
    required this.collection,
    required this.detail,
    required this.contacts,
  });

  final AppCollection collection;
  final AppDetail? detail;
  final List<AppContact> contacts;

  @override
  Widget build(BuildContext context) {
    final namedPeers = collection.collaboratorsIn(contacts).take(6).toList();
    final addresses = detail?.peers ?? const <String>[];
    final total = namedPeers.length + addresses.length;
    final livePeers = collection.livePeersFor(detail);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionLabel('PEERS - $total'),
        const SizedBox(height: 7),
        if (addresses.isNotEmpty && collection.totalBytesInt > 0) ...[
          _CollectionTransferProgress(collection: collection),
          const SizedBox(height: 10),
        ],
        if (total == 0)
          Text(
            livePeers == 0
                ? 'No peers connected right now'
                : '$livePeers peer${livePeers == 1 ? '' : 's'} connected - addresses unavailable',
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
              if (collection.collaboratorsIn(contacts).length >
                  namedPeers.length)
                PeerChip(
                  label:
                      '+${collection.collaboratorsIn(contacts).length - namedPeers.length} more',
                ),
              for (final address in addresses) _AnonymousPeer(address: address),
            ],
          ),
      ],
    );
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
  const _AnonymousPeer({required this.address});

  final String address;

  @override
  Widget build(BuildContext context) => PeerChip(
        label: address,
        color: AppColors.ember,
      );
}

class _CollectionTransferProgress extends StatelessWidget {
  const _CollectionTransferProgress({required this.collection});

  final AppCollection collection;

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
                  size: 9, color: AppColors.textDim, letterSpacing: 0.35),
            ),
            const Spacer(),
            Text(
              formatProgressPercent(collection.progressFor(null)),
              style: monoLabel(size: 10, color: color, weight: FontWeight.w700),
            ),
          ],
        ),
        const SizedBox(height: 5),
        ClipRRect(
          borderRadius: BorderRadius.circular(AppRadius.pill),
          child: LinearProgressIndicator(
            key: const Key('collectionPeerTransferProgress'),
            value: collection.progressFor(null).clamp(0.0, 1.0).toDouble(),
            minHeight: 4,
            backgroundColor: AppColors.borderStrong,
            valueColor: AlwaysStoppedAnimation(color),
          ),
        ),
        const SizedBox(height: 5),
        Text(
          '${formatBytes(collection.downloadedBytesInt)} of ${formatBytes(collection.totalBytesInt)} received on this device',
          style: monoLabel(size: 9, color: AppColors.textDim),
        ),
      ],
    );
  }
}
