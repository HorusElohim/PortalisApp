//! The app-facing Nexus boundary.
//!
//! These values deliberately do not re-export the core projection. The core
//! uses terse Rust names such as `Handle` and `Status`; the bridge needs a
//! stable, unambiguous vocabulary that can evolve without colliding with the
//! legacy generated API while both paths exist.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::api::StreamSink;
use crate::nexus::core::nexus::Nexus;
use crate::nexus::projection::state::{Command, Detail, Handle, LocalFile, PortalisState};
use crate::nexus::projection::wire::Wire;

/// The complete, app-renderable Nexus projection.
#[derive(Clone, Debug)]
pub struct AppSnapshot {
    pub device: AppDevice,
    pub connectivity: String,
    pub contacts: Vec<AppContact>,
    pub collections: Vec<AppCollection>,
    pub alerts: Vec<String>,
    /// What the engine is doing right now, aggregated once here rather than
    /// left for every screen to recompute from `collections` — two Flutter
    /// call sites disagreeing about how many transfers were active ("1 ACTIVE
    /// TRANSFER" above a window reading "0 collections") is exactly the
    /// class of bug one shared derivation exists to make impossible.
    pub activity: AppActivity,
}

/// What the engine is doing right now, as one answer, aggregated across
/// every collection in the snapshot it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppActivity {
    /// Collections currently moving bytes.
    pub transfers: u32,
    pub down_bytes_per_second: u32,
    pub up_bytes_per_second: u32,
    /// Peers across every collection — what "connected" means to a person,
    /// who does not think per collection.
    pub peers: u32,
}

/// This device as a person can identify it.
#[derive(Clone, Debug)]
pub struct AppDevice {
    pub name: String,
    pub handle: Option<String>,
    pub fingerprint: String,
    pub devices: u32,
}

/// A contact and the trust information needed to render it.
#[derive(Clone, Debug)]
pub struct AppContact {
    pub id: u32,
    pub display_name: String,
    pub handle: Option<String>,
    pub fingerprint: String,
    pub verified: bool,
    pub friendship: String,
    pub reachable: Option<String>,
}

/// What a collection contains and how it entered Portalis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCollectionNature {
    Native,
    Torrent,
}

/// This device's role in a collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCollectionRole {
    Owner,
    Member,
}

/// The typed lifecycle Rust projects for application decisions.
///
/// This replaces Flutter's hand-written parser for `Status::wire()` strings.
/// Human-readable labels remain separate presentation data; widgets compare
/// this generated enum and never reinterpret backend spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCollectionLifecycle {
    Available,
    Seeding,
    Paused,
    Draft,
    ResolvingMetadata,
    WaitingForSender,
    MetadataReady,
    DownloadRequested,
    RetryingMetadata,
    Downloading,
    Updating,
    WaitingForOwner,
    AccessRemoved,
    NeedsNewerVersion,
    CannotVerify,
    ConflictingHistory,
}

/// Application decisions that only Rust may make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppCollectionCapabilities {
    pub can_add_media: bool,
    pub can_select: bool,
    pub can_share: bool,
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_delete: bool,
    pub can_delete_files: bool,
}

/// Presentation-ready collection facts derived once in Rust.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppCollectionFacts {
    pub complete: bool,
    pub sharing: bool,
    pub moving: bool,
    /// Metadata discovery is active or retrying. There may be no byte total
    /// yet, so Flutter renders an indeterminate indicator from this fact.
    pub preparing: bool,
    /// Authoritative zero-to-one progress even when no live transfer sample
    /// exists (completed collections remain 1.0 after restart).
    pub progress: f32,
}

/// One collection in the inexpensive list projection.
#[derive(Clone, Debug)]
pub struct AppCollection {
    pub id: u32,
    pub name: String,
    pub nature: AppCollectionNature,
    pub role: AppCollectionRole,
    pub revision: u64,
    pub lifecycle: AppCollectionLifecycle,
    /// Human-readable/raw backend label for presentation and diagnostics.
    pub status_label: String,
    pub capabilities: AppCollectionCapabilities,
    pub facts: AppCollectionFacts,
    pub members: Vec<AppMember>,
    pub entries: u32,
    pub total_bytes: u64,
    pub on_disk_bytes: u64,
    pub uploaded_bytes: u64,
    /// When bytes first moved, and when it finished. Unix nanoseconds.
    ///
    /// The core wrote each down as it happened. The interface used to work
    /// "completed in" out for itself by measuring the transfer history, which
    /// measures the ring rather than the transfer — after a delete and a
    /// re-add that read as one six-minute download instead of two of half a
    /// minute. A recorded moment cannot be re-measured into something else.
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub transfer: Option<AppTransfer>,
    pub pending: Option<AppPending>,
    pub publish_progress: Option<AppPublishProgress>,
}

/// One member named by the collection's signed current revision.
#[derive(Clone, Debug)]
pub struct AppMember {
    /// Durable signing-root fingerprint. This remains present when the member
    /// is not yet a known local contact.
    pub fingerprint: String,
    /// Process-local contact handle when this device knows the person.
    pub contact: Option<u32>,
}

/// A coalesced transfer sample.
#[derive(Clone, Debug)]
pub struct AppTransfer {
    pub progress: f32,
    pub source_reading: bool,
    pub down_bytes_per_second: u32,
    pub up_bytes_per_second: u32,
    pub peers: u16,
    pub eta_secs: Option<u32>,
}

/// A locally accepted command that has not settled yet.
#[derive(Clone, Debug)]
pub struct AppPending {
    pub command: u64,
    pub queued: bool,
}

/// Progress of hashing/creating a torrent for a collection this device is
/// publishing (owner side, before seeding begins). Distinct from
/// `AppTransfer`, which covers moving bytes over the wire — this covers the
/// local, network-free work of turning selected sources into a torrent.
#[derive(Clone, Debug)]
pub struct AppPublishProgress {
    /// Coarse phase label ("preparing", "hashing", "seeding", ...). Not
    /// meant for exhaustive matching — see `PublishProgressSnapshot::stage`.
    pub stage: String,
    pub processed_bytes: u64,
    pub total_bytes: u64,
    pub completed_pieces: u64,
    pub total_pieces: u64,
}

/// A receiver-side transfer's completion, as a typed fact rather than
/// something Flutter infers from diffing successive snapshots (ADR-0016).
/// Emitted exactly once per completion by the same poller that records
/// `completed_at` — see `nexus::core::transfers::follow_transfers`.
#[derive(Clone, Debug)]
pub struct AppTransferCompleted {
    pub collection: u32,
    pub name: String,
}

/// The opt-in, expensive projection for the collection currently on screen.
#[derive(Clone, Debug)]
pub struct AppDetail {
    pub id: u32,
    pub entries: Vec<AppEntry>,
    pub pieces: Vec<u8>,
    /// Swarm connections, which are not contacts. See `Detail::peers`.
    pub peers: Vec<AppPeer>,
}

/// One connected swarm peer.
///
/// A connection rather than a person: the address names a socket and `client`
/// is self-reported by the far end, so neither identifies anybody. Only the
/// byte counters and rates are this device's own measurements.
#[derive(Clone, Debug)]
pub struct AppPeer {
    pub address: String,
    /// What the peer calls itself, when it says. Untrusted by construction.
    pub client: Option<String>,
    pub down_bytes: u64,
    pub up_bytes: u64,
    pub down_bytes_per_second: u32,
    pub up_bytes_per_second: u32,
}

/// One swarm connection, and which collection it belongs to.
///
/// Paired rather than nested so the peers call answers in one flat list: the
/// same address may be connected for two collections, and those are two
/// separate connections rather than one peer with two names.
#[derive(Clone, Debug)]
pub struct AppCollectionPeer {
    pub collection: u32,
    pub peer: AppPeer,
}

/// One cumulative endpoint ledger returned only for the collection that asked
/// for it. The totals are backend-calculated across saved app sessions.
#[derive(Clone, Debug)]
pub struct AppPeerHistory {
    pub address: String,
    pub client: Option<String>,
    pub first_seen_at: u64,
    pub last_seen_at: u64,
    pub down_bytes: u64,
    pub up_bytes: u64,
    pub last_down_bytes_per_second: u32,
    pub last_up_bytes_per_second: u32,
    pub peak_down_bytes_per_second: u32,
    pub peak_up_bytes_per_second: u32,
}

/// One exact endpoint/client observation accumulated across collections.
#[derive(Clone, Debug)]
pub struct AppPeoplePeer {
    pub peer: AppPeer,
    pub collections: Vec<u32>,
    /// True only when this exact endpoint/client is in the current live peer
    /// snapshot. Historical non-zero rates must never imply liveness.
    pub live: bool,
    pub peak_down_bytes_per_second: u32,
    pub peak_up_bytes_per_second: u32,
    pub last_seen_at: u64,
}

/// A selectable media entry in a collection detail projection.
#[derive(Clone, Debug)]
pub struct AppEntry {
    pub id: u32,
    pub label: String,
    pub bytes: u64,
    pub selected: bool,
    pub available: bool,
    /// How much of this entry is here.
    ///
    /// Carried per entry because several files of one torrent download at
    /// once and finish at different times: without this the interface can only
    /// say "not here" or "here", and a multi-file torrent shows nothing at all
    /// until the last byte of the last file lands.
    pub downloaded_bytes: u64,
    /// Where the bytes landed, once they have, so the interface can show a
    /// preview rather than a filename.
    pub path: Option<String>,
}

/// A native source selected by the app without copying its bytes through the
/// bridge.
#[derive(Clone, Debug)]
pub struct AppSourceFile {
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

/// The local acceptance result returned before a command performs I/O.
#[derive(Clone, Debug)]
pub struct AppAccepted {
    pub id: u64,
    pub collection: Option<u32>,
    pub queued: bool,
}

static RUNTIME: OnceLock<Mutex<Option<Nexus>>> = OnceLock::new();

fn runtime() -> &'static Mutex<Option<Nexus>> {
    RUNTIME.get_or_init(|| Mutex::new(None))
}

/// Recovers from a poisoned lock rather than bricking every future call.
///
/// A `std::sync::Mutex` poisons permanently the instant any holder panics
/// while it was locked — and `RUNTIME` is a single process-wide `OnceLock`,
/// so poisoning it once means every FRB call for the rest of the process's
/// life returns "the Nexus runtime lock was poisoned" instead of doing
/// anything, with no way back short of restarting the app. The rest of this
/// codebase already treats a poisoned lock as recoverable (see
/// `activity.rs`/`nexus.rs`'s `PoisonError::into_inner`) on the reasoning
/// that a panicked writer left the data in whatever state it was in when it
/// panicked, not corrupted — the same reasoning applies here, and applies
/// more, since this is the one lock every single call goes through.
fn locked_runtime() -> Result<std::sync::MutexGuard<'static, Option<Nexus>>, String> {
    Ok(runtime()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner))
}

/// Opens the local Nexus runtime once. Calling it again is harmless.
///
/// Async because opening supervises tasks, and those have to be spawned onto a
/// runtime. flutter_rust_bridge runs a synchronous function on a worker thread
/// that has none, where `spawn` panics rather than returning something a person
/// could act on — the app reports "there is no reactor running" and stops.
/// Anything the core starts eagerly at open belongs behind an async boundary
/// for the same reason.
///
/// # Errors
///
/// Returns a displayable reason when the device identity or local store cannot
/// be opened.
pub async fn start() -> Result<(), String> {
    crate::nexus::log::clog!("api", "start");
    let mut runtime = locked_runtime()?;
    if runtime.is_none() {
        *runtime = Some(Nexus::open_default().map_err(|error| error.to_string())?);
    }
    Ok(())
}

/// Stops the runtime and waits for its bounded shutdown.
pub async fn stop() -> Result<(), String> {
    crate::nexus::log::clog!("api", "stop");
    let nexus = locked_runtime()?.take();
    if let Some(nexus) = nexus {
        nexus.close().await;
    }
    Ok(())
}

/// Tells the runtime whether the app is foreground-active.
pub fn set_active(active: bool) -> Result<(), String> {
    let mut runtime = locked_runtime()?;
    let nexus = runtime
        .as_mut()
        .ok_or_else(|| "start Nexus before changing its lifecycle state".to_owned())?;
    nexus.set_active(active);
    Ok(())
}

/// Renames this device. Updates the persisted identity and the live
/// `AppSnapshot.device` in one call — see [`AppUserSummary`] and
/// `Nexus::rename_device` for why a separate identity path used to drift.
///
/// # Errors
/// Returns a displayable reason when the runtime is not started or the
/// persisted identity cannot be updated.
pub fn rename_device(nickname: String) -> Result<(), String> {
    let mut runtime = locked_runtime()?;
    let nexus = runtime
        .as_mut()
        .ok_or_else(|| "start Nexus before renaming this device".to_owned())?;
    nexus
        .rename_device(nickname)
        .map_err(|error| error.to_string())
}

/// The complete local diagnostics log, oldest line first — the same lines
/// every [`crate::nexus::log::clog!`] call already writes to stderr,
/// additionally kept in a bounded file so they survive after the console is
/// gone.
///
/// This never leaves the device on its own: it is read here only so the app
/// can show it, and sharing it anywhere is the person's own choice from the
/// Diagnostics screen — no telemetry, no server, no account.
///
/// # Errors
/// Returns a description when the log cannot be read. An empty result (no
/// error) is the normal case before anything has been logged yet.
pub fn diagnostics_log() -> Result<String, String> {
    crate::nexus::diagnostics::read()
}

/// Deletes everything logged so far. Nothing in Portalis calls this except
/// a person tapping "Clear" on the Diagnostics screen.
///
/// # Errors
/// Returns a description when the file exists but cannot be removed.
pub fn clear_diagnostics_log() -> Result<(), String> {
    crate::nexus::diagnostics::clear()
}

/// Where the diagnostics log lives on disk, so the Diagnostics screen can
/// show a person exactly what file they would be sharing.
pub fn diagnostics_log_path() -> String {
    crate::nexus::diagnostics::path()
}

/// Appends one line to the same local diagnostics log every
/// [`crate::nexus::log::clog!`] call writes to — the Flutter-side counterpart,
/// so a Dart error caught by `FlutterError.onError` or
/// `PlatformDispatcher.onError` lands in the one report a person shares,
/// not silently on a console nobody is attached to.
pub fn log_diagnostic(tag: String, message: String) {
    crate::nexus::log::clog!(&tag, "{message}");
}

/// One bounded recent local backend run.
#[derive(Clone, Debug)]
pub struct AppAppRun {
    pub run_id: u64,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub engine_running_ns: u64,
    pub foreground_ns: u64,
    pub network_down_bytes: u64,
    pub network_up_bytes: u64,
    pub completed_downloads: u64,
    pub peak_down_bytes_per_second: u32,
    pub peak_up_bytes_per_second: u32,
    /// One of `"current"`, `"graceful"`, `"interrupted"`.
    pub end_reason: String,
}

/// This device's own locally measured activity. Never leaves the device on
/// its own, never contains collection names, paths, peer endpoints, or
/// signing material.
#[derive(Clone, Debug)]
pub struct AppUserSummary {
    pub device: AppDevice,
    pub tracked_since: u64,
    pub current_run: AppAppRun,
    pub runs_started: u64,
    pub runs_completed_cleanly: u64,
    pub runs_interrupted: u64,
    pub lifetime_engine_running_ns: u64,
    pub lifetime_foreground_ns: u64,
    pub lifetime_network_down_bytes: u64,
    pub lifetime_network_up_bytes: u64,
    pub lifetime_completed_downloads: u64,
    pub lifetime_peak_down_bytes_per_second: u32,
    pub lifetime_peak_up_bytes_per_second: u32,
    pub last_activity_at: u64,
    pub last_clean_shutdown_at: u64,
    pub collections_owned: u32,
    pub collections_received: u32,
    pub entries_total: u32,
    pub catalog_bytes: u64,
    pub held_bytes: u64,
    pub verified_contacts: u32,
    pub unverified_contacts: u32,
    pub connectivity: String,
    pub recent_runs: Vec<AppAppRun>,
}

fn app_run(run: &crate::nexus::store::records::StoredAppRun) -> AppAppRun {
    use crate::nexus::store::records::AppRunEnd;

    AppAppRun {
        run_id: run.run_id,
        started_at: run.started_at,
        ended_at: run.ended_at,
        engine_running_ns: run.engine_running_ns,
        foreground_ns: run.foreground_ns,
        network_down_bytes: run.network_down_bytes,
        network_up_bytes: run.network_up_bytes,
        completed_downloads: run.completed_downloads,
        peak_down_bytes_per_second: run.peak_down_bytes_per_second,
        peak_up_bytes_per_second: run.peak_up_bytes_per_second,
        end_reason: match run.end_reason {
            AppRunEnd::Current => "current",
            AppRunEnd::Graceful => "graceful",
            AppRunEnd::Interrupted => "interrupted",
        }
        .to_owned(),
    }
}

/// This device's own locally measured activity: current run, lifetime
/// counters, library facts, and bounded recent runs. On-demand and
/// low-rate, deliberately separate from the fast `AppSnapshot` stream.
///
/// # Errors
/// Returns a displayable reason when the runtime is not started or the
/// durable ledger cannot be read.
pub fn user_summary() -> Result<AppUserSummary, String> {
    use crate::nexus::projection::state::Role;

    let runtime = locked_runtime()?;
    let nexus = runtime
        .as_ref()
        .ok_or_else(|| "start Nexus before reading the user summary".to_owned())?;
    let snapshot = nexus
        .activity_summary()
        .map_err(|error| error.to_string())?;
    let state = nexus.state();

    let mut collections_owned = 0u32;
    let mut collections_received = 0u32;
    let mut entries_total = 0u32;
    let mut catalog_bytes = 0u64;
    let mut held_bytes = 0u64;
    let mut verified_contacts = 0u32;
    let mut unverified_contacts = 0u32;
    for collection in &state.collections {
        match collection.role {
            Role::Owner => collections_owned += 1,
            Role::Member => collections_received += 1,
        }
        entries_total += collection.entries;
        catalog_bytes += collection.total_bytes;
        held_bytes += collection.on_disk_bytes;
    }
    for contact in &state.contacts {
        if contact.verified {
            verified_contacts += 1;
        } else {
            unverified_contacts += 1;
        }
    }

    Ok(AppUserSummary {
        device: AppDevice {
            name: state.device.name,
            handle: state.device.handle,
            fingerprint: state.device.fingerprint,
            devices: state.device.devices,
        },
        tracked_since: snapshot.activity.stats_started_at,
        current_run: app_run(&snapshot.run),
        runs_started: snapshot.activity.runs_started,
        runs_completed_cleanly: snapshot.activity.runs_completed_cleanly,
        runs_interrupted: snapshot.activity.runs_interrupted,
        lifetime_engine_running_ns: snapshot.activity.engine_running_ns,
        lifetime_foreground_ns: snapshot.activity.foreground_ns,
        lifetime_network_down_bytes: snapshot.activity.total_network_down_bytes,
        lifetime_network_up_bytes: snapshot.activity.total_network_up_bytes,
        lifetime_completed_downloads: snapshot.activity.completed_downloads,
        lifetime_peak_down_bytes_per_second: snapshot.activity.peak_down_bytes_per_second,
        lifetime_peak_up_bytes_per_second: snapshot.activity.peak_up_bytes_per_second,
        last_activity_at: snapshot.activity.last_activity_at,
        last_clean_shutdown_at: snapshot.activity.last_clean_shutdown_at,
        collections_owned,
        collections_received,
        entries_total,
        catalog_bytes,
        held_bytes,
        verified_contacts,
        unverified_contacts,
        connectivity: format!("{:?}", state.connectivity),
        recent_runs: snapshot.recent_runs.iter().map(app_run).collect(),
    })
}

/// Clears only durable device activity and bounded run history. Identity,
/// collections, and settings are never touched.
///
/// # Errors
/// Returns a displayable reason when the runtime is not started or the
/// store transaction fails.
pub fn clear_user_activity() -> Result<(), String> {
    locked_runtime()?
        .as_ref()
        .ok_or_else(|| "start Nexus before clearing activity".to_owned())?
        .clear_activity()
        .map_err(|error| error.to_string())
}

/// Streams complete app snapshots. The current state is sent first.
pub async fn watch_states(sink: StreamSink<AppSnapshot>) -> Result<(), String> {
    let mut states = locked_runtime()?
        .as_ref()
        .ok_or_else(|| "start Nexus before subscribing to state".to_owned())?
        .watch();

    loop {
        // See `watch_detail`: a subscriber that has gone is not an error.
        if sink.add(snapshot(&states.borrow())).is_err() {
            return Ok(());
        }
        if states.changed().await.is_err() {
            return Ok(());
        }
    }
}

/// Streams a typed fact each time a receiver-side transfer completes,
/// instead of Flutter inferring completion by diffing successive
/// `AppSnapshot`s itself (ADR-0016). Only `TransferSettled { ok: true }`
/// durable events are forwarded; the collection's current name is read from
/// the live state at the moment the event arrives, so a rename shortly
/// before completion is reflected rather than a stale name captured earlier.
pub async fn watch_transfer_completions(
    sink: StreamSink<AppTransferCompleted>,
) -> Result<(), String> {
    let (bus, states) = {
        let runtime = locked_runtime()?;
        let nexus = runtime
            .as_ref()
            .ok_or_else(|| "start Nexus before subscribing to completions".to_owned())?;
        (nexus.events_bus(), nexus.watch())
    };
    let mut events = bus.subscribe().await;

    loop {
        let Some(event) = events.next().await else {
            return Ok(());
        };
        let crate::nexus::core::events::Event::TransferSettled { collection, ok } = event else {
            continue;
        };
        if !ok {
            continue;
        }
        let handle = Handle(u32::try_from(collection.0).unwrap_or(u32::MAX));
        let name = states
            .borrow()
            .collections
            .iter()
            .find(|item| item.id == handle)
            .map(|item| item.name.clone());
        let Some(name) = name else {
            // The collection is gone by the time this event was processed
            // (deleted between the poller emitting it and this loop turn).
            // Nothing to notify about — never invent a name.
            continue;
        };
        // A closed sink is how a subscription ends, not a failure to report.
        if sink
            .add(AppTransferCompleted {
                collection: handle.0,
                name,
            })
            .is_err()
        {
            return Ok(());
        }
    }
}

/// Streams the detail tier for one collection, or `None` after unsubscribing.
pub async fn watch_detail(
    collection: Option<u32>,
    sink: StreamSink<Option<AppDetail>>,
) -> Result<(), String> {
    let mut detail = locked_runtime()?
        .as_ref()
        .ok_or_else(|| "start Nexus before subscribing to collection detail".to_owned())?
        .watch_detail(collection.map(Handle));

    loop {
        // A closed sink is how a subscription ends, not a failure to report:
        // the screen went away. Reporting it surfaced an alarming unhandled
        // exception in Flutter for the ordinary act of closing a collection.
        if sink
            .add(detail.borrow().as_ref().map(detail_projection))
            .is_err()
        {
            return Ok(());
        }
        if detail.changed().await.is_err() {
            return Ok(());
        }
    }
}

/// Every live swarm connection this device has, across all collections.
///
/// Its own call rather than a snapshot field: peers change every poll while
/// the rest of a snapshot does not, and carrying them in the summary tier
/// would push a per-second rewrite through every screen that renders a
/// collection list. Asked for by the one screen that shows them.
pub fn peers() -> Result<Vec<AppCollectionPeer>, String> {
    Ok(locked_runtime()?
        .as_ref()
        .ok_or_else(|| "start Nexus before listing peers".to_owned())?
        .peers()
        .into_iter()
        .map(|(collection, peer)| AppCollectionPeer {
            collection: collection.0,
            peer: AppPeer {
                address: peer.address,
                client: peer.client,
                down_bytes: peer.down_bytes,
                up_bytes: peer.up_bytes,
                down_bytes_per_second: peer.down_bytes_per_second,
                up_bytes_per_second: peer.up_bytes_per_second,
            },
        })
        .collect())
}

/// The selected collection's cumulative peer ledger. This is an on-demand
/// history tier, deliberately separate from live peer polling and snapshots.
pub fn peer_history(collection: u32) -> Result<Vec<AppPeerHistory>, String> {
    Ok(locked_runtime()?
        .as_ref()
        .ok_or_else(|| "start Nexus before reading peer history".to_owned())?
        .peer_history(crate::nexus::projection::state::Handle(collection))
        .into_iter()
        .map(|peer| AppPeerHistory {
            address: peer.address,
            client: peer.client,
            first_seen_at: peer.first_seen_at,
            last_seen_at: peer.last_seen_at,
            down_bytes: peer.total_down_bytes,
            up_bytes: peer.total_up_bytes,
            last_down_bytes_per_second: peer.last_down_bytes_per_second,
            last_up_bytes_per_second: peer.last_up_bytes_per_second,
            peak_down_bytes_per_second: peer.peak_down_bytes_per_second,
            peak_up_bytes_per_second: peer.peak_up_bytes_per_second,
        })
        .collect())
}

/// Backend-owned People projection. Saved history is replaced by the same
/// collection's effective live observation before endpoint/client grouping.
pub fn people_peers() -> Result<Vec<AppPeoplePeer>, String> {
    let runtime = locked_runtime()?;
    let nexus = runtime
        .as_ref()
        .ok_or_else(|| "start Nexus before listing peers".to_owned())?;
    let history = nexus
        .state()
        .collections
        .into_iter()
        .flat_map(|collection| {
            nexus
                .peer_history(collection.id)
                .into_iter()
                .map(move |peer| (collection.id, peer))
        });
    Ok(group_people_peers(history, nexus.peers()))
}

fn group_people_peers(
    history: impl IntoIterator<Item = (Handle, crate::nexus::store::records::StoredPeerHistory)>,
    live: impl IntoIterator<Item = (Handle, crate::nexus::projection::state::PeerState)>,
) -> Vec<AppPeoplePeer> {
    use std::collections::BTreeMap;

    let mut rows = BTreeMap::new();
    let mut facts = BTreeMap::new();
    for (collection, peer) in history {
        let key = (collection, peer.address.clone(), peer.client.clone());
        facts.insert(
            key.clone(),
            (
                peer.peak_down_bytes_per_second,
                peer.peak_up_bytes_per_second,
                peer.last_seen_at,
                false,
            ),
        );
        rows.insert(
            key,
            AppPeer {
                address: peer.address,
                client: peer.client,
                down_bytes: peer.total_down_bytes,
                up_bytes: peer.total_up_bytes,
                // A saved rate is a historical sample, not what is happening
                // now. Peak is carried separately for historical rendering.
                down_bytes_per_second: 0,
                up_bytes_per_second: 0,
            },
        );
    }
    for (collection, peer) in live {
        let key = (collection, peer.address.clone(), peer.client.clone());
        let fact = facts.entry(key.clone()).or_insert((0, 0, 0, false));
        fact.0 = fact.0.max(peer.down_bytes_per_second);
        fact.1 = fact.1.max(peer.up_bytes_per_second);
        fact.3 = true;
        rows.insert(
            key,
            AppPeer {
                address: peer.address,
                client: peer.client,
                down_bytes: peer.down_bytes,
                up_bytes: peer.up_bytes,
                down_bytes_per_second: peer.down_bytes_per_second,
                up_bytes_per_second: peer.up_bytes_per_second,
            },
        );
    }
    let mut grouped = BTreeMap::<(String, Option<String>), AppPeoplePeer>::new();
    for ((collection, address, client), peer) in rows {
        let (peak_down, peak_up, last_seen_at, live) = facts
            .remove(&(collection, address.clone(), client.clone()))
            .unwrap_or_default();
        let entry = grouped
            .entry((address, client))
            .or_insert_with(|| AppPeoplePeer {
                peer: AppPeer {
                    address: peer.address.clone(),
                    client: peer.client.clone(),
                    down_bytes: 0,
                    up_bytes: 0,
                    down_bytes_per_second: 0,
                    up_bytes_per_second: 0,
                },
                collections: Vec::new(),
                live: false,
                peak_down_bytes_per_second: 0,
                peak_up_bytes_per_second: 0,
                last_seen_at: 0,
            });
        entry.peer.down_bytes += peer.down_bytes;
        entry.peer.up_bytes += peer.up_bytes;
        entry.peer.down_bytes_per_second += peer.down_bytes_per_second;
        entry.peer.up_bytes_per_second += peer.up_bytes_per_second;
        entry.live |= live;
        entry.peak_down_bytes_per_second = entry
            .peak_down_bytes_per_second
            .max(peak_down)
            .max(entry.peer.down_bytes_per_second);
        entry.peak_up_bytes_per_second = entry
            .peak_up_bytes_per_second
            .max(peak_up)
            .max(entry.peer.up_bytes_per_second);
        entry.last_seen_at = entry.last_seen_at.max(last_seen_at);
        entry.collections.push(collection.0);
    }
    grouped.into_values().collect()
}

/// What a scanned invitation says about a collection, before any network.
///
/// A magnet answers only "what content", so an import screen had nothing to
/// show until the swarm replied. This is the sending device's own description,
/// carried in the code itself, so the receiver can name the collection, lay out
/// placeholders for its entries, and warn about a code that cannot work here —
/// all before the first packet.
///
/// Every field is untrusted input from a scanned image. It is safe to display
/// and to size a placeholder grid with; it is not authorization, and the
/// content itself remains verified by info hash exactly as before.
pub struct AppInvitation {
    /// The collection's name on the sharing device.
    pub name: String,
    /// The sharing device's name.
    pub owner: String,
    /// How many entries the collection holds.
    pub entries: u32,
    /// Seconds since the Unix epoch at which the code was produced.
    pub issued_at_secs: u32,
    /// Whether any advertised endpoint sits on a network this device is also
    /// on. False means the code was almost certainly produced on another
    /// network — the single most common reason an in-person share stalls.
    pub reachable_here: bool,
}

/// Describes a scanned invitation without importing it.
///
/// Separate from `send` so the interface can show what a code contains — and
/// refuse an unusable one — before committing to a durable collection row.
/// Returns `None` for anything that is not a Portalis invitation, including a
/// plain magnet, which the import path still accepts directly.
pub fn describe_invitation(link: String) -> Option<AppInvitation> {
    let invitation = portalis_nexus_protocol::Invitation::decode(&link).ok()?;
    let reachable_here = invitation.shares_network_with(&crate::nexus::torrent::local_addresses());
    Some(AppInvitation {
        name: invitation.name,
        owner: invitation.owner,
        entries: invitation.entries,
        issued_at_secs: invitation.issued_at_secs,
        reachable_here,
    })
}

/// The collection's shareable invitation link, when the local substrate has a
/// real persisted info hash for it. Fetched on demand rather than added to
/// every snapshot because a QR is only useful on the screen that asked for it,
/// and because its peer hints are only true of the network this device is on
/// at the moment it is asked.
pub fn share_uri(collection: u32) -> Result<Option<String>, String> {
    crate::nexus::log::clog!("api", "share_uri collection={collection}");
    locked_runtime()?
        .as_ref()
        .ok_or_else(|| "start Nexus before sharing a collection".to_owned())?
        .share_uri(Handle(collection))
        .map_err(|error| error.to_string())
}

/// One collection's transfer history, as it happens.
///
/// Its own stream rather than a field of the detail, because it changes for a
/// different reason and at a different rate than everything else there. The
/// detail describes what a collection *is* right now — its files, its verified
/// pieces, who it is talking to — and all of that is replaced wholesale when
/// any of it moves. The history only ever grows at the end.
///
/// Carrying it inside the detail meant appending one eighteen-byte reading
/// re-read the entire ring from the store, re-packed up to thirty kilobytes,
/// and marshalled all of it across the bridge — once a second, for a screen
/// that was already showing every row of it.
///
/// The cursor lives here, in the loop, because it belongs to one subscriber
/// rather than to the collection: two screens watching the same collection
/// have each seen a different amount of it. The first message carries
/// everything; each one after carries only what arrived since.
pub async fn watch_history(collection: u32, sink: StreamSink<Vec<u8>>) -> Result<(), String> {
    let handle = Handle(collection);
    let mut held_through = 0_u64;
    loop {
        let rows = {
            let runtime = locked_runtime()?;
            let nexus = runtime
                .as_ref()
                .ok_or_else(|| "start Nexus before subscribing to history".to_owned())?;
            nexus.history_after(handle, held_through)
        };

        // Nothing new is the ordinary case once a transfer settles, and it
        // costs one range scan that returns immediately. Saying nothing is
        // cheaper than saying the same thing again.
        if let Some((newest, packed)) = rows {
            held_through = newest;
            // A closed sink is how a subscription ends, not a failure to
            // report: the screen went away.
            if sink.add(packed).is_err() {
                return Ok(());
            }
        }
        tokio::time::sleep(crate::nexus::core::transfers::POLL_INTERVAL).await;
    }
}

/// One directory under the download folder, traced back to the collection
/// that owns it when Nexus still claims one.
#[derive(Clone, Debug)]
pub struct AppStorageEntry {
    pub name: String,
    pub bytes: u64,
    pub path: String,
    /// The owning collection's handle, when one claims it. Absent for the
    /// usual case: leftovers of a collection that has been deleted.
    pub collection: Option<u32>,
    pub collection_name: Option<String>,
}

/// What is on disk under the download directory, resolved against Nexus.
///
/// What is on disk under the download directory, resolved against Nexus.
///
/// Ownership is decided by the substrate handle a collection recorded when
/// its download started, matched to the directory the engine reports for
/// that torrent — not by name, which two collections may share.
///
/// # Errors
///
/// Returns a displayable reason when the directory cannot be walked.
pub async fn storage_breakdown() -> Result<Vec<AppStorageEntry>, String> {
    let raw = crate::nexus::torrent::storage_breakdown()
        .await
        .map_err(|error| error.to_string())?;
    let holdings = crate::nexus::substrate::current().holdings().await;

    // Where each carried torrent's files actually sit, so a directory can be
    // traced back to it. `starts_with` rather than equality: a multi-file
    // torrent nests its files below the folder it was given.
    let by_path: Vec<(std::path::PathBuf, &str)> = holdings
        .iter()
        .filter_map(|info| {
            info.files.first().map(|file| {
                (
                    std::path::PathBuf::from(&file.absolute_path),
                    info.info_hash.as_str(),
                )
            })
        })
        .collect();

    let owners = {
        let runtime = locked_runtime()?;
        let nexus = runtime
            .as_ref()
            .ok_or_else(|| "start Nexus before reading storage".to_owned())?;
        nexus.carried_collections()
    };

    Ok(raw
        .into_iter()
        .map(|entry| {
            let path = std::path::Path::new(&entry.path);
            let owner = by_path
                .iter()
                .find(|(files, _)| files.starts_with(path))
                .and_then(|(_, hash)| {
                    owners
                        .iter()
                        .find(|(_, _, handle)| handle == hash)
                        .map(|(id, name, _)| (*id, name.clone()))
                });
            AppStorageEntry {
                name: entry.name,
                bytes: entry.bytes,
                path: entry.path,
                collection: owner.as_ref().map(|(id, _)| id.0),
                collection_name: owner.map(|(_, name)| name),
            }
        })
        .collect())
}

/// Creates a private draft from opaque native source references.
pub fn create_collection(name: String, files: Vec<AppSourceFile>) -> Result<AppAccepted, String> {
    accept(
        "createCollection",
        None,
        files.len(),
        Command::CreateCollection {
            name,
            files: source_files(files),
        },
    )
}

/// Adds opaque native source references to an existing draft.
pub fn add_media(
    collection: u32,
    label: String,
    files: Vec<AppSourceFile>,
) -> Result<AppAccepted, String> {
    accept(
        "addMedia",
        Some(collection),
        files.len(),
        Command::AddMedia {
            collection: Handle(collection),
            label,
            files: source_files(files),
        },
    )
}

pub fn rename_collection(collection: u32, name: String) -> Result<AppAccepted, String> {
    accept(
        "renameCollection",
        Some(collection),
        0,
        Command::RenameCollection {
            collection: Handle(collection),
            name,
        },
    )
}

pub fn delete_collection(collection: u32, delete_files: bool) -> Result<AppAccepted, String> {
    accept(
        "deleteCollection",
        Some(collection),
        0,
        Command::DeleteCollection {
            collection: Handle(collection),
            delete_files,
        },
    )
}

pub fn set_collection_paused(collection: u32, paused: bool) -> Result<AppAccepted, String> {
    accept(
        "setPaused",
        Some(collection),
        0,
        Command::SetPaused {
            collection: Handle(collection),
            paused,
        },
    )
}

pub fn publish_draft(collection: u32) -> Result<AppAccepted, String> {
    accept(
        "publishDraft",
        Some(collection),
        0,
        Command::PublishDraft {
            collection: Handle(collection),
        },
    )
}

pub fn import_torrent(source: String) -> Result<AppAccepted, String> {
    accept("importTorrent", None, 0, Command::ImportTorrent { source })
}

pub fn download_selection(collection: u32, entries: Vec<u32>) -> Result<AppAccepted, String> {
    accept(
        "downloadSelection",
        Some(collection),
        entries.len(),
        Command::DownloadSelection {
            collection: Handle(collection),
            entries: entries.into_iter().map(Handle).collect(),
        },
    )
}

fn source_files(files: Vec<AppSourceFile>) -> Vec<LocalFile> {
    files
        .into_iter()
        .map(|file| LocalFile {
            name: file.name,
            path: PathBuf::from(file.path),
            bytes: file.bytes,
        })
        .collect()
}

/// Validates and accepts one already-well-formed command without waiting for
/// its I/O. Public bridge functions above own the command shape; this helper
/// owns the one runtime acceptance path.
fn accept(
    kind: &'static str,
    collection: Option<u32>,
    entries: usize,
    command: Command,
) -> Result<AppAccepted, String> {
    crate::nexus::log::clog!(
        "api",
        "send kind={} collection={:?} entries={}",
        kind,
        collection,
        entries
    );
    let runtime = locked_runtime()?;
    let accepted = runtime
        .as_ref()
        .ok_or_else(|| "start Nexus before sending a command".to_owned())?
        .command(&command)
        .map_err(|error| error.to_string())?;
    let accepted = AppAccepted {
        id: accepted.id,
        collection: accepted.collection.map(|handle| handle.0),
        queued: accepted.queued,
    };
    crate::nexus::log::clog!(
        "api",
        "accepted id={} collection={:?} queued={}",
        accepted.id,
        accepted.collection,
        accepted.queued
    );
    Ok(accepted)
}

fn snapshot(state: &PortalisState) -> AppSnapshot {
    AppSnapshot {
        device: AppDevice {
            name: state.device.name.clone(),
            handle: state.device.handle.clone(),
            fingerprint: state.device.fingerprint.clone(),
            devices: state.device.devices,
        },
        connectivity: state.connectivity.wire().to_owned(),
        contacts: state
            .contacts
            .iter()
            .map(|contact| AppContact {
                id: contact.id.0,
                display_name: contact.display_name.clone(),
                handle: contact.handle.clone(),
                fingerprint: contact.fingerprint.clone(),
                verified: contact.verified,
                friendship: contact.friendship.wire().to_owned(),
                reachable: contact.reachable.map(|security| format!("{security:?}")),
            })
            .collect(),
        collections: state
            .collections
            .iter()
            .map(collection_projection)
            .collect(),
        alerts: state
            .alerts
            .iter()
            .map(|alert| format!("{alert:?}"))
            .collect(),
        activity: app_activity(&state.collections),
    }
}

/// What the engine is doing right now, aggregated across every collection.
///
/// One derivation, so no two screens can disagree about how many transfers
/// are active — see [`AppActivity`]'s own doc for the bug this replaced.
fn app_activity(collections: &[crate::nexus::projection::state::CollectionState]) -> AppActivity {
    let mut activity = AppActivity {
        transfers: 0,
        down_bytes_per_second: 0,
        up_bytes_per_second: 0,
        peers: 0,
    };
    for collection in collections {
        let Some(transfer) = collection.transfer else {
            continue;
        };
        activity.transfers += 1;
        activity.down_bytes_per_second += transfer.down_bytes_per_second;
        activity.up_bytes_per_second += transfer.up_bytes_per_second;
        activity.peers += u32::from(transfer.peers);
    }
    activity
}

fn app_collection_lifecycle(
    status: crate::nexus::projection::state::Status,
) -> AppCollectionLifecycle {
    use crate::nexus::projection::state::Status;
    match status {
        Status::Available => AppCollectionLifecycle::Available,
        Status::Seeding => AppCollectionLifecycle::Seeding,
        Status::Paused => AppCollectionLifecycle::Paused,
        Status::Draft => AppCollectionLifecycle::Draft,
        Status::ResolvingMetadata => AppCollectionLifecycle::ResolvingMetadata,
        Status::WaitingForSender => AppCollectionLifecycle::WaitingForSender,
        Status::MetadataReady => AppCollectionLifecycle::MetadataReady,
        Status::DownloadRequested => AppCollectionLifecycle::DownloadRequested,
        Status::RetryingMetadata => AppCollectionLifecycle::RetryingMetadata,
        Status::Downloading => AppCollectionLifecycle::Downloading,
        Status::Updating => AppCollectionLifecycle::Updating,
        Status::WaitingForOwner => AppCollectionLifecycle::WaitingForOwner,
        Status::AccessRemoved => AppCollectionLifecycle::AccessRemoved,
        Status::NeedsNewerVersion => AppCollectionLifecycle::NeedsNewerVersion,
        Status::CannotVerify(_) => AppCollectionLifecycle::CannotVerify,
        Status::ConflictingHistory => AppCollectionLifecycle::ConflictingHistory,
    }
}

fn app_collection_contract(
    collection: &crate::nexus::projection::state::CollectionState,
) -> (AppCollectionCapabilities, AppCollectionFacts) {
    use crate::nexus::projection::state::{Nature, Status};

    let complete = matches!(collection.status, Status::Available | Status::Seeding);
    let preparing = matches!(
        collection.status,
        Status::ResolvingMetadata | Status::RetryingMetadata | Status::WaitingForSender
    );
    let moving = matches!(collection.status, Status::Downloading)
        || collection.transfer.is_some_and(|transfer| {
            transfer.down_bytes_per_second > 0 || transfer.up_bytes_per_second > 0
        });
    let progress = collection.transfer.map_or_else(
        || {
            if complete {
                1.0
            } else if collection.total_bytes == 0 {
                0.0
            } else {
                #[allow(clippy::cast_precision_loss, reason = "UI progress fraction")]
                {
                    (collection.on_disk_bytes as f32 / collection.total_bytes as f32)
                        .clamp(0.0, 1.0)
                }
            }
        },
        |transfer| transfer.progress.clamp(0.0, 1.0),
    );
    let capabilities = AppCollectionCapabilities {
        can_add_media: collection.nature == Nature::Native && collection.status == Status::Draft,
        can_select: collection.nature == Nature::Torrent
            && collection.status == Status::MetadataReady,
        can_share: collection.status != Status::Draft
            && (collection.entries > 0 || collection.revision > 0)
            // A status-only check answered "shareable" the instant a
            // restart's rehydration set the durable status, before the
            // engine had actually unpaused/loaded the torrent — producing a
            // visible button whose tap answered "Not ready to share". This
            // is the live engine's own fact, refreshed every transfer-poll
            // tick, so the button is only ever present when a share attempt
            // will actually succeed.
            && collection.share_ready,
        can_pause: matches!(
            collection.status,
            Status::Available
                | Status::Seeding
                | Status::DownloadRequested
                | Status::Downloading
                | Status::Updating
        ),
        can_resume: collection.status == Status::Paused,
        can_delete: true,
        can_delete_files: collection.on_disk_bytes > 0,
    };
    let facts = AppCollectionFacts {
        complete,
        sharing: complete && collection.entries > 0,
        moving,
        preparing,
        progress,
    };
    (capabilities, facts)
}

/// One collection, as the app reads it.
///
/// Split out so the contract check has somewhere to live that is not four
/// levels deep inside an iterator chain.
fn collection_projection(
    collection: &crate::nexus::projection::state::CollectionState,
) -> AppCollection {
    use crate::nexus::projection::state::Status;

    let status = collection.status.wire();
    // A status Flutter cannot match is a collection it answers `false` about
    // to every question — no start button, no progress, no label. Cheap to
    // check, and the one place the contract can be caught drifting at runtime.
    // `CannotVerify` is deliberately exempt: it deals its reason out of the
    // word, so it is emitted but not parsed back.
    debug_assert!(
        crate::nexus::projection::wire::emits::<Status>(status)
            || matches!(collection.status, Status::CannotVerify(_)),
        "status {status:?} is not a word the app contract knows"
    );

    let (capabilities, facts) = app_collection_contract(collection);
    AppCollection {
        id: collection.id.0,
        name: collection.name.clone(),
        nature: match collection.nature {
            crate::nexus::projection::state::Nature::Native => AppCollectionNature::Native,
            crate::nexus::projection::state::Nature::Torrent => AppCollectionNature::Torrent,
        },
        role: match collection.role {
            crate::nexus::projection::state::Role::Owner => AppCollectionRole::Owner,
            crate::nexus::projection::state::Role::Member => AppCollectionRole::Member,
        },
        revision: collection.revision,
        lifecycle: app_collection_lifecycle(collection.status),
        status_label: status.to_owned(),
        capabilities,
        facts,
        members: collection
            .members
            .iter()
            .map(|member| AppMember {
                fingerprint: hex::encode(member.root_key),
                contact: member.contact.map(|contact| contact.0),
            })
            .collect(),
        entries: collection.entries,
        total_bytes: collection.total_bytes,
        on_disk_bytes: collection.on_disk_bytes,
        uploaded_bytes: collection.uploaded_bytes,
        started_at: collection.started_at,
        completed_at: collection.completed_at,
        transfer: collection.transfer.map(|transfer| AppTransfer {
            progress: transfer.progress,
            source_reading: transfer.source_reading,
            down_bytes_per_second: transfer.down_bytes_per_second,
            up_bytes_per_second: transfer.up_bytes_per_second,
            peers: transfer.peers,
            eta_secs: transfer.eta_secs,
        }),
        pending: collection.pending.map(|pending| AppPending {
            command: pending.command,
            queued: pending.queued,
        }),
        publish_progress: collection
            .publish_progress
            .as_ref()
            .map(|progress| AppPublishProgress {
                stage: progress.stage.clone(),
                processed_bytes: progress.processed_bytes,
                total_bytes: progress.total_bytes,
                completed_pieces: progress.completed_pieces,
                total_pieces: progress.total_pieces,
            }),
    }
}

fn detail_projection(detail: &Detail) -> AppDetail {
    AppDetail {
        id: detail.id.0,
        entries: detail
            .entries
            .iter()
            .map(|entry| AppEntry {
                id: entry.id.0,
                label: entry.label.clone(),
                bytes: entry.bytes,
                selected: entry.selected,
                available: entry.available,
                downloaded_bytes: entry.downloaded_bytes,
                path: entry.path.clone(),
            })
            .collect(),
        pieces: detail.pieces.clone(),
        peers: detail
            .peers
            .iter()
            .map(|peer| AppPeer {
                address: peer.address.clone(),
                client: peer.client.clone(),
                down_bytes: peer.down_bytes,
                up_bytes: peer.up_bytes,
                down_bytes_per_second: peer.down_bytes_per_second,
                up_bytes_per_second: peer.up_bytes_per_second,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nexus::projection::state::{
        CollectionState, Connectivity, DeviceState, MemberState, Nature, Role, Status,
    };
    use crate::nexus::store::records::StoredPeerHistory;

    #[test]
    fn maps_source_metadata_without_moving_media_through_the_bridge() {
        assert_eq!(
            source_files(vec![AppSourceFile {
                name: "Episode 1.mp4".to_owned(),
                path: "phasset://native-identifier".to_owned(),
                bytes: 42,
            }]),
            vec![LocalFile {
                name: "Episode 1.mp4".to_owned(),
                path: PathBuf::from("phasset://native-identifier"),
                bytes: 42,
            }]
        );
    }

    #[test]
    fn snapshot_is_complete_and_uses_bridge_handles() {
        let projection = PortalisState {
            device: DeviceState {
                name: "Mina's Mac".to_owned(),
                handle: Some("mina#12345".to_owned()),
                fingerprint: "abcd".to_owned(),
                devices: 2,
            },
            connectivity: Connectivity::LocalOnly,
            contacts: Vec::new(),
            collections: Vec::new(),
            alerts: Vec::new(),
        };

        let app = snapshot(&projection);
        assert_eq!(app.device.name, "Mina's Mac");
        assert_eq!(app.connectivity, "LocalOnly");
        assert!(app.collections.is_empty());
        assert_eq!(
            app.activity,
            AppActivity {
                transfers: 0,
                down_bytes_per_second: 0,
                up_bytes_per_second: 0,
                peers: 0,
            }
        );
    }

    #[test]
    fn snapshot_activity_aggregates_only_moving_collections() {
        let moving = CollectionState {
            id: Handle(1),
            name: "Moving".to_owned(),
            nature: Nature::Torrent,
            role: Role::Owner,
            revision: 1,
            status: Status::Downloading,
            members: Vec::new(),
            entries: 1,
            total_bytes: 100,
            on_disk_bytes: 50,
            uploaded_bytes: 0,
            started_at: None,
            completed_at: None,
            transfer: Some(crate::nexus::projection::state::Transfer {
                progress: 0.5,
                source_reading: false,
                down_bytes_per_second: 125_000,
                up_bytes_per_second: 250_000,
                peers: 3,
                eta_secs: Some(4),
            }),
            pending: None,
            publish_progress: None,
            share_ready: false,
        };
        let idle = CollectionState {
            id: Handle(2),
            name: "Idle".to_owned(),
            nature: Nature::Native,
            role: Role::Owner,
            revision: 1,
            status: Status::Available,
            members: Vec::new(),
            entries: 1,
            total_bytes: 10,
            on_disk_bytes: 10,
            uploaded_bytes: 0,
            started_at: None,
            completed_at: None,
            transfer: None,
            pending: None,
            publish_progress: None,
            share_ready: false,
        };
        let projection = PortalisState {
            device: DeviceState {
                name: "Ada's laptop".to_owned(),
                handle: None,
                fingerprint: "aaaa".to_owned(),
                devices: 1,
            },
            connectivity: Connectivity::LocalOnly,
            contacts: Vec::new(),
            collections: vec![idle, moving],
            alerts: Vec::new(),
        };

        let app = snapshot(&projection);
        assert_eq!(
            app.activity,
            AppActivity {
                transfers: 1,
                down_bytes_per_second: 125_000,
                up_bytes_per_second: 250_000,
                peers: 3,
            },
            "the idle collection contributes nothing; only the moving one counts"
        );
    }

    #[test]
    fn collection_bridge_preserves_known_and_unknown_signed_members() {
        let collection = CollectionState {
            id: Handle(9),
            name: "Shared archive".to_owned(),
            nature: Nature::Native,
            role: Role::Member,
            revision: 4,
            status: Status::Available,
            members: vec![
                MemberState {
                    root_key: [0x11; portalis_nexus_protocol::DEVICE_KEY_BYTES],
                    contact: Some(Handle(7)),
                },
                MemberState {
                    root_key: [0x22; portalis_nexus_protocol::DEVICE_KEY_BYTES],
                    contact: None,
                },
            ],
            entries: 0,
            total_bytes: 0,
            on_disk_bytes: 0,
            uploaded_bytes: 0,
            started_at: None,
            completed_at: None,
            transfer: None,
            pending: None,
            publish_progress: None,
            share_ready: true,
        };

        let app = collection_projection(&collection);
        assert_eq!(app.members.len(), 2);
        assert_eq!(app.members[0].contact, Some(7));
        assert_eq!(app.members[0].fingerprint, "11".repeat(32));
        assert_eq!(app.members[1].contact, None);
        assert_eq!(app.members[1].fingerprint, "22".repeat(32));
        assert_eq!(app.lifecycle, AppCollectionLifecycle::Available);
        assert_eq!(app.nature, AppCollectionNature::Native);
        assert_eq!(app.role, AppCollectionRole::Member);
        assert!(app.facts.complete);
        assert!(!app.facts.preparing);
        assert!(app.capabilities.can_share);
        assert!(app.capabilities.can_delete);
    }

    #[test]
    fn metadata_resolution_is_a_typed_preparing_fact() {
        let resolving = CollectionState {
            id: Handle(12),
            name: "Scanned collection".to_owned(),
            nature: Nature::Torrent,
            role: Role::Owner,
            revision: 0,
            status: Status::ResolvingMetadata,
            members: Vec::new(),
            entries: 0,
            total_bytes: 0,
            on_disk_bytes: 0,
            uploaded_bytes: 0,
            started_at: None,
            completed_at: None,
            transfer: None,
            pending: None,
            publish_progress: None,
            share_ready: false,
        };

        let app = collection_projection(&resolving);
        assert_eq!(app.lifecycle, AppCollectionLifecycle::ResolvingMetadata);
        assert_eq!(app.nature, AppCollectionNature::Torrent);
        assert!(app.facts.preparing);
        assert!(!app.facts.complete);
        assert!(!app.capabilities.can_share);
        assert!(!app.capabilities.can_pause);
    }

    #[test]
    fn remembered_peer_rates_are_not_presented_as_live_people_rates() {
        let peers = group_people_peers(
            [(
                Handle(7),
                StoredPeerHistory {
                    address: "203.0.113.5:6881".to_owned(),
                    client: Some("qBittorrent 4.6".to_owned()),
                    first_seen_at: 1,
                    last_seen_at: 2,
                    total_down_bytes: 4_000_000,
                    total_up_bytes: 1_000_000,
                    checkpoint_down_bytes: 4_000_000,
                    checkpoint_up_bytes: 1_000_000,
                    checkpoint_epoch: 9,
                    last_down_bytes_per_second: 512_000,
                    last_up_bytes_per_second: 64_000,
                    peak_down_bytes_per_second: 900_000,
                    peak_up_bytes_per_second: 100_000,
                },
            )],
            std::iter::empty(),
        );

        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].collections, vec![7]);
        assert_eq!(peers[0].peer.down_bytes_per_second, 0);
        assert_eq!(peers[0].peer.up_bytes_per_second, 0);
        assert!(!peers[0].live);
        assert_eq!(peers[0].peak_down_bytes_per_second, 900_000);
        assert_eq!(peers[0].peak_up_bytes_per_second, 100_000);
        assert_eq!(peers[0].last_seen_at, 2);
    }
}
