//! Authenticated application connections without a Nexus-specific data layer.
//!
//! Iroh owns QUIC, TLS, direct paths, hole punching, and relay fallback. Nexus
//! supplies the existing device secret, accepted ALPNs, and remote address;
//! the caller owns every byte written to the returned QUIC connection.

use iroh::{
    Endpoint, NodeAddr, NodeId, RelayMode, SecretKey, Watcher as _,
    discovery::dns::DnsDiscovery,
    endpoint::{BindError, ConnectError, Connection, ConnectionType, Incoming},
};
pub use portalis_nexus_protocol::NEXUS_ALPN;

type EndpointAddr = NodeAddr;
type EndpointId = NodeId;

/// The path Iroh currently has available to a remote device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionPath {
    /// No verified path is currently available.
    Unavailable,
    /// Traffic has a verified direct UDP path.
    Direct,
    /// Traffic currently uses an encrypted relay path.
    Relay,
    /// Direct and relay addresses exist, but the direct path is not confirmed.
    Mixed,
}

/// How a device finds, and is found by, others it only knows by Node ID.
///
/// Explicit for the same reason `RelayMode` is: a test that reaches for
/// multicast or a public name server stops measuring this code and starts
/// measuring the network it happens to run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discovery {
    /// Resolve nothing. A peer is reachable only at an address supplied by
    /// the caller.
    Disabled,
    /// Find others by Node ID without announcing an address to the wider
    /// internet.
    ///
    /// For an endpoint whose key is generated per connection: nobody can look
    /// up an identity that exists for one dial, so publishing a record for it
    /// would announce this device's addresses under a name no one will ever
    /// ask for. mDNS still participates, because taking part in the local
    /// swarm is how a device on this network is found at all.
    Resolving,
    /// Publish and resolve everywhere available: mDNS for the local network,
    /// and pkarr records on n0's name server for everywhere else.
    ///
    /// Both, because they fail in opposite conditions — mDNS needs no
    /// internet and cannot leave the subnet; pkarr needs the internet and
    /// does not care where either device is.
    ///
    /// For anything a peer is meant to reach by Node ID alone.
    Published,
}

/// One device endpoint for both server and peer application connections.
#[derive(Debug, Clone)]
pub struct NexusEndpoint {
    inner: Endpoint,
}

impl NexusEndpoint {
    /// Binds an endpoint using the device's existing Ed25519 secret.
    ///
    /// `relay_mode` and `discovery` are explicit so production can be found
    /// from anywhere while local tests and deployments stay off the network
    /// entirely.
    ///
    /// # Errors
    ///
    /// Returns an error when Iroh cannot bind the local sockets or configure
    /// the requested relay.
    pub async fn bind(
        device_secret: [u8; 32],
        alpns: Vec<Vec<u8>>,
        relay_mode: RelayMode,
        discovery: Discovery,
    ) -> Result<Self, BindError> {
        let builder = Endpoint::builder()
            .relay_mode(relay_mode)
            .secret_key(SecretKey::from_bytes(&device_secret))
            .alpns(alpns);
        let builder = match discovery {
            Discovery::Disabled => builder.clear_discovery(),
            // Resolution without publication: the DNS half of `discovery_n0`
            // without its pkarr publisher.
            Discovery::Resolving => builder
                .clear_discovery()
                .add_discovery(DnsDiscovery::n0_dns())
                .discovery_local_network(),
            // `discovery_n0` publishes a pkarr record and resolves through
            // n0's name server; `discovery_local_network` is mDNS. Neither
            // replaces an address the caller already knows — both are tried
            // alongside it, and whichever answers first is used.
            Discovery::Published => builder.discovery_n0().discovery_local_network(),
        };
        Ok(Self {
            inner: builder.bind().await?,
        })
    }

    /// The public connection identity derived from the supplied device secret.
    #[must_use]
    pub fn id(&self) -> EndpointId {
        self.inner.node_id()
    }

    /// Current direct and relay addressing information to publish for peers.
    #[must_use]
    pub fn addr(&self) -> EndpointAddr {
        let unaddressed = EndpointAddr::new(self.id());
        self.inner.node_addr().get().unwrap_or(unaddressed)
    }

    /// The addressing to publish, once there is any.
    ///
    /// [`Self::addr`] answers immediately and may answer with nothing: binding
    /// a socket and discovering which addresses reach it are different events,
    /// and a caller that hands out the first answer gives a peer nowhere to
    /// send packets. Anything sharing its address with another device wants
    /// this one.
    pub async fn addr_when_ready(&self) -> EndpointAddr {
        self.inner.node_addr().initialized().await
    }

    /// Connects to a known application peer and returns its raw QUIC connection.
    ///
    /// # Errors
    ///
    /// Returns an error when no supplied address reaches the authenticated
    /// remote device or when QUIC, TLS, or ALPN negotiation fails.
    pub async fn connect(
        &self,
        remote: impl Into<EndpointAddr>,
        alpn: &[u8],
    ) -> Result<Connection, ConnectError> {
        self.inner.connect(remote, alpn).await
    }

    /// Waits for the next authenticated incoming connection attempt.
    pub async fn accept(&self) -> Option<Incoming> {
        self.inner.accept().await
    }

    /// Reports transport truth without turning it into application state.
    #[must_use]
    pub fn path_to(&self, remote: EndpointId) -> ConnectionPath {
        path_of(
            self.inner
                .conn_type(remote)
                .map(|mut watcher| watcher.get())
                .as_ref(),
        )
    }

    /// Notifies the endpoint after an OS network transition.
    pub async fn network_change(&self) {
        self.inner.network_change().await;
    }

    /// Closes all connections and releases the endpoint sockets.
    pub async fn close(&self) {
        self.inner.close().await;
    }
}

/// Translates the transport's view of a connection into ours.
fn path_of(conn_type: Option<&ConnectionType>) -> ConnectionPath {
    match conn_type {
        Some(ConnectionType::Direct(_)) => ConnectionPath::Direct,
        Some(ConnectionType::Relay(_)) => ConnectionPath::Relay,
        Some(ConnectionType::Mixed(_, _)) => ConnectionPath::Mixed,
        Some(ConnectionType::None) | None => ConnectionPath::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use ed25519_dalek::SigningKey;

    use super::*;

    /// Every mapping, including the two that need a relay server to arise in
    /// the wild — which is why the translation is a function rather than a
    /// match buried in a method that cannot be called without a network.
    #[test]
    fn a_connection_type_maps_onto_the_path_we_report() {
        use std::net::{Ipv4Addr, SocketAddr};

        let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);
        let relay: iroh::RelayUrl = "https://relay.invalid".parse().expect("a relay url");

        assert_eq!(
            path_of(Some(&ConnectionType::Direct(socket))),
            ConnectionPath::Direct
        );
        assert_eq!(
            path_of(Some(&ConnectionType::Relay(relay.clone()))),
            ConnectionPath::Relay
        );
        assert_eq!(
            path_of(Some(&ConnectionType::Mixed(socket, relay))),
            ConnectionPath::Mixed
        );
        assert_eq!(
            path_of(Some(&ConnectionType::None)),
            ConnectionPath::Unavailable
        );
        assert_eq!(path_of(None), ConnectionPath::Unavailable);
    }

    #[tokio::test]
    async fn reuses_the_existing_device_identity() {
        let secret = [7_u8; 32];
        let endpoint = NexusEndpoint::bind(
            secret,
            vec![NEXUS_ALPN.to_vec()],
            RelayMode::Disabled,
            Discovery::Disabled,
        )
        .await
        .expect("bind endpoint");

        assert_eq!(
            endpoint.id().as_bytes(),
            &SigningKey::from_bytes(&secret).verifying_key().to_bytes()
        );
        endpoint.close().await;
    }

    #[tokio::test]
    async fn returns_raw_streams_over_a_direct_connection() {
        let server = NexusEndpoint::bind(
            [1; 32],
            vec![NEXUS_ALPN.to_vec()],
            RelayMode::Disabled,
            Discovery::Disabled,
        )
        .await
        .expect("bind server");
        let client = NexusEndpoint::bind(
            [2; 32],
            Vec::new(),
            RelayMode::Disabled,
            Discovery::Disabled,
        )
        .await
        .expect("bind client");

        let mut address = EndpointAddr::new(server.id());
        let server_socket = server
            .inner
            .bound_sockets()
            .into_iter()
            .find(std::net::SocketAddr::is_ipv4)
            .expect("server IPv4 socket");
        address = address.with_direct_addresses([std::net::SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            server_socket.port(),
        )]);

        let accepting = tokio::spawn({
            let server = server.clone();
            async move {
                let incoming = server.accept().await.expect("incoming connection");
                let connection = incoming
                    .accept()
                    .expect("accept connection")
                    .await
                    .expect("authenticate connection");
                let (mut send, mut receive) = connection.accept_bi().await.expect("accept stream");
                let request = receive.read_to_end(16).await.expect("read request");
                send.write_all(&request).await.expect("write response");
                send.finish().expect("finish response");
                connection
            }
        });

        let connection = client
            .connect(address, NEXUS_ALPN)
            .await
            .expect("connect directly");
        let (mut send, mut receive) = connection.open_bi().await.expect("open stream");
        send.write_all(b"raw bytes").await.expect("write request");
        send.finish().expect("finish request");

        assert_eq!(
            receive.read_to_end(16).await.expect("read response"),
            b"raw bytes"
        );
        assert_eq!(client.path_to(server.id()), ConnectionPath::Direct);
        assert_eq!(client.addr().node_id, client.id());
        // A peer it has never met has no path, and an OS network change is
        // something the endpoint absorbs rather than reports on.
        assert_eq!(
            client.path_to(
                NexusEndpoint::bind(
                    [3; 32],
                    Vec::new(),
                    RelayMode::Disabled,
                    Discovery::Disabled
                )
                .await
                .expect("bind a stranger")
                .id()
            ),
            ConnectionPath::Unavailable
        );
        client.network_change().await;

        let _server_connection = accepting.await.expect("server task");
        client.close().await;
        server.close().await;

        // A closed endpoint has no addresses left to publish, and still knows
        // who it is.
        assert_eq!(client.addr().node_id, client.id());
    }
}
