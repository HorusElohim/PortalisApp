import 'package:flutter/foundation.dart';

import '../features/collections/domain/paste.dart';
import '../nexus/domain/app_state.dart';

typedef CollectionImporter = Future<AppAccepted> Function(String source);
typedef CollectionSelectionDownloader = Future<AppAccepted> Function(
  int collection,
  List<int> entries,
);

typedef CollectionDetailWatcher = Stream<AppDetail?> Function(int collection);

/// Wraps a collection's import URI for sharing.
///
/// A Portalis invitation is already an app-routable `portalis://` link, so it
/// is shown as-is. A bare magnet — which older collections and pasted links
/// still produce — keeps its existing wrapper, because iOS Camera can route a
/// registered custom scheme to Portalis while it has no system action for a
/// raw `magnet:` URI.
String collectionShareLink(String uri) => looksLikeInvitation(uri)
    ? uri
    : Uri(
        scheme: 'portalis',
        host: 'import',
        queryParameters: {'magnet': uri},
      ).toString();

/// Returns the validated import payload from a scanned or opened Portalis link.
///
/// Deep links are untrusted input, even after someone deliberately scans a QR.
/// An invitation is handed to the backend intact — it is the backend that owns
/// unwrapping it, and re-deriving a magnet here would put a second, divergent
/// parser for the same bytes in Dart. Anything else must be the wrapped magnet
/// form, restricted by host and checked with the same predicate as the pasted
/// import flow before any bridge command reaches the backend.
String? collectionMagnetFromLink(Uri uri) {
  if (uri.scheme != 'portalis') return null;
  final link = uri.toString();
  if (looksLikeInvitation(link)) return link;
  if (uri.host != 'import') return null;
  final magnet = uri.queryParameters['magnet'];
  return magnet != null && looksLikeMagnet(magnet) ? magnet : null;
}

/// Dispatches one validated collection link through the existing import path.
///
/// Returns the newly assigned collection handle, or `null` without touching
/// the backend when the incoming URI is not a Portalis collection link.
Future<int?> importCollectionLink(
  Uri uri, {
  required CollectionImporter import,
}) async {
  final magnet = collectionMagnetFromLink(uri);
  if (magnet == null) return null;
  debugPrint(
    '[collection-link] import start source_len=${magnet.length}',
  );
  final accepted = await import(magnet);
  debugPrint(
    '[collection-link] import result collection=${accepted.collection}',
  );
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
  required CollectionSelectionDownloader download,
  required CollectionDetailWatcher watchDetail,
}) async {
  debugPrint('[collection-link] download start collection=$collection');
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
  debugPrint(
    '[collection-link] download selection collection=$collection entries=$entries',
  );
  await download(collection, entries);
  debugPrint(
      '[collection-link] download command accepted collection=$collection');
}
