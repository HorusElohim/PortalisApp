//! Step 11 — reaching a device that is asleep.
//!
//! Step 8 showed two devices sharing with no service in existence. This is the
//! case that one cannot cover: Mira's phone is in her pocket. Ada cannot reach
//! her, and waiting until she can is not an answer a person accepts.
//!
//! So the service holds it. What matters is *how little* it holds: an opaque
//! blob addressed to a device identifier. The service knows somebody has
//! something for somebody, and nothing else — not the collection, not the
//! members, not a byte of content. That is why a mailbox can be a dumb queue.
//!
//! Everything else is unchanged. Mira verifies exactly as she did in step 7,
//! against Ada's device log and the revision she holds, because an object is
//! valid on its own terms and where it waited changes nothing.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 11-mailbox`.

use portalis_nexus_demo::{Core, NOW, a_collection_with, decode, encode, section};
use portalis_nexus_protocol::derive_device_id;
use portalis_nexus_storage::embedded::Embedded;
use portalis_nexus_storage::mailbox::{Limits, MAX_BYTES, MAX_ITEMS};

const NAME: &str = "Iceland, 2019";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = std::env::temp_dir().join(format!("portalis-mailbox-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)?;

    let ada = Core::new("Ada", 1);
    let mut mira = Core::new("Mira", 2);
    let mira_device = derive_device_id(&mira.person.public_key());

    section("The service knows very little");
    let service = Embedded::open(directory.join("service.redb"))?;
    println!("  one file, no replica set, nothing to operate");
    println!(
        "  limits: {MAX_ITEMS} items and {} MiB per device",
        MAX_BYTES / 1024 / 1024
    );

    section("Ada publishes while Mira is asleep");
    let (collection, descriptors) = a_collection_with(NAME, &ada.person, 2);
    let (collection, publication) = ada.publish_to(&collection, &[&ada, &mira], &descriptors, NOW);
    let bytes = encode(&publication);
    let sequence = service.deliver(mira_device, &bytes)?;
    println!(
        "  revision {} · {} bytes left as item {sequence}",
        collection.number(),
        bytes.len()
    );
    let (count, held) = service.mailbox_size(mira_device)?;
    println!("  Mira's mailbox: {count} item, {held} bytes");
    println!("  All the service sees is a blob addressed to a device. Not the");
    println!("  collection, not the members, not a byte of what is in it.");

    section("Mira's phone comes back");
    let waiting = service.drain(mira_device)?;
    println!("  collected {} item(s), oldest first", waiting.len());
    let received = mira.follow(&decode(&waiting[0].body)?, &ada, NAME).await?;
    println!(
        "  verified revision {} · {} entries · role {:?}",
        received.collection.number(),
        received.descriptors.len(),
        received.collection.role
    );
    println!("  The same verification as step 7. An object is valid on its own");
    println!("  terms; where it waited changes nothing about it.");
    assert_eq!(received.descriptors, descriptors);

    section("Collecting is what empties it");
    let (count, _) = service.mailbox_size(mira_device)?;
    println!("  after draining: {count} item(s)");
    println!("  Reading and removing are one operation, so a client that dies");
    println!("  between them does not leave a mailbox that fills forever.");
    assert_eq!(count, 0);

    section("A mailbox that is full says so");
    // Small limits, so the boundary is reachable without writing 64 MiB.
    let tight = Embedded::with_limits(
        directory.join("tight.redb"),
        Limits {
            items: 2,
            bytes: 1_024,
        },
    )?;
    for index in 0..2 {
        tight.deliver(mira_device, &[index; 16])?;
    }
    match tight.deliver(mira_device, b"one too many") {
        Err(error) => println!("  a third item → {error}"),
        Ok(_) => panic!("a full mailbox must refuse"),
    }
    println!("  Refused rather than dropped. A member who never receives a");
    println!("  revision needs to be a sender who was told, not a silence.");

    section("The service stayed optional");
    println!("  Step 8's demo still runs with no service in existence. This is");
    println!("  the case it cannot cover — a device that is not there — and it");
    println!("  is the only thing the service is for.");

    let _ = std::fs::remove_dir_all(&directory);
    println!("\nAda reached a device that was asleep, through a service that");
    println!("could not read what it was carrying.");
    Ok(())
}
