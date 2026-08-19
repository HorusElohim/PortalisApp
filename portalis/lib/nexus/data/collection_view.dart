import '../../features/collections/domain/collection.dart';
import '../../features/collections/domain/peer_observation.dart';
import '../../features/collections/domain/transfer_history.dart';
import '../domain/app_state.dart';

/// The engine's projection, as the collection screens read it.
///
/// A pairing, not a translation. [Collection] reads through to these values
/// rather than copying them — see its own doc for what the copying cost.
Collection collectionView({
  required AppCollection collection,
  required AppDetail? detail,
  required List<AppContact> contacts,
  Reading? lastReading,
}) =>
    Collection(
      collection,
      detail: detail,
      contacts: contacts,
      lastReading: lastReading,
    );

/// The history the core recorded, as the type the overview reads.
///
/// `null` when nothing has been recorded, so the overview hides the panel
/// rather than drawing an empty chart.
TransferHistory? transferHistory(List<Reading> readings) {
  if (readings.isEmpty) return null;
  return TransferHistory.restore(
    startedAt: readings.first.at,
    samples: [
      for (final reading in readings)
        TransferSample(
          at: reading.at,
          downBytesPerSecond: reading.downBytesPerSecond,
          upBytesPerSecond: reading.upBytesPerSecond,
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
List<PeerObservation> peerObservations({
  required AppCollection collection,
  required AppDetail? detail,
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
