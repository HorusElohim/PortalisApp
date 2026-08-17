import 'package:flutter/foundation.dart';

import '../data/service_repository.dart';
import '../domain/endpoint_config.dart';

/// UI state for the Nexus service this build ships with, independent of
/// its live connection.
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
}
