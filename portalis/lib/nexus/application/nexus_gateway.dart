import 'dart:typed_data';

import '../bridge/portalis_api.dart' as bridge;
import '../domain/app_state.dart';

/// The one fakeable contract between Flutter application code and Nexus.
///
/// An interface rather than the bridge functions themselves, because that is
/// what lets a widget test substitute Nexus without faking FFI. The projection
/// types it returns are generated bridge types: mirroring them by hand could
/// lose a field in silence.
abstract interface class NexusGateway {
  Future<void> start();
  Future<void> stop();
  Future<void> setActive(bool active);
  Stream<AppSnapshot> watchStates();
  Stream<AppDetail?> watchDetail(int? collection);

  /// One collection's readings, as they are recorded.
  ///
  /// Arrives as the rows a subscriber has not seen yet, not as the whole ring
  /// — the history only grows at the end, and re-sending all of it to append
  /// one row was thirty kilobytes a second for a screen already showing it.
  /// Whoever subscribes accumulates.
  Stream<Uint8List> watchHistory(int collection);
  Future<AppAccepted> send(AppCommand command);
}

/// Builds the generated command DTO with the empty fields the bridge requires.
///
/// The returned value is the bridge type itself; this factory removes call-site
/// boilerplate without introducing a second command model.
AppCommand appCommand({
  required String kind,
  String? name,
  List<AppSourceFile> files = const [],
  int? collection,
  String? label,
  bool? deleteFiles,
  bool? paused,
  int? entry,
  String? source,
  List<int> entries = const [],
  int? contact,
  String? handle,
  bool? accept,
  int? device,
  bool? active,
}) =>
    AppCommand(
      kind: kind,
      name: name,
      files: files,
      collection: collection,
      label: label,
      deleteFiles: deleteFiles,
      paused: paused,
      entry: entry,
      source: source,
      entries: Uint32List.fromList(entries),
      contact: contact,
      handle: handle,
      accept: accept,
      device: device,
      active: active,
    );

class FrbNexusGateway implements NexusGateway {
  const FrbNexusGateway();

  @override
  Future<void> start() => bridge.start();

  @override
  Future<void> stop() => bridge.stop();

  @override
  Future<void> setActive(bool active) => bridge.setActive(active: active);

  @override
  Stream<AppSnapshot> watchStates() => bridge.watchStates();

  @override
  Stream<AppDetail?> watchDetail(int? collection) =>
      bridge.watchDetail(collection: collection);

  @override
  Stream<Uint8List> watchHistory(int collection) =>
      bridge.watchHistory(collection: collection);

  @override
  Future<AppAccepted> send(AppCommand command) => bridge.send(command: command);
}
