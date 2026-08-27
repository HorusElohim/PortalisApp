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
}

class _Repository implements AppRepository {
  final commands = <EngineCommand>[];

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
  Future<AppAccepted> send(EngineCommand command) async {
    commands.add(command);
    return AppAccepted(id: BigInt.one, collection: null, queued: true);
  }
}
