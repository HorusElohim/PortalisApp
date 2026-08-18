//! Shared fixtures for the transport integration tests.
//!
//! Each test binary uses a subset of these helpers, so unused items are
//! expected here.
#![allow(dead_code)]

use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use portalis_nexus_client::{ClientConfig, DeviceSigner, EndpointAddr, ReconnectPolicy};
use portalis_nexus_protocol::v1::envelope::Payload;
use portalis_nexus_protocol::v1::{Envelope, Ping, ProtocolRange, ServerHello};
use portalis_nexus_protocol::{
    DEVICE_KEY_BYTES, ENCRYPTION_KEY_BYTES, LENGTH_PREFIX_BYTES, SIGNATURE_BYTES, decode_frame,
    frame_length, length_prefix, new_challenge, new_message_id,
};
use portalis_nexus_server::{AppState, binary_frame, hello_envelope, server_hello};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use x25519_dalek::{PublicKey, StaticSecret};

/// Bounds every "eventually" assertion so a hung test fails instead of hanging.
pub const PATIENCE: Duration = Duration::from_secs(30);

const SERVICE_SECRET: [u8; 32] = [7; 32];
const PEER_SECRET: [u8; 32] = [8; 32];

/// A signer over a fixed key, so a test can re-create the same device.
///
/// Shared by every integration test file rather than defined locally in
/// each: a locally-defined type is its own distinct monomorphization target
/// for `ClientProtocol`'s generic methods, which fragments coverage of that
/// generic code across as many compiled copies as there are local
/// definitions. One shared type means one compiled copy to cover.
pub struct TestDevice {
    signing: SigningKey,
    /// A real X25519 secret, so a test can actually open what was sealed to
    /// this device rather than only assert that bytes moved.
    encryption: StaticSecret,
}

impl TestDevice {
    /// The private half, which only the device itself ever holds. Passed to
    /// [`portalis_nexus_protocol::open`] by whoever is standing in for this
    /// device; it never crosses the wire.
    pub fn encryption_secret_key(&self) -> [u8; ENCRYPTION_KEY_BYTES] {
        self.encryption.to_bytes()
    }
}

impl DeviceSigner for TestDevice {
    fn public_key(&self) -> [u8; DEVICE_KEY_BYTES] {
        self.signing.verifying_key().to_bytes()
    }

    fn encryption_public_key(&self) -> [u8; ENCRYPTION_KEY_BYTES] {
        PublicKey::from(&self.encryption).to_bytes()
    }

    fn sign(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES] {
        self.signing.sign(payload).to_bytes()
    }
}

pub fn device(seed: u8) -> TestDevice {
    TestDevice {
        signing: SigningKey::from_bytes(&[seed; 32]),
        // A different byte pattern than the signing seed, so a test that
        // confuses the two keys fails rather than coincidentally passing.
        encryption: StaticSecret::from([seed.wrapping_add(128); 32]),
    }
}

/// Reserves an ephemeral port so a restarted server can rebind the address.
pub async fn reserve_address() -> SocketAddr {
    let reservation = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("reserve test server address");
    let address = reservation.local_addr().expect("test server address");
    drop(reservation);
    address
}

/// Starts the real Nexus server, ready to serve.
///
/// Signatures bind to the Node ID the test service exposes, not its address.
pub async fn start_server(address: SocketAddr) -> (AppState, JoinHandle<()>) {
    let state = AppState::default().with_server_identity(&endpoint(address).node_id.to_string());
    state.mark_ready();
    let SocketAddr::V4(address) = address else {
        panic!("tests bind IPv4 addresses")
    };
    let service = iroh::Endpoint::builder()
        .bind_addr_v4(address)
        .clear_discovery()
        .relay_mode(iroh::RelayMode::Disabled)
        .secret_key(iroh::SecretKey::from_bytes(&SERVICE_SECRET))
        .alpns(vec![portalis_nexus_client::NEXUS_ALPN.to_vec()])
        .bind()
        .await
        .expect("bind test QUIC server");
    let serving_state = state.clone();
    let handle = tokio::spawn(async move {
        let mut draining = serving_state.shutdown().register();
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _ = draining.changed() => {
                    service.close().await;
                    return;
                }
                incoming = service.accept() => {
                    let Some(incoming) = incoming else {
                        return;
                    };
                    let state = serving_state.clone();
                    let endpoint = service.clone();
                    connections.spawn(async move {
                        let Ok(connection) = incoming.await else {
                            return;
                        };
                        let observed_ip = portalis_nexus_server::quic::direct_peer_ip(
                            &endpoint,
                            &connection,
                        );
                        portalis_nexus_server::quic::serve(connection, state, observed_ip).await;
                    });
                }
            }
        }
    });
    (state, handle)
}

pub fn endpoint(address: SocketAddr) -> EndpointAddr {
    EndpointAddr::new(iroh::SecretKey::from_bytes(&SERVICE_SECRET).public())
        .with_direct_addresses([address])
}

/// Address of a deliberately narrow QUIC peer used only for transport faults.
pub fn peer_endpoint(address: SocketAddr) -> EndpointAddr {
    EndpointAddr::new(iroh::SecretKey::from_bytes(&PEER_SECRET).public())
        .with_direct_addresses([address])
}

/// Runs one QUIC peer for a test that needs behavior the real service should
/// not provide, such as silence or an intentionally malformed reply.
pub async fn start_peer<F, Fut>(
    address: SocketAddr,
    alpns: Vec<Vec<u8>>,
    serve: F,
) -> JoinHandle<()>
where
    F: Fn(iroh::endpoint::Connection) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let SocketAddr::V4(address) = address else {
        panic!("tests bind IPv4 addresses")
    };
    let endpoint = iroh::Endpoint::builder()
        .bind_addr_v4(address)
        .clear_discovery()
        .relay_mode(iroh::RelayMode::Disabled)
        .secret_key(iroh::SecretKey::from_bytes(&PEER_SECRET))
        .alpns(alpns)
        .bind()
        .await
        .expect("bind test QUIC peer");
    let serve = std::sync::Arc::new(serve);
    tokio::spawn(async move {
        loop {
            let Some(incoming) = endpoint.accept().await else {
                return;
            };
            let Ok(connection) = incoming.await else {
                return;
            };
            std::sync::Arc::clone(&serve)(connection).await;
        }
    })
}

/// A reconnect policy that retries quickly enough for a test to observe it.
pub fn brisk_policy(maximum_attempts: u32) -> ReconnectPolicy {
    ReconnectPolicy::new(
        Duration::from_millis(20),
        Duration::from_millis(20),
        maximum_attempts,
    )
    .expect("valid test reconnect policy")
}

pub fn brisk_config(maximum_attempts: u32) -> ClientConfig {
    ClientConfig {
        reconnect: brisk_policy(maximum_attempts),
        // Brisk about the dial as well as the backoff. A QUIC dial to a node
        // that is not there is not refused by anybody, so every attempt runs
        // to its bound; with the default one a three-attempt test waits half a
        // minute to learn what it learns in a fraction of a second.
        handshake_timeout: Duration::from_millis(250),
        ..ClientConfig::default()
    }
}

/// Polls until `condition` holds, failing the test if it never does.
pub async fn wait_until(label: &str, mut condition: impl FnMut() -> bool) {
    timeout(PATIENCE, async {
        while !condition() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting until {label}"));
}

/// A valid greeting from the current protocol version.
fn greeting() -> Envelope {
    let state = AppState::default();
    server_hello(state.protocol_policy(), 0)
}

/// A greeting advertising a protocol range this client cannot speak.
fn future_greeting() -> Envelope {
    hello_envelope(
        ServerHello {
            connection_id: new_message_id(),
            challenge: new_challenge(),
            server_time_unix_ns: 0,
            supported_protocols: Some(ProtocolRange {
                minimum: 99,
                maximum: 99,
            }),
        },
        0,
    )
}

pub fn unsolicited_ping(nonce: u64) -> Envelope {
    Envelope {
        message_id: new_message_id(),
        correlation_id: Vec::new(),
        timestamp_unix_ns: 1,
        payload: Some(Payload::Ping(Ping { nonce })),
    }
}

pub async fn greet(
    connection: iroh::endpoint::Connection,
    greeting: Envelope,
) -> Option<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream)> {
    let (mut send, receive) = connection.open_bi().await.ok()?;
    send_envelope(&mut send, &greeting).await?;
    Some((send, receive))
}

pub async fn send_envelope(
    send: &mut iroh::endpoint::SendStream,
    envelope: &Envelope,
) -> Option<()> {
    let frame = binary_frame(envelope);
    send.write_all(&length_prefix(&frame)).await.ok()?;
    send.write_all(&frame).await.ok()?;
    Some(())
}

pub async fn receive_envelope(receive: &mut iroh::endpoint::RecvStream) -> Option<Envelope> {
    let mut prefix = [0_u8; LENGTH_PREFIX_BYTES];
    receive.read_exact(&mut prefix).await.ok()?;
    let length = frame_length(prefix).ok()?;
    let mut frame = vec![0_u8; length];
    receive.read_exact(&mut frame).await.ok()?;
    decode_frame(&frame).ok()
}

/// Greets correctly, then never answers another message.
pub async fn silent_peer(connection: iroh::endpoint::Connection) {
    let Some((_send, mut receive)) = greet(connection, greeting()).await else {
        return;
    };
    while receive_envelope(&mut receive).await.is_some() {}
}

/// Greets with a protocol range this client cannot speak.
pub async fn future_peer(connection: iroh::endpoint::Connection) {
    let Some((_send, mut receive)) = greet(connection, future_greeting()).await else {
        return;
    };
    while receive_envelope(&mut receive).await.is_some() {}
}

/// Greets, then answers every request with a pong, whatever was asked.
pub async fn misanswering_peer(connection: iroh::endpoint::Connection) {
    let Some((mut send, mut receive)) = greet(connection, greeting()).await else {
        return;
    };
    while let Some(request) = receive_envelope(&mut receive).await {
        let pong = Envelope {
            message_id: new_message_id(),
            correlation_id: request.message_id,
            timestamp_unix_ns: 1,
            payload: Some(Payload::Pong(portalis_nexus_protocol::v1::Pong {
                nonce: 0,
            })),
        };
        if send_envelope(&mut send, &pong).await.is_none() {
            return;
        }
    }
}

/// Greets, then drops its QUIC connection when the returned switch is flipped.
pub async fn closable_peer(
    connection: iroh::endpoint::Connection,
    mut close: watch::Receiver<bool>,
) {
    let Some((_send, _receive)) = greet(connection.clone(), greeting()).await else {
        return;
    };
    let _ = close.changed().await;
    connection.close(0_u32.into(), b"test close");
}
