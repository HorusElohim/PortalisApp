//! Step 4 — content keys: sealed to a verified log, rotated on removal.
//!
//! The attack: a service adds a device to someone's list, the owner seals the
//! content key to it, and the service reads everything. Nothing downstream
//! notices — the ciphertext is valid, the recipient real, the owner did the
//! sealing. So the defence is structural. Every recipient's encryption key and
//! address come out of a replayed device log; there is no argument through
//! which another device can enter.
//!
//! A stale log is the same attack in different clothes, and the type system
//! handles that one: a `Recipient` can only be built by adopting an offered
//! log over one already held.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 04-sealing`.

use portalis_nexus_client::{
    KeyError, Recipient, Sealing, generate_content_key, open_content_key, rotate_content_key,
    seal_content_key,
};
use portalis_nexus_demo::{Person, section};
use portalis_nexus_protocol::{Action, DeviceLog, SHARE_ID_BYTES, derive_device_id};

const COLLECTION: [u8; SHARE_ID_BYTES] = [0x11; SHARE_ID_BYTES];

fn main() {
    let ada = Person::new("Ada", 1).with_second_device(0x21);
    let mira = Person::new("Mira", 2);
    let jonas = Person::new("Jonas", 3);
    let outsider = Person::new("an outsider", 9);

    section("Sealing to the members");
    let key = generate_content_key();
    let sealing = seal_content_key(
        &key,
        COLLECTION,
        &[ada.recipient(), mira.recipient(), jonas.recipient()],
    )
    .expect("seals");
    println!(
        "  {} envelope(s) for {} member(s) — Ada has two devices",
        sealing.envelopes.len(),
        sealing.members.len()
    );
    for person in [&ada, &mira, &jonas] {
        println!(
            "  {} opens it: {}",
            person.name,
            opens(person, &sealing) == Some(key)
        );
    }
    println!("  The membership the sealing produced goes into the revision, so");
    println!("  the two cannot disagree about what was sealed against.");

    section("A third party cannot open it");
    println!(
        "  no envelope is addressed to {}: {}",
        outsider.name,
        opens(&outsider, &sealing).is_none()
    );
    let intercepted = &sealing.envelopes[0];
    match open_content_key(
        &outsider.device().encryption_secret_key,
        COLLECTION,
        intercepted.recipient_device_id,
        &intercepted.envelope,
    ) {
        Err(error) => println!("  intercepting one and trying it → {error}"),
        Ok(_) => panic!("an outsider must never recover a content key"),
    }

    section("A device the log never enrolled");
    println!("  There is no way to express it. `seal_content_key` takes");
    println!("  recipients, and a recipient is a replayed device log — every");
    println!("  address and key comes out of it. An invented device would have");
    println!("  to be signed into the log first, which needs a key already");
    println!("  inside. That is step 2 doing its job here.");

    section("Removing Jonas: rotate");
    let (rotated, after) =
        rotate_content_key(COLLECTION, &[ada.recipient(), mira.recipient()]).expect("rotates");
    println!("  a new key, not a reshuffle: {}", rotated != key);
    println!(
        "  {} envelope(s) for {} member(s); nothing for {}: {}",
        after.envelopes.len(),
        after.members.len(),
        jonas.name,
        opens(&jonas, &after).is_none()
    );
    println!(
        "  What {} already holds still opens. No protocol reaches into a",
        jonas.name
    );
    println!("  device to take a key back — rotation closes the next revision,");
    println!("  not the last one.");

    section("A stale log cannot be used at all");
    // Genuine, signed, and one revocation behind: the attack needing no
    // forgery whatsoever.
    let current = without_the_second_device(&ada);
    match Recipient::current(&current, ada.entries()) {
        Err(KeyError::Log(error)) => println!("  yesterday's log, against today's → {error}"),
        other => panic!("a stale log must be refused, got {other:?}"),
    }
    println!("  Refused before sealing is reachable: a Recipient cannot be built");
    println!("  from a log behind the one already held, so sealing to a device");
    println!("  revoked this morning is not a mistake this API allows.");

    println!("\nA content key reached no device outside a verified log. The two");
    println!("ways to try — an invented device and a stale log — are one");
    println!("unrepresentable and one refused.");
}

/// The content key this person's first device can recover, if any.
fn opens(person: &Person, sealing: &Sealing) -> Option<[u8; 32]> {
    let device = person.device();
    let sealed = sealing
        .envelopes
        .iter()
        .find(|sealed| sealed.recipient_device_id == device.device_id)?;
    open_content_key(
        &device.encryption_secret_key,
        COLLECTION,
        sealed.recipient_device_id,
        &sealed.envelope,
    )
    .ok()
}

/// Ada's log after she revokes the device she linked.
fn without_the_second_device(ada: &Person) -> DeviceLog {
    let entries = ada.entries();
    let second = Person::new("second", 0x21);
    assert_eq!(
        derive_device_id(&second.public_key()),
        derive_device_id(&entries[1].subject_signing_key),
        "the same device the log enrolled"
    );
    let revoke = ada.states(3, entries.last(), Action::Revoke, &second);
    DeviceLog::replay(&[entries.to_vec(), vec![revoke]].concat()).expect("a revocation")
}
