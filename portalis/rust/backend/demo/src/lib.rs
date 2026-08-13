//! The cast every demo shares, so each binary is only its own argument.
//!
//! Five of these demos needed the same three things: a person with a device
//! and a signed log saying so, a way to print a section heading, and — once
//! objects started crossing a wire — a way to put a publication into bytes.
//! Written out each time, that was most of every file, and the point of a
//! demo is the part that differs.
//!
//! Every constructor here panics rather than returning a `Result`. These are
//! fixtures: a person whose own log does not replay is a broken demo, not a
//! condition a demo should carry handling for.
//!
//! Nothing here is a test double. [`Person`] builds a real device log with
//! real Ed25519 and X25519 keys, and [`Core`] verifies through the same
//! `receive` the application uses. A demo that leaned on a stub would be
//! demonstrating the stub.

use backend::collections::model::{Collection, CollectionError};
use backend::collections::publish::{Author, Publication, SealedEntryPayload, publish};
use backend::collections::receive::{Received, ReceivingDevice, receive};
use ed25519_dalek::{Signer, SigningKey};
use portalis_nexus_client::{
    Continuity, MemoryChainStore, Recipient, SealedFor, generate_content_key,
};
use portalis_nexus_protocol::{
    Action, DEVICE_KEY_BYTES, DeviceLog, ENCRYPTION_KEY_BYTES, LogEntry, NO_PREVIOUS_ENTRY,
    Revision, SIGNATURE_BYTES, SealedEnvelope, derive_device_id,
};
use x25519_dalek::{PublicKey, StaticSecret};

/// A fixed instant, so every demo prints the same numbers on every run.
pub const NOW: u64 = 1_700_000_000_000_000_000;

/// One person, one device, and the signed log that says so.
///
/// Deterministic from `seed`: the same person is the same keys every run,
/// which is what lets a demo's output be read as a vector rather than noise.
pub struct Person {
    pub name: &'static str,
    signing: SigningKey,
    secret: StaticSecret,
    entries: Vec<LogEntry>,
}

impl Person {
    #[must_use]
    pub fn new(name: &'static str, seed: u8) -> Self {
        let signing = SigningKey::from_bytes(&[seed; 32]);
        let secret = StaticSecret::from([seed.wrapping_add(0x40); ENCRYPTION_KEY_BYTES]);
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
            name,
            signing,
            secret,
            entries: vec![root],
        }
    }

    /// Enrols a second device, so a demo can show one being revoked.
    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn with_second_device(mut self, seed: u8) -> Self {
        let second = Self::new("second", seed);
        let previous = self.entries.last().expect("a root").hash();
        let mut entry = LogEntry {
            root_key: self.public_key(),
            sequence: self.entries.len() as u64 + 1,
            previous_hash: previous,
            action: Action::Enrol,
            subject_signing_key: second.public_key(),
            subject_encryption_key: *PublicKey::from(&second.secret).as_bytes(),
            at_unix_ns: NOW + 1,
            author_key: self.public_key(),
            signature: [0; SIGNATURE_BYTES],
        };
        entry.signature = self.signing.sign(&entry.signing_payload()).to_bytes();
        self.entries.push(entry);
        self
    }

    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn public_key(&self) -> [u8; DEVICE_KEY_BYTES] {
        self.signing.verifying_key().to_bytes()
    }

    /// The X25519 key a content key is sealed to.
    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn encryption_key(&self) -> [u8; ENCRYPTION_KEY_BYTES] {
        *PublicKey::from(&self.secret).as_bytes()
    }

    /// This person's own device log entry, signed by them.
    ///
    /// Demos about the log itself build their own sequences with this and
    /// [`Self::states`], rather than through the higher-level helpers, because
    /// the sequence is what they are demonstrating.
    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn root_entry(&self) -> LogEntry {
        self.entries[0]
    }

    /// An entry in this person's log, signed by this person.
    ///
    /// `previous` is the entry it follows; `None` makes it a root.
    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn states(
        &self,
        sequence: u64,
        previous: Option<&LogEntry>,
        action: Action,
        subject: &Self,
    ) -> LogEntry {
        let mut entry = LogEntry {
            root_key: self.public_key(),
            sequence,
            previous_hash: previous.map_or(NO_PREVIOUS_ENTRY, LogEntry::hash),
            action,
            subject_signing_key: subject.public_key(),
            subject_encryption_key: match action {
                Action::Enrol => subject.encryption_key(),
                Action::Revoke => [0; ENCRYPTION_KEY_BYTES],
            },
            at_unix_ns: NOW + sequence,
            author_key: self.public_key(),
            signature: [0; SIGNATURE_BYTES],
        };
        entry.signature = self.signing.sign(&entry.signing_payload()).to_bytes();
        entry
    }

    /// An entry in this person's log, authored by someone else.
    ///
    /// Separate from [`Self::states`] because who signs an entry is the thing
    /// half the device-log attacks turn on: a stranger, a revoked device, or
    /// someone impersonating an author they are not.
    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn states_by(
        &self,
        sequence: u64,
        previous: Option<&LogEntry>,
        action: Action,
        subject: &Self,
        author: &Self,
    ) -> LogEntry {
        let mut entry = self.states(sequence, previous, action, subject);
        entry.author_key = author.public_key();
        author.resign(entry)
    }

    /// Signs an arbitrary canonical payload, for demos that build objects
    /// this module does not know about.
    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn sign_bytes(&self, payload: &[u8]) -> [u8; SIGNATURE_BYTES] {
        self.signing.sign(payload).to_bytes()
    }

    /// Re-signs an entry after a demo has altered a field.
    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn resign(&self, mut entry: LogEntry) -> LogEntry {
        entry.signature = self.signing.sign(&entry.signing_payload()).to_bytes();
        entry
    }

    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn log(&self) -> DeviceLog {
        DeviceLog::replay(&self.entries).expect("a valid log")
    }

    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn recipient(&self) -> Recipient {
        Recipient::current(&self.log(), &self.entries).expect("a recipient at its own log")
    }

    /// # Panics
    ///
    /// If the fixture cannot be built, which means the demo is wrong.
    #[must_use]
    pub fn device(&self) -> ReceivingDevice {
        ReceivingDevice {
            device_id: derive_device_id(&self.public_key()),
            encryption_secret_key: self.secret.to_bytes(),
        }
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

/// One person's whole device: who they are, what they have verified, and
/// where they are in each chain.
pub struct Core {
    pub person: Person,
    chain: MemoryChainStore,
    held: Option<Collection>,
}

impl Core {
    #[must_use]
    pub fn new(name: &'static str, seed: u8) -> Self {
        Self {
            person: Person::new(name, seed),
            chain: MemoryChainStore::default(),
            held: None,
        }
    }

    #[must_use]
    pub fn number(&self) -> u64 {
        self.held.as_ref().map_or(0, Collection::number)
    }

    /// Publishes to everyone named, and returns the collection as it now is.
    ///
    /// # Panics
    ///
    /// If this core does not own the collection, which is a demo's mistake
    /// rather than a case worth handling.
    #[must_use]
    pub fn publish_to(
        &self,
        collection: &Collection,
        to: &[&Self],
        descriptors: &[Descriptor],
        at: u64,
    ) -> (Collection, Publication) {
        let recipients: Vec<Recipient> = to.iter().map(|core| core.person.recipient()).collect();
        publish(collection, &self.person, &recipients, descriptors, at).expect("publishes")
    }

    /// Takes this revision as a baseline. Once, when accepting an invitation.
    ///
    /// # Errors
    ///
    /// Whatever verification refused.
    pub async fn join(
        &mut self,
        publication: &Publication,
        from: &Self,
        name: &str,
    ) -> Result<Received, CollectionError> {
        self.accept(publication, from, name, Continuity::Join).await
    }

    /// Follows the chain, which refuses a gap, a rollback and a fork.
    ///
    /// # Errors
    ///
    /// Whatever verification refused.
    pub async fn follow(
        &mut self,
        publication: &Publication,
        from: &Self,
        name: &str,
    ) -> Result<Received, CollectionError> {
        self.accept(publication, from, name, Continuity::Strict)
            .await
    }

    async fn accept(
        &mut self,
        publication: &Publication,
        from: &Self,
        name: &str,
        continuity: Continuity,
    ) -> Result<Received, CollectionError> {
        let received = receive(
            publication,
            &from.person.log(),
            &self.person.device(),
            &self.chain,
            self.held.as_ref(),
            name,
            continuity,
        )
        .await?;
        self.held = Some(received.collection.clone());
        Ok(received)
    }
}

/// One entry's `.torrent`, alongside the info hash that names it.
pub type Descriptor = ([u8; 20], Vec<u8>);

/// A new collection with `count` signed entries, and their descriptors.
///
/// # Panics
///
/// If an entry cannot be added, which would mean the manifest rules changed.
#[must_use]
pub fn a_collection_with(name: &str, owner: &Person, count: u8) -> (Collection, Vec<Descriptor>) {
    let mut collection = backend::collections::publish::create(name, generate_content_key());
    let mut descriptors = Vec::new();
    for index in 1..=count {
        let info_hash = [index; 20];
        let label = format!("photo-{index}.jpg");
        backend::collections::publish::add_entry(
            &mut collection,
            owner,
            info_hash,
            &label,
            None,
            NOW,
        )
        .expect("adds");
        descriptors.push((
            info_hash,
            format!("d8:announce0:4:infod4:name{}:{label}ee", label.len()).into_bytes(),
        ));
    }
    (collection, descriptors)
}

/// A publication as bytes, so it can cross a wire.
///
/// Length-prefixed, so one read yields exactly one publication. This is a
/// demo's encoding rather than a protocol one: every object inside it is
/// already canonical and signed, and how they are packed together for one
/// exchange is the transport's business.
#[must_use]
pub fn encode(publication: &Publication) -> Vec<u8> {
    let mut bytes = Vec::new();
    push(&mut bytes, &publication.revision.encode());
    push(&mut bytes, &publication.sealed_manifest);
    count(&mut bytes, publication.entries.len());
    for entry in &publication.entries {
        bytes.extend_from_slice(&entry.info_hash);
        push(&mut bytes, &entry.payload);
    }
    count(&mut bytes, publication.keys.len());
    for sealed in &publication.keys {
        bytes.extend_from_slice(&sealed.member_root_key);
        bytes.extend_from_slice(&sealed.recipient_device_id);
        bytes.extend_from_slice(&sealed.envelope.ephemeral_public_key);
        push(&mut bytes, &sealed.envelope.ciphertext);
    }
    bytes
}

/// Reads back what [`encode`] wrote.
///
/// # Errors
///
/// When the bytes are truncated or hold something that is not a publication.
pub fn decode(bytes: &[u8]) -> anyhow::Result<Publication> {
    let mut reader = Reader { bytes };
    let revision = Revision::decode(reader.chunk()?)?;
    let sealed_manifest = reader.chunk()?.to_vec();

    let mut entries = Vec::new();
    for _ in 0..reader.u32()? {
        entries.push(SealedEntryPayload {
            info_hash: reader.array::<20>()?,
            payload: reader.chunk()?.to_vec(),
        });
    }

    let mut keys = Vec::new();
    for _ in 0..reader.u32()? {
        keys.push(SealedFor {
            member_root_key: reader.array::<32>()?,
            recipient_device_id: reader.array::<32>()?,
            envelope: SealedEnvelope {
                ephemeral_public_key: reader.array::<32>()?,
                ciphertext: reader.chunk()?.to_vec(),
            },
        });
    }

    Ok(Publication {
        revision,
        sealed_manifest,
        entries,
        keys,
    })
}

fn push(bytes: &mut Vec<u8>, value: &[u8]) {
    count(bytes, value.len());
    bytes.extend_from_slice(value);
}

fn count(bytes: &mut Vec<u8>, value: usize) {
    bytes.extend_from_slice(&u32::try_from(value).expect("bounded").to_be_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> anyhow::Result<&'a [u8]> {
        anyhow::ensure!(self.bytes.len() >= count, "truncated");
        let (taken, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> anyhow::Result<[u8; N]> {
        Ok(<[u8; N]>::try_from(self.take(N)?)?)
    }

    fn u32(&mut self) -> anyhow::Result<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn chunk(&mut self) -> anyhow::Result<&'a [u8]> {
        let length = self.u32()? as usize;
        self.take(length)
    }
}

/// A heading, so a demo's output reads as sections rather than a wall.
pub fn section(title: &str) {
    println!("\n{title}\n{}", "─".repeat(title.chars().count()));
}

/// The first few bytes of a key or hash, which is all a person needs to tell
/// two of them apart.
#[must_use]
pub fn short(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    bytes.iter().take(6).fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}
