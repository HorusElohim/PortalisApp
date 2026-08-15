//! Handshake, request correlation, and connection teardown.

use std::sync::Arc;
use std::time::Duration;

use portalis_nexus_client::{ClientConfig, ClientError, NexusClient, TransportError};
use portalis_nexus_protocol::MAX_PENDING_REQUESTS;
use portalis_nexus_protocol::v1::envelope::Payload;
use portalis_nexus_protocol::v1::{Envelope, Pong};
use portalis_nexus_protocol::{CURRENT_PROTOCOL_VERSION, new_message_id};
use tokio::time::timeout;

mod common;

use common::{
    PATIENCE, endpoint, future_peer, peer_endpoint, reserve_address, silent_peer, start_peer,
    start_server, wait_until,
};

#[tokio::test]
async fn establishes_a_validated_session() {
    let address = reserve_address().await;
    let (_state, server) = start_server(address).await;

    let client = NexusClient::connect(&endpoint(address))
        .await
        .expect("connect Nexus client");

    let hello = client.hello().expect("a live connection has a hello");
    assert_eq!(hello.challenge.len(), 32);
    assert_eq!(hello.connection_id.len(), 16);
    let range = hello.supported_protocols.expect("protocol range");
    assert!((range.minimum..=range.maximum).contains(&CURRENT_PROTOCOL_VERSION));
    assert!(client.is_connected());
    assert_eq!(client.in_flight(), 0);

    client.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn exchanges_a_correlated_ping() {
    let address = reserve_address().await;
    let (_state, server) = start_server(address).await;
    let client = NexusClient::connect(&endpoint(address))
        .await
        .expect("connect Nexus client");

    let response = client.ping(42).await.expect("receive pong");

    assert_eq!(response.correlation_id.len(), 16);
    assert_eq!(response.payload, Some(Payload::Pong(Pong { nonce: 42 })));
    assert_eq!(client.in_flight(), 0);
    client.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn correlates_concurrent_requests_independently() {
    let address = reserve_address().await;
    let (_state, server) = start_server(address).await;
    let client = NexusClient::connect(&endpoint(address))
        .await
        .expect("connect Nexus client");

    let responses = futures_util::future::join_all((0..32).map(|nonce| client.ping(nonce))).await;

    for (nonce, response) in responses.into_iter().enumerate() {
        let nonce = u64::try_from(nonce).expect("small nonce");
        assert_eq!(
            response.expect("pong").payload,
            Some(Payload::Pong(Pong { nonce })),
            "response {nonce} was correlated to the wrong request"
        );
    }
    assert_eq!(client.in_flight(), 0);
    client.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn relays_a_correlated_protocol_error_to_its_caller() {
    let address = reserve_address().await;
    let (_state, server) = start_server(address).await;
    let client = NexusClient::connect(&endpoint(address))
        .await
        .expect("connect Nexus client");
    // A Pong is not a command the server accepts before authentication.
    let unsupported = Envelope {
        message_id: new_message_id(),
        correlation_id: Vec::new(),
        timestamp_unix_ns: 1,
        payload: Some(Payload::Pong(Pong { nonce: 1 })),
    };

    let response = client
        .request(&unsupported)
        .await
        .expect("a protocol error is still a correlated response");

    assert_eq!(response.correlation_id, unsupported.message_id);
    assert!(
        matches!(response.payload, Some(Payload::ProtocolError(_))),
        "expected a protocol error, got {:?}",
        response.payload
    );
    client.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn rejects_a_server_that_does_not_speak_the_nexus_alpn() {
    let address = reserve_address().await;
    let server = start_peer(address, vec![b"not-nexus".to_vec()], |_| async {}).await;

    let error = NexusClient::connect(peer_endpoint(address))
        .await
        .expect_err("the ALPN is mandatory");

    assert!(
        matches!(error, TransportError::IrohConnect(_)),
        "expected the handshake to reject the ALPN, got {error:?}"
    );
    server.abort();
}

#[tokio::test]
async fn rejects_a_server_speaking_an_unsupported_protocol_version() {
    let address = reserve_address().await;
    let server = start_peer(
        address,
        vec![portalis_nexus_client::NEXUS_ALPN.to_vec()],
        future_peer,
    )
    .await;

    let error = NexusClient::connect(peer_endpoint(address))
        .await
        .expect_err("the protocol range excludes this client");

    assert!(
        matches!(
            error,
            TransportError::Client(ClientError::UnsupportedProtocolVersion)
        ),
        "expected an unsupported version, got {error:?}"
    );
    server.abort();
}

#[tokio::test]
async fn rejects_an_endpoint_without_an_address() {
    let error = NexusClient::connect(portalis_nexus_client::EndpointAddr::new(
        iroh::SecretKey::from_bytes(&[9; 32]).public(),
    ))
    .await
    .expect_err("the endpoint cannot be reached without an address");

    assert!(
        matches!(error, TransportError::IrohConnect(_)),
        "expected a QUIC error, got {error:?}"
    );
}

#[tokio::test]
async fn refuses_more_than_the_pending_request_limit() {
    let address = reserve_address().await;
    let server = start_peer(
        address,
        vec![portalis_nexus_client::NEXUS_ALPN.to_vec()],
        silent_peer,
    )
    .await;
    let config = ClientConfig {
        request_timeout: Duration::from_secs(30),
        ..ClientConfig::default()
    };
    let client = Arc::new(
        NexusClient::connect_with_config(peer_endpoint(address), &config)
            .await
            .expect("connect to the silent server"),
    );

    let stalled: Vec<_> = (0..MAX_PENDING_REQUESTS)
        .map(|nonce| {
            let client = Arc::clone(&client);
            tokio::spawn(async move { client.ping(nonce as u64).await })
        })
        .collect();
    wait_until("every request is registered", || {
        client.in_flight() == MAX_PENDING_REQUESTS
    })
    .await;

    let error = client
        .ping(u64::MAX)
        .await
        .expect_err("the registry is full");

    assert!(
        matches!(
            error,
            TransportError::Client(ClientError::TooManyPendingRequests)
        ),
        "expected a full registry, got {error:?}"
    );
    for request in stalled {
        request.abort();
    }
    server.abort();
}

#[tokio::test]
async fn draining_the_server_closes_live_connections() {
    let address = reserve_address().await;
    let (state, server) = start_server(address).await;
    let client = NexusClient::connect(&endpoint(address))
        .await
        .expect("connect before draining");

    timeout(PATIENCE, state.shutdown().drain())
        .await
        .expect("drain completes while a client is connected");

    assert!(state.shutdown().is_draining());
    assert!(
        client.ping(1).await.is_err(),
        "a drained connection cannot answer"
    );
    client.shutdown().await;
    server.abort();
}

/// The app talks to a server process, not to one hosted inside its own test
/// binary. This is the same handshake as the in-process case, made against a
/// separately-launched executable, so what it proves is that the thing an
/// operator actually runs is the thing a device can actually reach.
#[tokio::test]
#[ignore = "needs a server already listening; see tool/nexus_server.sh"]
async fn reaches_a_separately_launched_server() {
    let node_id = std::env::var("PORTALIS_NEXUS_NODE_ID").expect("a node id to dial");
    let address = std::env::var("PORTALIS_NEXUS_ADDR").expect("an address to dial");

    let endpoint =
        portalis_nexus_client::EndpointAddr::new(node_id.parse().expect("a valid node id"))
            .with_direct_addresses([address
                .parse::<std::net::SocketAddr>()
                .expect("a valid socket address")]);

    let client = NexusClient::connect(&endpoint)
        .await
        .expect("connect to the running server");

    let hello = client.hello().expect("a live connection has a hello");
    assert_eq!(hello.challenge.len(), 32);
}

/// Registering against a separately-launched service, so what is exercised is
/// the executable an operator runs rather than one hosted inside this binary.
#[tokio::test]
#[ignore = "needs a server already listening; see tool/nexus_server.sh"]
async fn registers_against_a_separately_launched_server() {
    let node_id = std::env::var("PORTALIS_NEXUS_NODE_ID").expect("a node id to dial");
    let address = std::env::var("PORTALIS_NEXUS_ADDR").expect("an address to dial");
    let endpoint =
        portalis_nexus_client::EndpointAddr::new(node_id.parse().expect("a valid node id"))
            .with_direct_addresses([address
                .parse::<std::net::SocketAddr>()
                .expect("a valid socket address")]);

    let client = NexusClient::connect(&endpoint)
        .await
        .expect("connect to the running server");
    let device = common::device(7);
    let handle = format!("probe{}", std::process::id());
    client
        .register(&handle, &device)
        .await
        .expect("register a user");
}
