import 'dart:typed_data';
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/features/collections/domain/picked_file.dart';
import 'package:portalis/features/collections/presentation/collection_overview.dart';
import 'package:portalis/features/collections/presentation/collection_share.dart';
import 'package:portalis/nexus/application/app_controller.dart';
import 'package:portalis/nexus/data/app_repository.dart';
import 'package:portalis/nexus/data/collection_source.dart';
import 'package:portalis/nexus/domain/app_state.dart';
import 'package:portalis/features/collections/presentation/collection_route.dart';
import 'package:portalis/features/collections/presentation/home_library.dart';

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
    final detail = AppDetail(
      id: 9,
      entries: [
        AppEntry(
          id: 2,
          label: 'episode.mp4',
          bytes: BigInt.from(34),
          selected: false,
          available: false,
        ),
      ],
      pieces: Uint8List(0),
      samples: Uint8List(0),
      peers: const [],
    );

    final seen = controller.watchDetail(9).first;
    repository.details.add(detail);

    expect((await seen)?.entries.single.selected, isFalse);
    expect(repository.detailCollections, [9]);
  });

  /// Choosing files happens on the collection itself, for as long as the
  /// collection exists — not on a preparation screen passed through once. The
  /// whole selection is sent each time, so the backend never has to reconcile
  /// a delta against what a screen believed it last saw.
  testWidgets('a torrent collection deselects a file in place', (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    await controller.start();
    await tester.binding.setSurfaceSize(const Size(420, 1400));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: NexusCollectionDetail(collection: 9, controller: controller),
      ),
    );
    repository.states.add(_torrentState());
    await tester.pump();
    repository.details.add(
      AppDetail(
        id: 9,
        entries: [
          AppEntry(
            id: 1,
            label: 'trailer.mp4',
            bytes: BigInt.from(5),
            selected: true,
            available: false,
          ),
          AppEntry(
            id: 2,
            label: 'feature.mp4',
            bytes: BigInt.from(34),
            selected: true,
            available: false,
          ),
        ],
        pieces: Uint8List(0),
        samples: Uint8List(0),
        peers: const [],
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    expect(find.text('trailer.mp4'), findsOneWidget);
    expect(find.text('feature.mp4'), findsOneWidget);
    await tester.tap(find.byKey(const Key('mediaWanted:1')));
    await tester.pump();

    expect(repository.commands, hasLength(1));
    expect(repository.commands.single.kind, 'downloadSelection');
    expect(repository.commands.single.collection, 9);
    expect(repository.commands.single.entries, [2]);
  });

  /// Deselecting the last file would ask the engine to fetch nothing, which
  /// is a collection that exists and does nothing — deleting it is what a
  /// person means by that, so the command is refused rather than sent.
  testWidgets('the last wanted file cannot be deselected', (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    await controller.start();
    await tester.binding.setSurfaceSize(const Size(420, 1400));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: NexusCollectionDetail(collection: 9, controller: controller),
      ),
    );
    repository.states.add(_torrentState());
    await tester.pump();
    repository.details.add(
      AppDetail(
        id: 9,
        entries: [
          AppEntry(
            id: 1,
            label: 'only.mp4',
            bytes: BigInt.from(5),
            selected: true,
            available: false,
          ),
        ],
        pieces: Uint8List(0),
        samples: Uint8List(0),
        peers: const [],
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    await tester.tap(find.byKey(const Key('mediaWanted:1')));
    await tester.pump();

    expect(repository.commands, isEmpty);
  });

  /// Sharing local files needs a no-copy picker, which Android and iOS do
  /// not have — and a Flutter test reports Android unless told otherwise, so
  /// without this the screen correctly refuses and the test looks broken.
  /// The magnet test below deliberately does *not* do this: importing a
  /// torrent must work on every platform.
  testWidgets('the existing New share page creates a Nexus collection',
      (tester) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    var closed = false;
    await tester.pumpWidget(
      MaterialApp(
        home: ShareScreen(
          controller: controller,
          onClose: () => closed = true,
          initialFiles: const [
            PickedFile(
              name: 'episode.mp4',
              path: '/media/episode.mp4',
              lengthBytes: 42,
            ),
          ],
        ),
      ),
    );
    await tester.enterText(
      find.byKey(const Key('collectionNameField')),
      'Episode archive',
    );
    await tester.pump();
    await tester.tap(find.byKey(const Key('createShareButton')));
    await tester.pump();

    expect(repository.commands, hasLength(1));
    final command = repository.commands.single;
    expect(command.kind, 'createCollection');
    expect(command.name, 'Episode archive');
    expect(command.files.single.name, 'episode.mp4');
    expect(command.files.single.path, '/media/episode.mp4');
    expect(command.files.single.bytes, BigInt.from(42));
    expect(closed, isTrue);
    // Reset inline: the framework asserts this is unset by the time the test
    // body returns, which is before any tear-down would run.
    debugDefaultTargetPlatformOverride = null;
  });

  testWidgets(
      'Home renders Nexus collections through the shared legacy row, '
      'translated rather than reimplemented', (tester) async {
    AppCollection? opened;
    var created = false;
    final collection = AppCollection(
      id: 9,
      name: 'Episode archive',
      nature: 'Torrent',
      role: 'Owner',
      revision: BigInt.one,
      status: 'Preparing',
      members: Uint32List(0),
      entries: 2,
      totalBytes: BigInt.from(39),
      onDiskBytes: BigInt.zero,
      uploadedBytes: BigInt.zero,
      transfer: null,
      pending: null,
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: NexusHomeLibrary(
            wide: false,
            state: AppSnapshot(
              device: const AppDevice(
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
            onSearch: (_) {},
            onImportTorrent: (_) async {},
            onCreateCollection: () => created = true,
            onOpen: (value) => opened = value,
            onCommand: (_) {},
          ),
        ),
      ),
    );

    expect(find.text('Episode archive'), findsOneWidget);
    // Preparing maps onto the legacy backend's own "importing" vocabulary —
    // the same word `CollectionRow`'s badge has always shown for it.
    expect(find.text('IMPORTING'), findsOneWidget);
    await tester.tap(find.text('Episode archive'));
    expect(opened, same(collection));
    await tester.tap(find.byKey(const Key('shareCollectionAction')));
    expect(created, isTrue);
  });

  testWidgets('New share imports a magnet without needing a name or files',
      (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    var closed = false;
    await tester.pumpWidget(
      MaterialApp(
        home: ShareScreen(controller: controller, onClose: () => closed = true),
      ),
    );

    await tester.tap(find.byKey(const Key('shareAddTorrent')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('sharePasteMagnet')));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, 'magnet:?xt=urn:btih:abc');
    await tester.tap(find.text('Add').last);
    await tester.pumpAndSettle();

    // A torrent is fetched rather than shared, so it needs neither the
    // collection name nor a file list this screen otherwise insists on.
    expect(repository.commands, hasLength(1));
    expect(repository.commands.single.kind, 'importTorrent');
    expect(repository.commands.single.source, 'magnet:?xt=urn:btih:abc');
    expect(closed, isTrue, reason: 'and it hands over to the selection step');
  });

  /// The bug this exists to prevent: the shell reported "1 ACTIVE TRANSFER"
  /// above a Home showing no collections, because the chrome counted from the
  /// legacy collections controller while the list came from Nexus. Status
  /// chrome that contradicts the list beside it makes a person distrust what
  /// they can see, so activity has exactly one derivation.
  test('activity is idle when Nexus holds nothing, whatever else is running',
      () {
    final controller = NexusAppController(repository: _Repository());

    // Before any state has arrived at all.
    expect(controller.activity.isMoving, isFalse);
    expect(controller.activity.transfers, 0);
    expect(controller.activity.peers, 0);

    // And with a collection that is not transferring.
    controller.debugSeed(_collectionState());
    expect(controller.activity.isMoving, isFalse);
    expect(controller.activity.transfers, 0);
  });

  test('activity sums only what is actually moving', () {
    final controller = NexusAppController(repository: _Repository());
    final idle = _collectionState().collections.single;
    controller.debugSeed(
      AppSnapshot(
        device: const AppDevice(
          name: 'Mina',
          handle: null,
          fingerprint: 'fingerprint',
          devices: 1,
        ),
        connectivity: 'LocalOnly',
        contacts: const [],
        collections: [
          idle,
          AppCollection(
            id: 10,
            name: 'Moving',
            nature: 'Torrent',
            role: 'Owner',
            revision: BigInt.one,
            status: 'Downloading',
            members: Uint32List(0),
            entries: 1,
            totalBytes: BigInt.from(100),
            onDiskBytes: BigInt.from(50),
            uploadedBytes: BigInt.zero,
            transfer: const AppTransfer(
              progress: 0.5,
              downBytesPerSecond: 125000,
              upBytesPerSecond: 250000,
              peers: 3,
              etaSecs: 4,
            ),
            pending: null,
          ),
        ],
        alerts: const [],
      ),
    );

    final activity = controller.activity;
    expect(activity.transfers, 1, reason: 'the idle collection is not counted');
    expect(activity.peers, 3);
    expect(activity.downMbps, 1.0);
    expect(activity.upMbps, 2.0);
    expect(activity.rateMbps, 3.0);
  });

  /// The wide layout's whole point: opening a collection grows its row into
  /// its own detail in place, rather than covering the shell with a pushed
  /// screen. This is the exact behaviour that went missing when Home was
  /// rewritten for Nexus.
  ///
  /// Drives it the way a person actually does — a tap, which is what
  /// `CollectionRow` needs to escalate past collapsed — rather than starting
  /// the tree already "open", which even the legacy row has never supported.
  /// A small stateful harness stands in for `Home`, which owns `openId` and
  /// the source that follows it; `Home` itself reaches a global singleton
  /// controller a unit test cannot substitute, so this proves the contract
  /// `Home` relies on instead of `Home`'s own wiring.
  testWidgets(
      'the wide layout grows the open row into its own detail instead of '
      'pushing a screen', (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<AppDetail?>.empty(),
    );

    int? openId;
    NexusCollectionSource? source;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: StatefulBuilder(
            builder: (context, setState) => NexusHomeLibrary(
              wide: true,
              state: controller.state,
              error: null,
              query: '',
              openId: openId,
              openSource: source,
              onSearch: (_) {},
              onImportTorrent: (_) async {},
              onCreateCollection: () {},
              // No addTearDown here: once this is handed to
              // `NexusHomeLibrary`, `CollectionDetail`'s own state is its
              // sole owner and disposes it when the row collapses or the
              // tree tears down — a second dispose call is the bug this
              // ownership rule exists to prevent.
              onOpen: (collection) => setState(() {
                openId = collection.id;
                source = NexusCollectionSource(
                  controller: controller,
                  collectionId: collection.id,
                );
              }),
              onCommand: (_) {},
            ),
          ),
        ),
      ),
    );

    expect(find.byType(CollectionOverview), findsNothing);
    await tester.tap(find.text('Episode archive'));
    await tester.pump();

    // No route was pushed — the detail is right here, grown out of the row.
    expect(find.byType(CollectionOverview), findsOneWidget);
    expect(find.byKey(const Key('collectionCommandrestart')), findsOneWidget);
  });

  testWidgets(
      'collection detail deletes through the same command bar and dialog '
      'the legacy screen uses', (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<AppDetail?>.empty(),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: NexusCollectionDetail(collection: 9, controller: controller),
      ),
    );

    await tester.tap(find.byKey(const Key('collectionCommanddelete')));
    await tester.pump();
    expect(find.text('Delete "Episode archive"?'), findsOneWidget);
    await tester.tap(find.byKey(const Key('deleteCollectionOnly')));
    await tester.pump();

    expect(repository.commands, hasLength(1));
    expect(repository.commands.single.kind, 'deleteCollection');
    expect(repository.commands.single.collection, 9);
    expect(repository.commands.single.deleteFiles, isFalse);
  });

  testWidgets('choosing "delete with files" carries the flag', (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<AppDetail?>.empty(),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: NexusCollectionDetail(collection: 9, controller: controller),
      ),
    );

    await tester.tap(find.byKey(const Key('collectionCommanddelete')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('deleteCollectionWithFiles')));
    await tester.pump();

    expect(repository.commands.single.deleteFiles, isTrue);
  });

  testWidgets(
      'restart resumes and pause pauses, both through the one setPaused '
      'command', (tester) async {
    final repository = _Repository();
    final controller = NexusAppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<AppDetail?>.empty(),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: NexusCollectionDetail(collection: 9, controller: controller),
      ),
    );

    await tester.tap(find.byKey(const Key('collectionCommandrestart')));
    await tester.pump();
    expect(repository.commands.last.kind, 'setPaused');
    expect(repository.commands.last.paused, isFalse);

    await tester.tap(find.byKey(const Key('collectionCommandpause')));
    await tester.pump();
    expect(repository.commands.last.kind, 'setPaused');
    expect(repository.commands.last.paused, isTrue);
  });
}

AppSnapshot _collectionState() => AppSnapshot(
      device: const AppDevice(
        name: 'Mina',
        handle: null,
        fingerprint: 'fingerprint',
        devices: 1,
      ),
      connectivity: 'LocalOnly',
      contacts: const [],
      collections: [
        AppCollection(
          id: 9,
          name: 'Episode archive',
          nature: 'Native',
          role: 'Owner',
          revision: BigInt.one,
          status: 'Available',
          members: Uint32List(0),
          entries: 0,
          totalBytes: BigInt.zero,
          onDiskBytes: BigInt.zero,
          uploadedBytes: BigInt.zero,
          transfer: null,
          pending: null,
        ),
      ],
      alerts: const [],
    );

AppSnapshot _state(String name) => AppSnapshot(
      device: AppDevice(
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

/// One torrent import, downloading. `Torrent` is what makes its files a
/// choice at all — see `NexusCollectionSource.supportsSelection`.
AppSnapshot _torrentState() => AppSnapshot(
      device: const AppDevice(
        name: 'Mina',
        handle: null,
        fingerprint: 'fingerprint',
        devices: 1,
      ),
      connectivity: 'LocalOnly',
      contacts: const [],
      collections: [
        AppCollection(
          id: 9,
          name: 'Big Buck Bunny',
          nature: 'Torrent',
          role: 'Owner',
          revision: BigInt.one,
          status: 'Downloading',
          members: Uint32List(0),
          entries: 2,
          totalBytes: BigInt.from(39),
          onDiskBytes: BigInt.zero,
          uploadedBytes: BigInt.zero,
          transfer: null,
          pending: null,
        ),
      ],
      alerts: const [],
    );

class _Repository implements NexusAppRepository {
  final states = StreamController<AppSnapshot>.broadcast();
  final details = StreamController<AppDetail?>.broadcast();
  final active = <bool>[];
  final detailCollections = <int?>[];
  final commands = <NexusCommand>[];
  var starts = 0;
  var stops = 0;

  @override
  Future<AppAccepted> send(NexusCommand command) async {
    commands.add(command);
    return AppAccepted(id: BigInt.zero, collection: null, queued: false);
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
  Stream<AppDetail?> watchDetail(int? collection) {
    detailCollections.add(collection);
    return details.stream;
  }

  @override
  Stream<AppSnapshot> watchStates() => states.stream;
}
