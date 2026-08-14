/// The trusted public identity and route of the Nexus service.
///
/// The Node ID is the cryptographic identity Iroh authenticates. The direct
/// address only tells it where to begin reaching that identity, so it may
/// change without changing what the app trusts.
class EndpointConfig {
  const EndpointConfig({this.nodeId, this.directAddress});

  final String? nodeId;
  final String? directAddress;

  bool get isConfigured => nodeId != null && directAddress != null;
}
