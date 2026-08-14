import '../bridge/portalis_api.dart' as bridge;
import '../domain/app_state.dart';

/// The native contract the application consumes.
///
/// An interface rather than the bridge functions themselves, because that is
/// what lets a widget test substitute the engine — every test in this project
/// implements this rather than faking FFI. The projection types it returns are
/// the generated ones: mirroring them by hand bought nothing and could lose a
/// field in silence. See `domain/app_state.dart`.
abstract interface class AppRepository {
  Future<void> start();
  Future<void> stop();
  Future<void> setActive(bool active);
  Stream<AppSnapshot> watchStates();
  Stream<AppDetail?> watchDetail(int? collection);
  Future<AppAccepted> send(EngineCommand command);
}

class FrbAppRepository implements AppRepository {
  const FrbAppRepository();

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
  Future<AppAccepted> send(EngineCommand command) =>
      bridge.send(command: command.toBridge());
}
