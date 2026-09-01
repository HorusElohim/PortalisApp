//! What the device keeps, and how a key is built.
//!
//! Client tables. Every table is bytes to bytes, and composite keys are built
//! here rather than by callers, because a key assembled in two
//! places is a key that will eventually be assembled two ways.
//!
//! Keys with a number in them use big-endian, which is not a style choice: it
//! makes lexicographic order agree with numeric order, so "the highest
//! revision of this collection" is the last row of a prefix range rather than
//! a full scan and a comparison.

use redb::TableDefinition;

/// Bumped whenever the shape of what is stored changes.
///
/// A store written by a newer version refuses to open (see
/// [`super::StoreError::FromTheFuture`]) rather than being read with the wrong
/// assumptions — silently misreading a user's own data is worse than declining
/// to start.
pub const SCHEMA_VERSION: u32 = 13;

/// Where the schema version itself lives.
pub const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
/// The key under which [`SCHEMA_VERSION`] is recorded.
pub const SCHEMA_VERSION_KEY: &str = "schema_version";

/// Key handles and this device's root key.
pub const IDENTITY: TableDefinition<&str, &[u8]> = TableDefinition::new("identity");
/// Verified device log entries, ours and our contacts'. Key: root ‖ sequence.
pub const DEVICE_LOG: TableDefinition<&[u8], &[u8]> = TableDefinition::new("device_log");
/// Handle, fingerprint, verification and friendship, by root key.
pub const CONTACTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("contacts");
/// Name, role, content key and local paths, by collection.
pub const COLLECTIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("collections");
/// Verified revisions. Key: collection ‖ number; the highest is the current.
pub const REVISIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("revisions");
/// Decoded manifests, by their content hash.
pub const MANIFESTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("manifests");
/// Descriptor bytes and local status, by info hash.
pub const ENTRIES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("entries");
/// A magnet URI or local `.torrent` path waiting for metadata resolution,
/// keyed by its owning Nexus collection.
pub const TORRENT_IMPORTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("torrent_imports");
/// Metadata-only selectable files, keyed by collection and ordinal.
pub const TORRENT_IMPORT_ENTRIES: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("torrent_import_entries");
/// The immutable `.torrent` descriptor once it has been resolved locally.
pub const TORRENT_IMPORT_DESCRIPTORS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("torrent_import_descriptors");
/// Commands awaiting connectivity, by sequence.
pub const OUTBOX: TableDefinition<u64, &[u8]> = TableDefinition::new("outbox");
/// The transfer history ring. Key: collection ‖ timestamp.
///
/// In Rust deliberately (D8): it is sampled from backend numbers, and keeping
/// it in Flutter made it a second source of truth re-encoded on every tick.
pub const SAMPLES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("samples");
/// Snapshot-only cumulative traffic ledgers, keyed by collection, address and
/// reported client name. This stays separate from the fast transfer ring.
pub const PEER_HISTORY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("peer_history");

/// One singleton aggregate for locally measured device activity.
pub const DEVICE_ACTIVITY: TableDefinition<&str, &[u8]> = TableDefinition::new("device_activity");

/// Bounded recent backend runs, keyed by their nanosecond start/run ID.
pub const APP_RUNS: TableDefinition<u64, &[u8]> = TableDefinition::new("app_runs");

/// A key that groups every row of one owner or collection together, ordered by
/// the number that follows.
#[must_use]
pub fn keyed(prefix: &[u8], number: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(prefix.len() + 8);
    key.extend_from_slice(prefix);
    key.extend_from_slice(&number.to_be_bytes());
    key
}

/// The half-open range covering every row under one prefix.
///
/// The upper bound is the prefix with `u64::MAX` as its number, inclusive,
/// which is exactly the last key that prefix can produce.
#[must_use]
pub fn range_of(prefix: &[u8]) -> (Vec<u8>, Vec<u8>) {
    (keyed(prefix, 0), keyed(prefix, u64::MAX))
}

/// The number a composite key ends with, if it is long enough to have one.
#[must_use]
pub fn number_of(key: &[u8]) -> Option<u64> {
    let start = key.len().checked_sub(8)?;
    <[u8; 8]>::try_from(&key[start..])
        .ok()
        .map(u64::from_be_bytes)
}

/// A collision-free key for one exact endpoint/client observation.
#[must_use]
pub fn peer_history_key(collection: &[u8], address: &str, client: Option<&str>) -> Vec<u8> {
    let mut key =
        Vec::with_capacity(collection.len() + address.len() + client.map_or(0, str::len) + 10);
    key.extend_from_slice(collection);
    key.extend_from_slice(&(address.len() as u32).to_be_bytes());
    key.extend_from_slice(address.as_bytes());
    match client {
        Some(client) => {
            key.push(1);
            key.extend_from_slice(&(client.len() as u32).to_be_bytes());
            key.extend_from_slice(client.as_bytes());
        }
        None => key.push(0),
    }
    key
}

/// Inclusive range covering every peer-history key under one collection.
#[must_use]
pub fn peer_history_range(collection: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut low = collection.to_vec();
    low.extend_from_slice(&0_u32.to_be_bytes());
    let mut high = collection.to_vec();
    high.extend_from_slice(&u32::MAX.to_be_bytes());
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_composite_key_orders_by_its_number() {
        let prefix = [7_u8; 16];

        assert!(keyed(&prefix, 1) < keyed(&prefix, 2));
        // The case big-endian exists for: byte order agreeing with value order
        // across a width boundary, where little-endian would invert it.
        assert!(keyed(&prefix, 255) < keyed(&prefix, 256));
        assert!(keyed(&prefix, u64::MAX - 1) < keyed(&prefix, u64::MAX));
    }

    #[test]
    fn a_range_covers_every_number_and_nothing_of_a_neighbour() {
        let prefix = [7_u8; 16];
        let neighbour = [8_u8; 16];
        let (low, high) = range_of(&prefix);

        assert!(low <= keyed(&prefix, 0));
        assert!(keyed(&prefix, u64::MAX) <= high);
        assert!(high < keyed(&neighbour, 0), "a neighbour is outside it");
    }

    #[test]
    fn a_number_can_be_read_back_out_of_a_key() {
        assert_eq!(number_of(&keyed(&[1, 2, 3], 42)), Some(42));
        assert_eq!(number_of(&keyed(&[], u64::MAX)), Some(u64::MAX));
        assert_eq!(number_of(&[1, 2, 3]), None, "too short to hold one");
    }

    #[test]
    fn peer_history_keys_keep_client_claims_distinct_and_scoped() {
        let collection = [7_u8; 16];
        let neighbour = [8_u8; 16];
        let without_client = peer_history_key(&collection, "203.0.113.5:6881", None);
        let qbittorrent =
            peer_history_key(&collection, "203.0.113.5:6881", Some("qBittorrent/5.2.3"));
        let transmission =
            peer_history_key(&collection, "203.0.113.5:6881", Some("Transmission/4.0.6"));

        assert_ne!(without_client, qbittorrent);
        assert_ne!(qbittorrent, transmission);
        let (low, high) = peer_history_range(&collection);
        assert!(low <= qbittorrent && qbittorrent <= high);
        let other = peer_history_key(&neighbour, "203.0.113.5:6881", Some("qBittorrent/5.2.3"));
        assert!(other < low || other > high);
    }
}
