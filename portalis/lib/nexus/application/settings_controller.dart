import 'package:flutter/foundation.dart';

import '../data/settings_repository.dart';
import '../domain/endpoint_config.dart';

/// UI state for the saved Nexus service, independent of its live connection.
class NexusSettingsController extends ChangeNotifier {
  NexusSettingsController({required NexusSettingsRepository repository})
      : _repository = repository;

  final NexusSettingsRepository _repository;

  factory NexusSettingsController.production() => NexusSettingsController(
        repository: const FrbNexusSettingsRepository(),
      );

  NexusEndpointConfig _config = const NexusEndpointConfig();
  NexusEndpointConfig get config => _config;
  String? lastError;
  bool _loaded = false;
  bool get loaded => _loaded;

  Future<void> load() async {
    try {
      _config = await _repository.load();
      lastError = null;
    } catch (error) {
      lastError = '$error';
    }
    _loaded = true;
    notifyListeners();
  }

  Future<void> save(NexusEndpointConfig config) async {
    _config = config;
    notifyListeners();
    try {
      await _repository.save(config);
      _config = await _repository.load();
      lastError = null;
    } catch (error) {
      lastError = '$error';
      try {
        _config = await _repository.load();
      } catch (_) {
        // Keep the attempted value visible while the error explains why.
      }
      rethrow;
    } finally {
      notifyListeners();
    }
  }
}
