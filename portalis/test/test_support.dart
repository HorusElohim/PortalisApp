import 'dart:typed_data';
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/app/app_controllers.dart';
import 'package:portalis/features/collections/domain/collection.dart';
import 'package:portalis/features/collections/domain/collection_import.dart';
import 'package:portalis/features/media/domain/media_item.dart';
import 'package:portalis/nexus/domain/app_state.dart';
import 'package:portalis/features/settings/domain/engine_settings.dart';
import 'package:portalis/main.dart';
import 'package:portalis/shell/navigation.dart';

export 'package:flutter/material.dart';
export 'package:flutter_test/flutter_test.dart';
export 'package:portalis/app/app_controllers.dart';
export 'package:portalis/design/design.dart';
export 'package:portalis/features/collections/domain/collection.dart';
export 'package:portalis/features/collections/domain/collection_import.dart';
export 'package:portalis/features/collections/domain/paste.dart';
export 'package:portalis/features/collections/presentation/collection_detail.dart';
export 'package:portalis/features/collections/presentation/collection_share.dart';
export 'package:portalis/features/collections/presentation/command_bar.dart';
export 'package:portalis/features/media/domain/media_item.dart';
export 'package:portalis/features/media/presentation/media_viewer_screen.dart';
export 'package:portalis/nexus/domain/app_state.dart';
export 'package:portalis/features/collections/presentation/collection_route.dart';
export 'package:portalis/features/settings/domain/engine_settings.dart';
export 'package:portalis/main.dart';
export 'package:portalis/features/people/presentation/people_screen.dart';
export 'package:portalis/shell/root_shell.dart';
export 'package:portalis/features/settings/presentation/settings_screen.dart';
export 'package:portalis/features/identity/presentation/user_screen.dart';
export 'package:portalis/shell/navigation.dart';
export 'package:portalis/design/theme.dart';

const phoneSize = Size(390, 844);
const desktopSize = Size(1280, 800);

Collection buildCollection({
  String id = 'c1',
  String name = 'Iceland trip',
  CollectionKind kind = CollectionKind.shared,
  double downloadMbps = 0,
  double uploadMbps = 0,
  String state = 'seeding',
  int livePeers = 0,
  List<String> torrentPeers = const [],
  int pendingMedia = 0,
  List<Collaborator> collaborators = const [],
  List<MediaItem> media = const [],
  int totalBytes = 0,
  int downloadedBytes = 0,
  int? etaSecs,
  CollectionImport? ingestion,
}) =>
    Collection(
      id: id,
      name: name,
      kind: kind,
      collaborators: collaborators,
      media: media,
      progress: totalBytes == 0 ? 0 : downloadedBytes / totalBytes,
      totalBytes: totalBytes,
      downloadedBytes: downloadedBytes,
      downloadMbps: downloadMbps,
      uploadMbps: uploadMbps,
      livePeers: livePeers,
      torrentPeers: torrentPeers,
      pendingMedia: pendingMedia,
      etaSecs: etaSecs,
      state: state,
      ingestion: ingestion,
    );

AppCollection buildNexusCollection({
  int id = 1,
  String name = 'Iceland trip',
  String nature = 'Native',
  String role = 'Owner',
  String status = 'Available',
  int entries = 0,
  int totalBytes = 0,
  Uint32List? members,
  AppTransfer? transfer,
}) =>
    AppCollection(
      id: id,
      name: name,
      nature: nature,
      role: role,
      revision: BigInt.one,
      status: status,
      members: members ?? Uint32List(0),
      entries: entries,
      totalBytes: BigInt.from(totalBytes),
      onDiskBytes: BigInt.zero,
      uploadedBytes: BigInt.zero,
      transfer: transfer,
      pending: null,
    );

AppSnapshot buildNexusState(List<AppCollection> collections) =>
    AppSnapshot(
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
  List<AppCollection> nexusCollections = const [],
  String? error,
}) async {
  await tester.binding.setSurfaceSize(size);
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(const MyApp());
  await tester.pump();
  AppControllers.nexusApp.debugSeed(
    buildNexusState(nexusCollections),
    details: const Stream<AppDetail?>.empty(),
    error: error,
  );
  await tester.pump();
}

Future<void> pumpTransition(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 300));
}


void resetTestState() {
  AppControllers.nexusApp.debugSeed(null);
  AppNavigation.tab.value = AppNavigation.homeTab;
  AppNavigation.depth.value = 0;
}
