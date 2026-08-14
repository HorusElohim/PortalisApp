//! Step 6 — one authoritative place for this device's own truth.
//!
//! `SPEC.md` §13: one transactional file holding the identity, the device logs
//! verified so far, the collections and their revisions, and the transfer
//! history. The test of a store is not what it writes; it is what comes back
//! after the process is gone. So this binary writes, drops everything, opens
//! the file again as a cold start would, and checks.
//!
//! Two properties are worth watching for.
//!
//! **The current revision is the highest number, not a separate row.** Nothing
//! records "this is the current one", because a summary can disagree with the
//! thing it summarises. It is a range query whose last row is the answer, and
//! that only works because keys are big-endian — the reason revision 256 sorts
//! after revision 255 rather than between 25 and 26.
//!
//! **The transfer history lives here rather than in Flutter (D8).** It is
//! sampled from backend numbers; keeping it on the other side of the bridge
//! made it a second source of truth, re-encoded on every tick.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 06-persistence`.

use backend::store::records::{
    EntryStatus, Role, StoredCollection, StoredContact, StoredEntry, StoredSample,
};
use backend::store::{Store, StoreError};

const COLLECTION: [u8; 16] = [0x11; 16];
const OTHER: [u8; 16] = [0x22; 16];
const ROOT: [u8; 32] = [0x33; 32];
const MANIFEST: [u8; 32] = [0x44; 32];
const INFO_HASH: [u8; 20] = [0x55; 20];

fn main() {
    let directory = std::env::temp_dir().join(format!("portalis-demo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    let file = directory.join("portalis.redb");

    write_everything(&file);

    section("The process restarts");
    let store = Store::open(&file).expect("reopens the same file");
    println!("  same file, nothing in memory carried over");

    report(&store);

    section("A store from a newer version refuses to open");
    let newer = directory.join("newer.redb");
    write_future_store(&newer);
    match Store::open(&newer) {
        Err(error @ StoreError::FromTheFuture { .. }) => println!("  {error}"),
        other => panic!("a future store must be refused, got {other:?}"),
    }
    println!("  Declining to start is the honest answer. Reading a newer schema");
    println!("  with older assumptions means misreading someone's own data,");
    println!("  which is worse than not opening it.");

    let _ = std::fs::remove_dir_all(&directory);
    println!("\nEverything written came back, including the transfer history,");
    println!("and a store this build cannot understand was refused.");
}

/// What a cold start finds waiting for it.
fn report(store: &Store) {
    section("What came back");
    println!(
        "  identity root key: {}",
        store
            .identity("root")
            .expect("reads")
            .is_some_and(|value| value == ROOT)
    );
    println!(
        "  contact: {}",
        store.contact(&[0x66; 32]).expect("reads").map_or_else(
            || "missing".to_owned(),
            |contact| format!(
                "{} (fingerprint compared: {})",
                contact.handle, contact.fingerprint_verified
            )
        )
    );
    for (id, collection) in store.collections().expect("reads") {
        println!(
            "  collection {:02x}{:02x}…: {} ({})",
            id[0],
            id[1],
            collection.name,
            match collection.role {
                Role::Owner => "we publish it",
                Role::Member => "we were given it",
            }
        );
    }

    section("The current revision is the highest, not the last written");
    let (number, bytes) = store
        .current_revision(&COLLECTION)
        .expect("reads")
        .expect("a revision");
    println!("  current: {number} — {}", String::from_utf8_lossy(&bytes));
    let numbers: Vec<u64> = store
        .revisions(&COLLECTION)
        .expect("reads")
        .into_iter()
        .map(|(number, _)| number)
        .collect();
    println!("  all held, in order: {numbers:?}");
    println!("  Written 1, 2, 3, 256, 255. Read back in numeric order, and 256");
    println!("  is current — because keys are big-endian, so byte order and");
    println!("  number order agree. Nothing stores \"current\" separately, so");
    println!("  nothing can disagree with the chain.");
    assert_eq!(number, 256, "the highest revision is the current one");

    section("Manifest, entry, outbox");
    println!(
        "  manifest: {}",
        store.manifest(&MANIFEST).expect("reads").map_or_else(
            || "missing".to_owned(),
            |bytes| String::from_utf8_lossy(&bytes).into_owned()
        )
    );
    let entry = store.entry(&INFO_HASH).expect("reads").expect("an entry");
    println!(
        "  entry: {} bytes of descriptor, status {:?}",
        entry.descriptor.len(),
        entry.status
    );
    println!(
        "  outbox: {} command(s) still waiting for connectivity",
        store.queued_commands().expect("reads").len()
    );

    section("Transfer history, in Rust rather than Flutter (D8)");
    let history = store.samples(&COLLECTION).expect("reads");
    let first = history.first().expect("a first reading");
    let last = history.last().expect("a last reading");
    println!("  {} readings survived the restart", history.len());
    println!("  oldest kept: t={} done={}", first.0, first.1.done);
    println!("  newest:      t={} done={}", last.0, last.1.done);
    println!("  A ring, not a permanent record: it exists to draw the recent");
    println!("  past. Sampled from backend numbers, so keeping it across the");
    println!("  bridge made it a second source of truth re-encoded every tick.");
    assert_eq!(history.len(), 60);
}

/// Everything a device might accumulate in a session, then dropped.
fn write_everything(file: &std::path::Path) {
    section("A device does some work");
    let store = Store::open(file).expect("opens a fresh store");
    println!("  schema version {}", store.version().expect("reads"));

    store.put_identity("root", &ROOT).expect("writes");
    store
        .put_contact(&StoredContact {
            handle: "mira#4KQ2P".to_owned(),
            fingerprint_verified: true,
            root_key: [0x66; 32],
        })
        .expect("writes");
    store
        .put_collection(
            &COLLECTION,
            &StoredCollection {
                name: "Iceland, 2019".to_owned(),
                role: Role::Owner,
                content_key: [0x77; 32],
                media_path: "/Users/ada/Pictures/Iceland".to_owned(),
                sources: Vec::new(),
                paused: false,
                on_disk_bytes: 0,
                substrate_handle: None,
            },
        )
        .expect("writes");
    store
        .put_collection(
            &OTHER,
            &StoredCollection {
                name: "Shared with me".to_owned(),
                role: Role::Member,
                content_key: [0x88; 32],
                media_path: "/Users/ada/Pictures/Shared".to_owned(),
                sources: Vec::new(),
                paused: false,
                on_disk_bytes: 0,
                substrate_handle: None,
            },
        )
        .expect("writes");

    // Published out of order on purpose: 256 is written before 255, and
    // both after 3.
    for number in [1_u64, 2, 3, 256, 255] {
        store
            .put_revision(&COLLECTION, number, format!("revision {number}").as_bytes())
            .expect("writes");
    }
    store
        .put_manifest(&MANIFEST, b"the manifest")
        .expect("writes");
    store
        .put_entry(
            &INFO_HASH,
            &StoredEntry {
                status: EntryStatus::Available,
                descriptor: b"d8:announce0:4:infod4:name5:photoee".to_vec(),
            },
        )
        .expect("writes");

    // A transfer running for a while.
    for at in 1..=200_u64 {
        store
            .put_sample(
                &COLLECTION,
                at,
                &StoredSample {
                    done: at * 512,
                    total: 102_400,
                    down_bytes_per_second: 4_096,
                    up_bytes_per_second: 1_024,
                    peers: 3,
                },
            )
            .expect("writes");
    }
    let trimmed = store.trim_samples(&COLLECTION, 60).expect("trims");
    println!("  200 readings recorded, {trimmed} trimmed to keep the newest 60");

    store
        .queue_command(1, b"publish revision 257")
        .expect("writes");
    println!("  everything written; the store is about to be dropped");
}

/// Writes a store and then claims a schema no build speaks yet.
fn write_future_store(path: &std::path::Path) {
    use redb::{ReadableTable as _, TableDefinition};

    const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
    let database = redb::Database::create(path).expect("creates");
    let write = database.begin_write().expect("writes");
    {
        let mut meta = write.open_table(META).expect("meta");
        meta.insert("schema_version", 99_u64).expect("bumps");
        assert!(meta.get("schema_version").expect("reads").is_some());
    }
    write.commit().expect("commits");
}

fn section(title: &str) {
    println!("\n{title}\n{}", "─".repeat(title.chars().count()));
}
