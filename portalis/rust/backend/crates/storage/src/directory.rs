//! Device logs, stored and served.
//!
//! `SPEC.md` §9: the service stores and serves these; the client verifies
//! them. That division is the whole reason a directory is safe to run. A
//! hostile service can serve an old log, a truncated one, or nothing at all —
//! and every one of those is something `DeviceLog::adopt` refuses. What it
//! cannot do is invent a device, because it cannot sign.
//!
//! So this module has no rules in it. It appends what it is given and hands
//! back what it holds, and the interesting behaviour lives on the reading
//! side, where it can be checked against a key.
//!
//! One thing it does enforce, and only because it is arithmetic rather than
//! judgement: an entry cannot overwrite one already stored at its sequence.
//! Rewriting history is not something a legitimate publisher does, and a store
//! that permits it turns a bug in one client into a fork for everybody.

use redb::{ReadableTable, TableDefinition};

use crate::StorageError;
use crate::embedded::{Embedded, keyed, prefix_range};

/// Verified log entries, by owner. Key: root key ‖ sequence, big-endian.
pub(crate) const DEVICE_LOGS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("device_logs");

/// How many entries one person's log may hold (§ limits).
pub const MAX_ENTRIES: usize = 512;

impl Embedded {
    /// Appends entries to a person's log.
    ///
    /// Takes encoded entries rather than parsed ones: the service does not
    /// verify a log and must not appear to. Whether these follow one another
    /// is the reader's question, asked against a key this service does not
    /// have.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Conflict`] when an entry would overwrite one
    /// already stored, or the log would exceed its limit.
    pub fn publish_log(
        &self,
        root_key: &[u8],
        entries: &[(u64, Vec<u8>)],
    ) -> Result<usize, StorageError> {
        self.transact(|write| {
            let mut logs = write.open_table(DEVICE_LOGS)?;
            let (low, high) = prefix_range(root_key);
            let held = logs.range(low.as_slice()..=high.as_slice())?.count();
            if held + entries.len() > MAX_ENTRIES {
                return Err(StorageError::Conflict);
            }

            let mut appended = 0;
            for (sequence, entry) in entries {
                let key = keyed(root_key, *sequence);
                // An identical entry arriving twice is a retry, and answering
                // it as success is what lets a publisher whose acknowledgement
                // was lost try again. A *different* entry at the same sequence
                // is a rewrite, which is refused.
                if let Some(stored) = logs.get(key.as_slice())? {
                    if stored.value() == entry.as_slice() {
                        continue;
                    }
                    return Err(StorageError::Conflict);
                }
                logs.insert(key.as_slice(), entry.as_slice())?;
                appended += 1;
            }
            Ok(appended)
        })
    }

    /// One person's log, in sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the read fails.
    pub fn fetch_log(&self, root_key: &[u8]) -> Result<Vec<(u64, Vec<u8>)>, StorageError> {
        let read = self.begin_read()?;
        let logs = read.open_table(DEVICE_LOGS)?;
        let (low, high) = prefix_range(root_key);

        let mut entries = Vec::new();
        for row in logs.range(low.as_slice()..=high.as_slice())? {
            let (key, value) = row?;
            let sequence = key
                .value()
                .get(root_key.len()..)
                .and_then(|tail| <[u8; 8]>::try_from(tail).ok())
                .map(u64::from_be_bytes)
                .ok_or(StorageError::Malformed)?;
            entries.push((sequence, value.value().to_vec()));
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "portalis-directory-{name}-{}-{:?}",
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

    const ADA: &[u8] = &[1; 32];
    const MIRA: &[u8] = &[2; 32];

    fn entries(count: u64) -> Vec<(u64, Vec<u8>)> {
        (1..=count)
            .map(|sequence| (sequence, format!("entry {sequence}").into_bytes()))
            .collect()
    }

    #[test]
    fn a_log_is_appended_and_read_back_in_order() {
        let scratch = Scratch::new("append");
        let store = scratch.open();

        assert_eq!(store.publish_log(ADA, &entries(3)).expect("publishes"), 3);

        assert_eq!(store.fetch_log(ADA).expect("reads"), entries(3));
        assert!(store.fetch_log(MIRA).expect("reads").is_empty());
    }

    /// Out of order in, in order out: the store does not care how a publisher
    /// batched them, and a reader needs them in sequence.
    #[test]
    fn entries_come_back_in_sequence_whatever_order_they_arrived() {
        let scratch = Scratch::new("order");
        let store = scratch.open();

        store
            .publish_log(ADA, &[(3, b"three".to_vec()), (1, b"one".to_vec())])
            .expect("publishes");
        store
            .publish_log(ADA, &[(2, b"two".to_vec())])
            .expect("publishes the gap");

        assert_eq!(
            store
                .fetch_log(ADA)
                .expect("reads")
                .into_iter()
                .map(|(sequence, _)| sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    /// A publisher whose acknowledgement was lost republishes the same bytes.
    /// That is the same entry, not a competing one.
    #[test]
    fn an_identical_entry_arriving_twice_is_a_retry() {
        let scratch = Scratch::new("retry");
        let store = scratch.open();
        store.publish_log(ADA, &entries(2)).expect("publishes");

        assert_eq!(
            store.publish_log(ADA, &entries(2)).expect("republishes"),
            0,
            "nothing new was appended, and nothing failed"
        );
        assert_eq!(store.fetch_log(ADA).expect("reads").len(), 2);
    }

    /// The one thing this module refuses, and only because it is arithmetic
    /// rather than judgement.
    #[test]
    fn a_different_entry_at_a_taken_sequence_is_refused() {
        let scratch = Scratch::new("rewrite");
        let store = scratch.open();
        store.publish_log(ADA, &entries(2)).expect("publishes");

        assert!(matches!(
            store.publish_log(ADA, &[(2, b"something else".to_vec())]),
            Err(StorageError::Conflict)
        ));
        assert_eq!(
            store.fetch_log(ADA).expect("reads"),
            entries(2),
            "and the attempt changed nothing"
        );
    }

    #[test]
    fn a_log_is_bounded() {
        let scratch = Scratch::new("bounded");
        let store = scratch.open();

        store
            .publish_log(ADA, &entries(MAX_ENTRIES as u64))
            .expect("exactly at the limit");
        assert!(matches!(
            store.publish_log(ADA, &[(9_999, b"one too many".to_vec())]),
            Err(StorageError::Conflict)
        ));
    }

    #[test]
    fn one_persons_log_is_not_anothers() {
        let scratch = Scratch::new("scoped");
        let store = scratch.open();

        store.publish_log(ADA, &entries(2)).expect("publishes");
        store.publish_log(MIRA, &entries(1)).expect("publishes");

        assert_eq!(store.fetch_log(ADA).expect("reads").len(), 2);
        assert_eq!(store.fetch_log(MIRA).expect("reads").len(), 1);
    }

    #[test]
    fn a_log_survives_a_restart() {
        let scratch = Scratch::new("restart");
        {
            scratch
                .open()
                .publish_log(ADA, &entries(2))
                .expect("publishes");
        }

        assert_eq!(scratch.open().fetch_log(ADA).expect("reads"), entries(2));
    }
}
