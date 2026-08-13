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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use thiserror::Error;
use tokio::sync::watch;

use super::supervisor::Supervisor;
use crate::projection::emit::Projector;
use crate::projection::state::{
    Accepted, Command, CommandError, Connectivity, Detail, DeviceState, Handle, PortalisState,
};
use crate::store::{Store, StoreError};

/// Where the core keeps its file, and who it is.
#[derive(Clone, Debug)]
pub struct Config {
    /// The local store's directory. One file inside it (§12).
    pub data_dir: std::path::PathBuf,
    /// What to call this device until the person renames it.
    pub device_name: String,
}

/// Why the core did not start.
#[derive(Debug, Error)]
pub enum OpenError {
    /// Including a store written by a newer version, which is reported as
    /// itself rather than as a generic failure — the person needs to be told
    /// to upgrade, not that something went wrong.
    #[error(transparent)]
    Store(#[from] StoreError),
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
    #[allow(
        dead_code,
        reason = "the workflows that write through it arrive with the bridge"
    )]
    store: Store,
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
        let store = Store::open(config.data_dir.join("portalis.redb"))?;
        let device = DeviceState {
            name: config.device_name.clone(),
            handle: None,
            fingerprint: String::new(),
            devices: 1,
        };
        let first = PortalisState {
            device,
            connectivity: Connectivity::LocalOnly,
            contacts: Vec::new(),
            collections: Vec::new(),
            alerts: Vec::new(),
        };

        Ok(Self {
            supervisor: Supervisor::default(),
            states: watch::Sender::new(first),
            details: watch::Sender::new(None),
            projector: Arc::new(Mutex::new(Projector::new())),
            next_command: AtomicU64::new(1),
            store,
        })
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
        if collection.is_none() {
            // Stop holding what nobody is looking at.
            self.details.send_replace(None);
        }
        self.details.subscribe()
    }

    /// Accepts a command, or says why not.
    ///
    /// Returns before anything is attempted. What becomes of it arrives
    /// through [`Self::watch`], on the object it affects, which is what lets
    /// the interface show an operation that was in flight when the app was
    /// last closed.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the command is malformed, not something
    /// this device may do, or needs a connection it cannot wait for.
    pub fn command(&self, command: &Command) -> Result<Accepted, CommandError> {
        validate(command)?;

        // Deferrable commands are queued rather than refused, which is what
        // lets the interface accept one instantly with the network down.
        let queued = command.is_deferrable();
        if !queued {
            return Err(CommandError::Unavailable);
        }
        Ok(Accepted {
            id: self.next_command.fetch_add(1, Ordering::Relaxed),
            queued,
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
        _ => return Ok(()),
    };
    Err(CommandError::Invalid(complaint.to_owned()))
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
        assert_ne!(first.id, second.id, "each is named separately");
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
        })
        .expect_err("must refuse");

        assert!(
            refused.to_string().contains("upgrade"),
            "the person is told what to do: {refused}"
        );
    }
}
