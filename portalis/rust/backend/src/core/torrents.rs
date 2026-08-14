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

use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, watch};

use crate::projection::state::{Handle, PortalisState, Status};
use crate::store::Store;
use crate::store::records::{StoredCollection, StoredImportEntry};
use crate::substrate::Substrate;

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
        files: Vec<usize>,
    },
}

/// Runs until shutdown, resolving sources and starting downloads.
pub(crate) async fn follow_torrent_imports(
    store: Arc<Store>,
    states: watch::Sender<PortalisState>,
    collections: Arc<Mutex<super::nexus::LocalCollections>>,
    substrate: Arc<dyn Substrate>,
    mut wakes: mpsc::Receiver<()>,
    mut shutdown: super::supervisor::Shutdown,
    details: super::nexus::DetailSources,
) {
    loop {
        tokio::select! {
            () = shutdown.requested() => return,
            wake = wakes.recv() => {
                if wake.is_none() {
                    return;
                }
            }
        }

        // Scanned rather than carried in the wake: a wake says "something
        // may have changed", never *what*, so one coalesced wake is enough
        // however many imports it covers.
        let pending = match pending_work(&store) {
            Ok(pending) => pending,
            Err(error) => {
                crate::log::clog!("nexus", "could not scan torrent imports: {error}");
                continue;
            }
        };

        for work in pending {
            let done = tokio::select! {
                () = shutdown.requested() => return,
                done = perform(&store, substrate.as_ref(), &work) => done,
            };
            let key = match work {
                Pending::Resolve { key, .. }
                | Pending::Acquire { key, .. }
                | Pending::Reconcile { key, .. } => key,
            };
            if let Err(error) = done {
                crate::log::clog!("nexus", "torrent import failed: {error:#}");
                continue;
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

/// Everything with a transition available, oldest collection first.
fn pending_work(store: &Store) -> Result<Vec<Pending>, crate::store::StoreError> {
    let mut pending = Vec::new();
    for (key, stored) in store.collections()? {
        let Some(source) = store.torrent_import(&key)? else {
            continue;
        };
        let entries = store.torrent_import_entries(&key)?;
        if entries.is_empty() {
            pending.push(Pending::Resolve { key, source });
            continue;
        }
        let files = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.selected)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        // Already carried: nothing left to start, but the engine still has
        // to be told what the person last chose — both which files and
        // whether to move at all. Asserted rather than remembered: both verbs
        // are idempotent, so re-stating the stored intent costs nothing and
        // cannot drift. This is also the only path by which a selection can
        // change after a download begins.
        if let Some(handle) = stored.substrate_handle {
            pending.push(Pending::Reconcile {
                key,
                handle,
                paused: stored.paused,
                files,
            });
            continue;
        }
        // Resolved, but nobody has chosen yet. The interface is showing the
        // list; waiting is the correct state, not a stalled one.
        if files.is_empty() {
            continue;
        }
        pending.push(Pending::Acquire { key, source, files });
    }
    Ok(pending)
}

async fn perform(
    store: &Store,
    substrate: &dyn Substrate,
    work: &Pending,
) -> anyhow::Result<()> {
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
            substrate.set_selection(handle, files).await?;
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
    let inspected = substrate.inspect(source).await?;
    anyhow::ensure!(
        !inspected.files.is_empty(),
        "that source names no files"
    );

    let entries = inspected
        .files
        .iter()
        .map(|file| StoredImportEntry {
            label: file.label.clone(),
            bytes: file.bytes,
            // Everything, until the person narrows it. A selection screen
            // that opens with nothing chosen makes "download" look broken.
            selected: true,
        })
        .collect::<Vec<_>>();
    store.put_torrent_import_entries(key, &entries)?;
    store.put_torrent_import_descriptor(key, &inspected.descriptor)?;

    // The real name replaces the placeholder taken from the source string.
    if let Some(stored) = store.collection(key)? {
        store.put_collection(
            key,
            &StoredCollection {
                name: inspected.name.clone(),
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
    let destination = crate::torrent::download_dir();
    let info = substrate
        .acquire_selection(source, files, &destination)
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
fn republish(
    store: &Store,
    states: &watch::Sender<PortalisState>,
    handle: Handle,
    key: &[u8],
) {
    let Ok(Some(stored)) = store.collection(key) else {
        return;
    };
    let entries = store.torrent_import_entries(key).unwrap_or_default();
    let name = stored.name.clone();
    let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    let total: u64 = entries.iter().map(|entry| entry.bytes).sum();
    let carried = stored.substrate_handle.is_some();

    states.send_if_modified(|state| {
        let Some(collection) = state
            .collections
            .iter_mut()
            .find(|collection| collection.id == handle)
        else {
            return false;
        };
        let status = if stored.paused {
            Status::Paused
        } else if carried {
            // The transfer poller takes over from here with real numbers.
            Status::Downloading
        } else {
            Status::Preparing
        };
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
    use crate::store::records::Role as StoredRole;

    fn store() -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "portalis-torrents-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        (
            Store::open(dir.join("portalis.redb")).expect("opens"),
            dir,
        )
    }

    fn collection(store: &Store, key: &[u8], source: &str) {
        store
            .put_collection(
                key,
                &StoredCollection {
                    name: "placeholder".to_owned(),
                    role: StoredRole::Owner,
                    content_key: [0; 32],
                    media_path: String::new(),
                    sources: Vec::new(),
                    paused: false,
                    on_disk_bytes: 0,
                    substrate_handle: None,
                },
            )
            .expect("writes");
        store.put_torrent_import(key, source).expect("writes");
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
                }],
            )
            .expect("writes");
        assert!(pending_work(&store).expect("scans").is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_chosen_selection_is_work_until_something_is_carrying_it() {
        let (store, dir) = store();
        collection(&store, b"a", "magnet:?xt=urn:btih:abc");
        store
            .put_torrent_import_entries(
                b"a",
                &[
                    StoredImportEntry {
                        label: "one.mkv".to_owned(),
                        bytes: 10,
                        selected: false,
                    },
                    StoredImportEntry {
                        label: "two.mkv".to_owned(),
                        bytes: 20,
                        selected: true,
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
                if handle == "abc" && files == &[1]
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
                    },
                    StoredImportEntry {
                        label: "two.mkv".to_owned(),
                        bytes: 20,
                        selected: true,
                    },
                ],
            )
            .expect("writes");
        assert!(matches!(
            pending_work(&store).expect("scans").as_slice(),
            [Pending::Reconcile { files, .. }] if files == &[0, 1]
        ));

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Reconciling states both halves of the stored intent, and states the
    /// selection first: a resume must never briefly fetch a file that was
    /// deselected while the transfer was stopped.
    #[tokio::test]
    async fn reconciling_states_the_selection_and_then_the_pause() {
        let (store, dir) = store();
        let substrate = crate::substrate::Recorded::default();

        perform(
            &store,
            &substrate,
            &Pending::Reconcile {
                key: b"a".to_vec(),
                handle: "abc".to_owned(),
                paused: true,
                files: vec![0, 2],
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
                    paused: false,
                    on_disk_bytes: 0,
                    substrate_handle: None,
                },
            )
            .expect("writes");

        assert!(pending_work(&store).expect("scans").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
