//! Establishing one authenticated-ready socket.

use std::time::Duration;

use portalis_nexus_protocol::v1::{Envelope, ServerHello};
use portalis_nexus_protocol::{LENGTH_PREFIX_BYTES, decode_frame, frame_length};
use tokio::time::{sleep, timeout};
use tracing::debug;
use uuid::Uuid;

use crate::protocol::validate_hello;
use crate::reconnect::ReconnectPolicy;
use crate::transport::Socket;
use crate::transport::error::TransportError;
use crate::{EndpointAddr, NEXUS_ALPN, NexusEndpoint};

/// Connects once and validates the server's hello, within `limit`.
///
/// The bound matters: a peer that accepts the TCP connection but never finishes
/// the upgrade would otherwise stall the caller, or the supervisor, forever.
pub(crate) async fn handshake(
    local: &NexusEndpoint,
    endpoint: EndpointAddr,
    limit: Duration,
) -> Result<(Socket, ServerHello), TransportError> {
    timeout(limit, connect_and_greet(local, endpoint))
        .await
        .map_err(|_| TransportError::HandshakeTimeout(limit))?
}

/// ALPN is negotiated with QUIC before the service opens its greeting stream.
async fn connect_and_greet(
    local: &NexusEndpoint,
    endpoint: EndpointAddr,
) -> Result<(Socket, ServerHello), TransportError> {
    let connection = local.connect(endpoint, NEXUS_ALPN).await?;
    let (send, mut receive) = connection.accept_bi().await?;
    let hello = validate_hello(receive_envelope(&mut receive).await?)?;

    Ok((
        Socket {
            endpoint: local.clone(),
            connection,
            send,
            receive,
        },
        hello,
    ))
}

/// Connects under a bounded exponential retry policy.
pub(crate) async fn handshake_with_retry(
    endpoint: EndpointAddr,
    policy: &ReconnectPolicy,
    limit: Duration,
) -> Result<(Socket, ServerHello), TransportError> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        let local = crate::transport::bind_endpoint().await?;
        match handshake(&local, endpoint.clone(), limit).await {
            Ok(connection) => {
                debug!(attempts, "Nexus handshake succeeded");
                return Ok(connection);
            }
            Err(error) if !policy.can_retry_after(attempts) => {
                return Err(TransportError::ReconnectExhausted {
                    attempts,
                    source: Box::new(error),
                });
            }
            Err(error) => {
                let delay = policy.delay_after_failure(attempts, random_entropy());
                debug!(attempts, delay_ms = delay.as_millis(), %error, "Nexus handshake failed; retrying");
                sleep(delay).await;
            }
        }
    }
}

async fn receive_envelope(
    receive: &mut iroh::endpoint::RecvStream,
) -> Result<Envelope, TransportError> {
    let mut prefix = [0_u8; LENGTH_PREFIX_BYTES];
    receive
        .read_exact(&mut prefix)
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
    let length = frame_length(prefix)?;
    let mut frame = vec![0_u8; length];
    receive
        .read_exact(&mut frame)
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
    Ok(decode_frame(&frame)?)
}

/// Draws jitter entropy without adding a random-number dependency.
fn random_entropy() -> u64 {
    let bytes = *Uuid::new_v4().as_bytes();
    u64::from_le_bytes(
        bytes[..8]
            .try_into()
            .expect("a UUID always contains eight leading bytes"),
    )
}
