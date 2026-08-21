import '../features/collections/domain/paste.dart';
import '../nexus/domain/app_state.dart';

typedef CollectionCommandSender = Future<AppAccepted> Function(
  EngineCommand command,
);

typedef CollectionDetailWatcher = Stream<AppDetail?> Function(int collection);

/// Wraps a collection's existing import URI in the app-owned URL scheme.
///
/// iOS Camera can route a registered custom scheme to Portalis, while it has no
/// system action for a raw `magnet:` URI. The magnet remains the payload the
/// Nexus import path already validates and understands.
String collectionShareLink(String magnet) => Uri(
      scheme: 'portalis',
      host: 'import',
      queryParameters: {'magnet': magnet},
    ).toString();

/// Returns the validated magnet payload from a Portalis collection link.
///
/// Deep links are untrusted input, even after someone deliberately scans a QR.
/// Restrict the host and reuse the same magnet predicate as the pasted-import
/// flow before any bridge command reaches the backend.
String? collectionMagnetFromLink(Uri uri) {
  if (uri.scheme != 'portalis' || uri.host != 'import') return null;
  final magnet = uri.queryParameters['magnet'];
  return magnet != null && looksLikeMagnet(magnet) ? magnet : null;
}

/// Dispatches one validated collection link through the existing import path.
///
/// Returns the newly assigned collection handle, or `null` without touching
/// the backend when the incoming URI is not a Portalis collection link.
Future<int?> importCollectionLink(
  Uri uri, {
  required CollectionCommandSender send,
}) async {
  final magnet = collectionMagnetFromLink(uri);
  if (magnet == null) return null;
  final accepted = await send(EngineCommand.importTorrent(magnet));
  return accepted.collection;
}

/// Confirms the default selection after a collection link resolves its
/// descriptor, starting the receiver-side torrent download.
///
/// A Portalis QR names content another device is carrying. Its completion must
/// therefore be `downloadSelection`, never `publishDraft`: publication is
/// reserved for local files this device owns.
Future<void> startCollectionLinkDownload(
  int collection, {
  required CollectionCommandSender send,
  required CollectionDetailWatcher watchDetail,
}) async {
  final detail = await watchDetail(collection).firstWhere(
    (detail) => detail != null && detail.entries.isNotEmpty,
  );
  final entries = [
    for (final entry in detail!.entries)
      if (entry.selected) entry.id,
  ];
  if (entries.isEmpty) {
    throw StateError('The shared collection did not select any files');
  }
  await send(EngineCommand(
    kind: 'downloadSelection',
    collection: collection,
    entries: entries,
  ));
}
