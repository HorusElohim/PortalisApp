import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/nexus/application/app_controller.dart';
import 'package:portalis/nexus/data/app_repository.dart';
import 'package:portalis/nexus/data/collection_source.dart';
import 'package:portalis/nexus/domain/app_state.dart';

void main() {
  test('fetchMedia resumes the selected unresolved files', () async {
    final repository = _Repository();
    final controller = AppController(repository: repository);
    final details = StreamController<AppDetail?>();
    controller.debugSeed(null, details: details.stream);
    final source =
        EngineCollectionSource(controller: controller, collectionId: 7);
    addTearDown(() async {
      source.dispose();
      await details.close();
    });

    details.add(AppDetail(
      id: 7,
      entries: [
        AppEntry(
          id: 1,
          label: 'pending.mov',
          bytes: BigInt.from(20),
          selected: true,
          available: false,
          downloadedBytes: BigInt.zero,
        ),
        AppEntry(
          id: 2,
          label: 'complete.jpg',
          bytes: BigInt.from(10),
          selected: true,
          available: true,
          downloadedBytes: BigInt.from(10),
        ),
        AppEntry(
          id: 3,
          label: 'excluded.png',
          bytes: BigInt.from(5),
          selected: false,
          available: false,
          downloadedBytes: BigInt.zero,
        ),
      ],
      pieces: Uint8List(0),
      peers: const [],
    ));
    await Future<void>.delayed(Duration.zero);

    final fetched = await source.fetchMedia('ignored');

    expect(fetched, 1);
    expect(
      repository.commands.map((command) => command.kind),
      ['downloadSelection', 'setPaused'],
    );
    expect(repository.commands.first.collection, 7);
    expect(repository.commands.first.entries, [1]);
    expect(repository.commands.last.collection, 7);
    expect(repository.commands.last.paused, isFalse);
  });

  test('completion reloads the durable peers before live peers disappear',
      () async {
    final repository = _Repository()
      ..peerHistoryAnswers = [
        const [],
        [
          AppPeerHistory(
            address: '203.0.113.5:6881',
            client: 'qBittorrent 4.6',
            firstSeenAt: BigInt.one,
            lastSeenAt: BigInt.two,
            downBytes: BigInt.from(4000000),
            upBytes: BigInt.from(1000000),
            lastDownBytesPerSecond: 512000,
            lastUpBytesPerSecond: 64000,
          ),
        ],
      ];
    final controller = AppController(repository: repository);
    controller.debugSeed(_state(completedAt: null));
    final source =
        EngineCollectionSource(controller: controller, collectionId: 7);
    addTearDown(source.dispose);
    await Future<void>.delayed(Duration.zero);
    expect(source.peerHistoryFor('7'), isEmpty);

    controller.debugSeed(_state(completedAt: BigInt.two));
    await Future<void>.delayed(Duration.zero);

    expect(repository.peerHistoryCalls, 2);
    expect(source.peerHistoryFor('7'), hasLength(1));
    expect(source.peerHistoryFor('7').single.downBytesPerSecond, 512000);
  });
}

AppSnapshot _state({required BigInt? completedAt}) => AppSnapshot(
      device: const AppDevice(
        name: 'Portalis',
        handle: null,
        fingerprint: 'test-fingerprint',
        devices: 1,
      ),
      connectivity: 'LocalOnly',
      contacts: const [],
      collections: [
        AppCollection(
          id: 7,
          name: 'Iceland',
          nature: 'Torrent',
          role: 'Receiver',
          revision: BigInt.one,
          status: completedAt == null ? 'Downloading' : 'Available',
          members: Uint32List(0),
          entries: 1,
          totalBytes: BigInt.from(4000000),
          onDiskBytes: BigInt.from(4000000),
          uploadedBytes: BigInt.zero,
          completedAt: completedAt,
          transfer: null,
          pending: null,
        ),
      ],
      alerts: const [],
    );

class _Repository implements AppRepository {
  final commands = <EngineCommand>[];
  List<List<AppPeerHistory>> peerHistoryAnswers = const [];
  int peerHistoryCalls = 0;

  @override
  Future<void> start() async {}

  @override
  Future<void> stop() async {}

  @override
  Future<void> setActive(bool active) async {}

  @override
  Stream<AppSnapshot> watchStates() => const Stream.empty();

  @override
  Stream<AppDetail?> watchDetail(int? collection) => const Stream.empty();

  @override
  Future<String?> shareUri(int collection) async => null;

  @override
  Stream<Uint8List> watchHistory(int collection) => const Stream.empty();

  @override
  Future<List<AppCollectionPeer>> peers() async => const [];

  @override
  Future<List<AppPeoplePeer>> peoplePeers() async => const [];

  @override
  Future<List<AppPeerHistory>> peerHistory(int collection) async {
    final answer = peerHistoryCalls < peerHistoryAnswers.length
        ? peerHistoryAnswers[peerHistoryCalls]
        : const <AppPeerHistory>[];
    peerHistoryCalls += 1;
    return answer;
  }

  @override
  Future<String> diagnosticsLog() async => '';

  @override
  Future<void> clearDiagnosticsLog() async {}

  @override
  Future<String> diagnosticsLogPath() async => '';

  @override
  Future<void> logDiagnostic(String tag, String message) async {}

  @override
  Future<AppAccepted> send(EngineCommand command) async {
    commands.add(command);
    return AppAccepted(id: BigInt.one, collection: null, queued: true);
  }
}
