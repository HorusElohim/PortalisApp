//! Step 3 — the revision chain: three revisions, a rollback, and a fork.
//!
//! A collection is a chain of signed revisions, not a row a service keeps for
//! you (D3). The difference only shows under attack, and the two attacks that
//! matter forge nothing.
//!
//! An **older revision, served as the current one**, is entirely genuine — the
//! owner did sign it, before removing someone. A **fork** is two revisions
//! that both verify and disagree. Neither is visible in the object in front of
//! you, which is why a reader holds the highest revision it has verified. Take
//! that away and a rollback is indistinguishable from an update.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 03-revisions`.

use portalis_nexus_client::{ChainError, Continuity, MemoryChainStore, verify_revision};
use portalis_nexus_demo::{NOW, Person, section, short};
use portalis_nexus_protocol::{
    Action, DEVICE_KEY_BYTES, DeviceLog, Member, NO_PREVIOUS_REVISION, Revision, RevisionHash,
    SHARE_ID_BYTES, SIGNATURE_BYTES,
};

const COLLECTION: [u8; SHARE_ID_BYTES] = [0x11; SHARE_ID_BYTES];

#[tokio::main]
async fn main() {
    let laptop = Person::new("laptop", 1);
    let phone = Person::new("phone", 2);
    let impostor = Person::new("an impostor", 9);
    let log = owner(&laptop, &phone);
    let store = MemoryChainStore::default();

    section("Three revisions");
    let first = revision(
        &laptop,
        &laptop,
        1,
        NO_PREVIOUS_REVISION,
        [0x22; 32],
        &[2, 3],
    );
    let second = revision(&laptop, &laptop, 2, first.hash(), [0x33; 32], &[2, 3]);
    // The third removes a member — the change a rollback would undo — and is
    // signed by the phone, because any unrevoked owner device may publish.
    let third = revision(&laptop, &phone, 3, second.hash(), [0x44; 32], &[2]);

    for candidate in [&first, &second, &third] {
        let accepted = accept(candidate, &log, &store)
            .await
            .expect("the owner's own");
        println!(
            "  revision {} · {} member(s) · {}",
            accepted.state.number,
            candidate.members.len(),
            short(&accepted.state.revision_hash)
        );
    }

    section("A rollback");
    refused(
        "the second revision, served again after the third",
        accept(&second, &log, &store).await,
        "the member removed in revision 3 would silently regain access",
    );
    println!("  Detectable only against the held state. The revision itself is");
    println!("  flawless — it was flawless when it was current.");

    section("A fork");
    let rival = revision(&laptop, &laptop, 3, second.hash(), [0x99; 32], &[2, 3]);
    assert!(rival.verify(), "the fork verifies on its own");
    match accept(&rival, &log, &store).await {
        Err(ChainError::Fork {
            number,
            kept,
            refused,
        }) => {
            println!("  two revisions numbered {number}");
            println!("    kept    {} — the first seen", short(&kept));
            println!("    refused {} — surfaced, not discarded", short(&refused));
        }
        other => panic!("a fork must be reported as one, got {other:?}"),
    }
    println!("  Never resolved silently: it means a compromised owner device or");
    println!("  a service splitting members' views, and picking a winner is not");
    println!("  a decision code can make correctly.");

    other_refusals(&laptop, &phone, &impostor, &log, third.hash(), &store).await;

    section("Not a refusal");
    let honest = revision(&laptop, &laptop, 4, third.hash(), [0x55; 32], &[2]);
    let accepted = verify_revision(
        &honest,
        &log,
        &store,
        Some(honest.manifest_hash),
        &[([2; DEVICE_KEY_BYTES], [0xff; 32])],
        Continuity::Strict,
    )
    .await
    .expect("a valid revision");
    println!(
        "  revision {} accepted, {} member(s) owed a re-seal",
        accepted.state.number,
        accepted.reseal_owed.len()
    );
    println!("  Their log moved after the owner sealed to them, so a new device");
    println!("  of theirs opens nothing yet. A job to do, not a lie to refuse.");

    println!("\nEight refusals, each with its own reason. Nothing was forged in");
    println!("the two that matter most.");
}

/// Everything the chain refuses that is neither a rollback nor a fork.
async fn other_refusals(
    laptop: &Person,
    phone: &Person,
    impostor: &Person,
    log: &DeviceLog,
    parent: RevisionHash,
    store: &MemoryChainStore,
) {
    section("The rest of the chain's refusals");
    let mut forged = revision(laptop, laptop, 4, parent, [0x55; 32], &[2]);
    forged.signature = impostor.sign_bytes(&forged.signing_payload());
    refused(
        "signed by someone other than its author",
        accept(&forged, log, store).await,
        "anyone could publish into anyone's collection",
    );
    refused(
        "from a device the owner never enrolled",
        accept(
            &revision(laptop, impostor, 4, parent, [0x55; 32], &[2]),
            log,
            store,
        )
        .await,
        "the service could publish on the owner's behalf",
    );
    refused(
        "from a device revoked after it was enrolled",
        accept(
            &revision(laptop, phone, 4, parent, [0x55; 32], &[2]),
            &without_the_phone(laptop, phone),
            store,
        )
        .await,
        "a stolen device would keep publishing after being removed",
    );
    refused(
        "skipping a number",
        accept(
            &revision(laptop, laptop, 6, parent, [0x55; 32], &[2]),
            log,
            store,
        )
        .await,
        "a reader would never learn what happened in between",
    );
    refused(
        "naming a parent that is not the one held",
        accept(
            &revision(laptop, laptop, 4, [7; 32], [0x55; 32], &[2]),
            log,
            store,
        )
        .await,
        "history could be rewritten without redoing everything after it",
    );

    let honest = revision(laptop, laptop, 4, parent, [0x55; 32], &[2]);
    refused(
        "naming a manifest other than the one fetched",
        verify_revision(
            &honest,
            log,
            store,
            Some([0xaa; 32]),
            &[],
            Continuity::Strict,
        )
        .await,
        "the signed list and the delivered list could differ",
    );
}

async fn accept(
    revision: &Revision,
    log: &DeviceLog,
    store: &MemoryChainStore,
) -> Result<portalis_nexus_client::Accepted, ChainError> {
    verify_revision(revision, log, store, None, &[], Continuity::Strict).await
}

fn refused<T: std::fmt::Debug>(what: &str, outcome: Result<T, ChainError>, cost: &str) {
    let error = outcome.expect_err("this must be refused");
    println!("\n  {what}");
    println!("    refused: {error}");
    println!("    without it: {cost}");
}

/// An owner with two devices.
fn owner(laptop: &Person, phone: &Person) -> DeviceLog {
    let root = laptop.root_entry();
    let enrol = laptop.states(2, Some(&root), Action::Enrol, phone);
    DeviceLog::replay(&[root, enrol]).expect("the owner's own devices")
}

fn without_the_phone(laptop: &Person, phone: &Person) -> DeviceLog {
    let root = laptop.root_entry();
    let enrol = laptop.states(2, Some(&root), Action::Enrol, phone);
    let revoke = laptop.states(3, Some(&enrol), Action::Revoke, phone);
    DeviceLog::replay(&[root, enrol, revoke]).expect("a revocation")
}

fn revision(
    owner: &Person,
    author: &Person,
    number: u64,
    previous: RevisionHash,
    manifest_hash: [u8; 32],
    members: &[u8],
) -> Revision {
    let mut revision = Revision {
        collection_id: COLLECTION,
        number,
        previous_hash: previous,
        manifest_hash,
        owner_root_key: owner.public_key(),
        at_unix_ns: NOW + number,
        members: members
            .iter()
            .map(|&root| Member {
                root_key: [root; DEVICE_KEY_BYTES],
                device_log_hash: [root.wrapping_add(0x80); 32],
            })
            .collect(),
        author_key: author.public_key(),
        signature: [0; SIGNATURE_BYTES],
    };
    revision.signature = author.sign_bytes(&revision.signing_payload());
    revision
}
