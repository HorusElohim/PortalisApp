import 'dart:typed_data';
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/app/app_controllers.dart';
import 'package:portalis/app/onboarding_controller.dart';
import 'package:portalis/features/collections/domain/collection.dart';
import 'package:portalis/nexus/domain/app_state.dart';
import 'package:portalis/features/settings/domain/engine.dart';
import 'package:portalis/main.dart';
import 'package:portalis/shell/navigation.dart';

export 'dart:async' show unawaited;
export 'package:flutter/material.dart';
export 'package:flutter_test/flutter_test.dart';
export 'package:portalis/app/app_controllers.dart';
export 'package:portalis/design/design.dart';
export 'package:portalis/features/collections/domain/collection.dart';
export 'package:portalis/features/collections/domain/paste.dart';
export 'package:portalis/features/collections/presentation/detail.dart';
export 'package:portalis/features/collections/presentation/add_sources.dart';
export 'package:portalis/features/collections/domain/draft_names.dart';
export 'package:portalis/features/media/domain/item.dart';
export 'package:portalis/features/media/presentation/viewer_screen.dart';
export 'package:portalis/nexus/domain/app_state.dart';
export 'package:portalis/features/collections/presentation/route.dart';
export 'package:portalis/features/settings/domain/engine.dart';
export 'package:portalis/main.dart';
export 'package:portalis/features/people/presentation/screen.dart';
export 'package:portalis/shell/root_shell.dart';
export 'package:portalis/features/settings/presentation/screen.dart';
export 'package:portalis/features/identity/presentation/user_screen.dart';
export 'package:portalis/shell/navigation.dart';
export 'package:portalis/design/theme.dart';

const phoneSize = Size(390, 844);
const desktopSize = Size(1280, 800);

/// A collection as the engine would report it.
///
/// Builds the backend's own values and wraps them, because that is all
/// [Collection] is now — tests that construct a shape the engine cannot
/// produce are tests of something that cannot happen.
Collection buildCollection({
  int id = 1,
  String name = 'Iceland trip',
  String nature = 'Native',
  String status = 'Available',
  int downBytesPerSecond = 0,
  int upBytesPerSecond = 0,
  int livePeers = 0,
  List<AppPeer> torrentPeers = const [],
  List<AppEntry> entries = const [],
  List<AppContact> contacts = const [],
  List<AppMember>? members,
  int totalBytes = 0,
  int downloadedBytes = 0,
  int? etaSecs,
}) {
  // A partial collection has a transfer even when no rate was named: the
  // engine reports one for anything it is carrying, and its progress is where
  // a fraction comes from.
  final moving = downBytesPerSecond > 0 ||
      upBytesPerSecond > 0 ||
      livePeers > 0 ||
      downloadedBytes > 0 ||
      status == 'Downloading';
  return Collection(
    buildNexusCollection(
      id: id,
      name: name,
      nature: nature,
      status: status,
      members: members,
      entries: entries.length,
      totalBytes: totalBytes,
      onDiskBytes: downloadedBytes,
      transfer: moving
          ? AppTransfer(
              progress: totalBytes == 0 ? 0 : downloadedBytes / totalBytes,
              sourceReading: false,
              downBytesPerSecond: downBytesPerSecond,
              upBytesPerSecond: upBytesPerSecond,
              peers: livePeers,
              etaSecs: etaSecs,
            )
          : null,
    ),
    detail: entries.isEmpty && torrentPeers.isEmpty
        ? null
        : AppDetail(
            id: id,
            entries: entries,
            pieces: Uint8List(0),
            peers: torrentPeers,
          ),
    contacts: contacts,
  );
}

/// One swarm connection, as the engine reports it.
///
/// Defaults to a connected-but-idle peer, which is the common real state and
/// the one a test is least likely to mean to assert about by accident.
AppPeer buildPeer({
  String address = '203.0.113.5:6881',
  String? client,
  int downBytes = 0,
  int upBytes = 0,
  int downBytesPerSecond = 0,
  int upBytesPerSecond = 0,
}) =>
    AppPeer(
      address: address,
      client: client,
      downBytes: BigInt.from(downBytes),
      upBytes: BigInt.from(upBytes),
      downBytesPerSecond: downBytesPerSecond,
      upBytesPerSecond: upBytesPerSecond,
    );

/// One file of a collection, as the engine reports it.
AppEntry buildEntry({
  int id = 1,
  String label = 'clip.mp4',
  int bytes = 0,
  int downloadedBytes = 0,
  bool selected = true,
  bool available = true,
  String? path,
}) =>
    AppEntry(
      id: id,
      label: label,
      bytes: BigInt.from(bytes),
      selected: selected,
      available: available,
      downloadedBytes: BigInt.from(downloadedBytes),
      path: path,
    );

AppCollection buildNexusCollection({
  int id = 1,
  String name = 'Iceland trip',
  String nature = 'Native',
  String role = 'Owner',
  String status = 'Available',
  int revision = 1,
  int entries = 0,
  int totalBytes = 0,
  int onDiskBytes = 0,
  int uploadedBytes = 0,
  BigInt? startedAt,
  BigInt? completedAt,
  List<AppMember>? members,
  AppTransfer? transfer,
}) {
  final lifecycle = _testLifecycle(status);
  final typedNature = nature == 'Torrent'
      ? AppCollectionNature.torrent
      : AppCollectionNature.native;
  final typedRole = role == 'Member' || role == 'Receiver'
      ? AppCollectionRole.member
      : AppCollectionRole.owner;
  final complete = lifecycle == AppCollectionLifecycle.available ||
      lifecycle == AppCollectionLifecycle.seeding;
  final preparing = lifecycle == AppCollectionLifecycle.resolvingMetadata ||
      lifecycle == AppCollectionLifecycle.retryingMetadata ||
      lifecycle == AppCollectionLifecycle.waitingForSender;
  final moving = lifecycle == AppCollectionLifecycle.downloading ||
      (transfer?.downBytesPerSecond ?? 0) > 0 ||
      (transfer?.upBytesPerSecond ?? 0) > 0;
  final progress = transfer?.progress ??
      (complete
          ? 1
          : totalBytes == 0
              ? 0
              : (onDiskBytes / totalBytes).clamp(0.0, 1.0));
  return AppCollection(
    id: id,
    name: name,
    nature: typedNature,
    role: typedRole,
    revision: BigInt.from(revision),
    lifecycle: lifecycle,
    statusLabel: status,
    capabilities: AppCollectionCapabilities(
      canAddMedia: typedNature == AppCollectionNature.native &&
          lifecycle == AppCollectionLifecycle.draft,
      canSelect: typedNature == AppCollectionNature.torrent &&
          lifecycle == AppCollectionLifecycle.metadataReady,
      canShare: lifecycle != AppCollectionLifecycle.draft &&
          (entries > 0 || revision > 0),
      canPause: const {
        AppCollectionLifecycle.available,
        AppCollectionLifecycle.seeding,
        AppCollectionLifecycle.downloadRequested,
        AppCollectionLifecycle.downloading,
        AppCollectionLifecycle.updating,
      }.contains(lifecycle),
      canResume: lifecycle == AppCollectionLifecycle.paused,
      canDelete: true,
      canDeleteFiles: onDiskBytes > 0,
    ),
    facts: AppCollectionFacts(
      complete: complete,
      sharing: complete && entries > 0,
      moving: moving,
      preparing: preparing,
      progress: progress,
    ),
    members: members ?? const [],
    entries: entries,
    totalBytes: BigInt.from(totalBytes),
    onDiskBytes: BigInt.from(onDiskBytes),
    uploadedBytes: BigInt.from(uploadedBytes),
    startedAt: startedAt,
    completedAt: completedAt,
    transfer: transfer,
    pending: null,
  );
}

AppCollectionLifecycle _testLifecycle(String status) => switch (status) {
      'Available' => AppCollectionLifecycle.available,
      'Seeding' => AppCollectionLifecycle.seeding,
      'Paused' => AppCollectionLifecycle.paused,
      'Draft' => AppCollectionLifecycle.draft,
      'ResolvingMetadata' => AppCollectionLifecycle.resolvingMetadata,
      'WaitingForSender' => AppCollectionLifecycle.waitingForSender,
      'MetadataReady' => AppCollectionLifecycle.metadataReady,
      'DownloadRequested' => AppCollectionLifecycle.downloadRequested,
      'RetryingMetadata' => AppCollectionLifecycle.retryingMetadata,
      'Downloading' => AppCollectionLifecycle.downloading,
      'Updating' => AppCollectionLifecycle.updating,
      'WaitingForOwner' => AppCollectionLifecycle.waitingForOwner,
      'AccessRemoved' => AppCollectionLifecycle.accessRemoved,
      'NeedsNewerVersion' => AppCollectionLifecycle.needsNewerVersion,
      'CannotVerify' => AppCollectionLifecycle.cannotVerify,
      'ConflictingHistory' => AppCollectionLifecycle.conflictingHistory,
      _ =>
        throw ArgumentError.value(status, 'status', 'unknown test lifecycle'),
    };

AppSnapshot buildNexusState(List<AppCollection> collections) => AppSnapshot(
      device: const AppDevice(
        name: 'Portalis',
        handle: null,
        fingerprint: 'test-fingerprint',
        devices: 1,
      ),
      connectivity: 'LocalOnly',
      contacts: const [],
      collections: collections,
      alerts: const [],
    );

AppAppRun buildAppRun({
  int runId = 1,
  int startedAt = 0,
  int? endedAt,
  String endReason = 'current',
  int networkDownBytes = 0,
  int networkUpBytes = 0,
}) =>
    AppAppRun(
      runId: BigInt.from(runId),
      startedAt: BigInt.from(startedAt),
      endedAt: endedAt == null ? null : BigInt.from(endedAt),
      engineRunningNs: BigInt.zero,
      foregroundNs: BigInt.zero,
      networkDownBytes: BigInt.from(networkDownBytes),
      networkUpBytes: BigInt.from(networkUpBytes),
      completedDownloads: BigInt.zero,
      peakDownBytesPerSecond: 0,
      peakUpBytesPerSecond: 0,
      endReason: endReason,
    );

AppUserSummary buildUserSummary({
  AppAppRun? currentRun,
  int trackedSince = 0,
  int runsStarted = 1,
  int lifetimeNetworkDownBytes = 0,
  int lifetimeNetworkUpBytes = 0,
  int collectionsOwned = 0,
  int collectionsReceived = 0,
  List<AppAppRun> recentRuns = const [],
}) =>
    AppUserSummary(
      device: const AppDevice(
        name: 'Portalis',
        handle: null,
        fingerprint: 'test-fingerprint',
        devices: 1,
      ),
      trackedSince: BigInt.from(trackedSince),
      currentRun: currentRun ?? buildAppRun(),
      runsStarted: BigInt.from(runsStarted),
      runsCompletedCleanly: BigInt.zero,
      runsInterrupted: BigInt.zero,
      lifetimeEngineRunningNs: BigInt.zero,
      lifetimeForegroundNs: BigInt.zero,
      lifetimeNetworkDownBytes: BigInt.from(lifetimeNetworkDownBytes),
      lifetimeNetworkUpBytes: BigInt.from(lifetimeNetworkUpBytes),
      lifetimeCompletedDownloads: BigInt.zero,
      lifetimePeakDownBytesPerSecond: 0,
      lifetimePeakUpBytesPerSecond: 0,
      lastActivityAt: BigInt.zero,
      lastCleanShutdownAt: BigInt.zero,
      collectionsOwned: collectionsOwned,
      collectionsReceived: collectionsReceived,
      entriesTotal: 0,
      catalogBytes: BigInt.zero,
      heldBytes: BigInt.zero,
      verifiedContacts: 0,
      unverifiedContacts: 0,
      connectivity: 'LocalOnly',
      recentRuns: recentRuns,
    );

EngineSettings buildEngineSettings() => const EngineSettings(
      listenPortStart: 6881,
      listenPortEnd: 6999,
      enableUpnpPortForwarding: true,
      disableDht: false,
      disableDhtPersistence: false,
      persistSession: true,
      fastresume: true,
      trackers: [],
    );

Future<void> pumpApp(
  WidgetTester tester, {
  Size size = phoneSize,
  List<AppCollection> engineCollections = const [],
  String? error,
  AppUserSummary? userSummary,
  List<AppCollectionPeer>? peers,
}) async {
  await tester.binding.setSurfaceSize(size);
  addTearDown(() => tester.binding.setSurfaceSize(null));
  // Every shell test exercises RootShell, not the first-run introduction —
  // see OnboardingScreen's own tests for that.
  OnboardingController.instance.markCompletedForTesting();
  await tester.pumpWidget(const MyApp());
  await tester.pump();
  AppControllers.engine.debugSeed(
    buildNexusState(engineCollections),
    details: const Stream<AppDetail?>.empty(),
    error: error,
    userSummary: userSummary,
    peers: peers,
  );
  await tester.pump();
}

Future<void> pumpTransition(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 300));
}

void resetTestState() {
  AppControllers.engine.debugSeed(null);
  AppNavigation.tab.value = AppNavigation.homeTab;
  AppNavigation.depth.value = 0;
}
