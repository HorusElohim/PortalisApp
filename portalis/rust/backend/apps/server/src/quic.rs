//! The same service, over QUIC instead of a WebSocket.
//!
//! Nothing about what the service *does* is here. Connections carry the same
//! framed envelopes, answered by the same [`dispatch`], with the same session
//! state — this module is the pipe, and it exists so the pipe can be replaced
//! without the rules moving.
//!
//! That is also why it is written before the WebSocket one is deleted. Two
//! transports over one dispatch proves the seam is real; deleting the old one
//! first would have proved only that it compiled.
//!
//! The shape matches the WebSocket loop deliberately: one long-lived
//! bidirectional stream per connection, a bounded read loop, and one writer
//! task owning the send half. A peer that stops reading fills a queue of at
//! most [`MAX_OUTBOUND_QUEUE`] entries and loses its connection rather than
//! growing the server's memory.

use std::time::{SystemTime, UNIX_EPOCH};

use iroh::endpoint::{Connection, RecvStream, SendStream};
use portalis_nexus_protocol::{
    MAX_FRAME_BYTES, MAX_OUTBOUND_QUEUE, decode_frame, format_id, payload_name,
};
use tokio::sync::{mpsc, watch};
use tracing::{Instrument, debug, info_span, warn};

use crate::handlers::{departed, dispatch};
use crate::messages::{binary_frame, hello_envelope, hello_payload};
use crate::session::Session;
use crate::state::AppState;

/// Runs one QUIC connection until the peer leaves or the server drains.
///
/// The caller has already accepted the connection, which is where a peer's
/// identity is established — this only carries what it says afterwards.
pub async fn serve(connection: Connection, state: AppState) {
    let issued_at = now_unix_ns();
    let hello = hello_payload(state.protocol_policy(), issued_at);
    let span = info_span!(
        "nexus_quic",
        connection_id = %format_id(&hello.connection_id)
    );

    async move {
        // The service opens the stream, so its greeting is the first thing on
        // it and a client never has to guess whether one is coming.
        let Ok((send, receive)) = connection.open_bi().await else {
            debug!("peer left before a stream was opened");
            return;
        };

        let mut draining = state.shutdown().register();
        let (outbound, inbox) = mpsc::channel(MAX_OUTBOUND_QUEUE);
        let writer = tokio::spawn(write_outbound(send, inbox));

        let mut session = Session::new(&hello);
        // Registered before the greeting, so an event caused by this
        // connection's own first command can already reach it.
        state
            .connections()
            .register(session.connection_id(), outbound.clone());

        // The writer has only just started and the queue is empty, so this
        // cannot fail. If the peer has already gone, the read loop below is
        // what notices, and it is the same notice it gives at any other time.
        let _ = outbound
            .send(binary_frame(&hello_envelope(hello, issued_at)))
            .await;
        debug!("connection established");
        read_inbound(receive, &outbound, &mut draining, &mut session, &state).await;

        departed(&session, &state, now_unix_ns()).await;
        drop(outbound);
        let _ = writer.await;
        debug!("connection closed");
    }
    .instrument(span)
    .await;
}

/// Reads until the peer leaves, the queue fills, or the server starts draining.
async fn read_inbound(
    mut receive: RecvStream,
    outbound: &mpsc::Sender<Vec<u8>>,
    draining: &mut watch::Receiver<bool>,
    session: &mut Session,
    state: &AppState,
) {
    loop {
        let frame = tokio::select! {
            _ = draining.changed() => {
                debug!("server is draining");
                return;
            }
            frame = read_frame(&mut receive) => frame,
        };
        let Some(frame) = frame else {
            return;
        };

        let Ok(request) = decode_frame(&frame) else {
            // A frame this service cannot parse ends the connection: there is
            // no message id to answer against, so there is nothing to say.
            warn!("undecodable frame");
            return;
        };
        let operation = payload_name(request.payload.as_ref());
        debug!(operation, message_id = %format_id(&request.message_id), "request received");

        let response = dispatch(session, state, &request, now_unix_ns()).await;
        if outbound.send(binary_frame(&response)).await.is_err() {
            debug!("outbound queue closed");
            return;
        }
    }
}

/// One length-prefixed frame, or `None` when the peer has finished.
async fn read_frame(receive: &mut RecvStream) -> Option<Vec<u8>> {
    let mut length = [0_u8; 4];
    receive.read_exact(&mut length).await.ok()?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME_BYTES {
        // Refused before allocating for it, which is the point of the bound.
        warn!(length, "frame over the limit");
        return None;
    }
    let mut frame = vec![0_u8; length];
    receive.read_exact(&mut frame).await.ok()?;
    Some(frame)
}

/// Owns the send half, so every outbound message crosses one bounded queue.
async fn write_outbound(mut send: SendStream, mut inbox: mpsc::Receiver<Vec<u8>>) {
    while let Some(frame) = inbox.recv().await {
        let length = u32::try_from(frame.len()).unwrap_or(u32::MAX);
        if send.write_all(&length.to_be_bytes()).await.is_err()
            || send.write_all(&frame).await.is_err()
        {
            debug!("peer stopped reading");
            return;
        }
    }
    let _ = send.finish();
}

fn now_unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
        })
}
