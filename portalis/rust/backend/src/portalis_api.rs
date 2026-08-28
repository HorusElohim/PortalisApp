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

/// One collection in the inexpensive list projection.
#[derive(Clone, Debug)]
pub struct AppCollection {
    pub id: u32,
    pub name: String,
    pub nature: String,
    pub role: String,
    pub revision: u64,
    pub status: String,
    pub members: Vec<u32>,
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
}

/// One exact endpoint/client observation accumulated across collections.
#[derive(Clone, Debug)]
pub struct AppPeoplePeer {
    pub peer: AppPeer,
    pub collections: Vec<u32>,
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

/// A request from the app. `kind` is explicit so this stays a single command
/// envelope across Dart and Rust without generated union helpers.
#[derive(Clone, Debug)]
pub struct AppCommand {
    pub kind: String,
    pub name: Option<String>,
    pub files: Vec<AppSourceFile>,
    pub collection: Option<u32>,
    pub label: Option<String>,
    pub delete_files: Option<bool>,
    pub entry: Option<u32>,
    pub source: Option<String>,
    pub entries: Vec<u32>,
    pub contact: Option<u32>,
    pub handle: Option<String>,
    pub accept: Option<bool>,
    pub device: Option<u32>,
    pub active: Option<bool>,
    pub paused: Option<bool>,
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

fn locked_runtime() -> Result<std::sync::MutexGuard<'static, Option<Nexus>>, String> {
    runtime()
        .lock()
        .map_err(|_| "the Nexus runtime lock was poisoned".to_owned())
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
        })
        .collect())
}

/// Backend-owned People projection. Saved history is replaced by the same
/// collection's effective live observation before endpoint/client grouping.
pub fn people_peers() -> Result<Vec<AppPeoplePeer>, String> {
    use std::collections::BTreeMap;
    let runtime = locked_runtime()?;
    let nexus = runtime
        .as_ref()
        .ok_or_else(|| "start Nexus before listing peers".to_owned())?;
    let mut rows = BTreeMap::new();
    for collection in nexus.state().collections {
        for peer in nexus.peer_history(collection.id) {
            rows.insert(
                (collection.id, peer.address.clone(), peer.client.clone()),
                AppPeer {
                    address: peer.address,
                    client: peer.client,
                    down_bytes: peer.total_down_bytes,
                    up_bytes: peer.total_up_bytes,
                    down_bytes_per_second: 0,
                    up_bytes_per_second: 0,
                },
            );
        }
    }
    for (collection, peer) in nexus.peers() {
        rows.insert(
            (collection, peer.address.clone(), peer.client.clone()),
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
            });
        entry.peer.down_bytes += peer.down_bytes;
        entry.peer.up_bytes += peer.up_bytes;
        entry.peer.down_bytes_per_second += peer.down_bytes_per_second;
        entry.peer.up_bytes_per_second += peer.up_bytes_per_second;
        entry.collections.push(collection.0);
    }
    Ok(grouped.into_values().collect())
}

/// The collection's shareable magnet URI, when the local substrate has a real
/// persisted info hash for it. The URI is fetched on demand rather than added
/// to every snapshot because a QR is only useful on the screen that asked for
/// it.
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

/// Validates and accepts one command without waiting for I/O.
pub fn send(command: AppCommand) -> Result<AppAccepted, String> {
    crate::nexus::log::clog!(
        "api",
        "send kind={} collection={:?} entries={}",
        command.kind,
        command.collection,
        command.entries.len()
    );
    let command = command.into_core()?;
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

impl AppCommand {
    fn into_core(self) -> Result<Command, String> {
        let handle = |value: Option<u32>, field: &str| {
            value
                .map(Handle)
                .ok_or_else(|| format!("{field} is required for {}", self.kind))
        };
        let text = |value: Option<String>, field: &str| {
            value.ok_or_else(|| format!("{field} is required for {}", self.kind))
        };
        let files = || {
            self.files
                .into_iter()
                .map(|file| LocalFile {
                    name: file.name,
                    path: PathBuf::from(file.path),
                    bytes: file.bytes,
                })
                .collect()
        };

        match self.kind.as_str() {
            "createCollection" => Ok(Command::CreateCollection {
                name: text(self.name, "name")?,
                files: files(),
            }),
            "addMedia" => Ok(Command::AddMedia {
                collection: handle(self.collection, "collection")?,
                label: text(self.label, "label")?,
                files: files(),
            }),
            "renameCollection" => Ok(Command::RenameCollection {
                collection: handle(self.collection, "collection")?,
                name: text(self.name, "name")?,
            }),
            "deleteCollection" => Ok(Command::DeleteCollection {
                collection: handle(self.collection, "collection")?,
                delete_files: self.delete_files.unwrap_or(false),
            }),
            "downloadEntry" => Ok(Command::DownloadEntry {
                collection: handle(self.collection, "collection")?,
                entry: handle(self.entry, "entry")?,
            }),
            "retryTransfer" => Ok(Command::RetryTransfer {
                collection: handle(self.collection, "collection")?,
            }),
            "setPaused" => Ok(Command::SetPaused {
                collection: handle(self.collection, "collection")?,
                // Required rather than defaulted: a pause command that
                // silently means "resume" because a field was missed is the
                // one mistake this crossing can make invisibly.
                paused: self
                    .paused
                    .ok_or_else(|| "paused is required for setPaused".to_owned())?,
            }),
            "publishDraft" => Ok(Command::PublishDraft {
                collection: handle(self.collection, "collection")?,
            }),
            "deleteFiles" => Ok(Command::DeleteFiles {
                collection: handle(self.collection, "collection")?,
            }),
            "importTorrent" => Ok(Command::ImportTorrent {
                source: text(self.source, "source")?,
            }),
            "downloadSelection" => Ok(Command::DownloadSelection {
                collection: handle(self.collection, "collection")?,
                entries: self.entries.into_iter().map(Handle).collect(),
            }),
            "shareWith" => Ok(Command::ShareWith {
                collection: handle(self.collection, "collection")?,
                contact: handle(self.contact, "contact")?,
            }),
            "removeMember" => Ok(Command::RemoveMember {
                collection: handle(self.collection, "collection")?,
                contact: handle(self.contact, "contact")?,
            }),
            "addContact" => Ok(Command::AddContact {
                handle: text(self.handle, "handle")?,
            }),
            "respondToRequest" => Ok(Command::RespondToRequest {
                contact: handle(self.contact, "contact")?,
                accept: self.accept.unwrap_or(false),
            }),
            "markVerified" => Ok(Command::MarkVerified {
                contact: handle(self.contact, "contact")?,
            }),
            "blockContact" => Ok(Command::BlockContact {
                contact: handle(self.contact, "contact")?,
            }),
            "revokeDevice" => Ok(Command::RevokeDevice {
                device: handle(self.device, "device")?,
            }),
            "setActive" => Ok(Command::SetActive {
                active: self.active.unwrap_or(false),
            }),
            "resolveFork" => {
                Err("resolveFork needs a revision hash and is not bridged yet".to_owned())
            }
            _ => Err(format!("unknown Nexus command: {}", self.kind)),
        }
    }
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
    }
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

    AppCollection {
        id: collection.id.0,
        name: collection.name.clone(),
        nature: collection.nature.wire().to_owned(),
        role: collection.role.wire().to_owned(),
        revision: collection.revision,
        status: status.to_owned(),
        members: collection.members.iter().map(|member| member.0).collect(),
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
    use crate::nexus::projection::state::{Connectivity, DeviceState};

    fn command(kind: &str) -> AppCommand {
        AppCommand {
            kind: kind.to_owned(),
            name: None,
            files: Vec::new(),
            collection: None,
            label: None,
            delete_files: None,
            entry: None,
            source: None,
            entries: Vec::new(),
            contact: None,
            handle: None,
            accept: None,
            device: None,
            active: None,
            paused: None,
        }
    }

    #[test]
    fn maps_torrent_import_without_a_legacy_type() {
        let mut command = command("importTorrent");
        command.source = Some("magnet:?xt=urn:btih:abc".to_owned());

        assert_eq!(
            command.into_core(),
            Ok(Command::ImportTorrent {
                source: "magnet:?xt=urn:btih:abc".to_owned()
            })
        );
    }

    #[test]
    fn maps_source_metadata_without_moving_media_through_the_bridge() {
        let mut command = command("createCollection");
        command.name = Some("Episodes".to_owned());
        command.files = vec![AppSourceFile {
            name: "Episode 1.mp4".to_owned(),
            path: "phasset://native-identifier".to_owned(),
            bytes: 42,
        }];

        assert_eq!(
            command.into_core(),
            Ok(Command::CreateCollection {
                name: "Episodes".to_owned(),
                files: vec![LocalFile {
                    name: "Episode 1.mp4".to_owned(),
                    path: PathBuf::from("phasset://native-identifier"),
                    bytes: 42,
                }],
            })
        );
    }

    #[test]
    fn maps_a_torrent_selection_to_core_handles() {
        let mut command = command("downloadSelection");
        command.collection = Some(7);
        command.entries = vec![2, 5];

        assert_eq!(
            command.into_core(),
            Ok(Command::DownloadSelection {
                collection: Handle(7),
                entries: vec![Handle(2), Handle(5)],
            })
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
    }

    #[test]
    fn rejects_an_unknown_command_before_it_reaches_the_core() {
        assert_eq!(
            command("explode").into_core(),
            Err("unknown Nexus command: explode".to_owned())
        );
    }
}
