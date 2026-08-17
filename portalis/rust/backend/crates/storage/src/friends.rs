//! Who has asked whom, and who said yes.
//!
//! One row per friendship rather than one per direction, keyed by the pair in
//! a fixed order (`FriendshipEdge` sorts the two identifiers). Two rows would
//! mean two answers to "are these people friends", and they would eventually
//! differ.
//!
//! Writes are versioned. A friendship changes through a small number of
//! transitions, and two devices answering the same request at once would
//! otherwise both succeed and one would silently win. A write that names a
//! version other than the stored one is refused, so the loser is told rather
//! than overwritten.

use redb::{ReadableTable, TableDefinition};

use portalis_nexus_server_core::{FriendshipEdge, FriendshipRecord, UserId};

use crate::StorageError;
use crate::store::{Store, decode, encode, pair};

/// Friendships, by ordered pair. Key: low ‖ high.
const FRIENDSHIPS: TableDefinition<&[u8], &str> = TableDefinition::new("friendships");

/// The friends endpoint.
#[derive(Debug)]
pub struct Friends {
    store: Store,
}

impl Friends {
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
            write.open_table(FRIENDSHIPS)?;
            Ok(())
        })?;
        Ok(Self { store })
    }

    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn find(&self, edge: FriendshipEdge) -> Result<Option<FriendshipRecord>, StorageError> {
        let read = self.store.read()?;
        let table = read.open_table(FRIENDSHIPS)?;
        table
            .get(key(edge).as_slice())?
            .map(|stored| decode(stored.value()))
            .transpose()
    }

    /// Writes a friendship only while the stored version still matches.
    ///
    /// A version of zero means the edge must not exist yet, which is how the
    /// first request is told apart from a reply to one.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Conflict`] when the stored version has moved,
    /// so a device that read an older one is told rather than overwriting.
    pub fn save(&self, record: &FriendshipRecord, expected: u64) -> Result<(), StorageError> {
        self.store.transact(|write| {
            let mut table = write.open_table(FRIENDSHIPS)?;
            let key = key(record.edge);
            let stored: Option<FriendshipRecord> = table
                .get(key.as_slice())?
                .map(|stored| decode(stored.value()))
                .transpose()?;
            if stored.map_or(0, |friendship| friendship.version) != expected {
                return Err(StorageError::Conflict);
            }
            table.insert(key.as_slice(), encode(record)?.as_str())?;
            Ok(())
        })
    }

    /// Every friendship joining `user`.
    ///
    /// Scans, because a friendship is keyed by the pair and either half may be
    /// the one asking. An index per side would be two more rows to keep in
    /// step with the one that matters.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn list(&self, user: UserId) -> Result<Vec<FriendshipRecord>, StorageError> {
        let read = self.store.read()?;
        let table = read.open_table(FRIENDSHIPS)?;

        let mut joined = Vec::new();
        for row in table.iter()? {
            let (_, value) = row?;
            let record: FriendshipRecord = decode(value.value())?;
            if record.edge.user_low() == user || record.edge.user_high() == user {
                joined.push(record);
            }
        }
        Ok(joined)
    }
}

fn key(edge: FriendshipEdge) -> Vec<u8> {
    pair(&edge.user_low(), &edge.user_high())
}
