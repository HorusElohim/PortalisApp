import '../bridge/nexus_settings.dart' as bridge;
import '../domain/endpoint_config.dart';

/// The Nexus service this build talks to.
///
/// Read-only: the service is the same one for everybody and is compiled in,
/// so there is nothing to persist and nothing for a person to set.
abstract interface class ServiceRepository {
  Future<EndpointConfig> load();
}

/// Flutter-Rust Bridge implementation. Generated DTOs remain at this edge.
class FrbServiceRepository implements ServiceRepository {
  const FrbServiceRepository();

  @override
  Future<EndpointConfig> load() async =>
      _fromBridge(await bridge.nexusEndpointConfig());

  static EndpointConfig _fromBridge(bridge.NexusEndpointConfig config) =>
      EndpointConfig(
        nodeId: config.nodeId,
        directAddress: config.directAddress,
      );
}
