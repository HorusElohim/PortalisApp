//! Deciding what crosses the bridge, and when.
//!
//! Four rules, and each one exists because its absence has a
//! cost somebody can feel:
//!
//! **An unchanged tick sends nothing.** Idle must cost zero. A projection that
//! re-sends an identical snapshot every second keeps a phone's radio and CPU
//! awake for no reason, and the interface cannot tell the difference — so the
//! comparison belongs here, once, rather than in every widget.
//!
//! **Progress coalesces.** A transfer produces readings continuously; a person
//! reads at most a few a second. Everything within a window collapses to the
//! newest, because an older reading of a moving number is not information.
//!
//! **Detail arrives only while it is wanted.** A piece map is tens of
//! thousands of bits. Sending one for a collection nobody is looking at is the
//! difference between a scroll that is smooth and one that is not.
//!
//! **A command is answered immediately.** Acceptance is a local decision, so
//! it does not wait for a network — what happens next arrives through the
//! state, on the object affected.

use std::time::Duration;

use super::state::{Detail, Handle, PortalisState};

/// The fastest the progress tier is allowed to move.
///
/// Four times a second: fast enough that a number looks live, slow enough that
/// nothing downstream is doing work a person cannot perceive.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// What one tick decided to send.
#[derive(Clone, Debug, PartialEq)]
pub struct Emission {
    /// The structure tier, present only when something actually changed.
    pub state: Option<PortalisState>,
    /// The detail tier, present only for a subscribed collection that changed.
    pub detail: Option<Detail>,
}

impl Emission {
    /// Whether this tick sends anything at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.state.is_none() && self.detail.is_none()
    }

    /// How many bytes this tick would put on the bridge.
    ///
    /// An estimate, and deliberately rough: it exists so a demo and a test can
    /// say "idle costs nothing" in numbers rather than in prose.
    #[must_use]
    pub fn size(&self) -> usize {
        let structure = self.state.as_ref().map_or(0, |state| {
            state.collections.len() * 96 + state.contacts.len() * 96 + state.alerts.len() * 16
        });
        let detail = self
            .detail
            .as_ref()
            .map_or(0, |detail| detail.entries.len() * 64 + detail.pieces.len());
        structure + detail
    }
}

/// Holds what was last sent, so it can decide what not to send again.
///
/// The whole projection is one of these plus a clock. It keeps the previous
/// snapshot rather than a hash of it, because "what changed" is a question the
/// interface never asks and this side answers by not sending.
#[derive(Debug, Default)]
pub struct Projector {
    last_state: Option<PortalisState>,
    last_detail: Option<Detail>,
    /// The collection whose view is open, if any.
    subscribed: Option<Handle>,
    /// When the progress tier was last allowed through.
    progress_sent_at: Option<Duration>,
}

impl Projector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to one collection's detail, or unsubscribes with `None`.
    ///
    /// Changing the subscription forgets what was last sent, so the new view
    /// gets a complete first delivery rather than a diff against a collection
    /// it never saw.
    pub fn watch_detail(&mut self, collection: Option<Handle>) {
        if self.subscribed != collection {
            self.subscribed = collection;
            self.last_detail = None;
        }
    }

    /// What is currently subscribed.
    #[must_use]
    pub const fn subscribed(&self) -> Option<Handle> {
        self.subscribed
    }

    /// Decides what to send for one tick.
    ///
    /// `now` is monotonic time since the core started, passed in rather than
    /// read, so the coalescing window is testable without sleeping.
    pub fn tick(
        &mut self,
        state: &PortalisState,
        detail: Option<&Detail>,
        now: Duration,
    ) -> Emission {
        let structure_changed = self.last_state.as_ref() != Some(state);
        let carries_progress = state
            .collections
            .iter()
            .any(|collection| collection.transfer.is_some());

        // A change that is only a moving number waits for the window; a change
        // to anything else does not, because a status or a member appearing is
        // a fact and facts are not sampled.
        let structural_change = self
            .last_state
            .as_ref()
            .is_none_or(|last| !same_except_progress(last, state));
        let within_window = self
            .progress_sent_at
            .is_some_and(|last| now.saturating_sub(last) < PROGRESS_INTERVAL);
        let send_state =
            structure_changed && (structural_change || !carries_progress || !within_window);

        if send_state {
            if carries_progress {
                self.progress_sent_at = Some(now);
            }
            self.last_state = Some(state.clone());
        }

        // Detail is only ever sent for the collection actually being looked
        // at, and only when it changed.
        let detail = detail
            .filter(|detail| self.subscribed == Some(detail.id))
            .filter(|detail| self.last_detail.as_ref() != Some(*detail))
            .cloned();
        if let Some(detail) = &detail {
            self.last_detail = Some(detail.clone());
        }

        Emission {
            state: send_state.then(|| state.clone()),
            detail,
        }
    }
}

/// Whether two snapshots differ only in numbers that are sampled.
///
/// Used to decide whether a change may wait for the coalescing window. A
/// collection appearing, a status changing or a member arriving must go now; a
/// byte count moving may wait.
fn same_except_progress(left: &PortalisState, right: &PortalisState) -> bool {
    left.device == right.device
        && left.connectivity == right.connectivity
        && left.contacts == right.contacts
        && left.alerts == right.alerts
        && left.collections.len() == right.collections.len()
        && left
            .collections
            .iter()
            .zip(&right.collections)
            .all(|(left, right)| {
                left.id == right.id
                    && left.name == right.name
                    && left.role == right.role
                    && left.revision == right.revision
                    && left.status == right.status
                    && left.members == right.members
                    && left.entries == right.entries
                    && left.total_bytes == right.total_bytes
                    && left.pending == right.pending
                    && left.transfer.is_some() == right.transfer.is_some()
            })
}

#[cfg(test)]
mod tests {
    use super::super::state::{
        Alert, CollectionState, Connectivity, DeviceState, Role, Status, Transfer,
    };
    use super::*;

    const COLLECTION: Handle = Handle(1);

    fn state(collections: Vec<CollectionState>) -> PortalisState {
        PortalisState {
            device: DeviceState {
                name: "Ada's laptop".to_owned(),
                handle: Some("ada#7Q2XZ".to_owned()),
                fingerprint: "aaaa bbbb".to_owned(),
                devices: 1,
            },
            connectivity: Connectivity::LocalOnly,
            contacts: Vec::new(),
            collections,
            alerts: Vec::new(),
        }
    }

    fn collection(transfer: Option<Transfer>) -> CollectionState {
        CollectionState {
            started_at: None,
            completed_at: None,
            id: COLLECTION,
            name: "Iceland".to_owned(),
            nature: crate::nexus::projection::state::Nature::Native,
            role: Role::Owner,
            revision: 1,
            status: Status::Available,
            members: Vec::new(),
            entries: 2,
            total_bytes: 1_024,
            on_disk_bytes: 0,
            uploaded_bytes: 0,
            transfer,
            pending: None,
        }
    }

    fn moving(done: f32) -> Option<Transfer> {
        Some(Transfer {
            progress: done,
            down_bytes_per_second: 4_096,
            up_bytes_per_second: 1_024,
            peers: 3,
            eta_secs: Some(10),
        })
    }

    fn detail(pieces: u8) -> Detail {
        Detail {
            id: COLLECTION,
            entries: Vec::new(),
            pieces: vec![pieces; 4],
            peers: Vec::new(),
        }
    }

    /// The rule idle costs nothing rests on.
    #[test]
    fn an_unchanged_tick_sends_nothing() {
        let mut projector = Projector::new();
        let state = state(vec![collection(None)]);

        let first = projector.tick(&state, None, Duration::ZERO);
        assert!(first.state.is_some(), "the first tick is a full snapshot");

        for tick in 1..10 {
            let quiet = projector.tick(&state, None, Duration::from_secs(tick));
            assert!(quiet.is_empty(), "tick {tick} sent something");
            assert_eq!(quiet.size(), 0);
        }
    }

    /// A fact is not sampled: a status change goes now, whatever the window.
    #[test]
    fn a_change_that_is_not_a_number_is_sent_immediately() {
        let mut projector = Projector::new();
        projector.tick(&state(vec![collection(moving(0.1))]), None, Duration::ZERO);

        let settled = CollectionState {
            status: Status::Available,
            transfer: None,
            ..collection(None)
        };
        let emission = projector.tick(&state(vec![settled]), None, Duration::from_millis(10));

        assert!(
            emission.state.is_some(),
            "a transfer finishing must not wait for the progress window"
        );
    }

    /// The coalescing rule: many readings, at most four a second.
    #[test]
    fn progress_coalesces_to_the_window() {
        let mut projector = Projector::new();
        projector.tick(&state(vec![collection(moving(0.0))]), None, Duration::ZERO);

        // Twenty readings across 200ms, inside one window. Counted with a
        // filter rather than an `if`, so the "something was sent" branch is
        // not a line that must never run.
        let sent = (1..=20_u64)
            .filter(|step| {
                let at = Duration::from_millis(step * 10);
                #[allow(clippy::cast_precision_loss, reason = "a test's progress fraction")]
                let progress = *step as f32 / 100.0;
                !projector
                    .tick(&state(vec![collection(moving(progress))]), None, at)
                    .is_empty()
            })
            .count();
        assert_eq!(sent, 0, "everything inside the window collapsed");

        // Past the window, the newest reading goes.
        let emission = projector.tick(
            &state(vec![collection(moving(0.5))]),
            None,
            PROGRESS_INTERVAL + Duration::from_millis(1),
        );
        let collections = emission
            .state
            .expect("a reading escapes the window")
            .collections;
        assert_eq!(
            collections[0].transfer.expect("moving").progress,
            0.5,
            "and it is the newest, not the oldest"
        );
    }

    #[test]
    fn detail_arrives_only_for_the_collection_being_watched() {
        let mut projector = Projector::new();
        let state = state(vec![collection(None)]);

        // Nothing subscribed: the expensive tier is not built for anybody.
        let emission = projector.tick(&state, Some(&detail(1)), Duration::ZERO);
        assert!(emission.detail.is_none());

        projector.watch_detail(Some(COLLECTION));
        let emission = projector.tick(&state, Some(&detail(1)), Duration::from_secs(1));
        assert_eq!(emission.detail, Some(detail(1)));

        // Unchanged detail is not re-sent.
        let emission = projector.tick(&state, Some(&detail(1)), Duration::from_secs(2));
        assert!(emission.detail.is_none());

        // A different collection's detail is not this subscription's business.
        let elsewhere = Detail {
            id: Handle(9),
            ..detail(2)
        };
        let emission = projector.tick(&state, Some(&elsewhere), Duration::from_secs(3));
        assert!(emission.detail.is_none());

        // Unsubscribing stops it.
        projector.watch_detail(None);
        assert_eq!(projector.subscribed(), None);
        let emission = projector.tick(&state, Some(&detail(3)), Duration::from_secs(4));
        assert!(emission.detail.is_none());
    }

    /// Opening a different collection's view gets a complete first delivery,
    /// not a diff against one it never saw.
    #[test]
    fn changing_subscription_forgets_what_was_sent() {
        let mut projector = Projector::new();
        let state = state(vec![collection(None)]);

        projector.watch_detail(Some(COLLECTION));
        projector.tick(&state, Some(&detail(1)), Duration::ZERO);

        projector.watch_detail(Some(Handle(9)));
        projector.watch_detail(Some(COLLECTION));
        let emission = projector.tick(&state, Some(&detail(1)), Duration::from_secs(1));

        assert_eq!(
            emission.detail,
            Some(detail(1)),
            "the same detail is sent again for a newly opened view"
        );
        // Re-subscribing to what is already subscribed changes nothing.
        projector.watch_detail(Some(COLLECTION));
        assert!(
            projector
                .tick(&state, Some(&detail(1)), Duration::from_secs(2))
                .is_empty()
        );
    }

    #[test]
    fn an_emission_reports_what_it_would_cost() {
        let mut projector = Projector::new();
        projector.watch_detail(Some(COLLECTION));
        let state = state(vec![collection(None)]);

        let emission = projector.tick(&state, Some(&detail(1)), Duration::ZERO);

        assert!(emission.size() > 0);
        assert!(!emission.is_empty());
        assert_eq!(
            Emission {
                state: None,
                detail: None
            }
            .size(),
            0
        );
    }

    #[test]
    fn alerts_and_contacts_count_as_structure() {
        let mut projector = Projector::new();
        let quiet = state(vec![collection(None)]);
        projector.tick(&quiet, None, Duration::ZERO);

        let alarming = PortalisState {
            alerts: vec![Alert::ConflictingHistory {
                collection: COLLECTION,
            }],
            ..quiet
        };
        let emission = projector.tick(&alarming, None, Duration::from_millis(1));

        assert!(
            emission.state.is_some(),
            "a fork must not wait for a progress window"
        );
    }
}
