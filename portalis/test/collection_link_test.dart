import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/app/collection_link.dart';
import 'package:portalis/nexus/domain/app_state.dart';

void main() {
  group('collection links', () {
    const magnet =
        'magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567';

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
  });
}
