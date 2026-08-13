//! The reader, writer, and supervisor tasks behind one client handle.

use portalis_nexus_protocol::v1::{Envelope, ServerHello};
use portalis_nexus_protocol::{
    LENGTH_PREFIX_BYTES, MAX_OUTBOUND_QUEUE, decode_frame, format_id, frame_length, length_prefix,
};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::pending::PendingRequests;
use crate::protocol::ClientProtocol;
use crate::reconnect::ReconnectPolicy;
use crate::transport::Socket;
use crate::transport::handshake::handshake_with_retry;

/// State shared between the caller's handle and its supervisor task.
pub(crate) struct Shared {
    pub(crate) pending: PendingRequests,
    pub(crate) events: mpsc::Sender<Envelope>,
    pub(crate) protocol: ClientProtocol,
    pub(crate) request_timeout: Duration,
    pub(crate) shutdown: watch::Sender<bool>,
    pub(crate) server_identity: String,
    live: Mutex<Option<Live>>,
}

/// The currently connected socket's send side and negotiated hello.
struct Live {
    outbound: mpsc::Sender<Vec<u8>>,
    hello: ServerHello,
}

impl Shared {
    pub(crate) fn new(
        events: mpsc::Sender<Envelope>,
        request_timeout: Duration,
        server_identity: String,
    ) -> Self {
        Self {
            pending: PendingRequests::default(),
            events,
            protocol: ClientProtocol::default(),
            request_timeout,
            shutdown: watch::Sender::new(false),
            server_identity,
            live: Mutex::new(None),
        }
    }

    pub(crate) fn outbound(&self) -> Option<mpsc::Sender<Vec<u8>>> {
        self.live().as_ref().map(|live| live.outbound.clone())
    }

    pub(crate) fn hello(&self) -> Option<ServerHello> {
        self.live().as_ref().map(|live| live.hello.clone())
    }

    fn set_live(&self, live: Live) {
        *self.live() = Some(live);
    }

    fn clear_live(&self) {
        self.live().take();
    }

    fn live(&self) -> MutexGuard<'_, Option<Live>> {
        // Never held across an await, so poisoning would mean a bug elsewhere.
        self.live
            .lock()
            .expect("the live connection slot is never poisoned")
    }
}

/// The reader, writer, and queue belonging to one live socket.
pub(crate) struct Tasks {
    endpoint: crate::NexusEndpoint,
    connection: iroh::endpoint::Connection,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
    outbound: mpsc::Sender<Vec<u8>>,
}

/// Publishes a connection and starts its reader and writer tasks.
pub(crate) fn start_connection(
    shared: &Arc<Shared>,
    (socket, hello): (Socket, ServerHello),
) -> Tasks {
    let (outbound, inbox) = mpsc::channel(MAX_OUTBOUND_QUEUE);
    debug!(
        connection_id = %format_id(&hello.connection_id),
        server_identity = %shared.server_identity,
        "Nexus connection established"
    );
    shared.set_live(Live {
        outbound: outbound.clone(),
        hello,
    });

    Tasks {
        endpoint: socket.endpoint,
        connection: socket.connection,
        writer: tokio::spawn(write_socket(socket.send, inbox)),
        reader: tokio::spawn(read_socket(Arc::clone(shared), socket.receive)),
        outbound,
    }
}

/// Keeps one connection live until the handle shuts down or retries run out.
///
/// `shutdown` is subscribed by the caller before this task is spawned:
/// `watch::Sender::subscribe` marks the current value as seen, so a receiver
/// created in here would miss a shutdown requested before the task first ran.
pub(crate) async fn supervise(
    shared: Arc<Shared>,
    endpoint: crate::EndpointAddr,
    policy: ReconnectPolicy,
    first: Tasks,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut next = Some(first);
    loop {
        let tasks = if let Some(tasks) = next.take() {
            tasks
        } else {
            let attempt = tokio::select! {
                _ = shutdown.changed() => return,
                attempt = handshake_with_retry(endpoint.clone(), &policy, shared.request_timeout) => attempt,
            };
            match attempt {
                Ok(connection) => start_connection(&shared, connection),
                Err(error) => {
                    warn!(server_identity = %shared.server_identity, %error, "Nexus client stopped reconnecting");
                    return;
                }
            }
        };
        run_connection(&shared, tasks, &mut shutdown).await;
        if *shutdown.borrow() {
            return;
        }
        debug!(server_identity = %shared.server_identity, "Nexus connection ended; reconnecting");
    }
}

/// Runs one connection until its reader ends or the handle shuts down.
async fn run_connection(shared: &Arc<Shared>, tasks: Tasks, shutdown: &mut watch::Receiver<bool>) {
    let Tasks {
        endpoint,
        connection,
        mut reader,
        writer,
        outbound,
    } = tasks;

    // A completed `JoinHandle` panics when polled again, so remember which
    // branch ended the connection before awaiting the reader a second time.
    let reader_finished = tokio::select! {
        _ = &mut reader => true,
        _ = shutdown.changed() => false,
    };

    // Dropping every queue sender lets the writer finish its QUIC stream. The
    // reader remains alive long enough to observe the service finishing too.
    drop(outbound);
    shared.clear_live();
    // One Nexus session owns one stream. A closed stream is not reusable for a
    // later handshake, so close the QUIC connection before retrying rather
    // than letting Iroh return a stale connection for the same peer ID.
    connection.close(0_u32.into(), b"Nexus stream ended");
    writer.abort();
    if !reader_finished {
        reader.abort();
        let _ = reader.await;
    }
    let _ = writer.await;
    drop(endpoint);
    shared.pending.cancel_all();
}

/// Routes inbound envelopes to their waiting request, or to the event stream.
async fn read_socket(shared: Arc<Shared>, mut receive: iroh::endpoint::RecvStream) {
    loop {
        let mut prefix = [0_u8; LENGTH_PREFIX_BYTES];
        if receive.read_exact(&mut prefix).await.is_err() {
            return;
        }
        let length = match frame_length(prefix) {
            Ok(length) => length,
            Err(error) => {
                warn!(%error, "Nexus server announced an invalid frame");
                return;
            }
        };
        let mut frame = vec![0_u8; length];
        if receive.read_exact(&mut frame).await.is_err() {
            return;
        }
        let envelope = match decode_frame(&frame) {
            Ok(envelope) => envelope,
            Err(error) => {
                warn!(%error, "Nexus server sent an invalid frame");
                return;
            }
        };
        if let Some(event) = shared.pending.route(envelope) {
            if shared.events.try_send(event).is_err() {
                warn!("Nexus event stream is full or closed");
                return;
            }
        }
    }
}

/// Writes queued frames, then finishes the stream once the queue is dropped.
async fn write_socket(mut send: iroh::endpoint::SendStream, mut inbox: mpsc::Receiver<Vec<u8>>) {
    while let Some(frame) = inbox.recv().await {
        if send.write_all(&length_prefix(&frame)).await.is_err()
            || send.write_all(&frame).await.is_err()
        {
            warn!("Nexus stream write failed");
            return;
        }
    }
    let _ = send.finish();
}
