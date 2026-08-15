//! One authoritative place for this device's own truth.
//!
//! `SPEC.md` §13 and §12: one file, in the platform data directory, holding
//! everything the device knows — its identity, the device logs it has
//! verified, its collections and their revisions, and the transfer history.
//!
//! Before this, collections lived in a JSON file rewritten whole on every
//! change. That has two problems a transactional store does not. A crash
//! partway through a write loses everything rather than one change — this
//! project's own machine filled its disk mid-session and the file was found
//! empty afterwards — and every reader had to hold the entire store in memory
//! to answer any question about it.
//!
//! Two rules keep the schema honest:
//!
//! **Append-only where the data is.** The current revision is the highest
//! number in `revisions`, never a separate mutable row that could disagree
//! with the chain it is supposed to summarise.
//!
//! **A store from the future refuses to open.** Reading a newer schema with
//! older assumptions means misinterpreting a person's own data, which is worse
//! than declining to start and saying why.

pub mod records;
pub mod schema;

use std::path::Path;
use std::sync::Arc;

#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};

use redb::{Database, ReadableTable, TableDefinition};
use thiserror::Error;

use records::{
    Malformed, StoredCollection, StoredContact, StoredEntry, StoredImportEntry, StoredSample,
};
use schema::{
    COLLECTIONS, CONTACTS, DEVICE_LOG, ENTRIES, IDENTITY, MANIFESTS, META, OUTBOX, REVISIONS,
    SAMPLES, SCHEMA_VERSION, SCHEMA_VERSION_KEY, TORRENT_IMPORTS, TORRENT_IMPORT_DESCRIPTORS,
    TORRENT_IMPORT_ENTRIES,
};

/// Why the store could not answer.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Written by a newer version of the application. Opening it would mean
    /// guessing at a shape this build does not know.
    #[error(
        "this store was written by a newer version of Portalis (schema {found}, this build speaks {SCHEMA_VERSION}) — upgrade to open it"
    )]
    FromTheFuture { found: u32 },
    #[error("the Portalis data directory could not be created: {0}")]
    DataDir(String),
    #[error("a stored row is malformed: the store may be damaged")]
    Malformed,
    /// Boxed because redb's error is large and every `Result` in this module
    /// would otherwise carry its width on the success path too.
    #[error(transparent)]
    Database(#[from] Box<redb::Error>),
}

// redb's error family is wide; every one of these means the same thing to a
// caller, so they collapse rather than leaking the storage engine upward.
macro_rules! from_redb {
    ($($error:ty),+ $(,)?) => {
        $(impl From<$error> for StoreError {
            fn from(error: $error) -> Self {
                Self::Database(Box::new(error.into()))
            }
        })+
    };
}
from_redb!(
    redb::DatabaseError,
    redb::TransactionError,
    redb::TableError,
    redb::StorageError,
    redb::CommitError,
);

impl From<Malformed> for StoreError {
    fn from(_: Malformed) -> Self {
        Self::Malformed
    }
}

/// The device's local store.
#[derive(Debug)]
pub struct Store {
    database: Database,
}

// During the staged migration, both the new Nexus core and the legacy
// collection adapter need tables in the same redb file. redb deliberately
// takes an exclusive process lock, so opening it separately is not a valid
// compatibility strategy. This is the one process-owned handle; it goes when
// the old adapter is deleted and the core becomes its sole consumer.
#[cfg(not(test))]
static APP_STORE: OnceLock<Mutex<Option<Arc<Store>>>> = OnceLock::new();

/// Opens the one production store handle shared by temporary migration paths.
///
/// # Errors
///
/// Returns an error when the data directory or database cannot be opened.
#[cfg(not(test))]
pub(crate) fn app_store() -> Result<Arc<Store>, StoreError> {
    let slot = APP_STORE.get_or_init(|| Mutex::new(None));
    let mut store = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(store) = store.as_ref() {
        return Ok(Arc::clone(store));
    }

    let data_dir = crate::paths::state_dir();
    std::fs::create_dir_all(&data_dir).map_err(|error| StoreError::DataDir(error.to_string()))?;
    let opened = Arc::new(Store::open(data_dir.join("portalis.redb"))?);
    *store = Some(Arc::clone(&opened));
    Ok(opened)
}

/// Test state directories are intentionally isolated per test, so they must
/// not share the production process cache above.
#[cfg(test)]
pub(crate) fn app_store() -> Result<Arc<Store>, StoreError> {
    let data_dir = crate::paths::state_dir();
    std::fs::create_dir_all(&data_dir).map_err(|error| StoreError::DataDir(error.to_string()))?;
    Ok(Arc::new(Store::open(data_dir.join("portalis.redb"))?))
}

impl Store {
    /// Opens the store at `path`, creating and initialising it if absent.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::FromTheFuture`] for a store a newer build wrote,
    /// or [`StoreError::Database`] when the file cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let store = Self {
            database: Database::create(path)?,
        };
        store.prepare()?;
        Ok(store)
    }

    /// Brings a freshly opened file up to the schema this build speaks.
    ///
    /// Creating every table here rather than lazily means a reader never has
    /// to handle "the table does not exist yet", which is the same question as
    /// "the table is empty" and was answered two ways often enough to be worth
    /// removing.
    fn prepare(&self) -> Result<(), StoreError> {
        let found = self.version()?;
        if found > u64::from(SCHEMA_VERSION) {
            return Err(StoreError::FromTheFuture {
                found: u32::try_from(found).unwrap_or(u32::MAX),
            });
        }

        let write = self.database.begin_write()?;
        {
            // Opening a table creates it. Listed explicitly so adding one to
            // §13 without creating it here fails to compile rather than at
            // the first read.
            write.open_table(META)?;
            write.open_table(IDENTITY)?;
            write.open_table(DEVICE_LOG)?;
            write.open_table(CONTACTS)?;
            write.open_table(COLLECTIONS)?;
            write.open_table(REVISIONS)?;
            write.open_table(MANIFESTS)?;
            write.open_table(ENTRIES)?;
            write.open_table(TORRENT_IMPORTS)?;
            write.open_table(TORRENT_IMPORT_ENTRIES)?;
            write.open_table(TORRENT_IMPORT_DESCRIPTORS)?;
            write.open_table(OUTBOX)?;
            write.open_table(SAMPLES)?;
            write
                .open_table(META)?
                .insert(SCHEMA_VERSION_KEY, u64::from(SCHEMA_VERSION))?;
        }
        write.commit()?;
        Ok(())
    }

    /// The schema version recorded in the file, or 0 for a store that has
    /// never been written to.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Database`] when the file cannot be read.
    pub fn version(&self) -> Result<u64, StoreError> {
        let read = self.database.begin_read()?;
        let Ok(meta) = read.open_table(META) else {
            // No meta table at all is a store nothing has written yet.
            return Ok(0);
        };
        Ok(meta
            .get(SCHEMA_VERSION_KEY)?
            .map_or(0, |version| version.value()))
    }

    // ----- collections -------------------------------------------------

    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_collection(
        &self,
        collection_id: &[u8],
        collection: &StoredCollection,
    ) -> Result<(), StoreError> {
        self.put(COLLECTIONS, collection_id, &collection.encode())
    }

    /// # Errors
    /// Returns [`StoreError`] when the read fails or the row is malformed.
    pub fn collection(&self, collection_id: &[u8]) -> Result<Option<StoredCollection>, StoreError> {
        self.get(COLLECTIONS, collection_id)?
            .map(|bytes| StoredCollection::decode(&bytes).map_err(StoreError::from))
            .transpose()
    }

    /// Every collection this device knows, in key order.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the read fails or a row is malformed.
    pub fn collections(&self) -> Result<Vec<(Vec<u8>, StoredCollection)>, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(COLLECTIONS)?;
        let mut collections = Vec::new();
        for row in table.iter()? {
            let (key, value) = row?;
            collections.push((
                key.value().to_vec(),
                StoredCollection::decode(value.value())?,
            ));
        }
        Ok(collections)
    }

    /// Removes a collection and everything recorded under its key.
    ///
    /// The history and the revisions go with it. Keys are handed out in
    /// sequence and reused, so anything left behind is inherited by the next
    /// collection to take the key: a torrent added after a delete opened with
    /// the deleted one's transfer chart already drawn, showing a download it
    /// had never done.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn forget_collection(&self, collection_id: &[u8]) -> Result<(), StoreError> {
        let (low, high) = schema::range_of(collection_id);
        let write = self.database.begin_write()?;
        {
            write.open_table(COLLECTIONS)?.remove(collection_id)?;
            write
                .open_table(SAMPLES)?
                .retain_in(low.as_slice()..=high.as_slice(), |_, _| false)?;
            write
                .open_table(REVISIONS)?
                .retain_in(low.as_slice()..=high.as_slice(), |_, _| false)?;
        }
        write.commit()?;
        Ok(())
    }

    // ----- revisions ---------------------------------------------------

    /// Records a verified revision. The highest number is the current one.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_revision(
        &self,
        collection_id: &[u8],
        number: u64,
        revision: &[u8],
    ) -> Result<(), StoreError> {
        self.put(REVISIONS, &schema::keyed(collection_id, number), revision)
    }

    /// The highest revision held for a collection, with its number.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the read fails.
    pub fn current_revision(
        &self,
        collection_id: &[u8],
    ) -> Result<Option<(u64, Vec<u8>)>, StoreError> {
        let (low, high) = schema::range_of(collection_id);
        let read = self.database.begin_read()?;
        let table = read.open_table(REVISIONS)?;
        // Last row of the prefix range, which big-endian keys make the highest
        // number rather than merely the last one written.
        let Some(row) = table.range(low.as_slice()..=high.as_slice())?.next_back() else {
            return Ok(None);
        };
        let (key, value) = row?;
        let number = schema::number_of(key.value()).ok_or(StoreError::Malformed)?;
        Ok(Some((number, value.value().to_vec())))
    }

    /// Every revision held for a collection, ascending.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the read fails.
    pub fn revisions(&self, collection_id: &[u8]) -> Result<Vec<(u64, Vec<u8>)>, StoreError> {
        let (low, high) = schema::range_of(collection_id);
        let read = self.database.begin_read()?;
        let table = read.open_table(REVISIONS)?;
        let mut revisions = Vec::new();
        for row in table.range(low.as_slice()..=high.as_slice())? {
            let (key, value) = row?;
            let number = schema::number_of(key.value()).ok_or(StoreError::Malformed)?;
            revisions.push((number, value.value().to_vec()));
        }
        Ok(revisions)
    }

    // ----- manifests and entries ---------------------------------------

    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_manifest(&self, manifest_hash: &[u8], manifest: &[u8]) -> Result<(), StoreError> {
        self.put(MANIFESTS, manifest_hash, manifest)
    }

    /// # Errors
    /// Returns [`StoreError`] when the read fails.
    pub fn manifest(&self, manifest_hash: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        self.get(MANIFESTS, manifest_hash)
    }

    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_entry(&self, info_hash: &[u8], entry: &StoredEntry) -> Result<(), StoreError> {
        self.put(ENTRIES, info_hash, &entry.encode())
    }

    /// # Errors
    /// Returns [`StoreError`] when the read fails or the row is malformed.
    pub fn entry(&self, info_hash: &[u8]) -> Result<Option<StoredEntry>, StoreError> {
        self.get(ENTRIES, info_hash)?
            .map(|bytes| StoredEntry::decode(&bytes).map_err(StoreError::from))
            .transpose()
    }

    // ----- torrent imports ---------------------------------------------

    /// Records a descriptor source before any torrent payload is downloaded.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_torrent_import(&self, collection_id: &[u8], source: &str) -> Result<(), StoreError> {
        self.put(TORRENT_IMPORTS, collection_id, source.as_bytes())
    }

    /// The unresolved import source, if this collection came from a torrent.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the row cannot be read or is not UTF-8.
    pub fn torrent_import(&self, collection_id: &[u8]) -> Result<Option<String>, StoreError> {
        self.get(TORRENT_IMPORTS, collection_id)?
            .map(|source| String::from_utf8(source).map_err(|_| StoreError::Malformed))
            .transpose()
    }

    /// Removes an import source once its owning collection is gone.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn forget_torrent_import(&self, collection_id: &[u8]) -> Result<(), StoreError> {
        let (low, high) = schema::range_of(collection_id);
        let write = self.database.begin_write()?;
        {
            write.open_table(TORRENT_IMPORTS)?.remove(collection_id)?;
            write
                .open_table(TORRENT_IMPORT_DESCRIPTORS)?
                .remove(collection_id)?;
            let mut entries = write.open_table(TORRENT_IMPORT_ENTRIES)?;
            let keys = entries
                .range(low.as_slice()..=high.as_slice())?
                .map(|row| row.map(|(key, _)| key.value().to_vec()))
                .collect::<Result<Vec<_>, _>>()?;
            for key in keys {
                entries.remove(key.as_slice())?;
            }
        }
        write.commit()?;
        Ok(())
    }

    /// Persists a resolved `.torrent` descriptor as this collection's initial
    /// content, so the local path is no longer needed after import.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_torrent_import_descriptor(
        &self,
        collection_id: &[u8],
        descriptor: &[u8],
    ) -> Result<(), StoreError> {
        self.put(TORRENT_IMPORT_DESCRIPTORS, collection_id, descriptor)
    }

    /// The imported `.torrent` descriptor, if metadata has resolved.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the row cannot be read.
    pub fn torrent_import_descriptor(
        &self,
        collection_id: &[u8],
    ) -> Result<Option<Vec<u8>>, StoreError> {
        self.get(TORRENT_IMPORT_DESCRIPTORS, collection_id)
    }

    /// Replaces one import's metadata-only file selection.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the transaction cannot be committed.
    pub fn put_torrent_import_entries(
        &self,
        collection_id: &[u8],
        entries: &[StoredImportEntry],
    ) -> Result<(), StoreError> {
        let (low, high) = schema::range_of(collection_id);
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(TORRENT_IMPORT_ENTRIES)?;
            let old = table
                .range(low.as_slice()..=high.as_slice())?
                .map(|row| row.map(|(key, _)| key.value().to_vec()))
                .collect::<Result<Vec<_>, _>>()?;
            for key in old {
                table.remove(key.as_slice())?;
            }
            for (ordinal, entry) in entries.iter().enumerate() {
                let ordinal = u64::try_from(ordinal).map_err(|_| StoreError::Malformed)?;
                table.insert(
                    schema::keyed(collection_id, ordinal).as_slice(),
                    entry.encode().as_slice(),
                )?;
            }
        }
        write.commit()?;
        Ok(())
    }

    /// Imported files in their torrent order.
    ///
    /// # Errors
    /// Returns [`StoreError`] when a row cannot be decoded.
    pub fn torrent_import_entries(
        &self,
        collection_id: &[u8],
    ) -> Result<Vec<StoredImportEntry>, StoreError> {
        let (low, high) = schema::range_of(collection_id);
        let read = self.database.begin_read()?;
        let table = read.open_table(TORRENT_IMPORT_ENTRIES)?;
        table
            .range(low.as_slice()..=high.as_slice())?
            .map(|row| {
                let (_, value) = row?;
                StoredImportEntry::decode(value.value()).map_err(StoreError::from)
            })
            .collect()
    }

    // ----- identity, contacts, device logs ------------------------------

    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_identity(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        let write = self.database.begin_write()?;
        {
            write.open_table(IDENTITY)?.insert(key, value)?;
        }
        write.commit()?;
        Ok(())
    }

    /// # Errors
    /// Returns [`StoreError`] when the read fails.
    pub fn identity(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(IDENTITY)?;
        Ok(table.get(key)?.map(|value| value.value().to_vec()))
    }

    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_contact(&self, contact: &StoredContact) -> Result<(), StoreError> {
        self.put(CONTACTS, &contact.root_key, &contact.encode())
    }

    /// # Errors
    /// Returns [`StoreError`] when the read fails or the row is malformed.
    pub fn contact(&self, root_key: &[u8]) -> Result<Option<StoredContact>, StoreError> {
        self.get(CONTACTS, root_key)?
            .map(|bytes| StoredContact::decode(&bytes).map_err(StoreError::from))
            .transpose()
    }

    /// Appends one verified device log entry.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_log_entry(
        &self,
        root_key: &[u8],
        sequence: u64,
        entry: &[u8],
    ) -> Result<(), StoreError> {
        self.put(DEVICE_LOG, &schema::keyed(root_key, sequence), entry)
    }

    /// One person's device log, in sequence order.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the read fails.
    pub fn device_log(&self, root_key: &[u8]) -> Result<Vec<Vec<u8>>, StoreError> {
        let (low, high) = schema::range_of(root_key);
        let read = self.database.begin_read()?;
        let table = read.open_table(DEVICE_LOG)?;
        let mut entries = Vec::new();
        for row in table.range(low.as_slice()..=high.as_slice())? {
            entries.push(row?.1.value().to_vec());
        }
        Ok(entries)
    }

    // ----- outbox and samples -------------------------------------------

    /// Queues a command until there is somewhere to send it.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn queue_command(&self, sequence: u64, command: &[u8]) -> Result<(), StoreError> {
        let write = self.database.begin_write()?;
        {
            write.open_table(OUTBOX)?.insert(sequence, command)?;
        }
        write.commit()?;
        Ok(())
    }

    /// Everything waiting to be sent, oldest first.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the read fails.
    pub fn queued_commands(&self) -> Result<Vec<(u64, Vec<u8>)>, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(OUTBOX)?;
        let mut queued = Vec::new();
        for row in table.iter()? {
            let (key, value) = row?;
            queued.push((key.value(), value.value().to_vec()));
        }
        Ok(queued)
    }

    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn settle_command(&self, sequence: u64) -> Result<(), StoreError> {
        let write = self.database.begin_write()?;
        {
            write.open_table(OUTBOX)?.remove(sequence)?;
        }
        write.commit()?;
        Ok(())
    }

    /// Records one transfer reading.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn put_sample(
        &self,
        collection_id: &[u8],
        at_unix_ns: u64,
        sample: &StoredSample,
    ) -> Result<(), StoreError> {
        self.put(
            SAMPLES,
            &schema::keyed(collection_id, at_unix_ns),
            &sample.encode(),
        )
    }

    /// One collection's transfer history, oldest first.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the read fails or a row is malformed.
    pub fn samples(&self, collection_id: &[u8]) -> Result<Vec<(u64, StoredSample)>, StoreError> {
        let (low, high) = schema::range_of(collection_id);
        let read = self.database.begin_read()?;
        let table = read.open_table(SAMPLES)?;
        let mut samples = Vec::new();
        for row in table.range(low.as_slice()..=high.as_slice())? {
            let (key, value) = row?;
            let at = schema::number_of(key.value()).ok_or(StoreError::Malformed)?;
            samples.push((at, StoredSample::decode(value.value())?));
        }
        Ok(samples)
    }

    /// Trims a collection's history to its newest `keep` readings.
    ///
    /// The history is a ring: it exists to draw a graph of the recent past,
    /// not to be a permanent record, and an untrimmed one grows for as long as
    /// a transfer runs.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the write fails.
    pub fn trim_samples(&self, collection_id: &[u8], keep: usize) -> Result<usize, StoreError> {
        let held = self.samples(collection_id)?;
        let Some(excess) = held.len().checked_sub(keep).filter(|excess| *excess > 0) else {
            return Ok(0);
        };

        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(SAMPLES)?;
            for (at, _) in &held[..excess] {
                table.remove(schema::keyed(collection_id, *at).as_slice())?;
            }
        }
        write.commit()?;
        Ok(excess)
    }

    // ----- the two operations every table above is built from -----------

    fn put(
        &self,
        table: TableDefinition<&[u8], &[u8]>,
        key: &[u8],
        value: &[u8],
    ) -> Result<(), StoreError> {
        let write = self.database.begin_write()?;
        {
            write.open_table(table)?.insert(key, value)?;
        }
        write.commit()?;
        Ok(())
    }

    fn get(
        &self,
        table: TableDefinition<&[u8], &[u8]>,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(table)?;
        Ok(table.get(key)?.map(|value| value.value().to_vec()))
    }
}

#[cfg(test)]
mod tests {
    //! Every test opens a real file in a temporary directory. A store tested
    //! against an in-memory double would prove nothing about the thing this
    //! module exists for, which is surviving a restart.

    use portalis_nexus_protocol::CONTENT_KEY_BYTES;
    use records::{EntryStatus, Role, StoredImportEntry};

    use super::*;

    const COLLECTION: [u8; 16] = [1; 16];
    const OTHER: [u8; 16] = [2; 16];
    const ROOT: [u8; 32] = [3; 32];

    /// A directory that removes itself, so tests leave no files behind.
    struct Scratch {
        path: std::path::PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "portalis-store-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self { path }
        }

        fn file(&self) -> std::path::PathBuf {
            self.path.join("portalis.redb")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn collection(name: &str) -> StoredCollection {
        StoredCollection {
            name: name.to_owned(),
            role: Role::Owner,
            content_key: [7; CONTENT_KEY_BYTES],
            media_path: "/media".to_owned(),
            sources: Vec::new(),
            paused: false,
            on_disk_bytes: 0,
            substrate_handle: None,
        }
    }

    fn sample(done: u64) -> StoredSample {
        StoredSample {
            done,
            total: 100,
            down_bytes_per_second: 10,
            up_bytes_per_second: 5,
            peers: 2,
        }
    }

    #[test]
    fn a_new_store_records_the_schema_it_was_written_with() {
        let scratch = Scratch::new("fresh");
        let store = Store::open(scratch.file()).expect("opens");

        assert_eq!(store.version().expect("reads"), u64::from(SCHEMA_VERSION));
        assert!(store.collections().expect("reads").is_empty());
    }

    /// The whole point of the module: close it, open it, and everything is
    /// still there.
    #[test]
    fn everything_written_survives_a_restart() {
        let scratch = Scratch::new("restart");

        {
            let store = Store::open(scratch.file()).expect("opens");
            store
                .put_collection(&COLLECTION, &collection("Iceland"))
                .expect("writes");
            store
                .put_revision(&COLLECTION, 1, b"revision one")
                .expect("writes");
            store
                .put_revision(&COLLECTION, 2, b"revision two")
                .expect("writes");
            store.put_manifest(&[9; 32], b"manifest").expect("writes");
            store
                .put_entry(
                    &[4; 20],
                    &StoredEntry {
                        status: EntryStatus::Available,
                        descriptor: b"torrent".to_vec(),
                    },
                )
                .expect("writes");
            store
                .put_sample(&COLLECTION, 10, &sample(1))
                .expect("writes");
            store
                .put_sample(&COLLECTION, 20, &sample(2))
                .expect("writes");
            store.put_identity("root", &ROOT).expect("writes");
            store.queue_command(1, b"publish").expect("writes");
        }

        let store = Store::open(scratch.file()).expect("reopens");

        assert_eq!(
            store.collection(&COLLECTION).expect("reads"),
            Some(collection("Iceland"))
        );
        assert_eq!(
            store.current_revision(&COLLECTION).expect("reads"),
            Some((2, b"revision two".to_vec())),
            "the highest number is the current one"
        );
        assert_eq!(store.revisions(&COLLECTION).expect("reads").len(), 2);
        assert_eq!(
            store.manifest(&[9; 32]).expect("reads"),
            Some(b"manifest".to_vec())
        );
        assert_eq!(
            store.entry(&[4; 20]).expect("reads").map(|e| e.status),
            Some(EntryStatus::Available)
        );
        assert_eq!(
            store.samples(&COLLECTION).expect("reads"),
            vec![(10, sample(1)), (20, sample(2))],
            "including the transfer history"
        );
        assert_eq!(store.identity("root").expect("reads"), Some(ROOT.to_vec()));
        assert_eq!(
            store.queued_commands().expect("reads"),
            vec![(1, b"publish".to_vec())]
        );
    }

    /// The gate: a store a newer build wrote is refused rather than read with
    /// the wrong assumptions.
    #[test]
    fn a_store_from_the_future_refuses_to_open() {
        let scratch = Scratch::new("future");
        {
            let store = Store::open(scratch.file()).expect("opens");
            let write = store.database.begin_write().expect("writes");
            {
                write
                    .open_table(META)
                    .expect("meta")
                    .insert(SCHEMA_VERSION_KEY, u64::from(SCHEMA_VERSION) + 1)
                    .expect("bumps the version");
            }
            write.commit().expect("commits");
        }

        let refused = Store::open(scratch.file()).expect_err("must refuse");

        assert!(
            matches!(
                refused,
                StoreError::FromTheFuture { found } if found == SCHEMA_VERSION + 1
            ),
            "got {refused:?}"
        );
        assert!(
            refused.to_string().contains("upgrade"),
            "and says what to do about it: {refused}"
        );
    }

    /// A store at the version before this one opens and is brought forward.
    /// With one schema released there is no earlier fixture to load, so this
    /// exercises the same path by writing the version a predecessor would
    /// have left behind.
    #[test]
    fn a_store_from_an_older_schema_is_brought_forward() {
        let scratch = Scratch::new("older");
        {
            let store = Store::open(scratch.file()).expect("opens");
            store
                .put_collection(&COLLECTION, &collection("from before"))
                .expect("writes");
            let write = store.database.begin_write().expect("writes");
            {
                write
                    .open_table(META)
                    .expect("meta")
                    .insert(SCHEMA_VERSION_KEY, 0_u64)
                    .expect("rewinds the version");
            }
            write.commit().expect("commits");
        }

        let store = Store::open(scratch.file()).expect("opens an older store");

        assert_eq!(store.version().expect("reads"), u64::from(SCHEMA_VERSION));
        assert_eq!(
            store.collection(&COLLECTION).expect("reads"),
            Some(collection("from before")),
            "and nothing was lost bringing it forward"
        );
    }

    /// Keys are sequential and reused, so anything left under a forgotten
    /// collection's key becomes the next collection's. A torrent added after a
    /// delete opened with a transfer chart already drawn for a download it had
    /// never done.
    #[test]
    fn forgetting_a_collection_forgets_what_was_recorded_under_it() {
        let scratch = Scratch::new("forget-scoped");
        let store = Store::open(scratch.file()).expect("opens");

        store.put_revision(&COLLECTION, 1, b"ours").expect("writes");
        store.put_sample(&COLLECTION, 1, &sample(1)).expect("writes");
        store.put_revision(&OTHER, 1, b"theirs").expect("writes");
        store.put_sample(&OTHER, 1, &sample(2)).expect("writes");

        store.forget_collection(&COLLECTION).expect("forgets");

        assert!(store.collection(&COLLECTION).expect("reads").is_none());
        assert!(store.samples(&COLLECTION).expect("reads").is_empty());
        assert_eq!(store.current_revision(&COLLECTION).expect("reads"), None);
        // The neighbour is untouched: one key's range, not the whole table.
        assert_eq!(store.samples(&OTHER).expect("reads").len(), 1);
        assert!(store.current_revision(&OTHER).expect("reads").is_some());
    }

    #[test]
    fn revisions_and_samples_are_scoped_to_their_collection() {
        let scratch = Scratch::new("scoped");
        let store = Store::open(scratch.file()).expect("opens");

        store.put_revision(&COLLECTION, 1, b"ours").expect("writes");
        store.put_revision(&OTHER, 1, b"theirs").expect("writes");
        store
            .put_sample(&COLLECTION, 1, &sample(1))
            .expect("writes");
        store.put_sample(&OTHER, 1, &sample(9)).expect("writes");

        assert_eq!(
            store.current_revision(&COLLECTION).expect("reads"),
            Some((1, b"ours".to_vec()))
        );
        assert_eq!(
            store.current_revision(&OTHER).expect("reads"),
            Some((1, b"theirs".to_vec()))
        );
        assert_eq!(store.samples(&COLLECTION).expect("reads").len(), 1);
        assert_eq!(
            store.samples(&OTHER).expect("reads")[0].1.done,
            9,
            "one collection's history is not another's"
        );
    }

    /// Big-endian keys are why this holds: written out of order, read back in
    /// numeric order, and the highest is genuinely the highest.
    #[test]
    fn the_current_revision_is_the_highest_not_the_last_written() {
        let scratch = Scratch::new("highest");
        let store = Store::open(scratch.file()).expect("opens");

        for number in [3_u64, 1, 256, 2, 255] {
            store
                .put_revision(&COLLECTION, number, format!("r{number}").as_bytes())
                .expect("writes");
        }

        assert_eq!(
            store.current_revision(&COLLECTION).expect("reads"),
            Some((256, b"r256".to_vec()))
        );
        let numbers: Vec<_> = store
            .revisions(&COLLECTION)
            .expect("reads")
            .into_iter()
            .map(|(number, _)| number)
            .collect();
        assert_eq!(numbers, [1, 2, 3, 255, 256]);
    }

    #[test]
    fn a_collection_that_was_never_stored_is_absent_rather_than_an_error() {
        let scratch = Scratch::new("absent");
        let store = Store::open(scratch.file()).expect("opens");

        assert_eq!(store.collection(&COLLECTION).expect("reads"), None);
        assert_eq!(store.current_revision(&COLLECTION).expect("reads"), None);
        assert!(store.revisions(&COLLECTION).expect("reads").is_empty());
        assert_eq!(store.manifest(&[0; 32]).expect("reads"), None);
        assert_eq!(store.entry(&[0; 20]).expect("reads"), None);
        assert_eq!(store.contact(&ROOT).expect("reads"), None);
        assert_eq!(store.identity("missing").expect("reads"), None);
        assert!(store.device_log(&ROOT).expect("reads").is_empty());
        assert!(store.samples(&COLLECTION).expect("reads").is_empty());
    }

    #[test]
    fn a_collection_can_be_replaced_and_forgotten() {
        let scratch = Scratch::new("forget");
        let store = Store::open(scratch.file()).expect("opens");

        store
            .put_collection(&COLLECTION, &collection("first"))
            .expect("writes");
        store
            .put_collection(&COLLECTION, &collection("renamed"))
            .expect("writes");
        assert_eq!(
            store
                .collection(&COLLECTION)
                .expect("reads")
                .map(|c| c.name),
            Some("renamed".to_owned())
        );
        assert_eq!(store.collections().expect("reads").len(), 1);

        store.forget_collection(&COLLECTION).expect("forgets");
        assert_eq!(store.collection(&COLLECTION).expect("reads"), None);
        // Forgetting something absent is not an error: the end state is what
        // was asked for either way.
        store.forget_collection(&COLLECTION).expect("forgets again");
    }

    #[test]
    fn a_torrent_source_belongs_to_its_collection_and_can_be_forgotten() {
        let scratch = Scratch::new("torrent-import");
        let store = Store::open(scratch.file()).expect("opens");

        store
            .put_torrent_import(&COLLECTION, "magnet:?xt=urn:btih:abc")
            .expect("stores");
        assert_eq!(
            store.torrent_import(&COLLECTION).expect("reads"),
            Some("magnet:?xt=urn:btih:abc".to_owned())
        );
        assert_eq!(store.torrent_import(&OTHER).expect("reads"), None);

        let entries = vec![
            StoredImportEntry {
                label: "cover.jpg".to_owned(),
                bytes: 12,
                selected: true,
            },
            StoredImportEntry {
                label: "episode.mp4".to_owned(),
                bytes: 34,
                selected: false,
            },
        ];
        store
            .put_torrent_import_entries(&COLLECTION, &entries)
            .expect("stores metadata");
        store
            .put_torrent_import_descriptor(&COLLECTION, b"torrent descriptor")
            .expect("stores descriptor");
        assert_eq!(
            store
                .torrent_import_entries(&COLLECTION)
                .expect("reads metadata"),
            entries
        );
        assert_eq!(
            store
                .torrent_import_descriptor(&COLLECTION)
                .expect("reads descriptor"),
            Some(b"torrent descriptor".to_vec())
        );

        store.forget_torrent_import(&COLLECTION).expect("forgets");
        assert_eq!(store.torrent_import(&COLLECTION).expect("reads"), None);
        assert!(store
            .torrent_import_entries(&COLLECTION)
            .expect("reads metadata")
            .is_empty());
        assert_eq!(
            store
                .torrent_import_descriptor(&COLLECTION)
                .expect("reads descriptor"),
            None
        );
    }

    #[test]
    fn a_device_log_is_read_back_in_sequence_order() {
        let scratch = Scratch::new("log");
        let store = Store::open(scratch.file()).expect("opens");

        for sequence in [2_u64, 1, 3] {
            store
                .put_log_entry(&ROOT, sequence, format!("entry {sequence}").as_bytes())
                .expect("writes");
        }

        assert_eq!(
            store.device_log(&ROOT).expect("reads"),
            vec![
                b"entry 1".to_vec(),
                b"entry 2".to_vec(),
                b"entry 3".to_vec()
            ]
        );
        assert!(store.device_log(&[9; 32]).expect("reads").is_empty());
    }

    #[test]
    fn a_contact_is_stored_under_its_root_key() {
        let scratch = Scratch::new("contact");
        let store = Store::open(scratch.file()).expect("opens");
        let contact = StoredContact {
            handle: "ada#7Q2XZ".to_owned(),
            fingerprint_verified: true,
            root_key: ROOT,
        };

        store.put_contact(&contact).expect("writes");

        assert_eq!(store.contact(&ROOT).expect("reads"), Some(contact));
    }

    #[test]
    fn the_outbox_keeps_order_and_empties_as_commands_settle() {
        let scratch = Scratch::new("outbox");
        let store = Store::open(scratch.file()).expect("opens");

        for sequence in [3_u64, 1, 2] {
            store
                .queue_command(sequence, format!("command {sequence}").as_bytes())
                .expect("writes");
        }
        assert_eq!(
            store
                .queued_commands()
                .expect("reads")
                .into_iter()
                .map(|(sequence, _)| sequence)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );

        store.settle_command(2).expect("settles");
        assert_eq!(store.queued_commands().expect("reads").len(), 2);
        for sequence in [1, 3] {
            store.settle_command(sequence).expect("settles");
        }
        assert!(store.queued_commands().expect("reads").is_empty());
    }

    /// The history is a ring, not a permanent record: an untrimmed one grows
    /// for as long as a transfer runs.
    #[test]
    fn the_sample_history_is_trimmed_to_the_newest_readings() {
        let scratch = Scratch::new("trim");
        let store = Store::open(scratch.file()).expect("opens");

        for at in 1..=10_u64 {
            store
                .put_sample(&COLLECTION, at, &sample(at))
                .expect("writes");
        }

        assert_eq!(store.trim_samples(&COLLECTION, 4).expect("trims"), 6);
        let kept: Vec<_> = store
            .samples(&COLLECTION)
            .expect("reads")
            .into_iter()
            .map(|(at, _)| at)
            .collect();
        assert_eq!(kept, [7, 8, 9, 10], "the newest survive");

        // Trimming to more than is held removes nothing, and is not an error.
        assert_eq!(store.trim_samples(&COLLECTION, 99).expect("trims"), 0);
        assert_eq!(store.trim_samples(&COLLECTION, 4).expect("trims"), 0);
        assert_eq!(store.samples(&COLLECTION).expect("reads").len(), 4);
    }

    #[test]
    fn a_damaged_row_is_reported_rather_than_guessed_at() {
        let scratch = Scratch::new("damaged");
        let store = Store::open(scratch.file()).expect("opens");

        // Something wrote nonsense where a collection should be.
        store
            .put(COLLECTIONS, &COLLECTION, b"not a collection")
            .expect("writes");

        assert!(matches!(
            store.collection(&COLLECTION),
            Err(StoreError::Malformed)
        ));
        assert!(matches!(store.collections(), Err(StoreError::Malformed)));

        store.put(ENTRIES, &[4; 20], &[]).expect("writes");
        assert!(matches!(store.entry(&[4; 20]), Err(StoreError::Malformed)));

        store.put(CONTACTS, &ROOT, b"x").expect("writes");
        assert!(matches!(store.contact(&ROOT), Err(StoreError::Malformed)));

        store
            .put(SAMPLES, &schema::keyed(&COLLECTION, 1), b"x")
            .expect("writes");
        assert!(matches!(
            store.samples(&COLLECTION),
            Err(StoreError::Malformed)
        ));
    }

    /// A key too short to carry a number cannot have been written by
    /// `keyed`, so reading one means the file has been tampered with.
    #[test]
    fn a_key_without_a_number_is_reported_as_damage() {
        let scratch = Scratch::new("shortkey");
        let store = Store::open(scratch.file()).expect("opens");

        store.put(REVISIONS, b"short", b"revision").expect("writes");
        store
            .put(SAMPLES, b"short", &sample(1).encode())
            .expect("writes");

        assert!(matches!(
            store.current_revision(b"shor"),
            Err(StoreError::Malformed)
        ));
        assert!(matches!(
            store.revisions(b"shor"),
            Err(StoreError::Malformed)
        ));
        assert!(matches!(store.samples(b"shor"), Err(StoreError::Malformed)));
    }

    #[test]
    fn a_path_that_cannot_be_opened_is_reported_as_a_database_failure() {
        let scratch = Scratch::new("unopenable");
        // A directory is not a database file.
        let failed = Store::open(&scratch.path).expect_err("must fail");

        assert!(matches!(failed, StoreError::Database(_)), "got {failed:?}");
        assert!(!failed.to_string().is_empty());
    }
}
