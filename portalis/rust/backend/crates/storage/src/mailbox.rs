//! What a device missed while it was asleep.
//!
//! `SPEC.md` §13 and §20.1. When a peer is reachable, objects go straight to
//! it and none of this is involved. When it is not — a phone in a pocket, a
//! laptop shut — they wait here until it connects.
//!
//! The service can hold these because it cannot read them. An item is an
//! opaque blob addressed to a device identifier, and the whole of what the
//! service knows is that somebody has something for somebody. That is why the
//! mailbox can be a dumb queue rather than anything that understands
//! collections.
//!
//! Three rules, and each one is a way of not becoming a liability:
//!
//! **Bounded per device.** 4 096 items and 64 MiB (§ limits). A mailbox that
//! grows without limit is a device that never came back turning into an
//! operator's disk bill, and a queue nobody drains is indistinguishable from
//! an attack.
//!
//! **Ordered, and drained rather than read.** Items come back oldest first,
//! and collecting them is what removes them. A mailbox that had to be told
//! what to delete is a mailbox that fills up when a client crashes between
//! reading and acknowledging.
//!
//! **Refused, not silently dropped, when full.** A sender that cannot deliver
//! needs to know, because the alternative is a member who never receives a
//! revision and never finds out why.

use redb::{ReadableTable, TableDefinition};

use portalis_nexus_server_core::DeviceId;

use crate::StorageError;
use crate::store::{Store, keyed, number_of, prefix_range};

/// Items waiting for one device. Key: device ‖ sequence, big-endian.
const ITEMS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("items");
/// The next sequence to use for a device, so two deliveries cannot collide.
const NEXT: TableDefinition<&[u8], u64> = TableDefinition::new("next");
/// What each device's mailbox currently holds: items, then bytes.
///
/// Kept rather than counted. Scanning a mailbox to find out whether one more
/// item fits makes every delivery cost the depth of the queue, so a mailbox
/// nobody drains gets slower exactly as it gets fuller — which is the moment
/// it most needs to stay cheap.
const HELD: TableDefinition<&[u8], (u64, u64)> = TableDefinition::new("held");

/// How many items one device may have waiting.
pub const MAX_ITEMS: usize = 4_096;
/// How much one device's mailbox may hold in total.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// What one device's mailbox may hold.
///
/// Configurable because an operator running this for a family and one running
/// it for a thousand people want different answers — and because a test that
/// must write 64 MiB to reach a boundary is a test that stops being run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub items: usize,
    pub bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            items: MAX_ITEMS,
            bytes: MAX_BYTES,
        }
    }
}

/// One thing waiting for a device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Where it sits in the queue. Returned so a caller can say what it
    /// collected, not so it can ask for one.
    pub sequence: u64,
    /// Opaque. The service does not know what this is and must not need to.
    pub body: Vec<u8>,
}

/// The mailbox endpoint.
#[derive(Debug)]
pub struct Mailbox {
    store: Store,
    limits: Limits,
}

impl Mailbox {
    /// Opens this endpoint's file with the standard limits.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the file cannot be opened or prepared.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        Self::with_limits(path, Limits::default())
    }

    /// Opens it with limits other than the standard ones.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the file cannot be opened or prepared.
    pub fn with_limits(
        path: impl AsRef<std::path::Path>,
        limits: Limits,
    ) -> Result<Self, StorageError> {
        let store = Store::open(path)?;
        store.declare(|write| {
            write.open_table(ITEMS)?;
            write.open_table(NEXT)?;
            write.open_table(HELD)?;
            Ok(())
        })?;
        Ok(Self { store, limits })
    }

    /// Leaves `body` for `device`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::MailboxFull`] when the device's mailbox is
    /// full, by count or by size, and says which. A sender that cannot deliver
    /// is told so, because the alternative is a member who never receives a
    /// revision and never learns why.
    pub fn deliver(&self, device: DeviceId, body: &[u8]) -> Result<u64, StorageError> {
        let limits = self.limits;
        self.store.transact(|write| {
            let mut held = write.open_table(HELD)?;
            let (count, bytes) = held
                .get(device.as_slice())?
                .map_or((0, 0), |stored| stored.value());
            let count = usize::try_from(count).map_err(|_| StorageError::Malformed)?;
            let bytes = usize::try_from(bytes).map_err(|_| StorageError::Malformed)?;

            if count >= limits.items {
                return Err(StorageError::MailboxFull {
                    held: count,
                    limit: limits.items,
                    unit: "items",
                });
            }
            let would_hold = bytes.saturating_add(body.len());
            if would_hold > limits.bytes {
                return Err(StorageError::MailboxFull {
                    held: would_hold,
                    limit: limits.bytes,
                    unit: "bytes",
                });
            }

            // A monotonic sequence per device, kept rather than derived from
            // the queue: deriving it from the last item would reuse numbers
            // after a drain, and two items with one number is a lost item.
            let mut next = write.open_table(NEXT)?;
            let sequence = next.get(device.as_slice())?.map_or(1, |at| at.value());
            next.insert(device.as_slice(), sequence + 1)?;
            write
                .open_table(ITEMS)?
                .insert(keyed(&device, sequence).as_slice(), body)?;
            held.insert(device.as_slice(), (count as u64 + 1, would_hold as u64))?;
            Ok(sequence)
        })
    }

    /// How many items and bytes are waiting for a device.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the read fails.
    pub fn size(&self, device: DeviceId) -> Result<(usize, usize), StorageError> {
        let read = self.store.read()?;
        let held = read.open_table(HELD)?;
        let (count, bytes) = held
            .get(device.as_slice())?
            .map_or((0, 0), |stored| stored.value());
        Ok((
            usize::try_from(count).map_err(|_| StorageError::Malformed)?,
            usize::try_from(bytes).map_err(|_| StorageError::Malformed)?,
        ))
    }

    /// Takes everything waiting for `device`, oldest first, and removes it.
    ///
    /// Draining and reading are the same operation on purpose. A mailbox that
    /// had to be told what to delete fills up whenever a client dies between
    /// reading and acknowledging, and that client is a phone.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the read or the write fails.
    pub fn drain(&self, device: DeviceId) -> Result<Vec<Item>, StorageError> {
        self.store.transact(|write| {
            let mut items_table = write.open_table(ITEMS)?;
            let (low, high) = prefix_range(device.as_slice());

            let mut items = Vec::new();
            for row in items_table.range(low.as_slice()..=high.as_slice())? {
                let (key, value) = row?;
                items.push(Item {
                    sequence: number_of(key.value(), device.len())?,
                    body: value.value().to_vec(),
                });
            }
            for item in &items {
                items_table.remove(keyed(&device, item.sequence).as_slice())?;
            }
            // Emptied in the same transaction that emptied the queue, so the
            // two cannot disagree about how full it is.
            write.open_table(HELD)?.remove(device.as_slice())?;
            Ok(items)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "portalis-mailbox-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn open(&self) -> Mailbox {
            Mailbox::open(self.0.join("mailbox.redb")).expect("opens")
        }

        /// Small limits, so a boundary is reachable without writing 64 MiB.
        fn tight(&self) -> Mailbox {
            Mailbox::with_limits(
                self.0.join("mailbox.redb"),
                Limits {
                    items: 4,
                    bytes: 64,
                },
            )
            .expect("opens")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const MIRA: DeviceId = [1; 32];
    const JONAS: DeviceId = [2; 32];

    #[test]
    fn items_come_back_oldest_first_and_are_gone_once_collected() {
        let scratch = Scratch::new("drain");
        let store = scratch.open();

        for body in [b"one".as_slice(), b"two", b"three"] {
            store.deliver(MIRA, body).expect("delivers");
        }

        let collected = store.drain(MIRA).expect("drains");
        assert_eq!(
            collected
                .iter()
                .map(|item| item.body.clone())
                .collect::<Vec<_>>(),
            vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
        );
        assert!(
            collected
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence),
            "in order"
        );
        assert!(
            store.drain(MIRA).expect("drains").is_empty(),
            "collecting is what removes them"
        );
    }

    /// Sequences never repeat, even after a drain: two items with one number
    /// is a lost item.
    #[test]
    fn a_sequence_is_never_reused_after_a_drain() {
        let scratch = Scratch::new("sequences");
        let store = scratch.open();

        let first = store.deliver(MIRA, b"before").expect("delivers");
        store.drain(MIRA).expect("drains");
        let second = store.deliver(MIRA, b"after").expect("delivers");

        assert!(second > first, "{second} must follow {first}");
    }

    #[test]
    fn one_devices_mailbox_is_not_anothers() {
        let scratch = Scratch::new("scoped");
        let store = scratch.open();

        store.deliver(MIRA, b"for Mira").expect("delivers");
        store.deliver(JONAS, b"for Jonas").expect("delivers");

        assert_eq!(store.drain(MIRA).expect("drains").len(), 1);
        assert_eq!(
            store.drain(JONAS).expect("drains")[0].body,
            b"for Jonas".to_vec(),
            "draining one did not touch the other"
        );
    }

    #[test]
    fn a_device_with_nothing_waiting_gets_an_empty_answer() {
        let scratch = Scratch::new("empty");
        let store = scratch.open();

        assert!(store.drain(MIRA).expect("drains").is_empty());
        assert_eq!(store.size(MIRA).expect("reads"), (0, 0));
    }

    /// The bound that keeps a device which never came back from becoming an
    /// operator's disk bill.
    #[test]
    fn a_full_mailbox_refuses_rather_than_dropping_silently() {
        let scratch = Scratch::new("full");
        let store = scratch.tight();

        // One item over the byte limit, in one go.
        assert!(matches!(
            store.deliver(MIRA, &[0_u8; 65]),
            Err(StorageError::MailboxFull { unit: "bytes", .. })
        ));
        assert_eq!(store.size(MIRA).expect("reads").0, 0);

        // And the same limit reached by accumulation. Boundary-minus-one,
        // boundary, boundary-plus-one, which every limit gets.
        let chunk = vec![0_u8; 32];
        store.deliver(JONAS, &chunk).expect("under");
        store.deliver(JONAS, &chunk).expect("exactly at it");
        assert_eq!(store.size(JONAS).expect("reads"), (2, 64));
        assert!(
            matches!(
                store.deliver(JONAS, b"!"),
                Err(StorageError::MailboxFull { unit: "bytes", .. })
            ),
            "a sender that cannot deliver is told so"
        );

        // Draining makes room again.
        store.drain(JONAS).expect("drains");
        store
            .deliver(JONAS, b"now there is room")
            .expect("delivers");
    }

    #[test]
    fn the_item_count_is_bounded_as_well_as_the_size() {
        let scratch = Scratch::new("count");
        let store = scratch.tight();

        for index in 0_u8..4 {
            store.deliver(MIRA, &[index]).expect("delivers");
        }

        assert_eq!(store.size(MIRA).expect("reads").0, 4);
        assert!(
            matches!(
                store.deliver(MIRA, b"!"),
                Err(StorageError::MailboxFull { unit: "items", .. })
            ),
            "the count bounds it even when the bytes would fit"
        );
    }

    /// The shipped limits are the ones §13 asks for, whatever a test uses.
    #[test]
    fn the_default_limits_are_the_specified_ones() {
        assert_eq!(
            Limits::default(),
            Limits {
                items: MAX_ITEMS,
                bytes: MAX_BYTES
            }
        );
    }

    /// The point of a mailbox: it is still there when the device comes back.
    #[test]
    fn what_is_waiting_survives_a_restart() {
        let scratch = Scratch::new("restart");
        {
            let store = scratch.open();
            store.deliver(MIRA, b"a revision").expect("delivers");
        }

        let store = scratch.open();

        let collected = store.drain(MIRA).expect("drains");
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].body, b"a revision".to_vec());
    }

    #[test]
    fn a_key_without_a_sequence_is_reported_as_damage() {
        assert!(matches!(
            number_of(b"short", 0),
            Err(StorageError::Malformed)
        ));
        assert_eq!(number_of(&keyed(&[1, 2], 7), 2).expect("reads"), 7);
    }
}
