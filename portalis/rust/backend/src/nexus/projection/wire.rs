//! The exact words the app-facing contract uses.
//!
//! Every enum below crosses the bridge as a string, and Flutter compares those
//! strings literally. That makes the spelling a public API — but it used to be
//! produced by `format!("{:?}", …)`, which means the contract was whatever
//! `#[derive(Debug)]` happened to print. Renaming a variant silently changed
//! the wire, adding one silently added a word Flutter had never heard of, and
//! neither failed to compile on either side of the FFI.
//!
//! It really did rot: three comparisons in the Flutter tree tested for
//! `'downloading'` and `'importing'`, which no version of this code has ever
//! emitted. They were simply always false, and nobody found out.
//!
//! So the mapping is written out by hand, exhaustively. A new variant fails to
//! compile here until somebody chooses its word, which is the one moment the
//! decision is actually being made. `parse` exists so the round trip can be
//! tested — a word that cannot be read back is a word Flutter cannot match.

use crate::nexus::projection::state::{Connectivity, Friendship, Nature, Role, Status};

/// One value's spelling on the wire, and how to read it back.
///
/// Implemented rather than derived: `Debug` is a debugging aid whose output
/// Rust is free to change, and this is a contract that must not move under a
/// shipped app.
pub(crate) trait Wire: Sized {
    /// The word Flutter compares against.
    fn wire(&self) -> &'static str;

    /// The value that word names, or `None` if it names nothing here.
    ///
    /// The reverse direction exists because a word Flutter cannot match is a
    /// word this side must not send. [`emits`] uses it to assert exactly that,
    /// and the tests use it to prove every spelling survives a round trip.
    fn parse(word: &str) -> Option<Self>;
}

/// Whether `word` is one this contract actually produces.
///
/// Used at the bridge to catch a value whose spelling has drifted out of the
/// set Flutter knows. It is a cheap match, and it runs where the alternative
/// is a screen quietly answering `false` to every question it asks about a
/// collection.
pub(crate) fn emits<T: Wire>(word: &str) -> bool {
    T::parse(word).is_some()
}

impl Wire for Status {
    fn wire(&self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::Seeding => "Seeding",
            Self::Paused => "Paused",
            Self::Draft => "Draft",
            Self::Preparing => "Preparing",
            Self::Downloading => "Downloading",
            Self::Updating => "Updating",
            Self::WaitingForOwner => "WaitingForOwner",
            Self::AccessRemoved => "AccessRemoved",
            Self::NeedsNewerVersion => "NeedsNewerVersion",
            // The reason is deliberately not in the word. A status is what the
            // interface switches on; why verification failed is a detail it
            // renders separately, and folding it in would make every arm of
            // that switch a prefix match.
            Self::CannotVerify(_) => "CannotVerify",
            Self::ConflictingHistory => "ConflictingHistory",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "Available" => Self::Available,
            "Seeding" => Self::Seeding,
            "Paused" => Self::Paused,
            "Draft" => Self::Draft,
            "Preparing" => Self::Preparing,
            "Downloading" => Self::Downloading,
            "Updating" => Self::Updating,
            "WaitingForOwner" => Self::WaitingForOwner,
            "AccessRemoved" => Self::AccessRemoved,
            "NeedsNewerVersion" => Self::NeedsNewerVersion,
            "ConflictingHistory" => Self::ConflictingHistory,
            // Not parsed: the word carries no reason, so reading it back would
            // have to invent one. Nothing needs to.
            _ => return None,
        })
    }
}

impl Wire for Nature {
    fn wire(&self) -> &'static str {
        match self {
            Self::Native => "Native",
            Self::Torrent => "Torrent",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "Native" => Self::Native,
            "Torrent" => Self::Torrent,
            _ => return None,
        })
    }
}

impl Wire for Role {
    fn wire(&self) -> &'static str {
        match self {
            Self::Owner => "Owner",
            Self::Member => "Member",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "Owner" => Self::Owner,
            "Member" => Self::Member,
            _ => return None,
        })
    }
}

impl Wire for Friendship {
    fn wire(&self) -> &'static str {
        match self {
            Self::Requested => "Requested",
            Self::Pending => "Pending",
            Self::Accepted => "Accepted",
            Self::Blocked => "Blocked",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "Requested" => Self::Requested,
            "Pending" => Self::Pending,
            "Accepted" => Self::Accepted,
            "Blocked" => Self::Blocked,
            _ => return None,
        })
    }
}

impl Wire for Connectivity {
    fn wire(&self) -> &'static str {
        match self {
            Self::LocalOnly => "LocalOnly",
            Self::Connecting => "Connecting",
            // The security details travel in `AppContact::reachable`, per
            // contact, where they describe something a person can act on. A
            // connectivity word is the one-line summary above that.
            Self::Online(_) => "Online",
            Self::Degraded { .. } => "Degraded",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "LocalOnly" => Self::LocalOnly,
            "Connecting" => Self::Connecting,
            // Not parsed for the same reason as `CannotVerify`: the word drops
            // the payload, so reconstructing the value would mean inventing it.
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nexus::projection::state::VerifyFailure;

    /// The words Flutter actually compares against, listed here so a change to
    /// any of them has to be made twice on purpose.
    ///
    /// This is the whole point of the module: these strings are a shipped
    /// contract, and a test that restated them by calling `wire()` would agree
    /// with any rename rather than catching one.
    #[test]
    fn every_status_keeps_the_word_the_interface_matches_on() {
        let contract = [
            (Status::Available, "Available"),
            (Status::Seeding, "Seeding"),
            (Status::Paused, "Paused"),
            (Status::Draft, "Draft"),
            (Status::Preparing, "Preparing"),
            (Status::Downloading, "Downloading"),
            (Status::Updating, "Updating"),
            (Status::WaitingForOwner, "WaitingForOwner"),
            (Status::AccessRemoved, "AccessRemoved"),
            (Status::NeedsNewerVersion, "NeedsNewerVersion"),
            (
                Status::CannotVerify(VerifyFailure::Signature),
                "CannotVerify",
            ),
            (Status::ConflictingHistory, "ConflictingHistory"),
        ];

        for (status, word) in contract {
            assert_eq!(status.wire(), word, "{status:?} changed its wire word");
        }
    }

    /// Every reason reads as the same word, so the interface can switch on the
    /// status without matching prefixes.
    #[test]
    fn a_verification_failure_is_one_word_whatever_the_reason() {
        for reason in [
            VerifyFailure::Signature,
            VerifyFailure::Rollback,
            VerifyFailure::BrokenChain,
            VerifyFailure::ContentMismatch,
        ] {
            assert_eq!(Status::CannotVerify(reason).wire(), "CannotVerify");
        }
    }

    #[test]
    fn the_other_contracts_keep_their_words_too() {
        assert_eq!(Nature::Native.wire(), "Native");
        assert_eq!(Nature::Torrent.wire(), "Torrent");
        assert_eq!(Role::Owner.wire(), "Owner");
        assert_eq!(Role::Member.wire(), "Member");
        assert_eq!(Friendship::Requested.wire(), "Requested");
        assert_eq!(Friendship::Pending.wire(), "Pending");
        assert_eq!(Friendship::Accepted.wire(), "Accepted");
        assert_eq!(Friendship::Blocked.wire(), "Blocked");
        assert_eq!(Connectivity::LocalOnly.wire(), "LocalOnly");
        assert_eq!(Connectivity::Connecting.wire(), "Connecting");
        assert_eq!(
            Connectivity::Degraded { since_unix_ns: 1 }.wire(),
            "Degraded"
        );
    }

    /// A word that cannot be read back is a word nothing can match on.
    #[test]
    fn every_payload_free_word_survives_a_round_trip() {
        for status in [
            Status::Available,
            Status::Seeding,
            Status::Paused,
            Status::Draft,
            Status::Preparing,
            Status::Downloading,
            Status::Updating,
            Status::WaitingForOwner,
            Status::AccessRemoved,
            Status::NeedsNewerVersion,
            Status::ConflictingHistory,
        ] {
            assert_eq!(Status::parse(status.wire()), Some(status));
        }
        for nature in [Nature::Native, Nature::Torrent] {
            assert_eq!(Nature::parse(nature.wire()), Some(nature));
        }
        for role in [Role::Owner, Role::Member] {
            assert_eq!(Role::parse(role.wire()), Some(role));
        }
        for friendship in [
            Friendship::Requested,
            Friendship::Pending,
            Friendship::Accepted,
            Friendship::Blocked,
        ] {
            assert_eq!(Friendship::parse(friendship.wire()), Some(friendship));
        }
    }

    /// The bug this module exists to prevent. Both of these were live in the
    /// Flutter tree and matched nothing.
    #[test]
    fn a_word_the_contract_does_not_use_is_refused_rather_than_guessed() {
        assert_eq!(Status::parse("downloading"), None, "case matters");
        assert_eq!(Status::parse("importing"), None, "never a status");
        assert_eq!(Status::parse(""), None);
        assert_eq!(Nature::parse("torrent"), None);
    }
}
