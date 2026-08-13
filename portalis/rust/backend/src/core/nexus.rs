//! The five calls the interface has, and no more without a reason recorded.
//!
//! `SPEC.md` §16. Everything above this line is Rust deciding things;
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

use thiserror::Error;
use tokio::sync::watch;

use super::supervisor::Supervisor;
use crate::projection::emit::Projector;
use crate::projection::state::{
    Accepted, CollectionState, Command, CommandError, Connectivity, Detail, DeviceState, Handle,
    PortalisState, Role, Status,
};
use crate::store::records::{Role as StoredRole, StoredCollection, StoredImportEntry};
use crate::store::{Store, StoreError};

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
#[derive(Debug)]
pub struct Nexus {
    supervisor: Supervisor,
    states: watch::Sender<PortalisState>,
    details: watch::Sender<Option<Detail>>,
    projector: Arc<Mutex<Projector>>,
    /// Names each accepted command, so the interface can match one to the
    /// `pending` field it appears in.
    next_command: AtomicU64,
    active: bool,
    store: Arc<Store>,
    collections: Mutex<LocalCollections>,
}

/// The process-local handles for durable collection records.
///
/// Store keys survive a restart; handles deliberately do not. Keeping their
/// mapping beside the projection prevents either identifier becoming the
/// other's accidental public API.
#[derive(Debug, Default)]
struct LocalCollections {
    keys: HashMap<Handle, Vec<u8>>,
    next_handle: u32,
}

impl LocalCollections {
    fn hydrate(store: &Store) -> Result<(Self, Vec<CollectionState>), StoreError> {
        let mut local = Self::default();
        let mut projected = Vec::new();
        for (key, stored) in store.collections()? {
            let imported_entries = store.torrent_import_entries(&key)?;
            let handle = local.assign(key.clone());
            let revision = store
                .current_revision(&key)?
                .map_or(0, |(number, _)| number);
            let status = if store.torrent_import(&key)?.is_some() {
                Status::Preparing
            } else {
                Status::Available
            };
            projected.push(CollectionState {
                id: handle,
                name: stored.name,
                role: match stored.role {
                    StoredRole::Owner => Role::Owner,
                    StoredRole::Member => Role::Member,
                },
                revision,
                status,
                members: Vec::new(),
                entries: u32::try_from(imported_entries.len()).unwrap_or(u32::MAX),
                total_bytes: imported_entries.iter().map(|entry| entry.bytes).sum(),
                transfer: None,
                pending: None,
            });
        }
        Ok((local, projected))
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
        let (collections, collection_states) = LocalCollections::hydrate(&store)?;
        let device = DeviceState {
            name: config.device_name.clone(),
            handle: None,
            fingerprint: config.fingerprint.clone(),
            devices: 1,
        };
        let first = PortalisState {
            device,
            connectivity: Connectivity::LocalOnly,
            contacts: Vec::new(),
            collections: collection_states,
            alerts: Vec::new(),
        };

        Ok(Self {
            supervisor: Supervisor::default(),
            states: watch::Sender::new(first),
            details: watch::Sender::new(None),
            projector: Arc::new(Mutex::new(Projector::new())),
            next_command: AtomicU64::new(1),
            active: true,
            store,
            collections: Mutex::new(collections),
        })
    }

    /// Opens the runtime from the one platform-owned state directory.
    pub fn open_default() -> Result<Self, OpenError> {
        let device = crate::device::device_identity()
            .map_err(|error| OpenError::Identity(error.to_string()))?;
        let config = Config {
            data_dir: crate::paths::state_dir(),
            device_name: device.nickname,
            fingerprint: device.device_id,
        };
        Self::open_with_store(&config, crate::store::app_store()?)
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
        let mut state = self.state();
        state.connectivity = if active {
            Connectivity::Connecting
        } else {
            Connectivity::LocalOnly
        };
        self.states.send_replace(state);
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

    /// Subscribes to one collection's detail, or unsubscribes with `None`.
    ///
    /// The expensive tier costs nothing until this is called, and stops
    /// costing anything the moment it is called with `None`.
    pub fn watch_detail(&self, collection: Option<Handle>) -> watch::Receiver<Option<Detail>> {
        self.projector
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .watch_detail(collection);
        if let Some(collection) = collection {
            self.details.send_replace(self.import_detail(collection));
        } else {
            // Stop holding what nobody is looking at.
            self.details.send_replace(None);
        }
        self.details.subscribe()
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

        // A collection the person has just created, renamed, or removed must
        // survive a crash before it can be published. The database write is
        // the acceptance boundary; network work still happens later.
        let collection = self.apply_local(command)?;

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
            Command::CreateCollection { name, .. } => self.create_collection(name).map(Some),
            Command::RenameCollection { collection, name } => {
                self.rename_collection(*collection, name).map(|()| None)
            }
            Command::DeleteCollection { collection, .. } => {
                self.delete_collection(*collection).map(|()| None)
            }
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

    fn create_collection(&self, name: &str) -> Result<Handle, CommandError> {
        let id = crate::collections::model::CollectionId::generate();
        let stored = StoredCollection {
            name: name.to_owned(),
            role: StoredRole::Owner,
            content_key: portalis_nexus_client::generate_content_key(),
            media_path: String::new(),
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
        state.collections.push(CollectionState {
            id: handle,
            name: stored.name,
            role: Role::Owner,
            revision: 0,
            status: Status::Available,
            members: Vec::new(),
            entries: 0,
            total_bytes: 0,
            transfer: None,
            pending: None,
        });
        self.states.send_replace(state);
        Ok(handle)
    }

    fn import_torrent(&self, source: &str) -> Result<Handle, CommandError> {
        let id = crate::collections::model::CollectionId::generate();
        let metadata = is_torrent_path(source)
            .then(|| crate::torrent::metadata_from_torrent_path(source))
            .transpose()
            .map_err(|error| {
                CommandError::Invalid(format!("could not read the .torrent file: {error}"))
            })?;
        let stored = StoredCollection {
            name: metadata
                .as_ref()
                .map_or_else(|| torrent_name(source), |metadata| metadata.name.clone()),
            role: StoredRole::Owner,
            content_key: portalis_nexus_client::generate_content_key(),
            media_path: String::new(),
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
        if let Some(metadata) = &metadata {
            let entries = metadata
                .files
                .iter()
                .map(|file| StoredImportEntry {
                    label: file.label.clone(),
                    bytes: file.bytes,
                    selected: true,
                })
                .collect::<Vec<_>>();
            if let Err(error) = self
                .store
                .put_torrent_import_entries(id.as_bytes(), &entries)
                .and_then(|()| {
                    self.store
                        .put_torrent_import_descriptor(id.as_bytes(), &metadata.descriptor)
                })
            {
                let _ = self.store.forget_torrent_import(id.as_bytes());
                let _ = self.store.forget_collection(id.as_bytes());
                return Err(persistence(error));
            }
        }

        let handle = self
            .collections
            .lock()
            .map_err(|_| CommandError::Persistence("the collection index was poisoned".to_owned()))?
            .assign(id.as_bytes().to_vec());
        let mut state = self.state();
        state.collections.push(CollectionState {
            id: handle,
            name: stored.name,
            role: Role::Owner,
            revision: 0,
            status: Status::Preparing,
            members: Vec::new(),
            entries: metadata.as_ref().map_or(0, |metadata| {
                u32::try_from(metadata.files.len()).unwrap_or(u32::MAX)
            }),
            total_bytes: metadata.as_ref().map_or(0, |metadata| {
                metadata.files.iter().map(|file| file.bytes).sum()
            }),
            transfer: None,
            pending: None,
        });
        self.states.send_replace(state);
        Ok(handle)
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

    fn delete_collection(&self, handle: Handle) -> Result<(), CommandError> {
        let key = self.collection_key(handle)?;
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
        for (index, entry) in entries.iter_mut().enumerate() {
            entry.selected =
                requested.contains(&u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1));
        }
        self.store
            .put_torrent_import_entries(&key, &entries)
            .map_err(persistence)?;
        self.details.send_replace(self.import_detail(collection));
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

    fn import_detail(&self, collection: Handle) -> Option<Detail> {
        let key = self.collection_key(collection).ok()?;
        self.store.torrent_import(&key).ok().flatten()?;
        let entries = self.store.torrent_import_entries(&key).ok()?;
        Some(Detail {
            id: collection,
            entries: entries
                .into_iter()
                .enumerate()
                .map(|(index, entry)| crate::projection::state::EntryState {
                    id: Handle(u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1)),
                    label: entry.label,
                    bytes: entry.bytes,
                    selected: entry.selected,
                    available: false,
                })
                .collect(),
            pieces: Vec::new(),
            samples: Vec::new(),
        })
    }

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
            self.details.send_replace(Some(detail));
        }
    }

    /// Stops every component and returns when the runtime is quiet.
    pub async fn close(self) {
        self.supervisor.shutdown().await;
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
            if !source.starts_with("magnet:?") && !is_torrent_path(source) =>
        {
            "choose a magnet URI or a .torrent file"
        }
        _ => return Ok(()),
    };
    Err(CommandError::Invalid(complaint.to_owned()))
}

fn torrent_name(source: &str) -> String {
    if source.starts_with("magnet:?") {
        return "Torrent import".to_owned();
    }
    std::path::Path::new(source)
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map_or_else(|| "Torrent import".to_owned(), ToOwned::to_owned)
}

fn is_torrent_path(source: &str) -> bool {
    std::path::Path::new(source)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("torrent"))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::projection::state::{CollectionState, Role, Status};

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
        Nexus::open(&Config {
            data_dir: scratch.0.clone(),
            device_name: "Ada's laptop".to_owned(),
            fingerprint: "ada-fingerprint".to_owned(),
        })
        .expect("opens")
    }

    fn collection(name: &str) -> CollectionState {
        CollectionState {
            id: Handle(1),
            name: name.to_owned(),
            role: Role::Owner,
            revision: 1,
            status: Status::Available,
            members: Vec::new(),
            entries: 1,
            total_bytes: 10,
            transfer: None,
            pending: None,
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

    #[tokio::test]
    async fn detail_costs_nothing_until_it_is_asked_for() {
        let scratch = Scratch::new("detail");
        let nexus = open(&scratch);
        let quiet = state(vec![collection("Iceland")]);
        let detail = Detail {
            id: Handle(1),
            entries: Vec::new(),
            pieces: vec![0xff; 8],
            samples: Vec::new(),
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

        // Closing the view stops it at once.
        nexus.watch_detail(None);
        assert_eq!(*watching.borrow(), None);
        nexus.close().await;
    }

    #[tokio::test]
    async fn foregrounding_and_backgrounding_change_only_connectivity() {
        let scratch = Scratch::new("activity");
        let mut nexus = open(&scratch);

        nexus.set_active(false);
        assert_eq!(nexus.state().connectivity, Connectivity::LocalOnly);
        nexus.set_active(true);
        assert_eq!(nexus.state().connectivity, Connectivity::Connecting);

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
        assert_eq!(imported.name, "Torrent import");
        assert_eq!(imported.status, Status::Preparing);
        assert_eq!(imported.entries, 0, "metadata has not resolved yet");
        nexus.close().await;

        let reopened = open(&scratch);
        assert_eq!(reopened.state().collections.len(), 1);
        assert_eq!(reopened.state().collections[0].status, Status::Preparing);
        reopened.close().await;
    }

    #[tokio::test]
    async fn a_local_torrent_resolves_a_durable_selection_without_downloading() {
        let scratch = Scratch::new("local-torrent-import");
        let source = scratch.0.join("fixture.torrent");
        std::fs::write(
            &source,
            b"d4:infod5:filesld6:lengthi5e4:pathl5:a.txteed6:lengthi7e4:pathl5:b.txteee4:name6:Bundle12:piece lengthi16384e6:pieces20:aaaaaaaaaaaaaaaaaaaaee",
        )
        .expect("writes descriptor");
        let nexus = open(&scratch);

        let accepted = nexus
            .command(&Command::ImportTorrent {
                source: source.display().to_string(),
            })
            .expect("imports metadata only");
        let imported = nexus.state().collections[0].clone();
        assert_eq!(accepted.collection, Some(imported.id));
        assert_eq!(imported.name, "Bundle");
        assert_eq!(imported.status, Status::Preparing);
        assert_eq!(imported.entries, 2);
        assert_eq!(imported.total_bytes, 12);

        let detail = nexus.watch_detail(Some(imported.id));
        let detail = detail.borrow().clone().expect("selection detail");
        assert_eq!(detail.entries.len(), 2);
        assert_eq!(detail.entries[0].label, "a.txt");
        assert_eq!(detail.entries[0].bytes, 5);
        assert!(
            detail.entries.iter().all(|entry| entry.selected),
            "everything starts selected"
        );
        assert!(!detail.entries[0].available, "nothing was downloaded");

        assert!(matches!(
            nexus.command(&Command::DownloadSelection {
                collection: imported.id,
                entries: Vec::new(),
            }),
            Err(CommandError::Invalid(message)) if message.contains("at least one")
        ));
        nexus
            .command(&Command::DownloadSelection {
                collection: imported.id,
                entries: vec![Handle(2)],
            })
            .expect("records a confirmed selection");
        let detail = nexus.watch_detail(Some(imported.id));
        assert_eq!(
            detail.borrow().as_ref().map(|detail| detail
                .entries
                .iter()
                .map(|entry| entry.selected)
                .collect::<Vec<_>>()),
            Some(vec![false, true]),
            "only the confirmed file remains selected"
        );
        nexus.close().await;

        std::fs::remove_file(source).expect("the original path is no longer needed");
        let reopened = open(&scratch);
        let restored = reopened.state().collections[0].clone();
        assert_eq!(restored.entries, 2);
        let detail = reopened.watch_detail(Some(restored.id));
        assert_eq!(
            detail.borrow().as_ref().map(|detail| {
                (
                    detail.entries.len(),
                    detail
                        .entries
                        .iter()
                        .map(|entry| entry.selected)
                        .collect::<Vec<_>>(),
                )
            }),
            Some((2, vec![false, true])),
            "the selection was persisted with the collection"
        );
        reopened.close().await;
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
                .open_table(crate::store::schema::META)
                .expect("meta")
                .insert(crate::store::schema::SCHEMA_VERSION_KEY, 99_u64)
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
}
