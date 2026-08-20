//! Adding and removing members, which is the same operation twice.
//!
//! There is no membership call. Membership is declared in the signed revision
//! (§7.6), so changing it means publishing a new one — and the whole of that
//! machinery already exists in [`super::publish`]. What lives here is the one
//! thing removal needs that addition does not: **rotation**.
//!
//! Removing someone means the next revision must be closed to them. Their
//! copy of the old key cannot be recalled — no protocol can reach into a
//! device and take something back — so what rotation achieves is that
//! everything published afterwards is sealed under a key they never had. That
//! is the honest guarantee, and it is worth stating plainly rather than
//! letting "removed" imply more than it does.
//!
//! Addition needs no rotation. A new member receives the current key, which
//! they are being given deliberately, and the revisions they could not read
//! before remain unreadable only in the sense that they were never sent them.

use crate::nexus::crypto::{Recipient, generate_content_key};

use super::model::{Collection, CollectionError};
use super::publish::{Author, Publication, publish};

/// Publishes a revision whose membership is exactly `members`.
///
/// Adding and removing are both this call with a different set — the
/// difference is only whether the content key is rotated first.
///
/// # Errors
///
/// Returns [`CollectionError`] when this device does not own the collection,
/// or a member has no authorized device to seal to.
pub fn set_members(
    collection: &Collection,
    author: &impl Author,
    members: &[Recipient],
    descriptors: &[([u8; 20], Vec<u8>)],
    at_unix_ns: u64,
) -> Result<(Collection, Publication), CollectionError> {
    publish(collection, author, members, descriptors, at_unix_ns)
}

/// Removes members by publishing a revision under a **new** content key,
/// sealed only to those who remain.
///
/// Every descriptor is re-sealed under the new key, which is why they are
/// required here: a revision whose manifest opens but whose entries do not
/// would leave a member able to list what they cannot fetch.
///
/// # Errors
///
/// Returns [`CollectionError`] when this device does not own the collection,
/// or a remaining member has no authorized device to seal to.
pub fn remove_members(
    collection: &Collection,
    author: &impl Author,
    remaining: &[Recipient],
    descriptors: &[([u8; 20], Vec<u8>)],
    at_unix_ns: u64,
) -> Result<(Collection, Publication), CollectionError> {
    let rotated = Collection {
        content_key: generate_content_key(),
        ..collection.clone()
    };
    publish(&rotated, author, remaining, descriptors, at_unix_ns)
}

#[cfg(test)]
mod tests {
    use super::super::publish::tests::{NOW, Person, descriptors, owned};
    use super::*;

    #[test]
    fn adding_a_member_keeps_the_key_and_widens_the_membership() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let collection = owned(&ada);

        let (with_mira, publication) = set_members(
            &collection,
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("adds");

        assert_eq!(publication.revision.members.len(), 2);
        assert_eq!(
            with_mira.content_key, collection.content_key,
            "a new member is given the key that already exists"
        );
    }

    #[test]
    fn removing_a_member_rotates_the_key_and_narrows_the_membership() {
        let (ada, mira, jonas) = (Person::new(1), Person::new(2), Person::new(3));
        let (shared, _) = set_members(
            &owned(&ada),
            &ada,
            &[ada.recipient(), mira.recipient(), jonas.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");

        let (rotated, publication) = remove_members(
            &shared,
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW + 1,
        )
        .expect("removes");

        assert_eq!(publication.revision.members.len(), 2);
        assert_ne!(
            rotated.content_key, shared.content_key,
            "a removal is a new key, not a shorter list"
        );
        assert!(
            !publication
                .revision
                .members
                .iter()
                .any(|member| member.root_key == jonas.public_key()),
            "and the revision does not name them"
        );
    }

    /// Rotation re-seals every descriptor, or a remaining member could list
    /// entries they cannot fetch.
    #[test]
    fn a_rotation_reseals_the_entry_payloads_too() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let (shared, before) = set_members(
            &owned(&ada),
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");

        let (_, after) = remove_members(&shared, &ada, &[ada.recipient()], &descriptors(), NOW + 1)
            .expect("removes");

        assert_eq!(after.entries.len(), before.entries.len());
        assert_ne!(
            after.entries[0].payload, before.entries[0].payload,
            "sealed under the new key"
        );
    }

    #[test]
    fn only_an_owner_may_change_the_membership() {
        let ada = Person::new(1);
        let member = Collection {
            role: crate::nexus::store::records::Role::Member,
            ..owned(&ada)
        };

        assert!(matches!(
            set_members(&member, &ada, &[ada.recipient()], &descriptors(), NOW),
            Err(CollectionError::NotTheOwner)
        ));
        assert!(matches!(
            remove_members(&member, &ada, &[ada.recipient()], &descriptors(), NOW),
            Err(CollectionError::NotTheOwner)
        ));
    }
}
