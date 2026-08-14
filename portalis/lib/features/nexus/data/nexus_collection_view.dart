import '../../collections/domain/collection.dart';
import '../../collections/domain/peer_observation.dart';
import '../../collections/domain/transfer_history.dart';
import '../../media/domain/media_item.dart';
import '../domain/nexus_app_state.dart';

/// The Nexus projection, in the shape the collection screen already draws.
///
/// A view model, not a store. Everything here is derived on each build from
/// the snapshot the core just sent and kept nowhere, so there is still one
/// source of truth — this only translates its vocabulary into the one the
/// existing widgets speak. The moment any of it were cached, it would become
/// the second source of truth the projection exists to remove.
///
/// It earns its place by letting every collection screen — the list row, the
/// inline-expanded card, the pushed detail — render the *same* widgets rather
/// than lookalikes: one set of design decisions, and no chance of two
/// versions disagreeing while both exist.
///
/// [detail] is optional because a list row has no business subscribing to a
/// per-collection detail stream just to draw a summary line — Nexus's own
/// design is that detail costs nothing until something asks for it. Without
/// one, entries are honestly unknown rather than guessed: [Collection.media]
/// becomes a stand-in list whose length is [NexusCollection.entries] and
/// whose items claim nothing else, which is exactly as much as a row without
/// a subscription is entitled to say.
Collection nexusCollectionView({
  required NexusCollection collection,
  required NexusDetail? detail,
  required List<NexusContact> contacts,
}) {
  final entries = detail?.entries;
  final swarm = detail?.peers ?? const <String>[];
  final transfer = collection.transfer;

  return Collection(
    // The process-local handle. `CollectionState` carries no durable public
    // identifier yet, and inventing one here would be worse than showing the
    // handle the rest of the interface is already using.
    id: '${collection.id}',
    name: collection.name,
    kind: collection.nature == 'Torrent'
        ? CollectionKind.torrent
        : CollectionKind.shared,
    collaborators: [
      for (final member in collection.members)
        for (final contact
            in contacts.where((contact) => contact.id == member))
          Collaborator(
            deviceId: contact.fingerprint,
            name: contact.displayName,
          ),
    ],
    media: entries == null
        ? _placeholderMedia(collection.entries)
        : [for (final entry in entries) _media(entry)],
    progress: transfer?.progress ?? detail?.progress ?? 0,
    totalBytes: collection.totalBytes.toInt(),
    downloadedBytes: collection.onDiskBytes.toInt(),
    uploadedBytes: collection.uploadedBytes.toInt(),
    downloadMbps: _megabits(transfer?.downBytesPerSecond ?? 0),
    uploadMbps: _megabits(transfer?.upBytesPerSecond ?? 0),
    livePeers: transfer?.peers ?? swarm.length,
    torrentPeers: swarm,
    // Unknown without a subscription is claimed as none, not guessed — the
    // expanded detail (which has one) reports the real count.
    pendingMedia:
        entries == null ? 0 : entries.where((entry) => !entry.available).length,
    etaSecs: transfer?.etaSecs,
    state: _legacyState(collection, transfer),
  );
}

/// The vocabulary `Collection.state` — and everything built on it,
/// `isSharing`, `isConnecting`, `isMoving`, `CollectionRow`'s glow and its
/// inline progress bar — was written around: lowercase, and the legacy
/// backend's own words. Nexus's `Status` names itself for Rust's `Debug`
/// (`Downloading`, `WaitingForOwner`, …), a different audience entirely, so
/// this is the one place the two vocabularies meet.
///
/// Every status that has no real legacy equivalent falls through unchanged.
/// That is deliberate, not a gap: none of `Collection`'s state-driven
/// behaviour matches an unrecognised string, so a status not translated here
/// still shows correctly — as a plain, uppercase badge — a fallback the
/// legacy backend's own `CollectionRow` already relied on.
String _legacyState(NexusCollection collection, NexusTransfer? transfer) {
  switch (collection.status) {
    case 'Downloading':
      return 'downloading';
    case 'Preparing':
      return 'importing';
    // No content key has arrived — there is nothing to show progress
    // against, which is exactly what legacy's indeterminate "connecting" bar
    // was for.
    case 'WaitingForOwner':
      return 'connecting';
    case 'Available':
      // Actively serving someone right now reads as seeding; settled and
      // idle reads as a plain "AVAILABLE" badge — legacy has no separate
      // word for "complete, but nobody is here for it".
      return transfer != null && transfer.upBytesPerSecond > 0
          ? 'seeding'
          : collection.status;
    default:
      return collection.status;
  }
}

/// A count-only stand-in for entries this device has not asked to see yet.
/// `CollectionRow`'s collapsed view reads only `media.length`, never a field
/// of an individual item, so a placeholder this empty is honest: it claims
/// nothing except how many there are.
List<MediaItem> _placeholderMedia(int count) => List.generate(
      count,
      (_) => const MediaItem(label: '', infoHash: '', sizeBytes: 0),
    );

/// The history the core recorded, as the type the overview reads.
///
/// `null` when nothing has been recorded, so the overview hides the panel
/// rather than drawing an empty chart.
TransferHistory? nexusTransferHistory(NexusDetail? detail) {
  final readings = detail?.readings ?? const [];
  if (readings.isEmpty) return null;
  return TransferHistory.restore(
    startedAt: readings.first.at,
    samples: [
      for (final reading in readings)
        TransferSample(
          at: reading.at,
          downloadMbps: reading.downloadMbps,
          uploadMbps: reading.uploadMbps,
          progress: reading.progress,
        ),
    ],
  );
}

/// The swarm, as the observations the peers surface reads.
///
/// Every address the core reports is one it is connected to now, so each is
/// seen now. There is no remembered-peer tier here: the core reports what is
/// live, and inventing a history of departed addresses would be the interface
/// keeping state the core deliberately does not.
List<PeerObservation> nexusPeerObservations({
  required NexusCollection collection,
  required NexusDetail? detail,
}) {
  final now = DateTime.now();
  return [
    for (final address in detail?.peers ?? const <String>[])
      PeerObservation(
        collectionId: '${collection.id}',
        collectionName: collection.name,
        address: address,
        lastSeen: now,
      ),
  ];
}

MediaItem _media(NexusEntry entry) => MediaItem(
      label: entry.label,
      // A Nexus entry is addressed by its collection and handle. The torrent
      // hash is the substrate's business and does not belong in the interface.
      infoHash: '',
      localPath: entry.path,
      progress: entry.available ? 1 : 0,
      sizeBytes: entry.bytes.toInt(),
      downloadedBytes: entry.available ? entry.bytes.toInt() : 0,
      fetched: entry.available,
    );

double _megabits(int bytesPerSecond) => bytesPerSecond * 8 / 1000000;
