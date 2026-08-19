//! Iroh-free cryptographic and revision helpers used by the local backend.
//!
//! Extracted from the former `portalis-nexus-client` crate when the Iroh
//! transport was removed (BitTorrent-only product). Sealing, opening, and
//! chain verification never depended on Iroh — they are pure functions over
//! `portalis_nexus_protocol` types — so they move here rather than being
//! deleted with the transport that used to sit beside them.

#[path = "crypto_capsule.rs"]
mod capsule;
#[path = "crypto_keys.rs"]
mod keys;
#[path = "crypto_verify.rs"]
mod verify;

pub use capsule::{Capsule, CapsuleError};
pub use keys::{
    KeyError, Recipient, SealedFor, Sealing, generate_content_key, open_content_key,
    seal_content_key,
};
pub use verify::{
    ChainError, ChainState, ChainStore, ChainStoreError, Continuity, MemoryChainStore,
    verify as verify_revision,
};
