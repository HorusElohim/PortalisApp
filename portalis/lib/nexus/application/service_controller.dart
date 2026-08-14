import 'package:flutter/foundation.dart';

import '../data/service_repository.dart';
import '../domain/endpoint_config.dart';

/// UI state for the saved Nexus service, independent of its live connection.
class ServiceController extends ChangeNotifier {
  ServiceController({required ServiceRepository repository})
      : _repository = repository;

  final ServiceRepository _repository;

  factory ServiceController.production() => ServiceController(
        repository: const FrbServiceRepository(),
      );

  EndpointConfig _config = const EndpointConfig();
  EndpointConfig get config => _config;
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

  Future<void> save(EndpointConfig config) async {
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
