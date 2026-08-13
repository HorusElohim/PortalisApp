//! The machinery every endpoint's file shares.
//!
//! Each endpoint owns its own database file — identity, collections, mailbox
//! and the directory are four files, not four sets of tables in one. That is a
//! deliberate change from the obvious arrangement, and the reasons are worth
//! stating because one of them is not the reason people usually give.
//!
//! **It is not about reading less.** redb reads one key at a time from one
//! table; a query never touches a table it did not name, whichever file it
//! lives in. Anyone reaching for this split to avoid reading too much has
//! already got what they wanted.
//!
//! What separate files actually buy:
//!
//! **Writes proceed in parallel.** redb allows exactly one write transaction
//! per database at a time. In one file, a mailbox delivery waits behind a
//! registration for no reason — they share nothing. In four, they do not wait.
//!
//! **A smaller blast radius.** A file that will not open takes its endpoint
//! down and no more, which for a self-hoster is the difference between "the
//! mailbox is broken" and "the service is broken".
//!
//! **Each endpoint is autonomous.** Its tables, its keys and its rules are in
//! one module with one file underneath, so it can be read, tested, and changed
//! without knowing what else exists.
//!
//! And the constraint that decides where the seams go: **a write cannot span
//! two files.** So the split follows the groups that genuinely need one
//! transaction — a user and their first device, a head and its snapshot, an
//! item and its sequence — and never cuts through one.

use std::path::Path;

use redb::Database;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::StorageError;

/// One endpoint's file, and the few operations every endpoint needs.
///
/// Composition rather than inheritance, which Rust does not have: an endpoint
/// holds one of these and adds its own tables and rules on top.
#[derive(Debug)]
pub struct Store {
    database: Database,
}

impl Store {
    /// Opens or creates `path`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Unavailable`] when the file cannot be opened,
    /// including when it holds tables of another shape.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Ok(Self {
            database: Database::create(path)?,
        })
    }

    /// Runs `work` in one transaction, committing only if it succeeds.
    ///
    /// Every write goes through here, which is what makes "both rows or
    /// neither" the default rather than something each method must remember.
    ///
    /// # Errors
    ///
    /// Whatever `work` returned, or a failure to begin or commit.
    pub fn transact<T>(
        &self,
        work: impl FnOnce(&redb::WriteTransaction) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let write = self.database.begin_write()?;
        let outcome = work(&write)?;
        write.commit()?;
        Ok(outcome)
    }

    /// Opens a read transaction.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Unavailable`] when the read cannot begin.
    pub fn read(&self) -> Result<redb::ReadTransaction, StorageError> {
        Ok(self.database.begin_read()?)
    }

    /// Creates the tables an endpoint declares, so a reader never has to tell
    /// "no table yet" from "no rows yet" — the same question, and one that was
    /// answered two ways often enough to be worth removing.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the tables cannot be created.
    pub fn declare(
        &self,
        tables: impl FnOnce(&redb::WriteTransaction) -> Result<(), StorageError>,
    ) -> Result<(), StorageError> {
        self.transact(tables)
    }
}

/// A record as it is stored.
///
/// JSON, deliberately unlike the canonical formats in `protocol`. Those are
/// hand-written because two implementations must agree byte for byte. Nothing
/// here crosses a wire or is signed — it is one process's private copy of
/// objects it cannot read — so a derived encoding costs nothing, a
/// hand-written one would be a thousand lines of opportunity to be wrong, and
/// being legible to whoever is self-hosting is a small bonus.
///
/// # Errors
///
/// Returns [`StorageError::Malformed`] for a value that will not encode.
pub fn encode<T: Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|_| StorageError::Malformed)
}

/// Reads back what [`encode`] wrote.
///
/// # Errors
///
/// Returns [`StorageError::Malformed`] for a row this build cannot read.
pub fn decode<T: DeserializeOwned>(value: &str) -> Result<T, StorageError> {
    serde_json::from_str(value).map_err(|_| StorageError::Malformed)
}

/// A key that groups every row of one owner together, ordered by the number
/// that follows.
///
/// Big-endian, which is not a style choice: it makes lexicographic order agree
/// with numeric order, so revision 256 sorts after 255 rather than between 25
/// and 26.
#[must_use]
pub fn keyed(prefix: &[u8], number: u64) -> Vec<u8> {
    let mut key = prefix.to_vec();
    key.extend_from_slice(&number.to_be_bytes());
    key
}

/// A key naming a relationship between two identifiers.
#[must_use]
pub fn pair(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut key = left.to_vec();
    key.extend_from_slice(right);
    key
}

/// The range covering every key under one prefix.
#[must_use]
pub fn prefix_range(prefix: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut high = prefix.to_vec();
    high.extend_from_slice(&[u8::MAX; 40]);
    (prefix.to_vec(), high)
}

/// The number a composite key ends with.
///
/// # Errors
///
/// Returns [`StorageError::Malformed`] for a key too short to hold one, which
/// means something wrote a key [`keyed`] did not build.
pub fn number_of(key: &[u8], prefix: usize) -> Result<u64, StorageError> {
    key.get(prefix..)
        .and_then(|tail| <[u8; 8]>::try_from(tail).ok())
        .map(u64::from_be_bytes)
        .ok_or(StorageError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_composite_key_orders_by_its_number() {
        let prefix = [7_u8; 16];

        assert!(keyed(&prefix, 1) < keyed(&prefix, 2));
        // The case big-endian exists for.
        assert!(keyed(&prefix, 255) < keyed(&prefix, 256));
        assert_eq!(
            number_of(&keyed(&prefix, 42), prefix.len()).expect("reads"),
            42
        );
        assert!(matches!(
            number_of(b"short", 0),
            Err(StorageError::Malformed)
        ));
    }

    #[test]
    fn a_range_covers_one_prefix_and_not_its_neighbour() {
        let (low, high) = prefix_range(&[7_u8; 16]);

        assert!(low <= keyed(&[7; 16], 0));
        assert!(keyed(&[7; 16], u64::MAX) <= high);
        assert!(high < keyed(&[8; 16], 0));
    }

    #[test]
    fn a_pair_names_both_halves_in_order() {
        assert_eq!(pair(&[1, 2], &[3, 4]), vec![1, 2, 3, 4]);
    }

    #[test]
    fn a_record_round_trips_and_nonsense_does_not() {
        let encoded = encode(&vec![1_u8, 2, 3]).expect("encodes");

        assert_eq!(decode::<Vec<u8>>(&encoded).expect("decodes"), vec![1, 2, 3]);
        assert!(matches!(
            decode::<Vec<u8>>("not a list"),
            Err(StorageError::Malformed)
        ));
    }

    #[test]
    fn a_path_that_cannot_be_opened_is_reported() {
        let directory = std::env::temp_dir();

        assert!(matches!(
            Store::open(&directory),
            Err(StorageError::Unavailable(_))
        ));
    }
}
