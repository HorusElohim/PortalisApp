//! Where the service keeps what it is given.
//!
//! ADR-0002: storage is a trait with exactly one wired engine. The coordination
//! node is a coordination plane, not a data plane — it holds small signed
//! objects it cannot read, so the worst a storage engine can do is lose one or
//! serve an old one, and both are things a client already detects.
//!
//! That is what makes the seam a trait rather than a hard-coded type, and what
//! makes a *second* engine unaffordable: two implementations for zero shipped
//! deployments is the parallel-variation trap. One strategy is chosen and
//! wired; the scale successor, when a node genuinely saturates, is `PostgreSQL`.
//!
//! - [`store`]: the machinery every endpoint's file shares.
//! - [`identity`], [`collections`], [`friends`], [`envelopes`], [`mailbox`],
//!   [`directory`]: one endpoint each, one file each, autonomous.
//! - [`embedded`]: the four of them together, as one engine.
//! - [`repositories`]: the engine wearing the service's own vocabulary.
//! - [`service`]: answering a peer that happens to be a service.
//!
//! The engine answers to a conformance suite it shares with the in-memory
//! double, which is the only way the seam means anything: implementations
//! nobody compares are separate behaviours.

pub mod collections;
pub mod directory;
pub mod embedded;
pub mod envelopes;
pub mod friends;
pub mod identity;
pub mod mailbox;
mod repositories;
pub mod service;
pub mod store;

use thiserror::Error;

/// Why a store could not answer.
///
/// Deliberately narrow: a service that leaks its engine's error taxonomy
/// upward makes callers that only work with one engine.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum StorageError {
    /// The write lost a compare-and-set, or would have overwritten history.
    #[error("that write conflicted with another")]
    Conflict,
    /// The handle is already claimed. Distinct from a conflict because the
    /// caller's answer differs: this one retries with another discriminator.
    #[error("that handle is already claimed")]
    HandleTaken,
    /// The device is already enrolled, possibly to somebody else. Nothing to
    /// retry — a device key is enrolled once.
    #[error("that device is already enrolled")]
    DeviceExists,
    /// A device's mailbox is full. Distinct from a conflict because the
    /// answer is different: a conflict means read again and retry, and this
    /// means the recipient has not collected anything for a long time.
    #[error("that device's mailbox is full: {held} of {limit} {unit}")]
    MailboxFull {
        held: usize,
        limit: usize,
        unit: &'static str,
    },
    /// A row that will not decode. Damage, or a store from another version.
    #[error("a stored row is malformed")]
    Malformed,
    #[error("the store is unavailable: {0}")]
    Unavailable(String),
}

// `redb`'s error family is wide and every one of these means the same thing
// here, so they collapse into one variant and reach it through `?` rather than
// a `map_err` at each of forty call sites.
macro_rules! unavailable_from {
    ($($error:ty),+ $(,)?) => {
        $(impl From<$error> for StorageError {
            fn from(error: $error) -> Self {
                Self::Unavailable(error.to_string())
            }
        })+
    };
}
impl StorageError {
    pub(crate) fn unavailable(error: impl std::fmt::Display) -> Self {
        Self::Unavailable(error.to_string())
    }
}

unavailable_from!(
    redb::DatabaseError,
    redb::TransactionError,
    redb::TableError,
    redb::StorageError,
    redb::CommitError,
);
