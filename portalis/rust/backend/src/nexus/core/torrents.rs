//! Turning a pasted source into a collection that is actually downloading.
//!
//! Two transitions, both driven from durable state rather than from an
//! in-memory job, so an interrupted process resumes them by scanning rather
//! than by remembering:
//!
//! 1. **Resolve.** A collection with a torrent source and no file list yet is
//!    inspected — for a `.torrent` that reads a descriptor, for a magnet it
//!    asks the swarm. The answer names the collection and gives the person
//!    something to choose from.
//! 2. **Acquire.** A collection whose files have been chosen, and which
//!    nothing is carrying yet, starts downloading them.
//! 3. **Reconcile.** A collection something is already carrying has the
//!    stored intent — which files, and whether to move at all — asserted
//!    against the engine. Both verbs are idempotent, so this is safe to
//!    repeat and is the only thing that keeps the two from drifting apart.
//!
//! The third is what makes a choice revisable. Told once at the moment a
//! download starts and never corrected, the engine kept the first answer
//! forever: deselecting a file did nothing and reselecting one could not be
//! said at all.
//!
//! Neither step is reached by a command directly: `Nexus::command` promises
//! not to wait for a network, and both of these do. A command records the
//! intent durably and wakes this worker, which is also why closing the app
//! mid-resolve loses nothing.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use tokio::sync::{Notify, watch};

use crate::nexus::projection::state::{Handle, PortalisState};
use crate::nexus::store::Store;
use crate::nexus::store::records::{StoredCollection, StoredImportEntry, StoredLifecycle};
use crate::nexus::substrate::Substrate;

/// A failed resolve remains durable work. Back off before scanning it again so
/// a temporarily unreachable magnet can recover without a restart, while a
/// malformed source cannot hot-loop the runtime.
const RETRY_DELAYS: [std::time::Duration; 4] = [
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(15),
    std::time::Duration::from_secs(30),
    std::time::Duration::from_secs(60),
];

fn retry_delay(failures: u32) -> std::time::Duration {
    RETRY_DELAYS[usize::try_from(failures.saturating_sub(1))
        .unwrap_or(3)
        .min(3)]
}

/// One collection this worker has something to do for.
enum Pending {
    /// The source has never been resolved: no file list exists yet.
    Resolve { key: Vec<u8>, source: String },
    /// Files were chosen and nothing is carrying them yet.
    Acquire {
        key: Vec<u8>,
        source: String,
        files: Vec<usize>,
    },
    /// Something is carrying it: make the engine agree with what is stored.
    Reconcile {
        key: Vec<u8>,
        handle: String,
        paused: bool,
        /// Which files to fetch, where that is a choice at all.
        ///
        /// `None` for a collection this device published: it owns every file,
        /// so there is nothing to choose and nothing to assert. An empty list
        /// would mean the opposite — fetch none of them — which for something
        /// that is seeding is the one instruction that must never be sent by
        /// accident.
        files: Option<Vec<usize>>,
    },
}

/// Runs until shutdown, resolving sources and starting downloads.
pub(crate) async fn follow_torrent_imports(
    store: Arc<Store>,
    states: watch::Sender<PortalisState>,
    collections: Arc<Mutex<super::nexus::LocalCollections>>,
    substrate: Arc<dyn Substrate>,
    wake: Arc<Notify>,
    mut shutdown: super::supervisor::Shutdown,
    details: super::nexus::DetailSources,
) {
    let mut failures: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut retry_deadlines: HashMap<Vec<u8>, Instant> = HashMap::new();
    loop {
        let next_retry = retry_deadlines.values().min().copied();
        tokio::select! {
            () = shutdown.requested() => return,
            _ = wake.notified() => {}
            () = async {
                if let Some(deadline) = next_retry {
                    tokio::time::sleep_until(deadline.into()).await;
                }
            }, if next_retry.is_some() => {}
        }

        // Scanned rather than carried in the wake: a wake says "something
        // may have changed", never *what*, so one coalesced wake is enough
        // however many imports it covers.
        let pending = match pending_work(&store) {
            Ok(pending) => pending,
            Err(error) => {
                crate::nexus::log::clog!("nexus", "could not scan torrent imports: {error}");
                continue;
            }
        };
        let now = Instant::now();
        let due = pending
            .into_iter()
            .filter(|work| {
                let key = work_key(work);
                retry_deadlines
                    .get(&key)
                    .is_none_or(|deadline| *deadline <= now)
            })
            .collect::<Vec<_>>();
        for work in due {
            let key = work_key(&work);
            crate::nexus::log::clog!(
                "nexus",
                "torrent worker performing correlation={} key={:?} {}",
                correlation_id(&key),
                key,
                match &work {
                    Pending::Resolve { source, .. } => {
                        format!("resolve source_len={}", source.len())
                    }
                    Pending::Acquire { source, files, .. } => {
                        format!("acquire source_len={} files={files:?}", source.len())
                    }
                    Pending::Reconcile { handle, files, .. } => {
                        format!("reconcile handle={handle} files={files:?}")
                    }
                }
            );
            let done = tokio::select! {
                () = shutdown.requested() => return,
                done = perform(&store, substrate.as_ref(), &work) => done,
            };
            if let Err(error) = done {
                let count = failures
                    .entry(key.clone())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                let delay = retry_delay(*count);
                retry_deadlines.insert(key.clone(), Instant::now() + delay);
                crate::nexus::log::clog!(
                    "nexus",
                    "torrent retry correlation={} failure={} after={}s: {error:#}",
                    correlation_id(&key),
                    *count,
                    delay.as_secs()
                );
                if matches!(work, Pending::Resolve { .. }) {
                    let status = if error.to_string().to_ascii_lowercase().contains("timeout") {
                        crate::nexus::projection::state::Status::RetryingMetadata
                    } else {
                        crate::nexus::projection::state::Status::WaitingForSender
                    };
                    if let Some(handle) = collections
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .handle(&key)
                    {
                        states.send_if_modified(|state| {
                            let Some(collection) =
                                state.collections.iter_mut().find(|item| item.id == handle)
                            else {
                                return false;
                            };
                            if collection.status == status {
                                return false;
                            }
                            collection.status = status;
                            true
                        });
                    }
                }
                continue;
            }
            if failures.remove(&key).is_some() {
                retry_deadlines.remove(&key);
                crate::nexus::log::clog!(
                    "nexus",
                    "torrent metadata recovered correlation={}",
                    correlation_id(&key)
                );
            }
            let handle = collections
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .handle(&key);
            if let Some(handle) = handle {
                republish(&store, &states, handle, &key);
                // A resolved file list is the detail tier's whole content for
                // an import, so a screen waiting on it updates here rather
                // than on the next transfer tick.
                details.refresh(handle);
            }
        }
    }
}

fn work_key(work: &Pending) -> Vec<u8> {
    match work {
        Pending::Resolve { key, .. }
        | Pending::Acquire { key, .. }
        | Pending::Reconcile { key, .. } => key.clone(),
    }
}

fn correlation_id(key: &[u8]) -> String {
    key.iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Everything with a transition available, oldest collection first.
fn pending_work(store: &Store) -> Result<Vec<Pending>, crate::nexus::store::StoreError> {
    use crate::nexus::core::lifecycle::Lifecycle;

    let mut pending = Vec::new();
    for (key, stored) in store.collections()? {
        let source = store.torrent_import(&key)?;
        let entries = if source.is_some() {
            store.torrent_import_entries(&key)?
        } else {
            Vec::new()
        };
        let lifecycle = stored.lifecycle;

        // Anything the engine already carries has the stored intent asserted
        // against it, whether it is a download or this device's own seed.
        // Asserted rather than remembered: both verbs are idempotent, so
        // re-stating costs nothing and cannot drift — and it is the only path
        // by which a selection can change after a transfer has begun.
        if let Some(handle) = stored.substrate_handle {
            let files = matches!(lifecycle, Lifecycle::TorrentRequested { .. })
                .then(|| selected_indices(&entries));
            pending.push(Pending::Reconcile {
                key,
                handle,
                paused: lifecycle.activity().is_some_and(|it| it.is_paused()),
                files,
            });
            continue;
        }

        match lifecycle {
            // Nothing knows what this source contains yet.
            Lifecycle::TorrentResolving => {
                if let Some(source) = source {
                    pending.push(Pending::Resolve { key, source });
                }
            }
            // The person confirmed a selection and nothing is carrying it.
            Lifecycle::TorrentRequested { .. } => {
                let files = selected_indices(&entries);
                // Confirming with nothing chosen is refused at the command, so
                // an empty list here would be a stored row that cannot happen.
                // Skipped rather than sent: "fetch nothing" and "fetch
                // everything" must never be the same request.
                if let Some(source) = source
                    && !files.is_empty()
                {
                    pending.push(Pending::Acquire { key, source, files });
                }
            }
            // A draft has nothing to transfer, and a resolved selection nobody
            // has confirmed is the interface waiting on a person rather than a
            // stalled job. Every resolved entry starts selected so the screen
            // opens with something in it, and reading that default as a request
            // is what made reopening the app download a torrent that had only
            // ever been inspected.
            Lifecycle::NativeDraft
            | Lifecycle::TorrentAwaitingSelection
            // Publishing this device's own sources is the publisher's job.
            | Lifecycle::NativePublished { .. } => {}
        }
    }
    Ok(pending)
}

/// Which entries the person actually asked for, by index.
fn selected_indices(entries: &[StoredImportEntry]) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.selected)
        .map(|(index, _)| index)
        .collect()
}

async fn perform(store: &Store, substrate: &dyn Substrate, work: &Pending) -> anyhow::Result<()> {
    match work {
        Pending::Resolve { key, source } => resolve(store, substrate, key, source).await,
        Pending::Acquire { key, source, files } => {
            acquire(store, substrate, key, source, files).await
        }
        Pending::Reconcile {
            handle,
            paused,
            files,
            ..
        } => {
            // Selection before pause: a paused torrent accepts the update, and
            // applying it first means resuming never briefly fetches a file
            // that was deselected while it was stopped.
            if let Some(files) = files {
                substrate.set_selection(handle, files).await?;
            }
            substrate.set_paused(handle, *paused).await
        }
    }
}

/// Asks what the source contains and records it.
async fn resolve(
    store: &Store,
    substrate: &dyn Substrate,
    key: &[u8],
    source: &str,
) -> anyhow::Result<()> {
    crate::nexus::log::clog!(
        "nexus",
        "torrent resolve begin source_len={} supplied_peer_hints={:?}",
        source.len(),
        crate::nexus::torrent::peer_hints_from_source(source)
            .as_ref()
            .map(|peers| peers.as_slice())
            .unwrap_or_default()
    );
    let peer_hints = crate::nexus::torrent::peer_hints_from_source(source)?;
    let inspected = substrate.inspect(source, &peer_hints).await?;
    anyhow::ensure!(!inspected.files.is_empty(), "that source names no files");

    let entries = inspected
        .files
        .iter()
        .map(|file| StoredImportEntry {
            label: file.label.clone(),
            bytes: file.bytes,
            // Everything, until the person narrows it. A selection screen
            // that opens with nothing chosen makes "download" look broken.
            selected: true,
            native_location: None,
        })
        .collect::<Vec<_>>();
    store.put_torrent_import_entries(key, &entries)?;
    store.put_torrent_import_descriptor(key, &inspected.descriptor)?;
    crate::nexus::log::clog!(
        "nexus",
        "torrent resolve complete source_len={} files={} info_hash={}",
        source.len(),
        entries.len(),
        inspected.info_hash
    );

    // The real name replaces the placeholder taken from the source string.
    if let Some(stored) = store.collection(key)? {
        store.put_collection(
            key,
            &StoredCollection {
                name: inspected.name.clone(),
                lifecycle: StoredLifecycle::TorrentAwaitingSelection,
                ..stored
            },
        )?;
    }
    Ok(())
}

/// Starts downloading the chosen files.
async fn acquire(
    store: &Store,
    substrate: &dyn Substrate,
    key: &[u8],
    source: &str,
    files: &[usize],
) -> anyhow::Result<()> {
    let destination = crate::nexus::torrent::download_dir();
    let peer_hints = crate::nexus::torrent::peer_hints_from_source(source)?;
    let descriptor = store
        .torrent_import_descriptor(key)?
        .ok_or_else(|| anyhow::anyhow!("resolved torrent has no persisted descriptor"))?;
    let info = substrate
        .acquire_selection(source, Some(&descriptor), files, &destination, &peer_hints)
        .await?;

    // Recorded last, and only once the download is genuinely started: the
    // handle is what attributes a transfer back to this collection, and one
    // pointing at a download that failed to start would attribute nothing.
    if let Some(stored) = store.collection(key)? {
        store.put_collection(
            key,
            &StoredCollection {
                substrate_handle: Some(info.info_hash.clone()),
                media_path: destination.to_string_lossy().into_owned(),
                ..stored
            },
        )?;
    }
    Ok(())
}

/// Brings one collection's projected row back in line with its stored facts.
fn republish(store: &Store, states: &watch::Sender<PortalisState>, handle: Handle, key: &[u8]) {
    let Ok(Some(stored)) = store.collection(key) else {
        return;
    };
    let entries = store.torrent_import_entries(key).unwrap_or_default();
    let name = stored.name.clone();
    let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    // What this device intends to fetch, for as long as nothing is carrying
    // it yet. Once something is, the poller replaces this with the engine's
    // own total — the number the fraction is actually measured against.
    let total: u64 = entries
        .iter()
        .filter(|entry| entry.selected)
        .map(|entry| entry.bytes)
        .sum();
    let carried = stored.substrate_handle.is_some();

    states.send_if_modified(|state| {
        let Some(collection) = state
            .collections
            .iter_mut()
            .find(|collection| collection.id == handle)
        else {
            return false;
        };
        // The transfer poller takes over from here with real numbers; this
        // only has to be right about what the store knows.
        let status = crate::nexus::projection::state::status_for(
            crate::nexus::projection::state::StatusFacts {
                completed: stored.completed_at.is_some(),
                carried,
                publishing: false,
                importing: true,
                locally_complete: false,
                ..crate::nexus::projection::state::StatusFacts::from_lifecycle(stored.lifecycle)
            },
        );
        if collection.name == name
            && collection.entries == count
            && collection.total_bytes == total
            && collection.status == status
        {
            return false;
        }
        collection.name = name;
        collection.entries = count;
        collection.total_bytes = total;
        collection.status = status;
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nexus::store::records::Role as StoredRole;

    #[test]
    fn failed_durable_imports_use_exponential_backoff_capped_at_one_minute() {
        assert_eq!(retry_delay(1), std::time::Duration::from_secs(5));
        assert_eq!(retry_delay(2), std::time::Duration::from_secs(15));
        assert_eq!(retry_delay(3), std::time::Duration::from_secs(30));
        assert_eq!(retry_delay(4), std::time::Duration::from_secs(60));
        assert_eq!(retry_delay(99), std::time::Duration::from_secs(60));
    }

    #[test]
    fn correlation_ids_are_short_and_stable() {
        assert_eq!(correlation_id(&[0x01, 0xab, 0x20, 0xff, 0x99]), "01ab20ff");
        assert_eq!(correlation_id(&[0x01]), "01");
    }

    fn store() -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "portalis-torrents-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        (Store::open(dir.join("portalis.redb")).expect("opens"), dir)
    }

    fn collection(store: &Store, key: &[u8], source: &str) {
        collection_with_lifecycle(store, key, source, StoredLifecycle::TorrentResolving);
    }

    fn collection_with_draft(store: &Store, key: &[u8], source: &str, draft: bool) {
        let lifecycle = if draft {
            StoredLifecycle::TorrentAwaitingSelection
        } else {
            StoredLifecycle::TorrentRequested {
                activity: crate::nexus::store::records::StoredActivity::Running,
            }
        };
        collection_with_lifecycle(store, key, source, lifecycle);
    }

    fn collection_with_lifecycle(
        store: &Store,
        key: &[u8],
        source: &str,
        lifecycle: StoredLifecycle,
    ) {
        store
            .put_collection(
                key,
                &StoredCollection {
                    name: "placeholder".to_owned(),
                    role: StoredRole::Owner,
                    content_key: [0; 32],
                    media_path: String::new(),
                    sources: Vec::new(),
                    lifecycle,
                    on_disk_bytes: 0,
                    substrate_handle: None,
                    started_at: None,
                    completed_at: None,
                },
            )
            .expect("writes");
        store.put_torrent_import(key, source).expect("writes");
    }

    /// Resolving a source selects every file so the selection screen opens
    /// with something in it. That default is not a request: until the person
    /// presses Download the collection is still a draft, and acquiring it
    /// would fetch a torrent nobody asked for. Opening the app wakes this
    /// worker, so without the guard merely reopening Portalis started the
    /// download.
    #[test]
    fn a_resolved_draft_is_never_acquired_before_the_person_confirms_it() {
        let (store, dir) = store();
        collection_with_draft(&store, b"a", "magnet:?xt=urn:btih:abc", true);
        store
            .put_torrent_import_entries(
                b"a",
                &[StoredImportEntry {
                    label: "one.mkv".to_owned(),
                    bytes: 10,
                    // Exactly what `resolve` writes: everything selected.
                    selected: true,
                    native_location: None,
                }],
            )
            .expect("writes");

        assert!(
            pending_work(&store).expect("scans").is_empty(),
            "a draft's default selection is not a download request"
        );

        // Confirming is what makes it work: the same rows, no longer a draft.
        let stored = store.collection(b"a").expect("reads").expect("exists");
        store
            .put_collection(
                b"a",
                &StoredCollection {
                    lifecycle: StoredLifecycle::TorrentRequested {
                        activity: crate::nexus::store::records::StoredActivity::Running,
                    },
                    ..stored
                },
            )
            .expect("writes");
        assert!(
            matches!(
                pending_work(&store).expect("scans").as_slice(),
                [Pending::Acquire { files, .. }] if files == &[0]
            ),
            "pressing Download is what starts it"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A source nobody has resolved is work; the same source once resolved
    /// but not yet chosen from is not — the person is still deciding, and
    /// waking the engine for that would start a download they never asked
    /// for.
    #[test]
    fn an_unresolved_source_is_work_and_an_unchosen_one_is_not() {
        let (store, dir) = store();
        collection(&store, b"a", "magnet:?xt=urn:btih:abc");

        let pending = pending_work(&store).expect("scans");
        assert!(matches!(
            pending.as_slice(),
            [Pending::Resolve { source, .. }] if source == "magnet:?xt=urn:btih:abc"
        ));

        store
            .put_torrent_import_entries(
                b"a",
                &[StoredImportEntry {
                    label: "one.mkv".to_owned(),
                    bytes: 10,
                    selected: false,
                    native_location: None,
                }],
            )
            .expect("writes");
        let stored = store.collection(b"a").expect("reads").expect("exists");
        store
            .put_collection(
                b"a",
                &StoredCollection {
                    lifecycle: StoredLifecycle::TorrentAwaitingSelection,
                    ..stored
                },
            )
            .expect("resolution moves it to awaiting selection");
        assert!(pending_work(&store).expect("scans").is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_chosen_selection_is_work_until_something_is_carrying_it() {
        let (store, dir) = store();
        collection_with_draft(&store, b"a", "magnet:?xt=urn:btih:abc", false);
        store
            .put_torrent_import_entries(
                b"a",
                &[
                    StoredImportEntry {
                        label: "one.mkv".to_owned(),
                        bytes: 10,
                        selected: false,
                        native_location: None,
                    },
                    StoredImportEntry {
                        label: "two.mkv".to_owned(),
                        bytes: 20,
                        selected: true,
                        native_location: None,
                    },
                ],
            )
            .expect("writes");

        let pending = pending_work(&store).expect("scans");
        assert!(
            matches!(pending.as_slice(), [Pending::Acquire { files, .. }] if files == &[1]),
            "only the chosen file, by its index"
        );

        // Once carried there is nothing left to *start*, but the engine is
        // still told what the person last chose — asserted every pass rather
        // than remembered, which is what keeps a pause from drifting.
        let stored = store.collection(b"a").expect("reads").expect("exists");
        store
            .put_collection(
                b"a",
                &StoredCollection {
                    substrate_handle: Some("abc".to_owned()),
                    ..stored
                },
            )
            .expect("writes");
        assert!(matches!(
            pending_work(&store).expect("scans").as_slice(),
            [Pending::Reconcile { handle, paused: false, files, .. }]
                if handle == "abc" && files.as_deref() == Some(&[1][..])
        ));

        // Changing the choice on a collection that is already downloading is
        // ordinary reconcile work rather than nothing at all, which is what
        // made a selection permanent once the first byte moved.
        store
            .put_torrent_import_entries(
                b"a",
                &[
                    StoredImportEntry {
                        label: "one.mkv".to_owned(),
                        bytes: 10,
                        selected: true,
                        native_location: None,
                    },
                    StoredImportEntry {
                        label: "two.mkv".to_owned(),
                        bytes: 20,
                        selected: true,
                        native_location: None,
                    },
                ],
            )
            .expect("writes");
        assert!(matches!(
            pending_work(&store).expect("scans").as_slice(),
            [Pending::Reconcile { files, .. }]
                if files.as_deref() == Some(&[0, 1][..])
        ));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Reconciling states both halves of the stored intent, and states the
    /// selection first: a resume must never briefly fetch a file that was
    /// deselected while the transfer was stopped.
    #[tokio::test]
    async fn reconciling_states_the_selection_and_then_the_pause() {
        let (store, dir) = store();
        let substrate = crate::nexus::substrate::Recorded::default();

        perform(
            &store,
            &substrate,
            &Pending::Reconcile {
                key: b"a".to_vec(),
                handle: "abc".to_owned(),
                paused: true,
                files: Some(vec![0, 2]),
            },
        )
        .await
        .expect("reconciles");

        assert_eq!(
            substrate.reselected.lock().unwrap().as_slice(),
            [("abc".to_owned(), vec![0, 2])]
        );
        assert_eq!(
            substrate.paused.lock().unwrap().as_slice(),
            [("abc".to_owned(), true)]
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A collection with no torrent source is somebody else's business —
    /// this worker must not touch a published collection.
    #[test]
    fn a_collection_without_a_torrent_source_is_never_work() {
        let (store, dir) = store();
        store
            .put_collection(
                b"a",
                &StoredCollection {
                    name: "published".to_owned(),
                    role: StoredRole::Owner,
                    content_key: [0; 32],
                    media_path: String::new(),
                    sources: Vec::new(),
                    lifecycle: StoredLifecycle::NativePublished {
                        activity: crate::nexus::store::records::StoredActivity::Running,
                    },
                    on_disk_bytes: 0,
                    substrate_handle: None,
                    started_at: None,
                    completed_at: None,
                },
            )
            .expect("writes");

        assert!(pending_work(&store).expect("scans").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
