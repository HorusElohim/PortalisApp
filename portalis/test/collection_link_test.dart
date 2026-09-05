import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/app/collection_link.dart';
import 'package:portalis/features/collections/domain/paste.dart';
import 'package:portalis/nexus/domain/app_state.dart';

const _hash = '0123456789abcdef0123456789abcdef01234567';

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

    /// The OS routes by scheme *and host*, so a host the app produces but does
    /// not register is delivered nowhere: scanning the code does nothing at
    /// all and the app never learns a link existed. That is exactly how the
    /// first invitation build shipped — the manifest still filtered only the
    /// older `import` host — and nothing in Dart could have caught it, because
    /// every layer this side of the OS handled the link correctly.
    test('every host the app can produce is registered on Android', () {
      final manifest = File(
        'android/app/src/main/AndroidManifest.xml',
      ).readAsStringSync();

      final hosts = {
        // Wraps a magnet.
        Uri.parse(collectionShareLink('magnet:?xt=urn:btih:$_hash')).host,
        // Carries a versioned invitation envelope.
        Uri.parse(collectionShareLink('${invitationPrefix}AAAA')).host,
      };

      for (final host in hosts) {
        expect(
          manifest,
          contains('android:scheme="portalis" android:host="$host"'),
          reason: 'portalis://$host/ is produced but not registered, so '
              'Android will not deliver it to the app',
        );
      }
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

    test('Android gallery bridge keeps the Rust JNI class name public', () {
      final activity = File(
        'android/app/src/main/kotlin/com/portalis/MainActivity.kt',
      ).readAsStringSync();

      expect(activity, contains('\nobject PortalisGallery {'));
      expect(activity, isNot(contains('\nprivate object PortalisGallery {')));
      expect(activity, contains('fun exportToMediaStore('));
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
            progressBuckets: Uint8List(0),
          ),
        ],
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
