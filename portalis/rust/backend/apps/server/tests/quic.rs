//! The service over QUIC, answered by the same handlers as the WebSocket.
//!
//! The point of this test is the word "same". Two transports over one dispatch
//! is only worth having if the second one needs no rules of its own, and the
//! way to show that is to complete a real operation through it — a
//! registration, with a real signature, verified by the same code.

use ed25519_dalek::{Signer, SigningKey};
use iroh::Watcher as _;
use iroh::endpoint::Connection;
use portalis_nexus_protocol::{
    CURRENT_PROTOCOL_VERSION, MAX_FRAME_BYTES, SessionBinding, decode_frame, encode_frame,
    new_message_id, registration_payload, v1,
};
use portalis_nexus_server::{AppState, DEFAULT_SERVER_AUTHORITY};

/// The same ALPN the client crate dials, kept here as a literal so the test
/// would notice a change to it rather than follow one.
const ALPN: &[u8] = b"portalis/nexus/1";

/// One end of a QUIC connection, framing envelopes the way the service does.
struct Client {
    connection: Connection,
    send: iroh::endpoint::SendStream,
    receive: iroh::endpoint::RecvStream,
}

impl Client {
    async fn send(&mut self, envelope: &v1::Envelope) {
        let frame = encode_frame(envelope).expect("encodes");
        let length = u32::try_from(frame.len()).expect("bounded");
        self.send
            .write_all(&length.to_be_bytes())
            .await
            .expect("writes a length");
        self.send.write_all(&frame).await.expect("writes a frame");
    }

    /// Like [`Self::send`], for when the peer is expected to be going away.
    async fn try_send(
        &mut self,
        envelope: &v1::Envelope,
    ) -> Result<(), iroh::endpoint::WriteError> {
        let frame = encode_frame(envelope).expect("encodes");
        let length = u32::try_from(frame.len()).expect("bounded");
        self.send.write_all(&length.to_be_bytes()).await?;
        self.send.write_all(&frame).await
    }

    async fn receive(&mut self) -> v1::Envelope {
        let mut length = [0_u8; 4];
        self.receive
            .read_exact(&mut length)
            .await
            .expect("reads a length");
        let mut frame = vec![0_u8; u32::from_be_bytes(length) as usize];
        self.receive
            .read_exact(&mut frame)
            .await
            .expect("reads a frame");
        decode_frame(&frame).expect("decodes")
    }
}

async fn connected() -> (Client, tokio::task::JoinHandle<()>, AppState) {
    let service = iroh::Endpoint::builder()
        .clear_discovery()
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .expect("binds");
    let address = service.node_addr().initialized().await;

    let state = AppState::default();
    state.mark_ready();
    let serving_state = state.clone();
    let serving = tokio::spawn(async move {
        while let Some(incoming) = service.accept().await {
            let Ok(connection) = incoming.await else {
                continue;
            };
            portalis_nexus_server::quic::serve(connection, serving_state.clone()).await;
        }
    });

    let client = iroh::Endpoint::builder()
        .clear_discovery()
        .relay_mode(iroh::RelayMode::Disabled)
        .bind()
        .await
        .expect("binds");
    let connection = client.connect(address, ALPN).await.expect("connects");
    // The service opens the stream and greets first, so a client never has to
    // guess whether one is coming.
    let (send, receive) = connection.accept_bi().await.expect("the service's stream");

    (
        Client {
            connection,
            send,
            receive,
        },
        serving,
        state,
    )
}

#[tokio::test]
async fn a_device_registers_over_quic_through_the_same_handlers() {
    let (mut client, serving, _state) = connected().await;

    let hello = client.receive().await;
    let Some(v1::envelope::Payload::ServerHello(hello)) = hello.payload else {
        panic!("the service greets first");
    };
    assert!(!hello.connection_id.is_empty());
    assert!(!hello.challenge.is_empty());

    // A real signature over the real payload, verified by the real handler.
    let signer = SigningKey::from_bytes(&[7; 32]);
    let public_key = signer.verifying_key().to_bytes();
    let binding = SessionBinding {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        server_authority: DEFAULT_SERVER_AUTHORITY,
        connection_id: &hello.connection_id,
        challenge: &hello.challenge,
        server_time_unix_ns: hello.server_time_unix_ns,
    };
    let payload = registration_payload(&binding, "Ada", &public_key, &[8; 32]);
    let signature = signer.sign(&payload).to_bytes();

    let request = v1::Envelope {
        message_id: new_message_id(),
        correlation_id: Vec::new(),
        timestamp_unix_ns: hello.server_time_unix_ns,
        payload: Some(v1::envelope::Payload::RegisterUser(v1::RegisterUser {
            requested_username: "Ada".to_owned(),
            device_public_key: public_key.to_vec(),
            encryption_public_key: vec![8; 32],
            signature: signature.to_vec(),
        })),
    };
    let message_id = request.message_id.clone();
    client.send(&request).await;

    let reply = client.receive().await;

    assert_eq!(reply.correlation_id, message_id, "answered, and correlated");
    match reply.payload {
        Some(v1::envelope::Payload::Authenticated(authenticated)) => {
            assert!(!authenticated.user_id.is_empty());
            assert_eq!(authenticated.username, "Ada");
        }
        other => panic!("expected a registration, got {other:?}"),
    }

    client.connection.close(0_u32.into(), b"done");
    serving.abort();
}

/// A frame the service cannot parse ends the connection rather than being
/// answered: there is no message id to answer against.
#[tokio::test]
async fn an_undecodable_frame_ends_the_connection() {
    let (mut client, serving, _state) = connected().await;
    let _hello = client.receive().await;

    let nonsense = b"not a protobuf envelope at all";
    let length = u32::try_from(nonsense.len()).expect("bounded");
    client
        .send
        .write_all(&length.to_be_bytes())
        .await
        .expect("writes");
    client.send.write_all(nonsense).await.expect("writes");

    let mut length = [0_u8; 4];
    assert!(
        client.receive.read_exact(&mut length).await.is_err(),
        "the service said nothing and left"
    );

    serving.abort();
}

/// A length prefix larger than the frame limit is refused before anything is
/// allocated for it, which is the entire point of having the bound.
#[tokio::test]
async fn a_frame_over_the_limit_is_refused_before_it_is_read() {
    let (mut client, serving, _state) = connected().await;
    let _hello = client.receive().await;

    let too_long = u32::try_from(MAX_FRAME_BYTES + 1).expect("bounded");
    client
        .send
        .write_all(&too_long.to_be_bytes())
        .await
        .expect("writes");
    // Deliberately no body: the service must refuse on the prefix alone,
    // rather than wait for bytes it has already agreed to hold.

    let mut length = [0_u8; 4];
    assert!(
        client.receive.read_exact(&mut length).await.is_err(),
        "the service refused it and left"
    );

    serving.abort();
}

/// A draining server stops reading. The connection ends without an answer,
/// which is what lets a deployment finish in bounded time.
#[tokio::test]
async fn a_draining_server_lets_its_connections_go() {
    let service = iroh::Endpoint::builder()
        .clear_discovery()
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .expect("binds");
    let address = service.node_addr().initialized().await;

    let state = AppState::default();
    state.mark_ready();
    let draining = state.clone();
    let serving = tokio::spawn(async move {
        while let Some(incoming) = service.accept().await {
            let Ok(connection) = incoming.await else {
                continue;
            };
            portalis_nexus_server::quic::serve(connection, state.clone()).await;
        }
    });

    let endpoint = iroh::Endpoint::builder()
        .clear_discovery()
        .relay_mode(iroh::RelayMode::Disabled)
        .bind()
        .await
        .expect("binds");
    let connection = endpoint.connect(address, ALPN).await.expect("connects");
    let (send, receive) = connection.accept_bi().await.expect("the service's stream");
    let mut client = Client {
        connection,
        send,
        receive,
    };
    let _hello = client.receive().await;

    draining.shutdown().drain().await;

    let mut length = [0_u8; 4];
    assert!(
        client.receive.read_exact(&mut length).await.is_err(),
        "the service let go rather than waiting to be asked something"
    );

    serving.abort();
}

/// A cheap request the service always answers, for tests about the pipe
/// rather than about any particular operation.
fn ping(nonce: u64) -> v1::Envelope {
    v1::Envelope {
        message_id: new_message_id(),
        correlation_id: Vec::new(),
        timestamp_unix_ns: 0,
        payload: Some(v1::envelope::Payload::Ping(v1::Ping { nonce })),
    }
}

/// A peer that leaves before the service can open its stream costs nothing:
/// the connection is dropped rather than waited on.
#[tokio::test]
async fn a_peer_gone_before_the_stream_opens_is_let_go() {
    let service = iroh::Endpoint::builder()
        .clear_discovery()
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .expect("binds");
    let address = service.node_addr().initialized().await;

    let state = AppState::default();
    state.mark_ready();
    let serving = tokio::spawn(async move {
        let incoming = service.accept().await.expect("an incoming connection");
        let connection = incoming.await.expect("the handshake completes");
        // Serving begins only once the peer has definitely gone, so the
        // failure under test is the one being tested and not a race with it.
        connection.closed().await;
        portalis_nexus_server::quic::serve(connection, state).await;
    });

    let endpoint = iroh::Endpoint::builder()
        .clear_discovery()
        .relay_mode(iroh::RelayMode::Disabled)
        .bind()
        .await
        .expect("binds");
    let connection = endpoint.connect(address, ALPN).await.expect("connects");
    connection.close(0_u32.into(), b"changed my mind");

    tokio::time::timeout(std::time::Duration::from_secs(10), serving)
        .await
        .expect("the service gave up rather than waiting")
        .expect("no panic");
}

/// A peer that stops reading loses its connection instead of making the
/// service hold messages for it. Both halves report it: the writer, when the
/// write fails, and the read loop, when the queue behind it has gone.
#[tokio::test]
async fn a_peer_that_stops_reading_loses_its_connection() {
    let (mut client, serving, state) = connected().await;
    let _hello = client.receive().await;
    assert_eq!(state.connections().len(), 1, "the service is holding it");

    client
        .receive
        .stop(0_u32.into())
        .expect("refuses anything further");

    // A peer that has stopped reading but keeps asking. Both halves have to
    // notice for the connection to end, and they notice at different moments:
    // the writer when a write is refused, the read loop when the queue behind
    // it has gone. Asking repeatedly rather than a fixed number of times is
    // deliberate — how many requests fit before the refusal crosses the
    // network is a matter of timing, and a test that assumed a number would
    // pass or fail by it.
    for nonce in 0..100 {
        if state.connections().is_empty() || client.try_send(&ping(nonce)).await.is_err() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    assert!(
        state.connections().is_empty(),
        "the service let go rather than holding messages for a peer that left"
    );

    serving.abort();
}
