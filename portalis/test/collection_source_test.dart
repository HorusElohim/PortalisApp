import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/nexus/application/app_controller.dart';
import 'package:portalis/nexus/data/app_repository.dart';
import 'package:portalis/nexus/data/collection_source.dart';
import 'package:portalis/nexus/domain/app_state.dart';

void main() {
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
  Future<AppUserSummary> userSummary() async => _fakeUserSummary();

  @override
  Future<void> clearUserActivity() async {}

  @override
  Future<void> renameDevice(String nickname) async {}

  @override
  Future<AppAccepted> send(EngineCommand command) async {
    commands.add(command);
    return AppAccepted(id: BigInt.one, collection: null, queued: true);
  }
}

AppUserSummary _fakeUserSummary() => AppUserSummary(
      device: const AppDevice(
        name: 'Portalis',
        handle: null,
        fingerprint: 'test-fingerprint',
        devices: 1,
      ),
      trackedSince: BigInt.zero,
      currentRun: AppAppRun(
        runId: BigInt.one,
        startedAt: BigInt.zero,
        engineRunningNs: BigInt.zero,
        foregroundNs: BigInt.zero,
        networkDownBytes: BigInt.zero,
        networkUpBytes: BigInt.zero,
        completedDownloads: BigInt.zero,
        peakDownBytesPerSecond: 0,
        peakUpBytesPerSecond: 0,
        endReason: 'current',
      ),
      runsStarted: BigInt.one,
      runsCompletedCleanly: BigInt.zero,
      runsInterrupted: BigInt.zero,
      lifetimeEngineRunningNs: BigInt.zero,
      lifetimeForegroundNs: BigInt.zero,
      lifetimeNetworkDownBytes: BigInt.zero,
      lifetimeNetworkUpBytes: BigInt.zero,
      lifetimeCompletedDownloads: BigInt.zero,
      lifetimePeakDownBytesPerSecond: 0,
      lifetimePeakUpBytesPerSecond: 0,
      lastActivityAt: BigInt.zero,
      lastCleanShutdownAt: BigInt.zero,
      collectionsOwned: 0,
      collectionsReceived: 0,
      entriesTotal: 0,
      catalogBytes: BigInt.zero,
      heldBytes: BigInt.zero,
      verifiedContacts: 0,
      unverifiedContacts: 0,
      connectivity: 'LocalOnly',
      recentRuns: const [],
    );
