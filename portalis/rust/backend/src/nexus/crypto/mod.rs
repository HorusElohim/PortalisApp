//! Iroh-free cryptographic and revision helpers used by the local backend.
//!
//! Extracted from the former `portalis-nexus-client` crate when the Iroh
//! transport was removed (BitTorrent-only product). Sealing, opening, and
//! chain verification never depended on Iroh — they are pure functions over
//! `portalis_nexus_protocol` types — so they move here rather than being
//! deleted with the transport that used to sit beside them.

mod keys;
mod verify;

pub use keys::{
    KeyError, Recipient, SealedFor, Sealing, generate_content_key, open_content_key,
    seal_content_key,
};
pub use verify::{ChainError, ChainStore, Continuity, MemoryChainStore, verify as verify_revision};
pub(crate) use verify::{ChainState, position};
