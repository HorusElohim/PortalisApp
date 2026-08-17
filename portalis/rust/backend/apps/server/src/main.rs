use std::error::Error;

use iroh::{Endpoint, RelayMode};
use portalis_nexus_protocol::NEXUS_ALPN;
use portalis_nexus_server::{
    AppState, GRACEFUL_DRAIN_TIMEOUT, MongoStore, NexusStore, ServerConfig, Storage,
    load_node_secret,
};
use portalis_nexus_storage::embedded::Embedded;
use tokio::sync::watch;
use tokio::time::timeout;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

/// What to say when nobody has said otherwise.
///
/// The transport narrates its own bookkeeping at INFO — every node it learns
/// about, every address it learns for it, every time a connection changes
/// type. That is the right level for somebody debugging iroh and the wrong
/// one for somebody watching their own service, where it buries the three
/// lines that are actually about Portalis. `RUST_LOG` overrides all of it.
const DEFAULT_FILTER: &str = "info,iroh=warn,iroh_quinn=warn,iroh_relay=warn,\
                              iroh_base=warn,netwatch=warn,portmapper=warn";

/// Whether to write logs for a machine or for a person.
///
/// JSON is right where something collects it and wrong where somebody is
/// reading it out of a terminal. Neither is a good default for the other, so
/// the default is whichever the output is: a terminal gets text, a pipe or a
/// file gets JSON. `PORTALIS_NEXUS_LOG` settles it either way.
fn wants_text_logs() -> bool {
    match std::env::var("PORTALIS_NEXUS_LOG").as_deref() {
        Ok("text") => true,
        Ok("json") => false,
        _ => std::io::IsTerminal::is_terminal(&std::io::stdout()),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    if wants_text_logs() {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            // Colour belongs on a terminal and nowhere else: escape codes in
            // a captured log are noise to a reader and a wall to anything
            // trying to read a value back out of it.
            .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
            // The module a line came from is noise when every line worth
            // reading came from the same place.
            .with_target(false)
            .compact()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init();
    }

    let config = ServerConfig::from_environment()?;
    let node_secret = load_node_secret(&config)?;
    // One binary, and the engine is whichever was configured (D5). Logged
    // because a service running on storage nobody expected is otherwise
    // diagnosed by guesswork.
    let store = match config.storage()? {
        Storage::Embedded { data_dir } => {
            info!(path = %data_dir.display(), "opening the embedded store");
            NexusStore::embedded(Embedded::open(&data_dir)?)
        }
        Storage::Mongo { uri, database } => {
            info!(database = %database, "connecting to MongoDB");
            NexusStore::mongo(MongoStore::connect(&uri, &database).await?)
        }
    };
    let endpoint = match config.listen_addr {
        std::net::SocketAddr::V4(address) => Endpoint::builder().bind_addr_v4(address),
        std::net::SocketAddr::V6(address) => Endpoint::builder().bind_addr_v6(address),
    }
    // Published, so a person needs only the Node ID to reach this service:
    // pkarr records on n0's name server for anywhere, mDNS for the same
    // network. The records are signed by the node secret, so publishing them
    // gives away where this service is but not the ability to impersonate it.
    .discovery_n0()
    .discovery_local_network()
    // Relays are how a device behind a NAT reaches this service at all, and
    // how a direct path gets negotiated once it can. Traffic that ends up
    // relayed is still end-to-end encrypted; the relay carries bytes it
    // cannot read.
    .relay_mode(RelayMode::Default)
    .secret_key(node_secret)
    .alpns(vec![NEXUS_ALPN.to_vec()])
    .bind()
    .await?;
    // Iroh authenticates this Node ID during every handshake; it is therefore
    // the stable authority device signatures bind to, not an address hint.
    let server_identity = endpoint.node_id().to_string();
    let state = AppState::with_store(store).with_server_identity(&server_identity);
    // Ready only once the store is reachable and its indexes exist.
    state.mark_ready();

    // QUIC uses UDP while the orchestration endpoints use TCP, so the service
    // can deliberately publish one address and port without making a second
    // control-plane setting part of every deployment.
    let health_listener = tokio::net::TcpListener::bind(config.listen_addr).await?;

    info!(
        listen_addr = %config.listen_addr,
        node_id = %endpoint.node_id(),
        server_identity = %server_identity,
        store = state.store().kind(),
        "Portalis Nexus is ready"
    );

    let (shutdown, _) = watch::channel(false);
    let health_state = state.clone();
    let mut health = tokio::spawn(serve_health(
        health_listener,
        health_state,
        shutdown.subscribe(),
    ));
    let mut quic = tokio::spawn(serve_quic(
        endpoint.clone(),
        state.clone(),
        shutdown.subscribe(),
    ));

    tokio::select! {
        () = shutdown_signal() => {}
        result = &mut health => {
            match result {
                Ok(Ok(())) => warn!("Nexus health listener stopped"),
                Ok(Err(error)) => warn!(%error, "Nexus health listener failed"),
                Err(error) => warn!(%error, "Nexus health task failed"),
            }
        }
        result = &mut quic => {
            if let Err(error) = result {
                warn!(%error, "Nexus QUIC task failed");
            }
            warn!("Nexus QUIC endpoint stopped accepting connections");
        }
    }

    // Tell the HTTP listener to finish its in-flight responses and the QUIC
    // accept loop to stop admitting work before draining the live sessions.
    let _ = shutdown.send(true);
    if timeout(GRACEFUL_DRAIN_TIMEOUT, &mut health).await.is_err() {
        health.abort();
    }
    if timeout(GRACEFUL_DRAIN_TIMEOUT, &mut quic).await.is_err() {
        quic.abort();
    }

    // Upgraded sockets outlive the HTTP serve loop, so drain them explicitly.
    info!("draining live Nexus sockets");
    if timeout(GRACEFUL_DRAIN_TIMEOUT, state.shutdown().drain())
        .await
        .is_err()
    {
        warn!(
            timeout_secs = GRACEFUL_DRAIN_TIMEOUT.as_secs(),
            "drain timed out; closing remaining sockets"
        );
    }
    endpoint.close().await;
    Ok(())
}

/// Accepts authenticated QUIC connections until process shutdown. Each
/// connection owns its service task, matching the concurrency the old HTTP
/// server provided without putting transport state in application handlers.
async fn serve_quic(endpoint: Endpoint, state: AppState, mut shutdown: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { return };
                let state = state.clone();
                let endpoint = endpoint.clone();
                tokio::spawn(async move {
                    let Ok(connection) = incoming.await else {
                        return;
                    };
                    let observed_ip = portalis_nexus_server::quic::direct_peer_ip(&endpoint, &connection);
                    portalis_nexus_server::quic::serve(connection, state, observed_ip).await;
                });
            }
        }
    }
}

/// Serves orchestration probes until the process begins its graceful drain.
async fn serve_health(
    listener: tokio::net::TcpListener,
    state: AppState,
    mut shutdown: watch::Receiver<bool>,
) -> std::io::Result<()> {
    axum::serve(listener, portalis_nexus_server::app(&state))
        .with_graceful_shutdown(async move {
            if !*shutdown.borrow() {
                let _ = shutdown.changed().await;
            }
        })
        .await
}

/// Waits for the first shutdown signal this platform can deliver.
///
/// Container runtimes and init systems ask a process to stop with `SIGTERM`;
/// only a terminal sends `SIGINT`. Listening for the interrupt alone means an
/// orchestrator's polite stop is ignored until its grace period expires and
/// the process is killed, severing live sockets rather than draining them.
///
/// A handler that cannot be installed waits forever instead of returning:
/// reporting "shut down now" because the listener failed would turn a missing
/// signal handler into an immediate shutdown.
async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for an interrupt");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};

        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                terminate.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for a termination request");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => info!("interrupted; shutting down"),
        () = terminate => info!("termination requested; shutting down"),
    }
}
