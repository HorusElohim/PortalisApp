import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/nexus/application/service_controller.dart';
import 'package:portalis/nexus/data/service_repository.dart';
import 'package:portalis/nexus/domain/endpoint_config.dart';

void main() {
  test('Nexus settings load and save one trusted endpoint', () async {
    final repository = _MemoryRepository();
    final controller = ServiceController(repository: repository);
    const endpoint = EndpointConfig(
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

class _MemoryRepository implements ServiceRepository {
  EndpointConfig value = const EndpointConfig();

  @override
  Future<EndpointConfig> load() async => value;

  @override
  Future<void> save(EndpointConfig config) async {
    value = config;
  }
}
