import 'dart:async';

import 'package:flutter/foundation.dart';

import '../data/nexus_app_repository.dart';
import '../domain/nexus_app_state.dart';

/// Owns Portalis's one app-level Nexus state subscription.
class NexusAppController extends ChangeNotifier {
  NexusAppController({required NexusAppRepository repository})
      : _repository = repository;

  factory NexusAppController.production() => NexusAppController(
        repository: const FrbNexusAppRepository(),
      );

  final NexusAppRepository _repository;
  StreamSubscription<NexusAppState>? _subscription;
  Future<void>? _starting;

  NexusAppState? _state;
  NexusAppState? get state => _state;
  String? lastError;
  bool get started => _starting != null;

  /// Opens the runtime and subscribes once. Repeated starts share the same
  /// future, which keeps hot reload and shell remounts from duplicating a
  /// native stream.
  Future<void> start() => _starting ??= _start();

  Future<void> _start() async {
    try {
      await _repository.start();
      _subscription = _repository.watchStates().listen(
        (state) {
          _state = state;
          lastError = null;
          notifyListeners();
        },
        onError: (Object error, StackTrace _) {
          lastError = '$error';
          notifyListeners();
        },
      );
    } catch (error) {
      lastError = '$error';
      _starting = null;
      notifyListeners();
      rethrow;
    }
  }

  Future<void> setActive(bool active) async {
    try {
      await _repository.setActive(active);
    } catch (error) {
      lastError = '$error';
      notifyListeners();
    }
  }

  Future<NexusAccepted> send(NexusCommand command) => _repository.send(command);

  Future<void> stop() async {
    final subscription = _subscription;
    _subscription = null;
    await subscription?.cancel();
    await _repository.stop();
    _starting = null;
  }

  @override
  void dispose() {
    unawaited(stop());
    super.dispose();
  }
}
