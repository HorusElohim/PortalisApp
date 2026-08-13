//! The app-facing Nexus boundary.
//!
//! These values deliberately do not re-export the core projection. The core
//! uses terse Rust names such as `Handle` and `Status`; the bridge needs a
//! stable, unambiguous vocabulary that can evolve without colliding with the
//! legacy generated API while both paths exist.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::api::StreamSink;
use crate::core::nexus::Nexus;
use crate::projection::state::{Command, Detail, Handle, PortalisState};

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
    pub role: String,
    pub revision: u64,
    pub status: String,
    pub members: Vec<u32>,
    pub entries: u32,
    pub total_bytes: u64,
    pub transfer: Option<AppTransfer>,
    pub pending: Option<AppPending>,
}

/// A coalesced transfer sample.
#[derive(Clone, Debug)]
pub struct AppTransfer {
    pub progress: f32,
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
    pub samples: Vec<u8>,
}

/// A selectable media entry in a collection detail projection.
#[derive(Clone, Debug)]
pub struct AppEntry {
    pub id: u32,
    pub label: String,
    pub bytes: u64,
    pub selected: bool,
    pub available: bool,
}

/// A request from the app. `kind` is explicit so this stays a single command
/// envelope across Dart and Rust without generated union helpers.
#[derive(Clone, Debug)]
pub struct AppCommand {
    pub kind: String,
    pub name: Option<String>,
    pub files: Vec<String>,
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
}

/// The local acceptance result returned before a command performs I/O.
#[derive(Clone, Debug)]
pub struct AppAccepted {
    pub id: u64,
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
/// # Errors
///
/// Returns a displayable reason when the device identity or local store cannot
/// be opened.
pub fn start() -> Result<(), String> {
    let mut runtime = locked_runtime()?;
    if runtime.is_none() {
        *runtime = Some(Nexus::open_default().map_err(|error| error.to_string())?);
    }
    Ok(())
}

/// Stops the runtime and waits for its bounded shutdown.
pub async fn stop() -> Result<(), String> {
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
        sink.add(snapshot(&states.borrow()))
            .map_err(|error| error.to_string())?;
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
        sink.add(detail.borrow().as_ref().map(detail_projection))
            .map_err(|error| error.to_string())?;
        if detail.changed().await.is_err() {
            return Ok(());
        }
    }
}

/// Validates and accepts one command without waiting for I/O.
pub fn send(command: AppCommand) -> Result<AppAccepted, String> {
    let command = command.into_core()?;
    let runtime = locked_runtime()?;
    let accepted = runtime
        .as_ref()
        .ok_or_else(|| "start Nexus before sending a command".to_owned())?
        .command(&command)
        .map_err(|error| error.to_string())?;
    Ok(AppAccepted {
        id: accepted.id,
        queued: accepted.queued,
    })
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
        let files = || self.files.into_iter().map(PathBuf::from).collect();

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
        connectivity: format!("{:?}", state.connectivity),
        contacts: state
            .contacts
            .iter()
            .map(|contact| AppContact {
                id: contact.id.0,
                display_name: contact.display_name.clone(),
                handle: contact.handle.clone(),
                fingerprint: contact.fingerprint.clone(),
                verified: contact.verified,
                friendship: format!("{:?}", contact.friendship),
                reachable: contact.reachable.map(|security| format!("{security:?}")),
            })
            .collect(),
        collections: state
            .collections
            .iter()
            .map(|collection| AppCollection {
                id: collection.id.0,
                name: collection.name.clone(),
                role: format!("{:?}", collection.role),
                revision: collection.revision,
                status: format!("{:?}", collection.status),
                members: collection.members.iter().map(|member| member.0).collect(),
                entries: collection.entries,
                total_bytes: collection.total_bytes,
                transfer: collection.transfer.map(|transfer| AppTransfer {
                    progress: transfer.progress,
                    down_bytes_per_second: transfer.down_bytes_per_second,
                    up_bytes_per_second: transfer.up_bytes_per_second,
                    peers: transfer.peers,
                    eta_secs: transfer.eta_secs,
                }),
                pending: collection.pending.map(|pending| AppPending {
                    command: pending.command,
                    queued: pending.queued,
                }),
            })
            .collect(),
        alerts: state
            .alerts
            .iter()
            .map(|alert| format!("{alert:?}"))
            .collect(),
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
            })
            .collect(),
        pieces: detail.pieces.clone(),
        samples: detail.samples.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::state::{Connectivity, DeviceState};

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
