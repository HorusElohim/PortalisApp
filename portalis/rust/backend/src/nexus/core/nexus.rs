//! The five calls the interface has, and no more without a reason recorded.
//!
//! Everything above this line is Rust deciding things;
//! everything below it is an interface rendering them. The narrowness is the
//! design: five calls cannot grow into a second architecture, and an interface
//! that can only subscribe and send cannot start keeping its own copy of the
//! truth (D8).
//!
//! Three properties are worth stating because each one is a promise the
//! interface relies on:
//!
//! **`watch` yields a complete snapshot first.** Not a delta. A restart, a hot
//! reload, or a widget mounting late all get everything, so none of them has
//! to have been listening earlier.
//!
//! **`command` answers immediately.** Acceptance is a local decision — is this
//! well-formed, is it something this device may do — and none of it waits for
//! I/O. A command that needs connectivity and can wait is queued and says so;
//! only the few that cannot are refused outright.
//!
//! **`watch_detail` is the only way to pay for the expensive tier.** No
//! subscription, no piece maps.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use thiserror::Error;
use tokio::sync::{Notify, watch};

use super::supervisor::Supervisor;
use crate::nexus::projection::emit::Projector;
use crate::nexus::projection::state::{
    Accepted, CollectionState, Command, CommandError, Connectivity, ContactState, Detail,
    DeviceState, Friendship, Handle, LocalFile, MemberState, Nature, PortalisState, Role, Status,
};
use crate::nexus::store::records::{
    Role as StoredRole, StoredActivity, StoredCollection, StoredImportEntry, StoredLifecycle,
    StoredSourceFile,
};
use crate::nexus::store::{Store, StoreError};

/// Where the core keeps its file, and who it is.
#[derive(Clone, Debug)]
pub struct Config {
    /// The local store's directory. One file inside it (§12).
    pub data_dir: std::path::PathBuf,
    /// What to call this device until the person renames it.
    pub device_name: String,
    /// The public signing key people compare when verifying this device.
    pub fingerprint: String,
}

/// Why the core did not start.
#[derive(Debug, Error)]
pub enum OpenError {
    /// Including a store written by a newer version, which is reported as
    /// itself rather than as a generic failure — the person needs to be told
    /// to upgrade, not that something went wrong.
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("the Portalis data directory could not be created: {0}")]
    DataDir(String),
    #[error("the device identity could not be loaded: {0}")]
    Identity(String),
}

/// The running core.
pub struct Nexus {
    supervisor: Supervisor,
    states: watch::Sender<PortalisState>,
    /// One channel per collection anyone is currently watching.
    ///
    /// Deliberately not one shared slot. A single sender meant opening a
    /// second collection replaced what the first one's subscriber received —
    /// two screens could not be open at once, and even swapping between them
    /// delivered one collection's contents to the other's stream for a frame.
    /// Per-collection senders make that unrepresentable rather than merely
    /// avoided.
    details: Arc<Mutex<HashMap<Handle, watch::Sender<Option<Detail>>>>>,
    projector: Arc<Mutex<Projector>>,
    /// Names each accepted command, so the interface can match one to the
    /// `pending` field it appears in.
    next_command: AtomicU64,
    active: bool,
    store: Arc<Store>,
    collections: Arc<Mutex<LocalCollections>>,
    /// The substrate's latest word on each collection, so the detail tier and
    /// the progress tier answer from the same reading.
    holdings: super::transfers::Holdings,
    publisher: Arc<Notify>,
    /// Wakes the worker that resolves torrent sources and starts downloads.
    torrents: Arc<Notify>,
    activity: crate::nexus::activity::DeviceActivityTracker,
    /// Kicked on `set_active(true)` to force every live transfer to
    /// re-establish its peer connections after the app returns from the
    /// background — see [`Substrate::reconnect_active`] for why the
    /// substrate's own reconnect logic cannot be relied on to have run
    /// while the process was suspended. A `Notify`, not a direct
    /// `tokio::spawn` from `set_active`: `set_active` is called from
    /// flutter_rust_bridge's synchronous worker thread, which has no Tokio
    /// runtime — spawning there panics with "there is no reactor running",
    /// and because that panic happens while `portalis_api::locked_runtime`'s
    /// mutex is held, it poisons the one lock every FRB call goes through,
    /// bricking the app until relaunch. `notify_one()` needs no runtime, so
    /// the actual `reconnect_active().await` call happens on the
    /// already-running supervised worker instead.
    resume: Arc<Notify>,
    /// Serializes import admission, so two commands racing inside the same
    /// process moment cannot both pass the durable-uniqueness check before
    /// either has written its row (ADR-0015). Held only for the duration of
    /// `import_torrent` — imports are an occasional user action, not a hot
    /// path — and never poisoned permanently by a failed import, since the
    /// guard is dropped (not the identity remembered) when the function
    /// returns either way.
    importing: Arc<Mutex<()>>,
}

impl std::fmt::Debug for Nexus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `substrate` is `dyn Substrate`, which does not implement `Debug`
        // (the trait covers both the production librqbit-backed
        // implementation and the test double, and requiring `Debug` there
        // just to satisfy a derive here is not worth the churn) — every
        // other field is printed as before.
        formatter
            .debug_struct("Nexus")
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

/// Everything the detail tier is assembled from.
///
/// Grouped because three different callers need exactly this set — the
/// runtime answering a subscribe, the transfer poller refreshing an open
/// collection, and the torrent worker publishing a freshly resolved file
/// list — and passing four values around three times is how they drift.
#[derive(Clone)]
pub(crate) struct DetailSources {
    pub(crate) store: Arc<Store>,
    pub(crate) collections: Arc<Mutex<LocalCollections>>,
    pub(crate) holdings: super::transfers::Holdings,
    pub(crate) senders: Arc<Mutex<HashMap<Handle, watch::Sender<Option<Detail>>>>>,
}

impl std::fmt::Debug for DetailSources {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DetailSources")
            .finish_non_exhaustive()
    }
}

impl DetailSources {
    /// Recomputes and publishes one collection's detail, if anyone is
    /// watching it.
    ///
    /// Nothing happens when nobody is subscribed, which is what keeps the
    /// expensive tier free until it is asked for.
    pub(crate) fn refresh(&self, collection: Handle) {
        let watching = self
            .senders()
            .get(&collection)
            .is_some_and(|sender| sender.receiver_count() > 0);
        if !watching {
            return;
        }
        let detail = self.build(collection);
        if let Some(sender) = self.senders().get(&collection) {
            sender.send_if_modified(|held| {
                if *held == detail {
                    return false;
                }
                *held = detail;
                true
            });
        }
    }

    /// Every collection someone is currently subscribed to.
    ///
    /// Collected rather than iterated in place so the lock is released before
    /// a caller rebuilds anything — building a detail reads the store, and
    /// holding this while doing that would make an unrelated subscribe wait
    /// on disk.
    pub(crate) fn watched(&self) -> Vec<Handle> {
        let mut senders = self.senders();
        senders.retain(|_, sender| sender.receiver_count() > 0);
        senders.keys().copied().collect()
    }

    pub(crate) fn senders(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<Handle, watch::Sender<Option<Detail>>>> {
        self.senders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn key(&self, collection: Handle) -> Option<Vec<u8>> {
        self.collections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .key(collection)
            .map(<[u8]>::to_vec)
    }

    fn build(&self, collection: Handle) -> Option<Detail> {
        let key = self.key(collection)?;
        let local_sources = self.store.collection(&key).ok()??.sources;
        // What the substrate last said, if it is carrying this at all. It
        // supplies what only it knows: where the bytes landed, which pieces
        // are verified, and who they are coming from.
        let held = self.holdings.get(&key);
        let entries = if local_sources.is_empty() {
            self.store
                .torrent_import_entries(&key)
                .ok()?
                .into_iter()
                .map(|entry| {
                    let native_path = entry.native_location;
                    let available = native_path.as_deref().is_some_and(|path| {
                        crate::nexus::content_location::ContentLocation::from_source_path(path)
                            .is_ok_and(|location| location.length(Some(entry.bytes)).is_ok())
                    });
                    (
                        entry.label,
                        entry.bytes,
                        entry.selected,
                        available,
                        native_path,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            // This device's own files, referenced where they already are. The
            // path is known without asking the substrate anything — it is the
            // source the person picked — so an owner sees previews whether or
            // not anything is currently seeding. `available` still checks the
            // reference resolves: a moved, renamed, or unmounted original must
            // not be presented as a verified, ready-to-share file.
            local_sources
                .into_iter()
                .map(|entry| {
                    let available =
                        crate::nexus::content_location::ContentLocation::from_source_path(
                            &entry.path,
                        )
                        .is_ok_and(|location| location.length(Some(entry.bytes)).is_ok());
                    (entry.label, entry.bytes, true, available, Some(entry.path))
                })
                .collect()
        };
        Some(Detail {
            id: collection,
            entries: entries
                .into_iter()
                .enumerate()
                .map(|(index, (label, bytes, selected, available, local_path))| {
                    // Matched by name rather than by position: the substrate
                    // orders a torrent's files its own way, and lining two
                    // lists up by index would put one file's path beside
                    // another file's name.
                    let carried = held
                        .as_ref()
                        .and_then(|info| info.files.iter().find(|file| file.name == label));
                    crate::nexus::projection::state::EntryState {
                        id: Handle(u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1)),
                        label,
                        bytes,
                        selected,
                        // Believe the substrate when it is carrying this, and
                        // the store otherwise: a file is available when its
                        // bytes are all present, not when a row says so.
                        available: carried.map_or(available, |file| {
                            file.downloaded_bytes >= file.length_bytes && file.length_bytes > 0
                        }),
                        // The substrate's own per-file count while it is
                        // carrying this; otherwise all-or-nothing, which is
                        // the truth for a file referenced where it already
                        // sits.
                        downloaded_bytes: carried
                            .map_or(if available { bytes } else { 0 }, |file| {
                                file.downloaded_bytes
                            }),
                        // The substrate's resolved location wins when it has
                        // one — a multi-file torrent lands in a subfolder
                        // nobody chose — and the person's own source stands in
                        // otherwise.
                        path: carried
                            .map(|file| file.absolute_path.clone())
                            .or(local_path),
                    }
                })
                .collect(),
            pieces: held.as_ref().map(pieces_of).unwrap_or_default(),
            peers: self.holdings.peers(&key),
        })
    }
}

/// The process-local handles for durable collection records.
///
/// Store keys survive a restart; handles deliberately do not. Keeping their
/// mapping beside the projection prevents either identifier becoming the
/// other's accidental public API.

#[derive(Debug, Default)]
pub struct LocalCollections {
    keys: HashMap<Handle, Vec<u8>>,
    next_handle: u32,
}

#[derive(Debug, Default)]
struct LocalContacts {
    handles: HashMap<[u8; portalis_nexus_protocol::DEVICE_KEY_BYTES], Handle>,
    states: Vec<ContactState>,
    next_handle: u32,
}

impl LocalContacts {
    fn member(
        &mut self,
        store: &Store,
        root_key: [u8; portalis_nexus_protocol::DEVICE_KEY_BYTES],
    ) -> Result<MemberState, StoreError> {
        let Some(stored) = store.contact(&root_key)? else {
            return Ok(MemberState {
                root_key,
                contact: None,
            });
        };
        let handle = if let Some(handle) = self.handles.get(&root_key) {
            *handle
        } else {
            self.next_handle = self.next_handle.saturating_add(1);
            let handle = Handle(self.next_handle);
            let display_name = stored
                .handle
                .split_once('#')
                .map_or_else(|| stored.handle.clone(), |(name, _)| name.to_owned());
            self.handles.insert(root_key, handle);
            self.states.push(ContactState {
                id: handle,
                display_name,
                handle: Some(stored.handle),
                fingerprint: hex::encode(root_key),
                verified: stored.fingerprint_verified,
                friendship: Friendship::Accepted,
                reachable: None,
            });
            handle
        };
        Ok(MemberState {
            root_key,
            contact: Some(handle),
        })
    }
}

struct HydratedMembership {
    number: u64,
    roots: Vec<[u8; portalis_nexus_protocol::DEVICE_KEY_BYTES]>,
    failure: Option<Status>,
}

fn hydrate_membership(
    store: &Store,
    key: &[u8],
    local_owner_root: [u8; portalis_nexus_protocol::DEVICE_KEY_BYTES],
) -> Result<HydratedMembership, StoreError> {
    let rows = store.revisions(key)?;
    let Some((highest_number, _)) = rows.last() else {
        return Ok(HydratedMembership {
            number: 0,
            roots: Vec::new(),
            failure: None,
        });
    };
    let highest_number = *highest_number;
    let expected_collection = <[u8; portalis_nexus_protocol::SHARE_ID_BYTES]>::try_from(key).ok();
    let mut previous: Option<portalis_nexus_protocol::Revision> = None;

    for (stored_number, bytes) in rows {
        // `Revision::decode` already runs `Revision::validate()` internally
        // (member ordering/uniqueness, first-revision shape, member count),
        // so a structurally invalid row is refused here before any of its
        // fields — including its member list — are trusted.
        let revision = match portalis_nexus_protocol::Revision::decode(&bytes) {
            Ok(revision) => revision,
            Err(portalis_nexus_protocol::RevisionError::UnknownDomain) => {
                return Ok(HydratedMembership {
                    number: highest_number,
                    roots: Vec::new(),
                    failure: Some(Status::NeedsNewerVersion),
                });
            }
            Err(_) => {
                return Ok(HydratedMembership {
                    number: highest_number,
                    roots: Vec::new(),
                    failure: Some(Status::CannotVerify(
                        crate::nexus::core::events::VerifyFailure::ContentMismatch,
                    )),
                });
            }
        };
        let collection_matches =
            expected_collection.is_some_and(|collection| revision.collection_id == collection);
        if !collection_matches || revision.number != stored_number {
            let failure = if revision.number < stored_number {
                crate::nexus::core::events::VerifyFailure::Rollback
            } else {
                crate::nexus::core::events::VerifyFailure::ContentMismatch
            };
            return Ok(HydratedMembership {
                number: highest_number,
                roots: Vec::new(),
                failure: Some(Status::CannotVerify(failure)),
            });
        }
        if !revision.verify() {
            return Ok(HydratedMembership {
                number: highest_number,
                roots: Vec::new(),
                failure: Some(Status::CannotVerify(
                    crate::nexus::core::events::VerifyFailure::Signature,
                )),
            });
        }
        // A genuine signature alone proves nothing about *authority* — any
        // keypair can sign a well-formed revision naming itself as owner.
        // Admission (`crypto::verify`) additionally requires the author to
        // be an authorized, non-revoked device on the claimed owner's
        // device log. Hydration reads its own persisted rows rather than a
        // network offer, and today's product only ever persists collections
        // this device itself owns or was admitted to via that same
        // authority check — so the equivalent hydration-time authority
        // check is: the revision's owner and author must be *this device's
        // own* identity. Anything else means the on-disk row was replaced
        // by something this device never verified, and must not be trusted
        // merely because it carries a valid signature over *some* key.
        if revision.owner_root_key != local_owner_root || revision.author_key != local_owner_root {
            return Ok(HydratedMembership {
                number: highest_number,
                roots: Vec::new(),
                failure: Some(Status::CannotVerify(
                    crate::nexus::core::events::VerifyFailure::Signature,
                )),
            });
        }
        if let Some(held) = previous.as_ref()
            && revision.owner_root_key != held.owner_root_key
        {
            return Ok(HydratedMembership {
                number: highest_number,
                roots: Vec::new(),
                failure: Some(Status::ConflictingHistory),
            });
        }
        let held = previous
            .as_ref()
            .map(|held| crate::nexus::crypto::ChainState {
                collection_id: held.collection_id,
                number: held.number,
                revision_hash: held.hash(),
            });
        let continuity = if held.is_some() {
            crate::nexus::crypto::Continuity::Strict
        } else {
            crate::nexus::crypto::Continuity::Join
        };
        if let Err(error) = crate::nexus::crypto::position(&revision, held.as_ref(), continuity) {
            let failure = match error {
                crate::nexus::crypto::ChainError::Fork { .. } => Status::ConflictingHistory,
                crate::nexus::crypto::ChainError::Rollback { .. } => {
                    Status::CannotVerify(crate::nexus::core::events::VerifyFailure::Rollback)
                }
                _ => Status::CannotVerify(crate::nexus::core::events::VerifyFailure::BrokenChain),
            };
            return Ok(HydratedMembership {
                number: highest_number,
                roots: Vec::new(),
                failure: Some(failure),
            });
        }
        previous = Some(revision);
    }

    let current = previous.expect("a non-empty revision set has a current revision");
    Ok(HydratedMembership {
        number: current.number,
        roots: current
            .members
            .into_iter()
            .map(|member| member.root_key)
            .collect(),
        failure: None,
    })
}

/// The one constructor for a collection entering the production projection.
///
/// Startup hydration, native draft creation, and torrent import all enter
/// through here. Later workers only refine fields for which they have newer
/// evidence (metadata or a live transfer); they do not rebuild lifecycle,
/// role, nature, or persisted byte/count facts independently (ADR-0017).
fn project_stored_collection(
    handle: Handle,
    stored: StoredCollection,
    revision: u64,
    members: Vec<MemberState>,
    imported_entries: &[StoredImportEntry],
    importing: bool,
    failure: Option<Status>,
) -> CollectionState {
    let status = failure.unwrap_or_else(|| {
        crate::nexus::projection::state::status_for(
            crate::nexus::projection::state::StatusFacts::from_stored(
                &stored, revision, importing, None,
            ),
        )
    });
    let (entries, total_bytes) = if stored.sources.is_empty() {
        (
            imported_entries.len(),
            // Selected only — the same denominator the engine counts progress
            // against. See `torrents::republish`.
            imported_entries
                .iter()
                .filter(|entry| entry.selected)
                .map(|entry| entry.bytes)
                .sum(),
        )
    } else {
        (
            stored.sources.len(),
            stored.sources.iter().map(|entry| entry.bytes).sum(),
        )
    };

    CollectionState {
        started_at: stored.started_at,
        completed_at: stored.completed_at,
        id: handle,
        name: stored.name,
        nature: if importing {
            Nature::Torrent
        } else {
            Nature::Native
        },
        role: match stored.role {
            StoredRole::Owner => Role::Owner,
            StoredRole::Member => Role::Member,
        },
        revision,
        status,
        members,
        entries: u32::try_from(entries).unwrap_or(u32::MAX),
        total_bytes,
        on_disk_bytes: stored.on_disk_bytes,
        uploaded_bytes: 0,
        transfer: None,
        publish_progress: None,
        pending: None,
    }
}

impl LocalCollections {
    fn hydrate(
        store: &Store,
    ) -> Result<(Self, Vec<ContactState>, Vec<CollectionState>), OpenError> {
        // The signing identity that authored/owns every revision this
        // device is willing to trust at hydration time. Loaded once, not
        // per collection: identity load failure fails the whole open
        // rather than silently treating some collections as unowned.
        let local_owner_root = crate::nexus::device::current_signing_identity()
            .map(|identity| identity.public_key())
            .map_err(|error| OpenError::Identity(error.to_string()))?;
        let mut local = Self::default();
        let mut contacts = LocalContacts::default();
        let mut projected = Vec::new();
        for (key, stored) in store.collections()? {
            let imported_entries = store.torrent_import_entries(&key)?;
            let handle = local.assign(key.clone());
            let membership = hydrate_membership(store, &key, local_owner_root)?;
            let revision = membership.number;
            let members = membership
                .roots
                .into_iter()
                .map(|root| contacts.member(store, root))
                .collect::<Result<Vec<_>, _>>()?;
            let torrent_import = store.torrent_import(&key)?;
            let torrent_import = torrent_import.is_some();
            projected.push(project_stored_collection(
                handle,
                stored,
                revision,
                members,
                &imported_entries,
                torrent_import,
                membership.failure,
            ));
        }
        Ok((local, contacts.states, projected))
    }

    fn assign(&mut self, key: Vec<u8>) -> Handle {
        self.next_handle += 1;
        let handle = Handle(self.next_handle);
        self.keys.insert(handle, key);
        handle
    }

    fn key(&self, handle: Handle) -> Option<&[u8]> {
        self.keys.get(&handle).map(Vec::as_slice)
    }

    pub(crate) fn handle(&self, key: &[u8]) -> Option<Handle> {
        self.keys
            .iter()
            .find_map(|(handle, stored)| (stored == key).then_some(*handle))
    }

    /// A test that drives the poller without standing up the whole Nexus
    /// needs a collection→handle mapping without hydrating from a store.
    #[cfg(test)]
    pub(crate) fn test_with_collection(key: &[u8]) -> Self {
        let mut local = Self::default();
        local.assign(key.to_vec());
        local
    }

    fn forget(&mut self, handle: Handle) {
        self.keys.remove(&handle);
    }
}

impl Nexus {
    /// Opens the store and starts the core.
    ///
    /// Nothing is awaited here that the first frame does not need: the store
    /// is opened, a snapshot is built from it, and everything else — the
    /// connection engine, the torrent session — starts afterwards under the
    /// supervisor. That is what the first-frame budget in §21 is about.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the store cannot be opened.
    pub fn open(config: &Config) -> Result<Self, OpenError> {
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|error| OpenError::DataDir(error.to_string()))?;
        Self::open_with_store(
            config,
            Arc::new(Store::open(config.data_dir.join("portalis.redb"))?),
        )
    }

    fn open_with_store(config: &Config, store: Arc<Store>) -> Result<Self, OpenError> {
        Self::open_with_store_and_substrate(config, store, crate::nexus::substrate::current())
    }

    fn open_with_store_and_substrate(
        config: &Config,
        store: Arc<Store>,
        substrate: Arc<dyn crate::nexus::substrate::Substrate>,
    ) -> Result<Self, OpenError> {
        let (collections, contacts, collection_states) = LocalCollections::hydrate(&store)?;
        let device = DeviceState {
            name: config.device_name.clone(),
            handle: None,
            fingerprint: config.fingerprint.clone(),
            devices: 1,
        };
        let first = PortalisState {
            device,
            connectivity: Connectivity::LocalOnly,
            contacts,
            collections: collection_states,
            alerts: Vec::new(),
        };

        let states = watch::Sender::new(first);
        let collections = Arc::new(Mutex::new(collections));
        let publisher = Arc::new(Notify::new());
        let torrents = Arc::new(Notify::new());
        let resume = Arc::new(Notify::new());
        let substrate_for_torrents = Arc::clone(&substrate);
        let substrate_for_resume = Arc::clone(&substrate);
        let holdings = super::transfers::Holdings::default();
        let activity = crate::nexus::activity::DeviceActivityTracker::start(
            Arc::clone(&store),
            unix_time_ns(),
        )?;
        let details: Arc<Mutex<HashMap<Handle, watch::Sender<Option<Detail>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let sources = DetailSources {
            store: Arc::clone(&store),
            collections: Arc::clone(&collections),
            holdings: holdings.clone(),
            senders: Arc::clone(&details),
        };
        let mut supervisor = Supervisor::default();
        supervisor.start_now("collection publisher", {
            let store = Arc::clone(&store);
            let states = states.clone();
            let collections = Arc::clone(&collections);
            let substrate = Arc::clone(&substrate);
            let publisher = Arc::clone(&publisher);
            move |shutdown| {
                publish_pending_collections(
                    store,
                    states,
                    collections,
                    substrate,
                    publisher,
                    shutdown,
                )
            }
        });
        // Started here rather than when a screen opens: transfer history is
        // recorded whether or not anybody is looking at it, which is the whole
        // difference between a chart that survives a restart and one that
        // begins when a person happens to navigate.
        // Historically this started a service-connectivity follower here too.
        // The Iroh-based Nexus control plane it followed has been removed —
        // this backend is BitTorrent-only now — so there is nothing left to
        // start.
        supervisor.start_now("transfer follower", {
            let store = Arc::clone(&store);
            let states = states.clone();
            let collections = Arc::clone(&collections);
            let holdings = holdings.clone();
            let sources_for_transfers = sources.clone();
            let activity_for_transfers = activity.clone();
            let bus = Arc::clone(supervisor.bus_arc());
            move |shutdown| {
                super::transfers::follow_transfers(
                    store,
                    states,
                    collections,
                    substrate,
                    holdings,
                    shutdown,
                    sources_for_transfers,
                    activity_for_transfers,
                    bus,
                )
            }
        });
        // Resolving a source and starting its download both wait on a
        // network, which `command` promises not to do. Started here so an
        // import interrupted by a restart resumes from the store rather than
        // needing the person to ask again.
        supervisor.start_now("torrent imports", {
            let store = Arc::clone(&store);
            let states = states.clone();
            let collections = Arc::clone(&collections);
            let substrate = Arc::clone(&substrate_for_torrents);
            let sources = sources.clone();
            let torrents = Arc::clone(&torrents);
            move |shutdown| {
                super::torrents::follow_torrent_imports(
                    store,
                    states,
                    collections,
                    substrate,
                    torrents,
                    shutdown,
                    sources,
                )
            }
        });
        // Checks every owned collection's zero-copy sources still exist on
        // disk, and converts one that has lost any of them into an ordinary
        // download of its own published content — see `watch_source_sanity`
        // for why that conversion, rather than a new status, is the fix.
        supervisor.start_now("source sanity", {
            let store = Arc::clone(&store);
            let torrents = Arc::clone(&torrents);
            move |shutdown| watch_source_sanity(store, torrents, shutdown)
        });
        // Runs `Substrate::reconnect_active` off the app's own async
        // runtime rather than from `set_active` directly — see the
        // `resume` field's doc comment for why `set_active` (called from
        // FRB's synchronous worker thread) cannot safely `tokio::spawn`
        // itself.
        supervisor.start_now("resume reconnect", {
            let substrate = Arc::clone(&substrate_for_resume);
            let resume = Arc::clone(&resume);
            move |mut shutdown| async move {
                loop {
                    tokio::select! {
                        () = shutdown.requested() => return,
                        _ = resume.notified() => {}
                    }
                    if let Err(error) = substrate.reconnect_active().await {
                        crate::nexus::log::clog!(
                            "torrent",
                            "reconnect on resume failed: {error:#}"
                        );
                    }
                }
            }
        });
        // One coalesced wake each is enough: both workers scan durable
        // collection state, so restart recovery does not depend on an
        // in-memory job.
        publisher.notify_one();
        torrents.notify_one();

        Ok(Self {
            supervisor,
            states,
            details,
            projector: Arc::new(Mutex::new(Projector::new())),
            next_command: AtomicU64::new(1),
            active: true,
            store,
            collections,
            holdings,
            publisher,
            torrents,
            activity,
            resume,
            importing: Arc::new(Mutex::new(())),
        })
    }

    /// Opens the runtime from the one platform-owned state directory.
    pub fn open_default() -> Result<Self, OpenError> {
        let device = crate::nexus::device::device_identity()
            .map_err(|error| OpenError::Identity(error.to_string()))?;
        let config = Config {
            data_dir: crate::nexus::paths::state_dir(),
            device_name: device.nickname,
            fingerprint: device.device_id,
        };
        Self::open_with_store(&config, crate::nexus::store::app_store()?)
    }

    /// The latest complete projection, without making a bridge subscription.
    #[must_use]
    pub fn state(&self) -> PortalisState {
        self.states.borrow().clone()
    }

    /// Starts or pauses network work while preserving the local projection.
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        if let Err(error) = self.activity.set_active(active, unix_time_ns()) {
            crate::nexus::log::clog!(
                "activity",
                "could not checkpoint lifecycle transition: {error}"
            );
        }
        // Deliberately not touched here: this device's own reachability from
        // the network's point of view does not depend on whether the app is
        // in the foreground.
        if active {
            // Coming back to the foreground (iOS/Android: the process may
            // have been fully suspended, not merely idle) is when a stalled
            // transfer needs to be actively re-kicked — librqbit's own
            // reconnect logic only runs while the process is scheduled, and
            // a suspended process was not. `set_active` runs on FRB's
            // synchronous worker thread, which has no Tokio runtime, so the
            // actual reconnect happens on the "resume reconnect" supervised
            // worker instead — see the `resume` field's doc comment for why
            // a direct `tokio::spawn` here previously panicked and poisoned
            // the one lock every FRB call depends on.
            self.resume.notify_one();
            // Metadata resolution has the same suspension failure mode. Wake
            // its durable worker too, so WaitingForSender retries immediately
            // after resume instead of waiting for the current backoff timer.
            self.torrents.notify_one();
        }
    }

    /// Renames this device and updates the live snapshot in the same call.
    ///
    /// ADR-0011 (decision #11): the persisted identity and the running
    /// projection are two copies of one fact, and a caller that only updates
    /// one of them (the previous separate `IdentityController`/`AppController`
    /// paths) can drift — a rename is durable but the still-running Nexus
    /// keeps showing the old name until the next full snapshot rebuild.
    /// Canonicalizing rename through Nexus makes that impossible: either both
    /// update together or the whole call fails and neither does.
    ///
    /// # Errors
    /// Returns a displayable reason when the persisted identity cannot be
    /// updated.
    pub fn rename_device(&mut self, nickname: String) -> anyhow::Result<()> {
        crate::nexus::device::set_nickname(nickname.clone())?;
        self.states.send_modify(|state| {
            state.device.name = nickname;
        });
        Ok(())
    }

    /// The state stream. Always holds a complete snapshot.
    ///
    /// A `watch` receiver rather than a channel of deltas, for two reasons: a
    /// subscriber that arrives late still sees everything, and a subscriber
    /// that falls behind sees the newest state rather than a backlog of stale
    /// ones. Both are what an interface actually wants.
    #[must_use]
    pub fn watch(&self) -> watch::Receiver<PortalisState> {
        self.states.subscribe()
    }

    /// This device's durable event bus, as a cloneable handle — see
    /// `nexus::core::events` for the guarantees it carries. Cloning is cheap
    /// (an `Arc` clone); subscribing is a separate async step so a caller
    /// can drop any lock it holds on the runtime before awaiting it.
    #[must_use]
    pub fn events_bus(&self) -> Arc<crate::nexus::core::events::EventBus> {
        Arc::clone(self.supervisor.bus_arc())
    }

    /// Subscribes to one collection's detail, or unsubscribes with `None`.
    ///
    /// The expensive tier costs nothing until this is called, and stops
    /// costing anything the moment it is called with `None`.
    /// This collection's readings after `at`, packed, with the newest moment
    /// they reach — or `None` when there is nothing newer.
    ///
    /// Answering "what do I not have yet?" rather than "what is there?" is
    /// what keeps an append costing an append. See `portalis_api::watch_history`.
    #[must_use]
    pub fn history_after(&self, collection: Handle, at: u64) -> Option<(u64, Vec<u8>)> {
        let key = self.collections.lock().ok()?.key(collection)?.to_owned();
        let rows = self.store.samples_after(&key, at).ok()?;
        let newest = rows.last()?.0;
        Some((newest, packed_samples(rows)))
    }

    /// Every live swarm connection, across every collection this device is
    /// carrying.
    ///
    /// Read from the same holdings the transfer poller writes, so the peers a
    /// people screen shows and the peer count on a collection row are the same
    /// reading rather than two answers from two clocks.
    #[must_use]
    pub fn peers(&self) -> Vec<(Handle, crate::nexus::projection::state::PeerState)> {
        let index = self
            .collections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(stored) = self.store.collections() else {
            return Vec::new();
        };
        stored
            .into_iter()
            .filter_map(|(key, _)| {
                let handle = index.handle(&key)?;
                Some(
                    self.holdings
                        .peers(&key)
                        .into_iter()
                        .map(move |peer| (handle, peer)),
                )
            })
            .flatten()
            .collect()
    }

    /// Durable cumulative endpoint ledgers for one collection, newest first.
    #[must_use]
    pub fn peer_history(
        &self,
        collection: Handle,
    ) -> Vec<crate::nexus::store::records::StoredPeerHistory> {
        let Some(key) = self
            .collections
            .lock()
            .ok()
            .and_then(|index| index.key(collection).map(ToOwned::to_owned))
        else {
            return Vec::new();
        };
        self.store.peer_history(&key).unwrap_or_default()
    }

    /// The current in-memory activity truth plus bounded durable runs.
    ///
    /// # Errors
    /// Returns [`StoreError`] when recent runs cannot be read.
    pub fn activity_summary(
        &self,
    ) -> Result<crate::nexus::activity::DeviceActivitySnapshot, StoreError> {
        self.activity.snapshot()
    }

    /// Clears only durable device activity and bounded run history. Identity,
    /// collections, settings, and collection-scoped peer history survive.
    ///
    /// # Errors
    /// Returns [`StoreError`] when the store transaction fails.
    pub fn clear_activity(&self) -> Result<(), StoreError> {
        self.activity.clear(unix_time_ns())
    }

    pub fn watch_detail(&self, collection: Option<Handle>) -> watch::Receiver<Option<Detail>> {
        self.projector
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .watch_detail(collection);
        let Some(collection) = collection else {
            // Nobody is looking. Anything whose screen has already gone is
            // dropped here; a live subscription is left alone, because this
            // caller does not say — and cannot know — whose it is.
            self.detail_senders()
                .retain(|_, sender| sender.receiver_count() > 0);
            // A channel that answers `None` once and is never written again.
            return watch::Sender::new(None).subscribe();
        };
        let detail = self.collection_detail(collection);
        let mut senders = self.detail_senders();
        // Dropped when the last receiver goes, so a closed screen stops
        // costing anything without needing anyone to say so.
        senders.retain(|_, sender| sender.receiver_count() > 0);
        let sender = senders
            .entry(collection)
            .or_insert_with(|| watch::Sender::new(None));
        sender.send_replace(detail);
        sender.subscribe()
    }

    /// Recomputes and publishes one collection's detail, if anyone is
    /// watching it.
    ///
    /// Called whenever something that shows up in the detail tier changes —
    /// a resolved file list, a transfer reading — so an open collection is
    /// live rather than a snapshot of the moment it was opened.
    fn refresh_detail(&self, collection: Handle) {
        self.detail_sources().refresh(collection);
    }

    fn detail_senders(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<Handle, watch::Sender<Option<Detail>>>> {
        self.details
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Accepts a command, or says why not.
    ///
    /// Does not wait for a network. Changes whose complete local effect is
    /// known are durably recorded first; publication and downloads remain
    /// queued and are reported through [`Self::watch`].
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the command is malformed, not something
    /// this device may do, or needs a connection it cannot wait for.
    pub fn command(&self, command: &Command) -> Result<Accepted, CommandError> {
        validate(command)?;
        match command {
            Command::ImportTorrent { source } => crate::nexus::log::clog!(
                "nexus",
                "command ImportTorrent source_kind={} source_len={}",
                if crate::nexus::torrent::is_magnet(source) {
                    "magnet"
                } else {
                    "torrent_path"
                },
                source.len()
            ),
            Command::DownloadSelection {
                collection,
                entries,
            } => crate::nexus::log::clog!(
                "nexus",
                "command DownloadSelection collection={collection:?} entries={:?}",
                entries
            ),
            _ => {}
        }

        // A collection the person has just created, renamed, or removed must
        // survive a crash before it can be published. The database write is
        // the acceptance boundary; network work still happens later.
        let collection = self.apply_local(command)?;
        if matches!(
            command,
            Command::CreateCollection { files, .. } | Command::AddMedia { files, .. }
                if !files.is_empty()
        ) {
            // Notify coalesces duplicate wakes without dropping a wake that
            // arrives while the worker is busy publishing another collection.
            self.publisher.notify_one();
        }

        // Deferrable commands are queued rather than refused, which is what
        // lets the interface accept one instantly with the network down.
        let queued = command.is_deferrable();
        if !queued {
            return Err(CommandError::Unavailable);
        }
        Ok(Accepted {
            id: self.next_command.fetch_add(1, Ordering::Relaxed),
            collection,
            queued,
        })
    }

    fn apply_local(&self, command: &Command) -> Result<Option<Handle>, CommandError> {
        match command {
            Command::CreateCollection { name, files } => {
                self.create_collection(name, files).map(Some)
            }
            Command::AddMedia {
                collection,
                label: _,
                files,
            } => self.add_media(*collection, files).map(|()| None),
            Command::RenameCollection { collection, name } => {
                self.rename_collection(*collection, name).map(|()| None)
            }
            Command::DeleteCollection {
                collection,
                delete_files,
            } => self
                .delete_collection(*collection, *delete_files)
                .map(|()| None),
            Command::SetPaused { collection, paused } => {
                self.set_paused(*collection, *paused).map(|()| None)
            }
            Command::PublishDraft { collection } => self.publish_draft(*collection).map(|()| None),
            Command::DeleteFiles { collection } => self.delete_files(*collection).map(|()| None),
            Command::ImportTorrent { source } => self.import_torrent(source).map(Some),
            Command::DownloadSelection {
                collection,
                entries,
            } => self
                .confirm_torrent_selection(*collection, entries)
                .map(|()| None),
            _ => Ok(None),
        }
    }

    /// Moves a draft to the one executable state appropriate to its kind.
    ///
    /// Idempotent: confirming something already shared is not an error, it is
    /// a second tap on a button whose first tap worked.
    fn publish_draft(&self, collection: Handle) -> Result<(), CommandError> {
        let key = self.collection_key(collection)?;
        let stored = self
            .store
            .collection(&key)
            .map_err(persistence)?
            .ok_or_else(|| missing_collection(collection))?;
        let lifecycle = match stored.lifecycle {
            StoredLifecycle::NativeDraft => Some(StoredLifecycle::NativePublished {
                activity: StoredActivity::Running,
            }),
            // Already published is idempotent, but still reaches `republish`
            // below: a crash can leave durable intent and revision in place
            // before the substrate handle is recorded.
            StoredLifecycle::NativePublished { .. } => None,
            StoredLifecycle::TorrentResolving
            | StoredLifecycle::TorrentAwaitingSelection
            | StoredLifecycle::TorrentRequested { .. } => {
                return Err(CommandError::Invalid(
                    "torrent imports can only be confirmed with Download".to_owned(),
                ));
            }
        };
        if let Some(lifecycle) = lifecycle {
            self.store
                .put_collection(
                    &key,
                    &StoredCollection {
                        lifecycle,
                        started_at: None,
                        completed_at: None,
                        ..stored
                    },
                )
                .map_err(persistence)?;
        }
        // The projection has to say so too. Writing the pause and leaving the
        // status alone left the interface offering Pause on something already
        // paused — the button read as inverted because the state behind it
        // and the state in front of it disagreed.
        self.refresh_status(collection, &key)?;
        self.republish(collection, &key);
        Ok(())
    }

    /// Recomputes one collection's status from the facts, and publishes it.
    ///
    /// Every command that changes what a collection is doing ends here rather
    /// than assigning a status it worked out for itself. Six call sites used
    /// to do the latter, and they disagreed.
    fn refresh_status(&self, handle: Handle, key: &[u8]) -> Result<(), CommandError> {
        let stored = self
            .store
            .collection(key)
            .map_err(persistence)?
            .ok_or_else(|| missing_collection(handle))?;
        let importing = self
            .store
            .torrent_import(key)
            .map_err(persistence)?
            .is_some();
        let revision = self
            .store
            .current_revision(key)
            .map_err(persistence)?
            .map_or(0, |(number, _)| number);
        let held = self.holdings.get(key);
        let status = crate::nexus::projection::state::status_for(
            crate::nexus::projection::state::StatusFacts::from_stored(
                &stored,
                revision,
                importing,
                held.as_ref(),
            ),
        );
        self.update_collection(handle, |collection| collection.status = status)
    }

    /// Wakes whichever worker owns this collection's next step.
    ///
    /// A collection with native sources is published; a torrent import is the
    /// torrent worker's business. One place decides which, so a caller that
    /// has just changed a collection does not have to know what kind it is.
    fn republish(&self, collection: Handle, key: &[u8]) {
        let is_import = self
            .store
            .torrent_import(key)
            .is_ok_and(|source| source.is_some());
        if is_import {
            self.torrents.notify_one();
        } else {
            self.publisher.notify_one();
        }
        self.refresh_detail(collection);
    }

    fn create_collection(&self, name: &str, files: &[LocalFile]) -> Result<Handle, CommandError> {
        let id = crate::nexus::collections::model::CollectionId::generate();
        let sources = prepare_sources(files)?;
        let stored = StoredCollection {
            name: name.to_owned(),
            role: StoredRole::Owner,
            content_key: crate::nexus::crypto::generate_content_key(),
            media_path: String::new(),
            sources: sources.clone(),
            // Chosen, not yet shared. Publishing waits for the person to say
            // so, which is what makes abandoning one cost nothing.
            lifecycle: StoredLifecycle::NativeDraft,
            on_disk_bytes: 0,
            substrate_handle: None,
            started_at: None,
            completed_at: None,
        };
        self.store
            .put_collection(id.as_bytes(), &stored)
            .map_err(persistence)?;

        let handle = self
            .collections
            .lock()
            .map_err(|_| CommandError::Persistence("the collection index was poisoned".to_owned()))?
            .assign(id.as_bytes().to_vec());
        let mut state = self.state();
        state.collections.push(project_stored_collection(
            handle,
            stored,
            0,
            Vec::new(),
            &[],
            false,
            None,
        ));
        self.states.send_replace(state);
        Ok(handle)
    }

    /// Records a torrent source and hands back its collection at once.
    ///
    /// What the source *contains* is resolved afterwards by the torrent
    /// worker, for a `.torrent` descriptor exactly as for a magnet. One path
    /// rather than two: a magnet's file list can only come from the swarm, so
    /// if the interface must handle "not known yet" for magnets it may as
    /// well be the only case there is — and a command that promises not to
    /// wait for a network cannot resolve one inline anyway.
    fn import_torrent(&self, source: &str) -> Result<Handle, CommandError> {
        crate::nexus::log::clog!(
            "nexus",
            "import_torrent accepted source_kind={} source_len={}",
            if crate::nexus::torrent::is_magnet(source) {
                "magnet"
            } else {
                "torrent_path"
            },
            source.len()
        );

        // Import identity is derived synchronously from the source itself
        // (a magnet's or `.torrent` file's BTv1 info hash), so duplicate
        // admission can be refused before this device contacts anything
        // (ADR-0015). The lock is held for the rest of this function —
        // imports are an occasional user action, not a hot path, and
        // holding it here is what makes two commands racing on the same
        // identity from different threads resolve to one durable row
        // instead of two: the second sees the first's already-durable
        // collection rather than a stale "not found yet" read.
        let identity = crate::nexus::torrent::canonical_import_identity(source);
        let _admission = self
            .importing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(identity) = identity.as_deref()
            && let Some(existing) = self.find_collection_by_import_identity(identity)?
        {
            crate::nexus::log::clog!(
                "nexus",
                "import_torrent identity={identity} already durable, returning existing collection"
            );
            return Ok(existing);
        }

        let id = crate::nexus::collections::model::CollectionId::generate();
        let stored = StoredCollection {
            // A placeholder until the source says its real name. Taken from
            // the source itself so the row is never nameless on screen.
            name: torrent_name(source),
            role: StoredRole::Owner,
            content_key: crate::nexus::crypto::generate_content_key(),
            media_path: String::new(),
            sources: Vec::new(),
            // Nothing is selectable until metadata resolution finishes.
            lifecycle: StoredLifecycle::TorrentResolving,
            on_disk_bytes: 0,
            substrate_handle: None,
            started_at: None,
            completed_at: None,
        };
        self.store
            .put_collection(id.as_bytes(), &stored)
            .map_err(persistence)?;
        if let Err(error) = self.store.put_torrent_import(id.as_bytes(), source) {
            // The collection was never valid without its source. Compensate
            // before returning rather than leaving a half-import visible.
            let _ = self.store.forget_collection(id.as_bytes());
            return Err(persistence(error));
        }

        let handle = self
            .collections
            .lock()
            .map_err(|_| CommandError::Persistence("the collection index was poisoned".to_owned()))?
            .assign(id.as_bytes().to_vec());
        let mut state = self.state();
        state.collections.push(project_stored_collection(
            handle,
            stored,
            0,
            Vec::new(),
            &[],
            true,
            None,
        ));
        self.states.send_replace(state);
        // The import worker resolves the durable source asynchronously.
        self.torrents.notify_one();
        Ok(handle)
    }

    /// The existing durable collection whose stored import source has the
    /// same canonical identity, if any.
    ///
    /// Fully store-driven rather than reading any in-memory cache, so a
    /// reimport of the same torrent after a process restart returns the
    /// existing collection exactly like one during the same run does.
    fn find_collection_by_import_identity(
        &self,
        identity: &str,
    ) -> Result<Option<Handle>, CommandError> {
        for (key, _stored) in self.store.collections().map_err(persistence)? {
            let Some(source) = self.store.torrent_import(&key).map_err(persistence)? else {
                continue;
            };
            if crate::nexus::torrent::canonical_import_identity(&source).as_deref()
                != Some(identity)
            {
                continue;
            }
            let handle = self
                .collections
                .lock()
                .map_err(|_| {
                    CommandError::Persistence("the collection index was poisoned".to_owned())
                })?
                .handle(&key);
            if handle.is_some() {
                return Ok(handle);
            }
        }
        Ok(None)
    }

    fn add_media(&self, handle: Handle, files: &[LocalFile]) -> Result<(), CommandError> {
        let key = self.collection_key(handle)?;
        let mut stored = self
            .store
            .collection(&key)
            .map_err(persistence)?
            .ok_or_else(|| missing_collection(handle))?;
        if stored.lifecycle != StoredLifecycle::NativeDraft {
            return Err(CommandError::Invalid(
                "only an unshared collection can receive more files".to_owned(),
            ));
        }
        if self
            .store
            .torrent_import(&key)
            .map_err(persistence)?
            .is_some()
        {
            return Err(CommandError::Invalid(
                "a torrent collection has a fixed file list".to_owned(),
            ));
        }

        let additions = prepare_sources(files)?;
        stored.sources.extend(additions);
        let mut names = stored
            .sources
            .iter()
            .map(|source| crate::nexus::torrent::SourceFile {
                name: source.label.clone(),
                path: source.path.clone(),
                length_bytes: Some(source.bytes),
            })
            .collect::<Vec<_>>();
        crate::nexus::torrent::make_source_names_unique(&mut names);
        for (stored, source) in stored.sources.iter_mut().zip(names) {
            stored.label = source.name;
        }
        self.store
            .put_collection(&key, &stored)
            .map_err(persistence)?;
        self.refresh_status(handle, &key)?;
        let entries = u32::try_from(stored.sources.len()).unwrap_or(u32::MAX);
        let total_bytes = stored.sources.iter().map(|source| source.bytes).sum();
        self.update_collection(handle, |collection| {
            collection.entries = entries;
            collection.total_bytes = total_bytes;
        })?;
        self.republish(handle, &key);
        Ok(())
    }

    fn rename_collection(&self, handle: Handle, name: &str) -> Result<(), CommandError> {
        let key = self.collection_key(handle)?;
        let mut stored = self
            .store
            .collection(&key)
            .map_err(persistence)?
            .ok_or_else(|| missing_collection(handle))?;
        stored.name = name.to_owned();
        self.store
            .put_collection(&key, &stored)
            .map_err(persistence)?;

        let mut state = self.state();
        let projected = state
            .collections
            .iter_mut()
            .find(|collection| collection.id == handle)
            .ok_or_else(|| missing_collection(handle))?;
        projected.name = stored.name;
        self.states.send_replace(state);
        Ok(())
    }

    /// Stops or resumes transferring one collection on this device.
    ///
    /// Durable before it is reported: a pause that a crash undoes would have
    /// this device quietly resume a transfer the person stopped, which is the
    /// one outcome the command exists to prevent.
    fn set_paused(&self, handle: Handle, paused: bool) -> Result<(), CommandError> {
        let key = self.collection_key(handle)?;
        let mut stored = self
            .store
            .collection(&key)
            .map_err(persistence)?
            .ok_or_else(|| missing_collection(handle))?;
        stored.lifecycle = match stored.lifecycle {
            StoredLifecycle::NativePublished { .. } => StoredLifecycle::NativePublished {
                activity: if paused {
                    StoredActivity::Paused
                } else {
                    StoredActivity::Running
                },
            },
            StoredLifecycle::TorrentRequested { .. } => StoredLifecycle::TorrentRequested {
                activity: if paused {
                    StoredActivity::Paused
                } else {
                    StoredActivity::Running
                },
            },
            StoredLifecycle::NativeDraft
            | StoredLifecycle::TorrentResolving
            | StoredLifecycle::TorrentAwaitingSelection => {
                return Err(CommandError::Invalid(
                    "only a requested collection can be paused or resumed".to_owned(),
                ));
            }
        };
        self.store
            .put_collection(&key, &stored)
            .map_err(persistence)?;
        // Recording the intent is not applying it. The reconciler is what
        // tells the engine, and it only runs when woken — without this the
        // interface reported Paused while the bytes kept moving, which is
        // exactly the kind of lie a pause must never be.
        self.republish(handle, &key);

        // Resuming hands the report back to whatever the engine is doing,
        // which this does not have to guess at — see `status_for`.
        self.refresh_status(handle, &key)
    }

    /// Removes the bytes this device holds and keeps the collection.
    ///
    /// The files go first and the count second. A crash between them leaves a
    /// count that is too high, which a person can correct by asking again; the
    /// other order leaves files nothing will ever account for.
    fn delete_files(&self, handle: Handle) -> Result<(), CommandError> {
        let key = self.collection_key(handle)?;
        let mut stored = self
            .store
            .collection(&key)
            .map_err(persistence)?
            .ok_or_else(|| missing_collection(handle))?;

        remove_media(&stored.media_path)?;
        stored.on_disk_bytes = 0;
        self.store
            .put_collection(&key, &stored)
            .map_err(persistence)?;

        self.update_collection(handle, |collection| {
            collection.on_disk_bytes = 0;
        })
    }

    /// Applies one change to a projected collection and republishes the state.
    fn update_collection(
        &self,
        handle: Handle,
        change: impl FnOnce(&mut CollectionState),
    ) -> Result<(), CommandError> {
        let mut state = self.state();
        let collection = state
            .collections
            .iter_mut()
            .find(|collection| collection.id == handle)
            .ok_or_else(|| missing_collection(handle))?;
        change(collection);
        self.states.send_replace(state);
        Ok(())
    }

    fn delete_collection(&self, handle: Handle, delete_files: bool) -> Result<(), CommandError> {
        let key = self.collection_key(handle)?;
        let stored = self.store.collection(&key).map_err(persistence)?;
        if delete_files {
            // Before the record goes, because afterwards there is nothing left
            // that knows where the files were.
            if let Some(media_path) = stored.as_ref().map(|stored| stored.media_path.clone()) {
                remove_media(&media_path)?;
            }
        }
        // Deleting the collection is the one moment its source references stop
        // being wanted. They are deliberately not dropped when a torrent is
        // merely released from the session: a collection can be unclaimed for a
        // moment without the person having asked for anything, and forgetting
        // where an owner's media lives is not recoverable.
        if let Some(substrate_handle) = stored.and_then(|stored| stored.substrate_handle)
            && let Err(error) = crate::nexus::torrent::forget_linked_sources(&substrate_handle)
        {
            crate::nexus::log::clog!(
                "nexus",
                "could not forget linked sources for {substrate_handle}: {error:#}"
            );
        }
        self.store
            .forget_torrent_import(&key)
            .map_err(persistence)?;
        self.store.forget_collection(&key).map_err(persistence)?;
        self.collections
            .lock()
            .map_err(|_| CommandError::Persistence("the collection index was poisoned".to_owned()))?
            .forget(handle);

        let mut state = self.state();
        let before = state.collections.len();
        state
            .collections
            .retain(|collection| collection.id != handle);
        if state.collections.len() == before {
            return Err(missing_collection(handle));
        }
        self.states.send_replace(state);
        Ok(())
    }

    /// Records exactly which resolved files the person confirmed. Starting
    /// the actual swarm transfer remains the substrate's job; keeping this
    /// choice durable first prevents a later engine restart from fetching a
    /// file the person deliberately excluded.
    fn confirm_torrent_selection(
        &self,
        collection: Handle,
        selected: &[Handle],
    ) -> Result<(), CommandError> {
        crate::nexus::log::clog!(
            "nexus",
            "confirm_torrent_selection collection={collection:?} entries={:?}",
            selected
        );
        if selected.is_empty() {
            return Err(CommandError::Invalid(
                "choose at least one file before downloading".to_owned(),
            ));
        }
        let key = self.collection_key(collection)?;
        if self
            .store
            .torrent_import(&key)
            .map_err(persistence)?
            .is_none()
        {
            return Err(CommandError::Invalid(
                "that collection is not a torrent import".to_owned(),
            ));
        }
        let mut entries = self
            .store
            .torrent_import_entries(&key)
            .map_err(persistence)?;
        let requested = selected
            .iter()
            .map(|handle| handle.0)
            .collect::<std::collections::HashSet<_>>();
        if requested.len() != selected.len()
            || requested.iter().any(|id| {
                usize::try_from(*id)
                    .ok()
                    .and_then(|id| id.checked_sub(1))
                    .is_none_or(|index| index >= entries.len())
            })
        {
            return Err(CommandError::Invalid(
                "the selected torrent file is no longer available".to_owned(),
            ));
        }
        let stored = self
            .store
            .collection(&key)
            .map_err(persistence)?
            .ok_or_else(|| missing_collection(collection))?;
        let requested_lifecycle = match stored.lifecycle {
            StoredLifecycle::TorrentAwaitingSelection => Some(StoredLifecycle::TorrentRequested {
                activity: StoredActivity::Running,
            }),
            // A repeated Download may update the durable selection, but it
            // does not silently resume a collection the person paused.
            StoredLifecycle::TorrentRequested { .. } => None,
            StoredLifecycle::TorrentResolving => {
                return Err(CommandError::Invalid(
                    "the torrent file list has not finished resolving".to_owned(),
                ));
            }
            StoredLifecycle::NativeDraft | StoredLifecycle::NativePublished { .. } => {
                return Err(CommandError::Invalid(
                    "that collection is not a torrent import".to_owned(),
                ));
            }
        };
        for (index, entry) in entries.iter_mut().enumerate() {
            entry.selected =
                requested.contains(&u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1));
        }
        // Commit selection first. A crash between these writes remains safely
        // AwaitingSelection; the unsafe inverse ordering could persist consent
        // with an old or default selection.
        self.store
            .put_torrent_import_entries(&key, &entries)
            .map_err(persistence)?;
        if let Some(lifecycle) = requested_lifecycle {
            self.store
                .put_collection(
                    &key,
                    &StoredCollection {
                        lifecycle,
                        started_at: None,
                        completed_at: None,
                        ..stored
                    },
                )
                .map_err(persistence)?;
        }
        self.refresh_status(collection, &key)?;
        self.republish(collection, &key);
        self.refresh_detail(collection);
        // Notify coalesces duplicate wakes; it never reports a full queue.
        self.torrents.notify_one();
        Ok(())
    }

    fn collection_key(&self, handle: Handle) -> Result<Vec<u8>, CommandError> {
        self.collections
            .lock()
            .map_err(|_| CommandError::Persistence("the collection index was poisoned".to_owned()))?
            .key(handle)
            .map(ToOwned::to_owned)
            .ok_or_else(|| missing_collection(handle))
    }

    /// The import URI for a collection the local substrate is actually
    /// carrying. A collection handle is process-local, so it cannot be shown
    /// to another device; the persisted BitTorrent info hash is the stable
    /// identifier that the existing import flow understands.
    pub fn share_uri(&self, collection: Handle) -> Result<Option<String>, CommandError> {
        let key = self.collection_key(collection)?;
        let Some(handle) = self
            .store
            .collection(&key)
            .map_err(persistence)?
            .and_then(|stored| stored.substrate_handle)
        else {
            return Ok(None);
        };
        // Do not turn a damaged local store row into a QR code that claims to
        // name a torrent. A valid v1 info hash is exactly forty hex digits.
        if handle.len() != 40 || !handle.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(None);
        }
        let Some(peer_hints) = crate::nexus::torrent::share_peer_hints(&handle) else {
            return Ok(None);
        };
        crate::nexus::log::clog!(
            "nexus",
            "share QR for {collection:?}: info_hash={handle}, direct_peer_hints={:?}",
            peer_hints.as_slice()
        );
        Ok(Some(crate::nexus::torrent::magnet_for_share(
            &handle,
            &peer_hints,
        )))
    }

    fn collection_detail(&self, collection: Handle) -> Option<Detail> {
        self.detail_sources().build(collection)
    }

    /// Every collection with a substrate handle, as `(handle, name, hash)`.
    ///
    /// For the storage view, which has directories on disk and needs to say
    /// which collection each belongs to. The substrate handle is the only
    /// honest join: a name is not unique and a process-local handle means
    /// nothing on disk.
    #[must_use]
    pub fn carried_collections(&self) -> Vec<(Handle, String, String)> {
        let Ok(stored) = self.store.collections() else {
            return Vec::new();
        };
        let index = self
            .collections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        stored
            .into_iter()
            .filter_map(|(key, collection)| {
                let handle = index.handle(&key)?;
                Some((handle, collection.name, collection.substrate_handle?))
            })
            .collect()
    }

    fn detail_sources(&self) -> DetailSources {
        DetailSources {
            store: Arc::clone(&self.store),
            collections: Arc::clone(&self.collections),
            holdings: self.holdings.clone(),
            senders: Arc::clone(&self.details),
        }
    }

    /// This collection's transfer history, packed for the bridge.
    ///
    /// Empty rather than absent when the history cannot be read: a chart with
    /// no points is a truthful "nothing recorded yet", and refusing the whole
    /// detail tier over it would blank a screen that has plenty else to show.
    /// Publishes a new snapshot, if it differs from the last one sent.
    ///
    /// The projector decides; this only carries the result. Called by whatever
    /// component just changed something, rather than on a timer, because a
    /// timer is a poll wearing a different hat.
    pub fn publish(
        &self,
        state: &PortalisState,
        detail: Option<&Detail>,
        now: std::time::Duration,
    ) {
        let emission = self
            .projector
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .tick(state, detail, now);

        if let Some(state) = emission.state {
            self.states.send_replace(state);
        }
        if let Some(detail) = emission.detail {
            let id = detail.id;
            if let Some(sender) = self.detail_senders().get(&id) {
                sender.send_replace(Some(detail));
            }
        }
    }

    /// Stops every component and returns when the runtime is quiet.
    pub async fn close(self) {
        self.supervisor.shutdown().await;
        if let Err(error) = self.activity.finish(unix_time_ns()) {
            crate::nexus::log::clog!(
                "activity",
                "could not checkpoint graceful shutdown: {error}"
            );
        }
    }
}

/// A failed publish (most often a Photos asset PhotoKit could not stream —
/// a stalled iCloud fetch, a revoked permission, a transient read timeout)
/// remains durable work. Same backoff ladder `follow_torrent_imports` uses
/// for the receiver side, so a transient hiccup recovers on its own instead
/// of leaving a collection in `ResolvingMetadata` forever with no retry and
/// no visible error.
const PUBLISH_RETRY_DELAYS: [std::time::Duration; 4] = [
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(15),
    std::time::Duration::from_secs(30),
    std::time::Duration::from_secs(60),
];

fn publish_retry_delay(failures: u32) -> std::time::Duration {
    PUBLISH_RETRY_DELAYS[usize::try_from(failures.saturating_sub(1))
        .unwrap_or(3)
        .min(3)]
}

async fn publish_pending_collections(
    store: Arc<Store>,
    states: watch::Sender<PortalisState>,
    collections: Arc<Mutex<LocalCollections>>,
    substrate: Arc<dyn crate::nexus::substrate::Substrate>,
    wake: Arc<Notify>,
    mut shutdown: super::supervisor::Shutdown,
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

        let pending = match store.collections() {
            Ok(collections) => collections
                .into_iter()
                .filter(|(_, collection)| {
                    // A draft is deliberately skipped: its files are chosen
                    // but not offered, and hashing them would start seeding
                    // something the person has not said to share. Asked of the
                    // lifecycle rather than of a flag, so this worker and the
                    // torrent worker cannot disagree about what a draft is.
                    collection.lifecycle.is_requested()
                        && !collection.sources.is_empty()
                        && collection.substrate_handle.is_none()
                })
                .collect::<Vec<_>>(),
            Err(error) => {
                crate::nexus::log::clog!("nexus", "could not scan pending collections: {error}");
                continue;
            }
        };
        let now = Instant::now();
        let due = pending
            .into_iter()
            .filter(|(key, _)| {
                retry_deadlines
                    .get(key)
                    .is_none_or(|deadline| *deadline <= now)
            })
            .collect::<Vec<_>>();

        for (key, collection) in due {
            let total = collection.sources.iter().map(|source| source.bytes).sum();
            crate::nexus::log::clog!(
                "nexus",
                "publisher starting collection key={:?} name={:?} sources={} bytes={}",
                key,
                collection.name,
                collection.sources.len(),
                total
            );
            let progress = crate::nexus::torrent::PublishProgress::new(total);
            let key_for_progress = key.clone();

            // Spawn a task that periodically surfaces the publish progress to the UI.
            let progress_for_ui = progress.clone();
            let states_for_ui = states.clone();
            let collections_for_ui = Arc::clone(&collections);
            let progress_updater = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
                loop {
                    interval.tick().await;
                    if progress_for_ui.is_cancelled() {
                        break;
                    }
                    let snapshot = progress_for_ui.snapshot();
                    if snapshot.stage != "preparing" {
                        let handle_opt = collections_for_ui
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .handle(&key_for_progress);
                        if let Some(handle) = handle_opt {
                            let _ = states_for_ui.send_if_modified(|state| {
                                if let Some(projected) =
                                    state.collections.iter_mut().find(|p| p.id == handle)
                                {
                                    let changed =
                                        projected.publish_progress != Some(snapshot.clone());
                                    projected.publish_progress = Some(snapshot);
                                    changed
                                } else {
                                    false
                                }
                            });
                        }
                    }
                }
            });

            let publishing = publish_collection_sources(
                &store,
                substrate.as_ref(),
                &key,
                &collection,
                progress.clone(),
            );
            tokio::pin!(publishing);
            let result = tokio::select! {
                () = shutdown.requested() => {
                    progress.cancel();
                    progress_updater.abort();
                    return;
                }
                result = &mut publishing => result,
            };
            // The ticker reports on this publication and must not outlive it.
            // Left running, it woke 500ms later and wrote its final snapshot
            // back over the cleared field below — re-asserting a hashing bar
            // on a finished collection forever, and leaking one task per
            // publication.
            progress_updater.abort();
            match result {
                Ok(revision) => {
                    crate::nexus::log::clog!(
                        "nexus",
                        "publisher completed collection key={:?} name={:?} revision={revision}",
                        key,
                        collection.name
                    );
                    failures.remove(&key);
                    retry_deadlines.remove(&key);
                    let handle = collections
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .handle(&key);
                    // Publishing settles the revision. What the collection is
                    // then doing is not this worker's call — a confirmed draft
                    // is paused on purpose, and declaring it Available here
                    // made the interface offer Pause on something stopped.
                    let status = store.collection(&key).ok().flatten().map(|stored| {
                        crate::nexus::projection::state::status_for(
                            crate::nexus::projection::state::StatusFacts::from_stored(
                                &stored, revision, false, None,
                            ),
                        )
                    });
                    if let Some(handle) = handle {
                        states.send_modify(|state| {
                            if let Some(projected) = state
                                .collections
                                .iter_mut()
                                .find(|projected| projected.id == handle)
                            {
                                projected.revision = revision;
                                if let Some(status) = status {
                                    projected.status = status;
                                }
                                projected.publish_progress = None;
                            }
                        });
                    }
                }
                Err(error) => {
                    let count = failures
                        .entry(key.clone())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                    let delay = publish_retry_delay(*count);
                    retry_deadlines.insert(key.clone(), Instant::now() + delay);
                    crate::nexus::log::clog!(
                        "nexus",
                        "publisher failed collection key={:?} name={:?} failure={} retry_after={}s: {error:#}",
                        key,
                        collection.name,
                        *count,
                        delay.as_secs()
                    );
                    // Surface the stall instead of leaving the interface
                    // showing "resolving metadata" forever with no sign
                    // anything went wrong or is being retried.
                    if let Some(handle) = collections
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .handle(&key)
                    {
                        states.send_if_modified(|state| {
                            let Some(projected) =
                                state.collections.iter_mut().find(|item| item.id == handle)
                            else {
                                return false;
                            };
                            if projected.status
                                == crate::nexus::projection::state::Status::RetryingMetadata
                            {
                                return false;
                            }
                            projected.status =
                                crate::nexus::projection::state::Status::RetryingMetadata;
                            projected.publish_progress = None;
                            true
                        });
                    }
                }
            }
        }
    }
}

async fn publish_collection_sources(
    store: &Store,
    substrate: &dyn crate::nexus::substrate::Substrate,
    key: &[u8],
    stored: &StoredCollection,
    progress: crate::nexus::torrent::PublishProgress,
) -> anyhow::Result<u64> {
    use anyhow::Context;
    use portalis_nexus_protocol::INFO_HASH_BYTES;

    let files = stored
        .sources
        .iter()
        .map(|source| crate::nexus::torrent::SourceFile {
            name: source.label.clone(),
            path: source.path.clone(),
            length_bytes: Some(source.bytes),
        })
        .collect();
    let published_torrent = substrate
        .publish(stored.name.clone(), files, progress)
        .await?;
    crate::nexus::log::clog!(
        "nexus",
        "publisher substrate returned collection key={:?} name={:?} info_hash={} descriptor_bytes={}",
        key,
        stored.name,
        published_torrent.info.info_hash,
        published_torrent.descriptor.len()
    );
    let info_hash: [u8; INFO_HASH_BYTES] = hex::decode(&published_torrent.info.info_hash)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("published torrent returned an invalid info hash"))?;
    let descriptor = published_torrent.descriptor;
    let collection_id = <[u8; portalis_nexus_protocol::SHARE_ID_BYTES]>::try_from(key)
        .map_err(|_| anyhow::anyhow!("stored collection key has the wrong length"))?;
    let author = crate::nexus::device::current_signing_identity()?;
    let mut collection = crate::nexus::collections::model::Collection {
        id: crate::nexus::collections::model::CollectionId(collection_id),
        name: stored.name.clone(),
        role: stored.role,
        content_key: stored.content_key,
        revision: None,
        manifest: portalis_nexus_protocol::Manifest::default(),
    };
    crate::nexus::collections::publish::add_entry(
        &mut collection,
        &author,
        info_hash,
        stored.name.clone(),
        None,
        unix_time_ns(),
    )?;
    let (published, publication) = crate::nexus::collections::publish::publish(
        &collection,
        &author,
        &[],
        &[(info_hash, descriptor.clone())],
        unix_time_ns(),
    )?;
    let manifest_hash = published.manifest.hash();
    store
        .put_manifest(&manifest_hash, &published.manifest.encode())
        .context("persisting the initial Nexus manifest")?;
    store
        .put_entry(
            &info_hash,
            &crate::nexus::store::records::StoredEntry {
                status: crate::nexus::store::records::EntryStatus::Available,
                descriptor,
            },
        )
        .context("persisting the initial Nexus descriptor")?;
    store
        .put_revision(
            key,
            publication.revision.number,
            &publication.revision.encode(),
        )
        .context("persisting the initial Nexus revision")?;
    // Recorded last, and only once the revision is durable: the handle is what
    // attributes a holding back to this collection, and a handle pointing at a
    // collection that failed to publish would attribute transfers to nothing.
    store
        .put_collection(
            key,
            &StoredCollection {
                substrate_handle: Some(published_torrent.info.info_hash.clone()),
                ..stored.clone()
            },
        )
        .context("recording the collection's substrate handle")?;
    Ok(publication.revision.number)
}

/// How often an owner's zero-copy sources are checked for still existing.
///
/// Independent of [`super::transfers::POLL_INTERVAL`]: that poller only
/// hears from the substrate about torrents it is actively carrying, and
/// tells nothing about whether the *files themselves* moved, were renamed,
/// or were deleted underneath the app. Only a filesystem stat answers that.
const SOURCE_SANITY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Watches every collection this device owns for a source that has gone
/// missing on disk, and — the moment one has — folds it back into the
/// ordinary receiver download path instead of leaving it silently unseedable.
///
/// No new status exists for this. A missing source simply means this device
/// can no longer act as this collection's seed, which is exactly what a
/// receiver's collection already looks like: known content, no local bytes,
/// waiting on the swarm. So the fix is not a new lifecycle branch — it is
/// converting the row to look like any other unfinished download, of its own
/// published content, and letting `follow_torrent_imports` (the worker that
/// already knows how to resolve and acquire a torrent) take it from there.
///
/// Every use case this covers, and why each needs no special handling beyond
/// this one conversion:
/// - **One of several sources deleted.** Detected the same way as all of
///   them missing — presence is checked per collection, not per file, since
///   a torrent generated from a partial local set is not the same torrent
///   any peer already holds; the receiver path re-fetches the whole thing
///   from whoever else has it, exactly as it would for a magnet found on
///   another device first.
/// - **A source renamed or moved rather than deleted.** Indistinguishable
///   from "deleted" by a stat call, and correctly so: this device no longer
///   has the referenced bytes at the path it promised, whatever the reason.
/// - **The collection is paused when its source disappears.** Left alone
///   until pause is lifted — a paused collection is a person's decision, and
///   converting it to "waiting to redownload" while they intended it to sit
///   idle would silently start recovering something they explicitly stopped.
///   The next tick after they resume catches it.
/// - **A draft's source disappears before it is ever shared.** Also left
///   alone: nothing has been published, nobody else may hold a copy, and
///   converting a draft to a download would ask the swarm for content that
///   was never offered to it. The draft still holds a stale reference; the
///   person notices reopening it, same as any other broken pick.
/// - **The receiver side (a collection this device is downloading, not
///   seeding).** Entirely out of scope: `stored.sources` is empty for a
///   torrent import, so nothing here ever matches one. Nothing to convert —
///   the ordinary download path already owns its lifecycle.
/// - **This device is both a peer's only seed and its source vanishes.**
///   Handled the same as any other missing source: the local row converts,
///   this device stops being available to publish/seed until re-added, and
///   any peer already relying on it experiences exactly what a seed going
///   permanently offline looks like — nothing this app can paper over from
///   one side of the connection.
async fn watch_source_sanity(
    store: Arc<Store>,
    torrents: Arc<Notify>,
    mut shutdown: super::supervisor::Shutdown,
) {
    let mut tick = tokio::time::interval(SOURCE_SANITY_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            () = shutdown.requested() => return,
            _ = tick.tick() => {}
        }

        let collections = match store.collections() {
            Ok(collections) => collections,
            Err(error) => {
                crate::nexus::log::clog!(
                    "nexus",
                    "could not scan collections for source sanity: {error}"
                );
                continue;
            }
        };

        let mut converted = false;
        for (key, stored) in collections {
            // Only an owner with a substrate handle can have lost a source
            // out from under an active seed. A draft has offered nothing to
            // anyone yet (see the doc comment above), and an empty source
            // list is a receiver's own download, never this worker's case.
            if stored.sources.is_empty() || stored.substrate_handle.is_none() {
                continue;
            }
            if stored
                .lifecycle
                .activity()
                .is_some_and(crate::nexus::store::records::StoredActivity::is_paused)
            {
                continue;
            }
            let all_present = stored.sources.iter().all(|source| {
                crate::nexus::content_location::ContentLocation::from_source_path(&source.path)
                    .is_ok_and(|location| location.length(Some(source.bytes)).is_ok())
            });
            if all_present {
                continue;
            }

            let missing = stored
                .sources
                .iter()
                .filter(|source| {
                    !crate::nexus::content_location::ContentLocation::from_source_path(&source.path)
                        .is_ok_and(|location| location.length(Some(source.bytes)).is_ok())
                })
                .map(|source| source.path.as_str())
                .collect::<Vec<_>>();
            crate::nexus::log::clog!(
                "nexus",
                "source sanity: collection key={:?} name={:?} lost {} of {} sources ({:?}); converting to a download of its own content",
                key,
                stored.name,
                missing.len(),
                stored.sources.len(),
                missing
            );

            let Some(handle) = stored.substrate_handle.clone() else {
                continue;
            };
            let magnet = format!("magnet:?xt=urn:btih:{handle}");
            let entries = crate::nexus::linked_source_store::sources_for(&handle)
                .ok()
                .flatten()
                .map(|sources| {
                    sources
                        .into_iter()
                        .map(|source| StoredImportEntry {
                            label: source.name,
                            bytes: source.length_bytes.unwrap_or_default(),
                            selected: true,
                            native_location: None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let descriptor = crate::nexus::linked_source_store::descriptor_for(&handle).ok();

            if let Err(error) = store.put_collection(
                &key,
                &StoredCollection {
                    sources: Vec::new(),
                    substrate_handle: None,
                    lifecycle: if entries.is_empty() {
                        StoredLifecycle::TorrentResolving
                    } else {
                        StoredLifecycle::TorrentRequested {
                            activity: StoredActivity::Running,
                        }
                    },
                    ..stored
                },
            ) {
                crate::nexus::log::clog!(
                    "nexus",
                    "source sanity: could not convert collection key={:?} to a download: {error}",
                    key
                );
                continue;
            }
            if let Err(error) = store.put_torrent_import(&key, &magnet) {
                crate::nexus::log::clog!(
                    "nexus",
                    "source sanity: could not record the import source for key={:?}: {error}",
                    key
                );
                continue;
            }
            if !entries.is_empty()
                && let Err(error) = store.put_torrent_import_entries(&key, &entries)
            {
                crate::nexus::log::clog!(
                    "nexus",
                    "source sanity: could not record file entries for key={:?}: {error}",
                    key
                );
            }
            if let Some(descriptor) = descriptor
                && let Err(error) = store.put_torrent_import_descriptor(&key, &descriptor)
            {
                crate::nexus::log::clog!(
                    "nexus",
                    "source sanity: could not persist the descriptor for key={:?}: {error}",
                    key
                );
            }
            let _ = crate::nexus::linked_source_store::remove(&handle);
            converted = true;
        }
        if converted {
            // Either worker can now pick this collection up: with entries
            // already known it goes straight to acquiring, otherwise the
            // torrent worker resolves the magnet first exactly as it would
            // for any freshly imported one.
            torrents.notify_one();
        }
    }
}

/// One collection's transfer history, packed for the bridge.
///
/// Empty rather than absent when the history cannot be read: a chart with no
/// points is a truthful "nothing recorded yet", and refusing the whole detail
/// tier over it would blank a screen that has plenty else to show.
/// Packs readings for the wire, oldest first.
///
/// Only ever called with what a subscriber does not already hold — the
/// history grows at the end, so re-sending the whole ring to append one row
/// was thirty kilobytes a second for a screen already showing all of it.
pub fn packed_samples(samples: Vec<(u64, crate::nexus::store::records::StoredSample)>) -> Vec<u8> {
    {
        {
            let mut packed = Vec::with_capacity(samples.len() * SAMPLE_ROW_BYTES);
            for (at_unix_ns, sample) in samples {
                packed.extend_from_slice(&at_unix_ns.to_be_bytes());
                packed.extend_from_slice(&sample.down_bytes_per_second.to_be_bytes());
                packed.extend_from_slice(&sample.up_bytes_per_second.to_be_bytes());
                packed.extend_from_slice(&progress_permille(&sample).to_be_bytes());
            }
            packed
        }
    }
}

/// One packed history row: `at_unix_ns ‖ down ‖ up ‖ progress`.
///
/// Fixed width so the far side reads it with an offset rather than a parser,
/// and big-endian so it is read the same way everywhere.
pub const SAMPLE_ROW_BYTES: usize = 8 + 4 + 4 + 2;

/// Progress as thousandths, which is the resolution a chart can show.
///
/// Sent as an integer rather than a float because the bridge carries bytes:
/// a `f32` would need its own encoding and would claim a precision no progress
/// bar has pixels for.
fn progress_permille(sample: &crate::nexus::store::records::StoredSample) -> u16 {
    if sample.total == 0 {
        return 0;
    }
    let permille = sample.done.saturating_mul(1000) / sample.total;
    u16::try_from(permille).unwrap_or(1000).min(1000)
}

/// One bit per piece, packed, from the runs the substrate reports.
///
/// The substrate speaks in byte ranges per file; a person sees one bar for the
/// whole collection. Verified runs become set bits and everything else stays
/// clear, so a missing range needs no representation of its own.
fn pieces_of(info: &crate::nexus::torrent::TorrentInfo) -> Vec<u8> {
    const PIECES: usize = 512;
    if info.total_bytes == 0 {
        return Vec::new();
    }
    let mut bits = vec![0_u8; PIECES.div_ceil(8)];
    let mut base = 0_u64;
    for file in &info.files {
        for run in &file.piece_runs {
            if run.verified {
                let from = span(base + run.offset_bytes, info.total_bytes, PIECES);
                let to = span(
                    base + run.offset_bytes + run.length_bytes,
                    info.total_bytes,
                    PIECES,
                );
                for piece in from..to.min(PIECES) {
                    bits[piece / 8] |= 1 << (piece % 8);
                }
            }
        }
        base += file.length_bytes;
    }
    bits
}

/// Which of `pieces` bars a byte offset falls in.
fn span(offset_bytes: u64, total_bytes: u64, pieces: usize) -> usize {
    let pieces = pieces as u64;
    usize::try_from(offset_bytes.saturating_mul(pieces) / total_bytes).unwrap_or(usize::MAX)
}

fn unix_time_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
        .unwrap_or_default()
}

fn prepare_sources(files: &[LocalFile]) -> Result<Vec<StoredSourceFile>, CommandError> {
    let mut sources = files
        .iter()
        .map(|file| crate::nexus::torrent::SourceFile {
            name: file.name.clone(),
            path: file.path.to_string_lossy().into_owned(),
            length_bytes: Some(file.bytes),
        })
        .collect::<Vec<_>>();
    crate::nexus::torrent::make_source_names_unique(&mut sources);
    sources
        .into_iter()
        .map(|source| {
            let location =
                crate::nexus::content_location::ContentLocation::from_source_path(&source.path)
                    .map_err(|error| CommandError::Invalid(error.to_string()))?;
            let bytes = location
                .length(source.length_bytes)
                .map_err(|error| CommandError::Invalid(error.to_string()))?;
            Ok(StoredSourceFile {
                label: source.name,
                path: source.path,
                bytes,
            })
        })
        .collect()
}

/// Removes a collection's downloaded bytes from this device.
///
/// An empty path is not an error: a collection whose media directory was never
/// chosen has nothing to remove, and neither has one already emptied. Nor is a
/// path that is not there — the outcome asked for is that the files are gone.
fn remove_media(media_path: &str) -> Result<(), CommandError> {
    if media_path.is_empty() {
        return Ok(());
    }
    match std::fs::remove_dir_all(media_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CommandError::Persistence(format!(
            "the downloaded files could not be removed: {error}"
        ))),
    }
}

fn persistence(error: StoreError) -> CommandError {
    CommandError::Persistence(error.to_string())
}

fn missing_collection(handle: Handle) -> CommandError {
    CommandError::Invalid(format!("collection {} is no longer available", handle.0))
}

/// What can be decided about a command without touching anything.
///
/// Deliberately shallow. Whether a collection exists, whether a peer will
/// accept — those are answers that need state or a network, and pretending to
/// give them here would make `command` slow and still wrong.
fn validate(command: &Command) -> Result<(), CommandError> {
    // One arm per complaint, guarded rather than nested, so the shape of this
    // function is "what is wrong with it" rather than "which command is it".
    let complaint = match command {
        Command::CreateCollection { name, .. } | Command::RenameCollection { name, .. }
            if name.trim().is_empty() =>
        {
            "a collection needs a name"
        }
        Command::AddContact { handle } if !handle.contains('#') => "a handle looks like ada#7Q2XZ",
        Command::AddMedia { files, .. } if files.is_empty() => "no files were chosen",
        Command::ImportTorrent { source } if source.trim().is_empty() => {
            "choose a magnet URI or .torrent file"
        }
        Command::ImportTorrent { source }
            if !crate::nexus::torrent::is_magnet(source)
                && !crate::nexus::torrent::is_torrent_path(source) =>
        {
            "choose a magnet URI or a .torrent file"
        }
        _ => return Ok(()),
    };
    Err(CommandError::Invalid(complaint.to_owned()))
}

fn torrent_name(source: &str) -> String {
    // A URL's tail is not a name: a magnet ends in whatever its last query
    // parameter happens to be, which is frequently somebody else's filename.
    if crate::nexus::torrent::is_remote_source(source) {
        return "Portalis collection import".to_owned();
    }
    std::path::Path::new(source)
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map_or_else(
            || "Portalis collection import".to_owned(),
            ToOwned::to_owned,
        )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::nexus::core::supervisor::Shutdown;
    use crate::nexus::core::transfers::{self as transfers, Holdings};
    use crate::nexus::projection::state::{CollectionState, Role, Status};
    use crate::nexus::store::records::{StoredContact, StoredImportEntry};

    /// A directory that removes itself.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "portalis-nexus-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn open(scratch: &Scratch) -> Nexus {
        open_with_substrate(
            scratch,
            Arc::new(crate::nexus::substrate::Recorded::default()),
        )
    }

    #[tokio::test]
    async fn activity_summary_starts_a_run_and_clearing_preserves_collections() {
        let scratch = Scratch::new("activity-summary");
        let nexus = open(&scratch);
        std::fs::write(scratch.0.join("kept.bin"), b"data").expect("writes source file");

        nexus
            .command(&Command::CreateCollection {
                name: "kept".to_owned(),
                files: vec![LocalFile {
                    name: "kept.bin".to_owned(),
                    path: scratch.0.join("kept.bin"),
                    bytes: 4,
                }],
            })
            .expect("accepts import");

        let before = nexus.activity_summary().expect("reads summary");
        assert_eq!(before.activity.runs_started, 1);
        assert_eq!(
            before.run.end_reason,
            crate::nexus::store::records::AppRunEnd::Current
        );

        nexus.clear_activity().expect("clears activity");
        let after = nexus.activity_summary().expect("reads summary");
        assert_eq!(
            after.activity.runs_started, 1,
            "clearing starts a fresh run"
        );
        assert!(after.recent_runs.len() <= 1);

        let survives = nexus
            .state()
            .collections
            .iter()
            .any(|collection| collection.name == "kept");
        assert!(survives, "clearing activity never destroys collections");

        nexus.close().await;
    }

    #[tokio::test]
    async fn public_views_and_add_media_use_the_durable_nexus_state() {
        let scratch = Scratch::new("public-views-and-add-media");
        let nexus = open(&scratch);
        let source = scratch.0.join("photo.jpg");
        std::fs::write(&source, b"image").expect("writes source file");

        let accepted = nexus
            .command(&Command::CreateCollection {
                name: "Photos".to_owned(),
                files: Vec::new(),
            })
            .expect("creates a collection");
        let collection = accepted.collection.expect("names the collection");

        let mut state = nexus.watch();
        assert_eq!(state.borrow().device.name, "Ada's laptop");
        let bus = nexus.events_bus();
        let _subscription = bus.subscribe().await;
        assert_eq!(bus.subscribers().await, 1);
        assert!(nexus.peers().is_empty());
        assert!(nexus.peer_history(collection).is_empty());
        assert!(nexus.watch_detail(None).borrow().is_none());

        nexus
            .command(&Command::AddMedia {
                collection,
                label: "photo.jpg".to_owned(),
                files: vec![LocalFile {
                    name: "photo.jpg".to_owned(),
                    path: source,
                    bytes: 5,
                }],
            })
            .expect("adds media to the draft");

        state.changed().await.expect("receives updated snapshot");
        let snapshot = state.borrow().clone();
        let projected = snapshot
            .collections
            .iter()
            .find(|item| item.id == collection)
            .expect("collection remains projected");
        assert_eq!(projected.entries, 1);
        let key = nexus
            .collection_key(collection)
            .expect("finds collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        assert_eq!(stored.sources.len(), 1);
        assert_eq!(stored.sources[0].bytes, 5);

        nexus.close().await;
    }

    /// Waits for a background worker to reach `done`, or fails the test.
    ///
    /// Bounded and condition-driven rather than a fixed sleep: a worker that
    /// never runs must fail loudly instead of passing on a slow machine and
    /// failing on a fast one.
    async fn settle(
        _nexus: &Nexus,
        watching: &mut watch::Receiver<Option<Detail>>,
        done: impl Fn(Option<&Detail>) -> bool,
    ) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if done(watching.borrow().as_ref()) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the worker never reached the expected state"
            );
            // Either the detail changed, or enough time passed to re-check a
            // condition that does not depend on it.
            let _ = tokio::time::timeout(Duration::from_millis(20), watching.changed()).await;
        }
    }

    fn open_with_substrate(
        scratch: &Scratch,
        substrate: Arc<dyn crate::nexus::substrate::Substrate>,
    ) -> Nexus {
        let config = Config {
            data_dir: scratch.0.clone(),
            device_name: "Ada's laptop".to_owned(),
            fingerprint: "ada-fingerprint".to_owned(),
        };
        Nexus::open_with_store_and_substrate(
            &config,
            Arc::new(Store::open(scratch.0.join("portalis.redb")).expect("opens store")),
            substrate,
        )
        .expect("opens")
    }

    fn collection(name: &str) -> CollectionState {
        CollectionState {
            started_at: None,
            completed_at: None,
            id: Handle(1),
            name: name.to_owned(),
            nature: Nature::Native,
            role: Role::Owner,
            revision: 1,
            status: Status::Available,
            members: Vec::new(),
            entries: 1,
            total_bytes: 10,
            on_disk_bytes: 0,
            uploaded_bytes: 0,
            transfer: None,
            pending: None,
            publish_progress: None,
        }
    }

    fn state(collections: Vec<CollectionState>) -> PortalisState {
        PortalisState {
            device: DeviceState {
                name: "Ada's laptop".to_owned(),
                handle: None,
                fingerprint: String::new(),
                devices: 1,
            },
            connectivity: Connectivity::LocalOnly,
            contacts: Vec::new(),
            collections,
            alerts: Vec::new(),
        }
    }

    /// A collection with a media directory holding one downloaded file.
    fn with_media(nexus: &Nexus, scratch: &Scratch, bytes: u64) -> (Handle, std::path::PathBuf) {
        nexus
            .command(&Command::CreateCollection {
                name: "Iceland".to_owned(),
                files: Vec::new(),
            })
            .expect("creates locally");
        let handle = nexus.state().collections[0].id;

        let media = scratch.0.join("media");
        std::fs::create_dir_all(&media).expect("a media directory");
        std::fs::write(media.join("one.jpg"), vec![0_u8; 4]).expect("a downloaded file");

        let key = nexus.collection_key(handle).expect("a known collection");
        let mut stored = nexus
            .store
            .collection(&key)
            .expect("reads")
            .expect("exists");
        stored.media_path = media.to_string_lossy().into_owned();
        stored.on_disk_bytes = bytes;
        nexus.store.put_collection(&key, &stored).expect("writes");

        (handle, media)
    }

    #[tokio::test]
    async fn only_a_carried_collection_has_a_share_uri() {
        let scratch = Scratch::new("collection-share-uri");
        let nexus = open(&scratch);
        let accepted = nexus
            .command(&Command::CreateCollection {
                name: "Iceland".to_owned(),
                files: Vec::new(),
            })
            .expect("creates a collection");
        let collection = accepted.collection.expect("names the collection");

        assert_eq!(nexus.share_uri(collection).expect("known collection"), None);

        let key = nexus.collection_key(collection).expect("finds its key");
        let mut stored = nexus
            .store
            .collection(&key)
            .expect("reads")
            .expect("exists");
        stored.substrate_handle = Some("01".repeat(20));
        nexus.store.put_collection(&key, &stored).expect("writes");

        assert!(
            nexus
                .share_uri(collection)
                .expect("checks live readiness")
                .is_none(),
            "a durable handle without a live loaded torrent must not produce a QR"
        );
        nexus.close().await;
    }

    /// A draft is private to this device. Nothing is hashed, nothing is
    /// offered, and abandoning it leaves no trace anywhere — which is the
    /// whole reason choosing files does not publish them.
    #[tokio::test]
    async fn a_draft_is_not_published_until_it_is_confirmed() {
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("draft-waits");
        let source = scratch.0.join("clip.mp4");
        std::fs::write(&source, b"clip").expect("writes source");
        let substrate = Arc::new(crate::nexus::substrate::Recorded::publishing(
            "22".repeat(20),
            b"descriptor".to_vec(),
        ));
        let nexus = open_with_substrate(&scratch, substrate.clone());

        let accepted = nexus
            .command(&Command::CreateCollection {
                name: "Holiday".to_owned(),
                files: vec![LocalFile {
                    name: "clip.mp4".to_owned(),
                    path: source,
                    bytes: 4,
                }],
            })
            .expect("accepts the files");
        let collection = accepted.collection.expect("names the collection");

        // Long enough that the publisher would have run if it were going to.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            substrate.published.lock().unwrap().is_empty(),
            "a draft is never handed to the substrate"
        );
        assert_eq!(nexus.state().collections[0].status, Status::Draft);

        let refused = nexus
            .command(&Command::SetPaused {
                collection,
                paused: true,
            })
            .expect_err("a draft has no transfer to pause");
        assert!(matches!(refused, CommandError::Invalid(_)));
        let key = nexus.collection_key(collection).expect("collection key");
        assert_eq!(
            nexus
                .store
                .collection(&key)
                .expect("reads")
                .expect("exists")
                .lifecycle,
            StoredLifecycle::NativeDraft,
            "the rejected command cannot leave a pause that resurfaces later"
        );

        nexus
            .command(&Command::PublishDraft { collection })
            .expect("confirms");
        tokio::time::timeout(Duration::from_secs(2), async {
            while nexus.state().collections[0].status != Status::Seeding {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("publishes and exposes its live seed state once confirmed");
        assert!(
            nexus
                .share_uri(collection)
                .expect("the collection still exists")
                .is_none(),
            "an injected substrate is not proof that the active librqbit session is seeding"
        );

        // Share begins the torrent rather than leaving a collection whose QR
        // claims it is shared while the engine has been told to remain idle.
        assert_eq!(
            nexus.state().collections[0].status,
            Status::Seeding,
            "the owner already has the source and is preparing its seed"
        );

        // Confirming twice is a second tap on a button, not an error.
        nexus
            .command(&Command::PublishDraft { collection })
            .expect("stays accepting");

        // Simulate a crash after the publication revision was durable but
        // before the collection's substrate handle was recorded. A reopened
        // collection must remain recoverable rather than staying Preparing
        // forever because it is no longer a draft.
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        nexus
            .store
            .put_collection(
                &key,
                &StoredCollection {
                    substrate_handle: None,
                    ..stored
                },
            )
            .expect("clears the simulated incomplete handle");
        nexus
            .command(&Command::PublishDraft { collection })
            .expect("retries an incomplete publication");
        tokio::time::timeout(Duration::from_secs(2), async {
            while substrate.published.lock().unwrap().len() < 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("republication completes");
        nexus.close().await;
    }

    /// A metadata source whose sender cannot be reached remains durable work,
    /// but the receiver must say why rather than appearing to resolve forever.
    /// This also exercises the worker's retry scheduling path: a later wake
    /// must respect its deadline, while closing the app must still stop it.
    #[tokio::test]
    async fn an_unreachable_metadata_sender_is_visible_and_the_worker_stops() {
        let scratch = Scratch::new("metadata-sender-unreachable");
        let nexus = open(&scratch);

        let accepted = nexus
            .command(&Command::ImportTorrent {
                source: "magnet:?xt=urn:btih:abc123".to_owned(),
            })
            .expect("records the source");
        let collection = accepted.collection.expect("names its collection");

        tokio::time::timeout(Duration::from_secs(2), async {
            while nexus
                .state()
                .collections
                .iter()
                .find(|item| item.id == collection)
                .is_some_and(|item| item.status != Status::WaitingForSender)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the failed resolution reaches the projection");
        assert_eq!(
            nexus
                .state()
                .collections
                .iter()
                .find(|item| item.id == collection)
                .expect("collection remains durable")
                .status,
            Status::WaitingForSender
        );

        tokio::time::timeout(Duration::from_secs(2), nexus.close())
            .await
            .expect("a waiting retry never delays shutdown");
    }

    /// A local-media publish that keeps failing (the real trigger: PhotoKit
    /// could not stream an iCloud-only asset) must not leave the collection
    /// silently stuck in `ResolvingMetadata` forever. It surfaces as
    /// `RetryingMetadata` and keeps retrying on a backoff, exactly like the
    /// receiver-side metadata resolver already does — regression coverage
    /// for the publisher previously only logging the failure and giving up.
    #[tokio::test]
    async fn a_stalled_publish_is_visible_and_keeps_retrying() {
        let scratch = Scratch::new("publish-stalled");
        // `Recorded::default()` has no configured publication, so every
        // `publish()` call fails — standing in for PhotoKit repeatedly
        // refusing to stream the selected asset.
        let nexus = open(&scratch);
        std::fs::write(scratch.0.join("clip.mp4"), b"data").expect("writes source file");

        let accepted = nexus
            .command(&Command::CreateCollection {
                name: "clip".to_owned(),
                files: vec![LocalFile {
                    name: "clip.mp4".to_owned(),
                    path: scratch.0.join("clip.mp4"),
                    bytes: 4,
                }],
            })
            .expect("accepts the local selection");
        let collection = accepted.collection.expect("names its collection");
        // A draft's files are never hashed until the person confirms sharing
        // — matches the real Flutter flow (createCollection, then
        // publishDraft) that actually wakes this worker.
        nexus
            .command(&Command::PublishDraft { collection })
            .expect("confirms the draft for sharing");

        tokio::time::timeout(Duration::from_secs(2), async {
            while nexus
                .state()
                .collections
                .iter()
                .find(|item| item.id == collection)
                .is_some_and(|item| item.status != Status::RetryingMetadata)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the failed publish reaches the projection instead of hanging silently");
        assert_eq!(
            nexus
                .state()
                .collections
                .iter()
                .find(|item| item.id == collection)
                .expect("collection remains durable")
                .status,
            Status::RetryingMetadata
        );

        tokio::time::timeout(Duration::from_secs(2), nexus.close())
            .await
            .expect("a waiting publish retry never delays shutdown");
    }

    /// Pausing has to reach the engine, not just the store.
    ///
    /// The reconciler is what applies stored intent, and it runs when woken.
    /// Without the wake the flag was recorded, the status said Paused, and
    /// the transfer carried on underneath it.
    #[tokio::test]
    async fn pausing_wakes_the_worker_that_tells_the_engine() {
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("pause-reaches");
        let substrate = Arc::new(crate::nexus::substrate::Recorded::inspecting(
            crate::nexus::substrate::Inspected {
                info_hash: "33".repeat(20),
                name: "Episode".to_owned(),
                files: vec![crate::nexus::torrent::TorrentMetadataFile {
                    label: "episode.mkv".to_owned(),
                    bytes: 10,
                }],
                descriptor: b"descriptor".to_vec(),
            },
        ));
        let nexus = open_with_substrate(&scratch, substrate.clone());

        let accepted = nexus
            .command(&Command::ImportTorrent {
                source: "magnet:?xt=urn:btih:abc123".to_owned(),
            })
            .expect("records the source");
        let collection = accepted.collection.expect("names its collection");
        let mut watching = nexus.watch_detail(Some(collection));
        settle(&nexus, &mut watching, |detail| {
            detail.is_some_and(|detail| !detail.entries.is_empty())
        })
        .await;
        nexus
            .command(&Command::DownloadSelection {
                collection,
                entries: vec![Handle(1)],
            })
            .expect("chooses the file");
        // Nothing can be paused before something is carrying it, so the
        // acquire has to land first — otherwise this would be testing the
        // race rather than the wake.
        tokio::time::timeout(Duration::from_secs(2), async {
            while substrate.selections.lock().unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the download starts");

        nexus
            .command(&Command::SetPaused {
                collection,
                paused: true,
            })
            .expect("pauses");

        tokio::time::timeout(Duration::from_secs(2), async {
            while !substrate
                .paused
                .lock()
                .unwrap()
                .iter()
                .any(|(_, paused)| *paused)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the engine is told to stop");
        nexus.close().await;
    }

    /// Pausing is a person's decision, so it has to outlast the process that
    /// took it. A pause a crash undoes would have this device quietly resume a
    /// transfer somebody stopped.
    #[tokio::test]
    async fn pausing_a_collection_is_reported_at_once_and_survives_a_restart() {
        let scratch = Scratch::new("pause");
        let nexus = open(&scratch);
        nexus
            .command(&Command::CreateCollection {
                name: "Iceland".to_owned(),
                files: Vec::new(),
            })
            .expect("creates locally");
        let collection = nexus.state().collections[0].id;
        // Shared first: pausing something never offered to anyone would be
        // stopping a transfer that was never going to start.
        nexus
            .command(&Command::PublishDraft { collection })
            .expect("shares it");

        nexus
            .command(&Command::SetPaused {
                collection,
                paused: true,
            })
            .expect("pauses");
        assert_eq!(nexus.state().collections[0].status, Status::Paused);
        nexus.close().await;

        let nexus = open(&scratch);
        assert_eq!(
            nexus.state().collections[0].status,
            Status::Paused,
            "a restart does not resume what a person stopped"
        );

        // And resuming hands it back to whatever the numbers say, rather than
        // to a second flag that could disagree with them.
        let collection = nexus.state().collections[0].id;
        nexus
            .command(&Command::SetPaused {
                collection,
                paused: false,
            })
            .expect("resumes");
        assert_eq!(nexus.state().collections[0].status, Status::Available);
        nexus.close().await;
    }

    /// Releasing a torrent must not forget where the person's media lives.
    ///
    /// The poller releases anything no collection currently claims, and a
    /// collection can be unclaimed for a moment without anyone having asked
    /// for it — a publish that has not recorded its handle yet, a rehydration
    /// that ran before the projection caught up. Purging the source references
    /// there destroyed the only record of where an owner's originals are, so
    /// the next launch could not restore them and fell back to reporting the
    /// download folder. Only deleting the collection forgets them.
    #[tokio::test]
    async fn releasing_a_torrent_keeps_the_source_references_and_deleting_drops_them() {
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("release-keeps-sources");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let info_hash = "55".repeat(20);
        crate::nexus::linked_source_store::upsert(
            crate::nexus::linked_source_store::LinkedSourceRecord {
                info_hash: info_hash.clone(),
                torrent_bytes: b"descriptor".to_vec(),
                sources: vec![crate::nexus::torrent::SourceFile {
                    name: "episode.mp4".to_owned(),
                    path: source.to_string_lossy().into_owned(),
                    length_bytes: Some(7),
                }],
                allow_missing_files: false,
            },
        )
        .expect("records where the original lives");

        let nexus = open(&scratch);
        let collection = nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source,
                    bytes: 7,
                }],
            })
            .expect("creates the collection")
            .collection
            .expect("names it");
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads")
            .expect("exists");
        nexus
            .store
            .put_collection(
                &key,
                &StoredCollection {
                    lifecycle: StoredLifecycle::NativePublished {
                        activity: StoredActivity::Running,
                    },
                    substrate_handle: Some(info_hash.clone()),
                    ..stored
                },
            )
            .expect("records the published handle");

        // Releasing is a session-level act, not a decision about the person's
        // media. The references have to survive it.
        crate::nexus::torrent::forget_torrent(&info_hash)
            .await
            .expect("releases without a live session");
        assert!(
            crate::nexus::linked_source_store::sources_for(&info_hash)
                .expect("reads")
                .is_some(),
            "a released torrent still knows where its originals are"
        );

        // Deleting the collection is the one act that forgets them.
        nexus
            .command(&Command::DeleteCollection {
                collection,
                delete_files: false,
            })
            .expect("deletes");
        assert!(
            crate::nexus::linked_source_store::sources_for(&info_hash)
                .expect("reads")
                .is_none(),
            "deleting the collection forgets its source references"
        );
        nexus.close().await;
    }

    /// Reclaiming disk space is not leaving the collection. Conflating the two
    /// would lose a membership that cannot be recovered locally.
    #[tokio::test]
    async fn deleting_the_files_keeps_the_collection() {
        let scratch = Scratch::new("delete-files");
        let nexus = open(&scratch);
        let (collection, media) = with_media(&nexus, &scratch, 4);

        nexus
            .command(&Command::DeleteFiles { collection })
            .expect("deletes the files");

        assert!(!media.exists(), "the downloaded bytes are gone");
        let state = nexus.state();
        assert_eq!(state.collections.len(), 1, "the collection is not");
        assert_eq!(state.collections[0].on_disk_bytes, 0);

        // Asking twice is not an error: the outcome asked for is that the
        // files are gone, and they are.
        nexus
            .command(&Command::DeleteFiles { collection })
            .expect("is content that they are already gone");
        nexus.close().await;
    }

    /// The flag was parsed and ignored before this: deleting a collection left
    /// its downloads behind with nothing left that knew where they were.
    #[tokio::test]
    async fn deleting_a_collection_with_its_files_removes_both() {
        let scratch = Scratch::new("delete-both");
        let nexus = open(&scratch);
        let (collection, media) = with_media(&nexus, &scratch, 4);

        nexus
            .command(&Command::DeleteCollection {
                collection,
                delete_files: true,
            })
            .expect("deletes");

        assert!(!media.exists(), "the downloaded bytes went with it");
        assert!(nexus.state().collections.is_empty());
        nexus.close().await;
    }

    /// The same command with the flag cleared keeps the bytes, which is what
    /// makes the flag worth carrying.
    #[tokio::test]
    async fn deleting_a_collection_without_its_files_leaves_them() {
        let scratch = Scratch::new("delete-record-only");
        let nexus = open(&scratch);
        let (collection, media) = with_media(&nexus, &scratch, 4);

        nexus
            .command(&Command::DeleteCollection {
                collection,
                delete_files: false,
            })
            .expect("deletes");

        assert!(media.exists(), "the files are the person's to keep");
        assert!(nexus.state().collections.is_empty());
        nexus.close().await;
    }

    fn stored_sample(done: u64, total: u64) -> crate::nexus::store::records::StoredSample {
        crate::nexus::store::records::StoredSample {
            done,
            total,
            down_bytes_per_second: 1,
            up_bytes_per_second: 2,
            peers: 3,
        }
    }

    /// Thousandths, because that is the resolution a progress bar has pixels
    /// for, and an integer needs no encoding of its own to cross the bridge.
    #[test]
    fn progress_crosses_as_thousandths_and_never_exceeds_them() {
        assert_eq!(progress_permille(&stored_sample(0, 100)), 0);
        assert_eq!(progress_permille(&stored_sample(50, 100)), 500);
        assert_eq!(progress_permille(&stored_sample(100, 100)), 1000);
        // A total of zero is metadata that has not arrived, not a finished
        // transfer, and dividing by it would be worse than saying nothing.
        assert_eq!(progress_permille(&stored_sample(5, 0)), 0);
        assert_eq!(
            progress_permille(&stored_sample(u64::MAX, 1)),
            1000,
            "clamped rather than wrapped"
        );
    }

    /// The substrate speaks in byte ranges per file and a person sees one bar
    /// for the whole collection, so the runs are folded onto a fixed number of
    /// bars rather than sent as they arrive.
    #[test]
    fn verified_runs_become_set_bits_and_everything_else_stays_clear() {
        let mut info = crate::nexus::torrent::TorrentInfo {
            id: 1,
            info_hash: "a1".to_owned(),
            name: "Iceland".to_owned(),
            state: "live".into(),
            progress_bytes: 50,
            source_check_bytes: None,
            fetched_bytes: 50,
            total_bytes: 100,
            uploaded_bytes: 0,
            finished: false,
            error: None,
            files: vec![crate::nexus::torrent::TorrentFile {
                name: "one.jpg".to_owned(),
                absolute_path: "/tmp/one.jpg".to_owned(),
                length_bytes: 100,
                downloaded_bytes: 50,
                piece_runs: vec![crate::nexus::torrent::PieceRun {
                    offset_bytes: 0,
                    length_bytes: 50,
                    verified: true,
                    peers: Vec::new(),
                }],
            }],
            live_peers: 0,
            live_peer_addrs: Vec::new(),
        };

        let bits = pieces_of(&info);
        assert_eq!(bits.len(), 64, "512 bars, packed");
        let set = bits.iter().map(|byte| byte.count_ones()).sum::<u32>();
        assert_eq!(set, 256, "the verified half, and only it");

        // A collection whose size is not known yet has no bars to draw.
        info.total_bytes = 0;
        assert!(pieces_of(&info).is_empty());

        // An unverified run is not a filled bar: having asked for bytes is not
        // the same as holding them.
        info.total_bytes = 100;
        info.files[0].piece_runs[0].verified = false;
        assert!(pieces_of(&info).iter().all(|byte| *byte == 0));
    }

    /// The history reaches the interface as fixed-width rows, so the far side
    /// reads it with an offset rather than a parser.
    #[tokio::test]
    async fn the_transfer_history_crosses_as_fixed_width_rows() {
        let scratch = Scratch::new("history");
        let nexus = open(&scratch);
        nexus
            .command(&Command::CreateCollection {
                name: "Iceland".to_owned(),
                files: Vec::new(),
            })
            .expect("creates locally");
        let collection = nexus.state().collections[0].id;
        let key = nexus.collection_key(collection).expect("known");

        assert!(
            nexus.history_after(collection, 0).is_none(),
            "nothing recorded yet is an empty chart, not a missing one"
        );

        for (at, done) in [(10_u64, 25_u64), (20, 50)] {
            nexus
                .store
                .put_sample(&key, at, &stored_sample(done, 100))
                .expect("records");
        }

        let (newest, packed) = nexus.history_after(collection, 0).expect("two readings");
        assert_eq!(newest, 20, "the moment a subscriber has now reached");
        assert_eq!(packed.len(), 2 * SAMPLE_ROW_BYTES);
        // The newest row last, and carrying what it was told.
        let last = &packed[SAMPLE_ROW_BYTES..];
        assert_eq!(u64::from_be_bytes(last[0..8].try_into().unwrap()), 20);
        assert_eq!(u32::from_be_bytes(last[8..12].try_into().unwrap()), 1);
        assert_eq!(u32::from_be_bytes(last[12..16].try_into().unwrap()), 2);
        assert_eq!(u16::from_be_bytes(last[16..18].try_into().unwrap()), 500);

        // Asked again from where it got to, there is nothing to say — which
        // is the whole point: an append costs an append, not a resend.
        assert!(nexus.history_after(collection, newest).is_none());
        nexus.close().await;
    }

    /// The promise a restart depends on.
    #[tokio::test]
    async fn watching_yields_a_complete_snapshot_before_anything_happens() {
        let scratch = Scratch::new("snapshot");
        let nexus = open(&scratch);

        let watcher = nexus.watch();
        let first = watcher.borrow().clone();

        assert_eq!(first.device.name, "Ada's laptop");
        assert!(first.collections.is_empty());
        assert_eq!(first.connectivity, Connectivity::LocalOnly);
        nexus.close().await;
    }

    /// A subscriber that arrives after everything has happened still sees it
    /// all, which is what makes a hot reload survivable.
    #[tokio::test]
    async fn a_late_subscriber_sees_the_current_state_not_a_backlog() {
        let scratch = Scratch::new("late");
        let nexus = open(&scratch);

        nexus.publish(&state(vec![collection("Iceland")]), None, Duration::ZERO);
        nexus.publish(
            &state(vec![collection("Iceland, 2019")]),
            None,
            Duration::from_secs(1),
        );

        let late = nexus.watch();
        let seen = late.borrow().clone();

        assert_eq!(seen.collections.len(), 1);
        assert_eq!(
            seen.collections[0].name, "Iceland, 2019",
            "the newest, not the first"
        );
        nexus.close().await;
    }

    #[tokio::test]
    async fn an_unchanged_publication_does_not_wake_a_subscriber() {
        let scratch = Scratch::new("quiet");
        let nexus = open(&scratch);
        let mut watcher = nexus.watch();
        watcher.mark_unchanged();

        let same = state(vec![collection("Iceland")]);
        nexus.publish(&same, None, Duration::ZERO);
        assert!(
            watcher.has_changed().expect("alive"),
            "the first one is new"
        );
        watcher.mark_unchanged();

        for tick in 1..5 {
            nexus.publish(&same, None, Duration::from_secs(tick));
        }
        assert!(
            !watcher.has_changed().expect("alive"),
            "nothing changed, so nobody was woken"
        );
        nexus.close().await;
    }

    #[tokio::test]
    async fn a_command_is_accepted_and_named_so_it_can_be_matched_later() {
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("command");
        let nexus = open(&scratch);

        let first = nexus
            .command(&Command::CreateCollection {
                name: "Iceland".to_owned(),
                files: Vec::new(),
            })
            .expect("accepted");
        let second = nexus
            .command(&Command::RetryTransfer {
                collection: Handle(1),
            })
            .expect("accepted");

        assert!(first.queued, "it will publish when there is a network");
        assert_eq!(first.collection, Some(Handle(1)));
        assert_eq!(second.collection, None);
        assert_ne!(first.id, second.id, "each is named separately");
        nexus.close().await;
    }

    /// Local collection changes cross the one durable boundary before they
    /// appear in the state stream. A restarted core therefore reconstructs
    /// the same collection rather than relying on a bridge-side cache.
    #[tokio::test]
    async fn collection_edits_are_visible_immediately_and_survive_a_restart() {
        let scratch = Scratch::new("durable-collections");
        let nexus = open(&scratch);

        nexus
            .command(&Command::CreateCollection {
                name: "Iceland".to_owned(),
                files: Vec::new(),
            })
            .expect("creates locally");
        let handle = nexus.state().collections[0].id;
        assert_eq!(nexus.state().collections[0].name, "Iceland");

        nexus
            .command(&Command::RenameCollection {
                collection: handle,
                name: "Iceland, 2019".to_owned(),
            })
            .expect("renames locally");
        nexus.close().await;

        let reopened = open(&scratch);
        assert_eq!(reopened.state().collections.len(), 1);
        assert_eq!(reopened.state().collections[0].name, "Iceland, 2019");
        assert_eq!(
            reopened.state().device.fingerprint,
            "ada-fingerprint",
            "the state has a trustworthy local fingerprint from the start"
        );

        reopened
            .command(&Command::DeleteCollection {
                collection: reopened.state().collections[0].id,
                delete_files: false,
            })
            .expect("removes locally");
        reopened.close().await;

        let empty = open(&scratch);
        assert!(empty.state().collections.is_empty());
        empty.close().await;
    }

    #[tokio::test]
    async fn the_first_snapshot_restores_every_signed_member_after_restart() {
        use crate::nexus::collections::publish::tests::Person;

        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("durable-members");
        let nexus = open(&scratch);
        let collection = nexus
            .command(&Command::CreateCollection {
                name: "Shared archive".to_owned(),
                files: Vec::new(),
            })
            .expect("creates the durable collection")
            .collection
            .expect("names the collection");
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("stored collection");
        nexus.close().await;

        // The owner of a persisted revision must be *this device's own*
        // identity for hydration to trust it (ADR-0013) — a hydration-time
        // authority check equivalent to admission's for the only case the
        // shipped product persists today: collections this device owns.
        let local_identity =
            crate::nexus::device::current_signing_identity().expect("loads local identity");
        let owner = Person::from_signing_key(ed25519_dalek::SigningKey::from_bytes(
            &local_identity.to_bytes(),
        ));
        let known = Person::new(12);
        let unknown = Person::new(13);
        let collection_id =
            <[u8; portalis_nexus_protocol::SHARE_ID_BYTES]>::try_from(key.as_slice())
                .expect("collection key shape");
        let signed = crate::nexus::collections::model::Collection {
            id: crate::nexus::collections::model::CollectionId(collection_id),
            name: stored.name.clone(),
            role: stored.role,
            content_key: stored.content_key,
            revision: None,
            manifest: portalis_nexus_protocol::Manifest::default(),
        };
        let (_, publication) = crate::nexus::collections::publish::publish(
            &signed,
            &owner,
            &[owner.recipient(), known.recipient(), unknown.recipient()],
            &[],
            1_700_000_000_000_000_000,
        )
        .expect("publishes a signed membership");
        let known_root = known.signing.verifying_key().to_bytes();
        let unknown_root = unknown.signing.verifying_key().to_bytes();
        let store = Store::open(scratch.0.join("portalis.redb")).expect("reopens store");
        store
            .put_revision(
                &key,
                publication.revision.number,
                &publication.revision.encode(),
            )
            .expect("persists signed revision");
        store
            .put_contact(&StoredContact {
                handle: "known#MEMBER".to_owned(),
                fingerprint_verified: true,
                root_key: known_root,
            })
            .expect("persists known contact");
        drop(store);

        let reopened = open(&scratch);
        let first = reopened.watch().borrow().clone();
        assert_eq!(first.collections[0].members.len(), 3);
        let known_member = first.collections[0]
            .members
            .iter()
            .find(|member| member.root_key == known_root)
            .expect("known signed member survives");
        let unknown_member = first.collections[0]
            .members
            .iter()
            .find(|member| member.root_key == unknown_root)
            .expect("unknown signed member is explicit");
        assert!(known_member.contact.is_some());
        assert!(unknown_member.contact.is_none());
        assert_eq!(first.contacts.len(), 1);
        assert_eq!(
            first.contacts[0].id,
            known_member.contact.expect("known handle")
        );
        reopened.close().await;

        let store = Store::open(scratch.0.join("portalis.redb")).expect("reopens store");
        let held = store
            .collection(&key)
            .expect("reads collection")
            .expect("stored collection");
        store
            .put_collection(
                &key,
                &StoredCollection {
                    role: StoredRole::Member,
                    ..held
                },
            )
            .expect("records member role");
        drop(store);
        let member_reopened = open(&scratch);
        assert_eq!(member_reopened.state().collections[0].role, Role::Member);
        assert_eq!(member_reopened.state().collections[0].members.len(), 3);
        member_reopened.close().await;
    }

    #[test]
    fn a_newer_signed_revision_replaces_the_hydrated_member_set() {
        use crate::nexus::collections::publish::tests::{Person, descriptors, owned};

        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("newer-members");
        let store = Store::open(scratch.0.join("portalis.redb")).expect("opens store");
        let local_identity =
            crate::nexus::device::current_signing_identity().expect("loads local identity");
        let owner = Person::from_signing_key(ed25519_dalek::SigningKey::from_bytes(
            &local_identity.to_bytes(),
        ));
        let removed = Person::new(22);
        let added = Person::new(23);
        let initial = owned(&owner);
        let key = initial.id.0;
        let (published, first) = crate::nexus::collections::publish::publish(
            &initial,
            &owner,
            &[owner.recipient(), removed.recipient()],
            &descriptors(),
            1,
        )
        .expect("first signed revision");
        let (_, second) = crate::nexus::collections::publish::publish(
            &published,
            &owner,
            &[owner.recipient(), added.recipient()],
            &descriptors(),
            2,
        )
        .expect("second signed revision");
        store
            .put_revision(&key, first.revision.number, &first.revision.encode())
            .expect("stores first revision");
        store
            .put_revision(&key, second.revision.number, &second.revision.encode())
            .expect("stores second revision");

        let hydrated = hydrate_membership(&store, &key, local_identity.public_key())
            .expect("hydrates production membership");
        assert_eq!(hydrated.number, 2);
        assert!(
            hydrated
                .roots
                .contains(&added.signing.verifying_key().to_bytes())
        );
        assert!(
            !hydrated
                .roots
                .contains(&removed.signing.verifying_key().to_bytes())
        );
        assert!(hydrated.failure.is_none());
    }

    #[test]
    fn persisted_revision_anomalies_are_explicit_hydration_failures() {
        use crate::nexus::collections::publish::tests::{Person, descriptors, owned};

        let _state = crate::nexus::paths::redirect_to_temp();
        let local_identity =
            crate::nexus::device::current_signing_identity().expect("loads local identity");
        let owner = Person::from_signing_key(ed25519_dalek::SigningKey::from_bytes(
            &local_identity.to_bytes(),
        ));
        let initial = owned(&owner);
        let key = initial.id.0;
        let (published, first) = crate::nexus::collections::publish::publish(
            &initial,
            &owner,
            &[owner.recipient()],
            &descriptors(),
            1,
        )
        .expect("first signed revision");
        let (_, second) = crate::nexus::collections::publish::publish(
            &published,
            &owner,
            &[owner.recipient()],
            &descriptors(),
            2,
        )
        .expect("second signed revision");

        let check = |name: &str, rows: &[(u64, Vec<u8>)], expected: Status| {
            let scratch = Scratch::new(name);
            let store = Store::open(scratch.0.join("portalis.redb")).expect("opens store");
            for (number, bytes) in rows {
                store
                    .put_revision(&key, *number, bytes)
                    .expect("stores anomaly fixture");
            }
            assert_eq!(
                hydrate_membership(&store, &key, local_identity.public_key())
                    .expect("hydrates explicit failure")
                    .failure,
                Some(expected)
            );
        };

        let mut forged = first.revision.encode();
        *forged.last_mut().expect("signature byte") ^= 1;
        check(
            "forged-revision",
            &[(1, forged)],
            Status::CannotVerify(crate::nexus::core::events::VerifyFailure::Signature),
        );
        check(
            "rollback-revision",
            &[(2, first.revision.encode())],
            Status::CannotVerify(crate::nexus::core::events::VerifyFailure::Rollback),
        );
        let mut future = first.revision.encode();
        future[0] ^= 1;
        check("future-revision", &[(1, future)], Status::NeedsNewerVersion);

        let mut broken = second.revision.clone();
        broken.previous_hash = [0x77; portalis_nexus_protocol::REVISION_HASH_BYTES];
        broken.signature =
            crate::nexus::collections::publish::Author::sign(&owner, &broken.signing_payload());
        check(
            "broken-revision-chain",
            &[(1, first.revision.encode()), (2, broken.encode())],
            Status::CannotVerify(crate::nexus::core::events::VerifyFailure::BrokenChain),
        );

        let mut conflicting = second.revision;
        conflicting.owner_root_key = [0x66; portalis_nexus_protocol::DEVICE_KEY_BYTES];
        conflicting.signature = crate::nexus::collections::publish::Author::sign(
            &owner,
            &conflicting.signing_payload(),
        );
        // Hydration's authority check (this device only trusts revisions it
        // owns) rejects the foreign `owner_root_key` before chain-position
        // logic ever runs, so this now surfaces as an explicit signature/
        // authority failure rather than reaching `ConflictingHistory` —
        // fail-closed earlier is the correct outcome for an owner swap an
        // attacker could stage exactly like this.
        check(
            "conflicting-owner-history",
            &[(1, first.revision.encode()), (2, conflicting.encode())],
            Status::CannotVerify(crate::nexus::core::events::VerifyFailure::Signature),
        );
    }

    #[tokio::test]
    async fn corrupt_revision_projects_an_explicit_verification_failure() {
        use crate::nexus::collections::publish::tests::Person;

        let scratch = Scratch::new("corrupt-membership");
        let nexus = open(&scratch);
        let collection = nexus
            .command(&Command::CreateCollection {
                name: "Damaged archive".to_owned(),
                files: Vec::new(),
            })
            .expect("creates collection")
            .collection
            .expect("collection handle");
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("stored collection");
        nexus.close().await;
        let owner = Person::new(31);
        let collection_id =
            <[u8; portalis_nexus_protocol::SHARE_ID_BYTES]>::try_from(key.as_slice())
                .expect("collection key shape");
        let signed = crate::nexus::collections::model::Collection {
            id: crate::nexus::collections::model::CollectionId(collection_id),
            name: stored.name,
            role: stored.role,
            content_key: stored.content_key,
            revision: None,
            manifest: portalis_nexus_protocol::Manifest::default(),
        };
        let (_, publication) = crate::nexus::collections::publish::publish(
            &signed,
            &owner,
            &[owner.recipient()],
            &[],
            1,
        )
        .expect("valid signed revision");
        let mut damaged = publication.revision.encode();
        damaged.pop();
        let store = Store::open(scratch.0.join("portalis.redb")).expect("reopens store");
        store
            .put_revision(&key, 1, &damaged)
            .expect("simulates at-rest corruption");
        drop(store);

        let reopened = open(&scratch);
        assert_eq!(
            reopened.state().collections[0].status,
            Status::CannotVerify(crate::nexus::core::events::VerifyFailure::ContentMismatch)
        );
        assert!(reopened.state().collections[0].members.is_empty());
        reopened.close().await;
    }

    /// Creating a share persists references to the originals, not their
    /// contents, and reconstructs the same useful projection after restart.
    #[tokio::test]
    async fn selected_files_are_durable_zero_copy_collection_sources() {
        let scratch = Scratch::new("durable-sources");
        let first_path = scratch.0.join("episode-one.mp4");
        let second_path = scratch.0.join("episode-two.mp4");
        std::fs::write(&first_path, b"first").expect("writes first source");
        std::fs::write(&second_path, b"second episode").expect("writes second source");
        let nexus = open(&scratch);

        let accepted = nexus
            .command(&Command::CreateCollection {
                name: "Episode archive".to_owned(),
                files: vec![
                    LocalFile {
                        name: "Episode 1.mp4".to_owned(),
                        path: first_path.clone(),
                        bytes: 5,
                    },
                    LocalFile {
                        name: "Episode 2.mp4".to_owned(),
                        path: second_path.clone(),
                        bytes: 14,
                    },
                ],
            })
            .expect("accepts original files");
        let handle = accepted.collection.expect("names the collection");
        let state = nexus.state();
        assert_eq!(state.collections[0].nature, Nature::Native);
        // Chosen, not yet shared: the sources are durable before anything is
        // hashed, which is what makes abandoning one free.
        assert_eq!(state.collections[0].status, Status::Draft);
        assert_eq!(state.collections[0].entries, 2);
        assert_eq!(state.collections[0].total_bytes, 19);
        let detail = nexus
            .watch_detail(Some(handle))
            .borrow()
            .clone()
            .expect("projects local files");
        assert_eq!(detail.entries[0].label, "Episode 1.mp4");
        assert!(detail.entries.iter().all(|entry| entry.available));

        let key = nexus.collection_key(handle).expect("has a durable key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists")
            .sources;
        assert_eq!(stored[0].path, first_path.to_string_lossy());
        assert_eq!(
            std::fs::read(&first_path).expect("original remains"),
            b"first"
        );
        nexus.close().await;

        let reopened = open(&scratch);
        assert_eq!(reopened.state().collections[0].entries, 2);
        assert_eq!(reopened.state().collections[0].total_bytes, 19);
        let reopened_detail = reopened
            .watch_detail(Some(reopened.state().collections[0].id))
            .borrow()
            .clone()
            .expect("restores detail");
        assert_eq!(reopened_detail.entries[1].label, "Episode 2.mp4");
        assert_eq!(
            std::fs::read(&second_path).expect("original remains"),
            b"second episode"
        );
        reopened.close().await;
    }

    #[tokio::test]
    async fn a_reopened_published_owner_collection_is_seeding_while_its_zero_copy_seed_rehydrates()
    {
        let scratch = Scratch::new("reopened-owner-seed-status");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let nexus = open(&scratch);

        let collection = nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source,
                    bytes: 7,
                }],
            })
            .expect("creates the local collection")
            .collection
            .expect("names the collection");
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        nexus
            .store
            .put_collection(
                &key,
                &StoredCollection {
                    lifecycle: StoredLifecycle::NativePublished {
                        activity: StoredActivity::Running,
                    },
                    substrate_handle: Some("44".repeat(20)),
                    ..stored
                },
            )
            .expect("records a published owner collection");
        nexus.close().await;

        let reopened = open(&scratch);
        assert_eq!(
            reopened.state().collections[0].status,
            Status::Seeding,
            "the owner already has the referenced source; startup rehydration must present its seed rather than a receiver download"
        );
        reopened.close().await;
    }

    /// ADR-0017 restart matrix: an owner's original zero-copy source can be
    /// gone by the time this device restarts (moved, renamed, the removable
    /// volume unmounted). Startup hydration must not panic or silently
    /// invent an available byte count for a file it cannot see — the detail
    /// projection reports it as unavailable, exactly like the live substrate
    /// path in `build` already does.
    #[tokio::test]
    async fn a_reopened_owner_collection_with_a_missing_zero_copy_source_hydrates_as_unavailable() {
        let scratch = Scratch::new("reopened-owner-missing-source");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let nexus = open(&scratch);

        let collection = nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source.clone(),
                    bytes: 7,
                }],
            })
            .expect("creates the local collection")
            .collection
            .expect("names the collection");
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        nexus
            .store
            .put_collection(
                &key,
                &StoredCollection {
                    lifecycle: StoredLifecycle::NativePublished {
                        activity: StoredActivity::Running,
                    },
                    substrate_handle: Some("55".repeat(20)),
                    ..stored
                },
            )
            .expect("records a published owner collection");
        nexus.close().await;

        // The original file is gone by the time this device restarts.
        std::fs::remove_file(&source).expect("removes the original source");

        let reopened = open(&scratch);
        // Hydration must not panic reading a source it cannot stat, and the
        // durable entry/byte facts it already recorded remain the answer —
        // they are what this device promised to seed, not a live disk probe.
        assert_eq!(reopened.state().collections[0].entries, 1);
        assert_eq!(reopened.state().collections[0].total_bytes, 7);
        let handle = reopened.state().collections[0].id;
        let detail = reopened
            .watch_detail(Some(handle))
            .borrow()
            .clone()
            .expect("projects local files even when one is missing");
        assert!(
            !detail.entries[0].available,
            "a missing zero-copy source must not be reported as available"
        );
        reopened.close().await;
    }

    /// The user-facing bug this guards: a collection created from local
    /// media whose source file is later deleted, moved, or renamed must not
    /// stay silently unseedable forever. Once this device can no longer read
    /// its own copy, the collection converts into an ordinary receiver
    /// download of the exact content it had already published — the same
    /// path any other unfinished torrent import takes, not a new state.
    #[tokio::test]
    async fn a_deleted_local_source_converts_its_collection_into_a_normal_download() {
        let scratch = Scratch::new("source-sanity-deleted");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let nexus = open(&scratch);

        let collection = nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source.clone(),
                    bytes: 7,
                }],
            })
            .expect("creates the local collection")
            .collection
            .expect("names the collection");
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        // Simulate a collection that already finished publishing and is
        // actively seeding — the state a real one reaches once the
        // publisher's own worker (tested separately) succeeds.
        nexus
            .store
            .put_collection(
                &key,
                &StoredCollection {
                    lifecycle: StoredLifecycle::NativePublished {
                        activity: StoredActivity::Running,
                    },
                    substrate_handle: Some("66".repeat(20)),
                    ..stored
                },
            )
            .expect("records a published, seeding owner collection");

        std::fs::remove_file(&source).expect("the source disappears underneath the app");

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let stored = nexus
                    .store
                    .collection(&key)
                    .ok()
                    .flatten()
                    .expect("collection remains durable");
                if stored.sources.is_empty() && stored.substrate_handle.is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the sanity worker converts the collection instead of leaving it stuck");

        let converted = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        assert!(
            !matches!(converted.lifecycle, StoredLifecycle::NativeDraft),
            "a converted collection is being downloaded back, never a private draft nobody has seen"
        );
        assert_eq!(
            nexus.store.torrent_import(&key).expect("reads import"),
            Some(format!("magnet:?xt=urn:btih:{}", "66".repeat(20))),
            "the exact content already published becomes what is downloaded back"
        );

        nexus.close().await;
    }

    /// Same conversion, but with the collection's own resolved file list
    /// already known (the durable record publishing itself wrote) — this is
    /// what most real owner collections look like, so the converted row
    /// goes straight to acquiring rather than re-resolving a magnet it
    /// already has every answer for.
    #[tokio::test]
    async fn a_deleted_local_source_with_a_known_file_list_skips_straight_to_acquiring() {
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("source-sanity-deleted-known-files");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let nexus = open(&scratch);

        let collection = nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source.clone(),
                    bytes: 7,
                }],
            })
            .expect("creates the local collection")
            .collection
            .expect("names the collection");
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        let handle = "88".repeat(20);
        nexus
            .store
            .put_collection(
                &key,
                &StoredCollection {
                    lifecycle: StoredLifecycle::NativePublished {
                        activity: StoredActivity::Running,
                    },
                    substrate_handle: Some(handle.clone()),
                    ..stored
                },
            )
            .expect("records a published, seeding owner collection");
        // What `publish_collection_sources` itself would have already
        // written: the exact file list and descriptor this device offered.
        crate::nexus::linked_source_store::upsert(
            crate::nexus::linked_source_store::LinkedSourceRecord {
                info_hash: handle.clone(),
                torrent_bytes: b"torrent descriptor".to_vec(),
                sources: vec![crate::nexus::torrent::SourceFile {
                    name: "episode.mp4".to_owned(),
                    path: source.to_string_lossy().into_owned(),
                    length_bytes: Some(7),
                }],
                allow_missing_files: false,
            },
        )
        .expect("records the published sources");

        std::fs::remove_file(&source).expect("the source disappears underneath the app");

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if !nexus
                    .store
                    .torrent_import_entries(&key)
                    .unwrap_or_default()
                    .is_empty()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the sanity worker records the already-known file list, not just the magnet");

        let entries = nexus
            .store
            .torrent_import_entries(&key)
            .expect("reads entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "episode.mp4");
        assert!(
            entries[0].selected,
            "the whole known collection is wanted back"
        );
        assert_eq!(
            nexus
                .store
                .torrent_import_descriptor(&key)
                .expect("reads descriptor"),
            Some(b"torrent descriptor".to_vec()),
            "the exact descriptor already published carries over rather than needing a fresh inspect"
        );

        nexus.close().await;
    }

    #[tokio::test]
    async fn a_paused_owner_collection_with_a_missing_source_is_left_alone() {
        let scratch = Scratch::new("source-sanity-paused");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let nexus = open(&scratch);

        let collection = nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source.clone(),
                    bytes: 7,
                }],
            })
            .expect("creates the local collection")
            .collection
            .expect("names the collection");
        let key = nexus.collection_key(collection).expect("collection key");
        let stored = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        nexus
            .store
            .put_collection(
                &key,
                &StoredCollection {
                    lifecycle: StoredLifecycle::NativePublished {
                        activity: StoredActivity::Paused,
                    },
                    substrate_handle: Some("77".repeat(20)),
                    ..stored
                },
            )
            .expect("records a paused, seeding owner collection");

        std::fs::remove_file(&source).expect("the source disappears underneath the app");

        // A person's pause is a decision, not a stall to fix. Give the
        // sanity worker several ticks to (not) act, rather than asserting
        // on its very first pass — a false pass on tick one would still be
        // wrong the moment a later tick converted it regardless.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let untouched = nexus
            .store
            .collection(&key)
            .expect("reads collection")
            .expect("collection exists");
        assert!(
            !untouched.sources.is_empty(),
            "a paused collection's own decision must not be overridden by a missing source"
        );
        assert_eq!(untouched.substrate_handle, Some("77".repeat(20)));

        nexus.close().await;
    }

    #[tokio::test]
    async fn a_native_collection_publishes_through_the_injected_zero_copy_substrate() {
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("publish-sources");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let substrate = Arc::new(crate::nexus::substrate::Recorded::publishing(
            "11".repeat(20),
            b"torrent descriptor".to_vec(),
        ));
        let nexus = open_with_substrate(&scratch, substrate.clone());
        let mut states = nexus.watch();

        nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source,
                    bytes: 7,
                }],
            })
            .expect("accepts source");
        // Creating one no longer publishes it: a draft is chosen, not shared,
        // and nothing hashes until the person says so.
        assert_eq!(
            states.borrow().collections[0].status,
            Status::Draft,
            "a new collection waits to be confirmed"
        );
        assert_eq!(states.borrow().collections[0].entries, 1);
        assert_eq!(states.borrow().collections[0].total_bytes, 7);
        // Read out before the call, not inside its arguments. A `borrow()`
        // temporary lives to the end of the enclosing statement, so passing
        // one as an argument holds the watch's read lock for the whole
        // command — and the command publishes, which needs the write lock.
        let collection = states.borrow().collections[0].id;
        nexus
            .command(&Command::PublishDraft { collection })
            .expect("confirms the draft");
        let settled = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                // Copied out before awaiting. `borrow()` holds the watch's
                // read lock, and a lock held across an await blocks the
                // publisher's own `send_modify` — a deadlock no timeout can
                // cancel, because the block is on a std lock rather than on
                // anything the runtime schedules. It turned a failing
                // assertion into a hung suite: this test holds the serial
                // state guard, so every other test waits behind it.
                let settled = states
                    .borrow()
                    .collections
                    .first()
                    .is_some_and(|collection| {
                        collection.status == Status::Seeding
                            && collection.revision == 1
                            && collection.entries == 1
                            && collection.total_bytes == 7
                    });
                if settled {
                    break;
                }
                states.changed().await.expect("runtime remains open");
            }
        })
        .await;
        assert!(
            settled.is_ok(),
            "publisher settles; final projection: {:?}",
            states.borrow().collections
        );

        assert_eq!(
            substrate
                .published
                .lock()
                .expect("publication log")
                .as_slice(),
            ["Episodes"]
        );
        let key = nexus
            .collection_key(nexus.state().collections[0].id)
            .expect("collection key");
        assert_eq!(
            nexus
                .store
                .current_revision(&key)
                .expect("reads revision")
                .expect("revision exists")
                .0,
            1
        );
        assert_eq!(
            nexus
                .store
                .entry(&[0x11; 20])
                .expect("reads descriptor")
                .expect("descriptor exists")
                .descriptor,
            b"torrent descriptor"
        );
        assert!(
            nexus.share_uri(collection).expect("share URI").is_none(),
            "a stored handle without a loaded torrent cannot be shared"
        );
        nexus.close().await;
    }

    /// The 500ms progress ticker must actually reach the projection while a
    /// publication is genuinely in flight — this is the ordinary case the
    /// ticker exists for, not the shutdown/completion edges the neighbouring
    /// tests cover. Held in "hashing" until this test explicitly releases
    /// it, a live collection must show `publish_progress` with that stage,
    /// and `isPublishing`-shaped code (Dart) must be able to tell it apart
    /// from every other resolving-metadata reason. The test runtime is
    /// paused and advanced by hand so the ticker's interval fires
    /// deterministically — no wall-clock sleep, no flakiness.
    #[tokio::test(start_paused = true)]
    async fn publishing_progress_reaches_the_projection_mid_publish() {
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("publish-progress-mid-flight");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let substrate = Arc::new(crate::nexus::substrate::Recorded::publishing_held(
            "11".repeat(20),
            b"torrent descriptor".to_vec(),
        ));
        let nexus = open_with_substrate(&scratch, substrate.clone());
        let mut states = nexus.watch();

        nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source,
                    bytes: 7,
                }],
            })
            .expect("accepts source");
        let collection = states.borrow().collections[0].id;
        nexus
            .command(&Command::PublishDraft { collection })
            .expect("confirms the draft");

        // Give the publish task a chance to reach "hashing" and register
        // its stage, then advance the paused clock past the ticker's
        // 500ms interval so its first tick actually fires.
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(600)).await;
        tokio::task::yield_now().await;

        assert_eq!(
            states.borrow().collections[0]
                .publish_progress
                .as_ref()
                .map(|progress| progress.stage.as_str()),
            Some("hashing"),
            "the ticker's tick reaches the projection while publishing runs; \
             final projection: {:?}",
            states.borrow().collections
        );

        substrate.release_publish();
        let settled = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let settled = states
                    .borrow()
                    .collections
                    .first()
                    .is_some_and(|collection| collection.status == Status::Seeding);
                if settled {
                    break;
                }
                states.changed().await.expect("runtime remains open");
            }
        })
        .await;
        assert!(settled.is_ok(), "publisher settles");

        nexus.close().await;
    }

    /// The progress ticker must die with the publication it reports on.
    ///
    /// It ran on a 500ms interval and only ever checked `is_cancelled()`,
    /// which is set on shutdown alone — so on the ordinary success path it
    /// outlived the publish and kept writing. The success path clears
    /// `publish_progress`, and then the surviving ticker put it straight
    /// back with the final "seeding" snapshot, forever, once per tick. A
    /// collection that had finished hashing showed a hashing bar for the
    /// rest of the process's life, and every publication leaked another
    /// ticker. Settling once is not enough to catch that: the bug only
    /// appears on the tick *after* completion.
    #[tokio::test]
    async fn publishing_progress_stops_when_the_publication_finishes() {
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("publish-progress-stops");
        let source = scratch.0.join("episode.mp4");
        std::fs::write(&source, b"episode").expect("writes source");
        let substrate = Arc::new(crate::nexus::substrate::Recorded::publishing(
            "11".repeat(20),
            b"torrent descriptor".to_vec(),
        ));
        let nexus = open_with_substrate(&scratch, substrate.clone());
        let mut states = nexus.watch();

        nexus
            .command(&Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "episode.mp4".to_owned(),
                    path: source,
                    bytes: 7,
                }],
            })
            .expect("accepts source");
        let collection = states.borrow().collections[0].id;
        nexus
            .command(&Command::PublishDraft { collection })
            .expect("confirms the draft");

        let settled = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let settled = states
                    .borrow()
                    .collections
                    .first()
                    .is_some_and(|collection| collection.status == Status::Seeding);
                if settled {
                    break;
                }
                states.changed().await.expect("runtime remains open");
            }
        })
        .await;
        assert!(settled.is_ok(), "publisher settles");
        assert_eq!(
            states.borrow().collections[0].publish_progress,
            None,
            "completing the publication clears the hashing progress"
        );

        // Past several ticker periods. A leaked ticker rewrites the cleared
        // field with its final snapshot within one 500ms interval.
        tokio::time::sleep(Duration::from_millis(1_600)).await;
        assert_eq!(
            states.borrow().collections[0].publish_progress,
            None,
            "no ticker survives the publication to resurrect a finished bar"
        );

        nexus.close().await;
    }

    #[tokio::test]
    async fn returning_to_the_foreground_reconnects_active_transfers() {
        // iOS/Android can fully suspend the process while it is
        // backgrounded, so librqbit's own reconnect logic — which only
        // runs while something is scheduling it — cannot be relied on to
        // have noticed a dead swarm on its own. `set_active(true)` (the
        // lifecycle bridge call both platforms' `AppLifecycleState.resumed`
        // handler makes) must actively kick every live transfer, not just
        // resume checkpointing activity.
        let _state = crate::nexus::paths::redirect_to_temp();
        let scratch = Scratch::new("resume-reconnects");
        let substrate = Arc::new(crate::nexus::substrate::Recorded::default());
        let mut nexus = open_with_substrate(&scratch, substrate.clone());

        nexus.set_active(false);
        assert_eq!(
            *substrate.reconnects.lock().unwrap(),
            0,
            "going to the background does not reconnect anything"
        );

        nexus.set_active(true);
        // `reconnect_active` is fired-and-forgotten from a spawned task, so
        // give the runtime one scheduling pass to run it rather than racing
        // on a bare assert.
        tokio::task::yield_now().await;
        let settled = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if *substrate.reconnects.lock().unwrap() >= 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(settled.is_ok(), "resuming to the foreground reconnects");

        // A second resume without an intervening background transition is a
        // no-op transition (`set_active` already ignores it) — the count
        // must not creep on every no-op call.
        nexus.set_active(true);
        tokio::task::yield_now().await;
        assert_eq!(
            *substrate.reconnects.lock().unwrap(),
            1,
            "resuming twice in a row without backgrounding in between reconnects once"
        );

        nexus.close().await;
    }

    /// Every refusal a command can earn without touching state.
    #[tokio::test]
    async fn a_malformed_command_is_refused_with_the_reason() {
        let scratch = Scratch::new("refusals");
        let nexus = open(&scratch);

        for (command, fragment) in [
            (
                Command::CreateCollection {
                    name: "   ".to_owned(),
                    files: Vec::new(),
                },
                "needs a name",
            ),
            (
                Command::RenameCollection {
                    collection: Handle(1),
                    name: String::new(),
                },
                "needs a name",
            ),
            (
                Command::AddMedia {
                    collection: Handle(1),
                    label: "one".to_owned(),
                    files: Vec::new(),
                },
                "no files",
            ),
            (
                Command::AddContact {
                    handle: "ada".to_owned(),
                },
                "ada#7Q2XZ",
            ),
        ] {
            let refused = nexus.command(&command).expect_err("must be refused");
            assert!(
                refused.to_string().contains(fragment),
                "{command:?} said {refused}"
            );
        }
        nexus.close().await;
    }

    /// The one kind that cannot wait: resolving a handle needs the directory
    /// now, so it is refused rather than queued into silence.
    #[tokio::test]
    async fn a_command_that_needs_a_connection_says_so_rather_than_queueing() {
        let scratch = Scratch::new("unavailable");
        let nexus = open(&scratch);

        assert_eq!(
            nexus.command(&Command::AddContact {
                handle: "mira#4KQ2P".to_owned()
            }),
            Err(CommandError::Unavailable)
        );
        nexus.close().await;
    }

    /// ADR-0011 decision #11: rename is canonicalized through Nexus so the
    /// live snapshot updates atomically with the persisted identity, rather
    /// than requiring a second frontend controller to separately reload.
    #[tokio::test]
    async fn renaming_through_nexus_updates_the_live_snapshot_immediately() {
        let scratch = Scratch::new("rename-live-snapshot");
        let mut nexus = open(&scratch);

        let before = nexus.state().device.name;
        assert_ne!(before, "Ada's New Name");

        nexus
            .rename_device("Ada's New Name".to_owned())
            .expect("renames");

        assert_eq!(nexus.state().device.name, "Ada's New Name");
        nexus.close().await;
    }

    #[tokio::test]
    async fn detail_costs_nothing_until_it_is_asked_for() {
        let scratch = Scratch::new("detail");
        let nexus = open(&scratch);
        let quiet = state(vec![collection("Iceland")]);
        let detail = Detail {
            id: Handle(1),
            entries: Vec::new(),
            pieces: vec![0xff; 8],
            peers: Vec::new(),
        };

        nexus.publish(&quiet, Some(&detail), Duration::ZERO);
        assert_eq!(
            *nexus.watch_detail(None).borrow(),
            None,
            "nobody was looking"
        );

        let watching = nexus.watch_detail(Some(Handle(1)));
        nexus.publish(&quiet, Some(&detail), Duration::from_secs(1));
        assert_eq!(watching.borrow().clone(), Some(detail));

        // Closing the view stops it — by dropping the receiver, which is
        // what a closed screen actually does. There is no way to forget: a
        // subscription that ends stops being refreshed whether or not anyone
        // announces it.
        drop(watching);
        nexus.refresh_detail(Handle(1));
        assert!(
            nexus
                .detail_senders()
                .get(&Handle(1))
                .is_some_and(|sender| sender.receiver_count() == 0),
            "nothing is refreshed for a view nobody holds"
        );

        // The empty slot itself is swept the next time anyone asks, so a
        // long session does not accumulate one per collection ever opened.
        nexus.watch_detail(None);
        assert!(nexus.detail_senders().is_empty());
        nexus.close().await;
    }

    /// Going to the background does not change what this device can reach,
    /// and coming back does not make a connection exist. Connectivity used to
    /// be derived from this flag alone, so an app in the foreground reported
    /// itself as connecting to a service nobody had configured — forever, and
    /// without a socket ever being opened.
    #[tokio::test]
    async fn foregrounding_does_not_invent_a_connection() {
        let scratch = Scratch::new("activity");
        let mut nexus = open(&scratch);

        let before = nexus.state().connectivity;
        nexus.set_active(false);
        assert_eq!(nexus.state().connectivity, before);
        nexus.set_active(true);
        assert_eq!(nexus.state().connectivity, before);
        assert_eq!(
            before,
            Connectivity::LocalOnly,
            "nothing is configured in a scratch device"
        );

        nexus.close().await;
    }

    #[tokio::test]
    async fn an_empty_torrent_import_is_refused_before_it_is_queued() {
        let scratch = Scratch::new("empty-import");
        let nexus = open(&scratch);

        assert!(matches!(
            nexus.command(&Command::ImportTorrent {
                source: "  ".to_owned()
            }),
            Err(CommandError::Invalid(message)) if message.contains("magnet URI")
        ));

        nexus.close().await;
    }

    #[tokio::test]
    async fn a_torrent_import_is_a_durable_collection_before_downloading() {
        let scratch = Scratch::new("torrent-import");
        let nexus = open(&scratch);

        nexus
            .command(&Command::ImportTorrent {
                source: "magnet:?xt=urn:btih:0123456789abcdef".to_owned(),
            })
            .expect("records the import");
        let imported = nexus.state().collections[0].clone();
        assert_eq!(imported.name, "Portalis collection import");
        // Resolution is active metadata work, not download authorization. The
        // collection becomes Draft only after a file list exists and awaits
        // the person's explicit Download decision.
        assert_eq!(imported.status, Status::ResolvingMetadata);
        assert_eq!(imported.entries, 0, "metadata has not resolved yet");
        nexus.close().await;

        let reopened = open(&scratch);
        assert_eq!(reopened.state().collections.len(), 1);
        // Still resolving after a restart: it is active metadata work but can
        // never acquire content until it reaches AwaitingSelection and the
        // person presses Download.
        assert_eq!(
            reopened.state().collections[0].status,
            Status::ResolvingMetadata
        );
        let key = reopened
            .collection_key(reopened.state().collections[0].id)
            .expect("collection key");
        assert_eq!(
            reopened
                .store
                .collection(&key)
                .expect("reads")
                .expect("exists")
                .lifecycle,
            StoredLifecycle::TorrentResolving
        );
        reopened.close().await;
    }

    /// ADR-0015: two concurrent identical imports return one collection
    /// rather than two racing to create durable rows for the same torrent.
    #[tokio::test]
    async fn concurrent_identical_imports_return_one_collection() {
        let scratch = Scratch::new("concurrent-identical-imports");
        let nexus = Arc::new(open(&scratch));
        const MAGNET: &str = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567";

        let first_nexus = Arc::clone(&nexus);
        let second_nexus = Arc::clone(&nexus);
        let first = tokio::task::spawn_blocking(move || {
            first_nexus.command(&Command::ImportTorrent {
                source: MAGNET.to_owned(),
            })
        });
        let second = tokio::task::spawn_blocking(move || {
            second_nexus.command(&Command::ImportTorrent {
                source: MAGNET.to_owned(),
            })
        });
        let first = first.await.expect("task joins").expect("accepted");
        let second = second.await.expect("task joins").expect("accepted");

        assert_eq!(
            first.collection, second.collection,
            "two racing identical imports must resolve to one collection"
        );
        assert_eq!(
            nexus.state().collections.len(),
            1,
            "no duplicate durable collection was created"
        );
        Arc::into_inner(nexus)
            .expect("no other references remain")
            .close()
            .await;
    }

    /// ADR-0015: equivalent magnet encodings (different casing, different
    /// optional parameters) name the same durable torrent identity.
    #[tokio::test]
    async fn equivalent_magnet_encodings_resolve_to_one_durable_identity() {
        let scratch = Scratch::new("equivalent-magnet-encodings");
        let nexus = open(&scratch);
        const CANONICAL: &str = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567";
        // Same info hash, different case and an extra display-name parameter
        // — cosmetically different, but the same torrent.
        const EQUIVALENT: &str =
            "magnet:?xt=urn:btih:0123456789ABCDEF0123456789ABCDEF01234567&dn=Same+Torrent";

        let first = nexus
            .command(&Command::ImportTorrent {
                source: CANONICAL.to_owned(),
            })
            .expect("first import accepted");
        let second = nexus
            .command(&Command::ImportTorrent {
                source: EQUIVALENT.to_owned(),
            })
            .expect("equivalent import accepted");

        assert_eq!(
            first.collection, second.collection,
            "differently encoded magnets for the same info hash are one collection"
        );
        assert_eq!(nexus.state().collections.len(), 1);
        nexus.close().await;
    }

    /// ADR-0015: reimporting the same torrent after a restart returns the
    /// existing collection rather than creating another durable row.
    #[tokio::test]
    async fn reimport_after_restart_returns_the_existing_collection() {
        let scratch = Scratch::new("reimport-after-restart");
        const MAGNET: &str = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567";

        let nexus = open(&scratch);
        nexus
            .command(&Command::ImportTorrent {
                source: MAGNET.to_owned(),
            })
            .expect("first import accepted");
        nexus.close().await;

        let reopened = open(&scratch);
        reopened
            .command(&Command::ImportTorrent {
                source: MAGNET.to_owned(),
            })
            .expect("reimport accepted");
        assert_eq!(
            reopened.state().collections.len(),
            1,
            "reimport after restart must not create a second row"
        );
        reopened.close().await;
    }

    /// ADR-0015: a failed import can be retried successfully — failure must
    /// never permanently poison the identity it was attempted under.
    #[tokio::test]
    async fn a_failed_import_can_be_retried_successfully() {
        let scratch = Scratch::new("retry-after-failure");
        let nexus = open(&scratch);

        // Malformed: is_magnet and is_torrent_path both reject this, so
        // command validation refuses it before any collection row exists.
        let rejected = nexus.command(&Command::ImportTorrent {
            source: String::new(),
        });
        assert!(rejected.is_err(), "an empty source is refused up front");
        assert!(
            nexus.state().collections.is_empty(),
            "a refused import leaves no row to retry against"
        );

        // The identical, valid source now succeeds — nothing about the
        // earlier rejection poisoned it.
        let accepted = nexus
            .command(&Command::ImportTorrent {
                source: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567".to_owned(),
            })
            .expect("retry with a valid source succeeds");
        assert!(accepted.collection.is_some());
        assert_eq!(nexus.state().collections.len(), 1);
        nexus.close().await;
    }

    /// A source is recorded at once and resolved afterwards, whether it is a
    /// local descriptor or a magnet. The interface sees "preparing" and then
    /// a file list, and never a command that blocked on a swarm.
    #[tokio::test]
    async fn a_torrent_source_resolves_into_a_selection_then_downloads_it() {
        let scratch = Scratch::new("torrent-import");
        let substrate = Arc::new(crate::nexus::substrate::Recorded::inspecting(
            crate::nexus::substrate::Inspected {
                info_hash: "abc123".to_owned(),
                name: "Bundle".to_owned(),
                files: vec![
                    crate::nexus::torrent::TorrentMetadataFile {
                        label: "a.txt".to_owned(),
                        bytes: 5,
                    },
                    crate::nexus::torrent::TorrentMetadataFile {
                        label: "b.txt".to_owned(),
                        bytes: 7,
                    },
                ],
                descriptor: b"descriptor".to_vec(),
            },
        ));
        let nexus = open_with_substrate(&scratch, substrate.clone());

        let accepted = nexus
            .command(&Command::ImportTorrent {
                source: "magnet:?xt=urn:btih:abc123".to_owned(),
            })
            .expect("records the source");
        let handle = accepted.collection.expect("names its collection");

        // Immediately: a row exists and metadata resolution is visibly active,
        // but this does not authorize downloading any content.
        let imported = nexus.state().collections[0].clone();
        assert_eq!(imported.status, Status::ResolvingMetadata);
        assert_eq!(imported.entries, 0, "nothing is known yet");

        let mut watching = nexus.watch_detail(Some(handle));
        settle(&nexus, &mut watching, |detail| {
            detail.is_some_and(|detail| detail.entries.len() == 2)
        })
        .await;

        // The worker resolved it: the real name, the real files, all chosen.
        let resolved = nexus.state().collections[0].clone();
        assert_eq!(
            resolved.name, "Bundle",
            "the source's own name replaces the placeholder"
        );
        assert_eq!(resolved.entries, 2);
        assert_eq!(resolved.total_bytes, 12);
        assert_eq!(
            resolved.status,
            Status::MetadataReady,
            "resolved choices wait for the explicit Download action"
        );
        let detail = watching.borrow().clone().expect("a selection");
        assert_eq!(detail.entries[0].label, "a.txt");
        assert!(
            detail.entries.iter().all(|entry| entry.selected),
            "everything starts selected"
        );
        assert!(
            substrate.selections.lock().unwrap().is_empty(),
            "and nothing is downloaded before anyone chooses"
        );

        // Publish is for local files this device owns. Routing a resolved
        // import through it would promote awaiting-selection straight to
        // executable, which is the explicit-Download bypass this refuses.
        let refused = nexus
            .command(&Command::PublishDraft { collection: handle })
            .expect_err("an import is confirmed with Download, not Publish");
        assert!(matches!(refused, CommandError::Invalid(_)));
        let key = nexus.collection_key(handle).expect("collection key");
        assert_eq!(
            nexus
                .store
                .collection(&key)
                .expect("reads")
                .expect("exists")
                .lifecycle,
            StoredLifecycle::TorrentAwaitingSelection,
            "the refusal left no executable intent behind"
        );
        assert!(
            substrate.selections.lock().unwrap().is_empty(),
            "and produced no acquisition work"
        );

        // Choosing one file starts exactly that download.
        assert!(matches!(
            nexus.command(&Command::DownloadSelection {
                collection: handle,
                entries: Vec::new(),
            }),
            Err(CommandError::Invalid(message)) if message.contains("at least one")
        ));
        nexus
            .command(&Command::DownloadSelection {
                collection: handle,
                entries: vec![Handle(2)],
            })
            .expect("confirms a selection");

        settle(&nexus, &mut watching, |_| {
            !substrate.selections.lock().unwrap().is_empty()
        })
        .await;
        let selections = substrate.selections.lock().unwrap().clone();
        let (source, files, _) = selections.first().expect("one download started");
        assert_eq!(source, "magnet:?xt=urn:btih:abc123");
        assert_eq!(files, &[1], "only the confirmed file, by its index");
        assert_eq!(
            substrate.acquisition_descriptors.lock().unwrap().as_slice(),
            [Some(b"descriptor".to_vec())],
            "starting a resolved import must not ask the sender for metadata a second time"
        );
        nexus.close().await;
    }

    #[tokio::test]
    async fn a_torrent_import_rejects_a_source_that_is_neither_a_magnet_nor_a_torrent_file() {
        let scratch = Scratch::new("bad-torrent-import");
        let nexus = open(&scratch);

        assert!(matches!(
            nexus.command(&Command::ImportTorrent {
                source: "https://example.com/movie".to_owned()
            }),
            Err(CommandError::Invalid(message)) if message.contains("magnet URI")
        ));
        assert!(nexus.state().collections.is_empty());
        nexus.close().await;
    }

    #[tokio::test]
    async fn a_store_from_the_future_refuses_to_open_and_says_why() {
        let scratch = Scratch::new("future");
        {
            let store = Store::open(scratch.0.join("portalis.redb")).expect("opens");
            drop(store);
        }
        // Written by a build that speaks a newer schema.
        let database = redb::Database::create(scratch.0.join("portalis.redb")).expect("opens");
        let write = database.begin_write().expect("writes");
        {
            write
                .open_table(crate::nexus::store::schema::META)
                .expect("meta")
                .insert(crate::nexus::store::schema::SCHEMA_VERSION_KEY, 99_u64)
                .expect("bumps");
        }
        write.commit().expect("commits");
        drop(database);

        let refused = Nexus::open(&Config {
            data_dir: scratch.0.clone(),
            device_name: "Ada's laptop".to_owned(),
            fingerprint: "ada-fingerprint".to_owned(),
        })
        .expect_err("must refuse");

        assert!(
            refused.to_string().contains("upgrade"),
            "the person is told what to do: {refused}"
        );
    }

    /// A receiver can finish while the app is open, then restart before the
    /// substrate has published its first reading. The durable completion is
    /// already authoritative at hydration time; presenting the stale process
    /// handle as an active download turns a completed QR import back into a
    /// download request until that reading arrives (or forever if it does
    /// not).
    #[tokio::test]
    async fn a_completed_receiver_import_is_available_when_the_app_reopens() {
        let scratch = Scratch::new("completed-receiver-restart");
        let store = Store::open(scratch.0.join("portalis.redb")).expect("opens store");
        store
            .put_collection(
                b"received",
                &StoredCollection {
                    name: "Jam Jar".to_owned(),
                    role: StoredRole::Member,
                    content_key: [0; 32],
                    media_path: scratch.0.join("downloads").to_string_lossy().into_owned(),
                    sources: Vec::new(),
                    lifecycle: StoredLifecycle::TorrentRequested {
                        activity: StoredActivity::Running,
                    },
                    on_disk_bytes: 41 * 1024 * 1024,
                    substrate_handle: Some("a1b2".to_owned()),
                    started_at: Some(1),
                    completed_at: Some(2),
                },
            )
            .expect("records the completed import");
        store
            .put_torrent_import(b"received", "magnet:?xt=urn:btih:a1b2")
            .expect("records its source");
        store
            .put_torrent_import_entries(
                b"received",
                &[StoredImportEntry {
                    label: "IMG_8416.HEIC".to_owned(),
                    bytes: 41 * 1024 * 1024,
                    selected: true,
                    native_location: None,
                }],
            )
            .expect("records its selected entry");
        drop(store);

        let reopened = open(&scratch);

        assert_eq!(
            reopened.state().collections[0].status,
            Status::Available,
            "a completed receiver import must not return as an active download"
        );
        reopened.close().await;
    }

    /// The transfer poller is the one worker that turns a substrate reading
    /// into state and history: it attributes a holding to the collection that
    /// claims it, records the start and finish moments when they arrive, keeps
    /// the ring of readings bounded, and releases a torrent no collection
    /// claims. Driven here with a scripted double rather than a swarm, so each
    /// of those decisions is exercised on real store rows instead of being
    /// trusted.
    ///
    /// A peer that connects, moves bytes, and disconnects before the
    /// collection either finishes or the app shuts down must still be
    /// durably remembered — see `a_peer_that_disconnects_mid_transfer_is_not_lost`
    /// just below, which is the regression this comment used to lack.
    #[tokio::test]
    async fn the_transfer_poller_turns_each_reading_into_state_and_history() {
        let scratch = Scratch::new("transfer-poller");
        let store = Arc::new(Store::open(scratch.0.join("portalis.redb")).expect("opens store"));
        store
            .put_collection(
                b"key",
                &StoredCollection {
                    name: "Iceland".to_owned(),
                    role: StoredRole::Owner,
                    content_key: [0; 32],
                    media_path: scratch.0.join("media").to_string_lossy().into_owned(),
                    sources: Vec::new(),
                    lifecycle: StoredLifecycle::TorrentRequested {
                        activity: StoredActivity::Running,
                    },
                    on_disk_bytes: 0,
                    substrate_handle: Some("a1b2".to_owned()),
                    started_at: None,
                    completed_at: None,
                },
            )
            .expect("writes the collection");

        let local = Arc::new(Mutex::new(LocalCollections::test_with_collection(b"key")));
        let handle = local
            .lock()
            .expect("local collections")
            .handle(b"key")
            .expect("the one collection");

        let mut initial = state(vec![collection("Iceland")]);
        initial.collections[0].id = handle;
        let (states, _watcher) = watch::channel(initial);

        // Two readings, in order: a download in progress, then the engine
        // saying it is done. A second torrent in the first reading that no
        // collection claims is the orphan the poller is meant to release.
        fn reading(
            info_hash: &str,
            progress: u64,
            finished: bool,
        ) -> crate::nexus::torrent::TorrentInfo {
            crate::nexus::torrent::TorrentInfo {
                id: 1,
                info_hash: info_hash.to_owned(),
                name: "Iceland".to_owned(),
                state: "live".to_owned(),
                progress_bytes: progress,
                source_check_bytes: None,
                fetched_bytes: progress,
                total_bytes: 100,
                uploaded_bytes: 0,
                finished,
                error: None,
                files: Vec::new(),
                live_peers: 1,
                live_peer_addrs: vec![crate::nexus::torrent::PeerLink {
                    address: "10.0.0.1:6881".to_owned(),
                    fetched_bytes: 0,
                    uploaded_bytes: 0,
                    client: None,
                }],
            }
        }
        let moving = reading("a1b2", 10, false);
        let done = reading("a1b2", 100, true);
        let orphan = reading("deadbeef", 10, false);
        let substrate = Arc::new(crate::nexus::substrate::Recorded::reading(vec![
            vec![moving, orphan],
            vec![done],
        ]));

        let holdings = Holdings::default();
        let activity = crate::nexus::activity::DeviceActivityTracker::start(
            Arc::clone(&store),
            unix_time_ns(),
        )
        .expect("starts activity ledger");
        let sources = super::DetailSources {
            store: Arc::clone(&store),
            collections: Arc::clone(&local),
            holdings: holdings.clone(),
            senders: Arc::new(Mutex::new(HashMap::new())),
        };
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let shutdown = Shutdown::from_signal(shutdown_rx);

        let poller = tokio::spawn({
            let substrate = Arc::clone(&substrate);
            let states = states.clone();
            let local = Arc::clone(&local);
            let sources = sources.clone();
            let store = Arc::clone(&store);
            let holdings = holdings.clone();
            let activity = activity.clone();
            let bus = Arc::new(crate::nexus::core::events::EventBus::new());
            async move {
                transfers::follow_transfers(
                    store, states, local, substrate, holdings, shutdown, sources, activity, bus,
                )
                .await
            }
        });

        // Bounded, condition-driven wait: the moments land once the second
        // reading has been processed, which is the point the assertions below
        // need. A poller that never runs must fail loudly.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let store_row = || store.collection(b"key").expect("reads").expect("exists");
        while !(store_row().started_at.is_some() && store_row().completed_at.is_some()) {
            assert!(
                std::time::Instant::now() < deadline,
                "the poller never recorded both moments"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        // The moments are written once each, the moment the engine reported
        // them, and the ring holds the readings that differed.
        let row = store_row();
        assert!(row.started_at.is_some(), "the first byte started it");
        assert!(row.completed_at.is_some(), "the engine's word finished it");
        assert!(row.completed_at.expect("an end") >= row.started_at.expect("a start"));
        let samples = store.samples(b"key").expect("reads the ring");
        assert!(
            samples.len() >= 2,
            "the ramp and the finish are both in the history, got {}",
            samples.len()
        );

        // The progress tier says what the last reading says, for the
        // collection the poller found by its handle.
        let seen = states.borrow();
        let seen = seen
            .collections
            .iter()
            .find(|collection| collection.id == handle)
            .expect("the collection is in the snapshot");
        assert_eq!(seen.on_disk_bytes, 100, "the reading's bytes land in state");
        assert_eq!(seen.total_bytes, 100);
        assert_eq!(seen.status, Status::Available, "finished is available");

        // The orphan was released: the poller noticed it and asked the engine
        // to let it go. (Copied out before the await, so the guard is never
        // held across it.)
        let released = substrate.released.lock().unwrap().clone();

        shutdown_tx.send(true).expect("asks the poller to stop");
        poller.await.expect("the poller winds up");
        let activity = activity.snapshot().expect("reads activity");
        assert_eq!(activity.run.network_down_bytes, 100);
        assert_eq!(activity.run.completed_downloads, 1);

        assert_eq!(
            released.as_slice(),
            &["deadbeef".to_owned()],
            "unclaimed torrents go"
        );
        assert!(
            !released.iter().any(|handle| handle == "a1b2"),
            "a claimed collection is never released"
        );
    }

    /// ADR-0016: a receiver-side completion is a typed durable event, not
    /// something Flutter has to infer from diffing two snapshots. Only the
    /// poller ever computes `completed_download`, so this is the one and
    /// only place `TransferSettled` can come from — proven here against the
    /// real `follow_transfers` production path, not a hand-rolled double.
    #[tokio::test]
    async fn a_completed_transfer_emits_a_typed_settled_event() {
        let scratch = Scratch::new("transfer-settled-event");
        let store = Arc::new(Store::open(scratch.0.join("portalis.redb")).expect("opens store"));
        store
            .put_collection(
                b"key",
                &StoredCollection {
                    name: "Iceland".to_owned(),
                    role: StoredRole::Member,
                    content_key: [0; 32],
                    media_path: scratch.0.join("media").to_string_lossy().into_owned(),
                    sources: Vec::new(),
                    lifecycle: StoredLifecycle::TorrentRequested {
                        activity: StoredActivity::Running,
                    },
                    on_disk_bytes: 0,
                    substrate_handle: Some("a1b2".to_owned()),
                    started_at: None,
                    completed_at: None,
                },
            )
            .expect("writes the collection");

        let local = Arc::new(Mutex::new(LocalCollections::test_with_collection(b"key")));
        let handle = local
            .lock()
            .expect("local collections")
            .handle(b"key")
            .expect("the one collection");

        let mut initial = state(vec![collection("Iceland")]);
        initial.collections[0].id = handle;
        let (states, _watcher) = watch::channel(initial);

        fn reading(progress: u64, finished: bool) -> crate::nexus::torrent::TorrentInfo {
            crate::nexus::torrent::TorrentInfo {
                id: 1,
                info_hash: "a1b2".to_owned(),
                name: "Iceland".to_owned(),
                state: "live".to_owned(),
                progress_bytes: progress,
                source_check_bytes: None,
                fetched_bytes: progress,
                total_bytes: 100,
                uploaded_bytes: 0,
                finished,
                error: None,
                files: Vec::new(),
                live_peers: 1,
                live_peer_addrs: vec![crate::nexus::torrent::PeerLink {
                    address: "10.0.0.1:6881".to_owned(),
                    fetched_bytes: 0,
                    uploaded_bytes: 0,
                    client: None,
                }],
            }
        }
        let substrate = Arc::new(crate::nexus::substrate::Recorded::reading(vec![
            vec![reading(10, false)],
            vec![reading(100, true)],
        ]));

        let holdings = Holdings::default();
        let activity = crate::nexus::activity::DeviceActivityTracker::start(
            Arc::clone(&store),
            unix_time_ns(),
        )
        .expect("starts activity ledger");
        let sources = super::DetailSources {
            store: Arc::clone(&store),
            collections: Arc::clone(&local),
            holdings: holdings.clone(),
            senders: Arc::new(Mutex::new(HashMap::new())),
        };
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let shutdown = Shutdown::from_signal(shutdown_rx);
        let bus = Arc::new(crate::nexus::core::events::EventBus::new());
        let mut settled = bus.subscribe().await;

        let poller = tokio::spawn({
            let states = states.clone();
            let local = Arc::clone(&local);
            let sources = sources.clone();
            let store = Arc::clone(&store);
            let holdings = holdings.clone();
            let bus = Arc::clone(&bus);
            async move {
                transfers::follow_transfers(
                    store, states, local, substrate, holdings, shutdown, sources, activity, bus,
                )
                .await
            }
        });

        let event = tokio::time::timeout(Duration::from_secs(10), settled.next())
            .await
            .expect("the poller emits the event before the deadline")
            .expect("the bus is still open");
        assert_eq!(
            event,
            crate::nexus::core::events::Event::TransferSettled {
                collection: crate::nexus::core::events::Handle(u64::from(handle.0)),
                ok: true,
            }
        );

        shutdown_tx.send(true).expect("asks the poller to stop");
        poller.await.expect("the poller winds up");
    }

    /// A peer that connects, sends bytes, and disconnects before the
    /// collection finishes or the app shuts down must still show up in
    /// durable peer history. Snapshotting only at completion or shutdown
    /// (the previous behaviour) meant a peer that left mid-transfer was
    /// simply never written — Store::peer_history would come back empty
    /// for it, and it would vanish from the People screen the moment its
    /// live connection dropped, with no way to ever re-derive it.
    #[tokio::test]
    async fn a_peer_that_disconnects_mid_transfer_is_not_lost() {
        let scratch = Scratch::new("peer-mid-transfer");
        let store = Arc::new(Store::open(scratch.0.join("portalis.redb")).expect("opens store"));
        store
            .put_collection(
                b"key",
                &StoredCollection {
                    name: "Iceland".to_owned(),
                    role: StoredRole::Owner,
                    content_key: [0; 32],
                    media_path: scratch.0.join("media").to_string_lossy().into_owned(),
                    sources: Vec::new(),
                    lifecycle: StoredLifecycle::TorrentRequested {
                        activity: StoredActivity::Running,
                    },
                    on_disk_bytes: 0,
                    substrate_handle: Some("a1b2".to_owned()),
                    started_at: None,
                    completed_at: None,
                },
            )
            .expect("writes the collection");

        let local = Arc::new(Mutex::new(LocalCollections::test_with_collection(b"key")));
        let handle = local
            .lock()
            .expect("local collections")
            .handle(b"key")
            .expect("the one collection");

        let mut initial = state(vec![collection("Iceland")]);
        initial.collections[0].id = handle;
        let (states, _watcher) = watch::channel(initial);

        fn reading(progress: u64, peer_present: bool) -> crate::nexus::torrent::TorrentInfo {
            crate::nexus::torrent::TorrentInfo {
                id: 1,
                info_hash: "a1b2".to_owned(),
                name: "Iceland".to_owned(),
                state: "live".to_owned(),
                progress_bytes: progress,
                source_check_bytes: None,
                fetched_bytes: progress,
                total_bytes: 100,
                uploaded_bytes: 0,
                finished: false,
                error: None,
                files: Vec::new(),
                live_peers: u32::from(peer_present),
                live_peer_addrs: if peer_present {
                    vec![crate::nexus::torrent::PeerLink {
                        address: "10.0.0.9:6881".to_owned(),
                        fetched_bytes: 40,
                        uploaded_bytes: 5,
                        client: Some("qBittorrent 4.6".to_owned()),
                    }]
                } else {
                    Vec::new()
                },
            }
        }
        // The peer is present on the first reading, then gone on the
        // second — disconnected, with the collection still in progress
        // (never finished, and the poller is stopped without a graceful
        // shutdown snapshot).
        let connected = reading(10, true);
        let disconnected = reading(40, false);
        let substrate = Arc::new(crate::nexus::substrate::Recorded::reading(vec![
            vec![connected],
            vec![disconnected],
        ]));

        let holdings = Holdings::default();
        let activity = crate::nexus::activity::DeviceActivityTracker::start(
            Arc::clone(&store),
            unix_time_ns(),
        )
        .expect("starts activity ledger");
        let sources = super::DetailSources {
            store: Arc::clone(&store),
            collections: Arc::clone(&local),
            holdings: holdings.clone(),
            senders: Arc::new(Mutex::new(HashMap::new())),
        };
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let shutdown = Shutdown::from_signal(shutdown_rx);

        let poller = tokio::spawn({
            let substrate = Arc::clone(&substrate);
            let states = states.clone();
            let local = Arc::clone(&local);
            let sources = sources.clone();
            let store = Arc::clone(&store);
            let holdings = holdings.clone();
            let activity = activity.clone();
            let bus = Arc::new(crate::nexus::core::events::EventBus::new());
            async move {
                transfers::follow_transfers(
                    store, states, local, substrate, holdings, shutdown, sources, activity, bus,
                )
                .await
            }
        });

        // Bounded, condition-driven wait: the peer must be durable the
        // moment the second reading (with it gone) has been processed —
        // not merely by the time the poller is asked to stop.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let durable = store.peer_history(b"key").expect("reads peer history");
            if durable.iter().any(|peer| peer.address == "10.0.0.9:6881") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "a peer that disconnected mid-transfer was never written to durable history"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let durable = store.peer_history(b"key").expect("reads peer history");
        let remembered = durable
            .iter()
            .find(|peer| peer.address == "10.0.0.9:6881")
            .expect("the disconnected peer is remembered");
        assert_eq!(remembered.total_down_bytes, 40);
        assert_eq!(remembered.total_up_bytes, 5);

        shutdown_tx.send(true).expect("asks the poller to stop");
        poller.await.expect("the poller winds up");
    }
}
