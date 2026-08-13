//! Where the service keeps what it is given.
//!
//! Decision D5: storage is a trait with two engines. A self-hoster wants one
//! file and no operational surface; an operator already running `MongoDB` wants
//! `MongoDB`. Neither is load-bearing for correctness — the service holds signed
//! objects it cannot read, so the worst a storage engine can do is lose them
//! or serve an old one, and both are things a client already detects.
//!
//! That is what makes two engines affordable. If the store were the source of
//! truth, having two would mean two chances to be subtly wrong. Because the
//! chain is the source of truth, a store is a cache with durability, and the
//! difference between engines is operational rather than semantic.
//!
//! - [`embedded`]: one file, no server, no replica set.
//! - [`directory`]: device logs, stored and served.
//! - [`service`]: answering a peer that happens to be a service.
//! - [`mailbox`]: what a device missed while it was asleep.
//!
//! The `MongoDB` engine currently lives in `apps/server` and moves here as the
//! service is rewritten; both then answer to the same conformance suite, which
//! is the only way "either engine" means anything.

pub mod directory;
pub mod embedded;
pub mod mailbox;
pub mod service;

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
unavailable_from!(
    redb::DatabaseError,
    redb::TransactionError,
    redb::TableError,
    redb::StorageError,
    redb::CommitError,
);
