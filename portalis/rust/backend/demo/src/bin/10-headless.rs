//! Step 10 — the five calls, driven headless.
//!
//! `SPEC.md` §16 is the whole surface between Rust and the interface: open,
//! watch, watch detail, command, close. The narrowness is the design — five
//! calls cannot grow into a second architecture, and an interface that can
//! only subscribe and send cannot start keeping its own copy of the truth.
//!
//! This is the Rust half of step 10's demo. It drives a real core against a
//! real store and prints what a subscriber would receive, which is exactly
//! what `10-headless.dart` will print once the bridge is regenerated — the
//! same stream, one language later.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 10-headless`.

use std::time::{Duration, Instant};

use backend::core::nexus::{Config, Nexus};
use backend::projection::build::{CollectionFacts, Handles, collection, snapshot};
use backend::projection::state::{
    Command, Connectivity, Detail, DeviceState, Handle, PortalisState, Progress,
};
use backend::store::records::Role;
use portalis_nexus_demo::section;

const COLLECTION: &[u8] = &[0x11; 16];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = std::env::temp_dir().join(format!("portalis-headless-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)?;

    section("open");
    let started = Instant::now();
    let nexus = Nexus::open(&Config {
        data_dir: directory.clone(),
        device_name: "Ada's laptop".to_owned(),
        fingerprint: "ada-fingerprint".to_owned(),
    })?;
    println!("  the store opened in {:?}", started.elapsed());
    println!("  Nothing else is awaited: the first frame needs the store and");
    println!("  nothing more, which is what the §21 budget is about.");

    section("watch — a complete snapshot, immediately");
    let mut states = nexus.watch();
    describe(&states.borrow_and_update());
    println!("  Complete rather than a delta, so a widget mounting late, a hot");
    println!("  reload and a restart all get everything.");

    section("command — answered with nothing connected");
    let started = Instant::now();
    let accepted = nexus.command(&Command::CreateCollection {
        name: "Iceland, 2019".to_owned(),
        files: Vec::new(),
    })?;
    println!(
        "  accepted as command {} in {:?} (queued: {})",
        accepted.id,
        started.elapsed(),
        accepted.queued
    );
    let refused = nexus
        .command(&Command::CreateCollection {
            name: "  ".to_owned(),
            files: Vec::new(),
        })
        .expect_err("a collection needs a name");
    println!("  a nameless collection → {refused}");
    println!("  Validation is local, so neither answer waited for anything.");

    section("watch — the collection appears");
    let mut handles = Handles::new();
    nexus.publish(&state(&mut handles, None), None, Duration::ZERO);
    if states.has_changed()? {
        describe(&states.borrow_and_update());
    }

    section("watch — a transfer, coalesced");
    let mut woken = 0;
    for step in 1..=200_u64 {
        nexus.publish(
            &state(&mut handles, Some(progress(step * 512, 200 * 512))),
            None,
            Duration::from_millis(step * 10),
        );
        if states.has_changed()? {
            states.mark_unchanged();
            woken += 1;
        }
    }
    println!("  200 readings over 2s → the subscriber woke {woken} times");
    describe(&states.borrow_and_update());
    println!("  A person reads a few numbers a second; an older reading of a");
    println!("  moving number is not information.");

    section("watch_detail — the expensive tier, only when asked");
    let piece_map = detail(handles.of(COLLECTION));
    nexus.publish(
        &state(&mut handles, None),
        Some(&piece_map),
        Duration::from_secs(3),
    );
    let mut details = nexus.watch_detail(None);
    println!(
        "  nobody looking → {:?}",
        details.borrow_and_update().is_some()
    );

    details = nexus.watch_detail(Some(handles.of(COLLECTION)));
    nexus.publish(
        &state(&mut handles, None),
        Some(&piece_map),
        Duration::from_secs(4),
    );
    let held = details.borrow_and_update().clone();
    println!(
        "  a view opens  → {} pieces, {} sample rows",
        held.as_ref().map_or(0, |detail| detail.pieces.len() * 8),
        held.as_ref().map_or(0, |detail| detail.samples.len() / 16)
    );
    nexus.watch_detail(None);
    println!(
        "  the view closes → {:?}",
        details.borrow_and_update().is_some()
    );

    section("close");
    let started = Instant::now();
    nexus.close().await;
    println!("  every task stopped in {:?}", started.elapsed());
    println!("  `close` returns when the runtime is quiet, so an app that is");
    println!("  quitting is actually quitting.");

    let _ = std::fs::remove_dir_all(&directory);
    println!("\nFive calls, and the whole interface fits behind them.");
    Ok(())
}

fn describe(state: &PortalisState) {
    println!(
        "  {} · {:?} · {} collection(s) · {} contact(s) · {} alert(s)",
        state.device.name,
        state.connectivity,
        state.collections.len(),
        state.contacts.len(),
        state.alerts.len()
    );
    for collection in &state.collections {
        let moving = collection.transfer.map_or_else(
            || "—".to_owned(),
            |transfer| {
                format!(
                    "{:.0}%{}",
                    transfer.progress * 100.0,
                    transfer
                        .eta_secs
                        .map_or_else(String::new, |eta| format!(", {eta}s left"))
                )
            },
        );
        println!(
            "    {} · revision {} · {:?} · {moving}",
            collection.name, collection.revision, collection.status
        );
    }
}

fn state(handles: &mut Handles, progress: Option<Progress>) -> PortalisState {
    snapshot(
        DeviceState {
            name: "Ada's laptop".to_owned(),
            handle: Some("ada#7Q2XZ".to_owned()),
            fingerprint: "a4f2 9c1b 77de 3081".to_owned(),
            devices: 2,
        },
        Connectivity::LocalOnly,
        Vec::new(),
        vec![collection(
            handles,
            &CollectionFacts {
                collection_id: COLLECTION.to_vec(),
                name: "Iceland, 2019".to_owned(),
                role: Role::Owner,
                revision: 1,
                entries: 240,
                total_bytes: 102_400,
                members: Vec::new(),
                progress,
                failure: None,
                paused: false,
                on_disk_bytes: 0,
            },
        )],
    )
}

fn progress(done: u64, total: u64) -> Progress {
    Progress {
        done,
        total,
        down_bytes_per_second: 4_194_304,
        up_bytes_per_second: 1_048_576,
        peers: 4,
    }
}

fn detail(id: Handle) -> Detail {
    Detail {
        id,
        entries: Vec::new(),
        pieces: vec![0b1010_1010; 1_024],
        samples: vec![0; 60 * 16],
        peers: vec!["10.0.0.1:6881".to_owned(), "10.0.0.2:6881".to_owned()],
    }
}
