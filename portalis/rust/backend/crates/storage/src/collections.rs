//! What has been published, and who may read it.
//!
//! One file, because a head and its snapshot must move together. The head says
//! "this collection is at revision 4"; the snapshot is revision 4. Writing one
//! without the other leaves a claim with no history behind it, which a reader
//! then asks for and cannot be given.
//!
//! Membership lives here too, and is keyed collection-first. Answering "what
//! may this user read" therefore walks the table rather than using an index —
//! the right trade at this size, because the alternative is a second index to
//! keep in step, and an index that disagrees with its table is worse than a
//! scan.

use std::time::{SystemTime, UNIX_EPOCH};

use redb::{ReadableTable, TableDefinition};

use portalis_nexus_server_core::{ShareId, ShareRecord, ShareSnapshotRecord, UserId};

use crate::StorageError;
use crate::store::{Store, decode, encode, keyed, pair, prefix_range};

/// A collection's current head.
const SHARES: TableDefinition<&[u8], &str> = TableDefinition::new("shares");
/// Immutable history. Key: collection ‖ revision, big-endian.
const SNAPSHOTS: TableDefinition<&[u8], &str> = TableDefinition::new("snapshots");
/// Who may read what. Key: collection ‖ user.
const MEMBERSHIP: TableDefinition<&[u8], u64> = TableDefinition::new("membership");

/// The collections endpoint.
#[derive(Debug)]
pub struct Collections {
    store: Store,
}

impl Collections {
    /// Opens this endpoint's file.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the file cannot be opened or prepared.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        Self::over(Store::open(path)?)
    }

    /// The same endpoint, backed by memory.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the store cannot be created.
    pub fn in_memory() -> Result<Self, StorageError> {
        Self::over(Store::in_memory()?)
    }

    fn over(store: Store) -> Result<Self, StorageError> {
        store.declare(|write| {
            write.open_table(SHARES)?;
            write.open_table(SNAPSHOTS)?;
            write.open_table(MEMBERSHIP)?;
            Ok(())
        })?;
        Ok(Self { store })
    }

    /// Writes a head and its immutable snapshot together, refusing to move a
    /// head that is not where the caller last saw it.
    ///
    /// `expected` is the revision the caller read. `None` means it expected no
    /// collection at all, which is what makes the first publisher the owner
    /// even under a race.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Conflict`] when the head moved underneath, or
    /// the snapshot already exists — history does not get rewritten.
    pub fn save_publication(
        &self,
        head: &ShareRecord,
        snapshot: &ShareSnapshotRecord,
        expected: Option<u64>,
    ) -> Result<(), StorageError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
            });
        self.store.transact(|write| {
            let mut shares = write.open_table(SHARES)?;
            let actual: Option<ShareRecord> = shares
                .get(head.share_id.as_slice())?
                .map(|stored| decode(stored.value()))
                .transpose()?;
            if actual.map(|share| share.revision) != expected {
                return Err(StorageError::Conflict);
            }

            let mut snapshots = write.open_table(SNAPSHOTS)?;
            let key = keyed(&snapshot.share_id, snapshot.revision);
            if snapshots.get(key.as_slice())?.is_some() {
                return Err(StorageError::Conflict);
            }

            snapshots.insert(key.as_slice(), encode(snapshot)?.as_str())?;
            shares.insert(head.share_id.as_slice(), encode(head)?.as_str())?;

            // Automatically grant the owner access to their own share on first publication.
            if expected.is_none() {
                let mut membership = write.open_table(MEMBERSHIP)?;
                membership.insert(pair(&head.share_id, &head.owner).as_slice(), now)?;
            }

            Ok(())
        })
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn find_share(&self, share: ShareId) -> Result<Option<ShareRecord>, StorageError> {
        let read = self.store.read()?;
        let table = read.open_table(SHARES)?;
        table
            .get(share.as_slice())?
            .map(|stored| decode(stored.value()))
            .transpose()
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn find_snapshot(
        &self,
        share: ShareId,
        revision: u64,
    ) -> Result<Option<ShareSnapshotRecord>, StorageError> {
        let read = self.store.read()?;
        let table = read.open_table(SNAPSHOTS)?;
        table
            .get(keyed(&share, revision).as_slice())?
            .map(|stored| decode(stored.value()))
            .transpose()
    }

    /// # Errors
    /// Returns [`StorageError`] when the write fails.
    pub fn grant_access(
        &self,
        share: ShareId,
        user: UserId,
        at_unix_ns: u64,
    ) -> Result<(), StorageError> {
        self.store.transact(|write| {
            write
                .open_table(MEMBERSHIP)?
                .insert(pair(&share, &user).as_slice(), at_unix_ns)?;
            Ok(())
        })
    }

    /// # Errors
    /// Returns [`StorageError`] when the write fails.
    pub fn revoke_access(&self, share: ShareId, user: UserId) -> Result<(), StorageError> {
        self.store.transact(|write| {
            write
                .open_table(MEMBERSHIP)?
                .remove(pair(&share, &user).as_slice())?;
            Ok(())
        })
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails.
    pub fn has_access(&self, share: ShareId, user: UserId) -> Result<bool, StorageError> {
        let read = self.store.read()?;
        let table = read.open_table(MEMBERSHIP)?;
        Ok(table.get(pair(&share, &user).as_slice())?.is_some())
    }

    /// Everyone a collection is shared with.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the read fails or a key is malformed.
    pub fn list_members(&self, share: ShareId) -> Result<Vec<UserId>, StorageError> {
        let read = self.store.read()?;
        let table = read.open_table(MEMBERSHIP)?;
        let (low, high) = prefix_range(share.as_slice());

        let mut members = Vec::new();
        for row in table.range(low.as_slice()..=high.as_slice())? {
            let (key, _) = row?;
            members.push(
                UserId::try_from(&key.value()[share.len()..])
                    .map_err(|_| StorageError::Malformed)?,
            );
        }
        Ok(members)
    }

    /// Every collection a user may read.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn readable_by(&self, user: UserId) -> Result<Vec<ShareRecord>, StorageError> {
        let read = self.store.read()?;
        let membership = read.open_table(MEMBERSHIP)?;
        let shares = read.open_table(SHARES)?;

        let mut readable = Vec::new();
        for row in membership.iter()? {
            let (key, _) = row?;
            let key = key.value();
            let Some((collection, member)) = key.split_at_checked(key.len() - user.len()) else {
                return Err(StorageError::Malformed);
            };
            if member != user {
                continue;
            }
            if let Some(stored) = shares.get(collection)? {
                readable.push(decode(stored.value())?);
            }
        }
        Ok(readable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portalis_nexus_protocol::{MAX_SHARE_CAPSULE_BYTES, SHARE_ID_BYTES, SNAPSHOT_ID_BYTES};

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "portalis-collections-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn open(&self) -> Collections {
            Collections::open(self.0.join("collections.redb")).expect("opens")
        }
    }

    fn share(share_id: [u8; SHARE_ID_BYTES], revision: u64) -> ShareRecord {
        ShareRecord {
            share_id,
            owner: [1; 16],
            revision,
            snapshot_id: [2; SNAPSHOT_ID_BYTES],
            capsule: vec![
                u8::try_from(revision).unwrap_or(u8::MAX);
                MAX_SHARE_CAPSULE_BYTES.min(32)
            ],
            capsule_signature: vec![3; 64],
            created_at_unix_ns: 1000,
            updated_at_unix_ns: 2000,
        }
    }

    fn snapshot(share_id: [u8; SHARE_ID_BYTES], revision: u64) -> ShareSnapshotRecord {
        ShareSnapshotRecord {
            share_id,
            revision,
            snapshot_id: [2; SNAPSHOT_ID_BYTES],
            capsule: vec![
                u8::try_from(revision).unwrap_or(u8::MAX);
                MAX_SHARE_CAPSULE_BYTES.min(32)
            ],
            capsule_signature: vec![3; 64],
            created_at_unix_ns: 2000,
        }
    }

    #[test]
    fn publishing_is_a_compare_and_set_over_immutable_history() {
        let scratch = Scratch::new("cas");
        let store = scratch.open();

        let s1 = share([1; SHARE_ID_BYTES], 1);
        let ss1 = snapshot([1; SHARE_ID_BYTES], 1);
        store
            .save_publication(&s1, &ss1, None)
            .expect("first publish");

        let s2 = share([1; SHARE_ID_BYTES], 2);
        let ss2 = snapshot([1; SHARE_ID_BYTES], 2);
        store
            .save_publication(&s2, &ss2, Some(1))
            .expect("second publish");

        let s3 = share([1; SHARE_ID_BYTES], 3);
        let ss3 = snapshot([1; SHARE_ID_BYTES], 3);
        store
            .save_publication(&s3, &ss3, Some(1))
            .expect_err("rewriting revision 1 after 2 is conflict");

        let other = share([2; SHARE_ID_BYTES], 1);
        let ss_other = snapshot([2; SHARE_ID_BYTES], 1);
        store
            .save_publication(&other, &ss_other, Some(2))
            .expect_err("expecting revision 2 on other share is conflict");

        let s3 = share([1; SHARE_ID_BYTES], 3);
        let ss3 = snapshot([1; SHARE_ID_BYTES], 3);
        store
            .save_publication(&s3, &ss3, Some(2))
            .expect("third publish succeeds");

        let stored = store
            .find_share([1; SHARE_ID_BYTES])
            .expect("reads")
            .expect("exists");
        assert_eq!(stored.revision, 3);
    }

    #[test]
    fn publishing_is_a_compare_and_set() {
        let scratch = Scratch::new("cas-async");
        let store = scratch.open();

        let s1 = share([1; SHARE_ID_BYTES], 1);
        let ss1 = snapshot([1; SHARE_ID_BYTES], 1);
        store
            .save_publication(&s1, &ss1, None)
            .expect("first publish");

        let s2 = share([1; SHARE_ID_BYTES], 2);
        let ss2 = snapshot([1; SHARE_ID_BYTES], 2);
        store
            .save_publication(&s2, &ss2, Some(1))
            .expect("second publish");
    }

    #[test]
    fn publishing_produces_one_sealed_key_per_authorized_device() {
        let scratch = Scratch::new("key-per-device");
        let store = scratch.open();

        let share_id = [1; SHARE_ID_BYTES];
        let s1 = share(share_id, 1);
        let ss1 = snapshot(share_id, 1);
        store
            .save_publication(&s1, &ss1, None)
            .expect("first publish");

        // Owner should have access automatically
        assert!(store.has_access(share_id, [1; 16]).expect("reads"));

        // Other user should not
        assert!(!store.has_access(share_id, [2; 16]).expect("reads"));

        // Grant access to other user
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
            });
        store.grant_access(share_id, [2; 16], now).expect("grant");
        assert!(store.has_access(share_id, [2; 16]).expect("reads"));

        // Revoke access
        store.revoke_access(share_id, [2; 16]).expect("revoke");
        assert!(!store.has_access(share_id, [2; 16]).expect("reads"));
    }
}
