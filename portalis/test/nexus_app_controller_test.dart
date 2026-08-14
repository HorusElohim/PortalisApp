import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/nexus/application/nexus_app_controller.dart';
import 'package:portalis/features/nexus/data/nexus_app_repository.dart';
import 'package:portalis/features/nexus/domain/nexus_app_state.dart';
import 'package:portalis/features/nexus/presentation/nexus_collection_detail.dart';
import 'package:portalis/features/nexus/presentation/nexus_home_library.dart';
import 'package:portalis/features/nexus/presentation/nexus_torrent_preparation.dart';

void main() {
  test('owns one state subscription and forwards lifecycle changes', () async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    var notifications = 0;
    controller.addListener(() => notifications++);

    await controller.start();
    repository.states.add(_state('Mina'));
    await Future<void>.delayed(Duration.zero);

    expect(repository.starts, 1);
    expect(controller.state?.device.name, 'Mina');
    expect(notifications, 1);

    await controller.start();
    await controller.setActive(false);
    expect(repository.starts, 1, reason: 'the subscription is app-owned');
    expect(repository.active, [false]);

    await controller.stop();
    expect(repository.stops, 1);
  });

  test('forwards the selected detail stream without caching it', () async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    final detail = NexusDetail(
      id: 9,
      entries: [
        NexusEntry(
          id: 2,
          label: 'episode.mp4',
          bytes: BigInt.from(34),
          selected: false,
          available: false,
        ),
      ],
      pieces: const [],
      samples: const [],
    );

    final seen = controller.watchDetail(9).first;
    repository.details.add(detail);

    expect((await seen)?.entries.single.selected, isFalse);
    expect(repository.detailCollections, [9]);
  });

  testWidgets('a torrent preparation edits and confirms only selected files',
      (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    await tester.pumpWidget(
      MaterialApp(
        home: NexusTorrentPreparation(collection: 9, controller: controller),
      ),
    );
    repository.details.add(
      NexusDetail(
        id: 9,
        entries: [
          NexusEntry(
            id: 1,
            label: 'trailer.mp4',
            bytes: BigInt.from(5),
            selected: true,
            available: false,
          ),
          NexusEntry(
            id: 2,
            label: 'feature.mp4',
            bytes: BigInt.from(34),
            selected: true,
            available: false,
          ),
        ],
        pieces: const [],
        samples: const [],
      ),
    );
    await tester.pump();

    expect(find.text('trailer.mp4'), findsOneWidget);
    expect(find.text('feature.mp4'), findsOneWidget);
    await tester.tap(find.byKey(const Key('nexusTorrentEntry:1')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('nexusConfirmSelection')));
    await tester.pump();

    expect(repository.commands, hasLength(1));
    expect(repository.commands.single.kind, 'downloadSelection');
    expect(repository.commands.single.collection, 9);
    expect(repository.commands.single.entries, [2]);
  });

  testWidgets('Home renders Nexus collections without a legacy projection',
      (tester) async {
    NexusCollection? opened;
    var created = false;
    final collection = NexusCollection(
      id: 9,
      name: 'Episode archive',
      role: 'Owner',
      revision: BigInt.one,
      status: 'Preparing',
      members: const [],
      entries: 2,
      totalBytes: BigInt.from(39),
      transfer: null,
      pending: null,
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: NexusHomeLibrary(
            wide: false,
            state: NexusAppState(
              device: const NexusDevice(
                name: 'Mina',
                handle: null,
                fingerprint: 'fingerprint',
                devices: 1,
              ),
              connectivity: 'LocalOnly',
              contacts: const [],
              collections: [collection],
              alerts: const [],
            ),
            error: null,
            query: '',
            filter: NexusCollectionFilter.all,
            onSearch: (_) {},
            onFilterChanged: (_) {},
            onImportTorrent: (_) async {},
            onCreateCollection: () => created = true,
            onJoin: (_) {},
            onOpen: (value) => opened = value,
          ),
        ),
      ),
    );

    expect(find.text('Episode archive'), findsOneWidget);
    expect(find.text('PREPARING'), findsOneWidget);
    await tester.tap(find.text('Episode archive'));
    expect(opened, same(collection));
    await tester.tap(find.byKey(const Key('nexusCreateCollection')));
    expect(created, isTrue);
  });

  testWidgets('collection detail sends its delete through Nexus',
      (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<NexusDetail?>.empty(),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: NexusCollectionDetail(collection: 9, controller: controller),
      ),
    );

    await tester.tap(find.byKey(const Key('nexusDeleteCollection')));
    await tester.pump();
    await tester.tap(find.text('Delete collection'));
    await tester.pump();

    expect(repository.commands, hasLength(1));
    expect(repository.commands.single.kind, 'deleteCollection');
    expect(repository.commands.single.collection, 9);
    expect(repository.commands.single.deleteFiles, isFalse);
  });

  testWidgets('collection detail sends its rename through Nexus',
      (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    controller.debugSeed(_collectionState());
    await tester.pumpWidget(
      MaterialApp(
        home: NexusCollectionDetail(collection: 9, controller: controller),
      ),
    );

    await tester.tap(find.byKey(const Key('nexusRenameCollection')));
    await tester.pump();
    await tester.enterText(find.byType(TextField), 'Renamed archive');
    await tester.tap(find.text('Rename').last);
    await tester.pump();

    expect(repository.commands, hasLength(1));
    expect(repository.commands.single.kind, 'renameCollection');
    expect(repository.commands.single.collection, 9);
    expect(repository.commands.single.name, 'Renamed archive');
  });
}

NexusAppState _collectionState() => NexusAppState(
      device: const NexusDevice(
        name: 'Mina',
        handle: null,
        fingerprint: 'fingerprint',
        devices: 1,
      ),
      connectivity: 'LocalOnly',
      contacts: const [],
      collections: [
        NexusCollection(
          id: 9,
          name: 'Episode archive',
          role: 'Owner',
          revision: BigInt.one,
          status: 'Available',
          members: const [],
          entries: 0,
          totalBytes: BigInt.zero,
          transfer: null,
          pending: null,
        ),
      ],
      alerts: const [],
    );

NexusAppState _state(String name) => NexusAppState(
      device: NexusDevice(
        name: name,
        handle: null,
        fingerprint: 'fingerprint',
        devices: 1,
      ),
      connectivity: 'LocalOnly',
      contacts: const [],
      collections: const [],
      alerts: const [],
    );

class _Repository implements NexusAppRepository {
  final states = StreamController<NexusAppState>.broadcast();
  final details = StreamController<NexusDetail?>.broadcast();
  final active = <bool>[];
  final detailCollections = <int?>[];
  final commands = <NexusCommand>[];
  var starts = 0;
  var stops = 0;

  @override
  Future<NexusAccepted> send(NexusCommand command) async {
    commands.add(command);
    return NexusAccepted(id: BigInt.zero, collection: null, queued: false);
  }

  @override
  Future<void> setActive(bool value) async => active.add(value);

  @override
  Future<void> start() async => starts++;

  @override
  Future<void> stop() async {
    stops++;
    await states.close();
    await details.close();
  }

  @override
  Stream<NexusDetail?> watchDetail(int? collection) {
    detailCollections.add(collection);
    return details.stream;
  }

  @override
  Stream<NexusAppState> watchStates() => states.stream;
}
