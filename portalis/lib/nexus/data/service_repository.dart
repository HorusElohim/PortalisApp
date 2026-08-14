import '../bridge/nexus_settings.dart' as bridge;
import '../domain/endpoint_config.dart';

/// Native persistence for the trusted Nexus service endpoint.
abstract interface class ServiceRepository {
  Future<EndpointConfig> load();
  Future<void> save(EndpointConfig config);
}

/// Flutter-Rust Bridge implementation. Generated DTOs remain at this edge.
class FrbServiceRepository implements ServiceRepository {
  const FrbServiceRepository();

  @override
  Future<EndpointConfig> load() async =>
      _fromBridge(await bridge.nexusEndpointConfig());

  @override
  Future<void> save(EndpointConfig config) =>
      bridge.setNexusEndpointConfig(config: _toBridge(config));

  static EndpointConfig _fromBridge(bridge.NexusEndpointConfig config) =>
      EndpointConfig(
        nodeId: config.nodeId,
        directAddress: config.directAddress,
      );

  static bridge.NexusEndpointConfig _toBridge(EndpointConfig config) =>
      bridge.NexusEndpointConfig(
        nodeId: config.nodeId,
        directAddress: config.directAddress,
      );
}
