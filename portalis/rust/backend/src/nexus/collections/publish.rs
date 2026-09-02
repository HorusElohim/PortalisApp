//! Creating a collection, adding to it, and producing a revision.
//!
//! Publishing produces a [`Publication`]: every object a peer needs and
//! nothing else. It is deliberately a value rather than a side effect on a
//! socket, because the same bundle goes to a peer over QUIC in step 8, to the
//! service, or across a function call in a test — and none of those should
//! change what is produced.
//!
//! Everything in a publication is either signed or sealed. The revision is
//! signed by an owner device; the manifest and every entry payload are sealed
//! under the content key; the content key itself is sealed once per authorized
//! device. A service that holds all of it learns the collection's identifier,
//! its size, and who its members are — and nothing about what is in it.

use crate::nexus::crypto::{Recipient, Sealing, seal_content_key};
use portalis_nexus_protocol::{
    DEVICE_KEY_BYTES, EntryContext, INFO_HASH_BYTES, Manifest, ManifestEntry, NO_PREVIOUS_REVISION,
    Revision, RevisionHash, SIGNATURE_BYTES, seal_entry, seal_manifest,
};

use super::model::{Collection, CollectionError, CollectionId};
use crate::nexus::store::records::Role;

/// Signs whatever it is given, without handing the key to the caller.
///
/// A trait rather than a key because a device's private key may live in a
/// keychain that never releases it (§12), and the workflows must not be
/// written in a way that assumes otherwise.
pub trait Author {
    /// This device's public signing key, which a revision names as its author.
    fn public_key(&self) -> [u8; DEVICE_KEY_BYTES];
    /// Signs a canonical payload.
    fn sign(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES];
}

impl Author for crate::nexus::domain::identity::DeviceIdentity {
    fn public_key(&self) -> [u8; DEVICE_KEY_BYTES] {
        self.public_key()
    }

    fn sign(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES] {
        self.sign(payload).to_bytes()
    }
}

/// One entry's `.torrent`, sealed for the members of one collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedEntryPayload {
    pub info_hash: [u8; INFO_HASH_BYTES],
    pub payload: Vec<u8>,
}

/// Everything a peer needs to receive one revision, and nothing more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    /// Signed by an owner device, naming the manifest and the membership.
    pub revision: Revision,
    /// The manifest, encrypted under the content key.
    pub sealed_manifest: Vec<u8>,
    /// Each entry's descriptor, encrypted under the same key.
    pub entries: Vec<SealedEntryPayload>,
    /// The content key, sealed once per device the members' logs authorize.
    pub keys: Vec<crate::nexus::crypto::SealedFor>,
}

/// Starts a collection this device owns.
///
/// The content key is generated here and never leaves except sealed. Nothing
/// is published yet: a collection with no entries has nothing to say.
#[must_use]
pub fn create(
    name: impl Into<String>,
    content_key: portalis_nexus_protocol::ContentKey,
) -> Collection {
    Collection {
        id: CollectionId::generate(),
        name: name.into(),
        role: Role::Owner,
        content_key,
        revision: None,
        manifest: Manifest::default(),
    }
}

/// Adds one signed entry to what the next revision will contain.
///
/// The entry is signed by the device adding it, so a manifest says who added
/// each item rather than only that the owner assembled it.
///
/// # Errors
///
/// Returns [`CollectionError`] when this device does not own the collection,
/// or the entry would make the manifest non-canonical — a duplicate info hash,
/// a name that is not normalized, or too many entries.
pub fn add_entry(
    collection: &mut Collection,
    author: &impl Author,
    info_hash: [u8; INFO_HASH_BYTES],
    name: impl Into<String>,
    thumbnail_hash: Option<[u8; 32]>,
    at_unix_ns: u64,
) -> Result<(), CollectionError> {
    if !collection.may_publish() {
        return Err(CollectionError::NotTheOwner);
    }

    let mut entry = ManifestEntry {
        info_hash,
        name: name.into(),
        thumbnail_hash,
        author_public_key: author.public_key(),
        added_at_unix_ns: at_unix_ns,
        signature: [0; SIGNATURE_BYTES],
    };
    entry.signature = author.sign(&entry.signing_payload());

    let mut entries: Vec<_> = collection.manifest.entries().to_vec();
    entries.push(entry);
    collection.manifest = Manifest::new(entries)?;
    Ok(())
}

/// Produces the next revision and everything sealed under it.
///
/// `recipients` is the membership: every person this revision is for,
/// including the owner, each at a device log that has been brought up to date.
/// Passing a different set is how a member is added or removed — there is no
/// separate membership call, because membership *is* the revision (§7.6).
///
/// `descriptors` supplies the `.torrent` for each entry, so this function
/// never touches a file. An entry with no descriptor is simply not carried in
/// this bundle; the manifest still names it, and a peer will ask.
///
/// # Errors
///
/// Returns [`CollectionError`] when this device does not own the collection,
/// or a recipient has no authorized device to seal to.
pub fn publish(
    collection: &Collection,
    author: &impl Author,
    recipients: &[Recipient],
    descriptors: &[([u8; INFO_HASH_BYTES], Vec<u8>)],
    at_unix_ns: u64,
) -> Result<(Collection, Publication), CollectionError> {
    if !collection.may_publish() {
        return Err(CollectionError::NotTheOwner);
    }

    // The membership comes back from the sealing rather than going into it,
    // so a revision cannot claim to have sealed against a log state that was
    // not used. Step 4 returns it for exactly this.
    let Sealing { envelopes, members } =
        seal_content_key(&collection.content_key, collection.id.0, recipients)?;

    let manifest_hash = collection.manifest.hash();
    let (number, previous_hash) = match &collection.revision {
        None => (1, NO_PREVIOUS_REVISION),
        Some(current) => (current.number + 1, current.hash()),
    };

    let mut revision = Revision {
        collection_id: collection.id.0,
        number,
        previous_hash,
        manifest_hash,
        owner_root_key: owner_root_key(collection, author),
        at_unix_ns,
        members,
        author_key: author.public_key(),
        signature: [0; SIGNATURE_BYTES],
    };
    revision.signature = author.sign(&revision.signing_payload());

    let sealed_manifest = seal_manifest(
        &collection.content_key,
        collection.id.0,
        number,
        &collection.manifest,
    );
    let entries = descriptors
        .iter()
        .map(|(info_hash, descriptor)| {
            let context = EntryContext {
                collection_id: collection.id.0,
                info_hash: *info_hash,
            };
            Ok(SealedEntryPayload {
                info_hash: *info_hash,
                payload: seal_entry(&collection.content_key, &context, descriptor)?,
            })
        })
        .collect::<Result<Vec<_>, CollectionError>>()?;

    let published = Collection {
        revision: Some(revision.clone()),
        ..collection.clone()
    };
    Ok((
        published,
        Publication {
            revision,
            sealed_manifest,
            entries,
            keys: envelopes,
        },
    ))
}

/// The root key a revision answers to.
///
/// Every revision of a collection names the same owner root, whichever of the
/// owner's devices signed it — that is what lets an owner link a device and
/// keep publishing. The first revision fixes it; later ones repeat it.
fn owner_root_key(collection: &Collection, author: &impl Author) -> [u8; DEVICE_KEY_BYTES] {
    collection
        .revision
        .as_ref()
        .map_or_else(|| author.public_key(), |current| current.owner_root_key)
}

/// The hash the next revision must name, or `None` before the first.
#[must_use]
pub fn head(collection: &Collection) -> Option<RevisionHash> {
    collection.revision.as_ref().map(Revision::hash)
}

#[cfg(test)]
pub(crate) mod tests {
    use ed25519_dalek::{Signer, SigningKey};
    use portalis_nexus_protocol::{
        Action, DeviceLog, ENCRYPTION_KEY_BYTES, LogEntry, NO_PREVIOUS_ENTRY, derive_device_id,
    };
    use x25519_dalek::{PublicKey, StaticSecret};

    use super::*;

    pub(crate) const NOW: u64 = 1_700_000_000_000_000_000;

    /// One person, one device, and the log that says so. Shared with the
    /// receive and members tests, which need the same cast.
    pub(crate) struct Person {
        pub(crate) signing: SigningKey,
        pub(crate) secret: StaticSecret,
        pub(crate) entries: Vec<LogEntry>,
    }

    impl Person {
        pub(crate) fn new(seed: u8) -> Self {
            Self::from_signing_key(SigningKey::from_bytes(&[seed; 32]))
        }

        /// Builds a person around an existing signing key — the seam a
        /// caller uses to make the fixture *be* a specific already-loaded
        /// identity (e.g. this device's own), rather than an arbitrary one.
        pub(crate) fn from_signing_key(signing: SigningKey) -> Self {
            // Deterministic from the signing key rather than truly random:
            // fixtures need a valid, distinct encryption key, not entropy —
            // and determinism keeps this call site free of an extra RNG
            // dependency edge. XOR-folding the 32-byte signing key against a
            // fixed distinguisher is enough to make it differ from the
            // signing key itself while staying a valid x25519 scalar input.
            let signing_bytes = signing.to_bytes();
            let mut secret_bytes = [0_u8; ENCRYPTION_KEY_BYTES];
            for (index, byte) in secret_bytes.iter_mut().enumerate() {
                *byte = signing_bytes[index] ^ 0x5a;
            }
            let secret = StaticSecret::from(secret_bytes);
            let public = signing.verifying_key().to_bytes();

            let mut root = LogEntry {
                root_key: public,
                sequence: 1,
                previous_hash: NO_PREVIOUS_ENTRY,
                action: Action::Enrol,
                subject_signing_key: public,
                subject_encryption_key: *PublicKey::from(&secret).as_bytes(),
                at_unix_ns: NOW,
                author_key: public,
                signature: [0; SIGNATURE_BYTES],
            };
            root.signature = signing.sign(&root.signing_payload()).to_bytes();

            Self {
                signing,
                secret,
                entries: vec![root],
            }
        }

        pub(crate) fn log(&self) -> DeviceLog {
            DeviceLog::replay(&self.entries).expect("a valid log")
        }

        pub(crate) fn recipient(&self) -> Recipient {
            Recipient::current(&self.log(), &self.entries).expect("a recipient")
        }

        pub(crate) fn device_id(&self) -> [u8; 32] {
            derive_device_id(&self.signing.verifying_key().to_bytes())
        }
    }

    impl Author for Person {
        fn public_key(&self) -> [u8; DEVICE_KEY_BYTES] {
            self.signing.verifying_key().to_bytes()
        }

        fn sign(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES] {
            self.signing.sign(payload).to_bytes()
        }
    }

    pub(crate) fn owned(author: &Person) -> Collection {
        let mut collection = create("Iceland", [7; 32]);
        add_entry(&mut collection, author, [1; 20], "one.jpg", None, NOW).expect("adds");
        collection
    }

    pub(crate) fn descriptors() -> Vec<([u8; 20], Vec<u8>)> {
        vec![([1; 20], b"d8:announce0:e".to_vec())]
    }

    #[test]
    fn a_new_collection_owns_itself_and_has_published_nothing() {
        let collection = create("Iceland", [7; 32]);

        assert_eq!(collection.name, "Iceland");
        assert!(collection.may_publish());
        assert_eq!(collection.number(), 0);
        assert!(head(&collection).is_none());
    }

    #[test]
    fn adding_an_entry_signs_it_with_the_device_that_added_it() {
        let ada = Person::new(1);
        let collection = owned(&ada);

        let entry = &collection.manifest.entries()[0];
        assert_eq!(entry.author_public_key, ada.public_key());
        assert!(entry.verify(), "and the signature stands on its own");
    }

    #[test]
    fn a_duplicate_entry_is_refused_rather_than_silently_replacing() {
        let ada = Person::new(1);
        let mut collection = owned(&ada);

        assert!(matches!(
            add_entry(&mut collection, &ada, [1; 20], "again.jpg", None, NOW),
            Err(CollectionError::Manifest(_))
        ));
    }

    #[test]
    fn only_an_owner_may_add_or_publish() {
        let ada = Person::new(1);
        let mut member = Collection {
            role: Role::Member,
            ..owned(&ada)
        };

        assert!(matches!(
            add_entry(&mut member, &ada, [2; 20], "two.jpg", None, NOW),
            Err(CollectionError::NotTheOwner)
        ));
        assert!(matches!(
            publish(&member, &ada, &[ada.recipient()], &descriptors(), NOW),
            Err(CollectionError::NotTheOwner)
        ));
    }

    #[test]
    fn publishing_produces_one_sealed_key_per_authorized_device() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let collection = owned(&ada);

        let (published, publication) = publish(
            &collection,
            &ada,
            &[ada.recipient(), mira.recipient()],
            &descriptors(),
            NOW,
        )
        .expect("publishes");

        assert_eq!(publication.revision.number, 1);
        assert_eq!(publication.revision.previous_hash, NO_PREVIOUS_REVISION);
        assert_eq!(publication.keys.len(), 2, "one device each");
        assert_eq!(publication.revision.members.len(), 2);
        assert_eq!(publication.entries.len(), 1);
        assert!(publication.revision.verify());
        assert_eq!(published.number(), 1);
        assert_eq!(head(&published), Some(publication.revision.hash()));
    }

    /// The chain is built here, so a second publication must name the first.
    #[test]
    fn the_next_revision_follows_the_one_before_it() {
        let ada = Person::new(1);
        let collection = owned(&ada);

        let (first_state, first) =
            publish(&collection, &ada, &[ada.recipient()], &descriptors(), NOW).expect("publishes");
        let (second_state, second) = publish(
            &first_state,
            &ada,
            &[ada.recipient()],
            &descriptors(),
            NOW + 1,
        )
        .expect("publishes again");

        assert_eq!(second.revision.number, 2);
        assert_eq!(second.revision.previous_hash, first.revision.hash());
        assert_eq!(second_state.number(), 2);
    }

    /// An owner who links a second device keeps publishing under the same
    /// root, which is what makes the collection theirs rather than one
    /// device's.
    #[test]
    fn every_revision_names_the_same_owner_root() {
        let (ada, ada_phone) = (Person::new(1), Person::new(9));
        let collection = owned(&ada);

        let (state, first) =
            publish(&collection, &ada, &[ada.recipient()], &descriptors(), NOW).expect("publishes");
        let (_, second) = publish(
            &state,
            &ada_phone,
            &[ada.recipient()],
            &descriptors(),
            NOW + 1,
        )
        .expect("a second device publishes");

        assert_eq!(
            second.revision.owner_root_key,
            first.revision.owner_root_key
        );
        assert_ne!(
            second.revision.author_key, first.revision.author_key,
            "a different device signed it"
        );
    }

    #[test]
    fn an_entry_with_no_descriptor_is_named_but_not_carried() {
        let ada = Person::new(1);
        let mut collection = owned(&ada);
        add_entry(&mut collection, &ada, [2; 20], "two.jpg", None, NOW).expect("adds");

        let (_, publication) =
            publish(&collection, &ada, &[ada.recipient()], &descriptors(), NOW).expect("publishes");

        assert_eq!(publication.revision.members.len(), 1);
        assert_eq!(
            publication.entries.len(),
            1,
            "one descriptor supplied, one carried"
        );
        assert_eq!(
            collection.manifest.entries().len(),
            2,
            "and the manifest still names both"
        );
    }

    #[test]
    fn a_member_with_no_authorized_device_cannot_be_published_to() {
        let (ada, mira) = (Person::new(1), Person::new(2));
        let mut revoked = mira.entries.clone();
        let mut revoke = LogEntry {
            sequence: 2,
            previous_hash: revoked[0].hash(),
            action: Action::Revoke,
            subject_encryption_key: [0; ENCRYPTION_KEY_BYTES],
            at_unix_ns: NOW + 1,
            ..revoked[0]
        };
        revoke.signature = mira.signing.sign(&revoke.signing_payload()).to_bytes();
        revoked.push(revoke);
        let log = DeviceLog::replay(&revoked).expect("a valid log");
        let recipient = Recipient::current(&log, &revoked).expect("a recipient");

        assert!(matches!(
            publish(
                &owned(&ada),
                &ada,
                &[ada.recipient(), recipient],
                &descriptors(),
                NOW
            ),
            Err(CollectionError::Keys(_))
        ));
    }
}
