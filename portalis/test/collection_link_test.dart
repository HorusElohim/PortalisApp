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

    test('imports only a validated collection link', () async {
      final commands = <EngineCommand>[];
      final imported = await importCollectionLink(
        Uri.parse(collectionShareLink(magnet)),
        send: (command) async {
          commands.add(command);
          return AppAccepted(id: BigInt.one, collection: 7, queued: false);
        },
      );

      expect(imported, 7);
      expect(commands, hasLength(1));
      expect(commands.single.kind, 'importTorrent');
      expect(commands.single.source, magnet);
    });

    test('a collection link starts downloading its resolved selection',
        () async {
      final commands = <EngineCommand>[];
      final details = StreamController<AppDetail?>();
      addTearDown(details.close);

      final imported = await importCollectionLink(
        Uri.parse(collectionShareLink(magnet)),
        send: (command) async {
          commands.add(command);
          return AppAccepted(id: BigInt.one, collection: 7, queued: false);
        },
      );
      final starting = startCollectionLinkDownload(
        imported!,
        send: (command) async {
          commands.add(command);
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

      expect(commands.map((command) => command.kind),
          ['importTorrent', 'downloadSelection']);
      expect(commands.last.collection, 7);
      expect(commands.last.entries, [4]);
      expect(commands.map((command) => command.kind),
          isNot(contains('publishDraft')));
    });
  });
}
