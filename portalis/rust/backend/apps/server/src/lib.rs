//! The Portalis Nexus control-plane server.
//!
//! Module layout:
//!
//! - [`config`]: process configuration read at startup.
//! - [`state`]: state shared by every handler.
//! - [`shutdown`]: graceful draining of live Nexus connections.
//! - [`health`]: liveness and readiness endpoints.
//! - [`connections`]: where to reach a live connection.
//! - [`environment`]: the production clock and random source.
//! - [`identity`]: the concrete services this server runs.
//! - [`store`]: the store behind them, in memory or durable.
//! - [`messages`]: envelope construction and inbound dispatch decisions.
//! - [`session`]: per-connection authentication state.
//! - [`handlers`]: domain commands, one module per subsystem.
//! - [`quic`]: the QUIC transport that carries those decisions.

use axum::Router;
use tower_http::trace::TraceLayer;

mod config;
mod connections;
mod environment;
mod handlers;
mod health;
mod identity;
mod messages;
mod node;
pub mod quic;
mod session;
mod shutdown;
mod state;
mod store;

pub use config::{DEFAULT_DATABASE, DEFAULT_LISTEN_ADDR, MissingStorage, ServerConfig, Storage};
pub use connections::Connections;
pub use environment::{OsRandom, SystemClock, now_unix_ns};
pub use handlers::{departed, dispatch};
pub use health::SERVICE_NAME;
pub use identity::{
    DefaultStore, NexusFriends, NexusIdentities, NexusShares, friends, identities, shares,
};
pub use messages::{
    authenticated_reply, binary_frame, hello_envelope, hello_payload, presence_event,
    protocol_error, reply_with, response_for, server_hello,
};
pub use node::{NodeSecretError, load_node_secret};
pub use portalis_nexus_storage::mongo::MongoStore;
pub use session::Session;
pub use shutdown::{GRACEFUL_DRAIN_TIMEOUT, Shutdown};
pub use state::AppState;
pub use store::NexusStore;

pub fn app(state: &AppState) -> Router {
    Router::new()
        .merge(health::routes())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone())
}
