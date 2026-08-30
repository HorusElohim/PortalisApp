//! Turning what the core holds into what the interface is told.
//!
//! Every derivation happens here, once. A percentage, a status, whether a
//! contact is verified — each is computed on this side, because the same rule
//! implemented twice is two rules that disagree the first time they are given
//! an awkward input.
//!
//! Handles are assigned here too, and only here. They are indices into what
//! this process currently holds, so the map lives with the thing that builds
//! the snapshot and nowhere else.

use std::collections::HashMap;

use super::state::{
    Alert, CollectionState, Connectivity, ContactState, DeviceState, Handle, PortalisState,
    Progress, Role, Status, Transfer,
};
use crate::nexus::store::records::Role as StoredRole;

/// Assigns handles and remembers them, so one collection keeps one handle for
/// as long as the process lives.
///
/// Stable within a run and meaningless across runs, which is exactly what §17
/// asks of a handle: cheap to send, meaningless to persist.
#[derive(Debug, Default)]
pub struct Handles {
    assigned: HashMap<Vec<u8>, Handle>,
    next: u32,
}

impl Handles {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The handle for `key`, assigning one if this is the first sighting.
    pub fn of(&mut self, key: &[u8]) -> Handle {
        if let Some(handle) = self.assigned.get(key) {
            return *handle;
        }
        self.next += 1;
        let handle = Handle(self.next);
        self.assigned.insert(key.to_vec(), handle);
        handle
    }

    /// How many handles have been assigned.
    #[must_use]
    pub fn len(&self) -> usize {
        self.assigned.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.assigned.is_empty()
    }
}

/// What the core knows about one collection, before it becomes a projection.
#[derive(Clone, Debug)]
pub struct CollectionFacts {
    pub collection_id: Vec<u8>,
    pub name: String,
    pub role: StoredRole,
    pub revision: u64,
    pub entries: u32,
    pub total_bytes: u64,
    pub members: Vec<Vec<u8>>,
    /// The newest transfer reading, if anything is moving.
    pub progress: Option<Progress>,
    /// Set when verification refused this collection's latest revision.
    pub failure: Option<Status>,
    /// Whether this device has been told to stop transferring it.
    pub paused: bool,
    /// How much of it this device is holding.
    pub on_disk_bytes: u64,
    /// How much this device has sent for it this session.
    pub uploaded_bytes: u64,
}

/// Builds one collection's projection, deriving everything the interface would
/// otherwise have to.
pub fn collection(handles: &mut Handles, facts: &CollectionFacts) -> CollectionState {
    let id = handles.of(&facts.collection_id);
    let members = facts
        .members
        .iter()
        .map(|member| handles.of(member))
        .collect();

    CollectionState {
        id,
        // This builder answers from a manifest, which records what a
        // collection is rather than when this device fetched it.
        started_at: None,
        completed_at: None,
        name: facts.name.clone(),
        nature: crate::nexus::projection::state::Nature::Native,
        role: match facts.role {
            StoredRole::Owner => Role::Owner,
            StoredRole::Member => Role::Member,
        },
        revision: facts.revision,
        // A failure outranks anything a transfer is doing: a person needs to
        // know they cannot trust it before they are told how fast it is.
        status: facts.failure.unwrap_or_else(|| status_of(facts)),
        members,
        entries: facts.entries,
        total_bytes: facts.total_bytes,
        on_disk_bytes: facts.on_disk_bytes,
        uploaded_bytes: facts.uploaded_bytes,
        transfer: facts.progress.map(transfer),
        pending: None,
    }
}

/// What a collection is doing, from the numbers rather than from a flag
/// somebody remembered to set.
fn status_of(facts: &CollectionFacts) -> Status {
    // A person's choice outranks the numbers. A collection whose last reading
    // arrived before it was paused is paused, not downloading.
    if facts.paused {
        return Status::Paused;
    }
    match facts.progress {
        None if facts.revision == 0 => Status::WaitingForOwner,
        None => Status::Available,
        Some(progress) if progress.done == 0 => Status::DownloadRequested,
        Some(progress) if progress.done >= progress.total => Status::Available,
        Some(_) => Status::Downloading,
    }
}

/// One reading, with the arithmetic the interface would otherwise repeat.
fn transfer(progress: Progress) -> Transfer {
    let fraction = if progress.total == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss, reason = "a fraction for a progress bar")]
        {
            progress.done as f32 / progress.total as f32
        }
    };
    Transfer {
        progress: fraction.clamp(0.0, 1.0),
        source_reading: false,
        down_bytes_per_second: progress.down_bytes_per_second,
        up_bytes_per_second: progress.up_bytes_per_second,
        peers: progress.peers,
        // Honest about not knowing: a rate of zero means no estimate rather
        // than an infinite one, and remaining/rate is the only claim the
        // numbers support.
        eta_secs: (progress.down_bytes_per_second > 0 && progress.total > progress.done).then(
            || {
                let remaining = progress.total - progress.done;
                u32::try_from(remaining / u64::from(progress.down_bytes_per_second))
                    .unwrap_or(u32::MAX)
            },
        ),
    }
}

/// Assembles the whole snapshot.
///
/// Alerts are derived rather than accumulated: a fork is an alert for as long
/// as the collection says so, and a contact is an alert for as long as their
/// fingerprint has not been compared. Nothing has to remember to clear one.
#[must_use]
pub fn snapshot(
    device: DeviceState,
    connectivity: Connectivity,
    contacts: Vec<ContactState>,
    collections: Vec<CollectionState>,
) -> PortalisState {
    let mut alerts: Vec<Alert> = collections
        .iter()
        .filter(|collection| collection.status == Status::ConflictingHistory)
        .map(|collection| Alert::ConflictingHistory {
            collection: collection.id,
        })
        .collect();
    alerts.extend(
        contacts
            .iter()
            .filter(|contact| !contact.verified)
            .map(|contact| Alert::UnverifiedContact {
                contact: contact.id,
            }),
    );

    PortalisState {
        device,
        connectivity,
        contacts,
        collections,
        alerts,
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::{Friendship, VerifyFailure};
    use super::*;

    const COLLECTION: &[u8] = &[1; 16];
    const MEMBER: &[u8] = &[2; 32];

    fn facts() -> CollectionFacts {
        CollectionFacts {
            collection_id: COLLECTION.to_vec(),
            name: "Iceland".to_owned(),
            role: StoredRole::Owner,
            revision: 3,
            entries: 12,
            total_bytes: 4_096,
            members: vec![MEMBER.to_vec()],
            progress: None,
            failure: None,
            paused: false,
            on_disk_bytes: 0,
            uploaded_bytes: 0,
        }
    }

    fn progress(done: u64, rate: u32) -> Progress {
        Progress {
            done,
            total: 1_000,
            down_bytes_per_second: rate,
            up_bytes_per_second: 128,
            peers: 3,
        }
    }

    fn device() -> DeviceState {
        DeviceState {
            name: "Ada's laptop".to_owned(),
            handle: Some("ada#7Q2XZ".to_owned()),
            fingerprint: "aaaa bbbb".to_owned(),
            devices: 2,
        }
    }

    fn contact(id: Handle, verified: bool) -> ContactState {
        ContactState {
            id,
            display_name: "Mira".to_owned(),
            handle: Some("mira#4KQ2P".to_owned()),
            fingerprint: "cccc dddd".to_owned(),
            verified,
            friendship: Friendship::Accepted,
            reachable: None,
        }
    }

    /// One object, one handle, for as long as the process lives.
    #[test]
    fn a_handle_is_stable_within_a_run_and_unique_across_objects() {
        let mut handles = Handles::new();
        assert!(handles.is_empty());

        let first = handles.of(COLLECTION);
        assert_eq!(handles.of(COLLECTION), first, "the same object, twice");
        assert_ne!(handles.of(MEMBER), first);
        assert_eq!(handles.len(), 2);

        // A fresh process assigns from the beginning again, which is why a
        // handle is meaningless to persist.
        assert_eq!(Handles::new().of(MEMBER), first);
    }

    #[test]
    fn a_collection_carries_its_members_as_handles() {
        let mut handles = Handles::new();

        let projected = collection(&mut handles, &facts());

        assert_eq!(projected.id, handles.of(COLLECTION));
        assert_eq!(projected.members, vec![handles.of(MEMBER)]);
        assert_eq!(projected.role, Role::Owner);
        assert_eq!(projected.revision, 3);
        assert_eq!(projected.entries, 12);
        assert_eq!(projected.status, Status::Available);
        assert!(projected.transfer.is_none());
    }

    #[test]
    fn a_member_is_projected_as_one() {
        let projected = collection(
            &mut Handles::new(),
            &CollectionFacts {
                role: StoredRole::Member,
                ..facts()
            },
        );

        assert_eq!(projected.role, Role::Member);
    }

    /// Status comes from the numbers, not from a flag somebody set.
    #[test]
    fn status_is_derived_from_what_is_actually_happening() {
        let cases = [
            (None, 3, Status::Available),
            (None, 0, Status::WaitingForOwner),
            (Some(progress(0, 100)), 3, Status::DownloadRequested),
            (Some(progress(500, 100)), 3, Status::Downloading),
            (Some(progress(1_000, 100)), 3, Status::Available),
        ];

        for (progress, revision, expected) in cases {
            let projected = collection(
                &mut Handles::new(),
                &CollectionFacts {
                    progress,
                    revision,
                    ..facts()
                },
            );
            assert_eq!(projected.status, expected, "for {progress:?} at {revision}");
        }
    }

    /// A person needs to know they cannot trust a collection before they are
    /// told how fast it is arriving.
    #[test]
    fn a_verification_failure_outranks_a_transfer() {
        let projected = collection(
            &mut Handles::new(),
            &CollectionFacts {
                progress: Some(progress(500, 100)),
                failure: Some(Status::CannotVerify(VerifyFailure::Rollback)),
                ..facts()
            },
        );

        assert_eq!(
            projected.status,
            Status::CannotVerify(VerifyFailure::Rollback)
        );
        assert!(
            projected.transfer.is_some(),
            "the numbers are still there; the headline is not theirs"
        );
    }

    #[test]
    fn a_transfer_carries_the_arithmetic_the_interface_would_repeat() {
        let projected = collection(
            &mut Handles::new(),
            &CollectionFacts {
                progress: Some(progress(250, 50)),
                ..facts()
            },
        );
        let transfer = projected.transfer.expect("moving");

        assert!((transfer.progress - 0.25).abs() < f32::EPSILON);
        assert_eq!(transfer.eta_secs, Some(15), "750 bytes at 50 a second");
        assert_eq!(transfer.peers, 3);
    }

    /// An estimate is offered only when the numbers support one.
    #[test]
    fn an_eta_is_absent_rather_than_invented() {
        for (progress, expected) in [
            (progress(250, 0), None),
            (progress(1_000, 50), None),
            (progress(250, 50), Some(15)),
        ] {
            let projected = collection(
                &mut Handles::new(),
                &CollectionFacts {
                    progress: Some(progress),
                    ..facts()
                },
            );
            assert_eq!(projected.transfer.expect("moving").eta_secs, expected);
        }
    }

    #[test]
    fn a_transfer_of_nothing_is_not_a_division_by_zero() {
        let projected = collection(
            &mut Handles::new(),
            &CollectionFacts {
                progress: Some(Progress {
                    done: 0,
                    total: 0,
                    down_bytes_per_second: 0,
                    up_bytes_per_second: 0,
                    peers: 0,
                }),
                ..facts()
            },
        );
        let transfer = projected.transfer.expect("a reading");

        assert!((transfer.progress - 0.0).abs() < f32::EPSILON);
        assert_eq!(transfer.eta_secs, None);
    }

    /// Alerts are derived, so nothing has to remember to clear one.
    #[test]
    fn alerts_follow_from_state_rather_than_being_accumulated() {
        let mut handles = Handles::new();
        let forked = CollectionState {
            status: Status::ConflictingHistory,
            ..collection(&mut handles, &facts())
        };
        let unverified = contact(Handle(9), false);

        let state = snapshot(
            device(),
            Connectivity::LocalOnly,
            vec![unverified.clone(), contact(Handle(10), true)],
            vec![forked.clone()],
        );

        assert_eq!(
            state.alerts,
            vec![
                Alert::ConflictingHistory {
                    collection: forked.id
                },
                Alert::UnverifiedContact {
                    contact: unverified.id
                },
            ]
        );

        // Resolve both and the alerts are simply not derived again.
        let calm = snapshot(
            device(),
            Connectivity::LocalOnly,
            vec![contact(Handle(10), true)],
            vec![collection(&mut handles, &facts())],
        );
        assert!(calm.alerts.is_empty());
    }

    #[test]
    fn a_snapshot_carries_everything_the_interface_renders() {
        let state = snapshot(
            device(),
            Connectivity::Connecting,
            vec![contact(Handle(9), true)],
            vec![collection(&mut Handles::new(), &facts())],
        );

        assert_eq!(state.device.devices, 2);
        assert_eq!(state.connectivity, Connectivity::Connecting);
        assert_eq!(state.contacts.len(), 1);
        assert_eq!(state.collections.len(), 1);
    }
}
