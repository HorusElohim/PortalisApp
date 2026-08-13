use std::error::Error;

use iroh::{Endpoint, RelayMode};
use portalis_nexus_protocol::NEXUS_ALPN;
use portalis_nexus_server::{
    AppState, GRACEFUL_DRAIN_TIMEOUT, MongoStore, NexusStore, ServerConfig, Storage,
    load_node_secret,
};
use portalis_nexus_storage::embedded::Embedded;
use tokio::time::timeout;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

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
    .clear_discovery()
    .relay_mode(RelayMode::Disabled)
    .secret_key(node_secret)
    .alpns(vec![NEXUS_ALPN.to_vec()])
    .bind()
    .await?;
    // Clients sign against the name they dialled, so a deployment reached by
    // any other name refuses every signature. Logged for exactly that reason.
    let state = AppState::with_store(store).with_server_authority(&config.server_authority);
    // Ready only once the store is reachable and its indexes exist.
    state.mark_ready();

    info!(
        listen_addr = %config.listen_addr,
        node_id = %endpoint.node_id(),
        server_authority = %config.server_authority,
        store = state.store().kind(),
        "Portalis Nexus is ready"
    );
    tokio::select! {
        () = shutdown_signal() => {}
        () = serve_quic(&endpoint, state.clone()) => {
            warn!("Nexus QUIC endpoint stopped accepting connections");
        }
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
async fn serve_quic(endpoint: &Endpoint, state: AppState) {
    while let Some(incoming) = endpoint.accept().await {
        let state = state.clone();
        tokio::spawn(async move {
            let Ok(connection) = incoming.await else {
                return;
            };
            portalis_nexus_server::quic::serve(connection, state).await;
        });
    }
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
