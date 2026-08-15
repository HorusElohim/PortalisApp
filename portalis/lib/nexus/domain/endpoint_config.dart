/// The trusted public identity and route of the Nexus service.
///
/// The Node ID is the cryptographic identity Iroh authenticates. The direct
/// address only tells it where to begin reaching that identity, so it may
/// change without changing what the app trusts.
/// Where a service runs when somebody is running one locally.
///
/// A suggestion, not a stored value: the field opens with it so the common
/// case is one paste of a Node ID rather than two of everything, and typing
/// over it costs nothing. `tool/nexus_server.sh` listens here by default.
const defaultDirectAddress = '127.0.0.1:8080';

class EndpointConfig {
  const EndpointConfig({this.nodeId, this.directAddress});

  final String? nodeId;
  final String? directAddress;

  bool get isConfigured => nodeId != null && directAddress != null;
}
