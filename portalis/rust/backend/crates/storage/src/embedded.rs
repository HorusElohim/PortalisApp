//! One file, no server, no replica set.
//!
//! This engine exists so a person can run their own service. The `MongoDB`
//! engine needs a replica set for transactions, which means three processes
//! and a failover story before anyone has shared a photograph — a reasonable
//! ask of an operator and an absurd one of somebody running this on a machine
//! in their house.
//!
//! `redb` gives what the service actually needs from a database: one file, and
//! writes that either happen or do not. Registration inserts a user and their
//! first device together, and publication writes a head and an immutable
//! snapshot together; both are one transaction here rather than a distributed
//! one.
//!
//! Records are stored as JSON. That is a deliberate difference from the
//! canonical formats in `protocol`, which are hand-written because two
//! implementations must agree on them byte for byte. Nothing here crosses a
//! wire or is signed: it is one process's private copy of objects it cannot
//! read, so a derived encoding costs nothing and a hand-written one would be
//! a thousand lines of opportunity to be wrong. Being legible to whoever is
//! self-hosting is a small bonus.

use std::path::Path;

use redb::{Database, ReadableTable, TableDefinition};
use serde::Serialize;
use serde::de::DeserializeOwned;

use portalis_nexus_server_core::{
    DeviceId, DeviceRecord, RepositoryError, ShareId, ShareRecord, ShareSnapshotRecord, UserId,
    UserRecord,
};

use crate::StorageError;

/// Users, by identifier.
const USERS: TableDefinition<&[u8], &str> = TableDefinition::new("users");
/// Handle claims: the indexed form and discriminator to a user.
const HANDLES: TableDefinition<&str, &[u8]> = TableDefinition::new("handles");
/// Devices, by device identifier.
const DEVICES: TableDefinition<&[u8], &str> = TableDefinition::new("devices");
/// Which devices belong to a user. Key: user ‖ device.
const USER_DEVICES: TableDefinition<&[u8], ()> = TableDefinition::new("user_devices");
/// A collection's current head, by collection.
const SHARES: TableDefinition<&[u8], &str> = TableDefinition::new("shares");
/// Immutable history. Key: collection ‖ revision, big-endian.
const SNAPSHOTS: TableDefinition<&[u8], &str> = TableDefinition::new("snapshots");
/// Who may read a collection. Key: collection ‖ user.
const MEMBERSHIP: TableDefinition<&[u8], u64> = TableDefinition::new("membership");

/// A store in one file.
#[derive(Debug)]
pub struct Embedded {
    database: Database,
    limits: crate::mailbox::Limits,
}

impl Embedded {
    /// Opens the file, creating it if absent.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Unavailable`] when the file cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::with_limits(path, crate::mailbox::Limits::default())
    }

    /// Opens the file with mailbox limits other than the defaults.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Unavailable`] when the file cannot be opened.
    pub fn with_limits(
        path: impl AsRef<Path>,
        limits: crate::mailbox::Limits,
    ) -> Result<Self, StorageError> {
        let store = Self {
            database: Database::create(path)?,
            limits,
        };
        store.prepare()?;
        Ok(store)
    }

    /// What this store's mailboxes may hold.
    pub(crate) const fn limits(&self) -> crate::mailbox::Limits {
        self.limits
    }

    /// Creates every table, so a reader never has to tell "no table yet" from
    /// "no rows yet" — the same question, and one that was answered two ways
    /// often enough to be worth removing.
    fn prepare(&self) -> Result<(), StorageError> {
        self.transact(|write| {
            write.open_table(USERS)?;
            write.open_table(HANDLES)?;
            write.open_table(DEVICES)?;
            write.open_table(USER_DEVICES)?;
            write.open_table(SHARES)?;
            write.open_table(SNAPSHOTS)?;
            write.open_table(MEMBERSHIP)?;
            write.open_table(crate::mailbox::MAILBOX)?;
            write.open_table(crate::mailbox::MAILBOX_NEXT)?;
            Ok(())
        })
    }

    // ----- identity -----------------------------------------------------

    /// Inserts a user and their first device as one unit.
    ///
    /// The two together or neither: a user with no device cannot authenticate
    /// and cannot be recovered, which is exactly the state a non-transactional
    /// store leaves behind when the second write fails. One file makes this a
    /// local transaction rather than a distributed one, which is the whole
    /// reason this engine exists.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Conflict`] when the handle is already claimed
    /// or the device is already enrolled.
    pub fn insert_registration(
        &self,
        user: &UserRecord,
        device: &DeviceRecord,
    ) -> Result<(), StorageError> {
        self.transact(|write| {
            let handle = handle_key(&user.normalized_username, &user.discriminator);
            let mut handles = write.open_table(HANDLES)?;
            if handles.get(handle.as_str())?.is_some() {
                return Err(StorageError::Conflict);
            }

            let mut devices = write.open_table(DEVICES)?;
            if devices.get(device.device_id.as_slice())?.is_some() {
                return Err(StorageError::Conflict);
            }

            handles.insert(handle.as_str(), user.user_id.as_slice())?;
            devices.insert(device.device_id.as_slice(), encode(device)?.as_str())?;
            write
                .open_table(USERS)?
                .insert(user.user_id.as_slice(), encode(user)?.as_str())?;
            write
                .open_table(USER_DEVICES)?
                .insert(pair(&user.user_id, &device.device_id).as_slice(), ())?;
            Ok(())
        })
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn find_user(&self, user: UserId) -> Result<Option<UserRecord>, StorageError> {
        self.get(USERS, user.as_slice())
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn find_user_by_handle(
        &self,
        normalized: &str,
        discriminator: &str,
    ) -> Result<Option<UserRecord>, StorageError> {
        let read = self.begin_read()?;
        let handles = read.open_table(HANDLES)?;
        let key = handle_key(normalized, discriminator);
        let Some(user) = handles.get(key.as_str())? else {
            return Ok(None);
        };
        let id = user.value().to_vec();
        drop(handles);
        self.get(USERS, &id)
    }

    /// # Errors
    /// Returns [`StorageError`] when the write fails or the device exists.
    pub fn enrol_device(&self, device: &DeviceRecord) -> Result<(), StorageError> {
        self.transact(|write| {
            let mut devices = write.open_table(DEVICES)?;
            if devices.get(device.device_id.as_slice())?.is_some() {
                return Err(StorageError::Conflict);
            }
            devices.insert(device.device_id.as_slice(), encode(device)?.as_str())?;
            write
                .open_table(USER_DEVICES)?
                .insert(pair(&device.user_id, &device.device_id).as_slice(), ())?;
            Ok(())
        })
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn find_device(&self, device: DeviceId) -> Result<Option<DeviceRecord>, StorageError> {
        self.get(DEVICES, device.as_slice())
    }

    /// Every device a user has, in a stable order.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn list_devices(&self, user: UserId) -> Result<Vec<DeviceRecord>, StorageError> {
        let read = self.begin_read()?;
        let index = read.open_table(USER_DEVICES)?;
        let devices = read.open_table(DEVICES)?;

        let (low, high) = prefix_range(user.as_slice());
        let mut found = Vec::new();
        for row in index.range(low.as_slice()..=high.as_slice())? {
            let (key, _) = row?;
            let device = &key.value()[user.len()..];
            if let Some(stored) = devices.get(device)? {
                found.push(decode(stored.value())?);
            }
        }
        Ok(found)
    }

    /// Replaces a device record, which is how revocation and a touch are both
    /// recorded.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the write fails.
    pub fn save_device(&self, device: &DeviceRecord) -> Result<(), StorageError> {
        self.put(DEVICES, device.device_id.as_slice(), device)
    }

    // ----- collections ---------------------------------------------------

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
    /// when the snapshot already exists — history does not get rewritten.
    pub fn save_publication(
        &self,
        head: &ShareRecord,
        snapshot: &ShareSnapshotRecord,
        expected: Option<u64>,
    ) -> Result<(), StorageError> {
        self.transact(|write| {
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
        self.get(SHARES, share.as_slice())
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn find_snapshot(
        &self,
        share: ShareId,
        revision: u64,
    ) -> Result<Option<ShareSnapshotRecord>, StorageError> {
        self.get(SNAPSHOTS, &keyed(&share, revision))
    }

    /// # Errors
    /// Returns [`StorageError`] when the write fails.
    pub fn grant_access(
        &self,
        share: ShareId,
        user: UserId,
        at_unix_ns: u64,
    ) -> Result<(), StorageError> {
        self.transact(|write| {
            write
                .open_table(MEMBERSHIP)?
                .insert(pair(&share, &user).as_slice(), at_unix_ns)?;
            Ok(())
        })
    }

    /// # Errors
    /// Returns [`StorageError`] when the write fails.
    pub fn revoke_access(&self, share: ShareId, user: UserId) -> Result<(), StorageError> {
        self.transact(|write| {
            write
                .open_table(MEMBERSHIP)?
                .remove(pair(&share, &user).as_slice())?;
            Ok(())
        })
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails.
    pub fn has_access(&self, share: ShareId, user: UserId) -> Result<bool, StorageError> {
        let read = self.begin_read()?;
        let table = read.open_table(MEMBERSHIP)?;
        Ok(table.get(pair(&share, &user).as_slice())?.is_some())
    }

    /// Everyone a collection is shared with.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the read fails.
    pub fn list_members(&self, share: ShareId) -> Result<Vec<UserId>, StorageError> {
        let read = self.begin_read()?;
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

    // ----- the two operations the rest is built from ----------------------

    /// Runs `work` in one transaction, committing only if it succeeds.
    ///
    /// Every write in this engine goes through here, which is what makes
    /// "either both rows or neither" the default rather than something each
    /// method has to remember. It also means the database's own failure paths
    /// exist in two places instead of forty.
    pub(crate) fn transact<T>(
        &self,
        work: impl FnOnce(&redb::WriteTransaction) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let write = self.database.begin_write()?;
        let outcome = work(&write)?;
        write.commit()?;
        Ok(outcome)
    }

    pub(crate) fn begin_read(&self) -> Result<redb::ReadTransaction, StorageError> {
        Ok(self.database.begin_read()?)
    }

    fn get<T: DeserializeOwned>(
        &self,
        table: TableDefinition<&[u8], &str>,
        key: &[u8],
    ) -> Result<Option<T>, StorageError> {
        let read = self.begin_read()?;
        let table = read.open_table(table)?;
        table
            .get(key)?
            .map(|stored| decode(stored.value()))
            .transpose()
    }

    fn put<T: Serialize>(
        &self,
        table: TableDefinition<&[u8], &str>,
        key: &[u8],
        value: &T,
    ) -> Result<(), StorageError> {
        self.transact(|write| {
            write
                .open_table(table)?
                .insert(key, encode(value)?.as_str())?;
            Ok(())
        })
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
            // A full mailbox is not a race, so it is not something to retry.
            error @ StorageError::MailboxFull { .. } => Self::Unavailable(error.to_string()),
            StorageError::Malformed => Self::Unavailable("a stored row is malformed".to_owned()),
            StorageError::Unavailable(reason) => Self::Unavailable(reason),
        }
    }
}

fn encode<T: Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|_| StorageError::Malformed)
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T, StorageError> {
    serde_json::from_str(value).map_err(|_| StorageError::Malformed)
}

/// A handle's key: the indexed form and its discriminator, which together are
/// what makes one unique.
fn handle_key(normalized: &str, discriminator: &str) -> String {
    format!("{normalized}#{discriminator}")
}

/// A key that groups every row of one owner together, ordered by the number
/// that follows. Big-endian, so byte order agrees with numeric order.
pub(crate) fn keyed(prefix: &[u8], number: u64) -> Vec<u8> {
    let mut key = prefix.to_vec();
    key.extend_from_slice(&number.to_be_bytes());
    key
}

/// A key naming a relationship between two identifiers.
fn pair(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut key = left.to_vec();
    key.extend_from_slice(right);
    key
}

/// The half-open range covering every key under one prefix.
pub(crate) fn prefix_range(prefix: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut high = prefix.to_vec();
    high.extend_from_slice(&[u8::MAX; 40]);
    (prefix.to_vec(), high)
}

#[cfg(test)]
mod tests {
    //! One suite, and it is written against behaviour rather than against
    //! `redb`. When the `MongoDB` engine moves here it answers to the same
    //! assertions, which is the only way "either engine" means anything.

    use super::*;

    /// A directory that removes itself.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "portalis-storage-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn open(&self) -> Embedded {
            Embedded::open(self.0.join("service.redb")).expect("opens")
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

    fn user(id: UserId, username: &str, discriminator: &str) -> UserRecord {
        UserRecord {
            user_id: id,
            username: username.to_owned(),
            normalized_username: username.to_lowercase(),
            discriminator: discriminator.to_owned(),
            created_at_unix_ns: 1,
        }
    }

    fn device(id: u8, owner: UserId) -> DeviceRecord {
        DeviceRecord {
            device_id: [id; 32],
            user_id: owner,
            public_key: [id; 32],
            encryption_public_key: [id; 32],
            created_at_unix_ns: 1,
            last_authenticated_at_unix_ns: None,
            revoked_at_unix_ns: None,
        }
    }

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

    /// The reason this engine exists: a registration is one transaction in one
    /// file, rather than a distributed one needing a replica set.
    #[test]
    fn a_registration_writes_a_user_and_a_device_or_neither() {
        let scratch = Scratch::new("registration");
        let store = scratch.open();

        store
            .insert_registration(&user(ADA, "Ada", "7Q2XZ"), &device(1, ADA))
            .expect("registers");

        assert_eq!(
            store.find_user(ADA).expect("reads"),
            Some(user(ADA, "Ada", "7Q2XZ"))
        );
        assert_eq!(
            store.find_device([1; 32]).expect("reads"),
            Some(device(1, ADA))
        );

        // The handle is claimed, so a second registration under it fails and
        // leaves nothing behind.
        assert!(matches!(
            store.insert_registration(&user(GRACE, "Ada", "7Q2XZ"), &device(2, GRACE)),
            Err(StorageError::Conflict)
        ));
        assert_eq!(store.find_user(GRACE).expect("reads"), None);
        assert_eq!(
            store.find_device([2; 32]).expect("reads"),
            None,
            "the device did not survive the failed registration"
        );
    }

    #[test]
    fn a_device_that_is_already_enrolled_is_refused() {
        let scratch = Scratch::new("enrolled");
        let store = scratch.open();
        store
            .insert_registration(&user(ADA, "Ada", "7Q2XZ"), &device(1, ADA))
            .expect("registers");

        assert!(matches!(
            store.insert_registration(&user(GRACE, "Grace", "4KQ2P"), &device(1, GRACE)),
            Err(StorageError::Conflict)
        ));
        assert!(matches!(
            store.enrol_device(&device(1, ADA)),
            Err(StorageError::Conflict)
        ));
    }

    #[test]
    fn a_handle_finds_its_user_and_a_stranger_finds_nobody() {
        let scratch = Scratch::new("handles");
        let store = scratch.open();
        store
            .insert_registration(&user(ADA, "Ada", "7Q2XZ"), &device(1, ADA))
            .expect("registers");

        assert_eq!(
            store.find_user_by_handle("ada", "7Q2XZ").expect("reads"),
            Some(user(ADA, "Ada", "7Q2XZ"))
        );
        // The discriminator is part of it: the same name is a different person.
        assert_eq!(
            store.find_user_by_handle("ada", "0000").expect("reads"),
            None
        );
        assert_eq!(
            store.find_user_by_handle("mira", "7Q2XZ").expect("reads"),
            None
        );
    }

    #[test]
    fn a_users_devices_are_theirs_and_nobody_elses() {
        let scratch = Scratch::new("devices");
        let store = scratch.open();
        store
            .insert_registration(&user(ADA, "Ada", "7Q2XZ"), &device(1, ADA))
            .expect("registers");
        store
            .insert_registration(&user(GRACE, "Grace", "4KQ2P"), &device(9, GRACE))
            .expect("registers");
        store.enrol_device(&device(2, ADA)).expect("enrols");

        let ada = store.list_devices(ADA).expect("reads");
        assert_eq!(ada.len(), 2);
        assert!(ada.iter().all(|device| device.user_id == ADA));
        assert_eq!(store.list_devices(GRACE).expect("reads").len(), 1);
        assert!(store.list_devices([9; 16]).expect("reads").is_empty());
    }

    #[test]
    fn a_device_record_can_be_replaced_to_record_a_revocation() {
        let scratch = Scratch::new("revoke");
        let store = scratch.open();
        store
            .insert_registration(&user(ADA, "Ada", "7Q2XZ"), &device(1, ADA))
            .expect("registers");

        let revoked = DeviceRecord {
            revoked_at_unix_ns: Some(99),
            ..device(1, ADA)
        };
        store.save_device(&revoked).expect("saves");

        assert_eq!(store.find_device([1; 32]).expect("reads"), Some(revoked));
        assert_eq!(
            store.list_devices(ADA).expect("reads").len(),
            1,
            "still theirs, and still listed"
        );
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

    /// The point of a durable engine, checked the only way that counts.
    #[test]
    fn everything_survives_a_restart() {
        let scratch = Scratch::new("restart");
        {
            let store = scratch.open();
            store
                .insert_registration(&user(ADA, "Ada", "7Q2XZ"), &device(1, ADA))
                .expect("registers");
            store
                .save_publication(&share(1, b"one"), &snapshot(1, b"one"), None)
                .expect("publishes");
            store.grant_access(SHARE, GRACE, 10).expect("grants");
        }

        let store = scratch.open();

        assert_eq!(
            store.find_user(ADA).expect("reads"),
            Some(user(ADA, "Ada", "7Q2XZ"))
        );
        assert_eq!(store.list_devices(ADA).expect("reads").len(), 1);
        assert_eq!(
            store.find_share(SHARE).expect("reads"),
            Some(share(1, b"one"))
        );
        assert_eq!(
            store.find_snapshot(SHARE, 1).expect("reads"),
            Some(snapshot(1, b"one"))
        );
        assert!(store.has_access(SHARE, GRACE).expect("reads"));
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

    #[test]
    fn a_damaged_row_is_reported_rather_than_guessed_at() {
        let scratch = Scratch::new("damaged");
        let store = scratch.open();

        store
            .transact(|write| {
                write
                    .open_table(USERS)?
                    .insert(ADA.as_slice(), "not a user")?;
                Ok(())
            })
            .expect("writes nonsense where a user should be");

        assert!(matches!(store.find_user(ADA), Err(StorageError::Malformed)));
    }

    /// The service's rules speak `RepositoryError`, so an engine that invented
    /// its own vocabulary would make callers that only work with one engine.
    #[test]
    fn a_failure_reaches_the_service_in_its_own_terms() {
        assert!(matches!(
            RepositoryError::from(StorageError::Conflict),
            RepositoryError::VersionConflict
        ));
        assert!(matches!(
            RepositoryError::from(StorageError::Malformed),
            RepositoryError::Unavailable(_)
        ));
        assert!(matches!(
            RepositoryError::from(StorageError::Unavailable("the disk is gone".to_owned())),
            RepositoryError::Unavailable(reason) if reason.contains("disk")
        ));
    }

    /// A file whose tables hold a different shape than this build expects —
    /// what a store written by a version that changed a type looks like. The
    /// engine reports it rather than reading the bytes as something they are
    /// not.
    #[test]
    fn a_table_written_with_another_shape_is_reported_not_reinterpreted() {
        // Same name as `USERS`, different value type.
        const IMPOSTOR: TableDefinition<&[u8], u64> = TableDefinition::new("users");

        let scratch = Scratch::new("mismatch");
        {
            let database = Database::create(scratch.0.join("service.redb")).expect("creates");
            let write = database.begin_write().expect("writes");
            {
                write
                    .open_table(IMPOSTOR)
                    .expect("impostor")
                    .insert(ADA.as_slice(), 1_u64)
                    .expect("writes");
            }
            write.commit().expect("commits");
        }

        let refused = Embedded::open(scratch.0.join("service.redb"))
            .expect_err("this build cannot read that shape");

        assert!(
            matches!(refused, StorageError::Unavailable(_)),
            "got {refused:?}"
        );
    }

    #[test]
    fn a_path_that_cannot_be_opened_is_reported() {
        let scratch = Scratch::new("unopenable");
        let failed = Embedded::open(&scratch.0).expect_err("a directory is not a database");

        assert!(matches!(failed, StorageError::Unavailable(_)));
        assert!(!failed.to_string().is_empty());
    }
}
