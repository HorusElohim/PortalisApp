import 'dart:typed_data';
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/nexus/application/app_controller.dart';
import 'package:portalis/nexus/data/app_repository.dart';
import 'package:portalis/nexus/domain/app_state.dart';
import 'package:portalis/features/collections/presentation/route.dart';
import 'package:portalis/features/collections/presentation/home_library.dart';

void main() {
  test('owns one state subscription and forwards lifecycle changes', () async {
    final repository = _Repository();
    final controller = AppController(repository: repository);
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
    final controller = AppController(repository: repository);
    final detail = AppDetail(
      id: 9,
      entries: [
        AppEntry(
          id: 2,
          label: 'episode.mp4',
          bytes: BigInt.from(34),
          selected: false,
          available: false,
          downloadedBytes: BigInt.zero,
        ),
      ],
      pieces: Uint8List(0),
      peers: const [],
    );

    final seen = controller.watchDetail(9).first;
    repository.details.add(detail);

    expect((await seen)?.entries.single.selected, isFalse);
    expect(repository.detailCollections, [9]);
  });

  /// Before receiving starts, checkboxes are a local editing decision. The
  /// only action that may cross the command boundary is the explicit Download
  /// button, which sends the final whole selection.
  testWidgets('a torrent draft stages deselection until Download is clicked',
      (tester) async {
    final repository = _Repository();
    final controller = AppController(repository: repository);
    await controller.start();
    await tester.binding.setSurfaceSize(const Size(420, 1400));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: CollectionRoute(collection: 9, controller: controller),
      ),
    );
    repository.states.add(_torrentState(status: 'Draft'));
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
            downloadedBytes: BigInt.zero,
          ),
          AppEntry(
            id: 2,
            label: 'feature.mp4',
            bytes: BigInt.from(34),
            selected: true,
            available: false,
            downloadedBytes: BigInt.zero,
          ),
        ],
        pieces: Uint8List(0),
        peers: const [],
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    expect(find.text('trailer.mp4'), findsOneWidget);
    expect(find.text('feature.mp4'), findsOneWidget);
    await tester.tap(find.byKey(const Key('mediaWanted:1')));
    await tester.pump();

    expect(find.text('skipped'), findsOneWidget);
    expect(repository.commands, isEmpty);

    await tester.tap(find.byKey(const Key('editFinish')));
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
    final controller = AppController(repository: repository);
    await controller.start();
    await tester.binding.setSurfaceSize(const Size(420, 1400));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: CollectionRoute(collection: 9, controller: controller),
      ),
    );
    repository.states.add(_torrentState(status: 'Draft'));
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
            downloadedBytes: BigInt.zero,
          ),
        ],
        pieces: Uint8List(0),
        peers: const [],
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    await tester.tap(find.byKey(const Key('mediaWanted:1')));
    await tester.pump();

    expect(repository.commands, isEmpty);
  });

  /// A draft opens ready to be changed, because it exists only because
  /// somebody is halfway through assembling it. The name is a suggestion
  /// already in the field rather than a question asked of an empty screen.
  testWidgets('a draft collection opens in edit mode and shares on confirm',
      (tester) async {
    final repository = _Repository();
    final controller = AppController(repository: repository);
    await controller.start();
    await tester.binding.setSurfaceSize(const Size(420, 1400));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: CollectionRoute(collection: 9, controller: controller),
      ),
    );
    repository.states.add(_torrentState(status: 'Draft', nature: 'Native'));
    await tester.pump();
    repository.details.add(
      AppDetail(
        id: 9,
        entries: [
          AppEntry(
            id: 1,
            label: 'clip.mp4',
            bytes: BigInt.from(5),
            selected: true,
            available: false,
            downloadedBytes: BigInt.zero,
          ),
        ],
        pieces: Uint8List(0),
        peers: const [],
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    // Open for changes without anybody asking, and the finishing move says
    // what it does: this has never been shared.
    expect(find.byKey(const Key('editCollectionName')), findsOneWidget);
    expect(find.text('Share this collection'), findsOneWidget);
    expect(find.text('Nothing has left this device yet.'), findsOneWidget);

    await tester.enterText(
      find.byKey(const Key('editCollectionName')),
      'Lisbon trip',
    );
    await tester.pump();
    await tester.tap(find.byKey(const Key('editFinish')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    // The rename lands before the publish: sharing something under a name
    // the person just replaced would share the wrong name.
    expect(repository.commands.map((command) => command.kind).toList(),
        ['renameCollection', 'publishDraft']);
    expect(repository.commands.first.name, 'Lisbon trip');
    expect(tester.takeException(), isNull);
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
          body: HomeLibrary(
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
            onCreateCollection: () => created = true,
            onOpen: (value) => opened = value,
            onCommand: (_) {},
          ),
        ),
      ),
    );

    expect(find.text('Episode archive'), findsOneWidget);
    // The engine's own word, shown as it is — there is no second vocabulary
    // between the two any more, and so nothing that can disagree.
    expect(find.text('PREPARING'), findsOneWidget);
    await tester.tap(find.text('Episode archive'));
    expect(opened, same(collection));
    await tester.tap(find.byKey(const Key('shareCollectionAction')));
    expect(created, isTrue);
  });

  /// Pasting a torrent link is asking for what is in it, not making
  /// something of your own — the name is the torrent's, and offering a field
  /// for it invites a person to answer a question nobody asked. Re-open it
  /// afterwards and it is theirs to rename like anything else.
  testWidgets('a torrent arriving for the first time is not named',
      (tester) async {
    final repository = _Repository();
    final controller = AppController(repository: repository);
    await controller.start();
    await tester.binding.setSurfaceSize(const Size(420, 1400));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: CollectionRoute(collection: 9, controller: controller),
      ),
    );
    repository.states.add(_torrentState(status: 'Draft'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    // Open for choosing files, with nothing to name and nothing to add:
    // a torrent's contents are its identity. The finishing action receives
    // the remote collection; it must never claim to publish local sources.
    expect(find.byKey(const Key('editCollectionName')), findsNothing);
    expect(find.byKey(const Key('editAddSources')), findsNothing);
    expect(find.text('Download selected files'), findsOneWidget);
    expect(find.text('Share this collection'), findsNothing);
    // But it still says what arrived. Not editable is not invisible.
    expect(find.text('Big Buck Bunny'), findsWidgets);

    // Once it is a collection rather than an arrival, the name is editable.
    repository.states.add(_torrentState());
    await tester.pump();
    await tester.tap(find.byKey(const Key('collectionCommandedit')));
    await tester.pump();
    expect(find.byKey(const Key('editCollectionName')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  /// An already-shared collection is only being edited, so its finishing
  /// move is Done — offering to share what is already shared would promise
  /// something that already happened.
  testWidgets('a shared collection edits without offering to share again',
      (tester) async {
    final repository = _Repository();
    final controller = AppController(repository: repository);
    await controller.start();
    await tester.binding.setSurfaceSize(const Size(420, 1400));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: CollectionRoute(collection: 9, controller: controller),
      ),
    );
    repository.states.add(_torrentState());
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    // Closed until asked: nothing is editable just because it is open.
    expect(find.byKey(const Key('editCollectionName')), findsNothing);

    await tester.tap(find.byKey(const Key('collectionCommandedit')));
    await tester.pump();
    expect(find.byKey(const Key('editCollectionName')), findsOneWidget);
    expect(find.text('Done'), findsOneWidget);
    expect(find.text('Share this collection'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  /// The bug this exists to prevent: the shell reported "1 ACTIVE TRANSFER"
  /// above a Home showing no collections, because the chrome counted from the
  /// legacy collections controller while the list came from Nexus. Status
  /// chrome that contradicts the list beside it makes a person distrust what
  /// they can see, so activity has exactly one derivation.
  test('activity is idle when Nexus holds nothing, whatever else is running',
      () {
    final controller = AppController(repository: _Repository());

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
    final controller = AppController(repository: _Repository());
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
              sourceReading: false,
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
    expect(activity.downBytesPerSecond, 125000);
    expect(activity.upBytesPerSecond, 250000);
  });

  /// Opening means one thing on every layout. The wide window used to grow
  /// the row in place, which left the collection's own controls — edit among
  /// them — reachable on one layout and not the other.
  testWidgets('the wide layout hands an opened collection to its owner',
      (tester) async {
    final repository = _Repository();
    final controller = AppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<AppDetail?>.empty(),
    );
    await tester.binding.setSurfaceSize(const Size(1280, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    int? opened;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: HomeLibrary(
            wide: true,
            state: controller.state,
            error: null,
            onCreateCollection: () {},
            onOpen: (collection) => opened = collection.id,
            onCommand: (_) {},
          ),
        ),
      ),
    );
    await tester.pump();

    await tester.tap(find.text('Episode archive').first);
    await tester.pump();

    // The id went to whoever owns navigation; nothing grew in place.
    expect(opened, isNotNull);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
      'collection detail deletes through the same command bar and dialog '
      'the legacy screen uses', (tester) async {
    final repository = _Repository();
    final controller = AppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<AppDetail?>.empty(),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: CollectionRoute(collection: 9, controller: controller),
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
    final controller = AppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<AppDetail?>.empty(),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: CollectionRoute(collection: 9, controller: controller),
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
    final controller = AppController(repository: repository);
    controller.debugSeed(
      _collectionState(),
      details: const Stream<AppDetail?>.empty(),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: CollectionRoute(collection: 9, controller: controller),
      ),
    );

    // Running, so the pair offers Pause and nothing else. Start appears only
    // once it is stopped — one button, never both.
    expect(find.byKey(const Key('collectionCommandrestart')), findsNothing);
    await tester.tap(find.byKey(const Key('collectionCommandpause')));
    await tester.pump();
    expect(repository.commands.last.kind, 'setPaused');
    expect(repository.commands.last.paused, isTrue);

    // And once paused, the other half — the same command, the other way.
    controller.debugSeed(
      _collectionState(status: 'Paused'),
      details: const Stream<AppDetail?>.empty(),
    );
    await tester.pump();
    expect(find.byKey(const Key('collectionCommandpause')), findsNothing);
    await tester.tap(find.byKey(const Key('collectionCommandrestart')));
    await tester.pump();
    expect(repository.commands.last.kind, 'setPaused');
    expect(repository.commands.last.paused, isFalse);
  });
}

AppSnapshot _collectionState({String status = 'Available'}) => AppSnapshot(
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
          status: status,
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
/// choice at all — see `EngineCollectionSource.supportsSelection`.
AppSnapshot _torrentState({
  String status = 'Downloading',
  String nature = 'Torrent',
}) =>
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
        AppCollection(
          id: 9,
          name: 'Big Buck Bunny',
          nature: nature,
          role: 'Owner',
          revision: BigInt.one,
          status: status,
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

class _Repository implements AppRepository {
  final states = StreamController<AppSnapshot>.broadcast();
  final details = StreamController<AppDetail?>.broadcast();
  final history = StreamController<Uint8List>.broadcast();
  final active = <bool>[];
  final detailCollections = <int?>[];
  final commands = <EngineCommand>[];
  var starts = 0;
  var stops = 0;

  @override
  Future<List<AppCollectionPeer>> peers() async => const [];

  @override
  Future<List<AppPeoplePeer>> peoplePeers() async => const [];

  @override
  Future<List<AppPeerHistory>> peerHistory(int collection) async => const [];

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
    return AppAccepted(id: BigInt.zero, collection: null, queued: false);
  }

  @override
  Future<String?> shareUri(int collection) async => null;

  @override
  Future<void> setActive(bool value) async => active.add(value);

  @override
  Future<void> start() async => starts++;

  @override
  Future<void> stop() async {
    stops++;
    await states.close();
    await details.close();
    await history.close();
  }

  @override
  Stream<AppDetail?> watchDetail(int? collection) {
    detailCollections.add(collection);
    return details.stream;
  }

  @override
  Stream<Uint8List> watchHistory(int collection) => history.stream;

  @override
  Stream<AppSnapshot> watchStates() => states.stream;
}
