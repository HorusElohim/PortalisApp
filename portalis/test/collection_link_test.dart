import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/app/collection_link.dart';
import 'package:portalis/nexus/domain/app_state.dart';

void main() {
  group('collection links', () {
    const magnet =
        'magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567'
        '&x.pe=192.168.1.42:61234';

    test('wrap a magnet in a Portalis import URI', () {
      final link = collectionShareLink(magnet);
      final uri = Uri.parse(link);

      expect(uri.scheme, 'portalis');
      expect(uri.host, 'import');
      expect(collectionMagnetFromLink(uri), magnet);
    });

    test('reject a foreign or unusable import URI', () {
      expect(
        collectionMagnetFromLink(Uri.parse('https://example.test/import')),
        isNull,
      );
      expect(
        collectionMagnetFromLink(
          Uri.parse('portalis://import?magnet=https%3A%2F%2Fexample.test'),
        ),
        isNull,
      );
    });

    test('carry an invitation through to the backend intact', () {
      // Produced by the Rust encoder; Dart deliberately does not parse the
      // envelope, so the link must survive both directions byte for byte.
      const invitation = 'portalis://c/AauqqqqqqqqqqqqqqqqqqqqqqwAAAA8';

      expect(collectionShareLink(invitation), invitation,
          reason: 'already app-routable; must not be wrapped again');
      expect(
        collectionMagnetFromLink(Uri.parse(invitation)),
        invitation,
        reason: 'the backend owns unwrapping it',
      );
    });

    test('reject a portalis URI that is neither invitation nor import', () {
      expect(
        collectionMagnetFromLink(Uri.parse('portalis://elsewhere/AAAA')),
        isNull,
      );
    });

    test('imports only a validated collection link', () async {
      final sources = <String>[];
      final imported = await importCollectionLink(
        Uri.parse(collectionShareLink(magnet)),
        import: (source) async {
          sources.add(source);
          return AppAccepted(id: BigInt.one, collection: 7, queued: false);
        },
      );

      expect(imported, 7);
      expect(sources, [magnet]);
    });

    test('a collection link starts downloading its resolved selection',
        () async {
      final sources = <String>[];
      final selections = <(int, List<int>)>[];
      final details = StreamController<AppDetail?>();
      addTearDown(details.close);

      final imported = await importCollectionLink(
        Uri.parse(collectionShareLink(magnet)),
        import: (source) async {
          sources.add(source);
          return AppAccepted(id: BigInt.one, collection: 7, queued: false);
        },
      );
      final starting = startCollectionLinkDownload(
        imported!,
        download: (collection, entries) async {
          selections.add((collection, entries));
          return AppAccepted(
              id: BigInt.from(2), collection: null, queued: false);
        },
        watchDetail: (_) => details.stream,
      );

      details.add(AppDetail(
        id: 7,
        entries: [
          AppEntry(
            id: 4,
            label: 'mac-only.mov',
            bytes: BigInt.from(12),
            selected: true,
            available: false,
            downloadedBytes: BigInt.zero,
          ),
        ],
        pieces: Uint8List(0),
        peers: const [],
      ));
      await starting;

      expect(sources, [magnet]);
      expect(selections, hasLength(1));
      expect(selections.single.$1, 7);
      expect(selections.single.$2, [4]);
    });
  });
}
