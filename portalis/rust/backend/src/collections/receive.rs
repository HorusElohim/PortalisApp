//! Taking a publication from someone else and deciding whether to believe it.
//!
//! Receiving is the side that matters. Publishing is bookkeeping over objects
//! this device already trusts; receiving is where every guarantee the earlier
//! steps built is actually spent, against a bundle that may have come from a
//! hostile service, a compromised device, or a peer replaying something old.
//!
//! The order is not arbitrary — it runs cheapest and most decisive first:
//!
//! 1. **The revision**, against the owner's device log and the highest
//!    revision already held. This catches a forged signature, a revoked
//!    author, a rollback and a fork before anything is decrypted.
//! 2. **The content key**, from the one envelope sealed to this device. If
//!    there is none, we are not a member of this revision and there is
//!    nothing further to do.
//! 3. **The manifest**, opened under that key and checked against the hash the
//!    revision signed for. A service that swaps the manifest fails here even
//!    though the revision is genuine.
//! 4. **The entries**, each opened under its own context.
//!
//! Nothing is written to the store until all of it passes, so a rejected
//! publication leaves no trace to reason about later.

use crate::crypto::{ChainStore, Continuity, open_content_key, verify_revision};
use portalis_nexus_protocol::DeviceLog;
use portalis_nexus_protocol::{
    DEVICE_ID_BYTES, EntryContext, INFO_HASH_BYTES, ManifestContext, open_entry, open_manifest,
};

use super::model::{Collection, CollectionError, CollectionId};
use super::publish::Publication;
use crate::store::records::Role;

/// A publication that verified, and what came out of it.
#[derive(Clone, Debug)]
pub struct Received {
    /// The collection as it now stands on this device.
    pub collection: Collection,
    /// Each entry's descriptor, decrypted.
    pub descriptors: Vec<([u8; INFO_HASH_BYTES], Vec<u8>)>,
    /// Members whose device log has moved since the owner sealed to them, so
    /// a re-seal is owed. Not a failure — see step 3.
    pub reseal_owed: Vec<[u8; 32]>,
}

/// This device's half of the sealing, kept together so a caller cannot pass
/// one device's identifier with another's secret.
#[derive(Clone, Copy, Debug)]
pub struct ReceivingDevice {
    pub device_id: [u8; DEVICE_ID_BYTES],
    pub encryption_secret_key: [u8; DEVICE_ID_BYTES],
}

/// Verifies a publication and returns what it contained.
///
/// `held` is what this device already knows about the collection, if
/// anything: `None` for a collection being joined, `Some` for one being
/// updated. It supplies the name, which is local, and nothing that affects
/// verification — everything decisive comes from the bundle and the store.
///
/// `continuity` says whether this is a first sighting to be trusted as a
/// baseline. Accepting an invitation to a collection published for months is
/// [`Continuity::Join`]; everything after it is [`Continuity::Strict`].
///
/// # Errors
///
/// Returns [`CollectionError`] for any step above that fails, without writing
/// anything.
pub async fn receive<S: ChainStore>(
    publication: &Publication,
    owner_log: &DeviceLog,
    device: &ReceivingDevice,
    chain: &S,
    held: Option<&Collection>,
    name: &str,
    continuity: Continuity,
) -> Result<Received, CollectionError> {
    let collection_id = CollectionId(publication.revision.collection_id);
    if held.is_some_and(|held| held.id != collection_id) {
        return Err(CollectionError::WrongCollection);
    }

    // 1. Is this revision one we should believe, and does it follow what we
    //    already hold? Checked before a single byte is decrypted.
    let accepted = verify_revision(
        &publication.revision,
        owner_log,
        chain,
        Some(publication.revision.manifest_hash),
        &[],
        continuity,
    )
    .await?;

    // 2. Are we a member of it? Being absent is an answer, not an error in
    //    the cryptographic sense — but there is nothing further we can do.
    let sealed = publication
        .keys
        .iter()
        .find(|sealed| sealed.recipient_device_id == device.device_id)
        .ok_or(CollectionError::NotSealedToUs)?;
    let content_key = open_content_key(
        &device.encryption_secret_key,
        collection_id.0,
        device.device_id,
        &sealed.envelope,
    )?;

    // 3. The manifest, bound to this collection and revision by its context,
    //    and checked against the hash the revision signed for.
    let context = ManifestContext {
        collection_id: collection_id.0,
        revision: publication.revision.number,
        manifest_hash: publication.revision.manifest_hash,
    };
    // Opening it is the check: the context carries the hash the revision
    // signed for, and `open_manifest` refuses anything whose content hash
    // disagrees. Repeating the comparison here would be a second answer to a
    // question already answered.
    let manifest = open_manifest(&content_key, &context, &publication.sealed_manifest)?;

    // 4. The entries. Each is bound to its own info hash, so one cannot be
    //    presented as another.
    let mut descriptors = Vec::with_capacity(publication.entries.len());
    for entry in &publication.entries {
        let context = EntryContext {
            collection_id: collection_id.0,
            info_hash: entry.info_hash,
        };
        descriptors.push((
            entry.info_hash,
            open_entry(&content_key, &context, &entry.payload)?,
        ));
    }

    Ok(Received {
        collection: Collection {
            id: collection_id,
            name: held.map_or_else(|| name.to_owned(), |held| held.name.clone()),
            role: held.map_or(Role::Member, |held| held.role),
            content_key,
            revision: Some(publication.revision.clone()),
            manifest,
        },
        descriptors,
        reseal_owed: accepted.reseal_owed,
    })
}

// Writing a received collection into the store is deliberately not here.
// Verifying is pure — it needs no file, and a caller that only wants to know
// whether a bundle is good should not have to supply a store to find out.
// Persisting has one honest shape, a single transaction so a crash cannot
// leave a collection with no revision, and it belongs with the caller that
// has both. Step 9 wires it.

#[cfg(test)]
mod tests {
    use crate::crypto::{Continuity, MemoryChainStore};

    use super::super::members::remove_members;
    use super::super::publish::publish;
    use super::super::publish::tests::{NOW, Person, descriptors, owned};
    use super::*;

    fn device(person: &Person) -> ReceivingDevice {
        ReceivingDevice {
            device_id: person.device_id(),
            encryption_secret_key: person.secret.to_bytes(),
        }
    }

    #[tokio::test]
    async fn a_member_verifies_a_publication_and_gets_everything_in_it() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let (_, publication) = publish(
            &owned(&ada),
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");
        let chain = MemoryChainStore::default();

        let received = receive(
            &publication,
            &ada.log(),
            &device(&mira),
            &chain,
            None,
            "Iceland",
            Continuity::Strict,
        )
        .await
        .expect("verifies");

        assert_eq!(received.collection.number(), 1);
        assert_eq!(received.collection.role, Role::Member);
        assert_eq!(received.collection.name, "Iceland");
        assert_eq!(received.descriptors, descriptors());
        assert_eq!(received.collection.manifest.entries().len(), 1);
        assert!(received.reseal_owed.is_empty());
    }

    /// Following, rather than joining: a member who already holds one
    /// revision takes the next. The name is theirs and stays theirs, because
    /// what a collection is called is local and never travels.
    #[tokio::test]
    async fn a_member_follows_the_chain_and_keeps_their_own_name_for_it() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let collection = owned(&ada);
        let (state, first) = publish(
            &collection,
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");
        let (_, second) = publish(
            &state,
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW + 1,
        )
        .expect("publishes again");
        let chain = MemoryChainStore::default();

        let mut held = receive(
            &first,
            &ada.log(),
            &device(&mira),
            &chain,
            None,
            "what Mira calls it",
            Continuity::Strict,
        )
        .await
        .expect("the first");

        held = receive(
            &second,
            &ada.log(),
            &device(&mira),
            &chain,
            Some(&held.collection),
            "ignored, because she already named it",
            Continuity::Strict,
        )
        .await
        .expect("the second follows the first");

        assert_eq!(held.collection.number(), 2);
        assert_eq!(held.collection.name, "what Mira calls it");
        assert_eq!(held.collection.role, Role::Member);
    }

    /// The one case a member cannot fix: they are simply not in it.
    #[tokio::test]
    async fn someone_who_was_not_sealed_to_is_told_so() {
        let (ada, stranger) = (Person::new(1), Person::new(9));
        let (_, publication) = publish(&owned(&ada), &ada, &[ada.recipient()], &descriptors(), NOW)
            .expect("publishes");

        assert!(matches!(
            receive(
                &publication,
                &ada.log(),
                &device(&stranger),
                &MemoryChainStore::default(),
                None,
                "Iceland",
                Continuity::Strict,
            )
            .await,
            Err(CollectionError::NotSealedToUs)
        ));
    }

    /// A service that keeps the genuine revision and swaps the manifest for
    /// another it happens to hold.
    #[tokio::test]
    async fn a_swapped_manifest_is_refused_even_though_the_revision_is_genuine() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let mut other = owned(&ada);
        super::super::publish::add_entry(&mut other, &ada, [2; 20], "two.jpg", None, NOW)
            .expect("adds");

        let (_, mut publication) = publish(
            &owned(&ada),
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");
        let (_, decoy) = publish(
            &other,
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes another");
        publication.sealed_manifest = decoy.sealed_manifest;

        assert!(
            matches!(
                receive(
                    &publication,
                    &ada.log(),
                    &device(&mira),
                    &MemoryChainStore::default(),
                    None,
                    "Iceland",
                    Continuity::Strict,
                )
                .await,
                Err(CollectionError::Sealed(_))
            ),
            "a manifest sealed for another revision does not even open"
        );
    }

    #[tokio::test]
    async fn a_publication_for_another_collection_is_refused() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let (_, publication) = publish(
            &owned(&ada),
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");
        let (elsewhere, _) = publish(
            &owned(&ada),
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");

        assert!(matches!(
            receive(
                &publication,
                &ada.log(),
                &device(&mira),
                &MemoryChainStore::default(),
                Some(&elsewhere),
                "Iceland",
                Continuity::Strict,
            )
            .await,
            Err(CollectionError::WrongCollection)
        ));
    }

    /// Rotation, from the receiving side: the removed member is not refused
    /// by a rule, they simply have no envelope.
    #[tokio::test]
    async fn a_removed_member_finds_nothing_sealed_to_them() {
        let (ada, mira, jonas) = (Person::new(1), Person::new(2), Person::new(3));
        let (state, first) = publish(
            &owned(&ada),
            &ada,
            &[ada.recipient(), mira.recipient(), jonas.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");
        let chain = MemoryChainStore::default();
        let held = receive(
            &first,
            &ada.log(),
            &device(&jonas),
            &chain,
            None,
            "Iceland",
            Continuity::Strict,
        )
        .await
        .expect("Jonas is a member of revision 1");

        let (_, second) = remove_members(
            &state,
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW + 1,
        )
        .expect("rotates");

        assert!(matches!(
            receive(
                &second,
                &ada.log(),
                &device(&jonas),
                &chain,
                Some(&held.collection),
                "Iceland",
                Continuity::Strict,
            )
            .await,
            Err(CollectionError::NotSealedToUs)
        ));
        // And what he already holds still opens, because nothing can take a
        // key back.
        assert_eq!(held.descriptors, descriptors());
    }

    /// An envelope addressed to this device that does not open. A service
    /// cannot forge one, but it can hand over a damaged or substituted
    /// envelope, and the answer must be a refusal rather than a panic.
    #[tokio::test]
    async fn an_envelope_that_does_not_open_is_refused() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let (_, mut publication) = publish(
            &owned(&ada),
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");

        let mine = publication
            .keys
            .iter_mut()
            .find(|sealed| sealed.recipient_device_id == mira.device_id())
            .expect("an envelope for Mira");
        let last = mine.envelope.ciphertext.len() - 1;
        mine.envelope.ciphertext[last] ^= 1;

        assert!(matches!(
            receive(
                &publication,
                &ada.log(),
                &device(&mira),
                &MemoryChainStore::default(),
                None,
                "Iceland",
                Continuity::Strict,
            )
            .await,
            Err(CollectionError::Keys(_))
        ));
    }
}
