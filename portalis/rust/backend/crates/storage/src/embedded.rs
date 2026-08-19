//! Four endpoints, four files, one engine.
//!
//! This is what a self-hoster runs: no server, no replica set, and a directory
//! of small files rather than a database to operate. It exists so that running
//! your own service is a decision about disk space rather than about whether
//! you know how to fail a replica set over.
//!
//! It is a composition rather than an implementation. Every rule lives in the
//! endpoint that owns it, and what is here is the mapping from the service's
//! vocabulary onto four autonomous parts. That arrangement follows from one
//! constraint: **a write cannot span two files.** So the seams sit where a
//! transaction is genuinely needed — a user and their first device, a head and
//! its snapshot, an item and its sequence — and nowhere else.
//!
//! What the split buys, in order of how much it matters:
//!
//! - **Writes proceed in parallel.** redb allows one write transaction per
//!   database. In one file a mailbox delivery waits behind a registration for
//!   no reason; in four it does not.
//! - **A smaller blast radius.** A file that will not open takes its endpoint
//!   down and no more.
//! - **Autonomy.** Each endpoint's tables, keys and rules are in one module
//!   with one file underneath, readable without knowing the rest exists.
//!
//! What it does not buy, despite the intuition: reading less. redb reads one
//! key from one table, and a query never touches a table it did not name —
//! whichever file that table lives in.

use std::path::Path;

use portalis_nexus_server_core::RepositoryError;

use crate::StorageError;
use crate::collections::Collections;
use crate::directory::Directory;
use crate::envelopes::Envelopes;
use crate::friends::Friends;
use crate::identity::Identity;
use crate::mailbox::{Limits, Mailbox};

/// The four endpoints, opened together.
#[derive(Debug)]
pub struct Embedded {
    identity: Identity,
    collections: Collections,
    friends: Friends,
    envelopes: Envelopes,
    mailbox: Mailbox,
    directory: Directory,
}

impl Embedded {
    /// Opens every endpoint's file under `directory`.
    ///
    /// A directory rather than a file, because there are four. `SPEC.md` §12
    /// says one location owns every path, and this is that location for the
    /// service.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when any of them cannot be opened.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::with_limits(directory, Limits::default())
    }

    /// Opens them with mailbox limits other than the standard ones.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when any of them cannot be opened.
    pub fn with_limits(directory: impl AsRef<Path>, limits: Limits) -> Result<Self, StorageError> {
        let directory = directory.as_ref();
        std::fs::create_dir_all(directory).map_err(StorageError::unavailable)?;
        Ok(Self {
            identity: Identity::open(directory.join("identity.redb"))?,
            collections: Collections::open(directory.join("collections.redb"))?,
            friends: Friends::open(directory.join("friends.redb"))?,
            envelopes: Envelopes::open(directory.join("envelopes.redb"))?,
            mailbox: Mailbox::with_limits(directory.join("mailbox.redb"), limits)?,
            directory: Directory::open(directory.join("directory.redb"))?,
        })
    }

    /// The same engine, backed by memory rather than files.
    ///
    /// This is what tests use, and it is the production code path — not a
    /// double that behaves almost like it. There is one implementation of
    /// every rule in here, so a test cannot pass against a store the service
    /// does not run. The parallel in-memory store this replaced had its own
    /// copy of every rule, and the two eventually disagreed about one.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when a store cannot be created.
    pub fn in_memory() -> Result<Self, StorageError> {
        Ok(Self {
            identity: Identity::in_memory()?,
            collections: Collections::in_memory()?,
            friends: Friends::in_memory()?,
            envelopes: Envelopes::in_memory()?,
            mailbox: Mailbox::in_memory(Limits::default())?,
            directory: Directory::in_memory()?,
        })
    }

    /// The identity endpoint.
    #[must_use]
    pub const fn identity(&self) -> &Identity {
        &self.identity
    }

    /// The collections endpoint.
    #[must_use]
    pub const fn collections(&self) -> &Collections {
        &self.collections
    }

    /// The friends endpoint.
    #[must_use]
    pub const fn friends(&self) -> &Friends {
        &self.friends
    }

    /// The sealed-key endpoint.
    #[must_use]
    pub const fn envelopes(&self) -> &Envelopes {
        &self.envelopes
    }

    /// The mailbox endpoint.
    #[must_use]
    pub const fn mailbox(&self) -> &Mailbox {
        &self.mailbox
    }

    /// The directory endpoint.
    #[must_use]
    pub const fn directory(&self) -> &Directory {
        &self.directory
    }
}

/// Maps a storage failure onto what `server-core` understands.
///
/// The service's rules are written against `RepositoryError`, so an engine
/// that invented its own vocabulary would make callers that only work with one
/// engine — which is the thing having two engines is supposed to avoid.
impl From<StorageError> for RepositoryError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Conflict => Self::VersionConflict,
            StorageError::HandleTaken => Self::HandleTaken,
            StorageError::DeviceExists => Self::DeviceExists,
            // A full mailbox is not a race, so it is not something to retry.
            error @ StorageError::MailboxFull { .. } => Self::Unavailable(error.to_string()),
            StorageError::Malformed => Self::Unavailable("a stored row is malformed".to_owned()),
            StorageError::Unavailable(reason) => Self::Unavailable(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same engine, without a file. Not a second implementation: the
    /// rules are the ones the durable engine runs, so a test cannot pass
    /// against behaviour the service does not have.
    #[test]
    fn the_engine_runs_in_memory_with_the_same_behaviour() {
        use portalis_nexus_server_core::{ShareRecord, ShareSnapshotRecord};

        let store = Embedded::in_memory().expect("opens");
        let share = |revision: u64| ShareRecord {
            share_id: [1; 16],
            owner: [2; 16],
            revision,
            snapshot_id: [3; 32],
            capsule: b"sealed".to_vec(),
            capsule_signature: vec![9; 64],
            created_at_unix_ns: 1,
            updated_at_unix_ns: revision,
        };
        let snapshot = |revision: u64| ShareSnapshotRecord {
            share_id: [1; 16],
            revision,
            snapshot_id: [3; 32],
            capsule: b"sealed".to_vec(),
            capsule_signature: vec![9; 64],
            created_at_unix_ns: revision,
        };

        store
            .collections()
            .save_publication(&share(1), &snapshot(1), None)
            .await
            .expect("publishes");
        assert_eq!(
            store.collections().find_share([1; 16]).expect("reads"),
            Some(share(1))
        );
        // The compare-and-set is the real one, not a simplification of it.
        assert!(matches!(
            store
                .collections()
                .save_publication(&share(2), &snapshot(2), None)
                .await,
            Err(StorageError::Conflict)
        ));

        store
            .mailbox()
            .deliver([4; 32], b"waiting")
            .expect("delivers");
        assert_eq!(store.mailbox().drain([4; 32]).expect("drains").len(), 1);

        // And it is genuinely separate storage: a second one shares nothing.
        assert_eq!(
            Embedded::in_memory()
                .expect("opens")
                .collections()
                .find_share([1; 16])
                .expect("reads"),
            None
        );
    }

    #[test]
    fn a_directory_that_cannot_be_made_is_reported() {
        // A file where the directory should be: the endpoints have nowhere to
        // live, and that is said rather than discovered one file at a time.
        let path = std::env::temp_dir().join(format!(
            "portalis-embedded-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::write(&path, b"not a directory").expect("writes a file");

        let refused = Embedded::open(&path).expect_err("must refuse");

        assert!(
            matches!(refused, StorageError::Unavailable(_)),
            "got {refused:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn every_failure_reaches_the_service_in_its_own_terms() {
        for (storage, expected) in [
            (StorageError::Conflict, "VersionConflict"),
            (StorageError::HandleTaken, "HandleTaken"),
            (StorageError::DeviceExists, "DeviceExists"),
            (
                StorageError::MailboxFull {
                    held: 2,
                    limit: 1,
                    unit: "items",
                },
                "Unavailable",
            ),
            (StorageError::Malformed, "Unavailable"),
            (
                StorageError::Unavailable("the disk is gone".to_owned()),
                "Unavailable",
            ),
        ] {
            let mapped = RepositoryError::from(storage);
            assert!(
                format!("{mapped:?}").starts_with(expected),
                "expected {expected}, got {mapped:?}"
            );
        }
    }
}
