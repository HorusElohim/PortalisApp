//! What the interface is told, and what it may ask for.
//!
//! `SPEC.md` §17. Two rules shape every type here.
//!
//! **Nothing is derived on the far side.** A field the interface would have to
//! compute — a percentage, a status, whether something is verified — is
//! computed here, once. The alternative is two implementations of the same
//! rule that disagree under load, which is what "the interface stops asking"
//! is really about.
//!
//! **Handles, not strings.** A [`Handle`] is opaque, process-local and cheap
//! to send. Hex appears only where a person reads it — a fingerprint they
//! compare — and never as an identifier the interface carries around.
//!
//! These are plain values with no behaviour, because they cross a bridge. A
//! type with methods on this side becomes a type with methods that do not
//! exist on the other.

use std::path::PathBuf;

/// Names one object for as long as this process lives.
///
/// Meaningless to persist: it is an index into what the core currently holds,
/// not an identity. Persisting one and using it after a restart would name
/// something else, which is why it is deliberately not the collection id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Handle(pub u32);

/// Whether the bytes travel directly, and how well the peer is known.
pub use crate::core::events::{Connectivity, Path, PeerTrust, Progress, Security};

/// This device, as the interface should describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceState {
    pub name: String,
    /// The person's handle, once they have one.
    pub handle: Option<String>,
    /// This device's own fingerprint, for someone else to compare.
    pub fingerprint: String,
    /// Every device on this person's log, including this one.
    pub devices: u32,
}

/// How far a friendship has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Friendship {
    /// We asked; they have not answered.
    Requested,
    /// They asked; we have not answered.
    Pending,
    Accepted,
    Blocked,
}

/// Someone this device knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactState {
    pub id: Handle,
    pub display_name: String,
    pub handle: Option<String>,
    /// Shown so a person can compare it out of band (D4). The one place hex
    /// is deliberate.
    pub fingerprint: String,
    /// Whether that comparison actually happened.
    pub verified: bool,
    pub friendship: Friendship,
    /// `None` when not connected.
    pub reachable: Option<Security>,
}

/// Whether this device publishes a collection or reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Owner,
    Member,
}

/// Why verification failed, in the terms §18 shows a person.
pub use crate::core::events::VerifyFailure;

/// What a collection is doing, as one answer rather than several booleans.
///
/// A single enum because the interface renders one line, and a set of flags
/// would let it render two contradictory ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Available,
    /// A descriptor arrived; the transfer has not started.
    Preparing,
    Downloading,
    /// The key was rotated and this device is republishing.
    Updating,
    /// A revision verified, and no content key has arrived for it yet.
    WaitingForOwner,
    /// Omitted from a later revision.
    AccessRemoved,
    NeedsNewerVersion,
    CannotVerify(VerifyFailure),
    /// Two revisions with one number. Never resolved silently.
    ConflictingHistory,
}

/// A transfer in flight. The progress tier, coalesced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transfer {
    /// Zero to one. Computed here so the interface does not divide.
    pub progress: f32,
    pub down_bytes_per_second: u32,
    pub up_bytes_per_second: u32,
    pub peers: u16,
    /// `None` when there is not enough history to say honestly.
    pub eta_secs: Option<u32>,
}

/// A command that has been accepted and not yet settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pending {
    pub command: u64,
    /// Whether it is waiting for connectivity rather than working.
    pub queued: bool,
}

/// One collection, as the interface shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionState {
    pub id: Handle,
    pub name: String,
    pub role: Role,
    pub revision: u64,
    pub status: Status,
    pub members: Vec<Handle>,
    pub entries: u32,
    pub total_bytes: u64,
    /// Progress tier: present only while something is moving.
    pub transfer: Option<Transfer>,
    pub pending: Option<Pending>,
}

/// Something that needs a person's attention and is not an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Alert {
    /// Two valid revisions with one number: a compromised owner device, or a
    /// service splitting members' views. Surfaced, never resolved silently.
    ConflictingHistory { collection: Handle },
    /// A contact whose fingerprint has never been compared.
    UnverifiedContact { contact: Handle },
    /// This device was revoked by its owner.
    SignedOut,
    /// A member linked a device after the owner sealed to them.
    ResealOwed { collection: Handle, contact: Handle },
}

/// Everything the interface renders, in one value.
///
/// A complete snapshot rather than a delta, so a restart never depends on
/// having seen earlier events (§16).
#[derive(Clone, Debug, PartialEq)]
pub struct PortalisState {
    pub device: DeviceState,
    pub connectivity: Connectivity,
    pub contacts: Vec<ContactState>,
    pub collections: Vec<CollectionState>,
    pub alerts: Vec<Alert>,
}

/// One entry, in the detail tier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryState {
    pub id: Handle,
    pub label: String,
    pub bytes: u64,
    /// The local choice to include this file when the torrent is confirmed.
    pub selected: bool,
    pub available: bool,
}

/// The expensive tier, delivered only while a collection's view is open.
///
/// `pieces` and `samples` are packed rather than object graphs: a piece map is
/// tens of thousands of bits, and rebuilding it as a list on every tick is how
/// a scroll becomes a stutter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detail {
    pub id: Handle,
    pub entries: Vec<EntryState>,
    /// One bit per piece, packed.
    pub pieces: Vec<u8>,
    /// Fixed-width `(t, down, up, progress)` rows.
    pub samples: Vec<u8>,
}

/// What the interface may ask the core to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    CreateCollection {
        name: String,
        files: Vec<PathBuf>,
    },
    AddMedia {
        collection: Handle,
        label: String,
        files: Vec<PathBuf>,
    },
    RenameCollection {
        collection: Handle,
        name: String,
    },
    DeleteCollection {
        collection: Handle,
        delete_files: bool,
    },
    DownloadEntry {
        collection: Handle,
        entry: Handle,
    },
    RetryTransfer {
        collection: Handle,
    },
    /// Resolves a magnet URI or `.torrent` file into a shareable collection.
    /// No payload bytes are fetched until a later selection confirms them.
    ImportTorrent {
        source: String,
    },
    /// Starts only the entries selected after torrent metadata is available.
    DownloadSelection {
        collection: Handle,
        entries: Vec<Handle>,
    },

    ShareWith {
        collection: Handle,
        contact: Handle,
    },
    RemoveMember {
        collection: Handle,
        contact: Handle,
    },
    ResolveFork {
        collection: Handle,
        keep: [u8; 32],
    },

    AddContact {
        handle: String,
    },
    RespondToRequest {
        contact: Handle,
        accept: bool,
    },
    MarkVerified {
        contact: Handle,
    },
    BlockContact {
        contact: Handle,
    },

    RevokeDevice {
        device: Handle,
    },
    SetActive {
        active: bool,
    },
}

impl Command {
    /// Whether this command can be queued until there is connectivity.
    ///
    /// Most can: they change local state and publish later, which is what
    /// lets the interface accept them instantly with the network down. The
    /// few that cannot are the ones whose whole purpose is to reach somebody.
    #[must_use]
    pub const fn is_deferrable(&self) -> bool {
        !matches!(self, Self::AddContact { .. })
    }
}

/// A command the core has taken responsibility for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accepted {
    /// Names this command in the `pending` field of whatever it affects, so
    /// the interface can show it after a restart mid-operation.
    pub id: u64,
    /// The collection created by this command, when there is one. This lets
    /// the interface open a newly imported torrent without guessing from a
    /// concurrently changing list.
    pub collection: Option<Handle>,
    /// Accepted but waiting for connectivity.
    pub queued: bool,
}

/// Why a command was not accepted.
///
/// Returned after local acceptance. Anything that needs peers or a transfer
/// arrives later through the state.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Invalid(String),
    #[error("that is not something this device may do")]
    NotPermitted,
    #[error("the {0} limit has been reached")]
    QuotaReached(&'static str),
    /// Needs connectivity and cannot be queued.
    #[error("this needs a connection, and cannot wait for one")]
    Unavailable,
    /// The local acceptance transaction could not be made durable.
    #[error("Portalis could not durably save this command: {0}")]
    Persistence(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_that_must_reach_somebody_cannot_be_queued() {
        assert!(
            !Command::AddContact {
                handle: "ada#7Q2XZ".to_owned()
            }
            .is_deferrable(),
            "resolving a handle needs the directory, now"
        );
    }

    /// Everything else is queued rather than refused, which is what lets the
    /// interface accept a command instantly with the network down.
    #[test]
    fn every_other_command_can_wait_for_connectivity() {
        let collection = Handle(1);
        let contact = Handle(2);
        let deferrable = [
            Command::CreateCollection {
                name: "Iceland".to_owned(),
                files: Vec::new(),
            },
            Command::AddMedia {
                collection,
                label: "one.jpg".to_owned(),
                files: Vec::new(),
            },
            Command::RenameCollection {
                collection,
                name: "Iceland, 2019".to_owned(),
            },
            Command::DeleteCollection {
                collection,
                delete_files: true,
            },
            Command::DownloadEntry {
                collection,
                entry: Handle(3),
            },
            Command::RetryTransfer { collection },
            Command::ImportTorrent {
                source: "magnet:?xt=urn:btih:abc".to_owned(),
            },
            Command::DownloadSelection {
                collection,
                entries: vec![Handle(3)],
            },
            Command::ShareWith {
                collection,
                contact,
            },
            Command::RemoveMember {
                collection,
                contact,
            },
            Command::ResolveFork {
                collection,
                keep: [1; 32],
            },
            Command::RespondToRequest {
                contact,
                accept: true,
            },
            Command::MarkVerified { contact },
            Command::BlockContact { contact },
            Command::RevokeDevice { device: Handle(4) },
            Command::SetActive { active: true },
        ];

        for command in deferrable {
            assert!(command.is_deferrable(), "{command:?} should queue");
        }
    }

    #[test]
    fn a_refusal_says_which_kind_it_is() {
        assert_eq!(
            CommandError::Invalid("a collection needs a name".to_owned()).to_string(),
            "a collection needs a name"
        );
        assert!(CommandError::QuotaReached("collection")
            .to_string()
            .contains("collection"));
        assert!(CommandError::NotPermitted.to_string().contains("may do"));
        assert!(CommandError::Unavailable
            .to_string()
            .contains("cannot wait"));
    }

    /// The interface renders one line per collection, so its state is one
    /// answer rather than a set of flags that could contradict each other.
    #[test]
    fn a_status_is_one_answer() {
        let statuses = [
            Status::Available,
            Status::Preparing,
            Status::Downloading,
            Status::Updating,
            Status::WaitingForOwner,
            Status::AccessRemoved,
            Status::NeedsNewerVersion,
            Status::CannotVerify(VerifyFailure::Rollback),
            Status::ConflictingHistory,
        ];

        for (index, status) in statuses.iter().enumerate() {
            for (other, another) in statuses.iter().enumerate() {
                assert_eq!(
                    status == another,
                    index == other,
                    "{status:?} vs {another:?}"
                );
            }
        }
        assert_ne!(
            Status::CannotVerify(VerifyFailure::Rollback),
            Status::CannotVerify(VerifyFailure::Signature),
            "the reason is part of the answer"
        );
    }

    #[test]
    fn a_handle_is_cheap_and_ordered() {
        let mut handles = [Handle(3), Handle(1), Handle(2)];
        handles.sort_unstable();

        assert_eq!(handles, [Handle(1), Handle(2), Handle(3)]);
        assert_eq!(size_of::<Handle>(), 4, "cheap to send across a bridge");
    }

    #[test]
    fn an_alert_names_what_needs_attention() {
        let alerts = [
            Alert::ConflictingHistory {
                collection: Handle(1),
            },
            Alert::UnverifiedContact { contact: Handle(2) },
            Alert::SignedOut,
            Alert::ResealOwed {
                collection: Handle(1),
                contact: Handle(2),
            },
        ];

        for alert in &alerts {
            assert_eq!(alert.clone(), alert.clone());
        }
        assert_ne!(alerts[0], alerts[1]);
    }
}
