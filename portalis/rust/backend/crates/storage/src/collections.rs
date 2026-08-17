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

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const ADA: UserId = [1; 16];
    const GRACE: UserId = [2; 16];
    const SHARE: ShareId = [3; 16];

    fn share(revision: u64, capsule: &[u8]) -> ShareRecord {
        ShareRecord {
            share_id: SHARE,
            owner: ADA,
            revision,
            snapshot_id: [u8::try_from(revision % 256).unwrap_or(0); 32],
            capsule: capsule.to_vec(),
            capsule_signature: vec![9; 64],
            created_at_unix_ns: 1,
            updated_at_unix_ns: revision,
        }
    }
    fn snapshot(revision: u64, capsule: &[u8]) -> ShareSnapshotRecord {
        ShareSnapshotRecord {
            share_id: SHARE,
            revision,
            snapshot_id: [u8::try_from(revision % 256).unwrap_or(0); 32],
            capsule: capsule.to_vec(),
            capsule_signature: vec![9; 64],
            created_at_unix_ns: revision,
        }
    }
    /// A head and its snapshot together, and a head that moved underneath is
    /// refused rather than overwritten.
    #[test]
    fn publishing_is_a_compare_and_set_over_immutable_history() {
        let scratch = Scratch::new("publish");
        let store = scratch.open();

        store
            .save_publication(&share(1, b"one"), &snapshot(1, b"one"), None)
            .expect("the first publication creates it");
        assert_eq!(
            store.find_share(SHARE).expect("reads"),
            Some(share(1, b"one"))
        );

        // Expecting no share when one exists: someone else got there first.
        assert!(matches!(
            store.save_publication(&share(1, b"other"), &snapshot(1, b"other"), None),
            Err(StorageError::Conflict)
        ));

        store
            .save_publication(&share(2, b"two"), &snapshot(2, b"two"), Some(1))
            .expect("advances");
        assert_eq!(
            store.find_share(SHARE).expect("reads").map(|s| s.revision),
            Some(2)
        );

        // History is immutable: revision 1 is still what it was.
        assert_eq!(
            store.find_snapshot(SHARE, 1).expect("reads"),
            Some(snapshot(1, b"one"))
        );
        assert!(matches!(
            store.save_publication(&share(1, b"rewritten"), &snapshot(1, b"rewritten"), Some(2)),
            Err(StorageError::Conflict)
        ));

        // A stale expectation loses.
        assert!(matches!(
            store.save_publication(&share(3, b"three"), &snapshot(3, b"three"), Some(1)),
            Err(StorageError::Conflict)
        ));
    }
    #[test]
    fn a_collection_nobody_published_is_absent_rather_than_an_error() {
        let scratch = Scratch::new("absent");
        let store = scratch.open();

        assert_eq!(store.find_share(SHARE).expect("reads"), None);
        assert_eq!(store.find_snapshot(SHARE, 1).expect("reads"), None);
        assert!(!store.has_access(SHARE, ADA).expect("reads"));
        assert!(store.list_members(SHARE).expect("reads").is_empty());
    }
    #[test]
    fn membership_is_granted_revoked_and_listed_per_collection() {
        let scratch = Scratch::new("membership");
        let store = scratch.open();
        let other: ShareId = [4; 16];

        store.grant_access(SHARE, ADA, 10).expect("grants");
        store.grant_access(SHARE, GRACE, 11).expect("grants");
        store.grant_access(other, GRACE, 12).expect("grants");

        assert!(store.has_access(SHARE, ADA).expect("reads"));
        let mut members = store.list_members(SHARE).expect("reads");
        members.sort_unstable();
        assert_eq!(members, vec![ADA, GRACE]);
        assert_eq!(
            store.list_members(other).expect("reads"),
            vec![GRACE],
            "one collection's membership is not another's"
        );

        store.revoke_access(SHARE, GRACE).expect("revokes");
        assert!(!store.has_access(SHARE, GRACE).expect("reads"));
        assert!(store.has_access(other, GRACE).expect("reads"));
        // Revoking twice is the same statement.
        store.revoke_access(SHARE, GRACE).expect("revokes again");
    }
    /// Big-endian revision keys, so revision 256 sorts after 255 rather than
    /// between 25 and 26.
    #[test]
    fn history_is_keyed_so_it_reads_back_in_order() {
        let scratch = Scratch::new("order");
        let store = scratch.open();

        let mut previous = None;
        for revision in [1_u64, 2, 255, 256, 257] {
            store
                .save_publication(
                    &share(revision, b"content"),
                    &snapshot(revision, b"content"),
                    previous,
                )
                .expect("publishes");
            previous = Some(revision);
        }

        for revision in [1_u64, 255, 256, 257] {
            assert!(
                store
                    .find_snapshot(SHARE, revision)
                    .expect("reads")
                    .is_some(),
                "revision {revision} is still there"
            );
        }
        assert_eq!(store.find_snapshot(SHARE, 3).expect("reads"), None);
    }
}
