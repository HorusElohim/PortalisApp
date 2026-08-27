/// A network peer seen on one collection.
///
/// This is deliberately not a collaborator. A torrent address has no signed
/// identity, so it can only be remembered as an observation with a timestamp.
///
/// The byte counters and rates are this device's own measurements of the
/// connection. [`client`] is not: it is a name the far end chose to announce,
/// so it is shown as a claim rather than as an identity.
class PeerObservation {
  const PeerObservation({
    required this.collectionId,
    required this.collectionName,
    required this.address,
    required this.lastSeen,
    this.client,
    this.downBytes = 0,
    this.upBytes = 0,
    this.downBytesPerSecond = 0,
    this.upBytesPerSecond = 0,
  });

  final String collectionId;
  final String collectionName;
  final String address;
  final DateTime lastSeen;

  /// What the peer calls itself, when it said. Untrusted by construction.
  final String? client;

  /// Bytes exchanged with this peer on this connection.
  final int downBytes;
  final int upBytes;

  /// Current rates, measured by the core between polls.
  final int downBytesPerSecond;
  final int upBytesPerSecond;

  /// Whether anything is moving with this peer right now. A connected peer
  /// that has gone quiet is still connected, and saying so is more honest
  /// than showing it as active because bytes once passed.
  bool get isMoving => downBytesPerSecond > 0 || upBytesPerSecond > 0;

  /// Whether this connection has ever carried anything. A peer that connected
  /// and exchanged nothing is a real and common state worth distinguishing.
  bool get hasExchanged => downBytes > 0 || upBytes > 0;
}
