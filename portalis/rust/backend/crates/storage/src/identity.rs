//! Who exists, and which devices are theirs.
//!
//! One file, because these four tables must move together. A registration
//! writes a user, their handle claim, their first device and the index that
//! links them — and a registration that wrote three of those would leave
//! somebody who cannot authenticate and cannot be recovered. That is exactly
//! the state a store without transactions leaves behind when the fourth write
//! fails, and it is why this endpoint is one file rather than four.

use redb::{ReadableTable, TableDefinition};

use portalis_nexus_server_core::{DeviceId, DeviceRecord, UserId, UserRecord};

use crate::StorageError;
use crate::store::{Store, decode, encode, pair, prefix_range};

/// Users, by identifier.
const USERS: TableDefinition<&[u8], &str> = TableDefinition::new("users");
/// Handle claims: the indexed form and discriminator, to a user.
const HANDLES: TableDefinition<&str, &[u8]> = TableDefinition::new("handles");
/// Devices, by device identifier.
const DEVICES: TableDefinition<&[u8], &str> = TableDefinition::new("devices");
/// Which devices belong to whom. Key: user ‖ device.
const USER_DEVICES: TableDefinition<&[u8], ()> = TableDefinition::new("user_devices");

/// The identity endpoint.
#[derive(Debug)]
pub struct Identity {
    store: Store,
}

impl Identity {
    /// Opens this endpoint's file.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the file cannot be opened or prepared.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        let store = Store::open(path)?;
        store.declare(|write| {
            write.open_table(USERS)?;
            write.open_table(HANDLES)?;
            write.open_table(DEVICES)?;
            write.open_table(USER_DEVICES)?;
            Ok(())
        })?;
        Ok(Self { store })
    }

    /// Writes a user and their first device, or neither.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::HandleTaken`] or [`StorageError::DeviceExists`],
    /// which are different answers: one retries with another discriminator,
    /// the other does not retry at all.
    pub fn insert_registration(
        &self,
        user: &UserRecord,
        device: &DeviceRecord,
    ) -> Result<(), StorageError> {
        self.store.transact(|write| {
            let handle = handle_key(&user.normalized_username, &user.discriminator);
            let mut handles = write.open_table(HANDLES)?;
            if handles.get(handle.as_str())?.is_some() {
                return Err(StorageError::HandleTaken);
            }
            let mut devices = write.open_table(DEVICES)?;
            if devices.get(device.device_id.as_slice())?.is_some() {
                return Err(StorageError::DeviceExists);
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
        let read = self.store.read()?;
        let table = read.open_table(USERS)?;
        table
            .get(user.as_slice())?
            .map(|stored| decode(stored.value()))
            .transpose()
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn find_user_by_handle(
        &self,
        normalized: &str,
        discriminator: &str,
    ) -> Result<Option<UserRecord>, StorageError> {
        let read = self.store.read()?;
        let handles = read.open_table(HANDLES)?;
        let key = handle_key(normalized, discriminator);
        let Some(owner) = handles.get(key.as_str())? else {
            return Ok(None);
        };
        read.open_table(USERS)?
            .get(owner.value())?
            .map(|stored| decode(stored.value()))
            .transpose()
    }

    /// # Errors
    /// Returns [`StorageError::DeviceExists`] when it is already enrolled.
    pub fn enrol_device(&self, device: &DeviceRecord) -> Result<(), StorageError> {
        self.store.transact(|write| {
            let mut devices = write.open_table(DEVICES)?;
            if devices.get(device.device_id.as_slice())?.is_some() {
                return Err(StorageError::DeviceExists);
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
        let read = self.store.read()?;
        let table = read.open_table(DEVICES)?;
        table
            .get(device.as_slice())?
            .map(|stored| decode(stored.value()))
            .transpose()
    }

    /// Every device a user has, in a stable order.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn list_devices(&self, user: UserId) -> Result<Vec<DeviceRecord>, StorageError> {
        let read = self.store.read()?;
        let index = read.open_table(USER_DEVICES)?;
        let devices = read.open_table(DEVICES)?;
        let (low, high) = prefix_range(user.as_slice());

        let mut found = Vec::new();
        for row in index.range(low.as_slice()..=high.as_slice())? {
            let (key, _) = row?;
            let device = key
                .value()
                .get(user.len()..)
                .ok_or(StorageError::Malformed)?;
            if let Some(stored) = devices.get(device)? {
                found.push(decode(stored.value())?);
            }
        }
        Ok(found)
    }

    /// Replaces a device record, which is how a revocation and a touch are
    /// both recorded.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the write fails.
    pub fn save_device(&self, device: &DeviceRecord) -> Result<(), StorageError> {
        self.store.transact(|write| {
            write
                .open_table(DEVICES)?
                .insert(device.device_id.as_slice(), encode(device)?.as_str())?;
            Ok(())
        })
    }
}

/// A handle's key: the indexed form and its discriminator, which together are
/// what make one unique.
fn handle_key(normalized: &str, discriminator: &str) -> String {
    format!("{normalized}#{discriminator}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "portalis-identity-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn open(&self) -> Identity {
            Identity::open(self.0.join("identity.redb")).expect("opens")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const ADA: UserId = [1; 16];
    const GRACE: UserId = [2; 16];

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
            Err(StorageError::HandleTaken)
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
            Err(StorageError::DeviceExists)
        ));
        assert!(matches!(
            store.enrol_device(&device(1, ADA)),
            Err(StorageError::DeviceExists)
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
}
