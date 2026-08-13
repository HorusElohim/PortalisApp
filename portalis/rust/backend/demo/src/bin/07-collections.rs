//! Step 7 — two cores, one collection, no network.
//!
//! A publication is a value, passed from one core to another by a function
//! call. That is the gate for this step, and not a shortcut: the claim is that
//! sharing does not depend on a service, and the way to test it is to remove
//! the service entirely. Step 8 adds QUIC and changes none of this.
//!
//! Two things to watch. **Jonas joins at revision 2** — he was never a member
//! of revision 1 and will never be sent it, so joining is a decision distinct
//! from following. **When he is removed the key is rotated**, and his copy of
//! the old key still opens the old revision, because nothing can reach into a
//! device and take a key back.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 07-collections`.

use backend::collections::members::remove_members;
use backend::collections::model::{Collection, CollectionError};
use backend::collections::publish::publish;
use backend::store::records::Role;
use portalis_nexus_demo::{Core, NOW, a_collection_with, section};

const NAME: &str = "Iceland, 2019";

#[tokio::main]
async fn main() {
    let ada = Core::new("Ada", 1);
    let mut mira = Core::new("Mira", 2);
    let mut jonas = Core::new("Jonas", 3);

    section("Ada makes a collection and publishes it to Mira");
    let (collection, descriptors) = a_collection_with(NAME, &ada.person, 2);
    let (collection, first) = ada.publish_to(&collection, &[&ada, &mira], &descriptors, NOW);
    let mira_first = mira
        .follow(&first, &ada, NAME)
        .await
        .expect("Mira verifies");
    println!(
        "  revision {} · {} members · Mira verified {} entries as {:?}",
        first.revision.number,
        first.revision.members.len(),
        mira_first.descriptors.len(),
        mira_first.collection.role
    );
    println!("  She checked Ada's signature against Ada's device log, the chain");
    println!("  against what she held, and the manifest against the hash the");
    println!("  revision signed for — before decrypting anything.");

    section("Jonas is added, and joins partway through");
    let (collection, second) =
        ada.publish_to(&collection, &[&ada, &mira, &jonas], &descriptors, NOW + 1);
    mira.follow(&second, &ada, NAME)
        .await
        .expect("Mira follows");
    let jonas_first = jonas.join(&second, &ada, NAME).await.expect("Jonas joins");
    println!("  Mira followed to {}; Jonas joined at it", mira.number());
    println!("  Demanding a chain from revision 1 would mean he could never");
    println!("  join: he was not a member of it and will never be sent it.");

    section("Jonas is removed: the key is rotated");
    let (_, third) = remove_members(
        &collection,
        &ada.person,
        &[ada.person.recipient(), mira.person.recipient()],
        &descriptors,
        NOW + 2,
    )
    .expect("rotates");

    match jonas.follow(&third, &ada, NAME).await {
        Err(CollectionError::NotSealedToUs) => {
            println!("  Jonas → refused: nothing in it is sealed to his device");
        }
        other => panic!("a removed member must not receive, got {other:?}"),
    }
    let mira_third = mira.follow(&third, &ada, NAME).await.expect("Mira remains");
    println!(
        "  Mira  → revision {} under a new key: {}",
        mira_third.collection.number(),
        mira_third.collection.content_key != mira_first.collection.content_key
    );
    println!("  Jonas still holds the old key and it still opens revision 2.");
    println!("  Rotation does not undo that — it makes revision 3 something the");
    println!("  old key was never used for.");
    assert_ne!(
        jonas_first.collection.content_key,
        mira_third.collection.content_key
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
            println!("  Mira → refused: publishing is the owner's");
        }
        other => panic!("a member must not publish, got {other:?}"),
    }

    println!("\nTwo cores exchanged three publications by hand and both verified.");
    println!("No socket, no service, no transport code — none of it was needed.");
}
