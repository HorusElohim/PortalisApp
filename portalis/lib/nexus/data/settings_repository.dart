import '../bridge/nexus_settings.dart' as bridge;
import '../domain/endpoint_config.dart';

/// Native persistence for the trusted Nexus service endpoint.
abstract interface class NexusSettingsRepository {
  Future<NexusEndpointConfig> load();
  Future<void> save(NexusEndpointConfig config);
}

/// Flutter-Rust Bridge implementation. Generated DTOs remain at this edge.
class FrbNexusSettingsRepository implements NexusSettingsRepository {
  const FrbNexusSettingsRepository();

  @override
  Future<NexusEndpointConfig> load() async =>
      _fromBridge(await bridge.nexusEndpointConfig());

  @override
  Future<void> save(NexusEndpointConfig config) =>
      bridge.setNexusEndpointConfig(config: _toBridge(config));

  static NexusEndpointConfig _fromBridge(bridge.NexusEndpointConfig config) =>
      NexusEndpointConfig(
        nodeId: config.nodeId,
        directAddress: config.directAddress,
      );

  static bridge.NexusEndpointConfig _toBridge(NexusEndpointConfig config) =>
      bridge.NexusEndpointConfig(
        nodeId: config.nodeId,
        directAddress: config.directAddress,
      );
}
