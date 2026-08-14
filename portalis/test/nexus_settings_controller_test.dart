import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/nexus/application/settings_controller.dart';
import 'package:portalis/nexus/data/settings_repository.dart';
import 'package:portalis/nexus/domain/endpoint_config.dart';

void main() {
  test('Nexus settings load and save one trusted endpoint', () async {
    final repository = _MemoryRepository();
    final controller = NexusSettingsController(repository: repository);
    const endpoint = NexusEndpointConfig(
      nodeId: 'node-id',
      directAddress: '127.0.0.1:7443',
    );

    await controller.load();
    expect(controller.loaded, isTrue);
    expect(controller.config.isConfigured, isFalse);

    await controller.save(endpoint);
    expect(repository.value, endpoint);
    expect(controller.config, endpoint);
    expect(controller.lastError, isNull);
  });
}

class _MemoryRepository implements NexusSettingsRepository {
  NexusEndpointConfig value = const NexusEndpointConfig();

  @override
  Future<NexusEndpointConfig> load() async => value;

  @override
  Future<void> save(NexusEndpointConfig config) async {
    value = config;
  }
}
