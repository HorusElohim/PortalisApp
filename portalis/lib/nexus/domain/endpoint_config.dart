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

  /// Where to look first. Optional: the engine resolves a Node ID on its own
  /// over mDNS on this network, or a signed record on a name server anywhere
  /// else, so this is worth setting only to skip that or to reach a service
  /// that publishes neither.
  final String? directAddress;

  /// A Node ID is the whole configuration. It is the identity the connection
  /// is authenticated against; an address is a hint that can be found.
  bool get isConfigured => nodeId != null;
}
