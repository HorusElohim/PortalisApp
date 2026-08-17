//! Sealed content keys, waiting for the device they were sealed to.
//!
//! One row per collection and recipient device, replaced rather than appended:
//! a rotated key and a retried push both land the same way, where appending
//! would leave a device to guess which of several is current.
//!
//! The service cannot read any of these. It holds a ciphertext and the
//! ephemeral public key needed to open it, addressed to a device — which is
//! why a page of them can be served without any judgement about who should
//! have it. Only the holder of that device's secret gets anything out of one.

use redb::TableDefinition;

use portalis_nexus_protocol::MAX_KEY_ENVELOPES_PER_PAGE;
use portalis_nexus_server_core::{DeviceId, KeyEnvelopePage, KeyEnvelopeRecord, ShareId};

use crate::StorageError;
use crate::store::{Store, decode, encode, pair, prefix_range};

/// Sealed keys. Key: device ‖ collection, so one device's are contiguous and
/// ordered by collection — which is the order a page walks.
const ENVELOPES: TableDefinition<&[u8], &str> = TableDefinition::new("envelopes");

/// The sealed-key endpoint.
#[derive(Debug)]
pub struct Envelopes {
    store: Store,
}

impl Envelopes {
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
            write.open_table(ENVELOPES)?;
            Ok(())
        })?;
        Ok(Self { store })
    }

    /// Stores an envelope, replacing any earlier one for the same collection
    /// and device.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the write fails.
    pub fn put(&self, envelope: &KeyEnvelopeRecord) -> Result<(), StorageError> {
        self.store.transact(|write| {
            write.open_table(ENVELOPES)?.insert(
                pair(&envelope.recipient_device_id, &envelope.share_id).as_slice(),
                encode(envelope)?.as_str(),
            )?;
            Ok(())
        })
    }

    /// One bounded page addressed to a device, ordered by collection.
    ///
    /// `after` is the last collection of the previous page, exclusive. Paging
    /// by key rather than by offset means a page is stable while envelopes are
    /// being written, which they are.
    ///
    /// # Errors
    /// Returns [`StorageError`] when the read fails or a row is malformed.
    pub fn page(
        &self,
        device: DeviceId,
        after: Option<ShareId>,
    ) -> Result<KeyEnvelopePage, StorageError> {
        let read = self.store.read()?;
        let table = read.open_table(ENVELOPES)?;
        let (low, high) = prefix_range(device.as_slice());
        let low = after.map_or(low, |share| {
            // Exclusive: one past the last collection of the previous page.
            let mut key = pair(&device, &share);
            key.push(0);
            key
        });

        let mut envelopes = Vec::new();
        let mut next_after_share_id = None;
        for row in table.range(low.as_slice()..=high.as_slice())? {
            let (_, value) = row?;
            if envelopes.len() == MAX_KEY_ENVELOPES_PER_PAGE {
                // There is more, and the caller is told where to resume rather
                // than being handed an unbounded answer.
                next_after_share_id = envelopes
                    .last()
                    .map(|envelope: &KeyEnvelopeRecord| envelope.share_id);
                break;
            }
            envelopes.push(decode(value.value())?);
        }
        Ok(KeyEnvelopePage {
            envelopes,
            next_after_share_id,
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
                "portalis-envelopes-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn open(&self) -> Envelopes {
            Envelopes::open(self.0.join("envelopes.redb")).expect("opens")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const MIRA: DeviceId = [1; 32];

    fn envelope(share: u8) -> KeyEnvelopeRecord {
        KeyEnvelopeRecord {
            share_id: [share; 16],
            recipient_device_id: MIRA,
            ephemeral_public_key: [2; 32],
            ciphertext: vec![share; 8],
            created_at_unix_ns: 1,
        }
    }

    /// A page is bounded, and says where to resume rather than handing back
    /// everything a device has ever been sent.
    #[test]
    fn a_device_with_more_than_a_page_is_told_where_to_resume() {
        let scratch = Scratch::new("paging");
        let store = scratch.open();
        let total = MAX_KEY_ENVELOPES_PER_PAGE + 10;
        for share in 0..total {
            store
                .put(&KeyEnvelopeRecord {
                    share_id: {
                        let mut id = [0_u8; 16];
                        id[..8].copy_from_slice(&(share as u64).to_be_bytes());
                        id
                    },
                    ..envelope(0)
                })
                .expect("stores");
        }

        let first = store.page(MIRA, None).expect("reads");
        assert_eq!(first.envelopes.len(), MAX_KEY_ENVELOPES_PER_PAGE);
        let cursor = first
            .next_after_share_id
            .expect("there is more, and it says so");

        // Resuming by key rather than offset, so the page is stable while
        // envelopes are still being written.
        let second = store.page(MIRA, Some(cursor)).expect("reads");
        assert_eq!(second.envelopes.len(), 10);
        assert_eq!(second.next_after_share_id, None);
        assert!(
            second.envelopes.iter().all(|held| held.share_id > cursor),
            "the cursor is exclusive"
        );
    }

    #[test]
    fn a_page_after_the_last_one_is_empty_rather_than_an_error() {
        let scratch = Scratch::new("past-the-end");
        let store = scratch.open();
        store.put(&envelope(1)).expect("stores");

        let page = store.page(MIRA, Some([1; 16])).expect("reads");

        assert!(page.envelopes.is_empty());
        assert_eq!(page.next_after_share_id, None);
    }
}
