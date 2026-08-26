//! What a collection is *for*, as one durable answer.
//!
//! The store keeps `draft`, `paused`, `substrate_handle` and a set of selected
//! import entries. Each is a fact, but the combinations are not all meaningful,
//! and the meaning of one depends on the others:
//!
//! - `draft` means "not shared yet" for a native collection and "the file list
//!   has not been confirmed" for a torrent import. Two different decisions
//!   under one name.
//! - `selected` on a resolved import means "chosen by the person" *after*
//!   confirmation and "the default everything starts at" before it. The same
//!   bit, read two ways, and reading it the wrong way is what made reopening
//!   the app start a download nobody had asked for.
//! - `paused` is a decision that only exists once there is something to pause.
//!   For a draft it is stored, ignored, and meaningless.
//!
//! This module reads those flags once and answers with the intent they encode.
//! Nothing here is a new source of truth — it is a lens over the stored row, so
//! there is no second state to migrate or keep in step. The workers ask what a
//! collection *is* rather than reassembling it from booleans, which is what
//! kept letting two of them disagree.

use crate::nexus::store::records::{StoredCollection, StoredImportEntry};

/// Whether this device should currently be moving bytes for a collection.
///
/// Only meaningful where there is a transfer to have an opinion about, which
/// is why it lives inside the variants that have one rather than beside them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Running,
    /// The person stopped it. Survives restarts, and outranks whatever the
    /// engine happens to be doing.
    Paused,
}

impl Activity {
    #[must_use]
    pub const fn of(paused: bool) -> Self {
        if paused { Self::Paused } else { Self::Running }
    }

    #[must_use]
    pub const fn is_paused(self) -> bool {
        matches!(self, Self::Paused)
    }
}

/// What one collection is, and what may therefore happen to it.
///
/// The variants are ordered as a collection moves through them. Every one is
/// reachable from the stored row alone, so a restart reconstructs the same
/// answer without depending on anything held in memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    /// The person's own files, chosen but never offered to anyone. Free to
    /// rename, add to, or abandon: nothing has been hashed or published.
    NativeDraft,
    /// Published from this device's own sources.
    NativePublished { activity: Activity },
    /// A torrent source whose contents are not known yet.
    TorrentResolving,
    /// Resolved, and waiting for the person to press Download.
    ///
    /// The distinction that matters most here: every resolved entry starts
    /// selected so the selection screen opens with something in it, and that
    /// default is *not* a request. Treating it as one is what made reopening
    /// the app begin a transfer that had only ever been inspected.
    TorrentAwaitingSelection,
    /// The person chose files and asked for them.
    TorrentRequested { activity: Activity },
}

impl Lifecycle {
    /// Reads the stored row as the intent it encodes.
    ///
    /// `import` is the collection's torrent source, where it has one, and
    /// `entries` its resolved file list. Both come from the same store read the
    /// caller was already doing.
    #[must_use]
    pub fn of(
        stored: &StoredCollection,
        import: Option<&str>,
        entries: &[StoredImportEntry],
    ) -> Self {
        let activity = Activity::of(stored.paused);
        if import.is_none() {
            return if stored.draft {
                Self::NativeDraft
            } else {
                Self::NativePublished { activity }
            };
        }
        if entries.is_empty() {
            return Self::TorrentResolving;
        }
        if stored.draft {
            Self::TorrentAwaitingSelection
        } else {
            Self::TorrentRequested { activity }
        }
    }

    /// Whether this device may move bytes for this collection at all.
    ///
    /// A draft has nothing to transfer and an unconfirmed selection has
    /// nothing the person has asked for, so both answer `false` however their
    /// entries happen to be flagged.
    #[must_use]
    pub const fn wants_transfer(&self) -> bool {
        matches!(
            self,
            Self::NativePublished {
                activity: Activity::Running
            } | Self::TorrentRequested {
                activity: Activity::Running
            }
        )
    }

    /// Whether the engine may be told to fetch this collection's files.
    ///
    /// Distinct from [`Self::wants_transfer`]: a paused request is still a
    /// request, so the engine is told what to fetch and then told to stop —
    /// which is what stops a resume briefly pulling a deselected file.
    #[must_use]
    pub const fn is_requested(&self) -> bool {
        matches!(
            self,
            Self::NativePublished { .. } | Self::TorrentRequested { .. }
        )
    }

    /// The person's transfer decision, where there is one to have.
    #[must_use]
    pub const fn activity(&self) -> Option<Activity> {
        match self {
            Self::NativePublished { activity } | Self::TorrentRequested { activity } => {
                Some(*activity)
            }
            Self::NativeDraft | Self::TorrentResolving | Self::TorrentAwaitingSelection => None,
        }
    }

    /// Whether the person has yet to confirm what this collection is.
    #[must_use]
    pub const fn is_draft(&self) -> bool {
        matches!(
            self,
            Self::NativeDraft | Self::TorrentResolving | Self::TorrentAwaitingSelection
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nexus::store::records::Role;

    fn stored(draft: bool, paused: bool) -> StoredCollection {
        StoredCollection {
            name: "Iceland".to_owned(),
            role: Role::Owner,
            content_key: [0; 32],
            media_path: String::new(),
            sources: Vec::new(),
            paused,
            on_disk_bytes: 0,
            substrate_handle: None,
            draft,
            started_at: None,
            completed_at: None,
        }
    }

    fn entry(selected: bool) -> StoredImportEntry {
        StoredImportEntry {
            label: "one.mkv".to_owned(),
            bytes: 10,
            selected,
            native_location: None,
        }
    }

    #[test]
    fn a_native_collection_is_a_draft_until_it_is_shared() {
        assert_eq!(
            Lifecycle::of(&stored(true, false), None, &[]),
            Lifecycle::NativeDraft
        );
        assert_eq!(
            Lifecycle::of(&stored(false, false), None, &[]),
            Lifecycle::NativePublished {
                activity: Activity::Running
            }
        );
        assert_eq!(
            Lifecycle::of(&stored(false, true), None, &[]),
            Lifecycle::NativePublished {
                activity: Activity::Paused
            }
        );
    }

    /// The bug this type exists to make unrepresentable. Every resolved entry
    /// starts selected, and before confirmation that default must not read as
    /// a request — otherwise merely opening the app starts the download.
    #[test]
    fn a_resolved_draft_wants_nothing_however_its_entries_are_flagged() {
        let source = Some("magnet:?xt=urn:btih:abc");
        let awaiting = Lifecycle::of(&stored(true, false), source, &[entry(true)]);

        assert_eq!(awaiting, Lifecycle::TorrentAwaitingSelection);
        assert!(!awaiting.wants_transfer(), "a default is not a request");
        assert!(!awaiting.is_requested());
        assert!(awaiting.is_draft());
        assert_eq!(awaiting.activity(), None, "nothing to pause yet");
    }

    #[test]
    fn confirming_the_selection_is_what_makes_it_a_request() {
        let source = Some("magnet:?xt=urn:btih:abc");

        let requested = Lifecycle::of(&stored(false, false), source, &[entry(true)]);
        assert_eq!(
            requested,
            Lifecycle::TorrentRequested {
                activity: Activity::Running
            }
        );
        assert!(requested.wants_transfer());
        assert!(requested.is_requested());
        assert!(!requested.is_draft());

        // A paused request is still a request: the engine is told what to
        // fetch, and then told to stop.
        let paused = Lifecycle::of(&stored(false, true), source, &[entry(true)]);
        assert!(!paused.wants_transfer(), "a person's pause outranks it");
        assert!(paused.is_requested(), "but the selection still applies");
        assert_eq!(paused.activity(), Some(Activity::Paused));
    }

    #[test]
    fn a_source_with_no_file_list_is_still_being_resolved() {
        let resolving = Lifecycle::of(&stored(true, false), Some("magnet:?xt=urn:btih:abc"), &[]);

        assert_eq!(resolving, Lifecycle::TorrentResolving);
        assert!(!resolving.wants_transfer());
        assert!(resolving.is_draft());
    }

    /// A pause recorded against something that cannot transfer is not a
    /// decision anyone made — it is a leftover bit. It must not resurface as
    /// one when the collection is later confirmed.
    #[test]
    fn a_pause_stored_on_a_draft_names_nothing() {
        assert_eq!(
            Lifecycle::of(&stored(true, true), None, &[]).activity(),
            None
        );
        assert_eq!(
            Lifecycle::of(
                &stored(true, true),
                Some("magnet:?xt=urn:btih:abc"),
                &[entry(true)]
            )
            .activity(),
            None
        );
    }
}
