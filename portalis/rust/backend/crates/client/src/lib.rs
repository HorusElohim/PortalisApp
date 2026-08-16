//! The portable Portalis Nexus client.
//!
//! This crate builds for macOS, iOS, Android, and Linux. It has no `MongoDB`,
//! `Axum`, or server-core dependency.
//!
//! Module layout:
//!
//! - [`error`]: deterministic protocol failures.
//! - [`protocol`]: client-side message construction and validation.
//! - [`pending`]: the bounded request/response correlation registry.
//! - [`reconnect`]: bounded exponential reconnect scheduling.
//! - [`signer`]: how a caller proves it owns a device key.
//! - [`config`]: tuning for one supervised connection.
//! - [`endpoint`]: authenticated direct-or-relayed QUIC connections.
//! - [`transport`]: the socket actor those rules drive.
//! - [`keys`]: sealing a content key to the devices a verified log allows.
//! - [`session`]: talking to a peer directly, with no service in the path.
//! - [`verify`]: whether a revision belongs after the one already held.

mod candidates;
mod capsule;
mod config;
mod endpoint;
mod error;
mod keys;
mod pending;
mod protocol;
mod reconnect;
mod session;
mod signer;
mod transport;
mod verify;

pub use candidates::{CandidateSource, PeerCandidate, merge_candidates};
pub use capsule::{Capsule, CapsuleError};
pub use config::{ClientConfig, DEFAULT_HANDSHAKE_TIMEOUT, DEFAULT_REQUEST_TIMEOUT};
pub use endpoint::{ConnectionPath, Discovery, NEXUS_ALPN, NexusEndpoint};
pub use error::ClientError;
pub use iroh::RelayMode;
pub use iroh::endpoint::{Connection, Incoming};
pub use iroh::{NodeAddr as EndpointAddr, NodeId as EndpointId};
pub use keys::{
    KeyError, Recipient, SealedFor, Sealing, generate_content_key, open_content_key,
    rotate_content_key, seal_content_key,
};
pub use pending::PendingRequests;
pub use protocol::{
    ClientProtocol, KeyEnvelopePage, validate_authenticated, validate_device_linked,
    validate_friend_event, validate_friend_list, validate_hello, validate_key_envelope_put,
    validate_key_envelopes, validate_peer_announced, validate_peer_lookup, validate_peer_withdrawn,
    validate_pong, validate_reply, validate_resolved, validate_share_access_granted,
    validate_share_access_revoked, validate_share_fetch, validate_share_handoff,
    validate_share_list, validate_share_published,
};
pub use reconnect::{ReconnectPolicy, ReconnectPolicyError};
pub use session::{
    KnownPeers, MAX_OBJECT_BYTES, Path as SecurityPath, PeerTrust, Request, Responder, Security,
    Session, SessionError,
};
pub use signer::DeviceSigner;
pub use transport::{NexusClient, TransportError, identity_of};
pub use verify::{
    Accepted, ChainError, ChainState, ChainStore, ChainStoreError, Continuity, MemoryChainStore,
    verify as verify_revision,
};
