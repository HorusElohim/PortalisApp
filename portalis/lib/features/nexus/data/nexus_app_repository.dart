import 'dart:typed_data';

import '../../../bridge_generated/portalis_api.dart' as bridge;
import '../domain/nexus_app_state.dart';

/// The complete native contract the application consumes during the Nexus
/// migration. Generated bridge imports belong only in this adapter.
abstract interface class NexusAppRepository {
  Future<void> start();
  Future<void> stop();
  Future<void> setActive(bool active);
  Stream<NexusAppState> watchStates();
  Stream<NexusDetail?> watchDetail(int? collection);
  Future<NexusAccepted> send(NexusCommand command);
}

class FrbNexusAppRepository implements NexusAppRepository {
  const FrbNexusAppRepository();

  @override
  Future<void> start() => bridge.start();

  @override
  Future<void> stop() => bridge.stop();

  @override
  Future<void> setActive(bool active) => bridge.setActive(active: active);

  @override
  Stream<NexusAppState> watchStates() =>
      bridge.watchStates().map(_stateFromBridge);

  @override
  Stream<NexusDetail?> watchDetail(int? collection) =>
      bridge.watchDetail(collection: collection).map(
            (detail) => detail == null ? null : _detailFromBridge(detail),
          );

  @override
  Future<NexusAccepted> send(NexusCommand command) async {
    final accepted = await bridge.send(command: _commandToBridge(command));
    return NexusAccepted(
      id: accepted.id,
      collection: accepted.collection,
      queued: accepted.queued,
    );
  }

  static NexusAppState _stateFromBridge(bridge.AppSnapshot state) =>
      NexusAppState(
        device: NexusDevice(
          name: state.device.name,
          handle: state.device.handle,
          fingerprint: state.device.fingerprint,
          devices: state.device.devices,
        ),
        connectivity: state.connectivity,
        contacts:
            state.contacts.map(_contactFromBridge).toList(growable: false),
        collections: state.collections
            .map(_collectionFromBridge)
            .toList(growable: false),
        alerts: List.unmodifiable(state.alerts),
      );

  static NexusContact _contactFromBridge(bridge.AppContact contact) =>
      NexusContact(
        id: contact.id,
        displayName: contact.displayName,
        handle: contact.handle,
        fingerprint: contact.fingerprint,
        verified: contact.verified,
        friendship: contact.friendship,
        reachable: contact.reachable,
      );

  static NexusCollection _collectionFromBridge(
          bridge.AppCollection collection) =>
      NexusCollection(
        id: collection.id,
        name: collection.name,
        nature: collection.nature,
        role: collection.role,
        revision: collection.revision,
        status: collection.status,
        members: List.unmodifiable(collection.members),
        entries: collection.entries,
        totalBytes: collection.totalBytes,
        onDiskBytes: collection.onDiskBytes,
        transfer: collection.transfer == null
            ? null
            : NexusTransfer(
                progress: collection.transfer!.progress,
                downBytesPerSecond: collection.transfer!.downBytesPerSecond,
                upBytesPerSecond: collection.transfer!.upBytesPerSecond,
                peers: collection.transfer!.peers,
                etaSecs: collection.transfer!.etaSecs,
              ),
        pending: collection.pending == null
            ? null
            : NexusPending(
                command: collection.pending!.command,
                queued: collection.pending!.queued,
              ),
      );

  static NexusDetail _detailFromBridge(bridge.AppDetail detail) => NexusDetail(
        id: detail.id,
        entries: detail.entries
            .map(
              (entry) => NexusEntry(
                id: entry.id,
                label: entry.label,
                bytes: entry.bytes,
                selected: entry.selected,
                available: entry.available,
                path: entry.path,
              ),
            )
            .toList(growable: false),
        pieces: List.unmodifiable(detail.pieces),
        samples: List.unmodifiable(detail.samples),
        peers: List.unmodifiable(detail.peers),
      );

  static bridge.AppCommand _commandToBridge(NexusCommand command) =>
      bridge.AppCommand(
        kind: command.kind,
        name: command.name,
        files: command.files
            .map(
              (file) => bridge.AppSourceFile(
                name: file.name,
                path: file.path,
                bytes: file.bytes,
              ),
            )
            .toList(growable: false),
        collection: command.collection,
        label: command.label,
        deleteFiles: command.deleteFiles,
        paused: command.paused,
        entry: command.entry,
        source: command.source,
        entries: Uint32List.fromList(command.entries),
        contact: command.contact,
        handle: command.handle,
        accept: command.accept,
        device: command.device,
        active: command.active,
      );
}
