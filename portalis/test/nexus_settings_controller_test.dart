import 'package:flutter_test/flutter_test.dart';
import 'package:portalis/nexus/application/service_controller.dart';
import 'package:portalis/nexus/data/service_repository.dart';
import 'package:portalis/nexus/domain/endpoint_config.dart';

void main() {
  test('the Nexus service is read from the build, not chosen', () async {
    const shipped = EndpointConfig(nodeId: 'node-id');
    final controller = ServiceController(
      repository: _ShippedRepository(shipped),
    );

    await controller.load();

    expect(controller.loaded, isTrue);
    expect(controller.config, shipped);
    expect(controller.config.isConfigured, isTrue,
        reason: 'a build that ships a service is configured on first run');
    expect(controller.lastError, isNull);
  });

  test('a build with no service says so instead of failing', () async {
    final controller = ServiceController(
      repository: _ShippedRepository(const EndpointConfig()),
    );

    await controller.load();

    expect(controller.config.isConfigured, isFalse);
    expect(controller.lastError, isNull);
  });
}

class _ShippedRepository implements ServiceRepository {
  _ShippedRepository(this.value);

  final EndpointConfig value;

  @override
  Future<EndpointConfig> load() async => value;
}
