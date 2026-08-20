//! What a collection is, once every earlier step is available.
//!
//! Steps 1 to 6 each built one piece: canonical formats, a device log that
//! says who a person's devices are, a revision chain that makes history
//! verifiable, sealing that reaches only authorized devices, a bus, and a
//! store. None of them is a feature on its own. This is where they become
//! one — creating a collection, adding media, publishing, and changing who
//! can read it.
//!
//! There is still no network here, and that is the point. A collection is a
//! set of signed objects; handing them to a peer is a separate problem, and
//! writing the workflows first means the transport in step 8 has nothing to
//! decide. Two cores in one process can exchange these objects by hand and
//! both verify, which is exactly what the demo does.
//!
//! What this module deliberately does not hold: any notion of a server, a
//! collaborator list the service maintains, or an invite secret. Membership
//! is declared in the signed revision (§7.6) and there is no second list to
//! disagree with it.

use portalis_nexus_protocol::{
    ContentKey, DEVICE_KEY_BYTES, Manifest, Revision, SHARE_ID_BYTES, new_message_id,
};
use thiserror::Error;

use crate::nexus::store::records::Role;

/// Names one collection. Sixteen bytes, generated once and never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CollectionId(pub [u8; SHARE_ID_BYTES]);

impl CollectionId {
    /// A fresh identifier. UUIDv7-shaped, so identifiers sort roughly by
    /// creation time and a store's key order is close to chronological.
    #[must_use]
    pub fn generate() -> Self {
        let id = new_message_id();
        let mut bytes = [0_u8; SHARE_ID_BYTES];
        let take = bytes.len().min(id.len());
        bytes[..take].copy_from_slice(&id[..take]);
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; SHARE_ID_BYTES] {
        &self.0
    }
}

/// A collection as this device holds it.
///
/// The content key is present for both roles: an owner generated it, a member
/// opened it out of a sealed envelope. Without it there is nothing to read, so
/// a collection that exists is a collection that can be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    pub role: Role,
    pub content_key: ContentKey,
    /// The highest revision verified, absent only between creating a
    /// collection and publishing its first revision.
    pub revision: Option<Revision>,
    /// The entries the current revision names.
    pub manifest: Manifest,
}

impl Collection {
    /// The revision number this collection is on, or 0 before the first.
    #[must_use]
    pub fn number(&self) -> u64 {
        self.revision.as_ref().map_or(0, |revision| revision.number)
    }

    /// Everyone the current revision names as a member, including the owner.
    #[must_use]
    pub fn members(&self) -> Vec<[u8; DEVICE_KEY_BYTES]> {
        self.revision.as_ref().map_or_else(Vec::new, |revision| {
            revision
                .members
                .iter()
                .map(|member| member.root_key)
                .collect()
        })
    }

    /// Whether this device may publish. Only the owner may, and only because
    /// a member's signature would not verify against the owner's device log
    /// anyway — this is a clear answer rather than a cryptographic one.
    #[must_use]
    pub const fn may_publish(&self) -> bool {
        matches!(self.role, Role::Owner)
    }
}

/// Why a collection operation did not happen.
#[derive(Debug, Error)]
pub enum CollectionError {
    #[error("only the owner of a collection may publish revisions of it")]
    NotTheOwner,
    #[error("this collection has no revision yet")]
    Unpublished,
    /// The bundle names a collection other than the one being received into.
    #[error("that publication belongs to a different collection")]
    WrongCollection,
    /// No sealed key in the bundle is addressed to this device. Either the
    /// owner does not consider us a member, or they sealed against a device
    /// log that predates this device.
    #[error("nothing in this publication is sealed to this device")]
    NotSealedToUs,
    #[error(transparent)]
    Chain(#[from] crate::nexus::crypto::ChainError),
    #[error(transparent)]
    Keys(#[from] crate::nexus::crypto::KeyError),
    #[error(transparent)]
    Manifest(#[from] portalis_nexus_protocol::ManifestError),
    #[error(transparent)]
    Sealed(#[from] portalis_nexus_protocol::SealedManifestError),
    #[error(transparent)]
    Entry(#[from] portalis_nexus_protocol::EntryError),
    #[error(transparent)]
    Store(#[from] crate::nexus::store::StoreError),
}

#[cfg(test)]
mod tests {
    use portalis_nexus_protocol::{Member, NO_PREVIOUS_REVISION, SIGNATURE_BYTES};

    use super::*;

    fn collection(role: Role, revision: Option<Revision>) -> Collection {
        Collection {
            id: CollectionId([1; SHARE_ID_BYTES]),
            name: "Iceland".to_owned(),
            role,
            content_key: [2; 32],
            revision,
            manifest: Manifest::default(),
        }
    }

    fn revision(members: &[u8]) -> Revision {
        Revision {
            collection_id: [1; SHARE_ID_BYTES],
            number: 4,
            previous_hash: [9; 32],
            manifest_hash: [3; 32],
            owner_root_key: [5; DEVICE_KEY_BYTES],
            at_unix_ns: 1,
            members: members
                .iter()
                .map(|&root| Member {
                    root_key: [root; DEVICE_KEY_BYTES],
                    device_log_hash: [root; 32],
                })
                .collect(),
            author_key: [5; DEVICE_KEY_BYTES],
            signature: [0; SIGNATURE_BYTES],
        }
    }

    #[test]
    fn an_unpublished_collection_is_at_revision_zero_with_no_members() {
        let collection = collection(Role::Owner, None);

        assert_eq!(collection.number(), 0);
        assert!(collection.members().is_empty());
        assert!(collection.may_publish());
    }

    #[test]
    fn a_published_collection_reports_its_number_and_membership() {
        let collection = collection(Role::Member, Some(revision(&[2, 3])));

        assert_eq!(collection.number(), 4);
        assert_eq!(
            collection.members(),
            vec![[2; DEVICE_KEY_BYTES], [3; DEVICE_KEY_BYTES]]
        );
        assert!(
            !collection.may_publish(),
            "a member holds the key but not the authority"
        );
    }

    #[test]
    fn a_first_revision_may_have_no_members_at_all() {
        let mut only_me = revision(&[]);
        only_me.number = 1;
        only_me.previous_hash = NO_PREVIOUS_REVISION;

        assert!(collection(Role::Owner, Some(only_me)).members().is_empty());
    }

    #[test]
    fn identifiers_are_unique_and_sixteen_bytes() {
        let first = CollectionId::generate();

        assert_ne!(first, CollectionId::generate());
        assert_eq!(first.as_bytes().len(), SHARE_ID_BYTES);
        assert_eq!(first, CollectionId(*first.as_bytes()));
    }
}
