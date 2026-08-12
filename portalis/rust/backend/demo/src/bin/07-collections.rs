//! Step 7 — two cores, one collection, no network.
//!
//! Everything the earlier steps built exists to make this possible. Ada
//! creates a collection, puts media in it, publishes, adds a member, and
//! removes one. Mira and Jonas each verify what they are handed against Ada's
//! device log and the revision they already hold.
//!
//! **There is no transport in this file.** A publication is a value, passed
//! from one core to another by a function call. That is the gate for this
//! step, and it is not a shortcut: the claim is that sharing does not depend
//! on a service, and the way to test that claim is to remove the service
//! entirely. Step 8 adds QUIC and changes none of what follows, because
//! carrying bytes is a different problem from deciding whether to believe
//! them.
//!
//! Two things to watch for. **Jonas joins at revision 2** — he was never a
//! member of revision 1 and will never be sent it, so joining is a distinct
//! decision from following, and the API makes him say which. **When he is
//! removed the key is rotated**, and his copy of the old key still opens the
//! old revision, because nothing can reach into a device and take a key back.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 07-collections`.

use backend::collections::members::remove_members;
use backend::collections::model::{Collection, CollectionError};
use backend::collections::publish::{Author, Publication, add_entry, create, publish};
use backend::collections::receive::{Received, ReceivingDevice, receive};
use backend::store::records::Role;
use ed25519_dalek::{Signer, SigningKey};
use portalis_nexus_client::{Continuity, MemoryChainStore, Recipient, generate_content_key};
use portalis_nexus_protocol::{
    Action, DEVICE_KEY_BYTES, DeviceLog, ENCRYPTION_KEY_BYTES, LogEntry, NO_PREVIOUS_ENTRY,
    SIGNATURE_BYTES, derive_device_id,
};
use x25519_dalek::{PublicKey, StaticSecret};

const NOW: u64 = 1_700_000_000_000_000_000;
const NAME: &str = "Iceland, 2019";

#[tokio::main]
async fn main() {
    let ada = Core::new(1);
    let mut mira = Core::new(2);
    let mut jonas = Core::new(3);

    section("Ada makes a collection");
    let mut collection = create(NAME, generate_content_key());
    println!("  {NAME} — owner, revision {}", collection.number());
    let descriptors = add_media(&mut collection, &ada);
    println!(
        "  {} entries, each signed by the device that added it",
        descriptors.len()
    );

    section("She publishes it to herself and Mira");
    let (state, first) = ada.publish(&collection, &[&ada, &mira], &descriptors, NOW);
    collection = state;
    describe(&first);

    section("Mira receives it — no network, just the bytes");
    let received = mira
        .follow(&first, &ada)
        .await
        .expect("Mira verifies Ada's publication");
    println!(
        "  verified revision {} · {} entries · role {:?}",
        received.collection.number(),
        received.descriptors.len(),
        received.collection.role
    );
    println!("  She checked Ada's signature against Ada's device log, the");
    println!("  revision against what she already held, and the manifest");
    println!("  against the hash the revision signed for — all before");
    println!("  decrypting a single descriptor.");

    section("Jonas is added, and joins partway through");
    let (state, second) = ada.publish(&collection, &[&ada, &mira, &jonas], &descriptors, NOW + 1);
    collection = state;
    describe(&second);

    mira.follow(&second, &ada).await.expect("Mira follows");
    let jonas_first = jonas.join(&second, &ada).await.expect("Jonas joins");
    println!(
        "  Mira followed to revision {}; Jonas joined at it",
        mira.number()
    );
    println!("  Joining is its own decision. Jonas was never a member of");
    println!("  revision 1 and will never be sent it, so demanding a chain");
    println!("  from the beginning would mean he could never join at all.");

    section("Jonas is removed: the key is rotated");
    let (_, third) = remove_members(
        &collection,
        &ada.person,
        &[ada.person.recipient(), mira.person.recipient()],
        &descriptors,
        NOW + 2,
    )
    .expect("rotates and publishes");
    describe(&third);

    match jonas.follow(&third, &ada).await {
        Err(CollectionError::NotSealedToUs) => {
            println!("\n  Jonas → refused: nothing in it is sealed to his device");
        }
        other => panic!("a removed member must not receive, got {other:?}"),
    }
    let mira_third = mira
        .follow(&third, &ada)
        .await
        .expect("Mira still receives");
    println!(
        "  Mira  → revision {} under a new content key: {}",
        mira_third.collection.number(),
        mira_third.collection.content_key != received.collection.content_key
    );

    section("What rotation does, and what it cannot");
    println!("  Jonas still holds the old key, and it still opens revision 2.");
    println!("  That is not a leak being tolerated — it is the truth about");
    println!("  what a key is. What rotation achieves is that revision 3 was");
    println!("  never sealed under it.");
    assert_ne!(
        jonas_first.collection.content_key, mira_third.collection.content_key,
        "the rotated key must differ from the one Jonas holds"
    );

    section("A member cannot publish");
    let as_member = Collection {
        role: Role::Member,
        ..collection
    };
    match publish(
        &as_member,
        &mira.person,
        &[mira.person.recipient()],
        &descriptors,
        NOW + 3,
    ) {
        Err(CollectionError::NotTheOwner) => {
            println!("  Mira → refused: only the owner publishes revisions");
        }
        other => panic!("a member must not publish, got {other:?}"),
    }
    println!("  A plain answer rather than a cryptographic one: her signature");
    println!("  would not verify against Ada's device log in any case.");

    println!("\nTwo cores exchanged three publications by hand and both");
    println!("verified. No socket, no service, no transport code — the point");
    println!("being that none of it was needed.");
}

/// One person's whole device: their identity, what they have verified, and
/// where they are in each chain.
struct Core {
    person: Person,
    chain: MemoryChainStore,
    held: Option<Collection>,
}

impl Core {
    fn new(seed: u8) -> Self {
        Self {
            person: Person::new(seed),
            chain: MemoryChainStore::default(),
            held: None,
        }
    }

    fn number(&self) -> u64 {
        self.held.as_ref().map_or(0, Collection::number)
    }

    fn publish(
        &self,
        collection: &Collection,
        to: &[&Self],
        descriptors: &[([u8; 20], Vec<u8>)],
        at: u64,
    ) -> (Collection, Publication) {
        let recipients: Vec<Recipient> = to.iter().map(|core| core.person.recipient()).collect();
        publish(collection, &self.person, &recipients, descriptors, at).expect("publishes")
    }

    /// Take this revision as a baseline. Once, when accepting an invitation.
    async fn join(
        &mut self,
        publication: &Publication,
        from: &Self,
    ) -> Result<Received, CollectionError> {
        self.accept(publication, from, Continuity::Join).await
    }

    /// Follow the chain, which refuses a gap, a rollback and a fork.
    async fn follow(
        &mut self,
        publication: &Publication,
        from: &Self,
    ) -> Result<Received, CollectionError> {
        self.accept(publication, from, Continuity::Strict).await
    }

    async fn accept(
        &mut self,
        publication: &Publication,
        from: &Self,
        continuity: Continuity,
    ) -> Result<Received, CollectionError> {
        let received = receive(
            publication,
            &from.person.log(),
            &self.person.device(),
            &self.chain,
            self.held.as_ref(),
            NAME,
            continuity,
        )
        .await?;
        self.held = Some(received.collection.clone());
        Ok(received)
    }
}

fn describe(publication: &Publication) {
    println!(
        "  revision {} · {} member(s) · {} sealed key(s) · {} entry payload(s)",
        publication.revision.number,
        publication.revision.members.len(),
        publication.keys.len(),
        publication.entries.len()
    );
}

/// Two photographs, signed as they are added.
fn add_media(collection: &mut Collection, author: &Core) -> Vec<([u8; 20], Vec<u8>)> {
    let media = [
        ([0x01_u8; 20], "glacier.jpg"),
        ([0x02_u8; 20], "harbour.jpg"),
    ];
    for (info_hash, name) in media {
        add_entry(collection, &author.person, info_hash, name, None, NOW).expect("adds");
    }
    media
        .iter()
        .map(|(info_hash, name)| {
            (
                *info_hash,
                format!("d8:announce0:4:infod4:name{}:{name}ee", name.len()).into_bytes(),
            )
        })
        .collect()
}

/// One person with one device, and the log that says so.
struct Person {
    signing: SigningKey,
    secret: StaticSecret,
    entries: Vec<LogEntry>,
}

impl Person {
    fn new(seed: u8) -> Self {
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
            signing,
            secret,
            entries: vec![root],
        }
    }

    fn log(&self) -> DeviceLog {
        DeviceLog::replay(&self.entries).expect("a valid log")
    }

    fn recipient(&self) -> Recipient {
        Recipient::current(&self.log(), &self.entries).expect("a recipient at its own log")
    }

    fn device(&self) -> ReceivingDevice {
        ReceivingDevice {
            device_id: derive_device_id(&self.signing.verifying_key().to_bytes()),
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

fn section(title: &str) {
    println!("\n{title}\n{}", "─".repeat(title.chars().count()));
}
